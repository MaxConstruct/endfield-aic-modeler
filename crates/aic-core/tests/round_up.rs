//! The round-up view: whole facilities to build, and how busy each one is.

mod common;

use aic_core::{Graph, RoundUp, solve};
use common::*;

#[test]
fn a_fractional_count_rounds_up_to_whole_facilities() {
    assert_eq!(
        RoundUp::of(1.75),
        RoundUp {
            facilities: 2,
            utilization: 0.875
        }
    );
    assert_eq!(
        RoundUp::of(3.0),
        RoundUp {
            facilities: 3,
            utilization: 1.0
        }
    );
}

#[test]
fn solver_noise_does_not_add_a_facility() {
    assert_eq!(RoundUp::of(2.000_000_01).facilities, 2);
}

#[test]
fn no_facilities_means_no_utilization() {
    assert_eq!(
        RoundUp::of(0.0),
        RoundUp {
            facilities: 0,
            utilization: 0.0
        }
    );
}

#[test]
fn totals_round_up_each_node_before_adding() {
    let catalog = TestCatalog::new()
        .facility("furnace", 5.0)
        .recipe("ingot", "furnace", 2.0, &[("ore", 1.0)], &[("ingot", 1.0)])
        .build();
    let mut g = Graph::new(region(TUNDRA));
    add_recipe(&mut g, &catalog, "ingot", Some(0.5));
    add_recipe(&mut g, &catalog, "ingot", Some(0.5));

    let totals = solve(&catalog, &g).totals;
    assert_eq!(totals.facilities[&furnace()], 1.0);
    assert_eq!(totals.facilities_rounded[&furnace()], 2);
}

fn furnace() -> aic_data::FacilityId {
    aic_data::FacilityId::from("furnace")
}
