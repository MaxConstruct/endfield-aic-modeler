//! The egui side of files: the dialogs, the browser's upload and download, the Save As prompt and
//! the window title. What a save, a draft, a recent file or an autosave is lies in
//! [`aic_editor::Session`] (`aic_editor::files`); this asks the player for paths and calls in.

use std::path::{Path, PathBuf};
use std::time::Duration;

use eframe::egui;
use web_time::Instant;

use crate::editor::Editor;
use crate::i18n::Strings;
use crate::store::BrowserFiles;
use aic_editor::Opened;
use aic_editor::throttle::Poll;

/// The storage key for the recent files list.
pub const RECENT_KEY: &str = "recent-files";
/// The storage key for the document open when the app last closed.
pub const LAST_KEY: &str = "last-document";
pub use aic_editor::files::AUTOSAVE_INTERVAL;
/// The save file extension.
const EXTENSION: &str = aic_editor::files::EXTENSION;
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
        self.session.recent_files()
    }

    pub fn set_recent_files(&mut self, files: Vec<PathBuf>) {
        self.session.set_recent_files(files);
    }

    /// Where drafts are kept; `None` keeps drafts in memory only (tests do this).
    pub fn set_drafts_dir(&mut self, dir: Option<PathBuf>) {
        self.session.set_drafts_dir(dir);
    }

    /// Keeps files in the browser's storage, starting with `files`, as the web build does.
    pub fn use_browser_files(&mut self, files: BrowserFiles) {
        self.session.use_browser_files(files);
    }

    /// The files in the browser's storage; `None` when files are on disk.
    pub fn browser_files(&self) -> Option<&BrowserFiles> {
        self.session.browser_files()
    }

    /// How `path` is named in menus and messages.
    pub fn file_label(&self, path: &Path) -> String {
        self.session.file_label(path)
    }

    /// The files waiting for the browser to download them, oldest first.
    pub fn take_downloads(&mut self) -> Vec<Download> {
        std::mem::take(&mut self.downloads)
    }

    /// The document to reopen next time: the open file or draft, once it has been saved.
    pub fn last_document(&self) -> Option<&Path> {
        self.session.last_document()
    }

    /// What a freshly opened document needs from the window: every view shows its top, the camera
    /// saved with it, and no menu left over from the old one.
    fn show_opened(&mut self, opened: Opened) {
        self.after_new_document();
        if let Some(view) = opened.view {
            self.canvas.show_view(view);
        }
    }

    /// Saves the open graph, then runs `action`.
    pub fn request(&mut self, action: Pending, strings: &Strings) {
        self.save_now(strings);
        match action {
            Pending::New => {
                self.session.start_new();
                self.after_new_document();
            }
            Pending::Open if self.session.is_browser() => self.pick_upload(),
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
        if let Some(opened) = self.session.open_upload(name, bytes) {
            self.show_opened(opened);
        }
    }

    /// Opens a save file in place of the current graph. A failure leaves the current graph open and
    /// says why in the status bar.
    pub fn open_file(&mut self, path: &Path, _strings: &Strings) {
        if let Some(opened) = self.session.open_file(path) {
            self.show_opened(opened);
        }
    }

    /// Reopens the document open when the app last closed. Whatever goes wrong, the app carries on
    /// with a new draft and the file is left as it is.
    pub fn reopen(&mut self, path: &Path, _strings: &Strings) {
        if let Some(opened) = self.session.reopen(path) {
            self.show_opened(opened);
        }
    }

    /// Cmd+S: saves now. A draft asks where to keep it instead, as Save As does. In the browser,
    /// also downloads the file.
    pub fn save(&mut self, strings: &Strings) {
        match self.session.save(Instant::now(), self.canvas.top_view()) {
            Some(name) => self.ask_where_to_save(name, strings),
            None => {
                if self.session.is_browser() {
                    self.download_document();
                }
            }
        }
    }

    /// Asks where to save: a file dialog on disk, a name in the browser.
    pub fn save_as(&mut self, strings: &Strings) {
        let name = self.session.offered_name();
        self.ask_where_to_save(name, strings);
    }

    fn ask_where_to_save(&mut self, name: String, strings: &Strings) {
        if self.session.is_browser() {
            let stem = name.strip_suffix(&format!(".{EXTENSION}")).unwrap_or(&name);
            self.save_as_prompt = Some(stem.to_owned());
            return;
        }
        self.pick_save_path(name, strings);
    }

    /// Saves under `name` in the browser's storage, then downloads the file. The extension is
    /// added when missing.
    pub fn save_as_named(&mut self, name: &str, _strings: &Strings) {
        if self
            .session
            .save_as_named(name, Instant::now(), self.canvas.top_view())
        {
            self.download_document();
        }
    }

    /// Whether a file of that name is already in the browser's storage, so Save As replaces it.
    pub fn browser_file_exists(&self, name: &str) -> bool {
        self.session.browser_file_exists(name)
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
        if let Some((name, bytes)) = self.session.saved_copy() {
            self.downloads.push(Download { name, bytes });
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
    pub fn save_to(&mut self, path: &Path, _strings: &Strings) {
        self.session
            .save_to(path, Instant::now(), self.canvas.top_view());
    }

    /// Saves now if anything changed: to the document's file, or to a new draft.
    pub fn save_now(&mut self, _strings: &Strings) {
        self.session
            .save_now(Instant::now(), self.canvas.top_view());
    }

    /// The autosave schedule: called every frame. Waits while the pointer is down or a field has
    /// focus, so a save never lands in the middle of a drag or of typing a number.
    pub(crate) fn autosave(&mut self, ctx: &egui::Context, _strings: &Strings) {
        let busy = ctx.input(|i| i.pointer.any_down()) || ctx.memory(|m| m.focused().is_some());
        let now = Instant::now();
        if let Poll::Wait(when) = self.session.autosave(now, busy, self.canvas.top_view()) {
            ctx.request_repaint_after(when.saturating_duration_since(now));
        }
    }

    /// Saves before the window closes.
    pub(crate) fn handle_close_request(&mut self, ctx: &egui::Context, strings: &Strings) {
        if ctx.input(|i| i.viewport().close_requested()) {
            self.save_now(strings);
        }
    }

    /// The window title: the file's name, or Untitled for a draft, marked until it is saved.
    pub(crate) fn update_title(&mut self, ctx: &egui::Context, _strings: &Strings) {
        let title = self.session.title();
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
