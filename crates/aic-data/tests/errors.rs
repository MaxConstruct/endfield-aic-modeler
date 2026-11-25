//! Load errors name the file and the JSON path that failed.

use aic_data::{Catalog, LoadError, Names};
use serde_json::{Value, json};

const DATA: &[u8] = include_bytes!("fixtures/data/v1.10.0.json");
const EN: Names = Names {
    file: "names/en.json",
    json: include_bytes!("fixtures/names/en.json"),
};

/// The fixture's `data.json`, changed by `edit`.
fn data_with(edit: impl FnOnce(&mut Value)) -> Vec<u8> {
    let mut value: Value = serde_json::from_slice(DATA).unwrap();
    edit(&mut value);
    serde_json::to_vec(&value).unwrap()
}

fn error_for(data: &[u8]) -> LoadError {
    Catalog::from_bytes(data, EN).expect_err("should not load")
}

fn invalid_path(error: &LoadError) -> &str {
    match error {
        LoadError::Invalid { path, .. } => path,
        other => panic!("expected an Invalid error, got {other:?}"),
    }
}

#[test]
fn wrong_type_names_the_json_path() {
    let data = data_with(|v| v["recipes"][2]["time_s"] = json!("two"));
    let error = error_for(&data);
    let LoadError::Json { file, path, .. } = &error else {
        panic!("expected a Json error, got {error:?}");
    };
    assert_eq!(file.to_str(), Some("data.json"));
    assert_eq!(path, "recipes[2].time_s");
    assert!(
        error
            .to_string()
            .starts_with("data.json: at `recipes[2].time_s`: ")
    );
}

#[test]
fn malformed_json_is_an_error() {
    assert!(matches!(
        error_for(b"{ \"format\": 1, \"items\": ["),
        LoadError::Json { .. }
    ));
}

#[test]
fn bad_language_file_names_that_file() {
    let names = Names {
        file: "names/fr.json",
        json: br#"{ "items": { "belt": 3 } }"#,
    };
    let error = Catalog::from_bytes(DATA, names).unwrap_err();
    let LoadError::Json { file, path, .. } = &error else {
        panic!("expected a Json error, got {error:?}");
    };
    assert_eq!(file.to_str(), Some("names/fr.json"));
    assert_eq!(path, "items.belt");
}

#[test]
fn recipe_with_unknown_facility_is_refused() {
    let data = data_with(|v| v["recipes"][5]["facilities"][1] = json!("mix_pool_9"));
    let error = error_for(&data);
    assert_eq!(invalid_path(&error), "recipes[5].facilities[1]");
    assert!(error.to_string().contains("unknown facility `mix_pool_9`"));
}

#[test]
fn recipe_with_unknown_item_is_refused() {
    let data = data_with(|v| v["recipes"][0]["out"]["copper_dust"] = json!(1));
    assert_eq!(
        invalid_path(&error_for(&data)),
        "recipes[0].out.copper_dust"
    );
}

#[test]
fn recipe_without_facility_is_refused() {
    let data = data_with(|v| v["recipes"][0]["facilities"] = json!([]));
    assert_eq!(invalid_path(&error_for(&data)), "recipes[0].facilities");
}

#[test]
fn recipe_with_zero_time_is_refused() {
    let data = data_with(|v| v["recipes"][1]["time_s"] = json!(0));
    assert_eq!(invalid_path(&error_for(&data)), "recipes[1].time_s");
}

#[test]
fn recipe_with_unknown_region_is_refused() {
    let data = data_with(|v| v["recipes"][0]["regions"] = json!(["moon"]));
    assert_eq!(invalid_path(&error_for(&data)), "recipes[0].regions[0]");
}

#[test]
fn duplicate_recipe_id_is_refused() {
    let data = data_with(|v| v["recipes"][1]["id"] = json!("copper_nugget"));
    assert_eq!(invalid_path(&error_for(&data)), "recipes[1].id");
}

#[test]
fn belt_and_pipe_capacities_must_be_positive() {
    let data = data_with(|v| v["belt_per_min"] = json!(0));
    assert_eq!(invalid_path(&error_for(&data)), "belt_per_min");
    let data = data_with(|v| v["pipe_per_min"] = json!(-120));
    assert_eq!(invalid_path(&error_for(&data)), "pipe_per_min");
}

#[test]
fn a_file_of_another_format_names_its_number() {
    let data = data_with(|v| v["format"] = json!(2));
    let error = error_for(&data);
    assert_eq!(invalid_path(&error), "format");
    assert!(error.to_string().contains("format 2"), "{error}");
    // Even when the rest of the file is in a shape this build doesn't know.
    let error = error_for(br#"{ "format": 7, "spaceships": [] }"#);
    assert!(error.to_string().contains("format 7"), "{error}");
}

#[test]
fn a_file_without_a_format_is_refused() {
    let data = data_with(|v| {
        v.as_object_mut().unwrap().remove("format");
    });
    assert_eq!(invalid_path(&error_for(&data)), "format");
}

#[test]
fn unknown_keys_are_refused() {
    let data = data_with(|v| v["recipes"][0]["producers"] = json!(["furnance_1"]));
    let LoadError::Json { path, .. } = error_for(&data) else {
        panic!("expected a Json error");
    };
    assert_eq!(path, "recipes[0].producers");
}

#[test]
fn a_trade_takes_one_item_and_gives_something() {
    let data = data_with(|v| v["recipes"][6]["in"]["copper_ore"] = json!(1));
    assert_eq!(invalid_path(&error_for(&data)), "recipes[6].kind");
    let data = data_with(|v| v["recipes"][6]["out"] = json!({}));
    assert_eq!(invalid_path(&error_for(&data)), "recipes[6].kind");
}

#[test]
fn an_environment_must_be_an_item() {
    let data = data_with(|v| v["recipes"][0]["environment"] = json!("gas_nope"));
    let error = error_for(&data);
    assert_eq!(invalid_path(&error), "recipes[0].environment");
    assert!(error.to_string().contains("unknown item `gas_nope`"));
}

#[test]
fn a_bad_activation_input_is_refused() {
    let cases = [
        (
            json!({ "item": "nope", "per_min": 6 }),
            "facilities[0].activation.item",
        ),
        (
            json!({ "item": "liquid_water", "per_min": 0 }),
            "facilities[0].activation.per_min",
        ),
        (
            json!({ "item": "liquid_water", "per_min": -6 }),
            "facilities[0].activation.per_min",
        ),
    ];
    for (activation, expected) in cases {
        let data = data_with(|v| v["facilities"][0]["activation"] = activation.clone());
        assert_eq!(invalid_path(&error_for(&data)), expected, "{activation}");
    }
}

#[test]
fn facilities_that_share_a_recipe_must_agree_on_its_activation_input() {
    // `liquid_sewage-mix` runs on both mixing pools; only one of them has an activation input.
    let data = data_with(|v| {
        v["facilities"][5]["activation"] = json!({ "item": "liquid_water", "per_min": 6 });
    });
    let error = error_for(&data);
    assert_eq!(invalid_path(&error), "recipes[5].facilities");
    assert!(
        error.to_string().contains("`mix_pool_1` and `mix_pool_2`"),
        "{error}"
    );
    // The same activation at different speeds doesn't fold into one quantity per craft either.
    let data = data_with(|v| {
        let water = json!({ "item": "liquid_water", "per_min": 6 });
        v["facilities"][5]["activation"] = water.clone();
        v["facilities"][6]["activation"] = water;
    });
    assert_eq!(invalid_path(&error_for(&data)), "recipes[5].facilities");
}

#[test]
fn a_whole_units_facility_has_no_activation_input() {
    let data = data_with(|v| {
        v["facilities"][0]["whole_units"] = json!(true);
        v["facilities"][0]["activation"] = json!({ "item": "liquid_water", "per_min": 6 });
    });
    let error = error_for(&data);
    assert_eq!(invalid_path(&error), "facilities[0].whole_units");
    assert!(error.to_string().contains("activation"), "{error}");
}

#[test]
fn facilities_that_share_a_recipe_must_agree_on_whole_units() {
    // `liquid_sewage-mix` runs on both mixing pools.
    let data = data_with(|v| v["facilities"][5]["whole_units"] = json!(true));
    let error = error_for(&data);
    assert_eq!(invalid_path(&error), "recipes[5].facilities");
    assert!(
        error.to_string().contains("`mix_pool_1` and `mix_pool_2`"),
        "{error}"
    );
}

#[test]
fn a_stack_is_of_existing_facilities_each_a_positive_whole_number() {
    let cases = [
        (
            json!({ "nope_1": 1 }),
            "facilities[6].stack.nope_1",
            "unknown facility `nope_1`",
        ),
        (
            json!({ "mix_pool_1": 0 }),
            "facilities[6].stack.mix_pool_1",
            "positive",
        ),
    ];
    for (stack, expected, message) in cases {
        let data = data_with(|v| v["facilities"][6]["stack"] = stack.clone());
        let error = error_for(&data);
        assert_eq!(invalid_path(&error), expected, "{stack}");
        assert!(error.to_string().contains(message), "{error}");
    }
    // Not a whole number: the JSON doesn't read.
    let data = data_with(|v| v["facilities"][6]["stack"] = json!({ "mix_pool_1": 1.5 }));
    let LoadError::Json { path, .. } = error_for(&data) else {
        panic!("expected a Json error");
    };
    assert_eq!(path, "facilities[6].stack.mix_pool_1");
}

#[test]
fn what_a_recipe_projects_is_existing_facilities_each_a_positive_whole_number() {
    let cases = [
        (
            json!({ "nope_1": 1 }),
            "recipes[5].projects.nope_1",
            "unknown facility `nope_1`",
        ),
        (
            json!({ "mix_pool_1": 0 }),
            "recipes[5].projects.mix_pool_1",
            "positive",
        ),
    ];
    for (projects, expected, message) in cases {
        let data = data_with(|v| v["recipes"][5]["projects"] = projects.clone());
        let error = error_for(&data);
        assert_eq!(invalid_path(&error), expected, "{projects}");
        assert!(error.to_string().contains(message), "{error}");
    }
}

#[test]
fn a_created_environment_must_be_an_item() {
    let data = data_with(|v| v["recipes"][4]["creates_environment"] = json!("gas_nope"));
    let error = error_for(&data);
    assert_eq!(invalid_path(&error), "recipes[4].creates_environment");
    assert!(error.to_string().contains("unknown item `gas_nope`"));
}

#[test]
fn an_alias_goes_from_an_id_that_is_gone_to_one_that_exists() {
    let cases = [
        (
            json!({ "items": { "old_ore": "nope" } }),
            "aliases.items.old_ore",
        ),
        (
            json!({ "items": { "copper_ore": "copper_nugget" } }),
            "aliases.items.copper_ore",
        ),
        (
            json!({ "recipes": { "old": "nope" } }),
            "aliases.recipes.old",
        ),
        (
            json!({ "recipes": { "liquid_water": "copper_nugget" } }),
            "aliases.recipes.liquid_water",
        ),
    ];
    for (aliases, expected) in cases {
        let data = data_with(|v| v["aliases"] = aliases.clone());
        assert_eq!(invalid_path(&error_for(&data)), expected, "{aliases}");
    }
}
