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
//! Files live in a [`FileStore`]: on disk, or in the web build, in the browser's storage. There,
//! Save and Save As also download a copy, and Open uploads a file into the browser's storage.

use std::path::{Path, PathBuf};
use std::time::Duration;

use aic_core::Graph;
use eframe::egui;
use web_time::{Instant, SystemTime, UNIX_EPOCH};

use crate::document::Document;
use crate::editor::Editor;
use crate::i18n::Strings;
use crate::store::{BrowserFiles, FileStore};
use crate::throttle::Poll;

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
/// Where the web build keeps drafts, and the files the player saved or opened.
const BROWSER_DRAFTS: &str = "/drafts";
const BROWSER_FILES: &str = "/files";
/// How often the editor looks for a file the browser is reading.
const UPLOAD_POLL: Duration = Duration::from_millis(100);

/// A file for the browser to download: the web build's Save, Save As and Export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Download {
    /// The file name, extension included.
    pub name: String,
    pub bytes: Vec<u8>,
}

/// A file the player picked to open in the browser.
#[derive(Debug)]
pub struct Upload {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// Something from the File menu that replaces the open graph. The open one is saved first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pending {
    New,
    Open,
    OpenPath(PathBuf),
}

impl Editor {
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

    /// Keeps files in the browser's storage, starting with `files`, as the web build does.
    pub fn use_browser_files(&mut self, files: BrowserFiles) {
        self.store = FileStore::Browser(files);
        self.drafts_dir = Some(PathBuf::from(BROWSER_DRAFTS));
    }

    /// The files in the browser's storage; `None` when files are on disk.
    pub fn browser_files(&self) -> Option<&BrowserFiles> {
        match &self.store {
            FileStore::Browser(files) => Some(files),
            FileStore::Disk => None,
        }
    }

    /// How `path` is named in menus and messages.
    pub fn file_label(&self, path: &Path) -> String {
        self.store.label(path)
    }

    /// The files waiting for the browser to download them, oldest first.
    pub fn take_downloads(&mut self) -> Vec<Download> {
        std::mem::take(&mut self.downloads)
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
            Pending::Open if self.store.is_browser() => self.pick_upload(),
            Pending::Open => self.pick_file(strings),
            Pending::OpenPath(path) => self.open_file(&path, strings),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn pick_file(&mut self, strings: &Strings) {
        let picked = rfd::FileDialog::new()
            .add_filter("AIC Modeler", &[EXTENSION])
            .pick_file();
        if let Some(path) = picked {
            self.open_file(&path, strings);
        }
    }

    /// The web build has no disk.
    #[cfg(target_arch = "wasm32")]
    fn pick_file(&mut self, _strings: &Strings) {}

    /// Asks the browser for a file to open; [`Self::poll_upload`] opens it once picked.
    fn pick_upload(&mut self) {
        #[cfg(target_arch = "wasm32")]
        {
            self.upload = Some(crate::web::pick_file(EXTENSION));
        }
    }

    /// Opens the file the player picked in the browser, once it has been read.
    pub(crate) fn poll_upload(&mut self, ctx: &egui::Context, strings: &Strings) {
        let Some(receiver) = &self.upload else {
            return;
        };
        match receiver.try_recv() {
            Ok(upload) => {
                self.upload = None;
                self.open_upload(&upload.name, upload.bytes, strings);
            }
            // The browser reads the file in the background; look again soon.
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(UPLOAD_POLL);
            }
            // Cancelled.
            Err(std::sync::mpsc::TryRecvError::Disconnected) => self.upload = None,
        }
    }

    /// Opens a file uploaded to the browser, keeping it in the browser's storage under its name. A
    /// file that doesn't open isn't kept.
    pub fn open_upload(&mut self, name: &str, bytes: Vec<u8>, strings: &Strings) {
        self.save_now(strings);
        let path = Path::new(BROWSER_FILES).join(file_name(name));
        let text = match String::from_utf8(bytes) {
            Ok(text) => text,
            Err(error) => {
                let error = error.to_string();
                self.status =
                    Some(strings.format("file-open-failed", &[("file", name), ("error", &error)]));
                return;
            }
        };
        let had = self.store.read(&path).ok();
        if let Err(error) = self.store.write(&path, &text) {
            let error = error.to_string();
            self.status =
                Some(strings.format("file-open-failed", &[("file", name), ("error", &error)]));
            return;
        }
        let opened = self.read_file(&path, strings);
        if let Err(message) = opened {
            match had {
                Some(old) => {
                    let _ = self.store.write(&path, &old);
                }
                None => self.store.remove(&path),
            }
            self.status = Some(message);
        } else {
            self.status = None;
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
        if !self.store.exists(path) {
            let file = self.store.label(path);
            self.status = Some(strings.format("file-last-missing", &[("file", &file)]));
            self.forget_recent(path);
            return;
        }
        self.open_file(path, strings);
    }

    fn read_file(&mut self, path: &Path, strings: &Strings) -> Result<(), String> {
        let name = self.store.label(path);
        let text = self
            .store
            .read(path)
            .map_err(|e| (name.clone(), e.to_string()));
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

    /// Cmd+S: saves now. A draft asks where to keep it instead, as Save As does. In the browser,
    /// also downloads the file.
    pub fn save(&mut self, strings: &Strings) {
        if self.doc.is_draft() || self.doc.path().is_none() {
            self.save_as(strings);
        } else {
            self.save_now(strings);
            if self.store.is_browser() {
                self.download_document();
            }
        }
    }

    /// The name Save As offers: the file's own, or Untitled for a draft.
    fn offered_name(&self, strings: &Strings) -> String {
        self.doc
            .path()
            .filter(|_| !self.doc.is_draft())
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| format!("{}.{EXTENSION}", strings.get("file-untitled")))
    }

    /// Asks where to save: a file dialog on disk, a name in the browser.
    pub fn save_as(&mut self, strings: &Strings) {
        let name = self.offered_name(strings);
        if self.store.is_browser() {
            let stem = name.strip_suffix(&format!(".{EXTENSION}")).unwrap_or(&name);
            self.save_as_prompt = Some(stem.to_owned());
            return;
        }
        self.pick_save_path(name, strings);
    }

    /// Saves under `name` in the browser's storage, then downloads the file. The extension is
    /// added when missing.
    pub fn save_as_named(&mut self, name: &str, strings: &Strings) {
        let path = Path::new(BROWSER_FILES).join(file_name(name));
        self.save_to(&path, strings);
        if self.doc.path() == Some(path.as_path()) && !self.save_failed {
            self.download_document();
        }
    }

    /// Whether a file of that name is already in the browser's storage, so Save As replaces it.
    pub fn browser_file_exists(&self, name: &str) -> bool {
        self.store.is_browser()
            && self
                .store
                .exists(&Path::new(BROWSER_FILES).join(file_name(name)))
    }

    /// The browser's Save As: a name for the file, kept in the browser and downloaded.
    pub(crate) fn save_as_prompt_ui(&mut self, ctx: &egui::Context, strings: &Strings) {
        let Some(mut name) = self.save_as_prompt.take() else {
            return;
        };
        let mut keep = true;
        let mut confirmed = false;
        egui::Window::new(strings.get("file-save-as-title"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(strings.get("file-save-as-hint"));
                ui.horizontal(|ui| {
                    ui.label(strings.get("file-save-as-name"));
                    let field = ui.text_edit_singleline(&mut name);
                    if !field.has_focus() && !field.lost_focus() {
                        field.request_focus();
                    }
                    confirmed |=
                        field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                });
                if self.browser_file_exists(&name) {
                    ui.label(strings.get("file-save-as-replaces"));
                }
                ui.horizontal(|ui| {
                    let valid = !name.trim().is_empty();
                    confirmed |= ui
                        .add_enabled(
                            valid,
                            egui::Button::new(strings.get("file-save-as-confirm")),
                        )
                        .clicked();
                    confirmed &= valid;
                    keep &= !ui.button(strings.get("file-save-as-cancel")).clicked();
                });
                keep &= !ui.input(|i| i.key_pressed(egui::Key::Escape));
            });
        if confirmed {
            self.save_as_named(&name, strings);
        } else if keep {
            self.save_as_prompt = Some(name);
        }
    }

    /// Queues the open document, as saved, for the browser to download.
    fn download_document(&mut self) {
        let Some(path) = self.doc.path() else {
            return;
        };
        if let Ok(text) = self.store.read(path) {
            self.downloads.push(Download {
                name: self.store.label(path),
                bytes: text.into_bytes(),
            });
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn pick_save_path(&mut self, _name: String, _strings: &Strings) {}

    #[cfg(not(target_arch = "wasm32"))]
    fn pick_save_path(&mut self, name: String, strings: &Strings) {
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
                self.store.remove(&old);
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
                .top_view()
                .is_some_and(|view| Some(view) != self.saved_view);
        self.doc.is_modified() || view_moved
    }

    /// Writes the file. Returns whether it worked; if not, the status bar says why.
    fn write(&mut self, path: &Path, draft: bool, strings: &Strings) -> bool {
        let view = self.canvas.top_view();
        let json = self
            .doc
            .graph()
            .to_json_with_view(self.catalog.version(), view);
        match self.store.write(path, &json) {
            Ok(()) => {
                self.doc.mark_saved(path.to_owned(), draft);
                self.saved_view = view;
                self.last_saved = Some(Instant::now());
                self.save_failed = false;
                true
            }
            Err(error) => {
                self.save_failed = true;
                let file = self.store.label(path);
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
            let path = path.to_owned();
            self.store.remove(&path);
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

/// `name` as a save file name: only its last part, with the extension added when missing.
fn file_name(name: &str) -> String {
    let name = name.rsplit(['/', '\\']).next().unwrap_or(name).trim();
    if Path::new(name)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case(EXTENSION))
    {
        name.to_owned()
    } else {
        format!("{name}.{EXTENSION}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_save_file_name_keeps_only_its_last_part_and_gets_the_extension() {
        assert_eq!(file_name("plan"), "plan.aicm");
        assert_eq!(file_name("plan.AICM"), "plan.AICM");
        assert_eq!(file_name("../drafts/plan"), "plan.aicm");
        assert_eq!(file_name(r"C:\plans\plan.aicm"), "plan.aicm");
    }
}
