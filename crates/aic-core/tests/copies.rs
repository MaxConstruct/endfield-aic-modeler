//! Groups, step 2: a group that stands for N identical copies. Limits inside define one copy; the
//! solver finds how many copies the rest of the graph can run.

mod common;

use aic_core::{
    Graph, GroupId, GroupMode, Member, NodeId, NodeKind, Port, PortRef, SectionStatus, Side,
    Solution, UndeterminedReason, Warning, solve,
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

/// 30 ore/min → 30 ingot/min per furnace, 5 kW each.
fn catalog() -> Catalog {
    TestCatalog::new()
        .facility("furnace", 5.0)
        .recipe("ingot", "furnace", 2.0, &[("ore", 1.0)], &[("ingot", 1.0)])
        .build()
}

/// ore source (limit `ore`) → [ingot furnace, limit `per_copy`] → ingot sink.
fn module(catalog: &Catalog, ore: f64, per_copy: Option<f64>) -> (Graph, GroupId, [NodeId; 3]) {
    let mut g = Graph::new(region(TUNDRA));
    let source = add(
        &mut g,
        NodeKind::Source {
            item: item("ore"),
            limit: ore,
        },
    );
    let furnace = add_recipe(&mut g, catalog, "ingot", per_copy);
    let goal = add(
        &mut g,
        NodeKind::Sink {
            item: item("ingot"),
            limit: None,
        },
    );
    g.connect(catalog, PortRef::any(source), PortRef::new(furnace, "ore"))
        .unwrap();
    g.connect(catalog, PortRef::new(furnace, "ingot"), PortRef::any(goal))
        .unwrap();
    let group = g.group(&[Member::Node(furnace)], "Smelting").unwrap();
    g.set_group_mode(group, GroupMode::Copies { limit: None })
        .unwrap();
    (g, group, [source, furnace, goal])
}

fn copies(solution: &Solution, group: GroupId) -> Option<f64> {
    *solution.copies.get(&group).expect("a copies group")
}

#[test]
fn the_graph_decides_how_many_copies_run() {
    let catalog = catalog();
    let (g, group, [source, furnace, goal]) = module(&catalog, 90.0, Some(1.0));
    let s = solve(&catalog, &g);

    assert_near(copies(&s, group), 3.0);
    // Inside shows one copy; outside sees all of them.
    assert_near(s.facility_count[&furnace], 1.0);
    assert_near(s.rate[&goal], 90.0);
    assert_near(s.rate[&source], 90.0);
    let out = Port {
        node: furnace,
        item: item("ingot"),
        side: Side::Output,
    };
    assert_near(s.port_flow[&out], 30.0);
    assert_near(s.group_flow[&out], 90.0);
    // Totals count every copy.
    assert_eq!(
        s.totals.facilities[&aic_data::FacilityId::from("furnace")],
        3.0
    );
    assert!((s.totals.power_draw_kw - 15.0).abs() < TOLERANCE);
    // 90/min needs three belts; that hint is the only warning.
    assert!(
        s.warnings
            .iter()
            .all(|w| matches!(w, Warning::BeltOverCapacity { .. })),
        "{:?}",
        s.warnings
    );
}

#[test]
fn a_copies_limit_caps_the_count() {
    let catalog = catalog();
    let (mut g, group, [source, ..]) = module(&catalog, 90.0, Some(1.0));
    g.set_group_mode(group, GroupMode::Copies { limit: Some(2.0) })
        .unwrap();
    let s = solve(&catalog, &g);
    assert_near(copies(&s, group), 2.0);
    assert_near(s.rate[&source], 60.0);
}

#[test]
fn copies_can_be_fractional_like_facility_counts() {
    let catalog = catalog();
    let (g, group, _) = module(&catalog, 45.0, Some(1.0));
    assert_near(copies(&solve(&catalog, &g), group), 1.5);
}

#[test]
fn a_copy_with_nothing_limiting_it_leaves_the_count_undetermined() {
    let catalog = catalog();
    let (g, group, [_, _, goal]) = module(&catalog, 90.0, None);
    let s = solve(&catalog, &g);
    assert_eq!(copies(&s, group), None);
    assert_eq!(s.rate[&goal], None);
    assert!(s.warnings.contains(&Warning::CopiesUndetermined { group }));
    assert!(s.sections.iter().any(|section| section.status
        == SectionStatus::Undetermined(UndeterminedReason::CopiesUndetermined)));
}

#[test]
fn copies_nest() {
    let catalog = catalog();
    let mut g = Graph::new(region(TUNDRA));
    let source = add(
        &mut g,
        NodeKind::Source {
            item: item("ore"),
            limit: 120.0,
        },
    );
    let furnace = add_recipe(&mut g, &catalog, "ingot", Some(1.0));
    g.connect(&catalog, PortRef::any(source), PortRef::new(furnace, "ore"))
        .unwrap();
    // Two furnaces per outer copy.
    let inner = g.group(&[Member::Node(furnace)], "Pair").unwrap();
    g.set_group_mode(inner, GroupMode::Copies { limit: Some(2.0) })
        .unwrap();
    let outer = g.group(&[Member::Group(inner)], "Block").unwrap();
    g.set_group_mode(outer, GroupMode::Copies { limit: None })
        .unwrap();

    let s = solve(&catalog, &g);
    assert_near(copies(&s, outer), 2.0);
    assert_near(copies(&s, inner), 2.0);
    assert_eq!(
        s.totals.facilities[&aic_data::FacilityId::from("furnace")],
        4.0
    );
}

#[test]
fn the_mode_survives_a_save_file() {
    let catalog = catalog();
    let (mut g, group, _) = module(&catalog, 90.0, Some(1.0));
    g.set_group_mode(group, GroupMode::Copies { limit: Some(4.0) })
        .unwrap();
    let loaded = Graph::from_json(&catalog, &g.to_json("test"))
        .unwrap()
        .graph;
    let (_, info) = loaded.groups().iter().next().unwrap();
    assert_eq!(info.mode, GroupMode::Copies { limit: Some(4.0) });
}
