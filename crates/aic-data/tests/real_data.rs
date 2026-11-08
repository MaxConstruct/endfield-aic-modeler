//! Checks against the real `data/v1.5.3/` in the repository. Ignored by default so the other tests
//! don't depend on game data; run with `cargo test -p aic-data -- --ignored`.

use std::path::Path;

use aic_data::{ItemId, load};

#[test]
#[ignore = "reads the real data/v1.5.3/ folder"]
fn real_v1_5_3_loads_and_is_consistent() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/v1.5.3");
    let catalog = load(&dir, "en").expect("real data loads");

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

    // en.json names every item and recipe.
    assert!(catalog.warnings().is_empty(), "{:?}", catalog.warnings());
}
