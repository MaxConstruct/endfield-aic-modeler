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
//!
//! Dialogs are the UI's: it asks the player for a path and calls back in. The camera is passed in
//! (`Option<CanvasView>`), never read from a canvas.

use std::path::{Path, PathBuf};
use std::time::Duration;

use aic_core::{CanvasView, Graph};
use web_time::{Instant, SystemTime, UNIX_EPOCH};

use crate::document::Document;
use crate::session::Session;
use crate::store::{BrowserFiles, FileStore};
use crate::throttle::{Poll, Throttle};

/// How long autosave waits between writes.
pub const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(5);
/// How many recent files are kept.
const RECENT_LIMIT: usize = 8;
/// The save file extension.
pub const EXTENSION: &str = "aicm";
/// Where the web build keeps drafts, and the files the player saved or opened.
const BROWSER_DRAFTS: &str = "/drafts";
const BROWSER_FILES: &str = "/files";

/// Where documents are kept and how they are saved.
pub(crate) struct Files {
    store: FileStore,
    /// Where drafts are saved; `None` keeps new graphs in memory until Save As.
    drafts_dir: Option<PathBuf>,
    /// Files opened or saved lately, newest first.
    recent: Vec<PathBuf>,
    autosave: Throttle,
    /// The canvas view as last saved, so moving the view also gets saved.
    saved_view: Option<CanvasView>,
    /// When the document was last written, and whether the last try failed.
    last_saved: Option<Instant>,
    save_failed: bool,
}

impl Files {
    pub(crate) fn new() -> Self {
        Self {
            store: FileStore::default(),
            drafts_dir: None,
            recent: Vec::new(),
            autosave: Throttle::new(AUTOSAVE_INTERVAL),
            saved_view: None,
            last_saved: None,
            save_failed: false,
        }
    }

    fn is_in_drafts(&self, path: &Path) -> bool {
        self.drafts_dir
            .as_ref()
            .is_some_and(|dir| path.parent() == Some(dir.as_path()))
    }

    fn new_draft_path(&self) -> Option<PathBuf> {
        let dir = self.drafts_dir.as_ref()?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        Some(dir.join(format!("untitled-{stamp}.{EXTENSION}")))
    }

    fn remember_recent(&mut self, path: &Path) {
        self.forget_recent(path);
        self.recent.insert(0, path.to_owned());
        self.recent.truncate(RECENT_LIMIT);
    }

    fn forget_recent(&mut self, path: &Path) {
        self.recent.retain(|p| p != path);
    }
}

/// A document that was opened: the camera saved with it, if any, for the UI to show.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Opened {
    pub view: Option<CanvasView>,
}

impl Session {
    // Where files are kept

    pub fn recent_files(&self) -> &[PathBuf] {
        &self.files.recent
    }

    pub fn set_recent_files(&mut self, files: Vec<PathBuf>) {
        self.files.recent = files;
        self.files.recent.truncate(RECENT_LIMIT);
    }

    /// Where drafts are kept; `None` keeps drafts in memory only (tests do this).
    pub fn set_drafts_dir(&mut self, dir: Option<PathBuf>) {
        self.files.drafts_dir = dir;
    }

    /// Whether new graphs are kept in drafts.
    pub fn keeps_drafts(&self) -> bool {
        self.files.drafts_dir.is_some()
    }

    /// Keeps files in the browser's storage, starting with `files`, as the web build does.
    pub fn use_browser_files(&mut self, files: BrowserFiles) {
        self.files.store = FileStore::Browser(files);
        self.files.drafts_dir = Some(PathBuf::from(BROWSER_DRAFTS));
    }

    /// The files in the browser's storage; `None` when files are on disk.
    pub fn browser_files(&self) -> Option<&BrowserFiles> {
        match &self.files.store {
            FileStore::Browser(files) => Some(files),
            FileStore::Disk => None,
        }
    }

    pub fn is_browser(&self) -> bool {
        self.files.store.is_browser()
    }

    /// How `path` is named in menus and messages.
    pub fn file_label(&self, path: &Path) -> String {
        self.files.store.label(path)
    }

    /// Whether a file of that name is already in the browser's storage, so Save As replaces it.
    pub fn browser_file_exists(&self, name: &str) -> bool {
        self.is_browser()
            && self
                .files
                .store
                .exists(&Path::new(BROWSER_FILES).join(file_name(name)))
    }

    /// The open document as saved, for the browser to download: its name and its text.
    pub fn saved_copy(&self) -> Option<(String, Vec<u8>)> {
        let path = self.doc.path()?;
        let text = self.files.store.read(path).ok()?;
        Some((self.files.store.label(path), text.into_bytes()))
    }

    /// The document to reopen next time: the open file or draft, once it has been saved.
    pub fn last_document(&self) -> Option<&Path> {
        self.doc.path()
    }

    // Opening

    /// Starts an empty draft in the current region. A draft left empty is deleted.
    pub fn start_new(&mut self) {
        self.drop_empty_draft();
        let region = self.doc.graph().region().clone();
        self.new_graph(region);
    }

    /// Opens a save file in place of the current graph. A failure leaves the current graph open and
    /// says why in the status bar.
    pub fn open_file(&mut self, path: &Path) -> Option<Opened> {
        match self.read_file(path) {
            Ok(opened) => {
                self.status = None;
                Some(opened)
            }
            Err(message) => {
                self.status = Some(message);
                None
            }
        }
    }

    /// Reopens the document open when the app last closed. Whatever goes wrong, the app carries on
    /// with a new draft and the file is left as it is.
    pub fn reopen(&mut self, path: &Path) -> Option<Opened> {
        if !self.files.store.exists(path) {
            let file = self.files.store.label(path);
            self.status = Some(self.strings.format("file-last-missing", &[("file", &file)]));
            self.files.forget_recent(path);
            return None;
        }
        self.open_file(path)
    }

    /// Opens a file uploaded to the browser, keeping it in the browser's storage under its name. A
    /// file that doesn't open isn't kept.
    pub fn open_upload(&mut self, name: &str, bytes: Vec<u8>) -> Option<Opened> {
        let path = Path::new(BROWSER_FILES).join(file_name(name));
        let failed = |session: &mut Self, error: String| {
            session.status = Some(
                session
                    .strings
                    .format("file-open-failed", &[("file", name), ("error", &error)]),
            );
        };
        let text = match String::from_utf8(bytes) {
            Ok(text) => text,
            Err(error) => {
                failed(self, error.to_string());
                return None;
            }
        };
        let had = self.files.store.read(&path).ok();
        if let Err(error) = self.files.store.write(&path, &text) {
            failed(self, error.to_string());
            return None;
        }
        match self.read_file(&path) {
            Ok(opened) => {
                self.status = None;
                Some(opened)
            }
            Err(message) => {
                match had {
                    Some(old) => {
                        let _ = self.files.store.write(&path, &old);
                    }
                    None => self.files.store.remove(&path),
                }
                self.status = Some(message);
                None
            }
        }
    }

    fn read_file(&mut self, path: &Path) -> Result<Opened, String> {
        let name = self.files.store.label(path);
        let text = self
            .files
            .store
            .read(path)
            .map_err(|e| (name.clone(), e.to_string()));
        let loaded = text.and_then(|text| {
            Graph::from_json(&self.catalog, &text).map_err(|e| (name.clone(), e.to_string()))
        });
        let loaded = loaded.map_err(|(file, error)| {
            self.files.forget_recent(path);
            self.strings
                .format("file-open-failed", &[("file", &file), ("error", &error)])
        })?;
        self.drop_empty_draft();
        self.other_data_version =
            (loaded.data_version != self.catalog.version()).then_some(loaded.data_version);
        let mut doc = Document::from_graph(loaded.graph, Some(path.to_owned()));
        let draft = self.files.is_in_drafts(path);
        doc.set_draft(draft);
        self.replace_document(doc);
        self.files.saved_view = loaded.view;
        if !draft {
            self.files.remember_recent(path);
        }
        Ok(Opened { view: loaded.view })
    }

    // Saving

    /// Cmd+S: saves now. A draft, or a document never saved, has no file to save into yet: this
    /// returns the name Save As offers, for the UI to ask for a path (or a name in the browser).
    pub fn save(&mut self, now: Instant, view: Option<CanvasView>) -> Option<String> {
        if self.doc.is_draft() || self.doc.path().is_none() {
            Some(self.offered_name())
        } else {
            self.save_now(now, view);
            None
        }
    }

    /// The name Save As offers: the file's own, or Untitled for a draft.
    pub fn offered_name(&self) -> String {
        self.doc
            .path()
            .filter(|_| !self.doc.is_draft())
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| format!("{}.{EXTENSION}", self.strings.get("file-untitled")))
    }

    /// Saves under `name` in the browser's storage. The extension is added when missing. Returns
    /// whether it was written.
    pub fn save_as_named(&mut self, name: &str, now: Instant, view: Option<CanvasView>) -> bool {
        let path = Path::new(BROWSER_FILES).join(file_name(name));
        self.save_to(&path, now, view);
        self.doc.path() == Some(path.as_path()) && !self.files.save_failed
    }

    /// Writes the graph to `path`, which becomes the document's file from now on. Moving a draft
    /// to a file of the player's deletes the draft.
    pub fn save_to(&mut self, path: &Path, now: Instant, view: Option<CanvasView>) {
        let old_draft = self
            .doc
            .path()
            .filter(|_| self.doc.is_draft())
            .map(Path::to_owned);
        let draft = self.files.is_in_drafts(path);
        if self.write(path, draft, now, view) {
            if let Some(old) = old_draft.filter(|old| old != path) {
                self.files.store.remove(&old);
            }
            self.other_data_version = None;
            if !draft {
                self.files.remember_recent(path);
            }
        }
    }

    /// Saves now if anything changed: to the document's file, or to a new draft.
    pub fn save_now(&mut self, now: Instant, view: Option<CanvasView>) {
        if !self.needs_saving(view) {
            return;
        }
        let target = match self.doc.path() {
            Some(path) => Some((path.to_owned(), self.doc.is_draft())),
            None => self.files.new_draft_path().map(|path| (path, true)),
        };
        if let Some((path, draft)) = target {
            self.write(&path, draft, now, view);
        }
        self.files.autosave.ran(now);
    }

    /// The autosave schedule: called every frame. Waits while `busy` (the pointer is down or a
    /// field has focus), so a save never lands in the middle of a drag or of typing a number. A
    /// `Poll::Wait` is when to call again.
    pub fn autosave(&mut self, now: Instant, busy: bool, view: Option<CanvasView>) -> Poll {
        let waiting = self.needs_saving(view) && !busy;
        match self.files.autosave.poll(now, waiting) {
            Poll::Run => {
                self.save_now(now, view);
                Poll::Idle
            }
            other => other,
        }
    }

    /// Whether the graph or the view changed since the last save. An empty new graph doesn't need
    /// a draft.
    pub fn needs_saving(&self, view: Option<CanvasView>) -> bool {
        let view_moved = self.doc.path().is_some()
            && view.is_some_and(|view| Some(view) != self.files.saved_view);
        self.doc.is_modified() || view_moved
    }

    /// Writes the file. Returns whether it worked; if not, the status says why.
    fn write(&mut self, path: &Path, draft: bool, now: Instant, view: Option<CanvasView>) -> bool {
        let json = self
            .doc
            .graph()
            .to_json_with_view(self.catalog.version(), view);
        match self.files.store.write(path, &json) {
            Ok(()) => {
                self.doc.mark_saved(path.to_owned(), draft);
                self.files.saved_view = view;
                self.files.last_saved = Some(now);
                self.files.save_failed = false;
                true
            }
            Err(error) => {
                self.files.save_failed = true;
                let file = self.files.store.label(path);
                self.status = Some(self.strings.format(
                    "file-save-failed",
                    &[("file", &file), ("error", &error.to_string())],
                ));
                false
            }
        }
    }

    /// Whether the last write failed.
    pub fn save_failed(&self) -> bool {
        self.files.save_failed
    }

    /// When the document was last written.
    pub fn last_saved(&self) -> Option<Instant> {
        self.files.last_saved
    }

    /// Deletes the open draft's file when its graph is empty: nothing worth keeping.
    fn drop_empty_draft(&mut self) {
        if self.doc.is_draft()
            && self.doc.graph().nodes().is_empty()
            && let Some(path) = self.doc.path()
        {
            let path = path.to_owned();
            self.files.store.remove(&path);
        }
    }

    /// Saves before the window closes.
    pub fn save_on_close(&mut self, now: Instant, view: Option<CanvasView>) {
        self.save_now(now, view);
    }

    /// The window title: the file's name, or Untitled for a draft, marked until it is saved.
    pub fn title(&self) -> String {
        let file = self
            .doc
            .path()
            .filter(|_| !self.doc.is_draft())
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.strings.get("file-untitled"));
        let key = if self.doc.is_modified() {
            "window-title-modified"
        } else {
            "window-title"
        };
        self.strings.format(key, &[("file", &file)])
    }
}

/// `name` as a save file name: only its last part, with the extension added when missing.
pub fn file_name(name: &str) -> String {
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
    use aic_core::{NodeKind, Pos};

    use super::*;
    use crate::settings::Availability;
    use crate::testing::fixture_catalog;

    fn smelter() -> NodeKind {
        NodeKind::Recipe {
            recipe: "iron_ingot".into(),
            facility: "smelter".into(),
            limit: Some(1.0),
        }
    }

    /// A session keeping drafts in a folder of its own, removed afterwards.
    struct Rig {
        session: Session,
        dir: PathBuf,
    }

    impl Rig {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("aic-files-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            let mut session = Session::new(fixture_catalog(), Availability::default());
            session.set_drafts_dir(Some(dir.join("drafts")));
            Self { session, dir }
        }

        fn change(&mut self) {
            self.session.add_node(smelter(), Pos::default(), None);
        }

        fn drafts(&self) -> Vec<PathBuf> {
            std::fs::read_dir(self.dir.join("drafts"))
                .map(|entries| entries.flatten().map(|e| e.path()).collect())
                .unwrap_or_default()
        }
    }

    impl Drop for Rig {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn a_save_file_name_keeps_only_its_last_part_and_gets_the_extension() {
        assert_eq!(file_name("plan"), "plan.aicm");
        assert_eq!(file_name("plan.AICM"), "plan.AICM");
        assert_eq!(file_name("../drafts/plan"), "plan.aicm");
        assert_eq!(file_name(r"C:\plans\plan.aicm"), "plan.aicm");
    }

    #[test]
    fn the_first_change_to_a_new_graph_creates_a_draft() {
        let mut rig = Rig::new("first-change");
        let now = Instant::now();
        rig.session.save_now(now, None);
        assert!(rig.drafts().is_empty(), "an empty new graph needs no draft");

        rig.change();
        rig.session.save_now(now, None);
        let drafts = rig.drafts();
        assert_eq!(drafts.len(), 1);
        assert!(rig.session.document().is_draft());
        assert_eq!(rig.session.document().path(), Some(drafts[0].as_path()));
        assert!(!rig.session.document().is_modified());
    }

    #[test]
    fn save_as_from_a_draft_moves_the_work_and_deletes_the_draft() {
        let mut rig = Rig::new("save-as");
        let now = Instant::now();
        rig.change();
        rig.session.save_now(now, None);
        let target = rig.dir.join("plan.aicm");
        rig.session.save_to(&target, now, None);

        assert!(target.exists());
        assert!(rig.drafts().is_empty());
        assert!(!rig.session.document().is_draft());
        assert_eq!(rig.session.recent_files(), std::slice::from_ref(&target));
        assert_eq!(rig.session.last_document(), Some(target.as_path()));
    }

    #[test]
    fn a_draft_left_empty_is_deleted_when_it_is_left() {
        let mut rig = Rig::new("empty-draft");
        let now = Instant::now();
        let node = rig.session.add_node(smelter(), Pos::default(), None);
        rig.session.save_now(now, None);
        assert_eq!(rig.drafts().len(), 1);

        rig.session.remove_node(node);
        rig.session.start_new();
        assert!(rig.drafts().is_empty());
    }

    #[test]
    fn a_kept_draft_survives_starting_a_new_graph() {
        let mut rig = Rig::new("kept-draft");
        let now = Instant::now();
        rig.change();
        rig.session.save_now(now, None);
        rig.session.start_new();
        assert_eq!(rig.drafts().len(), 1);
    }

    #[test]
    fn reopening_a_missing_last_document_says_so_and_starts_over() {
        let mut rig = Rig::new("missing");
        let missing = rig.dir.join("gone.aicm");
        assert!(rig.session.reopen(&missing).is_none());
        assert!(
            rig.session
                .status()
                .is_some_and(|s| s.contains("gone.aicm"))
        );
        assert!(rig.session.document().graph().nodes().is_empty());
        assert!(rig.session.recent_files().is_empty());
    }

    #[test]
    fn a_file_that_does_not_open_leaves_the_graph_and_says_why() {
        let mut rig = Rig::new("unreadable");
        std::fs::create_dir_all(&rig.dir).unwrap();
        let bad = rig.dir.join("bad.aicm");
        std::fs::write(&bad, "not a save file").unwrap();
        rig.change();
        assert!(rig.session.open_file(&bad).is_none());
        assert!(rig.session.status().is_some());
        assert_eq!(rig.session.document().graph().nodes().len(), 1);
    }

    #[test]
    fn a_saved_file_opens_again_with_its_camera() {
        let mut rig = Rig::new("round-trip");
        let now = Instant::now();
        rig.change();
        let view = CanvasView {
            x: 10.0,
            y: 20.0,
            zoom: 1.5,
        };
        let target = rig.dir.join("plan.aicm");
        rig.session.save_to(&target, now, Some(view));
        // No `.tmp` is left beside it: the write went through a temporary file and replaced it.
        let names: Vec<_> = std::fs::read_dir(&rig.dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| name != "drafts")
            .collect();
        assert_eq!(names, ["plan.aicm"]);

        let mut other = Rig::new("round-trip-other");
        let opened = other.session.open_file(&target).expect("opens");
        assert_eq!(opened.view, Some(view));
        assert_eq!(other.session.document().graph().nodes().len(), 1);
        assert!(!other.session.document().is_modified());
    }

    #[test]
    fn autosave_waits_while_busy_and_saves_the_last_change() {
        let mut rig = Rig::new("autosave");
        let start = Instant::now();
        rig.change();
        // The pointer is down: nothing is written.
        assert_eq!(rig.session.autosave(start, true, None), Poll::Idle);
        assert!(rig.drafts().is_empty());
        // Let go: it saves at once, the first save after a quiet spell.
        assert_eq!(rig.session.autosave(start, false, None), Poll::Idle);
        assert_eq!(rig.drafts().len(), 1);

        // Another change inside the interval waits for its end, then runs once.
        rig.change();
        let soon = start + Duration::from_secs(1);
        assert_eq!(
            rig.session.autosave(soon, false, None),
            Poll::Wait(start + AUTOSAVE_INTERVAL)
        );
        assert!(rig.session.needs_saving(None));
        let later = start + AUTOSAVE_INTERVAL;
        assert_eq!(rig.session.autosave(later, false, None), Poll::Idle);
        assert!(!rig.session.needs_saving(None));
    }

    #[test]
    fn a_moved_camera_needs_saving_once_the_document_has_a_file() {
        let mut rig = Rig::new("camera");
        let now = Instant::now();
        rig.change();
        let target = rig.dir.join("plan.aicm");
        let view = CanvasView {
            x: 0.0,
            y: 0.0,
            zoom: 1.0,
        };
        rig.session.save_to(&target, now, Some(view));
        assert!(!rig.session.needs_saving(Some(view)));
        let moved = CanvasView { x: 5.0, ..view };
        assert!(rig.session.needs_saving(Some(moved)));
    }

    #[test]
    fn the_title_marks_a_modified_file() {
        let mut rig = Rig::new("title");
        let now = Instant::now();
        assert_eq!(rig.session.title(), "Untitled — AIC Modeler");
        rig.change();
        let target = rig.dir.join("plan.aicm");
        rig.session.save_to(&target, now, None);
        assert_eq!(rig.session.title(), "plan.aicm — AIC Modeler");
        rig.change();
        assert_eq!(rig.session.title(), "plan.aicm • — AIC Modeler");
    }

    #[test]
    fn save_asks_for_a_name_only_when_there_is_no_file_yet() {
        let mut rig = Rig::new("save");
        let now = Instant::now();
        rig.change();
        assert_eq!(rig.session.save(now, None), Some("Untitled.aicm".into()));
        let target = rig.dir.join("plan.aicm");
        rig.session.save_to(&target, now, None);
        rig.change();
        assert_eq!(rig.session.save(now, None), None);
        assert!(!rig.session.document().is_modified());
        assert_eq!(rig.session.offered_name(), "plan.aicm");
    }
}
