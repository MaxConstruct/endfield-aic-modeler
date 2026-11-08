//! A node's limit typed as a flow through one of its ports (Satisfactory Modeler's "part limit").
//! It is the same limit: on a recipe node it becomes a facility count limit.

mod common;

use aic_core::{EditError, Graph, NodeKind, Port, Side};
use aic_data::Catalog;
use common::*;

/// 30 ore → 30 ingot per facility per minute; 30 ingot → 15 plate.
fn catalog() -> Catalog {
    TestCatalog::new()
        .facility("furnace", 5.0)
        .recipe("ingot", "furnace", 2.0, &[("ore", 1.0)], &[("ingot", 1.0)])
        .recipe(
            "plate",
            "furnace",
            2.0,
            &[("ingot", 1.0)],
            &[("plate", 0.5)],
        )
        .build()
}

fn port(node: aic_core::NodeId, item_id: &str, side: Side) -> Port {
    Port {
        node,
        item: item(item_id),
        side,
    }
}

fn limit_of(g: &Graph, node: aic_core::NodeId) -> Option<f64> {
    match g.node(node).map(|n| &n.kind) {
        Some(NodeKind::Recipe { limit, .. } | NodeKind::Sink { limit, .. }) => *limit,
        Some(NodeKind::Source { limit, .. } | NodeKind::ControlPort { limit, .. }) => Some(*limit),
        _ => None,
    }
}

#[test]
fn an_output_limit_becomes_a_facility_count_limit() {
    let catalog = catalog();
    let mut g = Graph::new(region(TUNDRA));
    let plate = add_recipe(&mut g, &catalog, "plate", None);

    g.set_port_limit(&catalog, &port(plate, "plate", Side::Output), Some(45.0))
        .unwrap();
    assert_eq!(limit_of(&g, plate), Some(3.0)); // 45 / 15 per facility
    assert_eq!(
        g.port_limit(&catalog, &port(plate, "plate", Side::Output)),
        Some(45.0)
    );
    // The same limit seen from the input side.
    assert_eq!(
        g.port_limit(&catalog, &port(plate, "ingot", Side::Input)),
        Some(90.0)
    );
}

#[test]
fn an_input_limit_uses_the_input_rate() {
    let catalog = catalog();
    let mut g = Graph::new(region(TUNDRA));
    let plate = add_recipe(&mut g, &catalog, "plate", Some(1.0));

    g.set_port_limit(&catalog, &port(plate, "ingot", Side::Input), Some(60.0))
        .unwrap();
    assert_eq!(limit_of(&g, plate), Some(2.0));
}

#[test]
fn clearing_a_port_limit_clears_the_nodes_limit() {
    let catalog = catalog();
    let mut g = Graph::new(region(TUNDRA));
    let ingot = add_recipe(&mut g, &catalog, "ingot", Some(2.0));

    g.set_port_limit(&catalog, &port(ingot, "ingot", Side::Output), None)
        .unwrap();
    assert_eq!(limit_of(&g, ingot), None);
    assert_eq!(
        g.port_limit(&catalog, &port(ingot, "ingot", Side::Output)),
        None
    );
}

#[test]
fn sources_sinks_and_control_ports_take_the_rate_as_is() {
    let catalog = catalog();
    let mut g = Graph::new(region(TUNDRA));
    let ore = add(
        &mut g,
        NodeKind::Source {
            item: item("ore"),
            limit: 30.0,
        },
    );
    let valve = add(
        &mut g,
        NodeKind::ControlPort {
            item: Some(item("ore")),
            limit: 30.0,
        },
    );

    g.set_port_limit(&catalog, &port(ore, "ore", Side::Output), Some(75.0))
        .unwrap();
    assert_eq!(limit_of(&g, ore), Some(75.0));
    g.set_port_limit(&catalog, &port(valve, "ore", Side::Input), Some(20.0))
        .unwrap();
    assert_eq!(limit_of(&g, valve), Some(20.0));
    assert_eq!(
        g.set_port_limit(&catalog, &port(ore, "ore", Side::Output), None),
        Err(EditError::LimitRequired)
    );
}

#[test]
fn a_port_the_node_lacks_or_a_node_without_limits_is_refused() {
    let catalog = catalog();
    let mut g = Graph::new(region(TUNDRA));
    let ingot = add_recipe(&mut g, &catalog, "ingot", None);
    let splitter = add(&mut g, NodeKind::Splitter { item: None });

    assert_eq!(
        g.set_port_limit(&catalog, &port(ingot, "plate", Side::Output), Some(1.0)),
        Err(EditError::NoSuchPort)
    );
    assert_eq!(
        g.set_port_limit(&catalog, &port(splitter, "ore", Side::Output), Some(1.0)),
        Err(EditError::NoLimit)
    );
}
