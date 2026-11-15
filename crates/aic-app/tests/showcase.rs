//! Renders the design system showcase (`examples/showcase.rs`) to `tests/snapshots/showcase.png`.

#[allow(dead_code)]
#[path = "../examples/showcase.rs"]
mod showcase;

use egui_kittest::Harness;

#[test]
fn design_system_showcase() {
    let mut harness = Harness::builder()
        .with_size([1440.0, 1500.0])
        .build_eframe(|cc| {
            aic_app::theme::install(&cc.egui_ctx);
            showcase::Showcase::default()
        });
    harness.run();
    harness.snapshot("showcase");
}
