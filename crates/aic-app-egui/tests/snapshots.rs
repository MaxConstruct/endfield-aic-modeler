//! Drives the app headlessly and compares it with the images in `tests/snapshots/`.
//!
//! After an intended UI change, run `UPDATE_SNAPSHOTS=1 cargo test -p aic-app-egui` and review the new
//! images before committing them.

use std::path::{Path, PathBuf};

use aic_app_egui::canvas::Selection;
use aic_app_egui::i18n::Strings;
use aic_app_egui::picker::Origin;
use aic_app_egui::store::BrowserFiles;
use aic_app_egui::{App, WINDOW_SIZE};
use aic_core::{Graph, NodeId, NodeKind, Port, PortRef, Side, Solution};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

const TOLERANCE: f64 = 1e-6;

fn fixture(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}

fn harness(version_dir: &Path) -> Harness<'static, App> {
    let catalog = aic_data::load(version_dir, "en").expect("data loads");
    let mut harness = Harness::builder()
        .with_size(WINDOW_SIZE)
        .build_eframe(|cc| {
            aic_app_egui::theme::install(&cc.egui_ctx);
            App::new(Strings::english(), Ok(catalog), None)
        });
    // The frame rate and solve time change from run to run; snapshots leave them out.
    hide_performance(&mut harness);
    harness.run();
    // The inspector starts closed; most tests read or type into it.
    harness
        .get_by_role_and_label(eframe::egui::accesskit::Role::Button, "Inspector")
        .click();
    harness.run();
    harness
}

fn hide_performance(harness: &mut Harness<'_, App>) {
    if let Some(editor) = harness.state_mut().editor_mut() {
        editor.set_show_performance(false);
        // A lit button stays lit, however fast the test runs.
        editor.hold_flash(true);
    }
}

fn editor<'a>(harness: &'a mut Harness<'_, App>) -> &'a mut aic_app_egui::editor::Editor {
    harness.state_mut().editor_mut().expect("data loaded")
}

fn selected(harness: &mut Harness<'_, App>) -> NodeId {
    match editor(harness).selection().and_then(Selection::single_node) {
        Some(node) => node,
        None => panic!(
            "expected one selected node, got {:?}",
            editor(harness).selection()
        ),
    }
}

/// Opens the picker from `node`'s port of `item` and clicks `label`: one click adds the node and
/// links it. `offset` places the new node relative to `node`.
fn pick(
    harness: &mut Harness<'_, App>,
    node: NodeId,
    side: Side,
    item: &str,
    offset: (f32, f32),
    label: &str,
) -> NodeId {
    let ws = editor(harness);
    let pos = ws.document().graph().node(node).expect("node exists").pos;
    let at = eframe::egui::pos2(pos.x + offset.0, pos.y + offset.1);
    ws.open_picker(
        Origin::Port {
            node,
            side,
            item: item.into(),
        },
        at,
    );
    harness.run();
    // The recipe browser lists the same names; the picker, drawn last, comes last.
    harness
        .get_all_by_label(label)
        .last()
        .unwrap_or_else(|| panic!("the picker offers {label}"))
        .click();
    harness.run();
    assert!(
        !editor(harness).picker_is_open(),
        "one click closes the picker"
    );
    selected(harness)
}

/// Stands in for dragging a link between two existing nodes.
fn link(harness: &mut Harness<'_, App>, from: NodeId, to: NodeId, item: &str) {
    let ws = editor(harness);
    ws.document_mut()
        .connect(PortRef::new(from, item), PortRef::new(to, item))
        .expect("link is valid");
    harness.run();
}

/// Types a new limit into the inspector's limit field of the selected node, replacing what is there.
fn type_limit(harness: &mut Harness<'_, App>, limit: &str) {
    let role = eframe::egui::accesskit::Role::TextInput;
    harness.get_by_role_and_label(role, "Limit").focus();
    harness.run();
    harness.key_press_modifiers(eframe::egui::Modifiers::COMMAND, eframe::egui::Key::A);
    harness.run();
    harness
        .get_by_role_and_label(role, "Limit")
        .type_text(limit);
    harness.run();
}

/// Fits the canvas to the graph, as a double-click on the background does: egui-snarl fits the
/// view the first time it sees the canvas.
/// The middle of an output port's rate, left of its pin at `pin`: 24 (half the 48 icon) plus the
/// 4 gap plus half the 54 rate, scaled by the canvas zoom.
fn beside_output_pin(
    harness: &mut Harness<'_, App>,
    pin: eframe::egui::Pos2,
) -> eframe::egui::Pos2 {
    let zoom = editor(harness).canvas_view().map_or(1.0, |view| view.zoom);
    pin - eframe::egui::vec2(55.0 * zoom, 0.0)
}

fn fit_view(harness: &mut Harness<'_, App>) {
    harness
        .ctx
        .memory_mut(|memory| memory.data = Default::default());
    harness.run();
}

/// Snapshots a canvas with links. egui-snarl draws wires in hash-set order, so where two wires
/// cross, the one on top changes between runs; a few hundred such pixels are allowed.
fn snapshot_graph(harness: &mut Harness<'_, App>, name: &str) {
    let options = egui_kittest::SnapshotOptions::new().max_failed_pixels(500);
    harness.snapshot_options(name, &options);
}

#[track_caller]
fn assert_near(actual: Option<f64>, expected: f64) {
    let actual = actual.expect("value is determined");
    assert!(
        (actual - expected).abs() < TOLERANCE,
        "expected {expected}, got {actual}"
    );
}

fn count(solution: &Solution, node: NodeId) -> Option<f64> {
    solution.facility_count.get(&node).copied().flatten()
}

fn flow(solution: &Solution, node: NodeId, item: &str, side: Side) -> Option<f64> {
    let port = Port {
        node,
        item: item.into(),
        side,
    };
    solution.port_flow.get(&port).copied().flatten()
}

#[test]
fn load_error_screen() {
    let mut harness = Harness::builder()
        .with_size(WINDOW_SIZE)
        .build_eframe(|cc| {
            aic_app_egui::theme::install(&cc.egui_ctx);
            App::new(
                Strings::english(),
                Err("no data folder found (tried ./data)".into()),
                None,
            )
        });
    harness.run();
    harness.get_by_label("Could not load game data");
    harness.snapshot("load_error");
}

#[test]
fn empty_graph() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    harness.snapshot("empty_graph");
}

#[test]
fn canvas_picker_lists_logistics_and_recipes() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    editor(&mut harness).open_picker(Origin::Canvas, eframe::egui::pos2(0.0, 0.0));
    harness.run();
    harness.get_by_label("Splitter");
    harness.snapshot("picker_canvas");
}

// Golden test 7 (docs/PLAN.md, M3), rebuilt the way a player would: the frame from the recipe
// browser, then every upstream node through the picker from an input port, one click each.
#[test]
fn rebuild_golden_07_through_the_picker() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    harness.get_by_label("Modular Frame").click();
    harness.run();
    let frame = selected(&mut harness);

    let reinforced = pick(
        &mut harness,
        frame,
        Side::Input,
        "reinforced_plate",
        (-80.0, -60.0),
        "Reinforced Iron Plate",
    );
    let rod = pick(
        &mut harness,
        frame,
        Side::Input,
        "iron_rod",
        (-80.0, 260.0),
        "Iron Rod",
    );
    let plate = pick(
        &mut harness,
        reinforced,
        Side::Input,
        "iron_plate",
        (-80.0, -160.0),
        "Iron Plate",
    );
    let screw = pick(
        &mut harness,
        reinforced,
        Side::Input,
        "screw",
        (-80.0, 80.0),
        "Screw",
    );
    link(&mut harness, rod, screw, "iron_rod");
    let smelter = pick(
        &mut harness,
        rod,
        Side::Input,
        "iron_ingot",
        (-380.0, -120.0),
        "Iron Ingot",
    );
    link(&mut harness, smelter, plate, "iron_ingot");
    let ore = pick(
        &mut harness,
        smelter,
        Side::Input,
        "iron_ore",
        (-80.0, 0.0),
        "Source of Iron Ore",
    );
    // A new source starts at one belt (30/min); the player types 60.
    type_limit(&mut harness, "60");

    let ws = editor(&mut harness);
    let graph = ws.document().graph();
    assert_eq!(graph.nodes().len(), 7);
    assert_eq!(graph.links().len(), 8);
    assert_eq!(
        graph.node(ore).map(|n| &n.kind),
        Some(&NodeKind::Source {
            item: "iron_ore".into(),
            limit: 60.0
        })
    );
    let s = ws.document().solution();
    assert_near(count(s, smelter), 2.0);
    assert_near(count(s, plate), 1.125);
    assert_near(count(s, rod), 1.75);
    assert_near(count(s, screw), 1.125);
    assert_near(count(s, reinforced), 0.75);
    assert_near(count(s, frame), 1.25);
    assert_near(flow(s, plate, "iron_ingot", Side::Input), 33.75);
    assert_near(flow(s, rod, "iron_ingot", Side::Input), 26.25);
    assert_near(flow(s, screw, "screw", Side::Output), 45.0);
    assert_near(flow(s, frame, "modular_frame", Side::Output), 2.5);

    fit_view(&mut harness);
    snapshot_graph(&mut harness, "golden_07");
}

// Golden test 5 (docs/PLAN.md, M3) with the real v1.5.3 data: the Yazhen seed loop.
#[test]
fn rebuild_golden_05_through_the_picker() {
    let mut harness = harness(&fixture("../../data/v1.5.3"));
    editor(&mut harness).new_graph("jinlong".into());
    harness.run();
    // The browser lists recipes in data order; the planting recipe is called Yazhen.
    let planting = add_from_browser(&mut harness, "Yazhen");

    let picking = pick(
        &mut harness,
        planting,
        Side::Input,
        "plant_grass_seed_2",
        (-160.0, -200.0),
        "Yazhen Seeds",
    );
    let pump = pick(
        &mut harness,
        planting,
        Side::Input,
        "liquid_water",
        (-80.0, 200.0),
        "Clean Water",
    );
    let goal = pick(
        &mut harness,
        planting,
        Side::Output,
        "plant_grass_2",
        (360.0, 0.0),
        "Sink of Yazhen",
    );
    type_limit(&mut harness, "10");
    link(&mut harness, planting, picking, "plant_grass_2");

    let ws = editor(&mut harness);
    let s = ws.document().solution();
    assert_near(count(s, planting), 1.0 / 3.0);
    assert_near(count(s, picking), 1.0 / 3.0);
    assert_near(count(s, pump), 1.0 / 6.0);
    assert_near(s.rate.get(&goal).copied().flatten(), 10.0);
    assert_near(flow(s, picking, "plant_grass_2", Side::Input), 10.0);

    fit_view(&mut harness);
    snapshot_graph(&mut harness, "golden_05");
}

fn secondary_click_at(harness: &mut Harness<'_, App>, pos: eframe::egui::Pos2) {
    harness.hover_at(pos);
    harness.step();
    for pressed in [true, false] {
        harness.event(eframe::egui::Event::PointerButton {
            pos,
            button: eframe::egui::PointerButton::Secondary,
            pressed,
            modifiers: eframe::egui::Modifiers::NONE,
        });
    }
    harness.run();
}

/// Clicks `label` in the recipe browser, scrolling it into view first.
fn add_from_browser(harness: &mut Harness<'_, App>, label: &str) -> NodeId {
    harness.get_by_label(label).scroll_to_me();
    harness.run();
    harness.get_by_label(label).click();
    harness.run();
    selected(harness)
}

#[test]
fn right_click_on_canvas_opens_the_picker() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    let center = editor(&mut harness).canvas_center_on_screen();
    secondary_click_at(&mut harness, center);
    assert_eq!(editor(&mut harness).picker_origin(), Some(&Origin::Canvas));
    harness.key_press(eframe::egui::Key::Escape);
    harness.run();
    assert!(!editor(&mut harness).picker_is_open(), "Escape closes it");
}

#[test]
fn right_click_on_an_open_port_adds_a_producer_of_its_item() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    let plate = add_from_browser(&mut harness, "Iron Plate");
    let pin = editor(&mut harness)
        .port_on_screen(plate, Side::Input, "iron_ingot")
        .expect("pin is drawn");
    secondary_click_at(&mut harness, pin);
    assert_eq!(
        editor(&mut harness).picker_origin(),
        Some(&Origin::Port {
            node: plate,
            side: Side::Input,
            item: "iron_ingot".into()
        })
    );
}

#[test]
fn dragging_a_link_onto_empty_canvas_picks_and_links_in_one_click() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    let plate = add_from_browser(&mut harness, "Iron Plate");
    let pin = editor(&mut harness)
        .port_on_screen(plate, Side::Input, "iron_ingot")
        .expect("pin is drawn");
    let drop = pin - eframe::egui::vec2(160.0, 0.0);
    harness.hover_at(pin);
    harness.step();
    harness.drag_at(pin);
    harness.step();
    for step in 1..=8 {
        harness.hover_at(pin.lerp(drop, step as f32 / 8.0));
        harness.step();
    }
    harness.drop_at(drop);
    harness.run();
    assert!(
        matches!(
            editor(&mut harness).picker_origin(),
            Some(Origin::Port {
                side: Side::Input,
                ..
            })
        ),
        "dropping the link opens the picker from its port"
    );
    harness.snapshot("picker_from_port");

    harness
        .get_all_by_label("Iron Ingot")
        .last()
        .expect("offered")
        .click();
    harness.run();
    let smelter = selected(&mut harness);
    let ws = editor(&mut harness);
    let graph = ws.document().graph();
    let (_, link) = graph.links().iter().next().expect("linked");
    assert_eq!((link.from, link.to), (smelter, plate));
    // The new node feeds the port, so it lands to the left of it.
    let smelter_out = ws
        .port_on_screen(smelter, Side::Output, "iron_ingot")
        .expect("pin is drawn");
    assert!(
        (smelter_out.x - drop.x).abs() < 20.0,
        "its output side sits at the drop point: {} vs {}",
        smelter_out.x,
        drop.x
    );
    snapshot_graph(&mut harness, "added_from_port");
}

fn add(harness: &mut Harness<'_, App>, kind: NodeKind, x: f32, y: f32) -> NodeId {
    editor(harness)
        .document_mut()
        .add_node(kind, eframe::egui::pos2(x, y))
}

fn link_any(harness: &mut Harness<'_, App>, from: PortRef, to: PortRef) {
    let ws = editor(harness);
    ws.document_mut().connect(from, to).expect("link is valid");
}

// A source too small for the smelter's limit throttles it (marker on its input). The ingots pass
// a control port into storage, which fills because the sink takes less.
#[test]
fn logistics_nodes_and_a_throttled_node() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    let ore = add(
        &mut harness,
        NodeKind::Source {
            item: "iron_ore".into(),
            limit: 30.0,
        },
        0.0,
        0.0,
    );
    let smelter = add(
        &mut harness,
        NodeKind::Recipe {
            recipe: "iron_ingot".into(),
            facility: "smelter".into(),
            limit: Some(2.0),
        },
        420.0,
        0.0,
    );
    let valve = add(
        &mut harness,
        NodeKind::ControlPort {
            item: None,
            limit: 40.0,
        },
        840.0,
        0.0,
    );
    let storage = add(&mut harness, NodeKind::Storage { item: None }, 1260.0, 0.0);
    let goal = add(
        &mut harness,
        NodeKind::Sink {
            item: "iron_ingot".into(),
            limit: Some(10.0),
        },
        1680.0,
        0.0,
    );
    link_any(
        &mut harness,
        PortRef::new(ore, "iron_ore"),
        PortRef::new(smelter, "iron_ore"),
    );
    link_any(
        &mut harness,
        PortRef::new(smelter, "iron_ingot"),
        PortRef::any(valve),
    );
    link_any(&mut harness, PortRef::any(valve), PortRef::any(storage));
    link_any(
        &mut harness,
        PortRef::any(storage),
        PortRef::new(goal, "iron_ingot"),
    );
    harness.run();

    let s = editor(&mut harness).document().solution().clone();
    assert_near(count(&s, smelter), 1.0);
    assert_eq!(
        s.throttle[&smelter].side,
        Side::Input,
        "the smelter is short of ore"
    );
    assert_near(s.storage[&storage].balance, 20.0);
    fit_view(&mut harness);
    snapshot_graph(&mut harness, "logistics_and_throttle");
}

#[test]
fn going_over_a_raw_resource_shows_a_warning() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    add(
        &mut harness,
        NodeKind::Recipe {
            recipe: "iron_ore".into(),
            facility: "miner".into(),
            limit: Some(1.0),
        },
        0.0,
        0.0,
    );
    harness.run();
    harness.get_by_label("Settings…").click();
    harness.run();
    let role = eframe::egui::accesskit::Role::TextInput;
    harness.get_by_role_and_label(role, "Iron Ore").focus();
    harness.run();
    harness
        .get_by_role_and_label(role, "Iron Ore")
        .type_text("45");
    harness.run();

    let ws = editor(&mut harness);
    assert_eq!(
        ws.availability().get(&"tundra".into(), &"iron_ore".into()),
        Some(45.0)
    );
    harness.get_by_label("Iron Ore: using 60 /min, but the region supplies 45 /min.");
    harness.snapshot("raw_resource_over_use");
}

fn limit_of(harness: &mut Harness<'_, App>, node: NodeId) -> Option<f64> {
    let ws = editor(harness);
    ws.document()
        .graph()
        .node(node)
        .and_then(|n| n.kind.limit())
}

/// Focuses the text field a screen reader knows as `label` and types `text` into it.
fn type_into(harness: &mut Harness<'_, App>, label: &str, text: &str) {
    let role = eframe::egui::accesskit::Role::TextInput;
    harness.get_by_role_and_label(role, label).focus();
    harness.run();
    harness.get_by_role_and_label(role, label).type_text(text);
    harness.run();
}

/// Both clicks go into one frame: the harness gives each queued event its own frame of 0.25 s,
/// longer than egui's double-click window.
fn double_click_at(harness: &mut Harness<'_, App>, pos: eframe::egui::Pos2) {
    harness.hover_at(pos);
    harness.step();
    for pressed in [true, false, true, false] {
        harness
            .input_mut()
            .events
            .push(eframe::egui::Event::PointerButton {
                pos,
                button: eframe::egui::PointerButton::Primary,
                pressed,
                modifiers: eframe::egui::Modifiers::NONE,
            });
    }
    harness.run();
}

#[test]
fn the_facility_count_limit_is_typed_on_the_card() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    let frame = add_from_browser(&mut harness, "Modular Frame");
    type_into(&mut harness, "Node limit", "1.25");
    assert_eq!(limit_of(&mut harness, frame), Some(1.25));
    let s = editor(&mut harness).document().solution().clone();
    assert_near(count(&s, frame), 1.25);
    snapshot_graph(&mut harness, "limit_on_card");
}

#[test]
fn double_clicking_a_port_sets_the_limit_as_a_rate() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    // Iron Plate: 20 plate per minute per facility.
    let plate = add_from_browser(&mut harness, "Iron Plate");
    let pin = editor(&mut harness)
        .port_on_screen(plate, Side::Output, "iron_plate")
        .expect("pin is drawn");
    let at = beside_output_pin(&mut harness, pin);
    double_click_at(&mut harness, at);
    assert!(
        editor(&mut harness).port_menu_is_open(),
        "double-click opens the port menu"
    );
    type_into(&mut harness, "Port limit", "60");
    assert_eq!(limit_of(&mut harness, plate), Some(3.0));
    snapshot_graph(&mut harness, "port_menu");
    harness.key_press(eframe::egui::Key::Escape);
    harness.run();
    // The card shows the same limit as a facility count.
    harness.get_by_role_and_label(eframe::egui::accesskit::Role::TextInput, "Node limit");
    assert_eq!(
        harness
            .get_by_role_and_label(eframe::egui::accesskit::Role::TextInput, "Node limit")
            .value()
            .as_deref(),
        Some("3")
    );
}

/// One wheel event at `pos`, in mouse-wheel lines or trackpad points, played out.
fn scroll_at(
    harness: &mut Harness<'_, App>,
    pos: eframe::egui::Pos2,
    unit: eframe::egui::MouseWheelUnit,
    delta: eframe::egui::Vec2,
) {
    harness.hover_at(pos);
    harness.step();
    harness
        .input_mut()
        .events
        .push(eframe::egui::Event::MouseWheel {
            unit,
            delta,
            phase: eframe::egui::TouchPhase::Move,
            modifiers: eframe::egui::Modifiers::NONE,
        });
    // egui spreads a wheel's scroll over a few frames.
    harness.run_steps(30);
}

/// Whether `zoom` is a whole step of 5%.
fn on_a_5_percent_step(zoom: f32) -> bool {
    let steps = zoom * 20.0;
    (steps - steps.round()).abs() < 1e-3
}

/// The graph point under `pos` on screen.
fn graph_point(harness: &mut Harness<'_, App>, pos: eframe::egui::Pos2) -> eframe::egui::Pos2 {
    let view = editor(harness).canvas_view().expect("canvas is shown");
    let corner = editor(harness).canvas_corner_on_screen();
    eframe::egui::pos2(
        view.x + (pos.x - corner.x) / view.zoom,
        view.y + (pos.y - corner.y) / view.zoom,
    )
}

#[test]
fn the_mouse_wheel_zooms_the_canvas_around_the_pointer() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    add_from_browser(&mut harness, "Iron Plate");
    let at = editor(&mut harness).canvas_center_on_screen() + eframe::egui::vec2(100.0, 50.0);
    let before = editor(&mut harness).canvas_view().expect("canvas is shown");
    let under = graph_point(&mut harness, at);
    // Wheel away from the player: zoom in.
    scroll_at(
        &mut harness,
        at,
        eframe::egui::MouseWheelUnit::Line,
        eframe::egui::vec2(0.0, 1.0),
    );
    let after = editor(&mut harness).canvas_view().expect("canvas is shown");
    assert!(after.zoom > before.zoom, "{} > {}", after.zoom, before.zoom);
    assert!(on_a_5_percent_step(after.zoom), "zoom is {}", after.zoom);
    let moved = graph_point(&mut harness, at) - under;
    assert!(
        moved.length() < 0.5,
        "the point under the pointer stays: {moved:?}"
    );
    scroll_at(
        &mut harness,
        at,
        eframe::egui::MouseWheelUnit::Line,
        eframe::egui::vec2(0.0, -2.0),
    );
    let out = editor(&mut harness).canvas_view().expect("canvas is shown");
    assert!(out.zoom < before.zoom, "{} < {}", out.zoom, before.zoom);
    assert!(on_a_5_percent_step(out.zoom), "zoom is {}", out.zoom);
}

#[test]
fn the_canvas_sits_on_whole_pixels_so_its_text_is_sharp() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    add_from_browser(&mut harness, "Iron Plate");
    // A zoom around a point between pixels would leave the canvas between pixels too.
    let at = editor(&mut harness).canvas_center_on_screen() + eframe::egui::vec2(100.3, 50.7);
    scroll_at(
        &mut harness,
        at,
        eframe::egui::MouseWheelUnit::Line,
        eframe::egui::vec2(0.0, 1.0),
    );
    let ppp = harness.ctx.pixels_per_point();
    let view = editor(&mut harness).canvas_view().expect("canvas is shown");
    let corner = editor(&mut harness).canvas_corner_on_screen();
    // Screen = zoom × graph + offset; the view's point is at the canvas's corner.
    let offset = corner.to_vec2() - eframe::egui::vec2(view.x, view.y) * view.zoom;
    for pixels in [offset.x * ppp, offset.y * ppp] {
        assert!((pixels - pixels.round()).abs() < 0.01, "offset {offset:?}");
    }
}

#[test]
fn small_scrolls_add_up_to_5_percent_steps() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    add_from_browser(&mut harness, "Iron Plate");
    let at = editor(&mut harness).canvas_center_on_screen();
    let before = editor(&mut harness).canvas_view().expect("canvas is shown");
    // Each is well under a 5% step on its own.
    for _ in 0..12 {
        scroll_at(
            &mut harness,
            at,
            eframe::egui::MouseWheelUnit::Point,
            eframe::egui::vec2(0.0, 2.0),
        );
        let zoom = editor(&mut harness)
            .canvas_view()
            .expect("canvas is shown")
            .zoom;
        assert!(on_a_5_percent_step(zoom), "zoom is {zoom}");
    }
    let after = editor(&mut harness).canvas_view().expect("canvas is shown");
    assert!(after.zoom > before.zoom, "{} > {}", after.zoom, before.zoom);
}

#[test]
fn the_status_bar_shows_the_zoom_and_sets_it() {
    use eframe::egui::accesskit::Role;
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    add_from_browser(&mut harness, "Iron Plate");
    let zoom = |harness: &mut Harness<'_, App>| {
        editor(harness).canvas_view().expect("canvas is shown").zoom
    };
    let shown = |harness: &mut Harness<'_, App>| {
        let percent = format!("{}%", (zoom(harness) * 100.0).round());
        harness.get_by_label(&percent);
    };
    shown(&mut harness);
    let center = editor(&mut harness).canvas_center_on_screen();
    let under = graph_point(&mut harness, center);

    // − and + step through 25%, 50%, … 200%.
    harness
        .get_by_role_and_label(Role::Button, "Zoom out")
        .click();
    harness.run();
    let out = zoom(&mut harness);
    harness
        .get_by_role_and_label(Role::Button, "Zoom out")
        .click();
    harness.run();
    assert!(
        (out - zoom(&mut harness) - 0.25).abs() < 1e-4,
        "{out} then {}",
        zoom(&mut harness)
    );
    harness
        .get_by_role_and_label(Role::Button, "Zoom in")
        .click();
    harness.run();
    assert!((zoom(&mut harness) - out).abs() < 1e-4);
    shown(&mut harness);

    // The slider runs from 20% to 200% in steps of 5%; its middle is 110%. It stays put while
    // the percentage beside it changes width.
    let slider = harness.get_by_role_and_label(Role::Slider, "Zoom").rect();
    harness.get_by_role_and_label(Role::Slider, "Zoom").click();
    harness.run();
    let middle = zoom(&mut harness);
    assert!((middle - 1.1).abs() < 1e-4, "zoom is {middle}");
    shown(&mut harness);
    assert_eq!(
        harness.get_by_role_and_label(Role::Slider, "Zoom").rect(),
        slider
    );
    // Each zoom snaps the canvas to whole pixels, so after four the middle may be a pixel off.
    let moved = (graph_point(&mut harness, center) - under) * middle;
    assert!(
        moved.length() < 2.0,
        "the middle of the canvas stays: {moved:?} on screen"
    );
}

#[test]
fn a_scroll_in_pixels_zooms_too() {
    // A Magic Mouse, or a wheel with smooth scrolling, reports points rather than lines.
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    add_from_browser(&mut harness, "Iron Plate");
    let at = editor(&mut harness).canvas_center_on_screen();
    let before = editor(&mut harness).canvas_view().expect("canvas is shown");
    scroll_at(
        &mut harness,
        at,
        eframe::egui::MouseWheelUnit::Point,
        eframe::egui::vec2(0.0, 40.0),
    );
    let after = editor(&mut harness).canvas_view().expect("canvas is shown");
    assert!(after.zoom > before.zoom, "{} > {}", after.zoom, before.zoom);
}

#[test]
fn double_clicking_a_ports_icon_opens_its_menu() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    let plate = add_from_browser(&mut harness, "Iron Plate");
    let pin = editor(&mut harness)
        .port_on_screen(plate, Side::Output, "iron_plate")
        .expect("pin is drawn");
    double_click_at(&mut harness, pin);
    assert!(
        editor(&mut harness).port_menu_is_open(),
        "double-click on the icon opens the port menu"
    );
    assert!(
        editor(&mut harness).document().graph().links().is_empty(),
        "no link starts from the double-click"
    );
}

#[test]
fn the_port_menu_clears_the_ports_links() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    let plate = add_from_browser(&mut harness, "Iron Plate");
    let smelter = pick(
        &mut harness,
        plate,
        Side::Input,
        "iron_ingot",
        (-80.0, 0.0),
        "Iron Ingot",
    );
    assert_eq!(editor(&mut harness).document().graph().links().len(), 1);
    let pin = editor(&mut harness)
        .port_on_screen(smelter, Side::Output, "iron_ingot")
        .expect("pin is drawn");
    let at = beside_output_pin(&mut harness, pin);
    double_click_at(&mut harness, at);
    assert!(
        editor(&mut harness).port_menu_is_open(),
        "double-click opens the port menu"
    );
    harness.get_by_label("Clear connections").click();
    harness.run();
    assert_eq!(editor(&mut harness).document().graph().links().len(), 0);
}

#[test]
fn inspector_starts_closed() {
    let catalog = aic_data::load(&aic_editor::testing::fixture_dir(), "en").expect("data loads");
    let mut harness = Harness::builder()
        .with_size(WINDOW_SIZE)
        .build_eframe(|cc| {
            aic_app_egui::theme::install(&cc.egui_ctx);
            App::new(Strings::english(), Ok(catalog), None)
        });
    harness.run();
    assert!(harness.query_by_label("Totals").is_none());
}

#[test]
fn side_panels_collapse() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    harness.get_by_label("Modular Frame");
    harness
        .get_by_role_and_label(eframe::egui::accesskit::Role::Button, "Recipes")
        .click();
    harness
        .get_by_role_and_label(eframe::egui::accesskit::Role::Button, "Inspector")
        .click();
    harness.run();
    assert!(
        harness.query_by_label("Modular Frame").is_none(),
        "browser hidden"
    );
    assert!(
        harness.query_by_label("Totals").is_none(),
        "inspector hidden"
    );
    harness.snapshot("panels_hidden");
}

// Icons from data/v1.5.3/images: item icons on the picker's rows, the depot pictures on Source and
// the facility picture on the card.
#[test]
fn icons_from_the_data_folder() {
    let mut harness = harness(&fixture("../../data/v1.5.3"));
    editor(&mut harness).new_graph("jinlong".into());
    harness.run();
    let planting = add_from_browser(&mut harness, "Yazhen");
    let at = {
        let pos = editor(&mut harness)
            .document()
            .graph()
            .node(planting)
            .unwrap()
            .pos;
        eframe::egui::pos2(pos.x - 80.0, pos.y)
    };
    editor(&mut harness).open_picker(
        Origin::Port {
            node: planting,
            side: Side::Input,
            item: "liquid_water".into(),
        },
        at,
    );
    harness.run();
    harness.snapshot("icons_picker");
}

// M6: editing, files and the round-up view.

use eframe::egui::{Event, Key, Modifiers, PointerButton, Pos2, pos2, vec2};

fn smelter(harness: &mut Harness<'_, App>, x: f32, y: f32) -> NodeId {
    add(
        harness,
        NodeKind::Recipe {
            recipe: "iron_ingot".into(),
            facility: "smelter".into(),
            limit: Some(1.0),
        },
        x,
        y,
    )
}

fn node_count(harness: &mut Harness<'_, App>) -> usize {
    editor(harness).document().graph().nodes().len()
}

fn node_pos(harness: &mut Harness<'_, App>, node: NodeId) -> Pos2 {
    let pos = editor(harness)
        .document()
        .graph()
        .node(node)
        .expect("node exists")
        .pos;
    pos2(pos.x, pos.y)
}

fn selected_nodes(harness: &mut Harness<'_, App>) -> Vec<NodeId> {
    let mut nodes = editor(harness)
        .selection()
        .map(|s| s.nodes().to_vec())
        .unwrap_or_default();
    nodes.sort();
    nodes
}

/// A pointer drag from `from` to `to` on screen with `modifiers` held, in a few steps.
fn drag(harness: &mut Harness<'_, App>, from: Pos2, to: Pos2, modifiers: Modifiers) {
    harness.hover_at(from);
    harness.run();
    harness.event(Event::ModifiersChanged(modifiers));
    harness.event(Event::PointerButton {
        pos: from,
        button: PointerButton::Primary,
        pressed: true,
        modifiers,
    });
    for step in 1..=4 {
        harness.event(Event::PointerMoved(from.lerp(to, step as f32 / 4.0)));
    }
    harness.event(Event::PointerButton {
        pos: to,
        button: PointerButton::Primary,
        pressed: false,
        modifiers,
    });
    harness.event(Event::ModifiersChanged(Modifiers::NONE));
    harness.run();
}

fn node_rect(harness: &mut Harness<'_, App>, node: NodeId) -> eframe::egui::Rect {
    editor(harness).node_on_screen(node).expect("node is drawn")
}

#[test]
fn delete_a_selection_then_undo_and_redo_it() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    smelter(&mut harness, 0.0, 0.0);
    smelter(&mut harness, 300.0, 0.0);
    harness.run();

    harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
    harness.run();
    assert_eq!(selected_nodes(&mut harness).len(), 2);
    harness.key_press(Key::Delete);
    harness.run();
    assert_eq!(node_count(&mut harness), 0);

    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    harness.run();
    assert_eq!(node_count(&mut harness), 2);
    harness.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    harness.run();
    assert_eq!(node_count(&mut harness), 0);
}

#[test]
fn shift_drag_on_the_canvas_selects_a_box_of_nodes() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    let a = smelter(&mut harness, 0.0, 0.0);
    let b = smelter(&mut harness, 300.0, 0.0);
    let far = smelter(&mut harness, 300.0, 400.0);
    harness.run();
    fit_view(&mut harness);
    let (ra, rb, rfar) = (
        node_rect(&mut harness, a),
        node_rect(&mut harness, b),
        node_rect(&mut harness, far),
    );
    let top_left = ra.min - vec2(20.0, 20.0);
    let bottom_right = pos2(rb.max.x + 20.0, (rb.max.y + rfar.min.y) / 2.0);
    drag(&mut harness, top_left, bottom_right, Modifiers::SHIFT);

    let mut expected = vec![a, b];
    expected.sort();
    assert_eq!(selected_nodes(&mut harness), expected);
}

#[test]
fn dragging_a_selected_node_moves_the_whole_selection() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    let a = smelter(&mut harness, 0.0, 0.0);
    let b = smelter(&mut harness, 300.0, 0.0);
    let alone = smelter(&mut harness, 0.0, 300.0);
    harness.run();
    fit_view(&mut harness);
    editor(&mut harness).select_nodes(vec![a, b]);
    harness.run();
    let (a0, b0, alone0) = (
        node_pos(&mut harness, a),
        node_pos(&mut harness, b),
        node_pos(&mut harness, alone),
    );
    let header = node_rect(&mut harness, a).center_top() + vec2(0.0, 6.0);
    drag(
        &mut harness,
        header,
        header + vec2(100.0, 50.0),
        Modifiers::NONE,
    );

    let moved = node_pos(&mut harness, a) - a0;
    assert!(moved.length() > 10.0, "the grabbed node moves: {moved:?}");
    let followed = node_pos(&mut harness, b) - b0;
    assert!(
        (followed - moved).length() < 1e-3,
        "the other selected node follows: {followed:?} vs {moved:?}"
    );
    assert_eq!(node_pos(&mut harness, alone), alone0);

    // One drag is one undo step.
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    harness.run();
    assert_eq!(node_pos(&mut harness, a), a0);
    assert_eq!(node_pos(&mut harness, b), b0);
}

#[test]
fn copy_and_paste_keep_links_inside_the_copy() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    let ingot = smelter(&mut harness, 0.0, 0.0);
    let plate = add(
        &mut harness,
        NodeKind::Recipe {
            recipe: "iron_plate".into(),
            facility: "constructor".into(),
            limit: None,
        },
        300.0,
        0.0,
    );
    link(&mut harness, ingot, plate, "iron_ingot");
    editor(&mut harness).select_nodes(vec![ingot, plate]);
    harness.run();

    // The window sends clipboard events rather than Cmd+C and Cmd+V key presses.
    harness.event(Event::Copy);
    harness.run();
    harness.event(Event::Paste("some text".into()));
    harness.run();
    assert_eq!(node_count(&mut harness), 4);
    assert_eq!(editor(&mut harness).document().graph().links().len(), 2);
    assert_eq!(
        selected_nodes(&mut harness).len(),
        2,
        "the copy is selected"
    );

    harness.key_press_modifiers(Modifiers::COMMAND, Key::D);
    harness.run();
    assert_eq!(node_count(&mut harness), 6);
}

#[test]
fn save_then_open_the_file_again() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    let path = std::env::temp_dir().join(format!("aic-save-{}.aicm", std::process::id()));
    smelter(&mut harness, 0.0, 0.0);
    harness.run();
    assert!(editor(&mut harness).document().is_modified());
    assert!(editor(&mut harness).title().starts_with("Untitled •"));

    editor(&mut harness).save_to(&path);
    harness.run();
    assert!(!editor(&mut harness).document().is_modified());
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    assert!(editor(&mut harness).title().starts_with(&name));
    assert_eq!(
        editor(&mut harness).recent_files(),
        std::slice::from_ref(&path)
    );

    editor(&mut harness).new_graph("tundra".into());
    harness.run();
    assert_eq!(node_count(&mut harness), 0);
    editor(&mut harness).open_file(&path);
    harness.run();
    assert_eq!(node_count(&mut harness), 1);
    assert_eq!(editor(&mut harness).document().path(), Some(path.as_path()));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn round_up_shows_whole_facilities_and_how_busy_they_are() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    let frame = add_from_browser(&mut harness, "Modular Frame");
    type_into(&mut harness, "Node limit", "1.25");
    editor(&mut harness).set_round_up(true);
    harness.run();
    harness.get_by_label("2");
    harness.get_by_label("62.5% busy");
    let _ = frame;
    snapshot_graph(&mut harness, "round_up");
}

// M7: data versions, overlays and languages.

fn data_folder_app() -> Harness<'static, App> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../aic-data/tests/fixtures");
    let folder = aic_app_egui::data::DataFolder::at(dir).map_err(|e| e.to_string());
    let mut harness = Harness::builder()
        .with_size(WINDOW_SIZE)
        .build_eframe(move |cc| {
            aic_app_egui::theme::install(&cc.egui_ctx);
            App::from_folder(Strings::english(), folder.clone(), None)
        });
    hide_performance(&mut harness);
    harness.run();
    harness
}

#[test]
fn switching_data_versions_keeps_the_graph_and_matches_recipes_by_id() {
    let mut harness = data_folder_app();
    assert_eq!(editor(&mut harness).catalog().version(), "v1.10.0");
    let nugget = add(
        &mut harness,
        NodeKind::Recipe {
            recipe: "copper_nugget".into(),
            facility: "furnance_1".into(),
            limit: Some(1.0),
        },
        0.0,
        0.0,
    );
    harness.run();
    editor(&mut harness)
        .settings_mut()
        .open_on(&"jinlong".into());
    harness.run();
    harness.snapshot("settings_data");

    let choice = aic_app_egui::data::DataChoice {
        version: Some("v1.9.0".into()),
        language: None,
    };
    editor(&mut harness).switch_data(choice.clone());
    harness.run();
    let ws = editor(&mut harness);
    assert_eq!(ws.catalog().version(), "v1.9.0");
    assert_eq!(ws.data_choice(), Some(&choice));
    assert!(matches!(
        ws.document().graph().node(nugget).map(|n| &n.kind),
        Some(NodeKind::MissingRecipe { .. })
    ));
}

#[test]
fn an_outpost_order_from_an_output_port_earns_coupons() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    let plate = add_from_browser(&mut harness, "Iron Plate");
    type_into(&mut harness, "Node limit", "1");
    let order = pick(
        &mut harness,
        plate,
        Side::Output,
        "iron_plate",
        (400.0, 0.0),
        "Coupon (Iron Plate)",
    );
    let ws = editor(&mut harness);
    assert!(matches!(
        ws.document().graph().node(order).map(|n| &n.kind),
        Some(NodeKind::OutpostOrder { .. })
    ));
    let s = ws.document().solution();
    assert_near(s.rate.get(&order).copied().flatten(), 20.0);
    assert_eq!(s.totals.coupons[&aic_data::ItemId::from("coupon")], 20.0);
    fit_view(&mut harness);
    snapshot_graph(&mut harness, "outpost_order");
}

#[test]
fn a_ratio_lock_is_typed_in_the_port_menu() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    let ore = add(
        &mut harness,
        NodeKind::Source {
            item: "iron_ore".into(),
            limit: 60.0,
        },
        0.0,
        0.0,
    );
    let goal = add(
        &mut harness,
        NodeKind::Sink {
            item: "iron_ore".into(),
            limit: None,
        },
        300.0,
        -120.0,
    );
    let smelter = add(
        &mut harness,
        NodeKind::Recipe {
            recipe: "iron_ingot".into(),
            facility: "smelter".into(),
            limit: None,
        },
        300.0,
        120.0,
    );
    link_any(&mut harness, PortRef::any(ore), PortRef::any(goal));
    link_any(
        &mut harness,
        PortRef::any(ore),
        PortRef::new(smelter, "iron_ore"),
    );
    harness.run();
    fit_view(&mut harness);
    let pin = editor(&mut harness)
        .port_on_screen(ore, Side::Output, "iron_ore")
        .expect("pin is drawn");
    let at = beside_output_pin(&mut harness, pin);
    double_click_at(&mut harness, at);
    assert!(editor(&mut harness).port_menu_is_open());

    type_into(&mut harness, "Share for Sink · Iron Ore", "2");
    type_into(&mut harness, "Share for Iron Ingot", "1");
    let s = editor(&mut harness).document().solution().clone();
    assert_near(flow(&s, goal, "iron_ore", Side::Input), 40.0);
    assert_near(flow(&s, smelter, "iron_ore", Side::Input), 20.0);
    snapshot_graph(&mut harness, "ratio_lock");

    harness.get_by_label("Unlock: split evenly").click();
    harness.run();
    let graph = editor(&mut harness).document().graph().clone();
    assert!(graph.links().values().all(|l| l.out_share.is_none()));
    // Unlocked, the sink takes everything: a sink's intake counts as throughput (ADR 0004).
    let s = editor(&mut harness).document().solution().clone();
    assert_near(flow(&s, goal, "iron_ore", Side::Input), 60.0);
}

/// ore source → smelter → constructor → plate sink, at the top of the graph.
fn plate_chain(harness: &mut Harness<'_, App>) -> [NodeId; 4] {
    let ore = add(
        harness,
        NodeKind::Source {
            item: "iron_ore".into(),
            limit: 60.0,
        },
        0.0,
        0.0,
    );
    let ingot = smelter(harness, 400.0, 0.0);
    let plate = add(
        harness,
        NodeKind::Recipe {
            recipe: "iron_plate".into(),
            facility: "constructor".into(),
            limit: None,
        },
        800.0,
        0.0,
    );
    let goal = add(
        harness,
        NodeKind::Sink {
            item: "iron_plate".into(),
            limit: None,
        },
        1250.0,
        0.0,
    );
    link_any(harness, PortRef::any(ore), PortRef::new(ingot, "iron_ore"));
    link(harness, ingot, plate, "iron_ingot");
    link_any(
        harness,
        PortRef::new(plate, "iron_plate"),
        PortRef::any(goal),
    );
    [ore, ingot, plate, goal]
}

#[test]
fn a_workspace_holds_its_nodes_and_opens_to_show_them_between_its_ports() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    let [_, ingot, plate, _] = plate_chain(&mut harness);
    let before = editor(&mut harness).document().solution().clone();
    editor(&mut harness).select_nodes(vec![ingot, plate]);
    harness.run();

    harness.key_press_modifiers(Modifiers::COMMAND, Key::G);
    harness.run();
    let ed = editor(&mut harness);
    let workspace = ed
        .selection()
        .and_then(Selection::single_workspace)
        .expect("the new workspace is selected");
    assert_eq!(
        ed.document().view().nodes().count(),
        3,
        "two nodes became one card"
    );
    assert_eq!(ed.document().view().wires().count(), 2);
    // A workspace changes nothing for the solver.
    let after = ed.document().solution().clone();
    for node in [ingot, plate] {
        assert_near(count(&after, node), count(&before, node).unwrap());
    }
    let port_out = ed.document().graph().ports_of(workspace, Side::Output)[0];
    fit_view(&mut harness);
    harness.get_by_label("Workspace 1");
    snapshot_graph(&mut harness, "workspace_card");
    assert!(
        editor(&mut harness)
            .workspace_on_screen(workspace)
            .is_some()
    );
    assert!(
        editor(&mut harness)
            .port_on_screen(port_out, Side::Output, "iron_plate")
            .is_some(),
        "the card shows the workspace's port"
    );

    // The inspector's button; kittest can't click inside the zoomed canvas layer.
    harness
        .get_all_by_label("Open")
        .find(|node| node.rect().left() > 980.0)
        .expect("the inspector offers it")
        .click();
    harness.run();
    let ed = editor(&mut harness);
    assert_eq!(ed.document().level(), Some(workspace));
    assert_eq!(
        ed.document().view().nodes().count(),
        4,
        "the two nodes and the two ports"
    );
    assert_eq!(ed.document().view().wires().count(), 3);
    fit_view(&mut harness);
    // That snapshot was recorded with no lit button.
    editor(&mut harness).end_flash();
    harness.run();
    snapshot_graph(&mut harness, "workspace_inside");

    // Panning moves the nodes, not the port tiles: they hold still in the same frame, numbers
    // and all.
    let port_in = editor(&mut harness)
        .document()
        .graph()
        .ports_of(workspace, Side::Input)[0];
    let before = node_rect(&mut harness, port_in);
    let tile = |harness: &mut Harness<'_, App>| {
        let image = harness.render().expect("renders");
        let r = before.shrink(2.0);
        image::imageops::crop_imm(
            &image,
            r.left() as u32,
            r.top() as u32,
            r.width() as u32,
            r.height() as u32,
        )
        .to_image()
    };
    let still = tile(&mut harness);
    // A scroll zooms; a middle-button drag on the background pans.
    let mut at = editor(&mut harness).canvas_center_on_screen() + vec2(0.0, 200.0);
    harness.hover_at(at);
    harness.step();
    harness.event(Event::PointerButton {
        pos: at,
        button: eframe::egui::PointerButton::Middle,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    harness.step();
    let view = editor(&mut harness).canvas_view().expect("canvas is shown");
    for _ in 0..3 {
        at += vec2(40.0, 25.0);
        harness.event(Event::PointerMoved(at));
        harness.step();
        let now = node_rect(&mut harness, port_in);
        assert!(
            (now.min - before.min).length() < 0.5,
            "the tile moved from {before:?} to {now:?}"
        );
        // A link passing behind may differ; the number may not move.
        let differ = tile(&mut harness)
            .pixels()
            .zip(still.pixels())
            .filter(|(a, b)| a != b)
            .count();
        assert!(differ < 40, "{differ} pixels of the tile changed");
    }
    let panned = editor(&mut harness).canvas_view().expect("canvas is shown");
    assert!(
        (panned.x - view.x).abs() > 1.0,
        "the drag panned: {view:?} to {panned:?}"
    );
    harness.event(Event::PointerButton {
        pos: at,
        button: eframe::egui::PointerButton::Middle,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    harness.run();

    harness.get_by_label("‹ Back").click();
    harness.run();
    assert_eq!(editor(&mut harness).document().level(), None);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    harness.run();
    assert_eq!(
        editor(&mut harness).document().view().nodes().count(),
        4,
        "undo takes the workspace apart again"
    );
}

#[test]
fn a_new_port_starts_without_an_item_and_an_unlinked_port_warns() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    let [ore, ingot, plate, _] = plate_chain(&mut harness);
    editor(&mut harness).select_nodes(vec![ingot, plate]);
    harness.run();
    harness.key_press_modifiers(Modifiers::COMMAND, Key::G);
    harness.run();
    let workspace = editor(&mut harness)
        .selection()
        .and_then(Selection::single_workspace)
        .unwrap();
    editor(&mut harness).document_mut().remove_node(ore);
    editor(&mut harness).show_level(Some(workspace));
    harness.run();
    harness.get_by_label("+ Input").click();
    harness.run();
    let ed = editor(&mut harness);
    let ports = ed.document().graph().ports_of(workspace, Side::Input);
    assert_eq!(ports.len(), 2);
    assert_eq!(
        ed.document().graph().node(ports[1]).unwrap().kind.item(),
        None
    );
    let warnings = &ed.document().solution().warnings;
    for port in &ports {
        assert!(
            warnings.contains(&aic_core::Warning::PortUnlinked {
                port: *port,
                outside: true
            }),
            "{warnings:?}"
        );
    }
    fit_view(&mut harness);
    snapshot_graph(&mut harness, "workspace_new_port");
}

#[test]
fn dragging_with_the_right_button_selects_a_box_of_nodes() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    let left = smelter(&mut harness, 0.0, 0.0);
    let middle = smelter(&mut harness, 400.0, 0.0);
    let right = smelter(&mut harness, 800.0, 0.0);
    fit_view(&mut harness);
    let a = node_rect(&mut harness, left);
    let b = node_rect(&mut harness, middle);
    let from = a.left_top() - vec2(10.0, 10.0);
    let to = b.center();
    harness.hover_at(from);
    harness.run();
    harness.event(Event::PointerButton {
        pos: from,
        button: PointerButton::Secondary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    for step in 1..=4 {
        harness.event(Event::PointerMoved(from.lerp(to, step as f32 / 4.0)));
    }
    harness.event(Event::PointerButton {
        pos: to,
        button: PointerButton::Secondary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    harness.run();
    let mut selected = selected_nodes(&mut harness);
    selected.sort();
    let mut expected = vec![left, middle];
    expected.sort();
    assert_eq!(selected, expected);
    assert!(!selected.contains(&right));
    assert!(
        !editor(&mut harness).picker_is_open(),
        "a right-drag doesn't open the picker"
    );
}

#[test]
fn export_saves_the_canvas_as_a_png() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    add_from_browser(&mut harness, "Iron Plate");
    let path = std::env::temp_dir().join(format!("aic-export-{}.png", std::process::id()));
    editor(&mut harness).request_export(path.clone());
    harness.run();
    let png = std::fs::read(&path).expect("the export is written");
    let _ = std::fs::remove_file(&path);
    let image = image::load_from_memory(&png).expect("a PNG").to_rgba8();
    // Only the canvas, between the side panels: narrower than the window, and as tall as the
    // space between the top bar and the status bar.
    let (width, height) = image.dimensions();
    assert!((600..WINDOW_SIZE[0] as u32).contains(&width), "{width}");
    assert!((600..WINDOW_SIZE[1] as u32).contains(&height), "{height}");
}

// Save files: drafts, autosave and reopening.

/// An empty folder in the temp directory for one test.
fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("aic-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn files_in(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|entries| entries.map(|e| e.unwrap().path()).collect())
        .unwrap_or_default();
    files.sort();
    files
}

#[test]
fn a_new_graph_autosaves_to_a_draft_and_save_as_moves_it_to_the_players_file() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    let dir = temp_dir("drafts");
    let drafts = dir.join("drafts");
    editor(&mut harness).set_drafts_dir(Some(drafts.clone()));
    harness.run();
    assert!(
        files_in(&drafts).is_empty(),
        "an untouched graph has no draft"
    );

    smelter(&mut harness, 0.0, 0.0);
    harness.run();
    let draft = files_in(&drafts);
    assert_eq!(draft.len(), 1, "the first change saves at once");
    assert!(editor(&mut harness).document().is_draft());
    assert!(editor(&mut harness).title().starts_with("Untitled —"));

    let mine = dir.join("plan.aicm");
    editor(&mut harness).save_to(&mine);
    harness.run();
    assert!(files_in(&drafts).is_empty(), "the draft is gone");
    assert!(!editor(&mut harness).document().is_draft());
    assert_eq!(editor(&mut harness).document().path(), Some(mine.as_path()));

    // New saves the open file first, then starts a fresh draft.
    smelter(&mut harness, 300.0, 0.0);
    editor(&mut harness).request(aic_app_egui::files::Pending::New);
    harness.run();
    let saved = Graph::from_json(
        editor(&mut harness).catalog(),
        &std::fs::read_to_string(&mine).unwrap(),
    )
    .unwrap();
    assert_eq!(saved.graph.nodes().len(), 2);
    assert_eq!(node_count(&mut harness), 0);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn reopening_restores_the_graph_and_where_the_canvas_looked() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    let dir = temp_dir("reopen");
    let path = dir.join("plan.aicm");
    let node = smelter(&mut harness, 120.0, 80.0);
    harness.run();
    fit_view(&mut harness);
    editor(&mut harness).save_to(&path);
    let view = editor(&mut harness)
        .canvas_view()
        .expect("the canvas was drawn");

    let mut again = self::harness(&aic_editor::testing::fixture_dir());
    editor(&mut again).reopen(&path);
    again.run();
    let ws = editor(&mut again);
    assert_eq!(ws.document().graph().nodes().len(), 1);
    let pos = ws.document().graph().nodes().values().next().unwrap().pos;
    assert_eq!((pos.x, pos.y), (120.0, 80.0), "positions are saved");
    let shown = ws.canvas_view().unwrap();
    assert!(
        (shown.zoom - view.zoom).abs() < 1e-4,
        "{shown:?} vs {view:?}"
    );
    assert!((shown.x - view.x).abs() < 0.5 && (shown.y - view.y).abs() < 0.5);
    let _ = node;
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_file_without_a_camera_opened_before_the_first_frame_is_fitted_to_its_nodes() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    let dir = temp_dir("reopen-fit");
    let path = dir.join("far.aicm");
    let node = smelter(&mut harness, 3000.0, 2000.0);
    let text = editor(&mut harness).document().graph().to_json("vtest");
    std::fs::write(&path, text).unwrap();

    // As at startup: the document is reopened before the first frame is drawn.
    let catalog = aic_data::load(&aic_editor::testing::fixture_dir(), "en").unwrap();
    let mut again = Harness::builder()
        .with_size(WINDOW_SIZE)
        .build_eframe(|cc| {
            aic_app_egui::theme::install(&cc.egui_ctx);
            let mut app = App::new(Strings::english(), Ok(catalog), None);
            app.editor_mut().unwrap().reopen(&path);
            app
        });
    again.run();
    let ws = editor(&mut again);
    let id = ws.document().graph().nodes().keys().next().unwrap();
    let on_screen = ws.node_on_screen(id).expect("the node was drawn");
    let window = eframe::egui::Rect::from_min_size(
        eframe::egui::Pos2::ZERO,
        eframe::egui::vec2(WINDOW_SIZE[0], WINDOW_SIZE[1]),
    );
    assert!(window.intersects(on_screen), "{on_screen:?} is off screen");
    let _ = node;
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_missing_or_broken_last_file_starts_a_new_graph_and_is_left_alone() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    let dir = temp_dir("broken");
    editor(&mut harness).set_drafts_dir(Some(dir.join("drafts")));

    editor(&mut harness).reopen(&dir.join("gone.aicm"));
    harness.run();
    harness.get_by_label_contains("is gone. Started a new one.");

    let broken = dir.join("broken.aicm");
    std::fs::write(&broken, "{ not json").unwrap();
    editor(&mut harness).reopen(&broken);
    harness.run();
    harness.get_by_label_contains("Could not open");
    smelter(&mut harness, 0.0, 0.0);
    harness.run();
    assert_eq!(
        std::fs::read_to_string(&broken).unwrap(),
        "{ not json",
        "autosave never writes over a file it couldn't read"
    );
    assert!(editor(&mut harness).document().is_draft());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_region_can_change_with_nodes_on_the_graph() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    harness
        .get_by_role_and_label(eframe::egui::accesskit::Role::ComboBox, "Region")
        .click();
    harness.run();
    harness.get_by_label("Wuling").click();
    harness.run();
    assert_eq!(
        editor(&mut harness).document().graph().region().as_str(),
        "jinlong"
    );
    smelter(&mut harness, 0.0, 0.0);
    harness.run();
    harness
        .get_by_role_and_label(eframe::egui::accesskit::Role::ComboBox, "Region")
        .click();
    harness.run();
    harness.get_by_label("Valley IV").click();
    harness.run();
    let ed = editor(&mut harness);
    assert_eq!(ed.document().graph().region().as_str(), "tundra");
    // A step of its own: undo takes the region back, nodes and all.
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    harness.run();
    let ed = editor(&mut harness);
    assert_eq!(ed.document().graph().region().as_str(), "jinlong");
    assert_eq!(ed.document().graph().nodes().len(), 1);
}

#[test]
fn the_save_state_does_not_move_the_zoom_control() {
    let slider = |harness: &mut Harness<'_, App>| {
        harness
            .get_by_role_and_label(eframe::egui::accesskit::Role::Slider, "Zoom")
            .rect()
    };
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    let dir = temp_dir("save-state-width");
    editor(&mut harness).set_drafts_dir(Some(dir.clone()));
    harness.get_by_label("Nothing to save yet");
    let before = slider(&mut harness);
    smelter(&mut harness, 0.0, 0.0);
    harness.run();
    harness.get_by_label("✔ Saved as a draft");
    assert_eq!(slider(&mut harness), before);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_toolbar_runs_commands_and_lights_up_for_their_keys() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    let dir = temp_dir("toolbar");
    editor(&mut harness).set_drafts_dir(Some(dir.clone()));
    harness.get_by_label("Nothing to save yet");

    let node = smelter(&mut harness, 0.0, 0.0);
    harness.run();
    harness.get_by_label("✔ Saved as a draft");
    editor(&mut harness).select_nodes(vec![node]);
    harness.run();

    harness.get_by_label("Duplicate").click();
    harness.run();
    assert_eq!(node_count(&mut harness), 2);

    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    harness.step();
    assert_eq!(
        editor(&mut harness).flashing(),
        Some(aic_editor::Command::Undo),
        "the key lights its button"
    );
    assert_eq!(node_count(&mut harness), 1);
    harness.snapshot("toolbar");
    let _ = std::fs::remove_dir_all(dir);
}

/// The player's Buckflower graph from real data: a limit on the middle planter settles everything
/// linked to it (ADR 0006).
#[test]
fn the_facility_name_sits_under_the_recipe_title() {
    let mut harness = harness(&fixture("../../data/v1.5.3"));
    let node = add(
        &mut harness,
        NodeKind::Recipe {
            recipe: "plant_moss_seed_1".into(),
            facility: "seedcol_1".into(),
            limit: None,
        },
        450.0,
        250.0,
    );
    harness.run();
    assert!(editor(&mut harness).node_on_screen(node).is_some());
    // Labels on the canvas report rects in the canvas's view space; the two are compared with each
    // other.
    let facility = harness.get_by_label("Seed-Picking Unit").rect();
    let title = harness
        .get_all_by_label("Buckflower Seed")
        .map(|n| n.rect())
        .filter(|rect| rect.bottom() <= facility.top() + 1.0)
        .max_by(|a, b| a.bottom().total_cmp(&b.bottom()))
        .expect("the title is above the facility");
    assert!(
        facility.top() >= title.bottom() - 1.0 && facility.top() < title.bottom() + 8.0,
        "{facility:?} sits right under {title:?}"
    );
    assert!(
        (facility.left() - title.left()).abs() < 1.0,
        "lined up with the title"
    );
    snapshot_graph(&mut harness, "facility_in_header");
}

#[test]
fn card_text_is_drawn_at_200_percent() {
    let mut harness = harness(&fixture("../../data/v1.5.3"));
    add(
        &mut harness,
        NodeKind::Recipe {
            recipe: "plant_moss_seed_1".into(),
            facility: "seedcol_1".into(),
            limit: None,
        },
        460.0,
        330.0,
    );
    harness.run();
    editor(&mut harness).zoom_canvas_to(2.0);
    harness.run();
    let zoom = editor(&mut harness)
        .canvas_view()
        .expect("canvas is shown")
        .zoom;
    assert!((zoom - 2.0).abs() < 1e-4, "zoom is {zoom}");
    harness.snapshot("card_at_200");
}

#[test]
fn a_cards_body_is_centered_whichever_ports_it_has() {
    let mut harness = harness(&fixture("../../data/v1.5.3"));
    let recipe = |recipe: &str, facility: &str| NodeKind::Recipe {
        recipe: recipe.into(),
        facility: facility.into(),
        limit: None,
    };
    let nodes = [
        // Inputs and outputs, outputs only, inputs only.
        add(
            &mut harness,
            recipe("plant_moss_seed_1", "seedcol_1"),
            450.0,
            40.0,
        ),
        add(&mut harness, recipe("liquid_acid", "pump_2"), 450.0, 260.0),
        add(
            &mut harness,
            recipe("liquid_cleaner_1-sewage", "liquid_cleaner_1"),
            450.0,
            480.0,
        ),
    ];
    harness.run();
    snapshot_graph(&mut harness, "centered_bodies");
    // Labels on the canvas report rects in the canvas's view space; the cards are taken there too.
    let fields: Vec<eframe::egui::Rect> = harness
        .get_all_by_role_and_label(eframe::egui::accesskit::Role::TextInput, "Node limit")
        .map(|n| n.rect())
        .collect();
    for node in nodes {
        let card = node_rect(&mut harness, node);
        let view = aic_app_egui::document::VIEW_SCALE;
        let card = eframe::egui::Rect::from_min_max(
            graph_point(&mut harness, card.min) * view,
            graph_point(&mut harness, card.max) * view,
        );
        let field = fields
            .iter()
            .find(|field| card.contains(field.center()))
            .expect("the card has a limit field");
        assert!(
            (field.center().x - card.center().x).abs() < 1.0,
            "{field:?} is centered on {card:?}"
        );
    }
}

#[test]
fn a_limit_in_the_middle_settles_the_buckflower_graph() {
    let mut harness = harness(&fixture("../../data/v1.5.3"));
    let recipe = |recipe: &str, facility: &str, limit: Option<f64>| NodeKind::Recipe {
        recipe: recipe.into(),
        facility: facility.into(),
        limit,
    };
    let picker = add(
        &mut harness,
        recipe("plant_moss_seed_1", "seedcol_1", None),
        0.0,
        80.0,
    );
    let limited = add(
        &mut harness,
        recipe("plant_moss_1", "planter_1", Some(3.0)),
        380.0,
        0.0,
    );
    let free = add(
        &mut harness,
        recipe("plant_moss_1", "planter_1", None),
        380.0,
        240.0,
    );
    let carbon = add(
        &mut harness,
        recipe("carbon_mtl-plant_moss_1", "furnance_1", None),
        760.0,
        110.0,
    );
    link(&mut harness, picker, limited, "plant_moss_seed_1");
    link(&mut harness, limited, carbon, "plant_moss_1");
    link(&mut harness, free, carbon, "plant_moss_1");

    let s = editor(&mut harness).document().solution().clone();
    assert_near(count(&s, limited), 3.0);
    assert_near(count(&s, picker), 1.5);
    assert_near(count(&s, carbon), 3.0);
    assert_near(count(&s, free), 0.0);
    fit_view(&mut harness);
    snapshot_graph(&mut harness, "limit_in_the_middle");
    // Cards are as wide whatever their numbers, limits and names.
    let widths: Vec<f32> = [picker, limited, free, carbon]
        .into_iter()
        .map(|node| node_rect(&mut harness, node).width())
        .collect();
    assert!(
        widths.iter().all(|w| (w - widths[0]).abs() < 0.5),
        "card widths differ: {widths:?}"
    );
}

#[test]
fn link_labels_show_over_their_links() {
    let mut harness = harness(&fixture("../../data/v1.5.3"));
    let recipe = |recipe: &str, facility: &str, limit: Option<f64>| NodeKind::Recipe {
        recipe: recipe.into(),
        facility: facility.into(),
        limit,
    };
    let gas = add(
        &mut harness,
        recipe("gas_xiranite", "gas_pump_1", Some(5.0)),
        0.0,
        0.0,
    );
    let powder = add(
        &mut harness,
        recipe("phase_trans_2-xiranite_powder", "phase_trans_2", None),
        380.0,
        140.0,
    );
    let water = add(
        &mut harness,
        recipe("liquid_water", "pump_1", None),
        380.0,
        380.0,
    );
    let liquid = add(
        &mut harness,
        recipe("liquid_xiranite", "mix_pool_1", None),
        780.0,
        140.0,
    );
    let refined = add(
        &mut harness,
        recipe("phase_trans_1-liquid_xiranite", "phase_trans_1", Some(3.0)),
        1180.0,
        0.0,
    );
    link(&mut harness, gas, powder, "gas_xiranite");
    link(&mut harness, gas, refined, "gas_xiranite");
    link(&mut harness, powder, liquid, "xiranite_powder");
    link(&mut harness, water, liquid, "liquid_water");
    link(&mut harness, liquid, refined, "liquid_xiranite");
    fit_view(&mut harness);
    snapshot_graph(&mut harness, "link_labels");
}

fn picker_rect(harness: &mut Harness<'_, App>) -> eframe::egui::Rect {
    let area = editor(harness).picker_area().expect("the picker is open");
    harness
        .ctx
        .memory(|m| m.area_rect(area))
        .expect("the picker is drawn")
}

#[test]
fn the_picker_opens_at_the_click_after_the_window_was_resized() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    // Far from the origin, so the canvas is panned and zoomed: graph space is not screen space.
    smelter(&mut harness, 2000.0, 1500.0);
    smelter(&mut harness, 2600.0, 1900.0);
    fit_view(&mut harness);
    let at = editor(&mut harness).canvas_center_on_screen();
    secondary_click_at(&mut harness, at);
    let rect = picker_rect(&mut harness);
    assert!(
        (rect.min - at).length() < 2.0,
        "the picker opens at {at:?}, not {:?}",
        rect.min
    );
    harness.set_size(eframe::egui::vec2(700.0, 450.0));
    harness.run();
    harness.key_press(eframe::egui::Key::Escape);
    harness.run();
    harness.set_size(WINDOW_SIZE.into());
    harness.run();

    let at = editor(&mut harness).canvas_center_on_screen() - eframe::egui::vec2(300.0, 300.0);
    secondary_click_at(&mut harness, at);
    let rect = picker_rect(&mut harness);
    assert!(
        (rect.min - at).length() < 2.0,
        "the picker opens at {at:?}, not {:?}",
        rect.min
    );
    assert!(rect.height() > 300.0, "full height again: {rect:?}");
}

// The web build's files: kept in the browser's storage, downloaded on Save, uploaded on Open.

fn browser_harness() -> Harness<'static, App> {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    editor(&mut harness).use_browser_files(BrowserFiles::default());
    harness
}

fn downloads(harness: &mut Harness<'_, App>) -> Vec<(String, String)> {
    editor(harness)
        .take_downloads()
        .into_iter()
        .map(|d| (d.name, String::from_utf8(d.bytes).unwrap()))
        .collect()
}

#[test]
fn in_the_browser_save_as_keeps_the_file_in_storage_and_downloads_it() {
    let mut harness = browser_harness();
    smelter(&mut harness, 0.0, 0.0);
    harness.run();
    assert!(
        editor(&mut harness).document().is_draft(),
        "the first change saves a draft in the browser"
    );
    let draft = editor(&mut harness).document().path().unwrap().to_owned();
    assert!(
        downloads(&mut harness).is_empty(),
        "autosave downloads nothing"
    );

    editor(&mut harness).save_as_named("plan");
    harness.run();
    let saved = downloads(&mut harness);
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].0, "plan.aicm");
    let graph = Graph::from_json(editor(&mut harness).catalog(), &saved[0].1).unwrap();
    assert_eq!(graph.graph.nodes().len(), 1);

    let ws = editor(&mut harness);
    assert!(!ws.document().is_draft());
    let files = ws.browser_files().expect("browser files");
    assert!(!files.contains(&draft), "the draft is gone");
    let path = ws.document().path().unwrap().to_owned();
    assert!(files.contains(&path));
    assert_eq!(ws.recent_files(), [path]);
    assert_eq!(ws.file_label(&ws.recent_files()[0]), "plan.aicm");

    // Save downloads the file again, under the same name.
    smelter(&mut harness, 300.0, 0.0);
    editor(&mut harness).save();
    let again = downloads(&mut harness);
    assert_eq!(again.len(), 1);
    assert_eq!(again[0].0, "plan.aicm");
    let graph = Graph::from_json(editor(&mut harness).catalog(), &again[0].1).unwrap();
    assert_eq!(graph.graph.nodes().len(), 2);
}

#[test]
fn in_the_browser_an_uploaded_file_opens_and_is_kept() {
    let mut source = browser_harness();
    smelter(&mut source, 0.0, 0.0);
    smelter(&mut source, 300.0, 0.0);
    editor(&mut source).save_as_named("plan");
    let (name, text) = downloads(&mut source).remove(0);

    let mut harness = browser_harness();
    editor(&mut harness).open_upload(&name, text.into_bytes());
    harness.run();
    assert_eq!(node_count(&mut harness), 2);
    let ws = editor(&mut harness);
    assert!(!ws.document().is_draft());
    let path = ws.document().path().unwrap().to_owned();
    assert_eq!(ws.file_label(&path), "plan.aicm");
    assert!(ws.browser_files().unwrap().contains(&path));
    assert_eq!(ws.recent_files(), [path]);

    editor(&mut harness).open_upload("broken.aicm", b"{ not json".to_vec());
    harness.run();
    harness.get_by_label_contains("Could not open broken.aicm");
    assert_eq!(node_count(&mut harness), 2, "the open graph stays");
    let ws = editor(&mut harness);
    assert_eq!(
        ws.browser_files().unwrap().len(),
        1,
        "a broken file isn't kept"
    );
}

/// eframe storage in memory, as the browser's local storage would hold it.
#[derive(Default)]
struct MemoryStorage(std::collections::HashMap<String, String>);

impl eframe::Storage for MemoryStorage {
    fn get_string(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }
    fn set_string(&mut self, key: &str, value: String) {
        self.0.insert(key.to_owned(), value);
    }
    fn remove_string(&mut self, key: &str) {
        self.0.remove(key);
    }
    fn flush(&mut self) {}
}

#[test]
fn in_the_browser_the_last_graph_reopens_from_storage() {
    let mut harness = browser_harness();
    smelter(&mut harness, 0.0, 0.0);
    harness.run();
    let mut storage = MemoryStorage::default();
    eframe::App::save(harness.state_mut(), &mut storage);

    let catalog = aic_data::load(&aic_editor::testing::fixture_dir(), "en").unwrap();
    let mut again = Harness::builder()
        .with_size(WINDOW_SIZE)
        .build_eframe(|cc| {
            aic_app_egui::theme::install(&cc.egui_ctx);
            App::in_browser(Strings::english(), Ok(catalog), Some(&storage))
        });
    again.run();
    assert_eq!(node_count(&mut again), 1);
    assert!(editor(&mut again).document().is_draft());
}

#[test]
fn the_status_bar_shows_the_frame_rate_and_solve_time_until_turned_off() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    editor(&mut harness).set_show_performance(true);
    smelter(&mut harness, 0.0, 0.0);
    harness.run();
    harness.get_by_label_contains(" fps");
    harness.get_by_label_contains("Solve ");

    harness.get_by_label("View").click();
    harness.run();
    harness.get_by_label("Frame rate and solve time").click();
    harness.run();
    assert!(harness.query_by_label_contains(" fps").is_none());
    assert!(harness.query_by_label_contains("Solve ").is_none());
}

// M10: multiple views.

use aic_app_egui::views::{ViewAction, ViewKey};

fn views(harness: &mut Harness<'_, App>) -> Vec<ViewKey> {
    editor(harness).views().keys()
}

fn focused(harness: &mut Harness<'_, App>) -> ViewKey {
    editor(harness).views().focused()
}

fn view_menu(harness: &mut Harness<'_, App>, item: &str) {
    harness.get_by_label("View").click();
    harness.run();
    // A menu button's label carries its shortcut too.
    harness.get_by_label_contains(item).click();
    harness.run();
}

#[test]
fn one_view_has_no_tab_bar() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    assert_eq!(views(&mut harness).len(), 1);
    assert!(harness.query_by_label("Close view").is_none());
}

#[test]
fn split_right_shows_the_graph_twice_and_an_edit_in_one_view_shows_in_the_other() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    let ingot = smelter(&mut harness, 0.0, 0.0);
    let first = focused(&mut harness);
    view_menu(&mut harness, "Split right");
    let keys = views(&mut harness);
    assert_eq!(keys.len(), 2);
    let second = focused(&mut harness);
    assert_ne!(second, first, "the new view has focus");
    let titles: Vec<String> = keys
        .iter()
        .map(|key| editor(&mut harness).view_title(*key, &Strings::english()))
        .collect();
    assert_eq!(titles[1], format!("{} · 2", titles[0]));

    // Added in the second view, drawn in the first.
    let plate = add(
        &mut harness,
        NodeKind::Recipe {
            recipe: "iron_plate".into(),
            facility: "constructor".into(),
            limit: None,
        },
        400.0,
        0.0,
    );
    editor(&mut harness).focus_view(first);
    harness.run();
    let ed = editor(&mut harness);
    assert!(
        ed.node_on_screen(plate).is_some(),
        "the first view draws it"
    );
    assert!(ed.node_on_screen(ingot).is_some());

    // One selection for every view.
    editor(&mut harness).select_nodes(vec![ingot]);
    harness.run();
    editor(&mut harness).focus_view(second);
    harness.run();
    assert_eq!(
        editor(&mut harness).selection(),
        Some(&Selection::node(ingot))
    );
    harness.snapshot("views_split");
}

#[test]
fn the_keys_split_a_view_down_and_close_it_again() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    harness.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::Backslash);
    harness.run();
    assert_eq!(views(&mut harness).len(), 2);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Backslash);
    harness.run();
    assert_eq!(views(&mut harness).len(), 3);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::W);
    harness.run();
    harness.key_press_modifiers(Modifiers::COMMAND, Key::W);
    harness.run();
    assert_eq!(views(&mut harness).len(), 1);
    // The last view stays.
    harness.key_press_modifiers(Modifiers::COMMAND, Key::W);
    harness.run();
    assert_eq!(views(&mut harness).len(), 1);
}

#[test]
fn a_view_of_a_dissolved_workspace_goes_back_to_the_top() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    let [_, ingot, plate, _] = plate_chain(&mut harness);
    editor(&mut harness).select_nodes(vec![ingot, plate]);
    harness.run();
    harness.key_press_modifiers(Modifiers::COMMAND, Key::G);
    harness.run();
    let workspace = editor(&mut harness)
        .selection()
        .and_then(Selection::single_workspace)
        .unwrap();
    let top = focused(&mut harness);
    editor(&mut harness)
        .views_mut()
        .request(ViewAction::OpenBeside(top, workspace));
    harness.run();
    let inside = focused(&mut harness);
    assert_ne!(inside, top);
    assert_eq!(editor(&mut harness).view_level(inside), Some(workspace));
    assert_eq!(editor(&mut harness).view_level(top), None);
    harness.snapshot("views_workspace_beside_top");

    editor(&mut harness).focus_view(top);
    harness.run();
    harness.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::G);
    harness.run();
    let ed = editor(&mut harness);
    assert!(ed.document().graph().workspace_info(workspace).is_none());
    assert_eq!(ed.view_level(inside), None);
}

#[test]
fn a_view_moves_to_a_window_of_its_own() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    smelter(&mut harness, 0.0, 0.0);
    view_menu(&mut harness, "Split right");
    let moved = focused(&mut harness);
    view_menu(&mut harness, "Move to new window");
    let ed = editor(&mut harness);
    assert_eq!(ed.views().window_count(), 1);
    assert!(ed.views().is_in_window(moved));
    assert_eq!(ed.views().len(), 2);
    harness.run();
    harness.snapshot("views_window");
}

#[test]
fn the_layout_of_the_views_is_kept_with_the_settings() {
    let mut harness = browser_harness();
    let [_, ingot, plate, _] = plate_chain(&mut harness);
    editor(&mut harness).select_nodes(vec![ingot, plate]);
    harness.run();
    harness.key_press_modifiers(Modifiers::COMMAND, Key::G);
    harness.run();
    let top = focused(&mut harness);
    let workspace = editor(&mut harness)
        .selection()
        .and_then(Selection::single_workspace)
        .unwrap();
    editor(&mut harness)
        .views_mut()
        .request(ViewAction::OpenBeside(top, workspace));
    harness.run();
    editor(&mut harness)
        .views_mut()
        .request(ViewAction::SplitDown(top));
    harness.run();
    let mut storage = MemoryStorage::default();
    eframe::App::save(harness.state_mut(), &mut storage);

    let catalog = aic_data::load(&aic_editor::testing::fixture_dir(), "en").unwrap();
    let mut again = Harness::builder()
        .with_size(WINDOW_SIZE)
        .build_eframe(|cc| {
            aic_app_egui::theme::install(&cc.egui_ctx);
            App::in_browser(Strings::english(), Ok(catalog), Some(&storage))
        });
    again.run();
    let keys = views(&mut again);
    assert_eq!(keys.len(), 3);
    let ed = editor(&mut again);
    let workspace = ed.document().graph().workspaces().keys().next().unwrap();
    let levels: Vec<_> = keys.iter().map(|key| ed.view_level(*key)).collect();
    assert_eq!(levels.iter().filter(|l| **l == Some(workspace)).count(), 1);
    assert_eq!(levels.iter().filter(|l| l.is_none()).count(), 2);
}

#[test]
fn a_tab_dragged_over_a_view_shows_where_it_lands_and_splits_there() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    smelter(&mut harness, 0.0, 0.0);
    view_menu(&mut harness, "Split right");
    view_menu(&mut harness, "Duplicate view");
    assert_eq!(views(&mut harness).len(), 3);
    // The duplicate's tab, second in the right-hand group (egui_dock's tabs aren't in the
    // accessibility tree), onto the bottom of the left view.
    let tab = pos2(760.0, 39.0);
    let target = pos2(420.0, 700.0);
    harness.hover_at(tab);
    harness.run();
    harness.event(Event::PointerButton {
        pos: tab,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    for step in 1..=8 {
        harness.event(Event::PointerMoved(tab.lerp(target, step as f32 / 8.0)));
        harness.step();
    }
    harness.step();
    harness.snapshot("views_drop_preview");
    harness.event(Event::PointerButton {
        pos: target,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    harness.run();
    assert_eq!(views(&mut harness).len(), 3, "moved, not closed");
    harness.snapshot("views_dropped");
}

// Recipes of every region, with region badges.

#[test]
fn a_recipe_of_another_region_is_offered_with_its_badge_and_warns_on_its_card() {
    let mut harness = harness(&aic_editor::testing::fixture_dir());
    // The graph is in Valley IV; Iron Wire is Wuling's only.
    harness.get_by_label_contains("Iron Wire").click();
    harness.run();
    let ed = editor(&mut harness);
    let node = ed
        .selection()
        .and_then(Selection::single_node)
        .expect("added");
    assert!(
        ed.document().solution().warnings.iter().any(|w| matches!(
            w,
            aic_core::Warning::Issue(aic_core::Issue::RecipeOutsideRegion { node: n, .. }) if *n == node
        )),
        "the solver says the region lacks it"
    );
    fit_view(&mut harness);
    harness.snapshot("region_badges");

    editor(&mut harness).open_picker(Origin::Canvas, pos2(300.0, 300.0));
    harness.run();
    harness.snapshot("region_badges_picker");
}
