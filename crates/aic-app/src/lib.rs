//! The AIC Modeler desktop UI. `main.rs` only opens the window; everything else lives here so tests
//! can drive the app headlessly.

pub mod i18n;

use eframe::egui;

use crate::i18n::Strings;

/// Initial window size in points.
pub const WINDOW_SIZE: [f32; 2] = [1280.0, 800.0];

pub struct App {
    #[expect(dead_code, reason = "used once the UI has widgets (M4)")]
    strings: Strings,
}

impl App {
    pub fn new(strings: Strings) -> Self {
        Self { strings }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |_ui| {});
    }
}
