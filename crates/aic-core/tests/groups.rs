//! Groups, step 1: a named set of nodes and groups that collapses into one node on the canvas.
//! A view only: the solver sees the flat graph.

mod common;

use aic_core::{EditError, Graph, Member, NodeId, NodeKind, Port, PortRef, Side, solve};
use aic_data::Catalog;
use common::*;

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

fn port(node: NodeId, item_id: &str, side: Side) -> Port {
    Port {
        node,
        item: item(item_id),
        side,
    }
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

#[test]
fn a_groups_ports_are_its_members_ports_not_linked_inside_it() {
    let catalog = catalog();
    let (mut g, [_, ingot, plate, _]) = chain(&catalog);
    let group = g
        .group(&[Member::Node(ingot), Member::Node(plate)], "Plates")
        .unwrap();

    assert_eq!(g.group_info(group).unwrap().name, "Plates");
    assert_eq!(g.node(ingot).unwrap().group, Some(group));
    // The ingot link stays inside; ore comes in and plates go out, both linked outside.
    assert_eq!(
        g.external_ports(&catalog, group),
        [
            port(ingot, "ore", Side::Input),
            port(plate, "plate", Side::Output)
        ]
    );
}

#[test]
fn groups_nest_and_the_outermost_collapsed_one_shows() {
    let catalog = catalog();
    let (mut g, [ore, ingot, plate, _]) = chain(&catalog);
    let inner = g.group(&[Member::Node(ingot)], "Ingots").unwrap();
    let outer = g
        .group(&[Member::Group(inner), Member::Node(plate)], "Plates")
        .unwrap();
    assert_eq!(g.group_info(inner).unwrap().parent, Some(outer));
    assert_eq!(g.nodes_in_group(outer), [ingot, plate]);

    assert_eq!(g.shown_as(ingot), None, "groups start expanded");
    g.set_group_collapsed(inner, true).unwrap();
    assert_eq!(g.shown_as(ingot), Some(inner));
    g.set_group_collapsed(outer, true).unwrap();
    assert_eq!(g.shown_as(ingot), Some(outer));
    assert_eq!(g.shown_as(ore), None);
}

#[test]
fn only_members_of_one_group_can_be_grouped_together() {
    let catalog = catalog();
    let (mut g, [ore, ingot, plate, _]) = chain(&catalog);
    g.group(&[Member::Node(ingot), Member::Node(plate)], "Plates")
        .unwrap();
    assert_eq!(
        g.group(&[Member::Node(ore), Member::Node(ingot)], "Mixed"),
        Err(EditError::MixedGroups)
    );
    assert_eq!(g.group(&[], "Empty"), Err(EditError::EmptyGroup));
}

#[test]
fn ungrouping_hands_members_to_the_parent() {
    let catalog = catalog();
    let (mut g, [_, ingot, plate, _]) = chain(&catalog);
    let inner = g.group(&[Member::Node(ingot)], "Ingots").unwrap();
    let outer = g
        .group(&[Member::Group(inner), Member::Node(plate)], "Plates")
        .unwrap();
    g.ungroup(outer).unwrap();
    assert!(g.group_info(outer).is_none());
    assert_eq!(g.group_info(inner).unwrap().parent, None);
    assert_eq!(g.node(plate).unwrap().group, None);
    assert_eq!(g.node(ingot).unwrap().group, Some(inner));
}

#[test]
fn a_group_left_without_nodes_goes_away() {
    let catalog = catalog();
    let (mut g, [_, ingot, plate, _]) = chain(&catalog);
    let group = g
        .group(&[Member::Node(ingot), Member::Node(plate)], "Plates")
        .unwrap();
    g.remove_node(ingot);
    assert!(g.group_info(group).is_some());
    g.remove_node(plate);
    assert!(g.group_info(group).is_none());
}

#[test]
fn removing_a_group_with_its_members() {
    let catalog = catalog();
    let (mut g, [ore, ingot, plate, goal]) = chain(&catalog);
    let inner = g.group(&[Member::Node(ingot)], "Ingots").unwrap();
    let outer = g
        .group(&[Member::Group(inner), Member::Node(plate)], "Plates")
        .unwrap();
    g.remove_group(outer);
    assert!(g.groups().is_empty());
    assert_eq!(g.nodes().keys().collect::<Vec<_>>(), [ore, goal]);
    assert!(g.links().is_empty());
}

#[test]
fn groups_are_a_view_only_and_survive_a_save_file() {
    let catalog = catalog();
    let (mut g, [_, ingot, plate, goal]) = chain(&catalog);
    let flat = solve(&catalog, &g);
    let group = g
        .group(&[Member::Node(ingot), Member::Node(plate)], "Plates")
        .unwrap();
    g.set_group_collapsed(group, true).unwrap();
    let grouped = solve(&catalog, &g);
    assert_eq!(grouped.rate[&goal], flat.rate[&goal]);
    assert_eq!(grouped.facility_count, flat.facility_count);

    let loaded = Graph::from_json(&catalog, &g.to_json("test"))
        .unwrap()
        .graph;
    let (loaded_group, info) = loaded.groups().iter().next().expect("the group is saved");
    assert_eq!(info.name, "Plates");
    assert!(info.collapsed);
    assert_eq!(loaded.nodes_in_group(loaded_group).len(), 2);
}
