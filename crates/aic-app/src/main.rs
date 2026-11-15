// A release build on Windows opens no console window beside the app; a debug build keeps it for logs.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;

use aic_app::App;
#[cfg(not(target_arch = "wasm32"))]
use aic_app::WINDOW_SIZE;
#[cfg(not(target_arch = "wasm32"))]
use aic_app::data::DataFolder;
use aic_app::i18n::Strings;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    let strings = Strings::english();
    let title = strings.get("app-title");
    let folder = DataFolder::find(data_flag().as_deref()).map_err(|error| error.to_string());
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(title.clone())
            .with_inner_size(WINDOW_SIZE),
        ..Default::default()
    };
    eframe::run_native(
        &title,
        options,
        Box::new(|cc| {
            aic_app::theme::install(&cc.egui_ctx);
            Ok(Box::new(App::from_folder(strings, folder, cc.storage)))
        }),
    )
}

/// The web build: runs the app in the page's canvas. Trunk calls this once the module has loaded.
#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;

    wasm_bindgen_futures::spawn_local(async {
        let canvas = eframe::web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id(CANVAS_ID))
            .and_then(|element| {
                element
                    .dyn_into::<eframe::web_sys::HtmlCanvasElement>()
                    .ok()
            })
            .expect("index.html has the app's canvas");
        let started = eframe::WebRunner::new()
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(|cc| {
                    aic_app::theme::install(&cc.egui_ctx);
                    Ok(Box::new(App::loading(Strings::english(), &cc.egui_ctx)))
                }),
            )
            .await;
        if let Err(error) = started {
            panic!("the app didn't start: {error:?}");
        }
    });
}

/// The `<canvas>` in `index.html` the app draws into.
#[cfg(target_arch = "wasm32")]
const CANVAS_ID: &str = "aic-modeler";

/// The path after `--data`, if given.
#[cfg(not(target_arch = "wasm32"))]
fn data_flag() -> Option<PathBuf> {
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--data" {
            return args.next().map(PathBuf::from);
        }
    }
    None
}
