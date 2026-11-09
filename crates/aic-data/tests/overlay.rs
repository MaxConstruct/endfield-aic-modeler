//! `overlay.json`: our own fixes on top of `data.json`, merged when the data loads.

use aic_data::{Catalog, ItemId, LoadError};

const DATA: &[u8] = include_bytes!("fixtures/v1.10.0/data.json");
const EN: &[u8] = include_bytes!("fixtures/v1.10.0/i18n/en.json");

fn with_overlay(overlay: &str) -> Result<Catalog, LoadError> {
    Catalog::from_bytes_with_overlay(DATA, Some(overlay.as_bytes()), EN, None)
}

#[test]
fn an_overlay_adds_items_and_recipes() {
    let catalog = with_overlay(
        r#"{
          "items": { "add": [{ "id": "copper_wire", "name": "铜线", "category": "product" }] },
          "recipes": { "add": [{
            "id": "copper_wire", "category": "product", "time": 2,
            "in": { "copper_nugget": 1 }, "out": { "copper_wire": 2 }, "producers": ["furnance_1"]
          }] }
        }"#,
    )
    .expect("overlay applies");
    let recipe = catalog.recipe("copper_wire").expect("recipe added");
    assert_eq!(recipe.outputs, [(ItemId::from("copper_wire"), 2.0)]);
    assert!(catalog.item("copper_wire").is_some());
}

#[test]
fn an_overlay_replaces_and_removes_recipes() {
    let catalog = with_overlay(
        r#"{
          "recipes": {
            "replace": [{
              "id": "plant_grass_seed_2", "category": "material", "time": 4,
              "in": { "plant_grass_2": 1 }, "out": { "plant_grass_seed_2": 2 }, "producers": ["seedcol_1"]
            }],
            "remove": ["liquid_sewage-mix"]
          }
        }"#,
    )
    .expect("overlay applies");
    assert_eq!(catalog.recipe("plant_grass_seed_2").unwrap().time_s, 4.0);
    assert!(catalog.recipe("liquid_sewage-mix").is_none());
    // The rest of the data is untouched, in its order.
    let first = catalog.recipes().keys().next().unwrap();
    assert_eq!(first.as_str(), "copper_nugget");
}

#[test]
fn an_overlay_overrides_facility_speeds() {
    let catalog = with_overlay(r#"{ "facility_speeds": { "mix_pool_2": 3 } }"#).unwrap();
    assert_eq!(catalog.facility("mix_pool_2").unwrap().speed, 3.0);
}

#[test]
fn adding_what_exists_or_changing_what_doesnt_is_an_error() {
    let errors = [
        (
            r#"{ "recipes": { "add": [{ "id": "copper_nugget", "category": "material", "time": 1, "producers": ["furnance_1"] }] } }"#,
            "recipes.add[0].id",
        ),
        (
            r#"{ "recipes": { "replace": [{ "id": "nope", "category": "material", "time": 1, "producers": ["furnance_1"] }] } }"#,
            "recipes.replace[0].id",
        ),
        (r#"{ "items": { "remove": ["nope"] } }"#, "items.remove[0]"),
        (
            r#"{ "facility_speeds": { "copper_ore": 2 } }"#,
            "facility_speeds.copper_ore",
        ),
    ];
    for (overlay, expected_path) in errors {
        match with_overlay(overlay) {
            Err(LoadError::Invalid { file, path, .. }) => {
                assert_eq!(file.to_str(), Some("overlay.json"));
                assert_eq!(path, expected_path);
            }
            other => panic!("{overlay}: expected an error at {expected_path}, got {other:?}"),
        }
    }
}

#[test]
fn a_bad_overlay_reports_the_overlay_file() {
    match with_overlay(r#"{ "recipes": 3 }"#) {
        Err(LoadError::Json { file, .. }) => assert_eq!(file.to_str(), Some("overlay.json")),
        other => panic!("expected a JSON error, got {other:?}"),
    }
}
