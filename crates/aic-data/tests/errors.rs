//! Load errors name the file and the JSON path that failed.

use aic_data::{Catalog, LoadError};
use serde_json::{Value, json};

const DATA: &[u8] = include_bytes!("fixtures/v1.10.0/data.json");
const EN: &[u8] = include_bytes!("fixtures/v1.10.0/i18n/en.json");

/// The fixture's `data.json`, changed by `edit`.
fn data_with(edit: impl FnOnce(&mut Value)) -> Vec<u8> {
    let mut value: Value = serde_json::from_slice(DATA).unwrap();
    edit(&mut value);
    serde_json::to_vec(&value).unwrap()
}

fn error_for(data: &[u8]) -> LoadError {
    Catalog::from_bytes(data, EN, None).expect_err("should not load")
}

fn invalid_path(error: &LoadError) -> &str {
    match error {
        LoadError::Invalid { path, .. } => path,
        other => panic!("expected an Invalid error, got {other:?}"),
    }
}

#[test]
fn wrong_type_names_the_json_path() {
    let data = data_with(|v| v["recipes"][2]["time"] = json!("two"));
    let error = error_for(&data);
    let LoadError::Json { file, path, .. } = &error else {
        panic!("expected a Json error, got {error:?}");
    };
    assert_eq!(file.to_str(), Some("data.json"));
    assert_eq!(path, "recipes[2].time");
    assert!(
        error
            .to_string()
            .starts_with("data.json: at `recipes[2].time`: ")
    );
}

#[test]
fn malformed_json_is_an_error() {
    assert!(matches!(
        error_for(b"{ \"items\": ["),
        LoadError::Json { .. }
    ));
}

#[test]
fn bad_language_file_names_that_file() {
    let error = Catalog::from_bytes(DATA, br#"{ "items": { "belt": 3 } }"#, None).unwrap_err();
    let LoadError::Json { file, path, .. } = &error else {
        panic!("expected a Json error, got {error:?}");
    };
    assert_eq!(file.to_str(), Some("i18n.json"));
    assert_eq!(path, "items.belt");
}

#[test]
fn recipe_with_unknown_facility_is_refused() {
    let data = data_with(|v| v["recipes"][5]["producers"][1] = json!("mix_pool_9"));
    let error = error_for(&data);
    assert_eq!(invalid_path(&error), "recipes[5].producers[1]");
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
    let data = data_with(|v| v["recipes"][0]["producers"] = json!([]));
    assert_eq!(invalid_path(&error_for(&data)), "recipes[0].producers");
}

#[test]
fn recipe_with_zero_time_is_refused() {
    let data = data_with(|v| v["recipes"][1]["time"] = json!(0));
    assert_eq!(invalid_path(&error_for(&data)), "recipes[1].time");
}

#[test]
fn recipe_with_unknown_region_is_refused() {
    let data = data_with(|v| v["recipes"][0]["locations"] = json!(["moon"]));
    assert_eq!(invalid_path(&error_for(&data)), "recipes[0].locations[0]");
}

#[test]
fn duplicate_recipe_id_is_refused() {
    let data = data_with(|v| v["recipes"][1]["id"] = json!("copper_nugget"));
    assert_eq!(invalid_path(&error_for(&data)), "recipes[1].id");
}

#[test]
fn bad_icon_position_is_refused() {
    let data = data_with(|v| v["icons"][1]["position"] = json!("left top"));
    assert_eq!(invalid_path(&error_for(&data)), "icons[1].position");
}

#[test]
fn missing_belt_is_refused() {
    let data = data_with(|v| {
        v["items"][0].as_object_mut().unwrap().remove("belt");
        v["defaults"].as_object_mut().unwrap().remove("maxBelt");
    });
    assert_eq!(invalid_path(&error_for(&data)), "items");
}

#[test]
fn defaults_naming_a_non_belt_is_refused() {
    let data = data_with(|v| v["defaults"]["maxBelt"] = json!("pipe"));
    assert_eq!(invalid_path(&error_for(&data)), "defaults.maxBelt");
}
