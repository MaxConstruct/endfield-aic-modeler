//! The AIC Modeler UI, on the desktop and in the browser. `main.rs` only finds the data and opens
//! the window (or starts the web runner); everything else lives here so tests can drive the app
//! headlessly.

pub use aic_editor::{data, format, i18n, names, store, throttle};
mod browser;
pub mod canvas;
pub mod document;
pub mod edit;
pub mod editor;
mod export;
pub mod files;
mod inspector;
pub mod picker;
mod port_menu;
pub mod settings;
pub mod theme;
mod toolbar;
pub mod views;
#[cfg(target_arch = "wasm32")]
mod web;
mod widgets;

#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;
use std::path::PathBuf;

use aic_data::Catalog;
#[cfg(not(target_arch = "wasm32"))]
use aic_data::LoadError;
use eframe::egui::{self, RichText};

use crate::data::{DataChoice, DataFolder};
use crate::editor::Editor;
use crate::i18n::Strings;
use crate::settings::Availability;
use crate::store::{BROWSER_FILES_KEY, BrowserFiles};

/// Initial window size in points.
pub const WINDOW_SIZE: [f32; 2] = [1280.0, 800.0];

pub struct App {
    strings: Strings,
    state: State,
}

enum State {
    /// The web build fetching game data from its site.
    #[cfg(target_arch = "wasm32")]
    Loading(web::DataDownload),
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
                if let Some(show) =
                    storage.and_then(|s| eframe::get_value::<bool>(s, editor::PERFORMANCE_KEY))
                {
                    editor.set_show_performance(show);
                }
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
        let mut app = Self::new(strings, catalog, storage);
        if let State::Ready(editor) = &mut app.state {
            editor.set_data_folder(folder, choice);
        }
        if cfg!(target_arch = "wasm32") {
            app.open_browser_files(storage);
        } else {
            app.open_drafts_dir(storage);
        }
        app
    }

    /// An app showing `catalog` that keeps its files in the browser's storage, as the web build
    /// does, starting with the files and last document in `storage`.
    pub fn in_browser(
        strings: Strings,
        catalog: Result<Catalog, String>,
        storage: Option<&dyn eframe::Storage>,
    ) -> Self {
        let mut app = Self::new(strings, catalog, storage);
        app.open_browser_files(storage);
        app
    }

    /// The web build: fetches the game data from the site, then opens as [`Self::from_folder`].
    #[cfg(target_arch = "wasm32")]
    pub fn loading(strings: Strings, ctx: &egui::Context) -> Self {
        Self {
            strings,
            state: State::Loading(web::DataDownload::start(ctx)),
        }
    }

    /// Keeps drafts in eframe's own folder, and reopens the last document.
    fn open_drafts_dir(&mut self, storage: Option<&dyn eframe::Storage>) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let app_name = self.strings.get("app-title");
            if let State::Ready(editor) = &mut self.state {
                editor.set_drafts_dir(eframe::storage_dir(&app_name).map(|dir| dir.join("drafts")));
            }
        }
        self.reopen_last(storage);
    }

    /// Keeps files in the browser's storage, and reopens the last document.
    fn open_browser_files(&mut self, storage: Option<&dyn eframe::Storage>) {
        if let State::Ready(editor) = &mut self.state {
            let files = storage
                .and_then(|s| eframe::get_value::<BrowserFiles>(s, BROWSER_FILES_KEY))
                .unwrap_or_default();
            editor.use_browser_files(files);
        }
        self.reopen_last(storage);
    }

    fn reopen_last(&mut self, storage: Option<&dyn eframe::Storage>) {
        let last = storage.and_then(|s| eframe::get_value::<PathBuf>(s, files::LAST_KEY));
        if let (State::Ready(editor), Some(last)) = (&mut self.state, last) {
            editor.reopen(&last, &self.strings);
        }
        let layout =
            storage.and_then(|s| eframe::get_value::<views::Layout>(s, views::STORAGE_KEY));
        if let (State::Ready(editor), Some(layout)) = (&mut self.state, layout) {
            editor.restore_layout(layout);
        }
    }

    pub fn editor(&self) -> Option<&Editor> {
        match &self.state {
            State::Ready(editor) => Some(editor),
            _ => None,
        }
    }

    pub fn editor_mut(&mut self) -> Option<&mut Editor> {
        match &mut self.state {
            State::Ready(editor) => Some(editor),
            _ => None,
        }
    }

    /// The web build: once the data has come, opens the editor on it.
    #[cfg(target_arch = "wasm32")]
    fn poll_download(&mut self, ui: &mut egui::Ui, frame: &eframe::Frame) {
        let State::Loading(download) = &mut self.state else {
            return;
        };
        match download.poll() {
            web::Fetched::Loading { done, total } => {
                let (done, total) = (done.to_string(), total.to_string());
                let text = self
                    .strings
                    .format("data-loading", &[("done", &done), ("total", &total)]);
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.centered_and_justified(|ui| {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(text);
                        });
                    });
                });
            }
            web::Fetched::Failed { file, error } => {
                let message = self
                    .strings
                    .format("data-fetch-failed", &[("file", &file), ("error", &error)]);
                self.state = State::Failed(message);
            }
            web::Fetched::Done(bundle) => {
                let strings = std::mem::replace(&mut self.strings, Strings::english());
                let folder = DataFolder::bundled(std::sync::Arc::new(bundle))
                    .ok_or_else(|| strings.get("load-failed-title"));
                *self = Self::from_folder(strings, folder, frame.storage());
            }
        }
    }

    /// The web build: hands the files the editor saved or exported to the browser to download.
    #[cfg(target_arch = "wasm32")]
    fn send_downloads(&mut self) {
        let Self { strings, state } = self;
        let State::Ready(editor) = state else {
            return;
        };
        for file in editor.take_downloads() {
            if let Err(error) = web::download(&file) {
                let message =
                    strings.format("export-failed", &[("file", &file.name), ("error", &error)]);
                editor.status = Some(message);
            }
        }
    }
}

/// Finds the data folder (the `--data` flag first, else the built-in data; see `DataFolder::find`)
/// and loads the newest data version in English.
#[cfg(not(target_arch = "wasm32"))]
pub fn load_catalog(flag: Option<&Path>) -> Result<Catalog, LoadError> {
    DataFolder::find(flag)?.load(&DataChoice::default())
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        #[cfg(target_arch = "wasm32")]
        self.poll_download(ui, _frame);
        match &mut self.state {
            #[cfg(target_arch = "wasm32")]
            State::Loading(_) => {}
            State::Ready(editor) => editor.ui(ui, &self.strings),
            State::Failed(message) => {
                let hint = if cfg!(target_arch = "wasm32") {
                    "load-failed-hint-web"
                } else {
                    "load-failed-hint"
                };
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.heading(self.strings.get("load-failed-title"));
                    ui.label(RichText::new(message.as_str()).monospace());
                    ui.label(self.strings.get(hint));
                });
            }
        }
        #[cfg(target_arch = "wasm32")]
        self.send_downloads();
    }

    /// eframe saves the window size and position with the settings, by default only every 30 s and
    /// on a clean quit. Save as often as the graph autosaves, so a killed app still reopens as it was.
    fn auto_save_interval(&self) -> std::time::Duration {
        files::AUTOSAVE_INTERVAL
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
            eframe::set_value(storage, editor::PERFORMANCE_KEY, &editor.show_performance());
            eframe::set_value(storage, views::STORAGE_KEY, &editor.layout());
            if let Some(files) = editor.browser_files() {
                eframe::set_value(storage, BROWSER_FILES_KEY, files);
            }
            if let Some(choice) = editor.data_choice() {
                eframe::set_value(storage, data::STORAGE_KEY, choice);
            }
        }
    }
}
