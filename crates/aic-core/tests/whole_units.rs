//! Whole units (ADR 0015): the Gas Dispersing Unit (`vaporizer_1`) and the Forges of Luminance
//! (`magic_spawner_1`, `_2`) take their gas and put out nothing. Each input is an activation input
//! of which nothing scales, and the facility count is a whole number: the largest the limit and
//! the supply allow.
//!
//! The recipes are the v1.7.0 ones: 6 a minute of a gas, nothing out. Values are worked by hand.

mod common;

use aic_core::{
    Graph, NodeId, NodeKind, Port, RoundUp, SectionStatus, Side, Solution, UndeterminedReason,
    solve,
};
use aic_data::{Catalog, FacilityId};
use common::*;

const TOLERANCE: f64 = 1e-6;

const DISPERSING_UNIT: &str = "vaporizer_1";
const DISPERSE_INERGEN: &str = "vaporizer_1-gas_inert";
const INERGEN: &str = "gas_inert";
const FORGE: &str = "magic_spawner_1";
const FORGE_L: &str = "magic_spawner_1-magic_ming";
const XIRAGEN: &str = "gas_xiranite";
/// Made up: a unit of whole units with two inputs, which no facility of the game has.
const TWO_GAS_UNIT: &str = "two_gas_unit";
const TWO_GASES: &str = "two_gas_unit-both";
const ACRIDGEN: &str = "gas_acid";

fn catalog() -> Catalog {
    TestCatalog::new()
        .facility_with(DISPERSING_UNIT, 1.0, 0.0, &[JINLONG])
        .whole_units(DISPERSING_UNIT)
        .recipe(
            DISPERSE_INERGEN,
            DISPERSING_UNIT,
            60.0,
            &[(INERGEN, 6.0)],
            &[],
        )
        .creates_environment(DISPERSE_INERGEN, INERGEN)
        .facility_with(FORGE, 1.0, 0.0, &[JINLONG])
        .whole_units(FORGE)
        .recipe(FORGE_L, FORGE, 60.0, &[(XIRAGEN, 6.0)], &[])
        .facility_with(TWO_GAS_UNIT, 1.0, 10.0, &[JINLONG])
        .whole_units(TWO_GAS_UNIT)
        // 6 Xiragen and 3 Acridgen a minute, nothing out.
        .recipe(
            TWO_GASES,
            TWO_GAS_UNIT,
            60.0,
            &[(XIRAGEN, 6.0), (ACRIDGEN, 3.0)],
            &[],
        )
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

fn port(node: NodeId, item_id: &str, side: Side) -> Port {
    Port {
        node,
        item: item(item_id),
        side,
    }
}

fn port_flow(s: &Solution, node: NodeId, item_id: &str) -> Option<f64> {
    let port = port(node, item_id, Side::Input);
    *s.port_flow
        .get(&port)
        .unwrap_or_else(|| panic!("{port:?} has a flow"))
}

fn depot_in(s: &Solution, node: NodeId, item_id: &str) -> Option<f64> {
    *s.depot
        .get(&port(node, item_id, Side::Input))
        .expect("the port is on the depot")
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

#[test]
fn a_unit_on_the_depot_runs_its_limit_rounded_down() {
    let catalog = catalog();
    // (limit, units running, Inergen from the depot)
    for (limit, running, inergen) in [(2.0, 2.0, 12.0), (1.5, 1.0, 6.0), (0.5, 0.0, 0.0)] {
        let mut g = graph();
        let unit = add_recipe(&mut g, &catalog, DISPERSE_INERGEN, Some(limit));
        let s = solve(&catalog, &g);
        assert_eq!(count(&s, unit), Some(running), "limit {limit}");
        assert_near(port_flow(&s, unit, INERGEN), inergen);
        assert_near(depot_in(&s, unit, INERGEN), inergen);
        // As many as its limit lets it run: not held back.
        assert!(!s.throttle.contains_key(&unit), "limit {limit}");
        let totals = &s.totals;
        let facility = FacilityId::from(DISPERSING_UNIT);
        assert_near(totals.facilities.get(&facility).copied(), running);
        assert_eq!(
            totals.facilities_rounded.get(&facility).copied(),
            Some(running as u32)
        );
        assert_eq!(
            totals.depot_in.get(&item(INERGEN)).copied().unwrap_or(0.0),
            inergen
        );
    }
}

#[test]
fn a_unit_on_the_depot_without_a_limit_is_undetermined() {
    let catalog = catalog();
    let mut g = graph();
    let unit = add_recipe(&mut g, &catalog, DISPERSE_INERGEN, None);
    let s = solve(&catalog, &g);
    assert_eq!(count(&s, unit), None);
    assert_eq!(
        s.sections[0].status,
        SectionStatus::Undetermined(UndeterminedReason::NoLimit)
    );
}

#[test]
fn a_forge_fed_by_links_runs_the_units_its_supply_covers() {
    let catalog = catalog();
    let mut g = graph();
    let xiragen = source(&mut g, XIRAGEN, 20.0);
    let forge = add_recipe(&mut g, &catalog, FORGE_L, None);
    let feed = link(&mut g, &catalog, xiragen, forge, XIRAGEN);
    let s = solve(&catalog, &g);
    // 20/min covers three units of 6; the count is the units running, not some part of them.
    assert_eq!(count(&s, forge), Some(3.0));
    assert_near(s.link_flow[&feed], 18.0);
    assert_near(port_flow(&s, forge, XIRAGEN), 18.0);
    assert_eq!(RoundUp::of(3.0).facilities, 3);
    let facility = FacilityId::from(FORGE);
    assert_eq!(s.totals.facilities.get(&facility).copied(), Some(3.0));
    assert_eq!(s.totals.facilities_rounded.get(&facility).copied(), Some(3));
}

#[test]
fn a_forge_whose_supply_falls_short_of_its_limit_is_throttled_on_its_input() {
    let catalog = catalog();
    let mut g = graph();
    let xiragen = source(&mut g, XIRAGEN, 9.0);
    let forge = add_recipe(&mut g, &catalog, FORGE_L, Some(2.0));
    let feed = link(&mut g, &catalog, xiragen, forge, XIRAGEN);
    let s = solve(&catalog, &g);
    // 9/min runs one unit; a second needs 12.
    assert_eq!(count(&s, forge), Some(1.0));
    assert_near(s.link_flow[&feed], 6.0);
    let reason = s
        .throttle
        .get(&forge)
        .expect("the forge is below its limit");
    assert_eq!((&reason.item, reason.side), (&item(XIRAGEN), Side::Input));
    // The source sends 6 of its 9, so it isn't at its limit.
    assert_eq!(reason.limited_by, None);
}

#[test]
fn units_sharing_a_supply_each_run_whole() {
    let catalog = catalog();
    let mut g = graph();
    let xiragen = source(&mut g, XIRAGEN, 20.0);
    let forges = [
        add_recipe(&mut g, &catalog, FORGE_L, None),
        add_recipe(&mut g, &catalog, FORGE_L, None),
    ];
    let feeds = forges.map(|forge| link(&mut g, &catalog, xiragen, forge, XIRAGEN));
    let s = solve(&catalog, &g);
    // However the two split it, 20/min runs three units between them, each of them whole.
    let mut total = 0.0;
    for (forge, feed) in forges.into_iter().zip(feeds) {
        let count = count(&s, forge).unwrap();
        assert_eq!(count, count.round(), "{count}");
        assert_near(s.link_flow[&feed], 6.0 * count);
        total += count;
    }
    assert_eq!(total, 3.0);
}

#[test]
fn a_unit_with_two_inputs_runs_on_both_or_not_at_all() {
    let catalog = catalog();
    for (supply, running) in [(10.0, 1.0), (5.0, 0.0)] {
        let mut g = graph();
        let xiragen = source(&mut g, XIRAGEN, supply);
        let unit = add_recipe(&mut g, &catalog, TWO_GASES, Some(3.0));
        let feed = link(&mut g, &catalog, xiragen, unit, XIRAGEN);
        let s = solve(&catalog, &g);
        assert_eq!(count(&s, unit), Some(running), "{supply}/min");
        assert_near(s.link_flow[&feed], 6.0 * running);
        // The Acridgen on the depot is the same running units' worth.
        assert_near(depot_in(&s, unit, ACRIDGEN), 3.0 * running);
        assert_near(port_flow(&s, unit, ACRIDGEN), 3.0 * running);
        assert_near(Some(s.totals.power_draw_kw), 10.0 * running);
    }
}

#[test]
fn a_limit_typed_through_a_units_port_is_the_most_units_it_covers() {
    let catalog = catalog();
    let mut g = graph();
    let forge = add_recipe(&mut g, &catalog, FORGE_L, None);
    let xiragen = port(forge, XIRAGEN, Side::Input);
    for (rate, units) in [(6.0, 1.0), (9.0, 1.0), (12.0, 2.0), (5.0, 0.0)] {
        g.set_port_limit(&catalog, &xiragen, Some(rate)).unwrap();
        assert_eq!(g.nodes()[forge].kind.limit(), Some(units), "{rate}/min");
    }
}

#[test]
fn a_fractional_limit_shows_on_a_units_port_as_the_units_it_runs() {
    // A graph saved before units ran whole may hold a fractional count limit: 1.5 runs one unit.
    let catalog = catalog();
    let mut g = graph();
    let forge = add_recipe(&mut g, &catalog, FORGE_L, Some(1.5));
    let shown = g.port_limit(&catalog, &port(forge, XIRAGEN, Side::Input));
    assert_eq!(shown, Some(6.0));
}
