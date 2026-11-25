//! `overlay.json`: our own fixes on top of `data.json`, merged when the data loads.

use aic_data::{Catalog, ItemId, LoadError, Names};

const DATA: &[u8] = include_bytes!("fixtures/data/v1.10.0.json");
const EN: Names = Names {
    file: "names/en.json",
    json: include_bytes!("fixtures/names/en.json"),
};

fn with_overlay(overlay: &str) -> Result<Catalog, LoadError> {
    Catalog::from_bytes_with_overlay(DATA, Some(overlay.as_bytes()), EN)
}

#[test]
fn an_overlay_adds_items_and_recipes() {
    let catalog = with_overlay(
        r#"{
          "items": { "add": [{ "id": "copper_wire" }] },
          "recipes": { "add": [{
            "id": "copper_wire", "category": "product", "time_s": 2,
            "in": { "copper_nugget": 1 }, "out": { "copper_wire": 2 }, "facilities": ["furnance_1"]
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
              "id": "plant_grass_seed_2", "category": "material", "time_s": 4,
              "in": { "plant_grass_2": 1 }, "out": { "plant_grass_seed_2": 2 }, "facilities": ["seedcol_1"]
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
fn an_overlay_replaces_a_facility() {
    let catalog = with_overlay(
        r#"{ "facilities": { "replace": [{ "id": "mix_pool_2", "speed": 3, "power_kw": 100 }] } }"#,
    )
    .unwrap();
    let pool = catalog.facility("mix_pool_2").unwrap();
    assert_eq!(pool.speed, 3.0);
    // A replaced entry is the whole entry: what it leaves out takes its default.
    assert!(pool.regions.is_empty());
}

#[test]
fn an_overlay_gives_a_facility_an_activation_input_on_top_of_its_recipes_inputs() {
    // `copper_nugget` on `furnance_1` takes 1 liquid_water per 2 s: 30/min.
    let catalog = with_overlay(
        r#"{ "facilities": { "replace": [{
          "id": "furnance_1", "power_kw": 5, "activation": { "item": "liquid_water", "per_min": 6 }
        }] } }"#,
    )
    .unwrap();
    let activation = (catalog.facility("furnance_1").unwrap().activation())
        .expect("furnance_1 has an activation input");
    assert_eq!(activation.item(), &ItemId::from("liquid_water"));
    assert_eq!(activation.per_min(), 6.0);
    // Every other facility has none.
    assert!(
        catalog
            .facility("planter_1")
            .unwrap()
            .activation()
            .is_none()
    );
    // The recipe now takes 30 + 6 per minute: 1.2 per 2 s craft.
    let recipe = catalog.recipe("copper_nugget").unwrap();
    let water = (recipe.inputs.iter()).find(|(item, _)| item.as_str() == "liquid_water");
    assert_eq!(water, Some(&(ItemId::from("liquid_water"), 1.2)));
}

#[test]
fn without_an_overlay_no_facility_has_an_activation_input() {
    let catalog = Catalog::from_bytes(DATA, EN).unwrap();
    assert!(
        catalog
            .facilities()
            .values()
            .all(|facility| facility.activation().is_none())
    );
}

#[test]
fn adding_what_exists_or_changing_what_doesnt_is_an_error() {
    let errors = [
        (
            r#"{ "recipes": { "add": [{ "id": "copper_nugget", "category": "material", "time_s": 1, "facilities": ["furnance_1"] }] } }"#,
            "recipes.add[0].id",
        ),
        (
            r#"{ "recipes": { "replace": [{ "id": "nope", "category": "material", "time_s": 1, "facilities": ["furnance_1"] }] } }"#,
            "recipes.replace[0].id",
        ),
        (r#"{ "items": { "remove": ["nope"] } }"#, "items.remove[0]"),
        (
            r#"{ "facilities": { "replace": [{ "id": "copper_ore" }] } }"#,
            "facilities.replace[0].id",
        ),
        (
            r#"{ "facilities": { "add": [{ "id": "furnance_1" }] } }"#,
            "facilities.add[0].id",
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
fn the_old_facility_keys_are_gone() {
    for overlay in [
        r#"{ "facility_speeds": { "mix_pool_2": 3 } }"#,
        r#"{ "facility_activation": { "furnance_1": { "item": "liquid_water", "per_min": 6 } } }"#,
    ] {
        match with_overlay(overlay) {
            Err(LoadError::Json { file, .. }) => assert_eq!(file.to_str(), Some("overlay.json")),
            other => panic!("{overlay}: expected a JSON error, got {other:?}"),
        }
    }
}

#[test]
fn an_activation_input_takes_only_an_item_and_a_rate() {
    let overlay = r#"{ "facilities": { "replace": [{ "id": "furnance_1", "activation": { "item": "liquid_water", "per_min": 6, "per_sec": 1 } }] } }"#;
    match with_overlay(overlay) {
        Err(LoadError::Json { file, path, .. }) => {
            assert_eq!(file.to_str(), Some("overlay.json"));
            assert_eq!(path, "facilities.replace[0].activation.per_sec");
        }
        other => panic!("expected a JSON error, got {other:?}"),
    }
}

#[test]
fn a_bad_overlay_reports_the_overlay_file() {
    match with_overlay(r#"{ "recipes": 3 }"#) {
        Err(LoadError::Json { file, .. }) => assert_eq!(file.to_str(), Some("overlay.json")),
        other => panic!("expected a JSON error, got {other:?}"),
    }
}
