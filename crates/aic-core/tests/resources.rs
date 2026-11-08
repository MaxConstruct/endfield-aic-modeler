//! Raw resource use and the availability check (docs/PLAN.md, M4).

mod common;

use std::collections::HashMap;

use aic_core::{Graph, NodeKind, Warning, check_resources, solve};
use aic_data::{Catalog, ItemId};
use common::*;

/// Ore is mined (30 per minute per miner); ingot is not a raw resource.
fn catalog() -> Catalog {
    TestCatalog::new()
        .facility("miner", 5.0)
        .facility("furnace", 5.0)
        .mining("ore", "miner", 2.0, ("ore", 1.0))
        .recipe("ingot", "furnace", 2.0, &[("ore", 1.0)], &[("ingot", 1.0)])
        .build()
}

fn used(totals: &aic_core::Totals, item_id: &str) -> f64 {
    totals.raw_resource_use.get(item_id).copied().unwrap_or(0.0)
}

#[test]
fn use_counts_mining_output_source_supply_and_depot_pulls() {
    let catalog = catalog();
    let mut g = Graph::new(region(TUNDRA));
    // A miner on the depot: 60 per minute.
    add_recipe(&mut g, &catalog, "ore", Some(2.0));
    // A source of ore: 15 per minute.
    add(
        &mut g,
        NodeKind::Source {
            item: item("ore"),
            limit: 15.0,
        },
    );
    // A furnace pulling 30 ore per minute from the depot.
    add_recipe(&mut g, &catalog, "ingot", Some(1.0));

    let s = solve(&catalog, &g);
    assert!((used(&s.totals, "ore") - 105.0).abs() < 1e-6);
    // Only raw resources are counted.
    assert_eq!(s.totals.raw_resource_use.len(), 1);
}

#[test]
fn a_mined_item_fed_through_a_link_is_counted_once() {
    let catalog = catalog();
    let mut g = Graph::new(region(TUNDRA));
    let miner = add_recipe(&mut g, &catalog, "ore", Some(1.0));
    let furnace = add_recipe(&mut g, &catalog, "ingot", None);
    link(&mut g, &catalog, miner, furnace, "ore");

    let s = solve(&catalog, &g);
    assert!((used(&s.totals, "ore") - 30.0).abs() < 1e-6);
}

#[test]
fn going_over_availability_warns() {
    let catalog = catalog();
    let mut g = Graph::new(region(TUNDRA));
    add_recipe(&mut g, &catalog, "ore", Some(2.0)); // 60 per minute
    let s = solve(&catalog, &g);

    let available = HashMap::from([(ItemId::from("ore"), 45.0)]);
    assert_eq!(
        check_resources(&s.totals, &available),
        vec![Warning::ResourceOverUse {
            item: item("ore"),
            used: 60.0,
            available: 45.0,
        }]
    );
    let plenty = HashMap::from([(ItemId::from("ore"), 60.0)]);
    assert_eq!(check_resources(&s.totals, &plenty), vec![]);
    // No entry means unlimited.
    assert_eq!(check_resources(&s.totals, &HashMap::new()), vec![]);
}
