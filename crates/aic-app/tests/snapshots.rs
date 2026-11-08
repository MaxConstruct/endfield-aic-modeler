//! Drives the app headlessly and compares it with the images in `tests/snapshots/`.
//!
//! After an intended UI change, run `UPDATE_SNAPSHOTS=1 cargo test -p aic-app` and review the new
//! images before committing them.

use std::path::{Path, PathBuf};

use aic_app::canvas::Selection;
use aic_app::i18n::Strings;
use aic_app::picker::Origin;
use aic_app::{App, WINDOW_SIZE};
use aic_core::{NodeId, NodeKind, Port, PortRef, Side, Solution};
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
        .build_eframe(|_cc| App::new(Strings::english(), Ok(catalog), None));
    harness.run();
    harness
}

fn workspace<'a>(harness: &'a mut Harness<'_, App>) -> &'a mut aic_app::workspace::Workspace {
    harness.state_mut().workspace_mut().expect("data loaded")
}

fn selected(harness: &mut Harness<'_, App>) -> NodeId {
    match workspace(harness).selection() {
        Some(Selection::Node(node)) => node,
        other => panic!("expected a selected node, got {other:?}"),
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
    let ws = workspace(harness);
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
        !workspace(harness).picker_is_open(),
        "one click closes the picker"
    );
    selected(harness)
}

/// Stands in for dragging a link between two existing nodes.
fn link(harness: &mut Harness<'_, App>, from: NodeId, to: NodeId, item: &str) {
    let ws = workspace(harness);
    let catalog = ws.catalog().clone();
    ws.document_mut()
        .connect(&catalog, PortRef::new(from, item), PortRef::new(to, item))
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
        .build_eframe(|_cc| {
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
    let mut harness = harness(&fixture("tests/fixtures/satisfactory/vtest"));
    harness.snapshot("empty_graph");
}

#[test]
fn canvas_picker_lists_logistics_and_recipes() {
    let mut harness = harness(&fixture("tests/fixtures/satisfactory/vtest"));
    workspace(&mut harness).open_picker(Origin::Canvas, eframe::egui::pos2(0.0, 0.0));
    harness.run();
    harness.get_by_label("Splitter");
    harness.snapshot("picker_canvas");
}

// Golden test 7 (docs/PLAN.md, M3), rebuilt the way a player would: the frame from the recipe
// browser, then every upstream node through the picker from an input port, one click each.
#[test]
fn rebuild_golden_07_through_the_picker() {
    let mut harness = harness(&fixture("tests/fixtures/satisfactory/vtest"));
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

    let ws = workspace(&mut harness);
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
    workspace(&mut harness).new_graph("jinlong".into());
    harness.run();
    // The browser lists recipes in data order; the planting recipe is called Yazhen.
    harness
        .get_all_by_label("Yazhen")
        .next()
        .expect("browser lists Yazhen")
        .click();
    harness.run();
    let planting = selected(&mut harness);

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

    let ws = workspace(&mut harness);
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

fn add_from_browser(harness: &mut Harness<'_, App>, label: &str) -> NodeId {
    harness.get_by_label(label).click();
    harness.run();
    selected(harness)
}

#[test]
fn right_click_on_canvas_opens_the_picker() {
    let mut harness = harness(&fixture("tests/fixtures/satisfactory/vtest"));
    let center = workspace(&mut harness).canvas_center_on_screen();
    secondary_click_at(&mut harness, center);
    assert_eq!(
        workspace(&mut harness).picker_origin(),
        Some(&Origin::Canvas)
    );
    harness.key_press(eframe::egui::Key::Escape);
    harness.run();
    assert!(
        !workspace(&mut harness).picker_is_open(),
        "Escape closes it"
    );
}

#[test]
fn right_click_on_an_open_port_adds_a_producer_of_its_item() {
    let mut harness = harness(&fixture("tests/fixtures/satisfactory/vtest"));
    let plate = add_from_browser(&mut harness, "Iron Plate");
    let pin = workspace(&mut harness)
        .port_on_screen(plate, Side::Input, "iron_ingot")
        .expect("pin is drawn");
    secondary_click_at(&mut harness, pin);
    assert_eq!(
        workspace(&mut harness).picker_origin(),
        Some(&Origin::Port {
            node: plate,
            side: Side::Input,
            item: "iron_ingot".into()
        })
    );
}

#[test]
fn dragging_a_link_onto_empty_canvas_picks_and_links_in_one_click() {
    let mut harness = harness(&fixture("tests/fixtures/satisfactory/vtest"));
    let plate = add_from_browser(&mut harness, "Iron Plate");
    let pin = workspace(&mut harness)
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
            workspace(&mut harness).picker_origin(),
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
    let ws = workspace(&mut harness);
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
    workspace(harness)
        .document_mut()
        .add_node(kind, eframe::egui::pos2(x, y))
}

fn link_any(harness: &mut Harness<'_, App>, from: PortRef, to: PortRef) {
    let ws = workspace(harness);
    let catalog = ws.catalog().clone();
    ws.document_mut()
        .connect(&catalog, from, to)
        .expect("link is valid");
}

// A source too small for the smelter's limit throttles it (marker on its input). The ingots pass
// a control port into storage, which fills because the sink takes less.
#[test]
fn logistics_nodes_and_a_throttled_node() {
    let mut harness = harness(&fixture("tests/fixtures/satisfactory/vtest"));
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
        300.0,
        0.0,
    );
    let valve = add(
        &mut harness,
        NodeKind::ControlPort {
            item: None,
            limit: 40.0,
        },
        650.0,
        0.0,
    );
    let storage = add(&mut harness, NodeKind::Storage { item: None }, 950.0, 0.0);
    let goal = add(
        &mut harness,
        NodeKind::Sink {
            item: "iron_ingot".into(),
            limit: Some(10.0),
        },
        1250.0,
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

    let s = workspace(&mut harness).document().solution().clone();
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
    let mut harness = harness(&fixture("tests/fixtures/satisfactory/vtest"));
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
    harness.get_by_label("Raw resources…").click();
    harness.run();
    let role = eframe::egui::accesskit::Role::TextInput;
    harness.get_by_role_and_label(role, "Iron Ore").focus();
    harness.run();
    harness
        .get_by_role_and_label(role, "Iron Ore")
        .type_text("45");
    harness.run();

    let ws = workspace(&mut harness);
    assert_eq!(
        ws.availability().get(&"tundra".into(), &"iron_ore".into()),
        Some(45.0)
    );
    harness.get_by_label("Iron Ore: using 60.00 /min, but the region supplies 45.00 /min.");
    harness.snapshot("raw_resource_over_use");
}

fn limit_of(harness: &mut Harness<'_, App>, node: NodeId) -> Option<f64> {
    let ws = workspace(harness);
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
    let mut harness = harness(&fixture("tests/fixtures/satisfactory/vtest"));
    let frame = add_from_browser(&mut harness, "Modular Frame");
    type_into(&mut harness, "Node limit", "1.25");
    assert_eq!(limit_of(&mut harness, frame), Some(1.25));
    let s = workspace(&mut harness).document().solution().clone();
    assert_near(count(&s, frame), 1.25);
    snapshot_graph(&mut harness, "limit_on_card");
}

#[test]
fn double_clicking_a_port_sets_the_limit_as_a_rate() {
    let mut harness = harness(&fixture("tests/fixtures/satisfactory/vtest"));
    // Iron Plate: 20 plate per minute per facility.
    let plate = add_from_browser(&mut harness, "Iron Plate");
    let pin = workspace(&mut harness)
        .port_on_screen(plate, Side::Output, "iron_plate")
        .expect("pin is drawn");
    double_click_at(&mut harness, pin - eframe::egui::vec2(24.0, 0.0));
    assert!(
        workspace(&mut harness).port_menu_is_open(),
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

#[test]
fn the_port_menu_clears_the_ports_links() {
    let mut harness = harness(&fixture("tests/fixtures/satisfactory/vtest"));
    let plate = add_from_browser(&mut harness, "Iron Plate");
    let smelter = pick(
        &mut harness,
        plate,
        Side::Input,
        "iron_ingot",
        (-80.0, 0.0),
        "Iron Ingot",
    );
    assert_eq!(workspace(&mut harness).document().graph().links().len(), 1);
    let pin = workspace(&mut harness)
        .port_on_screen(smelter, Side::Output, "iron_ingot")
        .expect("pin is drawn");
    double_click_at(&mut harness, pin - eframe::egui::vec2(24.0, 0.0));
    assert!(
        workspace(&mut harness).port_menu_is_open(),
        "double-click opens the port menu"
    );
    harness.get_by_label("Clear connections").click();
    harness.run();
    assert_eq!(workspace(&mut harness).document().graph().links().len(), 0);
}

#[test]
fn side_panels_collapse() {
    let mut harness = harness(&fixture("tests/fixtures/satisfactory/vtest"));
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
    workspace(&mut harness).new_graph("jinlong".into());
    harness.run();
    let planting = add_from_browser(&mut harness, "Yazhen");
    let at = {
        let pos = workspace(&mut harness)
            .document()
            .graph()
            .node(planting)
            .unwrap()
            .pos;
        eframe::egui::pos2(pos.x - 80.0, pos.y)
    };
    workspace(&mut harness).open_picker(
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
