//! The AIC Modeler desktop UI. `main.rs` only finds the data and opens the window; everything else
//! lives here so tests can drive the app headlessly.

mod browser;
pub mod canvas;
pub mod data;
pub mod document;
pub mod edit;
mod export;
pub mod files;
pub mod format;
pub mod i18n;
mod inspector;
pub mod names;
pub mod picker;
mod port_menu;
pub mod settings;
mod widgets;
pub mod workspace;

use std::path::Path;

use aic_data::{Catalog, LoadError};
use eframe::egui::{self, RichText};

use crate::data::{DataChoice, DataFolder};
use crate::i18n::Strings;
use crate::settings::Availability;
use crate::workspace::Workspace;

/// Initial window size in points.
pub const WINDOW_SIZE: [f32; 2] = [1280.0, 800.0];

pub struct App {
    strings: Strings,
    state: State,
}

enum State {
    Ready(Box<Workspace>),
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
                let mut workspace = Workspace::new(catalog, availability);
                workspace.set_recent_files(recent);
                State::Ready(Box::new(workspace))
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
        let mut app = Self::new(strings, catalog, storage);
        if let Some(workspace) = app.workspace_mut() {
            workspace.set_data_folder(folder, choice);
        }
        app
    }

    pub fn workspace(&self) -> Option<&Workspace> {
        match &self.state {
            State::Ready(workspace) => Some(workspace),
            State::Failed(_) => None,
        }
    }

    pub fn workspace_mut(&mut self) -> Option<&mut Workspace> {
        match &mut self.state {
            State::Ready(workspace) => Some(workspace),
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
            State::Ready(workspace) => workspace.ui(ui, &self.strings),
            State::Failed(message) => {
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.heading(self.strings.get("load-failed-title"));
                    ui.label(RichText::new(message.as_str()).monospace());
                    ui.label(self.strings.get("load-failed-hint"));
                });
            }
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if let State::Ready(workspace) = &self.state {
            eframe::set_value(storage, settings::STORAGE_KEY, workspace.availability());
            eframe::set_value(storage, files::RECENT_KEY, &workspace.recent_files());
            if let Some(choice) = workspace.data_choice() {
                eframe::set_value(storage, data::STORAGE_KEY, choice);
            }
        }
    }
}
