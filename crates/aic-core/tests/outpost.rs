//! Outpost orders: sinks for settlement trades. Items go in; the coupons they earn count in totals.

mod common;

use aic_core::{Graph, Issue, NodeKind, PortRef, Warning, solve};
use aic_data::Catalog;
use common::*;

fn catalog() -> Catalog {
    TestCatalog::new()
        .facility("furnace", 5.0)
        .facility("outpost", 0.0)
        .recipe("ingot", "furnace", 2.0, &[("ore", 1.0)], &[("ingot", 1.0)])
        .outpost_order("coupon-ingot", "outpost", "ingot", "coupon", 3.0)
        .build()
}

fn ingot_order(catalog: &Catalog, limit: Option<f64>) -> NodeKind {
    let mut kind = NodeKind::outpost_order(catalog, &"coupon-ingot".into()).expect("a trade");
    if let NodeKind::OutpostOrder { limit: slot, .. } = &mut kind {
        *slot = limit;
    }
    kind
}

#[test]
fn an_order_takes_its_item_from_the_trade_recipe() {
    let catalog = catalog();
    assert_eq!(
        NodeKind::outpost_order(&catalog, &"coupon-ingot".into()),
        Some(NodeKind::OutpostOrder {
            recipe: "coupon-ingot".into(),
            item: item("ingot"),
            limit: None,
        })
    );
    assert_eq!(
        NodeKind::outpost_order(&catalog, &"ingot".into()),
        None,
        "not a trade"
    );
}

#[test]
fn an_order_consumes_like_a_sink_and_earns_coupons() {
    let catalog = catalog();
    let mut g = Graph::new(region(TUNDRA));
    let furnace = add_recipe(&mut g, &catalog, "ingot", Some(1.0)); // 30 ingot/min
    let order = add(&mut g, ingot_order(&catalog, Some(20.0)));
    g.connect(
        &catalog,
        PortRef::new(furnace, "ingot"),
        PortRef::any(order),
    )
    .unwrap();

    let solution = solve(&catalog, &g);
    assert_eq!(solution.rate[&order], Some(20.0));
    assert_eq!(solution.totals.coupons[&item("coupon")], 60.0);
    assert!(
        solution.totals.depot_out.is_empty(),
        "coupons don't go to the depot"
    );
}

#[test]
fn a_trade_missing_from_the_data_is_left_out() {
    let catalog = catalog();
    let mut g = Graph::new(region(TUNDRA));
    let order = add(
        &mut g,
        NodeKind::OutpostOrder {
            recipe: "gone".into(),
            item: item("ingot"),
            limit: None,
        },
    );
    let solution = solve(&catalog, &g);
    assert!(
        solution
            .warnings
            .contains(&Warning::Issue(Issue::MissingRecipe {
                node: order,
                recipe: "gone".into(),
            }))
    );
    assert!(!solution.rate.contains_key(&order));
}

#[test]
fn orders_survive_a_save_file() {
    let catalog = catalog();
    let mut g = Graph::new(region(TUNDRA));
    add(&mut g, ingot_order(&catalog, Some(5.0)));
    let loaded = Graph::from_json(&catalog, &g.to_json("test"))
        .unwrap()
        .graph;
    let kind = &loaded.nodes().values().next().unwrap().kind;
    assert_eq!(*kind, ingot_order(&catalog, Some(5.0)));
}
