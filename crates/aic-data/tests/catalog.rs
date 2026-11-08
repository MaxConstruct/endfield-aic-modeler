//! `Catalog::from_bytes` against the hand-written fixture in `tests/fixtures/v1.10.0/`.

use aic_data::{Catalog, FacilityId, ItemId, LoadWarning, NameKind, RegionId};

const DATA: &[u8] = include_bytes!("fixtures/v1.10.0/data.json");
const EN: &[u8] = include_bytes!("fixtures/v1.10.0/i18n/en.json");

fn fixture() -> Catalog {
    Catalog::from_bytes(DATA, EN, None).expect("fixture loads")
}

fn items(pairs: &[(&str, f64)]) -> Vec<(ItemId, f64)> {
    pairs
        .iter()
        .map(|&(id, qty)| (ItemId::from(id), qty))
        .collect()
}

#[test]
fn counts_items_facilities_and_recipes() {
    let catalog = fixture();
    // Facilities are items too: every facility can also be crafted or carried.
    assert_eq!(catalog.items().len(), 21);
    assert_eq!(catalog.facilities().len(), 10);
    assert_eq!(catalog.recipes().len(), 8);
    assert_eq!(catalog.regions().len(), 2);
}

#[test]
fn version_comes_from_data_json() {
    assert_eq!(fixture().version(), "v1.10.0");
}

#[test]
fn belt_and_pipe_capacity_is_per_minute() {
    let catalog = fixture();
    assert_eq!(catalog.belt_per_min(), 30.0);
    assert_eq!(catalog.pipe_per_min(), 120.0);
}

#[test]
fn byproduct_recipe_keeps_every_output() {
    let recipe = fixture().recipe("copper_nugget").cloned().unwrap();
    assert_eq!(
        recipe.inputs,
        items(&[("copper_ore", 1.0), ("liquid_water", 1.0)])
    );
    assert_eq!(
        recipe.outputs,
        items(&[("copper_nugget", 1.0), ("liquid_sewage", 1.0)])
    );
    assert_eq!(recipe.time_s, 2.0);
    assert_eq!(recipe.facilities, vec![FacilityId::from("furnance_1")]);
}

#[test]
fn rate_per_min_scales_by_time_and_speed() {
    let catalog = fixture();
    let copper = catalog.recipe("copper_nugget").unwrap();
    // 1 per 2 s is 30 per minute.
    assert_eq!(copper.rate_per_min(1.0, 1.0), 30.0);
    let mix = catalog.recipe("liquid_sewage-mix").unwrap();
    let fast = catalog.facility("mix_pool_2").unwrap();
    // 2 per 3 s is 40 per minute, doubled by speed 2.
    assert_eq!(mix.rate_per_min(2.0, fast.speed), 80.0);
}

#[test]
fn mining_recipe_has_no_inputs() {
    let recipe = fixture().recipe("liquid_water").cloned().unwrap();
    assert!(recipe.is_mining);
    assert!(recipe.inputs.is_empty());
    assert!(!fixture().recipe("copper_nugget").unwrap().is_mining);
}

#[test]
fn recipe_power_overrides_facility_power() {
    let catalog = fixture();
    let battery = catalog.recipe("power_proc_battery_1").unwrap();
    assert_eq!(battery.power_override_kw, Some(-220.0));
    assert!(battery.outputs.is_empty());
    assert_eq!(
        catalog.recipe("copper_nugget").unwrap().power_override_kw,
        None
    );
}

#[test]
fn facility_power_and_speed() {
    let catalog = fixture();
    let furnace = catalog.facility("furnance_1").unwrap();
    assert_eq!(furnace.speed, 1.0);
    assert_eq!(furnace.power_kw, 5.0);
    // Burner facilities have no `usage`; they draw no electricity.
    assert_eq!(catalog.facility("gas_pump_1").unwrap().power_kw, 0.0);
}

#[test]
fn recipe_lists_every_facility_in_order() {
    let mix = fixture().recipe("liquid_sewage-mix").cloned().unwrap();
    assert_eq!(
        mix.facilities,
        vec![
            FacilityId::from("mix_pool_1"),
            FacilityId::from("mix_pool_2")
        ]
    );
}

#[test]
fn outpost_and_transfer_recipes_are_hidden() {
    let catalog = fixture();
    assert!(
        catalog
            .recipe("tundra_coupon-copper_nugget")
            .unwrap()
            .hidden
    );
    assert!(
        catalog
            .recipe("transfer_tundra_copper_nugget")
            .unwrap()
            .hidden
    );
    assert!(!catalog.recipe("copper_nugget").unwrap().hidden);
    assert!(!catalog.recipe("liquid_water").unwrap().hidden);
}

#[test]
fn regions_limit_recipes_and_facilities() {
    let catalog = fixture();
    let tundra = RegionId::from("tundra");
    let jinlong = RegionId::from("jinlong");

    let copper = catalog.recipe("copper_nugget").unwrap();
    assert_eq!(copper.regions, vec![jinlong.clone()]);
    assert!(copper.available_in(&jinlong));
    assert!(!copper.available_in(&tundra));

    // No `locations` means every region.
    let seed = catalog.recipe("plant_grass_seed_2").unwrap();
    assert!(seed.available_in(&tundra) && seed.available_in(&jinlong));

    assert!(!catalog.facility("pump_1").unwrap().available_in(&tundra));
    assert!(
        catalog
            .facility("furnance_1")
            .unwrap()
            .available_in(&tundra)
    );
}

#[test]
fn region_names_are_translated() {
    let catalog = fixture();
    assert_eq!(catalog.region("tundra").unwrap().name, "Valley IV");
    assert_eq!(catalog.region("jinlong").unwrap().name, "Wuling");
}

#[test]
fn fluids_are_detected_by_prefix() {
    let catalog = fixture();
    assert!(catalog.item("liquid_water").unwrap().is_fluid);
    assert!(catalog.item("liquid_sewage").unwrap().is_fluid);
    assert!(catalog.item("gas_inert").unwrap().is_fluid);
    assert!(!catalog.item("copper_ore").unwrap().is_fluid);
    // A facility whose ID starts with `gas_` is still a solid item.
    assert!(!catalog.item("gas_pump_1").unwrap().is_fluid);
}

#[test]
fn names_come_from_the_language_file() {
    let catalog = fixture();
    let copper = catalog.item("copper_nugget").unwrap();
    assert_eq!(copper.name, "Cuprium");
    assert_eq!(copper.name_zh, "赤铜块");
    assert_eq!(catalog.recipe("plant_grass_2").unwrap().name, "Yazhen");
    assert_eq!(catalog.facility("planter_1").unwrap().name, "Planting Unit");
}

#[test]
fn missing_name_falls_back_to_zh_and_warns_once() {
    let catalog = fixture();
    assert_eq!(catalog.item("gas_inert").unwrap().name, "惰性气体");
    let warnings: Vec<_> = catalog
        .warnings()
        .iter()
        .filter(|w| matches!(w, LoadWarning::MissingName { id, .. } if id == "gas_inert"))
        .collect();
    assert_eq!(
        warnings,
        vec![&LoadWarning::MissingName {
            kind: NameKind::Item,
            id: "gas_inert".to_owned()
        }]
    );
}

#[test]
fn icon_positions_become_pixel_rects() {
    let catalog = fixture();
    let icon = catalog.icon("copper_nugget").unwrap();
    assert_eq!(
        (icon.rect.x, icon.rect.y, icon.rect.w, icon.rect.h),
        (128, 64, 64, 64)
    );
    assert_eq!(icon.color.as_deref(), Some("#c87533"));
    assert_eq!(catalog.icon("missing-icon").unwrap().rect.x, 0);
    assert!(catalog.icons_webp().is_none());
}

#[test]
fn icon_bytes_are_kept_when_given() {
    let catalog = Catalog::from_bytes(DATA, EN, Some(b"webp bytes")).unwrap();
    assert_eq!(catalog.icons_webp(), Some(&b"webp bytes"[..]));
}

#[test]
fn writes_normalized_json() {
    let json = fixture().to_json();
    let value: serde_json::Value = serde_json::from_str(&json).expect("writer emits valid JSON");
    assert_eq!(value["items"]["copper_nugget"]["name"], "Cuprium");
    assert_eq!(
        value["recipes"]["copper_nugget"]["outputs"][1][0],
        "liquid_sewage"
    );
    assert_eq!(value["belt_per_min"], 30.0);
}

#[test]
fn raw_resources_are_what_mining_recipes_can_extract_in_the_region() {
    let catalog = fixture();
    let raw = |region: &str| -> Vec<String> {
        catalog
            .raw_resources(&RegionId::from(region))
            .into_iter()
            .map(ItemId::to_string)
            .collect()
    };
    assert_eq!(raw("jinlong"), ["liquid_water"]);
    // The water recipe is listed everywhere, but its only facility, the pump, is Wuling-only.
    assert!(raw("tundra").is_empty());
}
