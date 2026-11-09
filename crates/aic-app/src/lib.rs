//! The AIC Modeler desktop UI. `main.rs` only finds the data and opens the window; everything else
//! lives here so tests can drive the app headlessly.

mod browser;
pub mod canvas;
pub mod data;
pub mod document;
pub mod edit;
pub mod editor;
mod export;
pub mod files;
pub mod format;
pub mod i18n;
mod inspector;
pub mod names;
pub mod picker;
mod port_menu;
pub mod settings;
mod throttle;
mod toolbar;
mod widgets;

use std::path::{Path, PathBuf};

use aic_data::{Catalog, LoadError};
use eframe::egui::{self, RichText};

use crate::data::{DataChoice, DataFolder};
use crate::editor::Editor;
use crate::i18n::Strings;
use crate::settings::Availability;

/// Initial window size in points.
pub const WINDOW_SIZE: [f32; 2] = [1280.0, 800.0];

pub struct App {
    strings: Strings,
    state: State,
}

enum State {
    Ready(Box<Editor>),
    /// Game data didn't load; the window explains why.
    Failed(String),
}

impl App {
    /// An app showing `catalog`, or the reason it didn't load. Settings come from eframe storage
    /// when there is one.
    pub fn new(
        strings: Strings,
        catalog: Result<Catalog, String>,
        storage: Option<&dyn eframe::Storage>,
    ) -> Self {
        let availability = storage
            .and_then(|s| eframe::get_value::<Availability>(s, settings::STORAGE_KEY))
            .unwrap_or_default();
        let recent = storage
            .and_then(|s| eframe::get_value::<Vec<std::path::PathBuf>>(s, files::RECENT_KEY))
            .unwrap_or_default();
        let state = match catalog {
            Ok(catalog) => {
                let mut editor = Editor::new(catalog, availability);
                editor.set_recent_files(recent);
                State::Ready(Box::new(editor))
            }
            Err(message) => State::Failed(message),
        };
        Self { strings, state }
    }

    /// An app on the data folder `folder`, loading the data version and language last chosen. The
    /// settings window can switch between the folder's versions.
    pub fn from_folder(
        strings: Strings,
        folder: Result<DataFolder, String>,
        storage: Option<&dyn eframe::Storage>,
    ) -> Self {
        let choice = storage
            .and_then(|s| eframe::get_value::<DataChoice>(s, data::STORAGE_KEY))
            .unwrap_or_default();
        let folder = match folder {
            Ok(folder) => folder,
            Err(message) => return Self::new(strings, Err(message), storage),
        };
        // A chosen version that no longer loads falls back to the newest.
        let catalog = folder
            .load(&choice)
            .or_else(|_| folder.load(&DataChoice::default()))
            .map_err(|error| error.to_string());
        let last = storage.and_then(|s| eframe::get_value::<PathBuf>(s, files::LAST_KEY));
        let mut app = Self::new(strings, catalog, storage);
        let app_name = app.strings.get("app-title");
        let Self { strings, state } = &mut app;
        if let State::Ready(editor) = state {
            editor.set_data_folder(folder, choice);
            editor.set_drafts_dir(eframe::storage_dir(&app_name).map(|dir| dir.join("drafts")));
            if let Some(last) = last {
                editor.reopen(&last, strings);
            }
        }
        app
    }

    pub fn editor(&self) -> Option<&Editor> {
        match &self.state {
            State::Ready(editor) => Some(editor),
            State::Failed(_) => None,
        }
    }

    pub fn editor_mut(&mut self) -> Option<&mut Editor> {
        match &mut self.state {
            State::Ready(editor) => Some(editor),
            State::Failed(_) => None,
        }
    }
}

/// Finds the data folder (the `--data` flag first, see `aic_data::find_data_dir`) and loads the
/// newest data version in English.
pub fn load_catalog(flag: Option<&Path>) -> Result<Catalog, LoadError> {
    DataFolder::find(flag)?.load(&DataChoice::default())
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        match &mut self.state {
            State::Ready(editor) => editor.ui(ui, &self.strings),
            State::Failed(message) => {
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.heading(self.strings.get("load-failed-title"));
                    ui.label(RichText::new(message.as_str()).monospace());
                    ui.label(self.strings.get("load-failed-hint"));
                });
            }
        }
    }

    /// eframe calls this every so often and when the app closes: a last save of the graph, and the
    /// settings.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if let State::Ready(editor) = &mut self.state {
            editor.save_now(&self.strings);
            if let Some(last) = editor.last_document() {
                eframe::set_value(storage, files::LAST_KEY, &last.to_path_buf());
            }
            eframe::set_value(storage, settings::STORAGE_KEY, editor.availability());
            eframe::set_value(storage, files::RECENT_KEY, &editor.recent_files());
            if let Some(choice) = editor.data_choice() {
                eframe::set_value(storage, data::STORAGE_KEY, choice);
            }
        }
    }
}
