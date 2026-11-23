//! The AIC Modeler UI, on the desktop and in the browser. `main.rs` only finds the data and opens
//! the window (or starts the web runner); everything else lives here so tests can drive the app
//! headlessly.

pub use aic_editor::{data, i18n, store};
mod browser;
pub mod canvas;
pub mod document;
pub mod edit;
pub mod editor;
mod export;
pub mod files;
pub mod icons;
mod inspector;
pub mod picker;
mod port_menu;
pub mod settings;
pub mod theme;
mod toolbar;
mod tutorial;
pub mod views;
#[cfg(target_arch = "wasm32")]
pub mod web;
mod widgets;

/// mimalloc instead of the system allocator on the desktop. A frame makes many small allocations
/// (egui text and meshes, the solver's model), and on Windows the system heap took about a third
/// of each frame's CPU time. Declared here rather than in `main.rs`, so the perf examples and the
/// tests run on the same allocator as the app.
#[cfg(all(not(target_arch = "wasm32"), not(feature = "dhat-heap")))]
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// With the `dhat-heap` feature, dhat's allocator in mimalloc's place: it counts every allocation
/// while a `dhat::Profiler` runs, and is the system allocator otherwise. For measuring only.
#[cfg(all(not(target_arch = "wasm32"), feature = "dhat-heap"))]
#[global_allocator]
static ALLOCATOR: dhat::Alloc = dhat::Alloc;

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
    /// The web build: whether the loading page is still over the canvas.
    #[cfg(target_arch = "wasm32")]
    loading_page: bool,
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
                if let Some(show) =
                    storage.and_then(|s| eframe::get_value::<bool>(s, editor::PERFORMANCE_KEY))
                {
                    editor.set_show_performance(show);
                }
                State::Ready(Box::new(editor))
            }
            Err(message) => State::Failed(message),
        };
        Self {
            strings,
            state,
            #[cfg(target_arch = "wasm32")]
            loading_page: true,
        }
    }

    /// An app on the data folder `folder`, loading the data version and language last chosen. The
    /// Settings section can switch between the folder's versions.
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
        // A chosen version that no longer loads falls back to the newest, which is then the choice.
        let (catalog, loaded) = match folder.open(choice) {
            Ok(loaded) => (Ok(loaded.catalog), Some((loaded.choice, loaded.failed))),
            Err(error) => (Err(error.to_string()), None),
        };
        let mut app = Self::new(strings, catalog, storage);
        let mut failed = None;
        if let (State::Ready(editor), Some((choice, error))) = (&mut app.state, loaded) {
            editor.set_data_folder(folder, choice);
            failed = error;
        }
        if cfg!(target_arch = "wasm32") {
            app.open_browser_files(storage);
        } else {
            app.open_drafts_dir(storage);
        }
        // After the reopen, which sets or clears the status: the data error is the one to see.
        if let (State::Ready(editor), Some(error)) = (&mut app.state, failed) {
            editor.session.report_data_failure(&error);
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

    /// Reopens the last document and view layout; with nothing reopened, opens the tutorial.
    fn reopen_last(&mut self, storage: Option<&dyn eframe::Storage>) {
        let last = storage.and_then(|s| eframe::get_value::<PathBuf>(s, files::LAST_KEY));
        if let (State::Ready(editor), Some(last)) = (&mut self.state, last) {
            editor.reopen(&last);
        }
        if let State::Ready(editor) = &mut self.state {
            editor.open_tutorial_if_fresh();
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

    /// The web build: hands the files the editor saved or exported to the browser to download.
    #[cfg(target_arch = "wasm32")]
    fn send_downloads(&mut self) {
        let Self { strings, state, .. } = self;
        let State::Ready(editor) = state else {
            return;
        };
        for file in editor.take_downloads() {
            if let Err(error) = web::download(&file) {
                let message =
                    strings.format("export-failed", &[("file", &file.name), ("error", &error)]);
                editor.session.set_status(Some(message));
            }
        }
    }
}

/// The window's icon, for its title bar, the taskbar and the Dock: the plate drawn for Windows, or
/// the macOS-shaped one (with Apple's margin and shadow) on a Mac. A packaged app's own icon is in
/// its .exe (`build.rs`) or its bundle (`scripts/package-macos.sh`).
#[cfg(not(target_arch = "wasm32"))]
fn window_icon() -> egui::IconData {
    #[cfg(target_os = "macos")]
    let png = include_bytes!("../assets/app-icon/window-macos.png");
    #[cfg(not(target_os = "macos"))]
    let png = include_bytes!("../assets/app-icon/window-windows.png");
    eframe::icon_data::from_png_bytes(png).expect("the window icon is a PNG")
}

/// The desktop window's options, titled `title`: `main.rs` opens the app with them, and
/// `perf_app` times a start-up with them.
///
/// On Windows wgpu starts only its DX12 backend. Its default, every backend (Vulkan, DX12 and
/// GL), took about 300 ms longer to start on the Windows PC and drew no faster (PLAN.md, M13 step
/// 6). `WGPU_BACKEND` (`vulkan`, `gl`, or a list such as `dx12,vulkan`) still chooses others, for
/// a PC whose DX12 doesn't work.
#[cfg(not(target_arch = "wasm32"))]
pub fn native_options(title: &str) -> eframe::NativeOptions {
    #[allow(unused_mut)]
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(title)
            .with_inner_size(WINDOW_SIZE)
            .with_icon(window_icon()),
        ..Default::default()
    };
    #[cfg(windows)]
    if let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &mut options.wgpu_options.wgpu_setup {
        setup.instance_descriptor.backends =
            eframe::wgpu::Backends::from_env().unwrap_or(eframe::wgpu::Backends::DX12);
    }
    options
}

/// Finds the data folder (the `--data` flag first, else the built-in data; see `DataFolder::find`)
/// and loads the newest data version in English.
#[cfg(not(target_arch = "wasm32"))]
pub fn load_catalog(flag: Option<&Path>) -> Result<Catalog, LoadError> {
    DataFolder::find(flag)?.load(&DataChoice::default())
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        match &mut self.state {
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
        {
            self.send_downloads();
            // The first frame is drawn at the end of this one: the editor shows, ready, with the
            // page gone (PLAN.md, M13 step 4).
            if std::mem::take(&mut self.loading_page) {
                web::remove_loading_page();
            }
        }
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
            editor.save_now();
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
