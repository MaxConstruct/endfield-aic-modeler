//! Copying part of a graph and pasting it back: the nodes, and the links between them only.

mod common;

use aic_core::{Graph, NodeKind, PortRef, Pos};
use common::*;

fn catalog() -> aic_data::Catalog {
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

#[test]
fn a_copy_keeps_the_links_inside_it_and_drops_the_rest() {
    let catalog = catalog();
    let mut g = Graph::new(region(TUNDRA));
    let ore = add(
        &mut g,
        NodeKind::Source {
            item: item("ore"),
            limit: 30.0,
        },
    );
    let ingot = add_recipe(&mut g, &catalog, "ingot", Some(1.0));
    let plate = add_recipe(&mut g, &catalog, "plate", None);
    g.connect(&catalog, PortRef::any(ore), PortRef::new(ingot, "ore"))
        .unwrap();
    g.connect(
        &catalog,
        PortRef::new(ingot, "ingot"),
        PortRef::new(plate, "ingot"),
    )
    .unwrap();

    let copy = g.copy_nodes(&[ingot, plate]);
    assert_eq!(copy.nodes().len(), 2);
    assert_eq!(copy.links().len(), 1, "the source's link stays behind");
    assert_eq!(copy.region(), g.region());
}

#[test]
fn pasting_adds_new_nodes_moved_by_the_offset_and_links_them() {
    let catalog = catalog();
    let mut g = Graph::new(region(TUNDRA));
    let ingot = g.add_node(
        NodeKind::Recipe {
            recipe: "ingot".into(),
            facility: "furnace".into(),
            limit: Some(2.0),
        },
        Pos { x: 10.0, y: 20.0 },
    );
    let plate = add_recipe(&mut g, &catalog, "plate", None);
    g.connect(
        &catalog,
        PortRef::new(ingot, "ingot"),
        PortRef::new(plate, "ingot"),
    )
    .unwrap();
    let copy = g.copy_nodes(&[ingot, plate]);

    let pasted = g.paste(&catalog, &copy, Pos { x: 40.0, y: 40.0 });
    assert_eq!(pasted.len(), 2);
    assert!(pasted.iter().all(|id| *id != ingot && *id != plate));
    assert_eq!(g.nodes().len(), 4);
    assert_eq!(g.links().len(), 2);
    let new_ingot = pasted
        .iter()
        .map(|id| g.node(*id).unwrap())
        .find(|node| node.kind.limit() == Some(2.0))
        .expect("the limit is copied");
    assert_eq!(new_ingot.pos, Pos { x: 50.0, y: 60.0 });
    let new_link = g
        .links()
        .values()
        .find(|link| pasted.contains(&link.from))
        .expect("the copy is linked");
    assert!(pasted.contains(&new_link.to));
}

#[test]
fn a_pasted_logistics_node_keeps_its_item_only_while_linked() {
    let catalog = catalog();
    let mut g = Graph::new(region(TUNDRA));
    let ingot = add_recipe(&mut g, &catalog, "ingot", Some(1.0));
    let splitter = add(&mut g, NodeKind::Splitter { item: None });
    g.connect(
        &catalog,
        PortRef::new(ingot, "ingot"),
        PortRef::any(splitter),
    )
    .unwrap();

    // Copied alone, the splitter loses its only link, so it forgets its item.
    let copy = g.copy_nodes(&[splitter]);
    let pasted = g.paste(&catalog, &copy, Pos::default());
    assert_eq!(
        g.node(pasted[0]).unwrap().kind,
        NodeKind::Splitter { item: None }
    );
}
