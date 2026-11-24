//! The extractor against the hand-written tables in `tests/fixtures/Table`: tables in, data file
//! and names out.

use std::path::{Path, PathBuf};

use aic_data::file::{DataFile, RecipeEntry, RecipeKind};
use aic_data::{Catalog, Names, Phase};
use aic_extract::{Args, ExtractError, Extraction, NameKind, NamesFile, Tables, extract, run};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn tables() -> Tables {
    Tables::read(&fixtures().join("Table"), "EN").expect("fixture tables read")
}

fn previous() -> DataFile {
    let file = fixtures().join("previous.json");
    DataFile::from_json(&std::fs::read(&file).unwrap(), &file).expect("previous loads")
}

fn names() -> NamesFile {
    NamesFile::read(&fixtures().join("names.json")).unwrap()
}

/// The extraction with the previous version, and the names table it leaves.
fn extraction() -> (Extraction, NamesFile) {
    let mut names = names();
    let extraction = extract(&tables(), Some(&previous()), &mut names, "v9.9.9").expect("extracts");
    (extraction, names)
}

fn recipe<'a>(data: &'a DataFile, id: &str) -> &'a RecipeEntry {
    (data.recipes.iter())
        .find(|r| r.id == id)
        .unwrap_or_else(|| panic!("no recipe {id}"))
}

fn pairs(map: &indexmap::IndexMap<String, f64>) -> Vec<(&str, f64)> {
    map.iter().map(|(k, v)| (k.as_str(), *v)).collect()
}

#[test]
fn what_it_writes_loads_as_a_catalog_with_every_name() {
    let (Extraction { data, .. }, names) = extraction();
    let names_json = names.to_json();
    let catalog = Catalog::from_bytes(
        data.to_json().as_bytes(),
        Names {
            file: "names.json",
            json: names_json.as_bytes(),
        },
    )
    .expect("the data loads");
    assert!(catalog.warnings().is_empty(), "{:?}", catalog.warnings());
    assert_eq!(catalog.version(), "v9.9.9");
}

#[test]
fn a_craft_takes_the_games_id_time_and_items() {
    let (Extraction { data, .. }, _) = extraction();
    let craft = recipe(&data, "furnance_copper_nugget_1");
    // 2 rounds of 1000 ms.
    assert_eq!(craft.time_s, 2.0);
    assert_eq!(pairs(&craft.inputs), [("copper_ore", 1.0)]);
    assert_eq!(pairs(&craft.outputs), [("copper_nugget", 1.0)]);
    assert_eq!(craft.facilities, ["furnance_1"]);
    // Cuprium is a product (showing type 4) and shown only in Wuling.
    assert_eq!(craft.category, "type_4");
    assert_eq!(craft.regions, ["jinlong"]);
    assert_eq!(craft.kind, None);
}

#[test]
fn the_same_craft_on_two_facilities_is_one_recipe_named_after_the_first() {
    let (Extraction { data, .. }, names) = extraction();
    let pool = recipe(&data, "pool_sewage_1");
    assert_eq!(pool.facilities, ["mix_pool_1", "mix_pool_2"]);
    assert!(data.recipes.iter().all(|r| r.id != "pool_sewage_2"));
    assert_eq!(names.recipes["pool_sewage_1"], "Sewage");
}

#[test]
fn ingredient_groups_are_flattened_in_order_and_a_gas_environment_named() {
    let (Extraction { data, .. }, _) = extraction();
    let oven = recipe(&data, "xiranite_oven_xiranite_powder_2");
    assert_eq!(
        pairs(&oven.inputs),
        [("copper_nugget", 1.0), ("liquid_water", 1.0)]
    );
    // 4 rounds of 500 ms.
    assert_eq!(oven.time_s, 2.0);
    assert_eq!(oven.environment.as_deref(), Some("gas_inert"));
    assert_eq!(recipe(&data, "furnance_copper_nugget_1").environment, None);
}

#[test]
fn facilities_take_the_id_of_the_item_that_places_them() {
    let (Extraction { data, .. }, names) = extraction();
    let ids: Vec<&str> = data.facilities.iter().map(|f| f.id.as_str()).collect();
    for id in [
        "phase_trans_2",
        "power_sta_1",
        "liquid_clean_gate",
        "liquid_recycle_gate",
    ] {
        assert!(ids.contains(&id), "{id} in {ids:?}");
    }
    // A building no recipe runs on is left out.
    assert!(!ids.contains(&"battle_cannon_1"));
    let furnace = data
        .facilities
        .iter()
        .find(|f| f.id == "furnance_1")
        .unwrap();
    assert_eq!((furnace.power_kw, furnace.regions.len()), (5.0, 0));
    let pump = data
        .facilities
        .iter()
        .find(|f| f.id == "gas_pump_1")
        .unwrap();
    // No power needed; recommended for Wuling only.
    assert_eq!(
        (pump.power_kw, pump.regions.clone()),
        (0.0, vec!["jinlong".to_owned()])
    );
    assert_eq!(names.items["phase_trans_2"], "Solid-Gas Transmuting Unit");
}

#[test]
fn a_transmuting_unit_has_its_activation_input_apart_from_its_recipes() {
    let (Extraction { data, .. }, _) = extraction();
    let unit = data
        .facilities
        .iter()
        .find(|f| f.id == "phase_trans_2")
        .unwrap();
    let activation = unit.activation.as_ref().expect("an activation input");
    assert_eq!(
        (activation.item.as_str(), activation.per_min),
        ("gas_xiranite", 6.0)
    );
    let craft = recipe(&data, "liquid_transmuter_2_xiranite_powder_1");
    assert_eq!(pairs(&craft.inputs), [("gas_xiranite", 5.0)]);
}

#[test]
fn mining_recipes_come_from_rigs_extractors_and_natural_liquids_only() {
    let (Extraction { data, .. }, _) = extraction();
    let ore = recipe(&data, "copper_ore");
    assert_eq!((ore.kind, ore.time_s), (Some(RecipeKind::Mining), 3.0));
    assert!(ore.inputs.is_empty());
    let wet = recipe(&data, "copper_ore-liquid_water");
    assert_eq!(pairs(&wet.inputs), [("liquid_water", 1.0)]);
    assert_eq!(wet.facilities, ["miner_4"]);
    assert_eq!(recipe(&data, "gas_inert").facilities, ["gas_pump_1"]);
    let water = recipe(&data, "liquid_water");
    assert_eq!((water.time_s, water.kind), (1.0, Some(RecipeKind::Mining)));
    // A pump can draw liquid grass from a tank, but it isn't found in the world.
    assert!(data.recipes.iter().all(|r| r.id != "liquid_plant_grass_1"));
}

#[test]
fn the_other_buildings_get_recipes_of_their_own() {
    let (Extraction { data, .. }, names) = extraction();
    let power = recipe(&data, "power_proc_battery_1");
    assert_eq!((power.time_s, power.power_kw), (40.0, Some(-220.0)));
    assert_eq!(power.category, "gen-power");
    assert!(power.outputs.is_empty());
    assert_eq!(
        names.recipes["power_proc_battery_1"],
        "Power(LC Valley Battery)"
    );

    let cleaner = recipe(&data, "liquid_cleaner_1-liquid_sewage");
    assert_eq!(
        (cleaner.time_s, pairs(&cleaner.inputs)),
        (2.0, vec![("liquid_sewage", 1.0)])
    );
    assert_eq!(
        names.recipes["liquid_cleaner_1-liquid_sewage"],
        "Water Treatment Unit(Sewage)"
    );

    let inlet = recipe(&data, "liquid_clean_gate-liquid_sewage");
    assert_eq!(inlet.time_s, 0.5);
    // Three inlets at 2 a second fill 30 in 5 s.
    let outlet = recipe(&data, "liquid_recycle_gate-liquid_xiranite_poly");
    assert_eq!(outlet.time_s, 5.0);
    assert_eq!(pairs(&outlet.inputs), [("liquid_sewage", 30.0)]);
    assert_eq!(pairs(&outlet.outputs), [("liquid_xiranite_poly", 1.0)]);

    // 6 Inergen a minute, making nothing.
    let vapour = recipe(&data, "vaporizer_1-gas_inert");
    assert_eq!(
        (vapour.time_s, pairs(&vapour.inputs)),
        (60.0, vec![("gas_inert", 6.0)])
    );
    assert!(vapour.outputs.is_empty());
}

#[test]
fn outposts_trade_at_their_highest_level_and_the_larger_price_wins() {
    let (Extraction { data, report }, names) = extraction();
    let trade = recipe(&data, "tundra_coupon-iron_cmpt");
    assert_eq!(trade.kind, Some(RecipeKind::Trade));
    assert_eq!(pairs(&trade.outputs), [("domain_tundra_coupon", 3.0)]);
    assert_eq!(trade.facilities, ["settlement-tundra_coupon"]);
    assert_eq!(
        (trade.regions.clone(), trade.category.as_str()),
        (vec!["tundra".to_owned()], "settlement")
    );
    assert_eq!(
        recipe(&data, "tundra_coupon-proc_battery_1").outputs["domain_tundra_coupon"],
        16.0
    );
    let conflicts: Vec<(&str, f64)> = (report.price_conflicts.iter())
        .map(|c| (c.item.as_str(), c.kept))
        .collect();
    assert_eq!(conflicts, [("iron_cmpt", 3.0)]);
    assert_eq!(
        names.recipes["tundra_coupon-iron_cmpt"],
        "Valley Stock Bill(Ferrium Part)"
    );
    // Trades come last.
    assert_eq!(data.recipes.last().unwrap().kind, Some(RecipeKind::Trade));
}

#[test]
fn items_have_their_phase_and_keep_the_previous_versions_colours() {
    let (Extraction { data, .. }, _) = extraction();
    let item = |id: &str| data.items.iter().find(|i| i.id == id).unwrap();
    assert_eq!(item("liquid_water").phase, Phase::Liquid);
    assert_eq!(item("gas_inert").phase, Phase::Gas);
    // A coupon has no factory row: it is solid.
    assert_eq!(item("domain_tundra_coupon").phase, Phase::Solid);
    assert_eq!(item("copper_ore").color.as_deref(), Some("#aa5533"));
    assert_eq!(item("gas_inert").color, None);
}

#[test]
fn belt_pipe_regions_and_categories() {
    let (Extraction { data, .. }, names) = extraction();
    assert_eq!((data.belt_per_min, data.pipe_per_min), (30.0, 120.0));
    assert_eq!(data.regions, ["tundra", "jinlong"]);
    // In the game's order of item types, then power and trades; only those a recipe has.
    assert_eq!(
        data.categories,
        ["type_1", "type_4", "gen-power", "settlement"]
    );
    assert_eq!(names.categories["type_1"], "Minerals");
    assert_eq!(names.locations["jinlong"], "Wuling");
}

#[test]
fn a_filled_container_is_named_after_what_fills_it() {
    let (_, names) = extraction();
    // The game calls both "Ferrium Bottle".
    assert_eq!(names.items["iron_bottle"], "Ferrium Bottle");
    assert_eq!(
        names.items["fbottle_iron_water"],
        "Ferrium Bottle(Clean Water)"
    );
}

#[test]
fn filling_and_emptying_a_container_are_named_packaged_and_unpackage() {
    let (Extraction { report, .. }, names) = extraction();
    // The game calls them "Clean Water (Bottled)" and "Ferrium Bottle Production", the second
    // for every liquid a Ferrium Bottle can hold. Ours replace the table's.
    assert_eq!(
        names.recipes["filling_bottled_iron_water"],
        "Packaged Clean Water (Ferrium Bottle)"
    );
    assert_eq!(
        names.recipes["dismantler_iron_water_1"],
        "Unpackage Clean Water (Ferrium Bottle)"
    );
    let renamed: Vec<&str> = (report.names_changed.iter())
        .map(|d| d.id.as_str())
        .collect();
    assert!(
        renamed.contains(&"filling_bottled_iron_water"),
        "{renamed:?}"
    );
    // A recipe that only mixes a solid into a liquid is no packaging: nothing takes it apart.
    assert_eq!(
        names.recipes["xiranite_oven_xiranite_powder_2"],
        "Xiranite (Inergen)"
    );
}

#[test]
fn recipes_the_game_names_alike_are_told_apart_by_an_input_or_their_gas_environment() {
    let (Extraction { report, .. }, names) = extraction();
    // Two ways to make Cuprium: each is named after the input the other lacks, as the names
    // table calls it (the fixture's own name for the ore).
    assert_eq!(
        names.recipes["furnance_copper_nugget_1"],
        "Cuprium (Copper Ore (ours))"
    );
    assert_eq!(
        names.recipes["xiranite_oven_copper_nugget_1"],
        "Cuprium (Xiranite)"
    );
    // The one that takes nothing the other doesn't keeps the plain name.
    assert_eq!(names.recipes["copper_ore"], "Cuprium Ore");
    assert_eq!(
        names.recipes["copper_ore-liquid_water"],
        "Cuprium Ore (Clean Water)"
    );
    // The same items in: the gas environment one of them needs tells them apart.
    assert_eq!(
        names.recipes["xiranite_oven_heavy_1"],
        "Heavy Xiragen Purification"
    );
    assert_eq!(
        names.recipes["xiranite_oven_heavy_2"],
        "Heavy Xiragen Purification (Inergen)"
    );
    assert!(report.shared_names.is_empty(), "{:?}", report.shared_names);
}

/// Where each recipe is in the file.
fn order(data: &DataFile) -> impl Fn(&str) -> usize + '_ {
    |id| {
        (data.recipes.iter())
            .position(|r| r.id == id)
            .unwrap_or_else(|| panic!("no recipe {id}"))
    }
}

#[test]
fn recipes_keep_the_previous_versions_order_and_packaging_comes_last() {
    let (Extraction { data, .. }, _) = extraction();
    let at = order(&data);
    // The previous version lists the sewage inlet before the furnace; the game's own order has
    // every craft before it.
    assert!(at("liquid_clean_gate-liquid_sewage") < at("furnance_copper_nugget_1"));
    // A recipe the previous version lacks comes right after the last one that makes the same.
    assert_eq!(
        at("xiranite_oven_copper_nugget_1"),
        at("furnance_copper_nugget_1") + 1
    );
    // Filling and emptying containers come after every other recipe but the trades: first the
    // fillings, then the emptyings.
    let trades = (data.recipes.iter())
        .position(|r| r.kind == Some(RecipeKind::Trade))
        .expect("the fixture has trades");
    assert!(
        data.recipes[trades..]
            .iter()
            .all(|r| r.kind == Some(RecipeKind::Trade))
    );
    assert_eq!(at("filling_bottled_iron_water"), trades - 2);
    assert_eq!(at("dismantler_iron_water_1"), trades - 1);
}

#[test]
fn without_a_previous_version_recipes_are_in_the_games_order_with_packaging_last() {
    let extraction = extract(&tables(), None, &mut names(), "v9.9.9").expect("extracts");
    let at = order(&extraction.data);
    // Mining comes first, then the crafts by building.
    assert_eq!(extraction.data.recipes[0].kind, Some(RecipeKind::Mining));
    assert!(at("furnance_copper_nugget_1") < at("xiranite_oven_xiranite_powder_2"));
    assert!(at("xiranite_oven_copper_nugget_1") < at("liquid_clean_gate-liquid_sewage"));
    assert!(at("liquid_clean_gate-liquid_sewage") < at("filling_bottled_iron_water"));
    assert!(at("filling_bottled_iron_water") < at("dismantler_iron_water_1"));
}

#[test]
fn names_are_added_where_missing_and_never_changed() {
    let (Extraction { report, .. }, names) = extraction();
    assert_eq!(names.items["copper_ore"], "Copper Ore (ours)");
    assert_eq!(names.items["copper_nugget"], "Cuprium");
    let differences: Vec<(NameKind, &str, &str)> = (report.name_differences.iter())
        .map(|d| (d.kind, d.id.as_str(), d.game.as_str()))
        .collect();
    // An outpost is not a building of the game, so its name is ours to keep too.
    assert_eq!(
        differences,
        [
            (NameKind::Item, "copper_ore", "Cuprium Ore"),
            (
                NameKind::Item,
                "settlement-tundra_coupon",
                "Outpost Supplies(Valley IV)"
            ),
        ]
    );
    // The existing keys keep their place; new ones come after.
    assert_eq!(
        names.items.keys().next().map(String::as_str),
        Some("copper_ore")
    );
    assert_eq!(names.name.as_deref(), Some("Fixture"));
}

#[test]
fn a_facility_takes_the_games_name_over_the_tables() {
    let (Extraction { report, .. }, names) = extraction();
    assert_eq!(names.items["liquid_clean_gate"], "Sewage Inlet");
    // It keeps its place in the table.
    assert_eq!(
        names.items.get_index_of("liquid_clean_gate"),
        Some(1),
        "{:?}",
        names.items.keys().take(3).collect::<Vec<_>>()
    );
    let renamed: Vec<(&str, &str, &str)> = (report.names_changed.iter())
        .filter(|d| d.kind == NameKind::Item)
        .map(|d| (d.id.as_str(), d.ours.as_str(), d.game.as_str()))
        .collect();
    assert_eq!(
        renamed,
        [(
            "liquid_clean_gate",
            "Purification Node(Sewage Inlet)",
            "Sewage Inlet"
        )]
    );
    // A name of our own making for a facility that is no building of the game stays.
    assert_eq!(
        names.items["settlement-tundra_coupon"],
        "Valley Outpost (ours)"
    );
}

#[test]
fn old_ids_get_aliases_to_the_recipe_and_item_that_took_their_place() {
    let (Extraction { data, report }, _) = extraction();
    let aliases = &data.aliases;
    assert_eq!(aliases.items["tundra_coupon"], "domain_tundra_coupon");
    assert_eq!(aliases.items.len(), 1);
    // Same facility, same rates.
    assert_eq!(aliases.recipes["copper_nugget"], "furnance_copper_nugget_1");
    // 2 per second: one inlet's 1 per 0.5 s.
    assert_eq!(
        aliases.recipes["sewage-treat"],
        "liquid_clean_gate-liquid_sewage"
    );
    // Kept its ID: no alias needed.
    assert!(!aliases.recipes.contains_key("tundra_coupon-iron_cmpt"));
    // Half the rate of anything new: none.
    assert!(!aliases.recipes.contains_key("slow_nugget"));
    let unaliased: Vec<&str> = report.unaliased.iter().map(|u| u.id.as_str()).collect();
    assert_eq!(unaliased, ["slow_nugget"]);
}

#[test]
fn without_a_previous_version_there_are_no_aliases_or_colours() {
    let data = extract(&tables(), None, &mut names(), "v9.9.9")
        .unwrap()
        .data;
    assert!(data.aliases.is_empty());
    assert!(data.items.iter().all(|i| i.color.is_none()));
}

#[test]
fn a_missing_table_names_its_file() {
    let error = Tables::read(&fixtures(), "EN").unwrap_err();
    let ExtractError::Read { file, .. } = &error else {
        panic!("expected a read error, got {error:?}");
    };
    assert!(file.ends_with("FactoryMachineCraftTable.json"), "{file:?}");
}

#[test]
fn a_craft_whose_group_is_missing_names_the_table_and_key() {
    let mut tables = tables();
    tables.craft_groups.shift_remove("group_oven");
    let error = extract(&tables, None, &mut names(), "v9.9.9").unwrap_err();
    let message = error.to_string();
    assert!(
        message.starts_with("FactoryMachineCraftGroupTable.json: `group_oven`"),
        "{message}"
    );
    assert!(
        message.contains("xiranite_oven_xiranite_powder_2"),
        "{message}"
    );
}

#[test]
fn an_unknown_domain_is_an_error() {
    let mut tables = tables();
    tables.buildings["furnance_1"].recommend_domains = vec!["domain_9".to_owned()];
    let error = extract(&tables, None, &mut names(), "v9.9.9").unwrap_err();
    assert!(error.to_string().contains("domain_9"), "{error}");
}

#[test]
fn run_writes_the_version_and_the_names_and_checks_they_load() {
    let dir = std::env::temp_dir().join(format!("aic-extract-run-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let names_file = dir.join("en.json");
    std::fs::copy(fixtures().join("names.json"), &names_file).unwrap();
    let args = Args::parse(
        [
            fixtures().join("Table").display().to_string(),
            "--version".into(),
            "v9.9.9".into(),
            "--previous".into(),
            fixtures().join("previous.json").display().to_string(),
            "--data-dir".into(),
            dir.display().to_string(),
            "--names".into(),
            names_file.display().to_string(),
        ]
        .map(String::from),
    )
    .unwrap();
    let report = run(&args).expect("runs");
    assert!(report.contains("recipe aliases"), "{report}");
    let written = dir.join("v9.9.9.json");
    let data = DataFile::from_json(&std::fs::read(&written).unwrap(), &written).unwrap();
    assert_eq!(data.version.as_deref(), Some("v9.9.9"));
    assert!(
        NamesFile::read(&names_file)
            .unwrap()
            .items
            .contains_key("copper_nugget")
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn arguments_need_a_folder_and_a_version() {
    let parse = |args: &[&str]| Args::parse(args.iter().map(|a| (*a).to_owned()));
    assert!(parse(&["Table"]).is_err());
    assert!(parse(&["--version", "v1"]).is_err());
    assert!(parse(&["Table", "--version", "v1", "--colour"]).is_err());
    let args = parse(&["Table", "--version", "v1"]).unwrap();
    assert_eq!(args.data_dir, Path::new("data"));
    assert_eq!(args.lang, "EN");
}

#[test]
fn the_apps_names_table_comes_back_byte_for_byte() {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("../aic-editor/i18n/names/en.json");
    let bytes = std::fs::read(&file).unwrap();
    let names = NamesFile::from_json(&bytes, &file).unwrap();
    assert_eq!(names.to_json().as_bytes(), bytes.as_slice());
}
