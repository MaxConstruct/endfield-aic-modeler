//! `Catalog::from_bytes` against the hand-written fixture `tests/fixtures/data/v1.10.0.json`.

use aic_data::{
    Catalog, FacilityId, ImageKind, ItemId, LoadWarning, NameKind, Names, Phase, RegionId,
};

const DATA: &[u8] = include_bytes!("fixtures/data/v1.10.0.json");
const EN: Names = Names {
    file: "names/en.json",
    json: include_bytes!("fixtures/names/en.json"),
};

fn fixture() -> Catalog {
    Catalog::from_bytes(DATA, EN).expect("fixture loads")
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
    assert_eq!(catalog.items().len(), 9);
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
fn an_item_is_solid_unless_its_phase_says_otherwise() {
    let catalog = fixture();
    assert!(catalog.item("liquid_water").unwrap().is_fluid());
    assert!(catalog.item("liquid_sewage").unwrap().is_fluid());
    assert!(catalog.item("gas_inert").unwrap().is_fluid());
    assert!(!catalog.item("copper_ore").unwrap().is_fluid());
    assert_eq!(catalog.item("gas_inert").unwrap().phase, Phase::Gas);
    assert_eq!(catalog.item("liquid_water").unwrap().phase, Phase::Liquid);
    // Facilities are not items (ADR 0014).
    assert!(catalog.item("gas_pump_1").is_none());
}

#[test]
fn names_come_from_the_language_file() {
    let catalog = fixture();
    let copper = catalog.item("copper_nugget").unwrap();
    assert_eq!(copper.name, "Cuprium");
    assert_eq!(catalog.recipe("plant_grass_2").unwrap().name, "Yazhen");
    assert_eq!(catalog.facility("planter_1").unwrap().name, "Planting Unit");
}

#[test]
fn missing_name_falls_back_to_the_id_and_warns_once() {
    let catalog = fixture();
    assert_eq!(catalog.item("gas_inert").unwrap().name, "gas_inert");
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
fn an_item_has_the_colour_its_entry_gives() {
    let catalog = fixture();
    let color = |id: &str| catalog.item(id).unwrap().color.as_deref();
    assert_eq!(color("copper_nugget"), Some("#c87533"));
    assert_eq!(color("copper_ore"), None);
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

#[test]
fn a_recipe_shows_the_first_image_it_has_outputs_then_inputs_then_facility() {
    let catalog = fixture();
    // The icons the app has, as it tells the catalog.
    let icon = |recipe: &str, icons: &[(ImageKind, &str)]| {
        catalog
            .recipe_image(recipe, |kind, id| icons.contains(&(kind, id)))
            .map(|(kind, id)| (kind, id.to_owned()))
    };
    // No images at all.
    assert_eq!(icon("copper_nugget", &[]), None);
    // Burning a battery has no outputs: its input stands for it, then its facility.
    let station = (ImageKind::Facility, "power_sta_1");
    assert_eq!(
        icon("power_proc_battery_1", &[station]),
        Some((ImageKind::Facility, "power_sta_1".into()))
    );
    let battery = (ImageKind::Item, "proc_battery_1");
    assert_eq!(
        icon("power_proc_battery_1", &[station, battery]),
        Some((ImageKind::Item, "proc_battery_1".into()))
    );
    // A byproduct recipe shows its first output.
    let outputs = [
        (ImageKind::Item, "liquid_sewage"),
        (ImageKind::Item, "copper_nugget"),
    ];
    assert_eq!(
        icon("copper_nugget", &outputs),
        Some((ImageKind::Item, "copper_nugget".into()))
    );
    // A facility's picture is not an item's.
    assert_eq!(
        icon("copper_nugget", &[(ImageKind::Facility, "copper_nugget")]),
        None
    );
}

/// The fixture's `data.json`, changed by `edit`, loaded.
fn fixture_with(edit: impl FnOnce(&mut serde_json::Value)) -> Catalog {
    let mut value: serde_json::Value = serde_json::from_slice(DATA).unwrap();
    edit(&mut value);
    Catalog::from_bytes(&serde_json::to_vec(&value).unwrap(), EN).expect("edited fixture loads")
}

#[test]
fn an_activation_input_is_added_to_every_recipe_of_its_facility() {
    use serde_json::json;
    let catalog = fixture_with(|v| {
        // furnance_1: 6 water a minute whatever it makes. On the 2 s copper recipe, which takes
        // water anyway, that is 0.2 more per craft.
        v["facilities"][0]["activation"] = json!({ "item": "liquid_water", "per_min": 6 });
        // planter_1: 3 copper ore a minute, which its 2 s recipe doesn't take: 0.1 per craft, last.
        v["facilities"][2]["activation"] = json!({ "item": "copper_ore", "per_min": 3 });
    });
    let copper = catalog.recipe("copper_nugget").unwrap();
    assert_eq!(
        copper.inputs,
        items(&[("copper_ore", 1.0), ("liquid_water", 1.2)])
    );
    // (`serde_json::Value` sorts the edited file's keys, so only the added input's place is
    // worth checking here.)
    let grass = catalog.recipe("plant_grass_2").unwrap();
    assert_eq!(grass.inputs.len(), 3);
    assert_eq!(
        grass.inputs.last(),
        Some(&(ItemId::from("copper_ore"), 0.1))
    );
    // What the facility needs per running unit is all of what a unit takes of the added input.
    let planter = catalog.facility("planter_1").unwrap();
    let ore = (grass.input_rates(planter))
        .find(|(item, _)| item.as_str() == "copper_ore")
        .map(|(_, rate)| rate)
        .unwrap();
    assert!(
        (ore.activation - 3.0).abs() < 1e-9 && ore.scaled == 0.0,
        "{ore:?}"
    );
}

#[test]
fn every_input_of_a_whole_units_facility_is_an_activation_input_of_which_nothing_scales() {
    use serde_json::json;
    // The Thermal Bank as if it ran in whole units: its battery recipe takes 1 per 40 s.
    let catalog = fixture_with(|v| v["facilities"][4]["whole_units"] = json!(true));
    let bank = catalog.facility("power_sta_1").unwrap();
    assert!(bank.whole_units());
    assert_eq!(bank.activation(), None);
    let battery = catalog.recipe("power_proc_battery_1").unwrap();
    // The recipe's own quantities are as the file has them: nothing is added.
    assert_eq!(battery.inputs, items(&[("proc_battery_1", 1.0)]));
    let rates: Vec<(&str, f64, f64)> = (battery.input_rates(bank))
        .map(|(item, rate)| (item.as_str(), rate.scaled, rate.activation))
        .collect();
    assert_eq!(rates, [("proc_battery_1", 0.0, 1.5)]);
    // A file without the key runs every facility as before.
    let fixture = fixture();
    assert!(fixture.facilities().values().all(|f| !f.whole_units()));
    let bank = fixture.facility("power_sta_1").unwrap();
    let battery = fixture.recipe("power_proc_battery_1").unwrap();
    let rate = battery.input_rates(bank).next().unwrap().1;
    assert_eq!((rate.scaled, rate.activation), (1.5, 0.0));
}

#[test]
fn a_dispersing_recipe_names_the_gas_whose_environment_it_creates() {
    let catalog = fixture_with(|v| v["recipes"][4]["creates_environment"] = "gas_inert".into());
    let recipe = catalog.recipe("power_proc_battery_1").unwrap();
    assert_eq!(recipe.creates_environment, Some(ItemId::from("gas_inert")));
    assert_eq!(
        fixture()
            .recipe("power_proc_battery_1")
            .unwrap()
            .creates_environment,
        None
    );
}

#[test]
fn an_array_says_what_it_is_stacked_from_and_a_forge_recipe_what_it_projects() {
    use serde_json::json;
    let catalog = fixture_with(|v| {
        // As `magic_heng_2` is two of each T1, and `magic_ming_1` one of itself.
        v["facilities"][6]["stack"] = json!({ "furnance_1": 2, "mix_pool_1": 2 });
        v["facilities"][5]["stack"] = json!({ "mix_pool_1": 1 });
        v["recipes"][5]["projects"] = json!({ "furnance_1": 1, "mix_pool_1": 1 });
    });
    let stack = |id: &str| -> Vec<(&str, u32)> {
        (catalog.facility(id).unwrap().stack.iter())
            .map(|(facility, n)| (facility.as_str(), *n))
            .collect()
    };
    assert_eq!(stack("mix_pool_2"), [("furnance_1", 2), ("mix_pool_1", 2)]);
    assert_eq!(stack("mix_pool_1"), [("mix_pool_1", 1)]);
    assert_eq!(stack("furnance_1"), []);
    let projects: Vec<(FacilityId, u32)> = catalog
        .recipe("liquid_sewage-mix")
        .unwrap()
        .projects
        .clone();
    assert_eq!(
        projects,
        [
            (FacilityId::from("furnance_1"), 1),
            (FacilityId::from("mix_pool_1"), 1)
        ]
    );
    assert!(catalog.recipe("copper_nugget").unwrap().projects.is_empty());
}

#[test]
fn a_trade_is_hidden_and_a_mining_recipe_is_mining() {
    let catalog = fixture();
    let trade = catalog.recipe("tundra_coupon-copper_nugget").unwrap();
    assert!(trade.is_trade && trade.hidden);
    assert!(!catalog.recipe("copper_nugget").unwrap().is_trade);
    assert!(catalog.recipe("liquid_water").unwrap().is_mining);
}

#[test]
fn a_recipe_can_name_the_gas_environment_it_needs() {
    let catalog = fixture_with(|v| v["recipes"][0]["environment"] = "gas_inert".into());
    let copper = catalog.recipe("copper_nugget").unwrap();
    assert_eq!(copper.environment, Some(ItemId::from("gas_inert")));
    assert_eq!(catalog.recipe("liquid_water").unwrap().environment, None);
}

#[test]
fn aliases_lead_from_an_older_versions_ids_to_this_ones() {
    let catalog = fixture_with(|v| {
        v["aliases"] = serde_json::json!({
            "items": { "tundra_voucher": "tundra_coupon" },
            "recipes": { "copper_ingot": "copper_nugget" }
        });
    });
    assert_eq!(
        catalog.item_alias("tundra_voucher").map(ItemId::as_str),
        Some("tundra_coupon")
    );
    assert_eq!(
        catalog.recipe_alias("copper_ingot").map(|id| id.as_str()),
        Some("copper_nugget")
    );
    assert_eq!(catalog.recipe_alias("copper_nugget"), None);
    assert_eq!(fixture().item_alias("tundra_voucher"), None);
}

#[test]
fn a_mining_recipe_that_takes_an_input_still_yields_only_its_outputs_as_raw_resources() {
    // A rig that needs water, as the Hydro Mining Rig does.
    let catalog = fixture_with(|v| v["recipes"][1]["in"] = serde_json::json!({ "copper_ore": 1 }));
    let raw: Vec<&str> = (catalog
        .raw_resources(&RegionId::from("jinlong"))
        .into_iter())
    .map(ItemId::as_str)
    .collect();
    assert_eq!(raw, ["liquid_water"]);
}
