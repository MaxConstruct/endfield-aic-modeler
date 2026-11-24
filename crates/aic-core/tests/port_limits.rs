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

/// The Solid-Gas Transmuting Unit's v1.5.3 recipes: each running unit needs 6 Xiragen a minute,
/// whatever its throughput (ADR 0013).
fn transmuting_catalog() -> Catalog {
    TestCatalog::new()
        .facility("phase_trans_2", 50.0)
        // 30 Heavy Xiragen + 6 Xiragen → 12 Heavy Xiranite per minute; all 6 Xiragen is activation.
        .recipe(
            "enr_powder",
            "phase_trans_2",
            10.0,
            &[("gas_xiranite_enr", 5.0)],
            &[("xiranite_enr_powder", 2.0)],
        )
        // 36 Xiragen → 30 Xiranite per minute: 30 Xiragen scales, 6 is activation.
        .recipe(
            "powder",
            "phase_trans_2",
            2.0,
            &[("gas_xiranite", 1.0)],
            &[("xiranite_powder", 1.0)],
        )
        // On top of the inputs above: 1 more Xiragen per 10 s craft, 0.2 per 2 s craft.
        .activation("phase_trans_2", "gas_xiranite", 6.0)
        .build()
}

#[test]
fn a_limit_on_an_activation_port_runs_the_facilities_it_covers() {
    let catalog = transmuting_catalog();
    let mut g = Graph::new(region(TUNDRA));
    let unit = add_recipe(&mut g, &catalog, "enr_powder", None);
    let xiragen = port(unit, "gas_xiranite", Side::Input);
    // 6 per running unit, however busy: 6 runs one, 12 two, 3 none.
    for (rate, count) in [(6.0, 1.0), (12.0, 2.0), (3.0, 0.0), (17.0, 2.0)] {
        g.set_port_limit(&catalog, &xiragen, Some(rate)).unwrap();
        assert_eq!(limit_of(&g, unit), Some(count), "{rate}/min");
    }
}

#[test]
fn a_limit_on_a_mixed_port_counts_the_activation_once_per_running_facility() {
    let catalog = transmuting_catalog();
    let mut g = Graph::new(region(TUNDRA));
    let unit = add_recipe(&mut g, &catalog, "powder", None);
    let xiragen = port(unit, "gas_xiranite", Side::Input);
    // The largest x with 30x + 6·ceil(x) ≤ rate.
    let cases = [
        (21.0, 0.5), // one unit: 30 × 0.5 + 6
        (36.0, 1.0), // one unit flat out
        (40.0, 1.0), // a second unit needs 12 of activation, leaving 28 < 30 for more than 1
        (48.0, 1.2), // two units: 30 × 1.2 + 12
        (6.0, 0.0),  // the activation alone makes nothing
    ];
    for (rate, count) in cases {
        g.set_port_limit(&catalog, &xiragen, Some(rate)).unwrap();
        let limit = limit_of(&g, unit).unwrap();
        assert!((limit - count).abs() < 1e-9, "{rate}/min: {limit}");
    }
}

#[test]
fn a_count_limit_shows_on_an_activation_port_as_the_running_facilities_need() {
    let catalog = transmuting_catalog();
    let mut g = Graph::new(region(TUNDRA));
    let enr = add_recipe(&mut g, &catalog, "enr_powder", Some(0.5));
    let powder = add_recipe(&mut g, &catalog, "powder", Some(1.5));
    let shown = |g: &Graph, node, item_id: &str| {
        g.port_limit(&catalog, &port(node, item_id, Side::Input))
            .unwrap()
    };
    // Half a unit still needs one unit's 6; its other input scales.
    assert_eq!(shown(&g, enr, "gas_xiranite"), 6.0);
    assert_eq!(shown(&g, enr, "gas_xiranite_enr"), 15.0);
    // 1.5 units run two: 30 × 1.5 + 12.
    assert_eq!(shown(&g, powder, "gas_xiranite"), 57.0);
    // Typing back what is shown keeps the limit.
    g.set_port_limit(
        &catalog,
        &port(powder, "gas_xiranite", Side::Input),
        Some(57.0),
    )
    .unwrap();
    assert_eq!(limit_of(&g, powder), Some(1.5));
}
