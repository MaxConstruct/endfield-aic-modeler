//! M12 step 5: a frame's allocations, counted, so they don't creep back up.
//!
//! dhat replaces the global allocator, so this runs only with the `dhat-heap` feature:
//!
//! ```sh
//! cargo test -p aic-app --features dhat-heap --test allocations
//! ```
//!
//! A frame that makes more allocations than the ceiling fails. When a change lowers the count,
//! lower the ceiling to the new count (the failure message gives it); when it must raise it, say
//! why in docs/PLAN.md.
#![cfg(feature = "dhat-heap")]

use aic_app::App;
use aic_app::i18n::Strings;
use aic_core::{NodeId, NodeKind, PortRef};
use eframe::egui::{self, Pos2, vec2};

/// Allocations of one idle frame of [`plate_chain`] at 100%, UI and tessellation, as counted at
/// the end of M12 step 4 (2026-11-23): the same in every run, in the test profile. A release
/// build counts differently.
const CEILING: u64 = 1113;

/// Iron ore from a source, smelted, pressed into plates, into a sink: four cards, three links.
fn plate_chain(app: &mut App) {
    let editor = app.editor_mut().expect("the fixture loads");
    let mut doc = editor.document_mut();
    let mut add = |kind, x| doc.add_node(kind, Pos2::new(x, 0.0));
    let ore = add(
        NodeKind::Source {
            item: "iron_ore".into(),
            limit: 60.0,
        },
        0.0,
    );
    let ingot = add(
        NodeKind::Recipe {
            recipe: "iron_ingot".into(),
            facility: "smelter".into(),
            limit: Some(1.0),
        },
        400.0,
    );
    let plate = add(
        NodeKind::Recipe {
            recipe: "iron_plate".into(),
            facility: "constructor".into(),
            limit: None,
        },
        800.0,
    );
    let sink = add(
        NodeKind::Sink {
            item: "iron_plate".into(),
            limit: None,
        },
        1200.0,
    );
    let links: [(NodeId, NodeId, &str); 3] = [
        (ore, ingot, "iron_ore"),
        (ingot, plate, "iron_ingot"),
        (plate, sink, "iron_plate"),
    ];
    for (from, to, item) in links {
        doc.connect(PortRef::new(from, item), PortRef::new(to, item))
            .expect("the link is valid");
    }
}

#[test]
fn an_idle_frame_allocates_no_more_than_it_did() {
    let ctx = egui::Context::default();
    aic_app::theme::install(&ctx);
    let catalog = aic_editor::testing::fixture_catalog();
    let mut app = App::new(Strings::english(), Ok(catalog), None);
    plate_chain(&mut app);
    let mut time = 0.0;
    let mut frame = |app: &mut App| {
        time += 1.0 / 60.0;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 900.0))),
            time: Some(time),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            eframe::App::ui(app, ui, &mut eframe::Frame::_new_kittest());
        });
        // Nothing paints here, so no texture is uploaded.
        output.textures_delta.clear();
        let _ = ctx.tessellate(output.shapes, output.pixels_per_point);
    };
    // The first solve, the icons, the fit to the cards and egui's caches settle first.
    app.editor_mut().expect("loaded").zoom_canvas_to(1.0);
    for _ in 0..30 {
        frame(&mut app);
    }

    let _profiler = dhat::Profiler::builder().testing().build();
    let before = dhat::HeapStats::get().total_blocks;
    frame(&mut app);
    let made = dhat::HeapStats::get().total_blocks - before;
    assert!(
        made <= CEILING,
        "an idle frame made {made} allocations, more than the {CEILING} it made at the end of \
         M12 step 4"
    );
}
