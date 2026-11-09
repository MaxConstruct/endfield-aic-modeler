//! Workspaces: named places holding nodes and other workspaces. What crosses a workspace's edge
//! goes through its ports.

mod common;

use aic_core::{
    EditError, Graph, LinkError, Member, NodeId, NodeKind, PortRef, Pos, Side, Warning, solve,
};
use aic_data::Catalog;
use common::*;

const TOLERANCE: f64 = 1e-6;

fn catalog() -> Catalog {
    TestCatalog::new()
        .facility("furnace", 5.0)
        .recipe("ingot", "furnace", 2.0, &[("ore", 1.0)], &[("ingot", 1.0)])
        .recipe(
            "plate",
            "furnace",
            2.0,
            &[("ingot", 1.0)],
            &[("plate", 1.0)],
        )
        .build()
}

#[track_caller]
fn assert_near(actual: Option<f64>, expected: f64) {
    let actual = actual.expect("value is determined");
    assert!(
        (actual - expected).abs() < TOLERANCE,
        "expected {expected}, got {actual}"
    );
}

/// ore source → ingot → plate → plate sink.
fn chain(catalog: &Catalog) -> (Graph, [NodeId; 4]) {
    let mut g = Graph::new(region(TUNDRA));
    let ore = add(
        &mut g,
        NodeKind::Source {
            item: item("ore"),
            limit: 30.0,
        },
    );
    let ingot = add_recipe(&mut g, catalog, "ingot", None);
    let plate = add_recipe(&mut g, catalog, "plate", None);
    let goal = add(
        &mut g,
        NodeKind::Sink {
            item: item("plate"),
            limit: None,
        },
    );
    g.connect(catalog, PortRef::any(ore), PortRef::new(ingot, "ore"))
        .unwrap();
    g.connect(
        catalog,
        PortRef::new(ingot, "ingot"),
        PortRef::new(plate, "ingot"),
    )
    .unwrap();
    g.connect(catalog, PortRef::new(plate, "plate"), PortRef::any(goal))
        .unwrap();
    (g, [ore, ingot, plate, goal])
}

fn linked(g: &Graph, from: NodeId, to: NodeId) -> bool {
    g.links().values().any(|l| l.from == from && l.to == to)
}

#[test]
fn a_workspace_made_from_a_selection_sends_crossing_links_through_ports() {
    let catalog = catalog();
    let (mut g, [ore, ingot, plate, goal]) = chain(&catalog);
    let flat = solve(&catalog, &g);
    let ws = g
        .workspace_from(&[Member::Node(ingot), Member::Node(plate)], "Plates")
        .unwrap();

    assert_eq!(g.workspace_info(ws).unwrap().name, "Plates");
    assert_eq!(g.node(ingot).unwrap().workspace, Some(ws));
    let [port_in] = g.ports_of(ws, Side::Input)[..] else {
        panic!("one input port");
    };
    let [port_out] = g.ports_of(ws, Side::Output)[..] else {
        panic!("one output port");
    };
    assert_eq!(
        g.node(port_in).unwrap().kind,
        NodeKind::WorkspacePort {
            side: Side::Input,
            item: Some(item("ore"))
        }
    );
    assert!(linked(&g, ore, port_in) && linked(&g, port_in, ingot));
    assert!(linked(&g, ingot, plate), "the link inside stays");
    assert!(linked(&g, plate, port_out) && linked(&g, port_out, goal));
    assert!(!linked(&g, ore, ingot));
    assert_eq!(g.members_of(None).len(), 3, "ore, goal and the workspace");

    // A plain workspace changes nothing for the solver.
    let s = solve(&catalog, &g);
    assert_near(s.rate[&goal], flat.rate[&goal].unwrap());
    for node in [ingot, plate] {
        assert_near(s.facility_count[&node], flat.facility_count[&node].unwrap());
    }
    assert_near(s.rate[&port_in], 30.0);
}

#[test]
fn a_link_crosses_a_workspace_edge_only_through_a_port() {
    let catalog = catalog();
    let mut g = Graph::new(region(TUNDRA));
    let ore = add(
        &mut g,
        NodeKind::Source {
            item: item("ore"),
            limit: 30.0,
        },
    );
    let ws = g.add_workspace("Ingots", None, Pos::default()).unwrap();
    let ingot = g
        .add_node_in(
            NodeKind::Recipe {
                recipe: "ingot".into(),
                facility: "furnace".into(),
                limit: None,
            },
            Pos::default(),
            Some(ws),
        )
        .unwrap();
    assert_eq!(
        g.connect(&catalog, PortRef::any(ore), PortRef::new(ingot, "ore")),
        Err(LinkError::AcrossWorkspaces)
    );
    let port = g.add_port(ws, Side::Input).unwrap();
    g.connect(&catalog, PortRef::any(ore), PortRef::any(port))
        .unwrap();
    g.connect(&catalog, PortRef::any(port), PortRef::new(ingot, "ore"))
        .unwrap();
    // The port took its item from its first link.
    assert_eq!(g.node(port).unwrap().kind.item(), Some(&item("ore")));
    assert_near(solve(&catalog, &g).facility_count[&ingot], 1.0);
}

#[test]
fn a_port_with_an_end_unlinked_lets_nothing_through_and_says_so() {
    let catalog = catalog();
    let (mut g, [ore, ingot, plate, _]) = chain(&catalog);
    let ws = g
        .workspace_from(&[Member::Node(ingot), Member::Node(plate)], "Plates")
        .unwrap();
    g.remove_node(ore);
    let port_in = g.ports_of(ws, Side::Input)[0];

    let s = solve(&catalog, &g);
    assert_near(s.facility_count[&ingot], 0.0);
    assert!(
        s.warnings.contains(&Warning::PortUnlinked {
            port: port_in,
            outside: true
        }),
        "{:?}",
        s.warnings
    );
    assert!(
        s.totals.depot_in.is_empty(),
        "an unlinked port doesn't pull from the depot"
    );
}

#[test]
fn workspaces_nest_and_can_be_empty() {
    let catalog = catalog();
    let (mut g, [_, ingot, plate, _]) = chain(&catalog);
    let inner = g.workspace_from(&[Member::Node(ingot)], "Ingots").unwrap();
    let outer = g
        .workspace_from(&[Member::Workspace(inner), Member::Node(plate)], "Plates")
        .unwrap();
    assert_eq!(g.workspace_info(inner).unwrap().parent, Some(outer));
    assert!(g.nodes_in_workspace(outer).contains(&ingot));
    // The inner workspace's ports moved with it; its links now reach the outer workspace's ports.
    assert_eq!(solve(&catalog, &g).facility_count[&ingot], Some(1.0));

    let empty = g.add_workspace("Later", None, Pos::default()).unwrap();
    g.remove_node(plate);
    assert!(
        g.workspace_info(empty).is_some(),
        "an empty workspace stays"
    );
    assert!(g.workspace_info(outer).is_some());
}

#[test]
fn only_members_of_one_workspace_go_in_a_new_one_together() {
    let catalog = catalog();
    let (mut g, [ore, ingot, plate, _]) = chain(&catalog);
    g.workspace_from(&[Member::Node(ingot), Member::Node(plate)], "Plates")
        .unwrap();
    assert_eq!(
        g.workspace_from(&[Member::Node(ore), Member::Node(ingot)], "Mixed"),
        Err(EditError::MixedWorkspaces)
    );
    assert_eq!(
        g.workspace_from(&[], "Empty"),
        Err(EditError::EmptyWorkspace)
    );
}

#[test]
fn dissolving_a_workspace_links_through_its_ports_again() {
    let catalog = catalog();
    let (mut g, [ore, ingot, plate, goal]) = chain(&catalog);
    let flat = solve(&catalog, &g);
    let ws = g
        .workspace_from(&[Member::Node(ingot), Member::Node(plate)], "Plates")
        .unwrap();
    g.dissolve(ws).unwrap();
    assert!(g.workspaces().is_empty());
    assert_eq!(g.nodes().len(), 4, "the ports are gone");
    assert!(linked(&g, ore, ingot) && linked(&g, plate, goal));
    assert_eq!(g.node(ingot).unwrap().workspace, None);
    assert_eq!(solve(&catalog, &g).rate[&goal], flat.rate[&goal]);
}

#[test]
fn removing_a_workspace_removes_what_is_in_it() {
    let catalog = catalog();
    let (mut g, [ore, ingot, plate, goal]) = chain(&catalog);
    let inner = g.workspace_from(&[Member::Node(ingot)], "Ingots").unwrap();
    let outer = g
        .workspace_from(&[Member::Workspace(inner), Member::Node(plate)], "Plates")
        .unwrap();
    g.remove_workspace(outer);
    assert!(g.workspaces().is_empty());
    assert_eq!(g.nodes().keys().collect::<Vec<_>>(), [ore, goal]);
    assert!(g.links().is_empty());
}

#[test]
fn workspaces_and_ports_survive_a_save_file() {
    let catalog = catalog();
    let (mut g, [_, ingot, plate, goal]) = chain(&catalog);
    let ws = g
        .workspace_from(&[Member::Node(ingot), Member::Node(plate)], "Plates")
        .unwrap();
    let before = solve(&catalog, &g);

    let json = g.to_json("test");
    assert!(json.contains("\"format_version\": 2"));
    let loaded = Graph::from_json(&catalog, &json).unwrap().graph;
    let (loaded_ws, info) = loaded.workspaces().iter().next().expect("saved");
    assert_eq!(info.name, "Plates");
    assert_eq!(loaded.ports_of(loaded_ws, Side::Input).len(), 1);
    assert_eq!(loaded.nodes_in_workspace(loaded_ws).len(), 4);
    let after = solve(&catalog, &loaded);
    let goal_rate = |s: &aic_core::Solution, g: &Graph| {
        g.nodes()
            .iter()
            .find(|(_, n)| matches!(n.kind, NodeKind::Sink { .. }))
            .and_then(|(id, _)| s.rate.get(&id).copied().flatten())
    };
    assert_eq!(goal_rate(&after, &loaded), before.rate[&goal]);
    let _ = ws;
}

#[test]
fn a_version_1_group_opens_as_a_workspace_with_ports() {
    let catalog = catalog();
    let json = r#"{
      "format_version": 1,
      "data_version": "test",
      "region": "tundra",
      "nodes": [
        { "id": 0, "pos": { "x": 0, "y": 0 }, "kind": "source", "item": "ore", "limit": 30 },
        { "id": 1, "pos": { "x": 200, "y": 0 }, "group": 0,
          "kind": "recipe", "recipe": "ingot", "facility": "furnace" },
        { "id": 2, "pos": { "x": 400, "y": 0 }, "kind": "sink", "item": "ingot" }
      ],
      "links": [
        { "from": 0, "to": 1, "item": "ore" },
        { "from": 1, "to": 2, "item": "ingot" }
      ],
      "groups": [
        { "id": 0, "name": "Smelting", "collapsed": true, "pos": { "x": 200, "y": 0 } }
      ]
    }"#;
    let g = Graph::from_json(&catalog, json).unwrap().graph;
    let (ws, info) = g
        .workspaces()
        .iter()
        .next()
        .expect("the group is a workspace");
    assert_eq!(info.name, "Smelting");
    assert_eq!(g.ports_of(ws, Side::Input).len(), 1);
    assert_eq!(g.ports_of(ws, Side::Output).len(), 1);
    assert!(
        g.validate(&catalog).is_empty(),
        "{:?}",
        g.validate(&catalog)
    );
    let s = solve(&catalog, &g);
    let sink = g
        .nodes()
        .iter()
        .find(|(_, n)| matches!(n.kind, NodeKind::Sink { .. }))
        .map(|(id, _)| id)
        .unwrap();
    assert_near(s.rate[&sink], 30.0);
}

#[test]
fn copying_a_workspace_copies_what_is_in_it_and_pastes_into_another() {
    let catalog = catalog();
    let (mut g, [_, ingot, plate, _]) = chain(&catalog);
    let ws = g
        .workspace_from(&[Member::Node(ingot), Member::Node(plate)], "Plates")
        .unwrap();
    let copy = g.copy_members(&[Member::Workspace(ws)]);
    assert_eq!(copy.workspaces().len(), 1);
    assert_eq!(copy.nodes().len(), 4, "two recipes and two ports");
    assert_eq!(
        copy.links().len(),
        3,
        "the links inside; the outside ones stay behind"
    );

    let target = g.add_workspace("Target", None, Pos::default()).unwrap();
    let pasted = g.paste_into(&catalog, &copy, Pos { x: 10.0, y: 0.0 }, Some(target));
    let [Member::Workspace(new)] = pasted[..] else {
        panic!("one workspace pasted: {pasted:?}");
    };
    assert_eq!(g.workspace_info(new).unwrap().parent, Some(target));
    assert_eq!(g.nodes_in_workspace(new).len(), 4);
    assert!(g.validate(&catalog).is_empty());
}

#[test]
fn ports_are_left_out_when_pasted_at_the_top() {
    let catalog = catalog();
    let (mut g, [_, ingot, plate, _]) = chain(&catalog);
    let ws = g
        .workspace_from(&[Member::Node(ingot), Member::Node(plate)], "Plates")
        .unwrap();
    let port = g.ports_of(ws, Side::Input)[0];
    let copy = g.copy_members(&[Member::Node(port), Member::Node(ingot)]);
    let pasted = g.paste_into(&catalog, &copy, Pos::default(), None);
    assert_eq!(pasted.len(), 1, "only the recipe: a port needs a workspace");
}
