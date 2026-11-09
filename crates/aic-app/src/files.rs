//! Save files and autosave.
//!
//! A document is always saved somewhere, a few seconds after each change at most:
//!
//! - A new graph is a *draft*. Its first change creates `untitled-<time>.aicm` in the app's own
//!   drafts folder, and it autosaves there.
//! - An opened file, or one the player picked with Save As, autosaves into that file. Save As from
//!   a draft moves the work to the new file and deletes the draft.
//! - The app reopens the last document, draft or file, when it starts. A file that is missing or
//!   can't be read is left alone; the app starts a new draft and says why in the status bar.
//!
//! Writes go to a temporary file that then replaces the real one, so a crash mid-write can't
//! leave half a file.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use aic_core::Graph;
use eframe::egui;

use crate::document::Document;
use crate::i18n::Strings;
use crate::throttle::Poll;
use crate::workspace::Workspace;

/// The storage key for the recent files list.
pub const RECENT_KEY: &str = "recent-files";
/// The storage key for the document open when the app last closed.
pub const LAST_KEY: &str = "last-document";
/// How long autosave waits between writes.
pub const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(5);
/// How many recent files are kept.
const RECENT_LIMIT: usize = 8;
/// The save file extension.
const EXTENSION: &str = "aicm";

/// Something from the File menu that replaces the open graph. The open one is saved first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pending {
    New,
    Open,
    OpenPath(PathBuf),
}

impl Workspace {
    pub fn recent_files(&self) -> &[PathBuf] {
        &self.recent_files
    }

    pub fn set_recent_files(&mut self, files: Vec<PathBuf>) {
        self.recent_files = files;
        self.recent_files.truncate(RECENT_LIMIT);
    }

    /// Where drafts are kept; `None` keeps drafts in memory only (tests do this).
    pub fn set_drafts_dir(&mut self, dir: Option<PathBuf>) {
        self.drafts_dir = dir;
    }

    /// The document to reopen next time: the open file or draft, once it has been saved.
    pub fn last_document(&self) -> Option<&Path> {
        self.doc.path()
    }

    /// Saves the open graph, then runs `action`.
    pub fn request(&mut self, action: Pending, strings: &Strings) {
        self.save_now(strings);
        match action {
            Pending::New => self.start_new(),
            Pending::Open => {
                let picked = rfd::FileDialog::new()
                    .add_filter("AIC Modeler", &[EXTENSION])
                    .pick_file();
                if let Some(path) = picked {
                    self.open_file(&path, strings);
                }
            }
            Pending::OpenPath(path) => self.open_file(&path, strings),
        }
    }

    /// Starts an empty draft in the current region. A draft left empty is deleted.
    fn start_new(&mut self) {
        self.drop_empty_draft();
        let region = self.doc.graph().region().clone();
        self.new_graph(region);
    }

    /// Opens a save file in place of the current graph. A failure leaves the current graph open and
    /// says why in the status bar.
    pub fn open_file(&mut self, path: &Path, strings: &Strings) {
        match self.read_file(path, strings) {
            Ok(()) => self.status = None,
            Err(message) => self.status = Some(message),
        }
    }

    /// Reopens the document open when the app last closed. Whatever goes wrong, the app carries on
    /// with a new draft and the file is left as it is.
    pub fn reopen(&mut self, path: &Path, strings: &Strings) {
        if !path.exists() {
            let file = path.display().to_string();
            self.status = Some(strings.format("file-last-missing", &[("file", &file)]));
            self.forget_recent(path);
            return;
        }
        self.open_file(path, strings);
    }

    fn read_file(&mut self, path: &Path, strings: &Strings) -> Result<(), String> {
        let name = path.display().to_string();
        let text = std::fs::read_to_string(path).map_err(|e| (name.clone(), e.to_string()));
        let loaded = text.and_then(|text| {
            Graph::from_json(&self.catalog, &text).map_err(|e| (name.clone(), e.to_string()))
        });
        let loaded = loaded.map_err(|(file, error)| {
            self.forget_recent(path);
            strings.format("file-open-failed", &[("file", &file), ("error", &error)])
        })?;
        self.drop_empty_draft();
        self.other_data_version =
            (loaded.data_version != self.catalog.version()).then_some(loaded.data_version);
        let mut doc = Document::from_graph(&self.catalog, loaded.graph, Some(path.to_owned()));
        let draft = self.is_in_drafts(path);
        doc.set_draft(draft);
        self.set_document(doc);
        self.saved_view = loaded.view;
        if let Some(view) = loaded.view {
            self.canvas.show_view(view);
        }
        if !draft {
            self.remember_recent(path);
        }
        Ok(())
    }

    /// Cmd+S: saves now. A draft asks where to keep it instead, as Save As does.
    pub fn save(&mut self, strings: &Strings) {
        if self.doc.is_draft() || self.doc.path().is_none() {
            self.save_as(strings);
        } else {
            self.save_now(strings);
        }
    }

    pub fn save_as(&mut self, strings: &Strings) {
        let name = self
            .doc
            .path()
            .filter(|_| !self.doc.is_draft())
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| format!("{}.{EXTENSION}", strings.get("file-untitled")));
        let picked = rfd::FileDialog::new()
            .add_filter("AIC Modeler", &[EXTENSION])
            .set_file_name(name)
            .save_file();
        if let Some(mut path) = picked {
            if path.extension().is_none() {
                path.set_extension(EXTENSION);
            }
            self.save_to(&path, strings);
        }
    }

    /// Writes the graph to `path`, which becomes the document's file from now on. Moving a draft
    /// to a file of the player's deletes the draft.
    pub fn save_to(&mut self, path: &Path, strings: &Strings) {
        let old_draft = self
            .doc
            .path()
            .filter(|_| self.doc.is_draft())
            .map(Path::to_owned);
        let draft = self.is_in_drafts(path);
        if self.write(path, draft, strings) {
            if let Some(old) = old_draft.filter(|old| old != path) {
                let _ = std::fs::remove_file(old);
            }
            self.other_data_version = None;
            if !draft {
                self.remember_recent(path);
            }
        }
    }

    /// Saves now if anything changed: to the document's file, or to a new draft.
    pub fn save_now(&mut self, strings: &Strings) {
        if !self.needs_saving() {
            return;
        }
        let target = match self.doc.path() {
            Some(path) => Some((path.to_owned(), self.doc.is_draft())),
            None => self.new_draft_path().map(|path| (path, true)),
        };
        if let Some((path, draft)) = target {
            self.write(&path, draft, strings);
        }
        self.autosave.ran(Instant::now());
    }

    /// The autosave schedule: called every frame. Waits while the pointer is down or a field has
    /// focus, so a save never lands in the middle of a drag or of typing a number.
    pub(crate) fn autosave(&mut self, ctx: &egui::Context, strings: &Strings) {
        let busy = ctx.input(|i| i.pointer.any_down()) || ctx.memory(|m| m.focused().is_some());
        let waiting = self.needs_saving() && !busy;
        match self.autosave.poll(Instant::now(), waiting) {
            Poll::Idle => {}
            Poll::Run => self.save_now(strings),
            Poll::Wait(when) => {
                ctx.request_repaint_after(when.saturating_duration_since(Instant::now()));
            }
        }
    }

    /// Whether the graph or the view changed since the last save. An empty new graph doesn't need
    /// a draft.
    fn needs_saving(&self) -> bool {
        let view_moved = self.doc.path().is_some()
            && self
                .canvas
                .view()
                .is_some_and(|view| Some(view) != self.saved_view);
        self.doc.is_modified() || view_moved
    }

    /// Writes the file. Returns whether it worked; if not, the status bar says why.
    fn write(&mut self, path: &Path, draft: bool, strings: &Strings) -> bool {
        let view = self.canvas.view();
        let json = self
            .doc
            .graph()
            .to_json_with_view(self.catalog.version(), view);
        match write_atomically(path, json.as_bytes()) {
            Ok(()) => {
                self.doc.mark_saved(path.to_owned(), draft);
                self.saved_view = view;
                self.last_saved = Some(Instant::now());
                self.save_failed = false;
                true
            }
            Err(error) => {
                self.save_failed = true;
                let file = path.display().to_string();
                self.status = Some(strings.format(
                    "file-save-failed",
                    &[("file", &file), ("error", &error.to_string())],
                ));
                false
            }
        }
    }

    fn new_draft_path(&self) -> Option<PathBuf> {
        let dir = self.drafts_dir.as_ref()?;
        if std::fs::create_dir_all(dir).is_err() {
            return None;
        }
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        Some(dir.join(format!("untitled-{stamp}.{EXTENSION}")))
    }

    fn is_in_drafts(&self, path: &Path) -> bool {
        self.drafts_dir
            .as_ref()
            .is_some_and(|dir| path.parent() == Some(dir.as_path()))
    }

    /// Deletes the open draft's file when its graph is empty: nothing worth keeping.
    fn drop_empty_draft(&mut self) {
        if self.doc.is_draft()
            && self.doc.graph().nodes().is_empty()
            && let Some(path) = self.doc.path()
        {
            let _ = std::fs::remove_file(path);
        }
    }

    fn remember_recent(&mut self, path: &Path) {
        self.forget_recent(path);
        self.recent_files.insert(0, path.to_owned());
        self.recent_files.truncate(RECENT_LIMIT);
    }

    fn forget_recent(&mut self, path: &Path) {
        self.recent_files.retain(|p| p != path);
    }

    /// Saves before the window closes.
    pub(crate) fn handle_close_request(&mut self, ctx: &egui::Context, strings: &Strings) {
        if ctx.input(|i| i.viewport().close_requested()) {
            self.save_now(strings);
        }
    }

    /// The window title: the file's name, or Untitled for a draft, marked until it is saved.
    pub(crate) fn update_title(&mut self, ctx: &egui::Context, strings: &Strings) {
        let file = self
            .doc
            .path()
            .filter(|_| !self.doc.is_draft())
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| strings.get("file-untitled"));
        let key = if self.doc.is_modified() {
            "window-title-modified"
        } else {
            "window-title"
        };
        let title = strings.format(key, &[("file", &file)]);
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
    }

    /// The window title as last set.
    pub fn title(&self) -> &str {
        &self.last_title
    }
}

/// Writes `bytes` to a temporary file next to `path`, then puts it in `path`'s place.
fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(".tmp");
    let temporary = PathBuf::from(temporary);
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(&temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_atomic_write_replaces_the_file_and_leaves_no_temporary() {
        let dir = std::env::temp_dir().join(format!("aic-atomic-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("plan.aicm");
        std::fs::write(&path, "old").unwrap();
        write_atomically(&path, b"new").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }
}
