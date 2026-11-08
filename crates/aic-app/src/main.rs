mod i18n;

use i18n::Strings;

fn main() -> eframe::Result {
    let strings = Strings::english();
    let title = strings.get("app-title");
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(title.clone())
            .with_inner_size([1280.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native(
        &title,
        options,
        Box::new(|_cc| Ok(Box::new(App { strings }))),
    )
}

struct App {
    #[expect(dead_code, reason = "used once the UI has widgets (M4)")]
    strings: Strings,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut eframe::egui::Ui, _frame: &mut eframe::Frame) {
        eframe::egui::CentralPanel::default().show(ui, |_ui| {});
    }
}
