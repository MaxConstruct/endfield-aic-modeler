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
fn an_overlay_gives_a_facility_an_activation_input() {
    // `copper_nugget` on `furnance_1` takes 1 liquid_water per 2 s: 30/min.
    let catalog = with_overlay(
        r#"{ "facility_activation": { "furnance_1": { "item": "liquid_water", "per_min": 6 } } }"#,
    )
    .unwrap();
    let activation = (catalog.facility("furnance_1").unwrap().activation.as_ref())
        .expect("furnance_1 has an activation input");
    assert_eq!(activation.item(), &ItemId::from("liquid_water"));
    assert_eq!(activation.per_min(), 6.0);
    // Every other facility has none.
    assert!(catalog.facility("planter_1").unwrap().activation.is_none());
    // The recipe still lists the whole 30/min.
    let recipe = catalog.recipe("copper_nugget").unwrap();
    assert!(recipe.inputs.contains(&(ItemId::from("liquid_water"), 1.0)));
}

#[test]
fn without_an_overlay_no_facility_has_an_activation_input() {
    let catalog = Catalog::from_bytes(DATA, EN).unwrap();
    assert!(
        catalog
            .facilities()
            .values()
            .all(|facility| facility.activation.is_none())
    );
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
fn a_bad_activation_input_is_an_error() {
    let activation = |facility: &str, item: &str, per_min: f64| {
        format!(
            r#"{{ "facility_activation": {{ "{facility}": {{ "item": "{item}", "per_min": {per_min} }} }} }}"#
        )
    };
    let errors = [
        (
            activation("nope", "liquid_water", 6.0),
            "facility_activation.nope",
            "unknown facility `nope`",
        ),
        (
            activation("copper_ore", "liquid_water", 6.0),
            "facility_activation.copper_ore",
            "`copper_ore` is not a facility",
        ),
        (
            activation("furnance_1", "nope", 6.0),
            "facility_activation.furnance_1.item",
            "unknown item `nope`",
        ),
        (
            activation("furnance_1", "liquid_water", 0.0),
            "facility_activation.furnance_1.per_min",
            "expected a positive number, found 0",
        ),
        (
            activation("furnance_1", "liquid_water", -6.0),
            "facility_activation.furnance_1.per_min",
            "expected a positive number, found -6",
        ),
        // `copper_nugget` is the recipe's output, not one of its inputs.
        (
            activation("furnance_1", "copper_nugget", 6.0),
            "facility_activation.furnance_1",
            "recipe `copper_nugget` does not take `copper_nugget`",
        ),
        // The recipe takes 30/min of water at one facility.
        (
            activation("furnance_1", "liquid_water", 31.0),
            "facility_activation.furnance_1",
            "recipe `copper_nugget` takes 30/min of `liquid_water`, less than the 31/min each \
             running facility needs",
        ),
    ];
    for (overlay, expected_path, expected_message) in errors {
        match with_overlay(&overlay) {
            Err(LoadError::Invalid {
                file,
                path,
                message,
            }) => {
                assert_eq!(file.to_str(), Some("overlay.json"));
                assert_eq!(path, expected_path, "{overlay}");
                assert_eq!(message, expected_message, "{overlay}");
            }
            other => panic!("{overlay}: expected an error at {expected_path}, got {other:?}"),
        }
    }
}

#[test]
fn an_activation_input_takes_only_an_item_and_a_rate() {
    let overlay = r#"{ "facility_activation": { "furnance_1": { "item": "liquid_water", "per_min": 6, "per_sec": 1 } } }"#;
    match with_overlay(overlay) {
        Err(LoadError::Json { file, path, .. }) => {
            assert_eq!(file.to_str(), Some("overlay.json"));
            assert_eq!(path, "facility_activation.furnance_1.per_sec");
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
