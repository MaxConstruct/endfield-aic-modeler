//! M3 golden tests (docs/PLAN.md, section 6). Values are compared at full precision.

mod common;

use aic_core::{
    Graph, NodeId, NodeKind, Port, SectionStatus, Side, Solution, UndeterminedReason, Warning,
    solve,
};
use aic_data::Catalog;
use common::*;

const TOLERANCE: f64 = 1e-6;

#[track_caller]
fn assert_near(actual: Option<f64>, expected: f64) {
    let actual = actual.expect("value is determined");
    assert!(
        (actual - expected).abs() < TOLERANCE,
        "expected {expected}, got {actual}"
    );
}

fn count(solution: &Solution, node: NodeId) -> Option<f64> {
    *solution
        .facility_count
        .get(&node)
        .expect("node has a count")
}

fn rate(solution: &Solution, node: NodeId) -> Option<f64> {
    *solution.rate.get(&node).expect("node has a rate")
}

fn depot(solution: &Solution, node: NodeId, item_id: &str, side: Side) -> Option<f64> {
    let port = Port {
        node,
        item: item(item_id),
        side,
    };
    *solution
        .depot
        .get(&port)
        .unwrap_or_else(|| panic!("{port:?} is a depot port"))
}

fn graph() -> Graph {
    Graph::new(region(TUNDRA))
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

fn no_warnings(solution: &Solution) {
    assert_eq!(solution.warnings, vec![], "unexpected warnings");
}

/// Furnace recipes shared by several tests. Every rate is per facility at speed 1.
fn furnace_catalog() -> Catalog {
    TestCatalog::new()
        .facility("furnace", 5.0)
        // 30 ore → 30 ingot per minute.
        .recipe("ingot", "furnace", 2.0, &[("ore", 1.0)], &[("ingot", 1.0)])
        // 30 ingot → 20 plate.
        .recipe(
            "plate",
            "furnace",
            6.0,
            &[("ingot", 3.0)],
            &[("plate", 2.0)],
        )
        // 10 plate → 40 screw.
        .recipe(
            "screw",
            "furnace",
            6.0,
            &[("plate", 1.0)],
            &[("screw", 4.0)],
        )
        // 30 ore + 30 water → 30 copper + 30 sewage.
        .recipe(
            "copper_nugget",
            "furnace",
            2.0,
            &[("copper_ore", 1.0), ("liquid_water", 1.0)],
            &[("copper_nugget", 1.0), ("liquid_sewage", 1.0)],
        )
        .build()
}

// 1. Single recipe: sink limited to 10. Count = 10 / rate.
#[test]
fn golden_01_single_recipe() {
    let catalog = furnace_catalog();
    let mut g = graph();
    let ingot = add_recipe(&mut g, &catalog, "ingot", None);
    let goal = sink(&mut g, "ingot", Some(10.0));
    let wire = link(&mut g, &catalog, ingot, goal, "ingot");

    let s = solve(&catalog, &g);
    assert_near(count(&s, ingot), 10.0 / 30.0);
    assert_near(s.link_flow[&wire], 10.0);
    assert_near(rate(&s, goal), 10.0);
    assert_near(depot(&s, ingot, "ore", Side::Input), 10.0);
    no_warnings(&s);
}

// 2. Byproduct to depot: sewage unconnected shows as depot output, with no warning.
#[test]
fn golden_02_byproduct_to_depot() {
    let catalog = furnace_catalog();
    let mut g = graph();
    let copper = add_recipe(&mut g, &catalog, "copper_nugget", Some(1.0));

    let s = solve(&catalog, &g);
    assert_near(count(&s, copper), 1.0);
    assert_near(depot(&s, copper, "liquid_sewage", Side::Output), 30.0);
    assert_near(depot(&s, copper, "copper_nugget", Side::Output), 30.0);
    assert_near(depot(&s, copper, "liquid_water", Side::Input), 30.0);
    assert!(s.throttle.is_empty());
    no_warnings(&s);
}

// 3. Byproduct throttles: sewage linked to a sink limited below the sewage output.
#[test]
fn golden_03_byproduct_throttles() {
    let catalog = furnace_catalog();
    let mut g = graph();
    let copper = add_recipe(&mut g, &catalog, "copper_nugget", Some(1.0));
    let drain = sink(&mut g, "liquid_sewage", Some(10.0));
    link(&mut g, &catalog, copper, drain, "liquid_sewage");

    let s = solve(&catalog, &g);
    assert_near(count(&s, copper), 1.0 / 3.0);
    assert_near(depot(&s, copper, "copper_nugget", Side::Output), 10.0);
    let reason = &s.throttle[&copper];
    assert_eq!(
        (&reason.item, reason.side),
        (&item("liquid_sewage"), Side::Output)
    );
    assert_eq!(reason.limited_by, Some(drain));
}

// 4. Fan-out evens out: A makes 30/min and splits evenly to B and C, though C needs less per facility.
#[test]
fn golden_04_fan_out_evens_out() {
    let catalog = TestCatalog::new()
        .facility("miner", 5.0)
        .facility("press", 5.0)
        .recipe("a", "miner", 2.0, &[], &[("x", 1.0)]) // 30 x per minute
        .recipe("b", "press", 3.0, &[("x", 1.0)], &[("bx", 1.0)]) // takes 20 x per minute
        .recipe("c", "press", 6.0, &[("x", 1.0)], &[("cx", 1.0)]) // takes 10 x per minute
        .build();
    let mut g = graph();
    let a = add_recipe(&mut g, &catalog, "a", Some(1.0));
    let b = add_recipe(&mut g, &catalog, "b", Some(5.0));
    let c = add_recipe(&mut g, &catalog, "c", Some(5.0));
    let to_b = link(&mut g, &catalog, a, b, "x");
    let to_c = link(&mut g, &catalog, a, c, "x");

    let s = solve(&catalog, &g);
    assert_near(count(&s, a), 1.0);
    assert_near(s.link_flow[&to_b], 15.0);
    assert_near(s.link_flow[&to_c], 15.0);
    assert_near(count(&s, b), 0.75);
    assert_near(count(&s, c), 1.5);
    for consumer in [b, c] {
        let reason = &s.throttle[&consumer];
        assert_eq!((&reason.item, reason.side), (&item("x"), Side::Input));
        assert_eq!(reason.limited_by, Some(a));
    }
    assert!(!s.throttle.contains_key(&a));
}

// 5. Seed loop, with the v1.5.3 recipes `plant_grass_2`, `plant_grass_seed_2` and `liquid_water`.
#[test]
fn golden_05_seed_loop() {
    let catalog = TestCatalog::new()
        .facility("planter_1", 20.0)
        .facility("seedcol_1", 10.0)
        .facility("pump_1", 10.0)
        .recipe(
            "plant_grass_2",
            "planter_1",
            2.0,
            &[("plant_grass_seed_2", 1.0), ("liquid_water", 1.0)],
            &[("plant_grass_2", 2.0)],
        )
        .recipe(
            "plant_grass_seed_2",
            "seedcol_1",
            2.0,
            &[("plant_grass_2", 1.0)],
            &[("plant_grass_seed_2", 1.0)],
        )
        .recipe("liquid_water", "pump_1", 1.0, &[], &[("liquid_water", 1.0)])
        .build();
    let mut g = graph();
    let planting = add_recipe(&mut g, &catalog, "plant_grass_2", None);
    let picking = add_recipe(&mut g, &catalog, "plant_grass_seed_2", None);
    let pump = add_recipe(&mut g, &catalog, "liquid_water", None);
    let goal = sink(&mut g, "plant_grass_2", Some(10.0));
    let to_goal = link(&mut g, &catalog, planting, goal, "plant_grass_2");
    let to_picking = link(&mut g, &catalog, planting, picking, "plant_grass_2");
    let seeds = link(&mut g, &catalog, picking, planting, "plant_grass_seed_2");
    let water = link(&mut g, &catalog, pump, planting, "liquid_water");

    let s = solve(&catalog, &g);
    assert_near(count(&s, planting), 1.0 / 3.0);
    assert_near(count(&s, picking), 1.0 / 3.0);
    assert_near(count(&s, pump), 1.0 / 6.0);
    assert_near(s.link_flow[&to_goal], 10.0);
    assert_near(s.link_flow[&to_picking], 10.0);
    assert_near(s.link_flow[&seeds], 10.0);
    assert_near(s.link_flow[&water], 10.0);
    no_warnings(&s);
}

// 6. Limited source, open end: source 60/min → three steps → sink with no limit.
#[test]
fn golden_06_limited_source_open_end() {
    let catalog = furnace_catalog();
    let mut g = graph();
    let ore = source(&mut g, "ore", 60.0);
    let ingot = add_recipe(&mut g, &catalog, "ingot", None);
    let plate = add_recipe(&mut g, &catalog, "plate", None);
    let screw = add_recipe(&mut g, &catalog, "screw", None);
    let goal = sink(&mut g, "screw", None);
    link(&mut g, &catalog, ore, ingot, "ore");
    link(&mut g, &catalog, ingot, plate, "ingot");
    link(&mut g, &catalog, plate, screw, "plate");
    link(&mut g, &catalog, screw, goal, "screw");

    let s = solve(&catalog, &g);
    // 60 ore → 60 ingot → 40 plate → 160 screw.
    assert_near(rate(&s, ore), 60.0);
    assert_near(count(&s, ingot), 2.0);
    assert_near(count(&s, plate), 2.0);
    assert_near(count(&s, screw), 4.0);
    assert_near(rate(&s, goal), 160.0);
}

// 7. Satisfactory reference: the split is uneven because the recipe ratios force it.
#[test]
fn golden_07_satisfactory_reference() {
    let catalog = TestCatalog::new()
        .facility("smelter", 4.0)
        .facility("constructor", 4.0)
        .facility("assembler", 15.0)
        .recipe(
            "iron_ingot",
            "smelter",
            2.0,
            &[("iron_ore", 1.0)],
            &[("iron_ingot", 1.0)],
        )
        .recipe(
            "iron_plate",
            "constructor",
            6.0,
            &[("iron_ingot", 3.0)],
            &[("iron_plate", 2.0)],
        )
        .recipe(
            "iron_rod",
            "constructor",
            4.0,
            &[("iron_ingot", 1.0)],
            &[("iron_rod", 1.0)],
        )
        .recipe(
            "screw",
            "constructor",
            6.0,
            &[("iron_rod", 1.0)],
            &[("screw", 4.0)],
        )
        .recipe(
            "reinforced_plate",
            "assembler",
            12.0,
            &[("iron_plate", 6.0), ("screw", 12.0)],
            &[("reinforced_plate", 1.0)],
        )
        .recipe(
            "modular_frame",
            "assembler",
            60.0,
            &[("reinforced_plate", 3.0), ("iron_rod", 12.0)],
            &[("modular_frame", 2.0)],
        )
        .build();
    let mut g = graph();
    let ore = source(&mut g, "iron_ore", 60.0);
    let smelter = add_recipe(&mut g, &catalog, "iron_ingot", None);
    let plate = add_recipe(&mut g, &catalog, "iron_plate", None);
    let rod = add_recipe(&mut g, &catalog, "iron_rod", None);
    let screw = add_recipe(&mut g, &catalog, "screw", None);
    let reinforced = add_recipe(&mut g, &catalog, "reinforced_plate", None);
    let frame = add_recipe(&mut g, &catalog, "modular_frame", None);
    let ore_in = link(&mut g, &catalog, ore, smelter, "iron_ore");
    let ingot_plate = link(&mut g, &catalog, smelter, plate, "iron_ingot");
    let ingot_rod = link(&mut g, &catalog, smelter, rod, "iron_ingot");
    let plate_rp = link(&mut g, &catalog, plate, reinforced, "iron_plate");
    let rod_screw = link(&mut g, &catalog, rod, screw, "iron_rod");
    let rod_frame = link(&mut g, &catalog, rod, frame, "iron_rod");
    let screw_rp = link(&mut g, &catalog, screw, reinforced, "screw");
    let rp_frame = link(&mut g, &catalog, reinforced, frame, "reinforced_plate");

    let s = solve(&catalog, &g);
    assert_near(count(&s, smelter), 2.0);
    assert_near(count(&s, plate), 1.125);
    assert_near(count(&s, rod), 1.75);
    assert_near(count(&s, screw), 1.125);
    assert_near(count(&s, reinforced), 0.75);
    assert_near(count(&s, frame), 1.25);
    assert_near(s.link_flow[&ore_in], 60.0);
    assert_near(s.link_flow[&ingot_plate], 33.75);
    assert_near(s.link_flow[&ingot_rod], 26.25);
    assert_near(s.link_flow[&plate_rp], 22.5);
    assert_near(s.link_flow[&rod_screw], 11.25);
    assert_near(s.link_flow[&rod_frame], 15.0);
    assert_near(s.link_flow[&screw_rp], 45.0);
    assert_near(s.link_flow[&rp_frame], 3.75);
    assert_near(depot(&s, frame, "modular_frame", Side::Output), 2.5);
}

// 8. Undetermined: nothing limits the section, so every value is blank.
#[test]
fn golden_08_undetermined_between_storages() {
    let catalog = furnace_catalog();
    let mut g = graph();
    let before = add(&mut g, NodeKind::Storage { item: None });
    let ingot = add_recipe(&mut g, &catalog, "ingot", None);
    let after = add(&mut g, NodeKind::Storage { item: None });
    let feed = link(&mut g, &catalog, before, ingot, "ore");
    let out = link(&mut g, &catalog, ingot, after, "ingot");

    let s = solve(&catalog, &g);
    assert_eq!(count(&s, ingot), None);
    assert_eq!(s.link_flow[&feed], None);
    assert_eq!(s.link_flow[&out], None);
    assert_eq!(s.storage[&before].balance, None);
    assert_eq!(s.storage[&after].balance, None);
    assert_eq!(s.sections.len(), 1);
    assert!(matches!(
        s.sections[0].status,
        SectionStatus::Undetermined(_)
    ));
    assert!(matches!(
        s.warnings.as_slice(),
        [Warning::Undetermined { section: 0, .. }]
    ));
}

#[test]
fn golden_08_undetermined_lone_node() {
    let catalog = furnace_catalog();
    let mut g = graph();
    let ingot = add_recipe(&mut g, &catalog, "ingot", None);

    let s = solve(&catalog, &g);
    assert_eq!(count(&s, ingot), None);
    assert_eq!(depot(&s, ingot, "ore", Side::Input), None);
    assert!(matches!(
        s.warnings.as_slice(),
        [Warning::Undetermined {
            reason: UndeterminedReason::NoLimit,
            ..
        }]
    ));
}

#[test]
fn undetermined_when_a_limit_exists_but_does_not_bound() {
    let catalog = furnace_catalog();
    let mut g = graph();
    // A storage drained through a splitter: one branch has a limit, the other doesn't, so the
    // section has a limit but nothing bounds it.
    let store = add(&mut g, NodeKind::Storage { item: None });
    let ingot = add_recipe(&mut g, &catalog, "ingot", Some(1.0));
    let splitter = add(&mut g, NodeKind::Splitter { item: None });
    let limited = sink(&mut g, "ingot", Some(10.0));
    let drain = sink(&mut g, "ingot", None);
    link(&mut g, &catalog, ingot, store, "ingot");
    link(&mut g, &catalog, store, splitter, "ingot");
    link(&mut g, &catalog, splitter, limited, "ingot");
    link(&mut g, &catalog, splitter, drain, "ingot");

    let s = solve(&catalog, &g);
    // The upstream section is fine; the drain's section is unbounded.
    assert_near(count(&s, ingot), 1.0);
    assert_eq!(rate(&s, drain), None);
    assert_eq!(s.storage[&store].balance, None);
    assert!(s.warnings.iter().any(|w| matches!(
        w,
        Warning::Undetermined {
            reason: UndeterminedReason::Unbounded,
            ..
        }
    )));
}

// 9. Storage decouples: upstream runs at its limits and the balance holds the difference.
#[test]
fn golden_09_storage_decouples() {
    let catalog = furnace_catalog();
    let mut g = graph();
    let first = add_recipe(&mut g, &catalog, "ingot", Some(1.0));
    let second = add_recipe(&mut g, &catalog, "ingot", Some(1.0));
    let store = add(&mut g, NodeKind::Storage { item: None });
    let plate = add_recipe(&mut g, &catalog, "plate", Some(1.0));
    link(&mut g, &catalog, first, store, "ingot");
    link(&mut g, &catalog, second, store, "ingot");
    link(&mut g, &catalog, store, plate, "ingot");

    let s = solve(&catalog, &g);
    assert_near(count(&s, first), 1.0);
    assert_near(count(&s, second), 1.0);
    assert_near(count(&s, plate), 1.0);
    let flow = &s.storage[&store];
    assert_near(flow.inflow, 60.0);
    assert_near(flow.outflow, 30.0);
    assert_near(flow.balance, 30.0);
    assert_near(flow.pass_through, 30.0);
    assert!(s.throttle.is_empty());
    assert_eq!(s.sections.len(), 2);
}

// 10. Lone node runs at its limit.
#[test]
fn golden_10_lone_node_runs_at_its_limit() {
    let catalog = furnace_catalog();
    let mut g = graph();
    let ingot = add_recipe(&mut g, &catalog, "ingot", Some(5.0));

    let s = solve(&catalog, &g);
    assert_near(count(&s, ingot), 5.0);
    assert_near(depot(&s, ingot, "ore", Side::Input), 150.0);
    assert_near(depot(&s, ingot, "ingot", Side::Output), 150.0);
}

// 11. Control ports in series: the tighter one wins.
#[test]
fn golden_11_control_ports_in_series() {
    let catalog = furnace_catalog();
    let mut g = graph();
    let ore = source(&mut g, "ore", 60.0);
    let loose = add(
        &mut g,
        NodeKind::ControlPort {
            item: None,
            limit: 40.0,
        },
    );
    let tight = add(
        &mut g,
        NodeKind::ControlPort {
            item: None,
            limit: 25.0,
        },
    );
    let goal = sink(&mut g, "ore", None);
    link(&mut g, &catalog, ore, loose, "ore");
    link(&mut g, &catalog, loose, tight, "ore");
    let out = link(&mut g, &catalog, tight, goal, "ore");

    let s = solve(&catalog, &g);
    assert_near(s.link_flow[&out], 25.0);
    assert_near(rate(&s, loose), 25.0);
    assert_near(rate(&s, goal), 25.0);
    assert_eq!(s.throttle[&loose].limited_by, Some(tight));
    assert_eq!(s.throttle[&ore].limited_by, Some(tight));
}

// 12. Fan-in evens out: each feeder supplies half.
#[test]
fn golden_12_fan_in_evens_out() {
    let catalog = furnace_catalog();
    let mut g = graph();
    let first = add_recipe(&mut g, &catalog, "ingot", Some(1.0));
    let second = add_recipe(&mut g, &catalog, "ingot", Some(1.0));
    let plate = add_recipe(&mut g, &catalog, "plate", Some(1.0));
    let a = link(&mut g, &catalog, first, plate, "ingot");
    let b = link(&mut g, &catalog, second, plate, "ingot");

    let s = solve(&catalog, &g);
    assert_near(count(&s, plate), 1.0);
    assert_near(s.link_flow[&a], 15.0);
    assert_near(s.link_flow[&b], 15.0);
    assert_near(count(&s, first), 0.5);
    assert_near(count(&s, second), 0.5);
    assert_eq!(s.throttle[&first].limited_by, Some(plate));
}

// 13. Sections are independent: an undetermined section doesn't blank a solved one.
#[test]
fn golden_13_sections_are_independent() {
    let catalog = furnace_catalog();
    let mut g = graph();
    let open = add_recipe(&mut g, &catalog, "ingot", None);
    let limited = add_recipe(&mut g, &catalog, "plate", Some(2.0));

    let s = solve(&catalog, &g);
    assert_eq!(count(&s, open), None);
    assert_near(count(&s, limited), 2.0);
    assert_eq!(s.sections.len(), 2);
    let solved = s
        .sections
        .iter()
        .filter(|r| r.status == SectionStatus::Solved)
        .count();
    assert_eq!(solved, 1);
}

// Beyond the golden list.

#[test]
fn fan_out_respects_a_branch_that_takes_less() {
    let catalog = TestCatalog::new()
        .facility("miner", 5.0)
        .facility("press", 5.0)
        .recipe("a", "miner", 2.0, &[], &[("x", 1.0)]) // 30 x
        .recipe("b", "press", 6.0, &[("x", 1.0)], &[("bx", 1.0)]) // 10 x each
        .build();
    let mut g = graph();
    let a = add_recipe(&mut g, &catalog, "a", Some(1.0));
    let small = add_recipe(&mut g, &catalog, "b", Some(0.5)); // takes at most 5
    let splitter = add(&mut g, NodeKind::Splitter { item: None });
    let big = add_recipe(&mut g, &catalog, "b", Some(5.0));
    let other = add_recipe(&mut g, &catalog, "b", Some(5.0));
    link(&mut g, &catalog, a, splitter, "x");
    let to_small = link(&mut g, &catalog, splitter, small, "x");
    let to_big = link(&mut g, &catalog, splitter, big, "x");
    let to_other = link(&mut g, &catalog, splitter, other, "x");

    let s = solve(&catalog, &g);
    // 30 split three ways: the small branch takes its 5, the other two share the rest evenly.
    assert_near(s.link_flow[&to_small], 5.0);
    assert_near(s.link_flow[&to_big], 12.5);
    assert_near(s.link_flow[&to_other], 12.5);
    assert_near(rate(&s, splitter), 30.0);
}

#[test]
fn belt_over_capacity_is_a_hint() {
    let catalog = furnace_catalog();
    let mut g = graph();
    let ingot = add_recipe(&mut g, &catalog, "ingot", Some(3.0)); // 90 ingot per minute
    let goal = sink(&mut g, "ingot", None);
    let wire = link(&mut g, &catalog, ingot, goal, "ingot");

    let s = solve(&catalog, &g);
    assert_near(s.link_flow[&wire], 90.0);
    assert_eq!(
        s.warnings,
        vec![Warning::BeltOverCapacity {
            link: wire,
            lanes: 3
        }]
    );
}

#[test]
fn totals_sum_depot_power_and_facilities() {
    let catalog = TestCatalog::new()
        .facility("furnace", 5.0)
        .facility("power_sta_1", 0.0)
        .recipe("ingot", "furnace", 2.0, &[("ore", 1.0)], &[("ingot", 1.0)])
        .recipe_with(
            "burn",
            &["power_sta_1"],
            40.0,
            &[("battery", 1.0)],
            &[],
            Some(-220.0),
            &[],
        )
        .build();
    let mut g = graph();
    add_recipe(&mut g, &catalog, "ingot", Some(1.5));
    add_recipe(&mut g, &catalog, "ingot", Some(0.5));
    add_recipe(&mut g, &catalog, "burn", Some(2.0));

    let s = solve(&catalog, &g);
    let totals = &s.totals;
    assert!(totals.complete);
    assert_near(totals.depot_in.get(&item("ore")).copied(), 60.0);
    assert_near(totals.depot_out.get(&item("ingot")).copied(), 60.0);
    assert_near(totals.depot_in.get(&item("battery")).copied(), 3.0);
    assert_near(Some(totals.power_draw_kw), 10.0);
    assert_near(Some(totals.power_generation_kw), 440.0);
    assert_near(Some(totals.power_net_kw), 430.0);
    assert_near(totals.facilities.get("furnace").copied(), 2.0);
    assert_near(totals.facilities.get("power_sta_1").copied(), 2.0);
}

#[test]
fn missing_recipe_nodes_are_left_out_with_a_warning() {
    let catalog = furnace_catalog();
    let mut g = graph();
    add_recipe(&mut g, &catalog, "ingot", Some(1.0));
    let json = g
        .to_json("v1")
        .replace("\"recipe\": \"ingot\"", "\"recipe\": \"gone\"");
    let loaded = Graph::from_json(&catalog, &json).unwrap().graph;

    let s = solve(&catalog, &loaded);
    assert!(s.facility_count.is_empty() || s.facility_count.values().all(Option::is_none));
    assert!(s.warnings.iter().any(|w| matches!(w, Warning::Issue(_))));
}

// Cross-check with SatisfactoryTools' "Add producer of X" (docs/ref/sftools-add-node-flow.md).
// SFTools sizes a new producer to the consumer's remaining demand by hand; here a producer added
// with no limit should be sized the same way by the solver.

fn ai_limiter_catalog() -> Catalog {
    TestCatalog::new()
        .facility("assembler", 15.0)
        .facility("constructor", 4.0)
        // 25 copper sheet + 100 quickwire → 5 AI limiters per minute.
        .recipe(
            "ai_limiter",
            "assembler",
            12.0,
            &[("copper_sheet", 5.0), ("quickwire", 20.0)],
            &[("ai_limiter", 1.0)],
        )
        // 20 copper ingot → 10 copper sheet per minute.
        .recipe(
            "copper_sheet",
            "constructor",
            6.0,
            &[("copper_ingot", 2.0)],
            &[("copper_sheet", 1.0)],
        )
        .build()
}

// SFTools F2: a producer added for an unmet input is sized to that input.
#[test]
fn sftools_f2_added_producer_meets_the_demand() {
    let catalog = ai_limiter_catalog();
    let mut g = graph();
    let limiter = add_recipe(&mut g, &catalog, "ai_limiter", Some(1.0));
    let sheets = add_recipe(&mut g, &catalog, "copper_sheet", None);
    let wire = link(&mut g, &catalog, sheets, limiter, "copper_sheet");

    let s = solve(&catalog, &g);
    assert_near(count(&s, limiter), 1.0);
    assert_near(count(&s, sheets), 2.5);
    assert_near(s.link_flow[&wire], 25.0);
    assert_near(depot(&s, sheets, "copper_ingot", Side::Input), 50.0);
    assert_near(depot(&s, limiter, "quickwire", Side::Input), 100.0);
}

// SFTools F3: the demand is partly met by a feeder that can only give 10. The new producer covers
// the remaining 15.
#[test]
fn sftools_f3_added_producer_covers_the_rest() {
    let catalog = ai_limiter_catalog();
    let mut g = graph();
    let limiter = add_recipe(&mut g, &catalog, "ai_limiter", Some(1.0));
    let existing = add_recipe(&mut g, &catalog, "copper_sheet", Some(1.0));
    let added = add_recipe(&mut g, &catalog, "copper_sheet", None);
    let old_wire = link(&mut g, &catalog, existing, limiter, "copper_sheet");
    let new_wire = link(&mut g, &catalog, added, limiter, "copper_sheet");

    let s = solve(&catalog, &g);
    assert_near(s.link_flow[&old_wire], 10.0);
    assert_near(s.link_flow[&new_wire], 15.0);
    assert_near(count(&s, added), 1.5);
}

// Where we differ from SFTools on purpose: if the existing feeder has spare capacity, the fan-in
// draws evenly from both feeders (GLOSSARY: Fan-in) instead of filling the existing one first.
#[test]
fn added_producer_shares_evenly_with_a_feeder_that_has_room() {
    let catalog = ai_limiter_catalog();
    let mut g = graph();
    let limiter = add_recipe(&mut g, &catalog, "ai_limiter", Some(1.0));
    let existing = add_recipe(&mut g, &catalog, "copper_sheet", Some(3.0));
    let added = add_recipe(&mut g, &catalog, "copper_sheet", None);
    let old_wire = link(&mut g, &catalog, existing, limiter, "copper_sheet");
    let new_wire = link(&mut g, &catalog, added, limiter, "copper_sheet");

    let s = solve(&catalog, &g);
    assert_near(s.link_flow[&old_wire], 12.5);
    assert_near(s.link_flow[&new_wire], 12.5);
    assert_near(count(&s, existing), 1.25);
    assert_near(count(&s, added), 1.25);
}
