use std::path::PathBuf;

use aic_app::data::DataFolder;
use aic_app::i18n::Strings;
use aic_app::{App, WINDOW_SIZE};

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
        Box::new(|cc| Ok(Box::new(App::from_folder(strings, folder, cc.storage)))),
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
