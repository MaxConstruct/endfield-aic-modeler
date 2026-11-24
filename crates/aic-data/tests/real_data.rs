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
    assert_eq!(catalog.items().len(), 125);
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

/// The transmuting units' activation inputs are in the data file itself (ADR 0014), and the
/// loader adds them back to every one of their recipes.
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

fn real(version: &str) -> aic_data::Catalog {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../data/{version}.json"));
    let names = Names {
        file: "names/en.json",
        json: include_bytes!("../../aic-editor/i18n/names/en.json"),
    };
    load(&file, names).expect("real data loads")
}

/// `data/v1.6.0.json`, written by `aic-extract` from the game's tables (ADR 0014).
#[test]
#[ignore = "reads the real data/v1.6.0.json"]
fn real_v1_6_0_loads_named_with_the_activation_inputs_of_the_game() {
    let catalog = real("v1.6.0");
    assert_eq!(catalog.version(), "v1.6.0");
    // The extractor added every name the data needs.
    assert!(catalog.warnings().is_empty(), "{:?}", catalog.warnings());
    assert_eq!(catalog.items().len(), 247);
    assert_eq!(catalog.facilities().len(), 32);
    assert_eq!(catalog.recipes().len(), 387);
    assert_eq!(catalog.belt_per_min(), 30.0);
    assert_eq!(catalog.pipe_per_min(), 120.0);

    let with_activation: Vec<(&str, &str, f64)> = (catalog.facilities().values())
        .filter_map(|f| {
            let a = f.activation.as_ref()?;
            Some((f.id.as_str(), a.item().as_str(), a.per_min()))
        })
        .collect();
    assert_eq!(
        with_activation,
        [
            ("phase_trans_1", "liquid_xiranite", 6.0),
            ("phase_trans_2", "gas_xiranite", 6.0)
        ]
    );

    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/v1.6.0.json");
    let data = aic_data::file::DataFile::from_json(&std::fs::read(&file).unwrap(), &file).unwrap();
    assert_eq!(data.aliases.items.len(), 7);
    assert_eq!(data.aliases.recipes.len(), 155);
}

/// Every v1.5.3 recipe that v1.6.0 still has, under its ID or an alias, runs at the same rates
/// per minute on each of its facilities, activation inputs included. A trade is untimed, so only
/// its coupons per item count. The only recipes left behind are the cross-region transfers, which
/// the app hides, and the old data's own water rig.
#[test]
#[ignore = "reads the real data/v1.5.3.json and data/v1.6.0.json"]
fn real_v1_5_3_recipes_run_the_same_in_v1_6_0() {
    let (old, new) = (real("v1.5.3"), real("v1.6.0"));
    let item = |id: &ItemId| -> String {
        new.item_alias(id.as_str())
            .unwrap_or(id)
            .as_str()
            .to_owned()
    };
    let close = |a: f64, b: f64| (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0);
    let mut left_behind = Vec::new();
    let mut compared = 0;
    for recipe in old.recipes().values() {
        let id = recipe.id.as_str();
        let Some(counterpart) = new
            .recipe(id)
            .or_else(|| new.recipe_alias(id).and_then(|n| new.recipe(n.as_str())))
        else {
            left_behind.push(id.to_owned());
            continue;
        };
        if recipe.is_trade {
            assert!(counterpart.is_trade, "{id}");
            let ratio = |r: &aic_data::Recipe| r.outputs[0].1 / r.inputs[0].1;
            assert_eq!(
                item(&recipe.inputs[0].0),
                counterpart.inputs[0].0.as_str(),
                "{id}"
            );
            assert_eq!(
                item(&recipe.outputs[0].0),
                counterpart.outputs[0].0.as_str(),
                "{id}"
            );
            assert!(close(ratio(recipe), ratio(counterpart)), "{id}");
            compared += 1;
            continue;
        }
        for facility_id in &recipe.facilities {
            assert!(
                counterpart.facilities.contains(facility_id),
                "{id} on {facility_id}"
            );
            let (was, is) = (
                old.facility(facility_id.as_str()).unwrap(),
                new.facility(facility_id.as_str()).unwrap(),
            );
            let inputs = |r: &aic_data::Recipe, f: &aic_data::Facility, rename: bool| {
                let mut rates: Vec<(String, f64, f64)> = (r.input_rates(f))
                    .map(|(i, rate)| {
                        let i = if rename {
                            item(i)
                        } else {
                            i.as_str().to_owned()
                        };
                        (i, rate.scaled, rate.activation)
                    })
                    .collect();
                rates.sort_by(|a, b| a.0.cmp(&b.0));
                rates
            };
            let outputs = |r: &aic_data::Recipe, f: &aic_data::Facility, rename: bool| {
                let mut rates: Vec<(String, f64)> = (r.outputs.iter())
                    .map(|(i, q)| {
                        let i = if rename {
                            item(i)
                        } else {
                            i.as_str().to_owned()
                        };
                        (i, r.rate_per_min(*q, f.speed))
                    })
                    .collect();
                rates.sort_by(|a, b| a.0.cmp(&b.0));
                rates
            };
            let (a, b) = (inputs(recipe, was, true), inputs(counterpart, is, false));
            assert_eq!(a.len(), b.len(), "{id}: {a:?} vs {b:?}");
            for (x, y) in a.iter().zip(&b) {
                assert!(
                    x.0 == y.0 && close(x.1, y.1) && close(x.2, y.2),
                    "{id}: {a:?} vs {b:?}"
                );
            }
            let (a, b) = (outputs(recipe, was, true), outputs(counterpart, is, false));
            assert_eq!(a.len(), b.len(), "{id}: {a:?} vs {b:?}");
            for (x, y) in a.iter().zip(&b) {
                assert!(x.0 == y.0 && close(x.1, y.1), "{id}: {a:?} vs {b:?}");
            }
            assert_eq!(was.activation, is.activation, "{id} on {facility_id}");
        }
        compared += 1;
    }
    assert_eq!(compared, 197);
    let transfers = (left_behind.iter())
        .filter(|id| id.starts_with("transfer_"))
        .count();
    assert_eq!(transfers, 59);
    let rest: Vec<&String> = (left_behind.iter())
        .filter(|id| !id.starts_with("transfer_"))
        .collect();
    assert_eq!(rest, ["__miner_water"]);
}
