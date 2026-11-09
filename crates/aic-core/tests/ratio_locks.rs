//! Ratio locks: a fixed split the player sets on a fan-out or fan-in, replacing the even split.
//! Each link has a share at each end; links with a share at the same port keep their flows in
//! proportion to their shares.

mod common;

use aic_core::{EditError, Graph, LinkId, NodeId, NodeKind, PortRef, Side, Solution, solve};
use aic_data::Catalog;
use common::*;

const TOLERANCE: f64 = 1e-6;

fn catalog() -> Catalog {
    TestCatalog::new()
        .facility("furnace", 5.0)
        .recipe("ingot", "furnace", 2.0, &[("ore", 1.0)], &[("ingot", 1.0)])
        .build()
}

fn source(g: &mut Graph, limit: f64) -> NodeId {
    add(
        g,
        NodeKind::Source {
            item: item("ore"),
            limit,
        },
    )
}

fn sink(g: &mut Graph, limit: Option<f64>) -> NodeId {
    add(
        g,
        NodeKind::Sink {
            item: item("ore"),
            limit,
        },
    )
}

fn link(g: &mut Graph, catalog: &Catalog, from: NodeId, to: NodeId) -> LinkId {
    g.connect(catalog, PortRef::any(from), PortRef::any(to))
        .unwrap()
}

#[track_caller]
fn assert_flow(solution: &Solution, link: LinkId, expected: f64) {
    let actual = solution.link_flow[&link].expect("flow is determined");
    assert!(
        (actual - expected).abs() < TOLERANCE,
        "expected {expected}, got {actual}"
    );
}

#[test]
fn a_locked_fan_out_splits_by_its_shares() {
    let catalog = catalog();
    let mut g = Graph::new(region(TUNDRA));
    let ore = source(&mut g, 60.0);
    let (a, b) = (sink(&mut g, None), sink(&mut g, None));
    let (to_a, to_b) = (
        link(&mut g, &catalog, ore, a),
        link(&mut g, &catalog, ore, b),
    );

    let even = solve(&catalog, &g);
    assert_flow(&even, to_a, 30.0);
    assert_flow(&even, to_b, 30.0);

    g.set_share(to_a, Side::Output, Some(2.0)).unwrap();
    g.set_share(to_b, Side::Output, Some(1.0)).unwrap();
    let locked = solve(&catalog, &g);
    assert_flow(&locked, to_a, 40.0);
    assert_flow(&locked, to_b, 20.0);
}

#[test]
fn a_lock_holds_even_when_it_costs_throughput() {
    let catalog = catalog();
    let mut g = Graph::new(region(TUNDRA));
    let ore = source(&mut g, 60.0);
    let (small, big) = (sink(&mut g, Some(10.0)), sink(&mut g, None));
    let (to_small, to_big) = (
        link(&mut g, &catalog, ore, small),
        link(&mut g, &catalog, ore, big),
    );
    g.set_share(to_small, Side::Output, Some(1.0)).unwrap();
    g.set_share(to_big, Side::Output, Some(1.0)).unwrap();

    let s = solve(&catalog, &g);
    assert_flow(&s, to_small, 10.0);
    assert_flow(&s, to_big, 10.0);
    assert_eq!(s.rate[&ore], Some(20.0));
    assert!(
        s.throttle.contains_key(&ore),
        "the source runs below its limit"
    );
}

#[test]
fn a_locked_fan_in_draws_by_its_shares() {
    let catalog = catalog();
    let mut g = Graph::new(region(TUNDRA));
    let (first, second) = (source(&mut g, 100.0), source(&mut g, 100.0));
    let goal = sink(&mut g, Some(60.0));
    let (from_first, from_second) = (
        link(&mut g, &catalog, first, goal),
        link(&mut g, &catalog, second, goal),
    );
    g.set_share(from_first, Side::Input, Some(2.0)).unwrap();
    g.set_share(from_second, Side::Input, Some(1.0)).unwrap();

    let s = solve(&catalog, &g);
    assert_flow(&s, from_first, 40.0);
    assert_flow(&s, from_second, 20.0);
}

#[test]
fn links_without_a_share_stay_free() {
    let catalog = catalog();
    let mut g = Graph::new(region(TUNDRA));
    let ore = source(&mut g, 90.0);
    let (a, b, c) = (
        sink(&mut g, None),
        sink(&mut g, Some(10.0)),
        sink(&mut g, None),
    );
    let to_a = link(&mut g, &catalog, ore, a);
    let to_b = link(&mut g, &catalog, ore, b);
    let to_c = link(&mut g, &catalog, ore, c);
    g.set_share(to_a, Side::Output, Some(3.0)).unwrap();
    g.set_share(to_b, Side::Output, Some(1.0)).unwrap();

    let s = solve(&catalog, &g);
    assert_flow(&s, to_a, 30.0);
    assert_flow(&s, to_b, 10.0);
    assert_flow(&s, to_c, 50.0);
}

#[test]
fn shares_must_be_positive_and_survive_copies_and_files() {
    let catalog = catalog();
    let mut g = Graph::new(region(TUNDRA));
    let ore = source(&mut g, 60.0);
    let goal = sink(&mut g, None);
    let to_goal = link(&mut g, &catalog, ore, goal);
    for bad in [0.0, -1.0, f64::NAN] {
        assert!(matches!(
            g.set_share(to_goal, Side::Output, Some(bad)),
            Err(EditError::InvalidShare(_))
        ));
    }
    g.set_share(to_goal, Side::Output, Some(70.0)).unwrap();
    assert_eq!(g.link(to_goal).unwrap().share(Side::Output), Some(70.0));

    let copy = copy_nodes(&g, &[ore, goal]);
    assert_eq!(
        copy.links().values().next().unwrap().share(Side::Output),
        Some(70.0)
    );
    let loaded = Graph::from_json(&catalog, &g.to_json("test"))
        .unwrap()
        .graph;
    let loaded_link = loaded.links().values().next().unwrap();
    assert_eq!(loaded_link.share(Side::Output), Some(70.0));
    assert_eq!(loaded_link.share(Side::Input), None);
}
