//! Checks against the real `data/v1.5.3.json` in the repository, with the app's English names
//! (`crates/aic-editor/i18n/names/en.json`). Ignored by default so the other tests don't depend on
//! game data; run with `cargo test -p aic-data -- --ignored`.

use std::path::Path;

use aic_data::{ItemId, Names, load};

#[test]
#[ignore = "reads the real data/v1.5.3.json"]
fn real_v1_5_3_loads_and_is_consistent() {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/v1.5.3.json");
    let names = Names {
        file: "names/en.json",
        json: include_bytes!("../../aic-editor/i18n/names/en.json"),
    };
    let catalog = load(&file, names).expect("real data loads");

    assert_eq!(catalog.version(), "v1.5.3");
    assert_eq!(catalog.items().len(), 161);
    assert_eq!(catalog.recipes().len(), 257);
    assert_eq!(catalog.facilities().len(), 34);
    assert_eq!(catalog.belt_per_min(), 30.0);
    assert_eq!(catalog.pipe_per_min(), 120.0);

    let copper = catalog.recipe("copper_nugget").unwrap();
    let outputs: Vec<&ItemId> = copper.outputs.iter().map(|(id, _)| id).collect();
    assert_eq!(
        outputs,
        [
            &ItemId::from("copper_nugget"),
            &ItemId::from("liquid_sewage")
        ]
    );

    let battery = catalog.recipe("power_proc_battery_1").unwrap();
    assert_eq!(battery.power_override_kw, Some(-220.0));

    // The loader refuses dangling IDs, but check here too so a loosened loader can't slip through.
    for recipe in catalog.recipes().values() {
        for facility in &recipe.facilities {
            assert!(
                catalog.facility(facility.as_str()).is_some(),
                "{}: {facility}",
                recipe.id
            );
        }
        for (item, _) in recipe.inputs.iter().chain(&recipe.outputs) {
            assert!(
                catalog.item(item.as_str()).is_some(),
                "{}: {item}",
                recipe.id
            );
        }
    }

    // The English names name every item and recipe.
    assert!(catalog.warnings().is_empty(), "{:?}", catalog.warnings());
}

/// `data/v1.5.3_overlay.json` declares the transmuting units' activation inputs, which the data
/// folds into every one of their recipes.
#[test]
#[ignore = "reads the real data/v1.5.3.json"]
fn real_v1_5_3_transmuting_units_have_activation_inputs() {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/v1.5.3.json");
    let names = Names {
        file: "names/en.json",
        json: include_bytes!("../../aic-editor/i18n/names/en.json"),
    };
    let catalog = load(&file, names).expect("real data loads");

    let activation = |id: &str| {
        let facility = catalog.facility(id).expect("facility exists");
        let activation = facility
            .activation
            .as_ref()
            .expect("has an activation input");
        (activation.item().as_str(), activation.per_min())
    };
    // Solid-Gas Transmuting Unit: 6 Xiragen per minute.
    assert_eq!(activation("phase_trans_2"), ("gas_xiranite", 6.0));
    // Fluid-Gas Transmuting Unit: 6 Liquid Xiranite per minute.
    assert_eq!(activation("phase_trans_1"), ("liquid_xiranite", 6.0));
    let others: Vec<&str> = (catalog.facilities().values())
        .filter(|facility| facility.activation.is_some())
        .map(|facility| facility.id.as_str())
        .filter(|id| !["phase_trans_1", "phase_trans_2"].contains(id))
        .collect();
    assert!(others.is_empty(), "{others:?}");
}
