//! Activation inputs (ADR 0013): a facility that needs an input at a fixed rate for each facility
//! that is running, whatever its throughput. A node at count `x` needs `6 × ceil(x)` per minute of
//! it on top of what scales with `x`.
//!
//! The recipes are the v1.5.3 ones of the Solid-Gas Transmuting Unit (`phase_trans_2`), whose
//! activation input is 6 Xiragen (`gas_xiranite`) per minute. Values are worked by hand.

mod common;

use aic_core::{Graph, NodeId, NodeKind, Port, RoundUp, Side, Solution, solve};
use aic_data::Catalog;
use common::*;

const TOLERANCE: f64 = 1e-6;

const UNIT: &str = "phase_trans_2";
const ENR_POWDER: &str = "phase_trans_2-xiranite_enr_powder";
const POWDER: &str = "phase_trans_2-xiranite_powder";
const HEAVY_XIRAGEN: &str = "gas_xiranite_enr";
const XIRAGEN: &str = "gas_xiranite";
const HEAVY_XIRANITE: &str = "xiranite_enr_powder";
const XIRANITE: &str = "xiranite_powder";

fn catalog() -> Catalog {
    TestCatalog::new()
        .facility_with(UNIT, 1.0, 50.0, &[JINLONG])
        // 5 Heavy Xiragen + 1 Xiragen → 2 Heavy Xiranite per 10 s: 30 + 6 → 12 per minute. All
        // 6 Xiragen is the activation input; none of it scales.
        .recipe(
            ENR_POWDER,
            UNIT,
            10.0,
            &[(HEAVY_XIRAGEN, 5.0), (XIRAGEN, 1.0)],
            &[(HEAVY_XIRANITE, 2.0)],
        )
        // 1.2 Xiragen → 1 Xiranite per 2 s: 36 → 30 per minute. 30 Xiragen scales, 6 doesn't.
        .recipe(POWDER, UNIT, 2.0, &[(XIRAGEN, 1.2)], &[(XIRANITE, 1.0)])
        .activation(UNIT, XIRAGEN, 6.0)
        .build()
}

fn graph() -> Graph {
    Graph::new(region(JINLONG))
}

#[track_caller]
fn assert_near(actual: Option<f64>, expected: f64) {
    let actual = actual.expect("value is determined");
    assert!(
        (actual - expected).abs() < TOLERANCE,
        "expected {expected}, got {actual}"
    );
}

fn count(s: &Solution, node: NodeId) -> Option<f64> {
    *s.facility_count.get(&node).expect("node has a count")
}

fn port_flow(s: &Solution, node: NodeId, item_id: &str, side: Side) -> Option<f64> {
    let port = Port {
        node,
        item: item(item_id),
        side,
    };
    *s.port_flow
        .get(&port)
        .unwrap_or_else(|| panic!("{port:?} has a flow"))
}

fn source(g: &mut Graph, item_id: &str, limit: f64) -> NodeId {
    add(
        g,
        NodeKind::Source {
            item: item(item_id),
            limit,
        },
    )
}

fn sink(g: &mut Graph, item_id: &str, limit: Option<f64>) -> NodeId {
    add(
        g,
        NodeKind::Sink {
            item: item(item_id),
            limit,
        },
    )
}

#[test]
fn a_unit_needs_its_whole_activation_input_whatever_its_count() {
    let catalog = catalog();
    // (limit, Heavy Xiragen in, Xiragen in, Heavy Xiranite out), all on the depot.
    let cases = [
        // Half a unit: half of 30 and of 12, but one running unit's whole 6.
        (0.5, 15.0, 6.0, 6.0),
        (1.0, 30.0, 6.0, 12.0),
        // 1.5 units are two running: 2 × 6.
        (1.5, 45.0, 12.0, 18.0),
        (2.0, 60.0, 12.0, 24.0),
        // Nothing runs, so nothing is needed.
        (0.0, 0.0, 0.0, 0.0),
    ];
    for (limit, heavy_xiragen, xiragen, heavy_xiranite) in cases {
        let mut g = graph();
        let unit = add_recipe(&mut g, &catalog, ENR_POWDER, Some(limit));
        let s = solve(&catalog, &g);
        assert_near(count(&s, unit), limit);
        assert_near(
            port_flow(&s, unit, HEAVY_XIRAGEN, Side::Input),
            heavy_xiragen,
        );
        assert_near(port_flow(&s, unit, XIRAGEN, Side::Input), xiragen);
        assert_near(
            port_flow(&s, unit, HEAVY_XIRANITE, Side::Output),
            heavy_xiranite,
        );
        assert_near(
            s.depot
                .get(&port(unit, XIRAGEN, Side::Input))
                .copied()
                .flatten(),
            xiragen,
        );
    }
}

#[test]
fn only_the_activation_part_of_a_mixed_input_stays_whole() {
    let catalog = catalog();
    let mut g = graph();
    let unit = add_recipe(&mut g, &catalog, POWDER, Some(0.5));
    let s = solve(&catalog, &g);
    // Half of the 30 that scales, plus one running unit's 6.
    assert_near(port_flow(&s, unit, XIRAGEN, Side::Input), 21.0);
    assert_near(port_flow(&s, unit, XIRANITE, Side::Output), 15.0);
}

#[test]
fn a_unit_whose_activation_input_falls_short_does_not_run() {
    let catalog = catalog();
    let mut g = graph();
    let xiragen = source(&mut g, XIRAGEN, 5.0);
    let unit = add_recipe(&mut g, &catalog, ENR_POWDER, Some(2.0));
    let feed = link(&mut g, &catalog, xiragen, unit, XIRAGEN);
    let s = solve(&catalog, &g);
    // 5/min can't start one unit, which needs 6 however little it makes.
    assert_near(count(&s, unit), 0.0);
    assert_near(s.link_flow[&feed], 0.0);
    assert_near(port_flow(&s, unit, HEAVY_XIRAGEN, Side::Input), 0.0);
    let reason = s.throttle.get(&unit).expect("the unit is below its limit");
    assert_eq!((&reason.item, reason.side), (&item(XIRAGEN), Side::Input));
    // The source isn't at its limit (it sends nothing), so no node at its limit is found behind
    // the port.
    assert_eq!(reason.limited_by, None);
}

#[test]
fn a_unit_runs_as_many_facilities_as_its_activation_input_covers() {
    let catalog = catalog();
    for limit in [None, Some(2.0)] {
        let mut g = graph();
        let xiragen = source(&mut g, XIRAGEN, 8.0);
        let unit = add_recipe(&mut g, &catalog, ENR_POWDER, limit);
        let feed = link(&mut g, &catalog, xiragen, unit, XIRAGEN);
        let s = solve(&catalog, &g);
        // 8/min covers one running unit (6), not two (12). Heavy Xiragen comes from the depot
        // without end, so that one unit runs flat out: count 1, not some part of it.
        assert_near(count(&s, unit), 1.0);
        assert_near(s.link_flow[&feed], 6.0);
        assert_near(port_flow(&s, unit, HEAVY_XIRAGEN, Side::Input), 30.0);
        assert_near(port_flow(&s, unit, HEAVY_XIRANITE, Side::Output), 12.0);
    }
}

#[test]
fn a_count_held_by_another_input_still_needs_the_whole_activation_input() {
    let catalog = catalog();
    let mut g = graph();
    let heavy = source(&mut g, HEAVY_XIRAGEN, 15.0);
    let unit = add_recipe(&mut g, &catalog, ENR_POWDER, None);
    link(&mut g, &catalog, heavy, unit, HEAVY_XIRAGEN);
    let s = solve(&catalog, &g);
    // 15 of the 30 one unit takes.
    assert_near(count(&s, unit), 0.5);
    assert_near(port_flow(&s, unit, XIRAGEN, Side::Input), 6.0);
    assert_near(port_flow(&s, unit, HEAVY_XIRANITE, Side::Output), 6.0);
}

#[test]
fn one_supply_that_covers_one_unit_runs_one_of_two() {
    let catalog = catalog();
    let mut g = graph();
    let xiragen = source(&mut g, XIRAGEN, 9.0);
    let a = add_recipe(&mut g, &catalog, ENR_POWDER, None);
    let b = add_recipe(&mut g, &catalog, ENR_POWDER, None);
    let to_a = link(&mut g, &catalog, xiragen, a, XIRAGEN);
    let to_b = link(&mut g, &catalog, xiragen, b, XIRAGEN);
    let s = solve(&catalog, &g);
    // An even split, 4.5 each, starts neither unit. 9/min starts one, flat out.
    let (a, b) = (count(&s, a).unwrap(), count(&s, b).unwrap());
    let mut counts = [a, b];
    counts.sort_by(f64::total_cmp);
    assert_near(Some(counts[0]), 0.0);
    assert_near(Some(counts[1]), 1.0);
    let mut flows = [s.link_flow[&to_a].unwrap(), s.link_flow[&to_b].unwrap()];
    flows.sort_by(f64::total_cmp);
    assert_near(Some(flows[0]), 0.0);
    assert_near(Some(flows[1]), 6.0);
}

/// Twenty units limited to 0.6 share 100 Xiragen a minute. All twenty running would need 120;
/// sixteen need 96. So sixteen run at 0.6 and four are off.
fn twenty_units_sharing_100() -> (Catalog, Graph, Vec<NodeId>) {
    let catalog = catalog();
    let mut g = graph();
    let xiragen = source(&mut g, XIRAGEN, 100.0);
    let units: Vec<NodeId> = (0..20)
        .map(|_| {
            let unit = add_recipe(&mut g, &catalog, ENR_POWDER, Some(0.6));
            link(&mut g, &catalog, xiragen, unit, XIRAGEN);
            unit
        })
        .collect();
    (catalog, g, units)
}

#[test]
fn a_shared_activation_input_runs_the_units_it_covers_and_switches_off_the_rest() {
    let (catalog, g, units) = twenty_units_sharing_100();
    let s = solve(&catalog, &g);
    let counts: Vec<f64> = units.iter().map(|&u| count(&s, u).unwrap()).collect();
    let running = counts
        .iter()
        .filter(|&&c| (c - 0.6).abs() < TOLERANCE)
        .count();
    let off = counts.iter().filter(|&&c| c.abs() < TOLERANCE).count();
    assert_eq!((running, off), (16, 4), "{counts:?}");
    assert_near(
        s.totals.depot_in.get(&item(HEAVY_XIRAGEN)).copied(),
        16.0 * 18.0,
    );
}

#[test]
fn the_same_graph_settles_the_same_units_on_every_solve() {
    let (catalog, g, units) = twenty_units_sharing_100();
    let counts =
        |s: &Solution| -> Vec<Option<f64>> { units.iter().map(|&u| count(s, u)).collect() };
    let first = counts(&solve(&catalog, &g));
    for _ in 0..5 {
        assert_eq!(counts(&solve(&catalog, &g)), first);
    }
}

#[test]
fn a_search_cut_short_still_gives_a_consistent_answer_every_time() {
    // Twenty identical units on one short supply: proving the best answer takes about 12,000
    // branch-and-bound nodes, so pass 1 stops at its node limit and keeps the best answer found.
    // That answer still follows every rule, and it is the same on every solve.
    let (catalog, g, units) = twenty_units_sharing_100();
    let check = |s: &Solution| {
        let mut sent = 0.0;
        for &unit in &units {
            let count = count(s, unit).unwrap();
            let running = f64::from(RoundUp::of(count).facilities);
            let xiragen = port_flow(s, unit, XIRAGEN, Side::Input).unwrap();
            // Each running facility's whole 6, carried by the unit's one Xiragen link.
            assert_near(Some(xiragen), 6.0 * running);
            assert_near(port_flow(s, unit, HEAVY_XIRAGEN, Side::Input), 30.0 * count);
            sent += xiragen;
        }
        // What the source sends is what the units take, and no more than its limit.
        let source = s.rate.values().find_map(|rate| *rate).unwrap();
        assert_near(Some(source), sent);
        assert!(source <= 100.0 + TOLERANCE);
    };
    let first = solve(&catalog, &g);
    check(&first);
    let counts =
        |s: &Solution| -> Vec<Option<f64>> { units.iter().map(|&u| count(s, u)).collect() };
    for _ in 0..3 {
        let again = solve(&catalog, &g);
        check(&again);
        assert_eq!(counts(&again), counts(&first));
    }
}

#[test]
fn a_unit_without_a_limit_on_the_depot_is_settled_by_a_limit() {
    // ADR 0006: the unit can grow without end from the depot, so the sink's limit settles it,
    // and the sink without a limit takes nothing.
    let catalog = catalog();
    let mut g = graph();
    let unit = add_recipe(&mut g, &catalog, ENR_POWDER, None);
    let limited = sink(&mut g, HEAVY_XIRANITE, Some(6.0));
    let open = sink(&mut g, HEAVY_XIRANITE, None);
    link(&mut g, &catalog, unit, limited, HEAVY_XIRANITE);
    link(&mut g, &catalog, unit, open, HEAVY_XIRANITE);
    let s = solve(&catalog, &g);
    assert_near(count(&s, unit), 0.5);
    assert_near(*s.rate.get(&limited).unwrap(), 6.0);
    assert_near(*s.rate.get(&open).unwrap(), 0.0);
    assert_near(port_flow(&s, unit, XIRAGEN, Side::Input), 6.0);
}

#[test]
fn a_depot_activation_input_leaves_an_even_split_even() {
    // 30/min of Heavy Xiragen split between two units; their Xiragen is on the depot. Pass 2 splits
    // the Heavy Xiragen evenly, 15 each, so each unit runs at 0.5: one running facility each,
    // needing 6 Xiragen whole.
    let catalog = catalog();
    let mut g = graph();
    let heavy = source(&mut g, HEAVY_XIRAGEN, 30.0);
    let a = add_recipe(&mut g, &catalog, ENR_POWDER, None);
    let b = add_recipe(&mut g, &catalog, ENR_POWDER, None);
    let to_a = link(&mut g, &catalog, heavy, a, HEAVY_XIRAGEN);
    let to_b = link(&mut g, &catalog, heavy, b, HEAVY_XIRAGEN);
    let s = solve(&catalog, &g);
    for (unit, feed) in [(a, to_a), (b, to_b)] {
        assert_near(count(&s, unit), 0.5);
        assert_near(s.link_flow[&feed], 15.0);
        assert_near(port_flow(&s, unit, XIRAGEN, Side::Input), 6.0);
        assert_near(
            s.depot
                .get(&port(unit, XIRAGEN, Side::Input))
                .copied()
                .flatten(),
            6.0,
        );
    }
}

#[test]
fn an_even_split_after_pass_1_keeps_each_count_inside_its_running_facilities() {
    // 75/min of Heavy Xiragen and 18/min of Xiragen, each split between two units. 18 starts
    // three facilities between them, which make 2.5 units' worth of the 75. However pass 1 shares
    // the three facilities out and pass 2 then evens the splits, each unit's Xiragen must be 6 for
    // each of its facilities that runs: its count rounded up.
    let catalog = catalog();
    let mut g = graph();
    let heavy = source(&mut g, HEAVY_XIRAGEN, 75.0);
    let xiragen = source(&mut g, XIRAGEN, 18.0);
    let units = [
        add_recipe(&mut g, &catalog, ENR_POWDER, None),
        add_recipe(&mut g, &catalog, ENR_POWDER, None),
    ];
    for unit in units {
        link(&mut g, &catalog, heavy, unit, HEAVY_XIRAGEN);
        link(&mut g, &catalog, xiragen, unit, XIRAGEN);
    }
    let s = solve(&catalog, &g);
    let counts: Vec<f64> = units.iter().map(|&u| count(&s, u).unwrap()).collect();
    assert_near(Some(counts.iter().sum()), 2.5);
    for unit in units {
        let running = RoundUp::of(count(&s, unit).unwrap()).facilities;
        assert_near(
            port_flow(&s, unit, XIRAGEN, Side::Input),
            6.0 * f64::from(running),
        );
    }
    assert_near(*s.rate.get(&xiragen).unwrap(), 18.0);
}

#[test]
fn totals_count_the_running_units() {
    let catalog = catalog();
    let mut g = graph();
    add_recipe(&mut g, &catalog, ENR_POWDER, Some(0.5));
    add_recipe(&mut g, &catalog, ENR_POWDER, Some(1.5));
    let s = solve(&catalog, &g);
    let unit = aic_data::FacilityId::from(UNIT);
    assert_near(s.totals.facilities.get(&unit).copied(), 2.0);
    // One running for the half, two for the one and a half.
    assert_eq!(s.totals.facilities_rounded.get(&unit), Some(&3));
    assert_eq!(RoundUp::of(1.5).facilities, 2);
    // 6 + 12 Xiragen: the three running units' activation inputs.
    assert_near(s.totals.depot_in.get(&item(XIRAGEN)).copied(), 18.0);
}

fn port(node: NodeId, item_id: &str, side: Side) -> Port {
    Port {
        node,
        item: item(item_id),
        side,
    }
}
