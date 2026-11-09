//! The File menu: new graphs, opening and saving `.aicm` files, recent files, and the dialogs that
//! ask before unsaved changes are dropped.

use std::path::{Path, PathBuf};

use aic_core::Graph;
use aic_data::RegionId;
use eframe::egui::{self, RichText};

use crate::document::Document;
use crate::i18n::Strings;
use crate::names;
use crate::workspace::Workspace;

/// The storage key for the recent files list.
pub const RECENT_KEY: &str = "recent-files";
/// How many recent files are kept.
const RECENT_LIMIT: usize = 8;
/// The save file extension.
const EXTENSION: &str = "aicm";

/// An action that drops the open graph, so it waits until the player agrees to lose unsaved
/// changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pending {
    New,
    Open,
    OpenPath(PathBuf),
    Quit,
}

impl Workspace {
    pub fn recent_files(&self) -> &[PathBuf] {
        &self.recent_files
    }

    pub fn set_recent_files(&mut self, files: Vec<PathBuf>) {
        self.recent_files = files;
        self.recent_files.truncate(RECENT_LIMIT);
    }

    /// Whether a dialog asks the player to drop unsaved changes.
    pub fn is_confirming_discard(&self) -> bool {
        self.confirm_discard.is_some()
    }

    pub fn new_dialog_is_open(&self) -> bool {
        self.new_dialog.is_some()
    }

    /// Runs `action` now, or once the player agrees to drop unsaved changes.
    pub fn request(&mut self, action: Pending, strings: &Strings) {
        if self.doc.is_modified() {
            self.confirm_discard = Some(action);
        } else {
            self.perform(action, strings);
        }
    }

    fn perform(&mut self, action: Pending, strings: &Strings) {
        match action {
            Pending::New => self.new_dialog = Some(self.doc.graph().region().clone()),
            Pending::Open => {
                let picked = rfd::FileDialog::new()
                    .add_filter("AIC Modeler", &[EXTENSION])
                    .pick_file();
                if let Some(path) = picked {
                    self.open_file(&path, strings);
                }
            }
            Pending::OpenPath(path) => self.open_file(&path, strings),
            Pending::Quit => self.allow_close = true,
        }
    }

    /// Opens a save file in place of the current graph. A failure leaves the current graph open and
    /// says why in the status bar.
    pub fn open_file(&mut self, path: &Path, strings: &Strings) {
        match self.read_file(path, strings) {
            Ok(()) => self.status = None,
            Err(message) => self.status = Some(message),
        }
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
        self.other_data_version =
            (loaded.data_version != self.catalog.version()).then_some(loaded.data_version);
        let doc = Document::from_graph(&self.catalog, loaded.graph, Some(path.to_owned()));
        self.set_document(doc);
        self.remember_recent(path);
        Ok(())
    }

    /// Saves to the open file, or asks where when the graph has never been saved.
    pub fn save(&mut self, strings: &Strings) {
        match self.doc.path().map(Path::to_owned) {
            Some(path) => self.save_to(&path, strings),
            None => self.save_as(strings),
        }
    }

    pub fn save_as(&mut self, strings: &Strings) {
        let name = self
            .doc
            .path()
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

    /// Writes the graph to `path`, recording the data version it was made with.
    pub fn save_to(&mut self, path: &Path, strings: &Strings) {
        let json = self.doc.graph().to_json(self.catalog.version());
        match std::fs::write(path, json) {
            Ok(()) => {
                self.doc.mark_saved(path.to_owned());
                self.other_data_version = None;
                self.remember_recent(path);
                self.status = None;
            }
            Err(error) => {
                let file = path.display().to_string();
                self.status = Some(strings.format(
                    "file-save-failed",
                    &[("file", &file), ("error", &error.to_string())],
                ));
            }
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

    /// Asks before the window closes on unsaved changes.
    pub(crate) fn handle_close_request(&mut self, ctx: &egui::Context) {
        if self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if ctx.input(|i| i.viewport().close_requested()) && self.doc.is_modified() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm_discard = Some(Pending::Quit);
        }
    }

    /// The window title: the file's name, marked when it has unsaved changes.
    pub(crate) fn update_title(&mut self, ctx: &egui::Context, strings: &Strings) {
        let file = self
            .doc
            .path()
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

    /// The discard-changes question and the New dialog.
    pub(crate) fn file_dialogs(&mut self, ctx: &egui::Context, strings: &Strings) {
        if let Some(action) = self.confirm_discard.clone() {
            let mut answer = None;
            let modal = egui::Modal::new(egui::Id::new("confirm-discard")).show(ctx, |ui| {
                ui.set_width(320.0);
                ui.heading(strings.get("discard-title"));
                ui.label(strings.get("discard-body"));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(strings.get("discard-confirm")).clicked() {
                        answer = Some(true);
                    }
                    if ui.button(strings.get("dialog-cancel")).clicked() {
                        answer = Some(false);
                    }
                });
            });
            if modal.should_close() && answer.is_none() {
                answer = Some(false);
            }
            match answer {
                Some(true) => {
                    self.confirm_discard = None;
                    self.perform(action, strings);
                }
                Some(false) => self.confirm_discard = None,
                None => {}
            }
        }

        if let Some(mut region) = self.new_dialog.clone() {
            let mut answer = None;
            let modal = egui::Modal::new(egui::Id::new("new-graph")).show(ctx, |ui| {
                ui.set_width(320.0);
                ui.heading(strings.get("new-title"));
                ui.label(strings.get("new-region"));
                for id in self.catalog.regions().keys() {
                    let name = names::region(&self.catalog, id.as_str());
                    ui.radio_value(&mut region, id.clone(), RichText::new(name));
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(strings.get("new-create")).clicked() {
                        answer = Some(true);
                    }
                    if ui.button(strings.get("dialog-cancel")).clicked() {
                        answer = Some(false);
                    }
                });
            });
            if modal.should_close() && answer.is_none() {
                answer = Some(false);
            }
            match answer {
                Some(true) => {
                    self.new_dialog = None;
                    self.new_graph(region);
                }
                Some(false) => self.new_dialog = None,
                None => self.new_dialog = Some(region),
            }
        }
    }

    /// Starts the New dialog on `region`, as File > New does once any unsaved changes are dropped.
    pub fn open_new_dialog(&mut self, region: RegionId) {
        self.new_dialog = Some(region);
    }
}
