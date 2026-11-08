//! Renders the app headlessly and compares it with the images in `tests/snapshots/`.
//!
//! After an intended UI change, run `UPDATE_SNAPSHOTS=1 cargo test -p aic-app` and review the new
//! images before committing them.

use aic_app::i18n::Strings;
use aic_app::{App, WINDOW_SIZE};
use egui_kittest::Harness;

#[test]
fn empty_window() {
    let mut harness = Harness::builder()
        .with_size(WINDOW_SIZE)
        .build_eframe(|_cc| App::new(Strings::english()));
    harness.run();
    harness.snapshot("empty_window");
}
