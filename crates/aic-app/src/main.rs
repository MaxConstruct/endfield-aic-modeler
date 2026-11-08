use aic_app::i18n::Strings;
use aic_app::{App, WINDOW_SIZE};

fn main() -> eframe::Result {
    let strings = Strings::english();
    let title = strings.get("app-title");
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(title.clone())
            .with_inner_size(WINDOW_SIZE),
        ..Default::default()
    };
    eframe::run_native(
        &title,
        options,
        Box::new(|_cc| Ok(Box::new(App::new(strings)))),
    )
}
