//! Drives the app headlessly and compares it with the images in `tests/snapshots/`.
//!
//! After an intended UI change, run `UPDATE_SNAPSHOTS=1 cargo test -p aic-app` and review the new
//! images before committing them.

use std::path::{Path, PathBuf};

use aic_app::canvas::Selection;
use aic_app::i18n::Strings;
use aic_app::picker::Origin;
use aic_app::{App, WINDOW_SIZE};
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
        .build_eframe(|_cc| App::new(Strings::english(), Ok(catalog), None));
    harness.run();
    harness
}

fn workspace<'a>(harness: &'a mut Harness<'_, App>) -> &'a mut aic_app::workspace::Workspace {
    harness.state_mut().workspace_mut().expect("data loaded")
}

fn selected(harness: &mut Harness<'_, App>) -> NodeId {
    match workspace(harness)
        .selection()
        .and_then(Selection::single_node)
    {
        Some(node) => node,
        None => panic!(
            "expected one selected node, got {:?}",
            workspace(harness).selection()
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
    harness.get_by_label("Settings…").click();
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
    workspace(harness).document().graph().nodes().len()
}

fn node_pos(harness: &mut Harness<'_, App>, node: NodeId) -> Pos2 {
    let pos = workspace(harness)
        .document()
        .graph()
        .node(node)
        .expect("node exists")
        .pos;
    pos2(pos.x, pos.y)
}

fn selected_nodes(harness: &mut Harness<'_, App>) -> Vec<NodeId> {
    let mut nodes = workspace(harness)
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
    workspace(harness)
        .node_on_screen(node)
        .expect("node is drawn")
}

#[test]
fn delete_a_selection_then_undo_and_redo_it() {
    let mut harness = harness(&fixture("tests/fixtures/satisfactory/vtest"));
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
    let mut harness = harness(&fixture("tests/fixtures/satisfactory/vtest"));
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
    let mut harness = harness(&fixture("tests/fixtures/satisfactory/vtest"));
    let a = smelter(&mut harness, 0.0, 0.0);
    let b = smelter(&mut harness, 300.0, 0.0);
    let alone = smelter(&mut harness, 0.0, 300.0);
    harness.run();
    fit_view(&mut harness);
    workspace(&mut harness).select_nodes(vec![a, b]);
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
    let mut harness = harness(&fixture("tests/fixtures/satisfactory/vtest"));
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
    workspace(&mut harness).select_nodes(vec![ingot, plate]);
    harness.run();

    // The window sends clipboard events rather than Cmd+C and Cmd+V key presses.
    harness.event(Event::Copy);
    harness.run();
    harness.event(Event::Paste("some text".into()));
    harness.run();
    assert_eq!(node_count(&mut harness), 4);
    assert_eq!(workspace(&mut harness).document().graph().links().len(), 2);
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
    let mut harness = harness(&fixture("tests/fixtures/satisfactory/vtest"));
    let strings = Strings::english();
    let path = std::env::temp_dir().join(format!("aic-save-{}.aicm", std::process::id()));
    smelter(&mut harness, 0.0, 0.0);
    harness.run();
    assert!(workspace(&mut harness).document().is_modified());
    assert!(workspace(&mut harness).title().starts_with("Untitled •"));

    workspace(&mut harness).save_to(&path, &strings);
    harness.run();
    assert!(!workspace(&mut harness).document().is_modified());
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    assert!(workspace(&mut harness).title().starts_with(&name));
    assert_eq!(
        workspace(&mut harness).recent_files(),
        std::slice::from_ref(&path)
    );

    workspace(&mut harness).new_graph("tundra".into());
    harness.run();
    assert_eq!(node_count(&mut harness), 0);
    workspace(&mut harness).open_file(&path, &strings);
    harness.run();
    assert_eq!(node_count(&mut harness), 1);
    assert_eq!(
        workspace(&mut harness).document().path(),
        Some(path.as_path())
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn round_up_shows_whole_facilities_and_how_busy_they_are() {
    let mut harness = harness(&fixture("tests/fixtures/satisfactory/vtest"));
    let frame = add_from_browser(&mut harness, "Modular Frame");
    type_into(&mut harness, "Node limit", "1.25");
    workspace(&mut harness).set_round_up(true);
    harness.run();
    harness.get_by_label("2");
    harness.get_by_label("62.5% busy");
    let _ = frame;
    snapshot_graph(&mut harness, "round_up");
}

// M7: data versions, overlays and languages.

fn data_folder_app() -> Harness<'static, App> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../aic-data/tests/fixtures");
    let folder = aic_app::data::DataFolder::at(dir).map_err(|e| e.to_string());
    let mut harness = Harness::builder()
        .with_size(WINDOW_SIZE)
        .build_eframe(move |_cc| App::from_folder(Strings::english(), folder.clone(), None));
    harness.run();
    harness
}

#[test]
fn switching_data_versions_keeps_the_graph_and_matches_recipes_by_id() {
    let mut harness = data_folder_app();
    assert_eq!(workspace(&mut harness).catalog().version(), "v1.10.0");
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
    workspace(&mut harness)
        .settings_mut()
        .open_on(&"jinlong".into());
    harness.run();
    harness.snapshot("settings_data");

    let choice = aic_app::data::DataChoice {
        version: Some("v1.9.0".into()),
        language: None,
    };
    workspace(&mut harness).switch_data(choice.clone(), &Strings::english());
    harness.run();
    let ws = workspace(&mut harness);
    assert_eq!(ws.catalog().version(), "v1.9.0");
    assert_eq!(ws.data_choice(), Some(&choice));
    assert!(matches!(
        ws.document().graph().node(nugget).map(|n| &n.kind),
        Some(NodeKind::MissingRecipe { .. })
    ));
}

#[test]
fn an_outpost_order_from_an_output_port_earns_coupons() {
    let mut harness = harness(&fixture("tests/fixtures/satisfactory/vtest"));
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
    let ws = workspace(&mut harness);
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
    let mut harness = harness(&fixture("tests/fixtures/satisfactory/vtest"));
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
    let pin = workspace(&mut harness)
        .port_on_screen(ore, Side::Output, "iron_ore")
        .expect("pin is drawn");
    double_click_at(&mut harness, pin - vec2(24.0, 0.0));
    assert!(workspace(&mut harness).port_menu_is_open());

    type_into(&mut harness, "Share for Sink · Iron Ore", "2");
    type_into(&mut harness, "Share for Iron Ingot", "1");
    let s = workspace(&mut harness).document().solution().clone();
    assert_near(flow(&s, goal, "iron_ore", Side::Input), 40.0);
    assert_near(flow(&s, smelter, "iron_ore", Side::Input), 20.0);
    snapshot_graph(&mut harness, "ratio_lock");

    harness.get_by_label("Unlock: split evenly").click();
    harness.run();
    let graph = workspace(&mut harness).document().graph().clone();
    assert!(graph.links().values().all(|l| l.out_share.is_none()));
    // Unlocked, the sink takes everything: a sink's intake counts as throughput (ADR 0004).
    let s = workspace(&mut harness).document().solution().clone();
    assert_near(flow(&s, goal, "iron_ore", Side::Input), 60.0);
}

#[test]
fn grouping_collapses_nodes_into_one_card_with_their_outside_ports() {
    let mut harness = harness(&fixture("tests/fixtures/satisfactory/vtest"));
    let ore = add(
        &mut harness,
        NodeKind::Source {
            item: "iron_ore".into(),
            limit: 60.0,
        },
        0.0,
        0.0,
    );
    let ingot = smelter(&mut harness, 250.0, 0.0);
    let plate = add(
        &mut harness,
        NodeKind::Recipe {
            recipe: "iron_plate".into(),
            facility: "constructor".into(),
            limit: None,
        },
        550.0,
        0.0,
    );
    let goal = add(
        &mut harness,
        NodeKind::Sink {
            item: "iron_plate".into(),
            limit: None,
        },
        900.0,
        0.0,
    );
    link_any(
        &mut harness,
        PortRef::any(ore),
        PortRef::new(ingot, "iron_ore"),
    );
    link(&mut harness, ingot, plate, "iron_ingot");
    link_any(
        &mut harness,
        PortRef::new(plate, "iron_plate"),
        PortRef::any(goal),
    );
    let before = workspace(&mut harness).document().solution().clone();
    workspace(&mut harness).select_nodes(vec![ingot, plate]);
    harness.run();

    harness.key_press_modifiers(Modifiers::COMMAND, Key::G);
    harness.run();
    let ws = workspace(&mut harness);
    let group = ws
        .selection()
        .and_then(Selection::single_group)
        .expect("the new group is selected");
    assert_eq!(
        ws.document().view().nodes().count(),
        3,
        "two nodes became one card"
    );
    assert_eq!(ws.document().view().wires().count(), 2);
    // A group is a view only: the numbers don't change.
    let after = ws.document().solution().clone();
    for node in [ingot, plate] {
        assert_near(count(&after, node), count(&before, node).unwrap());
    }
    fit_view(&mut harness);
    harness.get_by_label("Group 1");
    snapshot_graph(&mut harness, "group_collapsed");

    // The card's ports are the inside nodes' ports: the port menu works on them.
    let pin = workspace(&mut harness)
        .port_on_screen(plate, Side::Output, "iron_plate")
        .expect("the group shows the plate port");
    assert!(workspace(&mut harness).group_on_screen(group).is_some());
    double_click_at(&mut harness, pin - vec2(24.0, 0.0));
    assert!(workspace(&mut harness).port_menu_is_open());
    harness.key_press(Key::Escape);
    harness.run();

    // The inspector's button; kittest can't click inside the zoomed canvas layer.
    assert!(
        workspace(&mut harness)
            .selection()
            .is_some_and(|s| s.single_group() == Some(group))
    );
    harness
        .get_all_by_label("Expand group")
        .find(|node| node.rect().left() > 980.0)
        .expect("the inspector offers it")
        .click();
    harness.run();
    assert_eq!(workspace(&mut harness).document().view().nodes().count(), 4);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    harness.run();
    assert_eq!(
        workspace(&mut harness).document().view().nodes().count(),
        3,
        "undo collapses it again"
    );
}

#[test]
fn a_copies_group_runs_as_many_copies_as_the_graph_allows() {
    let mut harness = harness(&fixture("tests/fixtures/satisfactory/vtest"));
    let ore = add(
        &mut harness,
        NodeKind::Source {
            item: "iron_ore".into(),
            limit: 90.0,
        },
        0.0,
        0.0,
    );
    let ingot = smelter(&mut harness, 300.0, 0.0);
    let goal = add(
        &mut harness,
        NodeKind::Sink {
            item: "iron_ingot".into(),
            limit: None,
        },
        650.0,
        0.0,
    );
    link_any(
        &mut harness,
        PortRef::any(ore),
        PortRef::new(ingot, "iron_ore"),
    );
    link_any(
        &mut harness,
        PortRef::new(ingot, "iron_ingot"),
        PortRef::any(goal),
    );
    workspace(&mut harness).select_nodes(vec![ingot]);
    harness.run();
    harness.key_press_modifiers(Modifiers::COMMAND, Key::G);
    harness.run();
    harness.get_by_label("Stands for copies").click();
    harness.run();

    let ws = workspace(&mut harness);
    let group = ws.selection().and_then(Selection::single_group).unwrap();
    let s = ws.document().solution().clone();
    assert_near(s.copies.get(&group).copied().flatten(), 3.0);
    assert_near(s.rate.get(&goal).copied().flatten(), 90.0);
    assert_near(count(&s, ingot), 1.0);
    assert_eq!(
        s.totals.facilities[&aic_data::FacilityId::from("smelter")],
        3.0
    );
    fit_view(&mut harness);
    snapshot_graph(&mut harness, "group_copies");
}

#[test]
fn export_saves_the_canvas_as_a_png() {
    let mut harness = harness(&fixture("tests/fixtures/satisfactory/vtest"));
    add_from_browser(&mut harness, "Iron Plate");
    let path = std::env::temp_dir().join(format!("aic-export-{}.png", std::process::id()));
    workspace(&mut harness).request_export(path.clone());
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
    let mut harness = harness(&fixture("tests/fixtures/satisfactory/vtest"));
    let strings = Strings::english();
    let dir = temp_dir("drafts");
    let drafts = dir.join("drafts");
    workspace(&mut harness).set_drafts_dir(Some(drafts.clone()));
    harness.run();
    assert!(
        files_in(&drafts).is_empty(),
        "an untouched graph has no draft"
    );

    smelter(&mut harness, 0.0, 0.0);
    harness.run();
    let draft = files_in(&drafts);
    assert_eq!(draft.len(), 1, "the first change saves at once");
    assert!(workspace(&mut harness).document().is_draft());
    assert!(workspace(&mut harness).title().starts_with("Untitled —"));

    let mine = dir.join("plan.aicm");
    workspace(&mut harness).save_to(&mine, &strings);
    harness.run();
    assert!(files_in(&drafts).is_empty(), "the draft is gone");
    assert!(!workspace(&mut harness).document().is_draft());
    assert_eq!(
        workspace(&mut harness).document().path(),
        Some(mine.as_path())
    );

    // New saves the open file first, then starts a fresh draft.
    smelter(&mut harness, 300.0, 0.0);
    workspace(&mut harness).request(aic_app::files::Pending::New, &strings);
    harness.run();
    let saved = Graph::from_json(
        workspace(&mut harness).catalog(),
        &std::fs::read_to_string(&mine).unwrap(),
    )
    .unwrap();
    assert_eq!(saved.graph.nodes().len(), 2);
    assert_eq!(node_count(&mut harness), 0);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn reopening_restores_the_graph_and_where_the_canvas_looked() {
    let mut harness = harness(&fixture("tests/fixtures/satisfactory/vtest"));
    let strings = Strings::english();
    let dir = temp_dir("reopen");
    let path = dir.join("plan.aicm");
    let node = smelter(&mut harness, 120.0, 80.0);
    harness.run();
    fit_view(&mut harness);
    workspace(&mut harness).save_to(&path, &strings);
    let view = workspace(&mut harness)
        .canvas_view()
        .expect("the canvas was drawn");

    let mut again = self::harness(&fixture("tests/fixtures/satisfactory/vtest"));
    workspace(&mut again).reopen(&path, &strings);
    again.run();
    let ws = workspace(&mut again);
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
fn a_missing_or_broken_last_file_starts_a_new_graph_and_is_left_alone() {
    let mut harness = harness(&fixture("tests/fixtures/satisfactory/vtest"));
    let strings = Strings::english();
    let dir = temp_dir("broken");
    workspace(&mut harness).set_drafts_dir(Some(dir.join("drafts")));

    workspace(&mut harness).reopen(&dir.join("gone.aicm"), &strings);
    harness.run();
    harness.get_by_label_contains("is gone. Started a new one.");

    let broken = dir.join("broken.aicm");
    std::fs::write(&broken, "{ not json").unwrap();
    workspace(&mut harness).reopen(&broken, &strings);
    harness.run();
    harness.get_by_label_contains("Could not open");
    smelter(&mut harness, 0.0, 0.0);
    harness.run();
    assert_eq!(
        std::fs::read_to_string(&broken).unwrap(),
        "{ not json",
        "autosave never writes over a file it couldn't read"
    );
    assert!(workspace(&mut harness).document().is_draft());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_region_can_change_until_the_first_node() {
    let mut harness = harness(&fixture("tests/fixtures/satisfactory/vtest"));
    harness
        .get_by_role_and_label(eframe::egui::accesskit::Role::ComboBox, "Region")
        .click();
    harness.run();
    harness.get_by_label("Wuling").click();
    harness.run();
    assert_eq!(
        workspace(&mut harness).document().graph().region().as_str(),
        "jinlong"
    );
    smelter(&mut harness, 0.0, 0.0);
    harness.run();
    assert!(
        workspace(&mut harness)
            .document_mut()
            .set_region("tundra".into())
            .is_err()
    );
}

#[test]
fn the_toolbar_runs_commands_and_lights_up_for_their_keys() {
    let mut harness = harness(&fixture("tests/fixtures/satisfactory/vtest"));
    let dir = temp_dir("toolbar");
    workspace(&mut harness).set_drafts_dir(Some(dir.clone()));
    harness.get_by_label("Nothing to save yet");

    let node = smelter(&mut harness, 0.0, 0.0);
    harness.run();
    harness.get_by_label("✔ Saved as a draft");
    workspace(&mut harness).select_nodes(vec![node]);
    harness.run();

    harness.get_by_label("Duplicate").click();
    harness.run();
    assert_eq!(node_count(&mut harness), 2);

    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    harness.step();
    assert_eq!(
        workspace(&mut harness).flashing(),
        Some(aic_app::edit::Shortcut::Undo),
        "the key lights its button"
    );
    assert_eq!(node_count(&mut harness), 1);
    harness.snapshot("toolbar");
    let _ = std::fs::remove_dir_all(dir);
}

/// The player's Buckflower graph from real data: a limit on the middle planter settles everything
/// linked to it (ADR 0006).
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

    let s = workspace(&mut harness).document().solution().clone();
    assert_near(count(&s, limited), 3.0);
    assert_near(count(&s, picker), 1.5);
    assert_near(count(&s, carbon), 3.0);
    assert_near(count(&s, free), 0.0);
    fit_view(&mut harness);
    snapshot_graph(&mut harness, "limit_in_the_middle");
}
