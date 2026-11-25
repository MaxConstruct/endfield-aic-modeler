//! Gas environments and arrays (ADR 0015): a recipe that needs a gas environment is warned about
//! when no node that creates it runs, and the arrays' projections are counted against what the
//! forges project. Neither changes a number of the solve.
//!
//! The recipes are the v1.7.0 ones. Values are worked by hand.

mod common;

use aic_core::{Graph, Member, NodeId, NodeKind, Solution, Warning, solve};
use aic_data::{Catalog, FacilityId};
use common::*;

const TOLERANCE: f64 = 1e-6;

const DISPERSING_UNIT: &str = "vaporizer_1";
const DISPERSE_INERGEN: &str = "vaporizer_1-gas_inert";
const DISPERSE_ACRIDGEN: &str = "vaporizer_1-gas_acid";
const INERGEN: &str = "gas_inert";
const ACRIDGEN: &str = "gas_acid";
const XIRANITE_PRODUCTION: &str = "xiranite_oven_xiranite_powder_2";

const L_ARRAY: &str = "magic_ming_1";
const T_ARRAY: &str = "magic_hui_1";
const E_ARRAY: &str = "magic_heng_1";
const EQUPRIUM: &str = "formation_magic_heng_item_copper_enr_heng_1";
const EQUIRANITE: &str = "formation_magic_heng_item_xiranite_enr_heng_1";
const LUCIDOLITE: &str = "magic_balancer_1_copper_bright_1";
const BIPOLAR: &str = "magic_spawner_2";
const BIPOLAR_L_T: &str = "magic_spawner_2-magic_ming-magic_hui";
const FORGE: &str = "magic_spawner_1";
const FORGE_L: &str = "magic_spawner_1-magic_ming";
const XIRAGEN: &str = "gas_xiranite";

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
        .recipe(
            DISPERSE_ACRIDGEN,
            DISPERSING_UNIT,
            60.0,
            &[(ACRIDGEN, 6.0)],
            &[],
        )
        .creates_environment(DISPERSE_ACRIDGEN, ACRIDGEN)
        // Forge of the Sky: 1 Carbon and 1 Clean Water → 1 Xiranite per 2 s, in Inergen.
        .facility_with("xiranite_oven_1", 1.0, 50.0, &[JINLONG])
        .recipe(
            XIRANITE_PRODUCTION,
            "xiranite_oven_1",
            2.0,
            &[("carbon_mtl", 1.0), ("liquid_water", 1.0)],
            &[("xiranite_powder", 1.0)],
        )
        .environment(XIRANITE_PRODUCTION, INERGEN)
        // Arrays: an E-Array T1 converts 25 per 10 s at 40%, 60 a minute.
        .facility_with(L_ARRAY, 0.2, 0.0, &[JINLONG])
        .stack(L_ARRAY, &[(L_ARRAY, 1)])
        .facility_with(T_ARRAY, 0.2, 0.0, &[JINLONG])
        .stack(T_ARRAY, &[(T_ARRAY, 1)])
        .facility_with(E_ARRAY, 0.4, 0.0, &[JINLONG])
        .stack(E_ARRAY, &[(L_ARRAY, 1), (T_ARRAY, 1)])
        .recipe(
            EQUPRIUM,
            E_ARRAY,
            10.0,
            &[("copper_nugget", 25.0)],
            &[("copper_enr_heng", 25.0)],
        )
        .recipe(
            EQUIRANITE,
            E_ARRAY,
            10.0,
            &[("xiranite_powder", 25.0)],
            &[("xiranite_enr_heng", 25.0)],
        )
        // Forge of Equilibrium: 10 Equiranite and 10 Equprium → 5 Lucidolite per 10 s.
        .facility_with("magic_balancer_1", 1.0, 50.0, &[JINLONG])
        .recipe(
            LUCIDOLITE,
            "magic_balancer_1",
            10.0,
            &[("xiranite_enr_heng", 10.0), ("copper_enr_heng", 10.0)],
            &[("copper_bright", 5.0)],
        )
        .facility_with(FORGE, 1.0, 0.0, &[JINLONG])
        .whole_units(FORGE)
        .recipe(FORGE_L, FORGE, 60.0, &[(XIRAGEN, 6.0)], &[])
        .projects(FORGE_L, &[(L_ARRAY, 1)])
        .facility_with(BIPOLAR, 1.0, 0.0, &[JINLONG])
        .whole_units(BIPOLAR)
        .recipe(BIPOLAR_L_T, BIPOLAR, 60.0, &[(XIRAGEN, 6.0)], &[])
        .projects(BIPOLAR_L_T, &[(L_ARRAY, 1), (T_ARRAY, 1)])
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

fn source(g: &mut Graph, item_id: &str, limit: f64) -> NodeId {
    add(
        g,
        NodeKind::Source {
            item: item(item_id),
            limit,
        },
    )
}

fn missing(s: &Solution) -> Vec<(NodeId, &str)> {
    (s.warnings.iter())
        .filter_map(|warning| match warning {
            Warning::EnvironmentMissing { node, gas } => Some((*node, gas.as_str())),
            _ => None,
        })
        .collect()
}

fn short(s: &Solution) -> Vec<(&str, u32, u32)> {
    (s.warnings.iter())
        .filter_map(|warning| match warning {
            Warning::ProjectionsShort {
                projection,
                needed,
                provided,
            } => Some((projection.as_str(), *needed, *provided)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_recipe_alone_misses_its_environment_and_still_runs() {
    let catalog = catalog();
    let mut g = graph();
    let oven = add_recipe(&mut g, &catalog, XIRANITE_PRODUCTION, Some(1.0));
    let s = solve(&catalog, &g);
    assert_eq!(missing(&s), [(oven, INERGEN)]);
    assert_near(count(&s, oven), 1.0);
}

#[test]
fn a_running_unit_of_the_gas_anywhere_in_the_graph_sets_the_environment() {
    let catalog = catalog();
    let mut g = graph();
    let oven = add_recipe(&mut g, &catalog, XIRANITE_PRODUCTION, Some(1.0));
    add_recipe(&mut g, &catalog, DISPERSE_INERGEN, Some(1.0));
    let s = solve(&catalog, &g);
    assert_eq!(missing(&s), []);
    assert_near(count(&s, oven), 1.0);
}

#[test]
fn a_unit_starved_of_its_gas_sets_no_environment() {
    let catalog = catalog();
    let mut g = graph();
    let oven = add_recipe(&mut g, &catalog, XIRANITE_PRODUCTION, Some(1.0));
    let inergen = source(&mut g, INERGEN, 3.0);
    let unit = add_recipe(&mut g, &catalog, DISPERSE_INERGEN, Some(1.0));
    link(&mut g, &catalog, inergen, unit, INERGEN);
    let s = solve(&catalog, &g);
    assert_eq!(count(&s, unit), Some(0.0));
    assert_eq!(missing(&s), [(oven, INERGEN)]);
}

#[test]
fn a_unit_of_another_gas_does_not_help() {
    let catalog = catalog();
    let mut g = graph();
    let oven = add_recipe(&mut g, &catalog, XIRANITE_PRODUCTION, Some(1.0));
    add_recipe(&mut g, &catalog, DISPERSE_ACRIDGEN, Some(1.0));
    let s = solve(&catalog, &g);
    assert_eq!(missing(&s), [(oven, INERGEN)]);
}

#[test]
fn a_unit_inside_a_workspace_counts() {
    let catalog = catalog();
    let mut g = graph();
    add_recipe(&mut g, &catalog, XIRANITE_PRODUCTION, Some(1.0));
    let unit = add_recipe(&mut g, &catalog, DISPERSE_INERGEN, Some(1.0));
    g.workspace_from(&[Member::Node(unit)], "Gas").unwrap();
    let s = solve(&catalog, &g);
    assert_eq!(missing(&s), []);
}

#[test]
fn a_unit_whose_section_is_undetermined_counts_as_running() {
    let catalog = catalog();
    let mut g = graph();
    add_recipe(&mut g, &catalog, XIRANITE_PRODUCTION, Some(1.0));
    let unit = add_recipe(&mut g, &catalog, DISPERSE_INERGEN, None);
    let s = solve(&catalog, &g);
    assert_eq!(count(&s, unit), None);
    assert_eq!(missing(&s), []);
}

#[test]
fn every_recipe_that_needs_it_is_warned_about_whatever_its_count() {
    let catalog = catalog();
    let mut g = graph();
    let idle = add_recipe(&mut g, &catalog, XIRANITE_PRODUCTION, Some(0.0));
    let open = add_recipe(&mut g, &catalog, XIRANITE_PRODUCTION, None);
    let s = solve(&catalog, &g);
    assert_eq!(missing(&s), [(idle, INERGEN), (open, INERGEN)]);
}

/// 30 Cuprium and 30 Xiranite a minute, each through an E-Array T1 (half an array each), into the
/// Forge of Equilibrium (half of one), which makes 15 Lucidolite a minute. Returns the graph and
/// the two arrays and the Forge of Equilibrium.
fn lucidolite_line(catalog: &Catalog) -> (Graph, [NodeId; 3]) {
    let mut g = graph();
    let cuprium = source(&mut g, "copper_nugget", 30.0);
    let xiranite = source(&mut g, "xiranite_powder", 30.0);
    let equprium = add_recipe(&mut g, catalog, EQUPRIUM, None);
    let equiranite = add_recipe(&mut g, catalog, EQUIRANITE, None);
    let balancer = add_recipe(&mut g, catalog, LUCIDOLITE, None);
    link(&mut g, catalog, cuprium, equprium, "copper_nugget");
    link(&mut g, catalog, xiranite, equiranite, "xiranite_powder");
    link(&mut g, catalog, equprium, balancer, "copper_enr_heng");
    link(&mut g, catalog, equiranite, balancer, "xiranite_enr_heng");
    (g, [equprium, equiranite, balancer])
}

#[test]
fn a_lucidolite_line_runs_half_an_array_on_each_side() {
    let catalog = catalog();
    let (g, [equprium, equiranite, balancer]) = lucidolite_line(&catalog);
    let s = solve(&catalog, &g);
    assert_near(count(&s, equprium), 0.5);
    assert_near(count(&s, equiranite), 0.5);
    assert_near(count(&s, balancer), 0.5);
    assert_near(
        s.totals.depot_out.get(&item("copper_bright")).copied(),
        15.0,
    );
    // Arrays count among the facilities like any other.
    let e_array = FacilityId::from(E_ARRAY);
    assert_near(s.totals.facilities.get(&e_array).copied(), 1.0);
    assert_eq!(s.totals.facilities_rounded.get(&e_array).copied(), Some(2));
}

#[test]
fn arrays_without_forges_are_short_of_every_projection() {
    let catalog = catalog();
    let (g, _) = lucidolite_line(&catalog);
    let s = solve(&catalog, &g);
    // Each half array rounds up to one: one L and one T each.
    assert_eq!(short(&s), [(L_ARRAY, 2, 0), (T_ARRAY, 2, 0)]);
}

#[test]
fn one_bipolar_forge_leaves_both_projections_short_and_two_leave_neither() {
    let catalog = catalog();
    for (forges, expected) in [(1.0, vec![(L_ARRAY, 2, 1), (T_ARRAY, 2, 1)]), (2.0, vec![])] {
        let (mut g, _) = lucidolite_line(&catalog);
        let bipolar = add_recipe(&mut g, &catalog, BIPOLAR_L_T, Some(forges));
        let s = solve(&catalog, &g);
        assert_eq!(count(&s, bipolar), Some(forges));
        assert_eq!(short(&s), expected, "{forges} forges");
    }
}

#[test]
fn a_projection_with_an_undetermined_node_is_not_checked() {
    let catalog = catalog();
    // The forge's section has no limit, so how many it runs is unknown: L is not checked. T has
    // no forge at all, and is short.
    let (mut g, _) = lucidolite_line(&catalog);
    add_recipe(&mut g, &catalog, FORGE_L, None);
    let s = solve(&catalog, &g);
    assert_eq!(short(&s), [(T_ARRAY, 2, 0)]);
    // An array whose own section is undetermined leaves its projections unchecked too.
    let mut g = graph();
    add_recipe(&mut g, &catalog, EQUPRIUM, None);
    let s = solve(&catalog, &g);
    assert_eq!(short(&s), []);
}

#[test]
fn an_idle_array_needs_no_projection() {
    let catalog = catalog();
    let mut g = graph();
    add_recipe(&mut g, &catalog, EQUPRIUM, Some(0.0));
    // A count a hair above a whole number is that number.
    add_recipe(&mut g, &catalog, EQUIRANITE, Some(1.000_000_000_1));
    add_recipe(&mut g, &catalog, FORGE_L, Some(1.0));
    let s = solve(&catalog, &g);
    assert_eq!(short(&s), [(T_ARRAY, 1, 0)]);
}
