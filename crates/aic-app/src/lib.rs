//! The AIC Modeler desktop UI. `main.rs` only finds the data and opens the window; everything else
//! lives here so tests can drive the app headlessly.

mod browser;
pub mod canvas;
pub mod document;
pub mod format;
pub mod i18n;
mod inspector;
pub mod names;
pub mod picker;
pub mod settings;
mod widgets;
pub mod workspace;

use std::path::Path;

use aic_data::{Catalog, LoadError};
use eframe::egui::{self, RichText};

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
        let state = match catalog {
            Ok(catalog) => State::Ready(Box::new(Workspace::new(catalog, availability))),
            Err(message) => State::Failed(message),
        };
        Self { strings, state }
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
    let dir = aic_data::find_data_dir(flag)?;
    let versions = aic_data::list_versions(&dir)?;
    let newest = versions
        .first()
        .ok_or(LoadError::NoDataDir { tried: vec![dir] })?;
    aic_data::load(&newest.path, "en")
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
        }
    }
}
