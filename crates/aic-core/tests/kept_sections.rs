//! M12: a solve keeps the result of each section that did not change since the last one, and
//! gives exactly what a full solve gives (docs/PLAN.md, M12 step 2).

mod common;

use aic_core::{Graph, KeptSections, NodeId, NodeKind, Solution, solve, solve_keeping};
use aic_data::Catalog;
use common::*;

/// A fan-out (30 x split between two consumers, which it throttles) and an ingot chain into a
/// sink: two sections.
fn catalog() -> Catalog {
    TestCatalog::new()
        .facility("miner", 5.0)
        .facility("press", 5.0)
        .facility("furnace", 5.0)
        .recipe("a", "miner", 2.0, &[], &[("x", 1.0)])
        .recipe("b", "press", 3.0, &[("x", 1.0)], &[("bx", 1.0)])
        .recipe("c", "press", 6.0, &[("x", 1.0)], &[("cx", 1.0)])
        .recipe("ingot", "furnace", 2.0, &[("ore", 1.0)], &[("ingot", 1.0)])
        .build()
}

struct Plan {
    graph: Graph,
    /// The fan-out's producer, and the ingot chain's recipe node and sink.
    a: NodeId,
    ingot: NodeId,
    goal: NodeId,
}

fn plan(catalog: &Catalog) -> Plan {
    let mut graph = Graph::new(region(TUNDRA));
    let a = add_recipe(&mut graph, catalog, "a", Some(1.0));
    let b = add_recipe(&mut graph, catalog, "b", Some(5.0));
    let c = add_recipe(&mut graph, catalog, "c", Some(5.0));
    link(&mut graph, catalog, a, b, "x");
    link(&mut graph, catalog, a, c, "x");
    let ingot = add_recipe(&mut graph, catalog, "ingot", None);
    let goal = add(
        &mut graph,
        NodeKind::Sink {
            item: item("ingot"),
            limit: Some(10.0),
        },
    );
    link(&mut graph, catalog, ingot, goal, "ingot");
    Plan {
        graph,
        a,
        ingot,
        goal,
    }
}

/// Every part of two solutions is the same, to the bit.
#[track_caller]
fn assert_same(kept: &Solution, full: &Solution) {
    assert_eq!(kept.sections, full.sections, "sections");
    assert_eq!(kept.facility_count, full.facility_count, "facility counts");
    assert_eq!(kept.rate, full.rate, "rates");
    assert_eq!(kept.link_flow, full.link_flow, "link flows");
    assert_eq!(kept.link_belts, full.link_belts, "link belts");
    assert_eq!(kept.port_flow, full.port_flow, "port flows");
    assert_eq!(kept.depot, full.depot, "depot");
    assert_eq!(kept.storage, full.storage, "storage");
    assert_eq!(kept.coupons, full.coupons, "coupons");
    assert_eq!(kept.throttle, full.throttle, "throttle reasons");
    assert_eq!(kept.totals, full.totals, "totals");
    assert_eq!(kept.warnings, full.warnings, "warnings");
}

#[test]
fn a_section_that_did_not_change_is_kept_and_gives_what_a_full_solve_gives() {
    let catalog = catalog();
    let mut plan = plan(&catalog);
    let mut kept = KeptSections::default();
    solve_keeping(&catalog, &plan.graph, &mut kept);

    // Only the ingot chain changes.
    plan.graph.set_limit(plan.goal, Some(20.0)).unwrap();
    let again = solve_keeping(&catalog, &plan.graph, &mut kept);

    assert_eq!(kept.kept(), 1, "the fan-out is kept");
    assert_same(&again, &solve(&catalog, &plan.graph));
    assert_near(again.facility_count[&plan.ingot], 20.0 / 30.0);
    assert_near(again.facility_count[&plan.a], 1.0);
    assert_eq!(again.throttle.len(), 2, "both consumers of the fan-out");
}

#[test]
fn a_section_whose_limit_changed_is_solved_again() {
    let catalog = catalog();
    let mut plan = plan(&catalog);
    let mut kept = KeptSections::default();
    let first = solve_keeping(&catalog, &plan.graph, &mut kept);
    assert_near(first.facility_count[&plan.a], 1.0);

    plan.graph.set_limit(plan.a, Some(0.5)).unwrap();
    let again = solve_keeping(&catalog, &plan.graph, &mut kept);

    assert_eq!(kept.kept(), 1, "the ingot chain is kept");
    assert_near(again.facility_count[&plan.a], 0.5);
    assert_same(&again, &solve(&catalog, &plan.graph));
}

#[test]
fn nothing_changed_keeps_every_section() {
    let catalog = catalog();
    let plan = plan(&catalog);
    let mut kept = KeptSections::default();
    let first = solve_keeping(&catalog, &plan.graph, &mut kept);
    assert_eq!(kept.kept(), 0, "nothing to keep the first time");
    let again = solve_keeping(&catalog, &plan.graph, &mut kept);
    assert_eq!(kept.kept(), 2);
    assert_same(&again, &first);
}

#[test]
fn a_section_that_gained_a_node_is_solved_again() {
    let catalog = catalog();
    let mut plan = plan(&catalog);
    let mut kept = KeptSections::default();
    solve_keeping(&catalog, &plan.graph, &mut kept);

    // A merger linked to the ingot chain becomes part of its section.
    let merger = add(&mut plan.graph, NodeKind::Merger { item: None });
    link(&mut plan.graph, &catalog, plan.ingot, merger, "ingot");
    let again = solve_keeping(&catalog, &plan.graph, &mut kept);
    assert_eq!(again.sections.len(), 2);
    assert_eq!(
        kept.kept(),
        1,
        "the fan-out is kept, the grown section is not"
    );
    assert_same(&again, &solve(&catalog, &plan.graph));
}

#[test]
fn removing_a_section_keeps_the_others_with_their_new_places() {
    let catalog = catalog();
    let mut plan = plan(&catalog);
    // A lone sink with no limit: a section of its own, undetermined, with a warning that names
    // its place among the sections.
    add(
        &mut plan.graph,
        NodeKind::Sink {
            item: item("ingot"),
            limit: None,
        },
    );
    let mut kept = KeptSections::default();
    solve_keeping(&catalog, &plan.graph, &mut kept);

    // The fan-out falls apart into its two consumers, each a section of its own.
    plan.graph.remove_node(plan.a);
    let again = solve_keeping(&catalog, &plan.graph, &mut kept);
    assert_eq!(kept.kept(), 2, "the ingot chain and the lone sink");
    assert_same(&again, &solve(&catalog, &plan.graph));
}

#[track_caller]
fn assert_near(actual: Option<f64>, expected: f64) {
    let actual = actual.expect("value is determined");
    assert!(
        (actual - expected).abs() < 1e-6,
        "expected {expected}, got {actual}"
    );
}
