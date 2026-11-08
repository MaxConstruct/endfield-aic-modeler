use std::path::PathBuf;

use aic_app::i18n::Strings;
use aic_app::{App, WINDOW_SIZE, load_catalog};

fn main() -> eframe::Result {
    let strings = Strings::english();
    let title = strings.get("app-title");
    let catalog = load_catalog(data_flag().as_deref()).map_err(|error| error.to_string());
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(title.clone())
            .with_inner_size(WINDOW_SIZE),
        ..Default::default()
    };
    eframe::run_native(
        &title,
        options,
        Box::new(|cc| Ok(Box::new(App::new(strings, catalog, cc.storage)))),
    )
}

/// The path after `--data`, if given.
fn data_flag() -> Option<PathBuf> {
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--data" {
            return args.next().map(PathBuf::from);
        }
    }
    None
}
