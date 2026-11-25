//! Checks against the real data versions in the repository (`data/v1.5.3.json` to `v1.7.0.json`),
//! with the app's English names (`crates/aic-editor/i18n/names/en.json`). Ignored by default so
//! the other tests don't depend on game data; run with `cargo test -p aic-data -- --ignored`.

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
        let activation = facility.activation().expect("has an activation input");
        (activation.item().as_str(), activation.per_min())
    };
    // Solid-Gas Transmuting Unit: 6 Xiragen per minute.
    assert_eq!(activation("phase_trans_2"), ("gas_xiranite", 6.0));
    // Fluid-Gas Transmuting Unit: 6 Liquid Xiranite per minute.
    assert_eq!(activation("phase_trans_1"), ("liquid_xiranite", 6.0));
    let others: Vec<&str> = (catalog.facilities().values())
        .filter(|facility| facility.activation().is_some())
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
            let a = f.activation()?;
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

    // Marked by hand (ADR 0015): the Gas Dispersing Unit runs whole, and each of its recipes
    // spreads its gas's environment.
    let whole: Vec<&str> = (catalog.facilities().values())
        .filter(|f| f.whole_units())
        .map(|f| f.id.as_str())
        .collect();
    assert_eq!(whole, ["vaporizer_1"]);
    for gas in ["gas_water", "gas_acid", "gas_inert", "gas_xiranite"] {
        let recipe = catalog.recipe(&format!("vaporizer_1-{gas}")).unwrap();
        assert_eq!(
            recipe.creates_environment.as_ref().map(|g| g.as_str()),
            Some(gas)
        );
    }
    let creating = (catalog.recipes().values())
        .filter(|r| r.creates_environment.is_some())
        .count();
    assert_eq!(creating, 4);
}

/// What two data versions make of one recipe, per minute on one facility: each input's scaled and
/// activation parts, and each output.
type Rates = (Vec<(String, f64, f64)>, Vec<(String, f64)>);

fn rates(
    recipe: &aic_data::Recipe,
    facility: &aic_data::Facility,
    item: impl Fn(&ItemId) -> String,
) -> Rates {
    let mut inputs: Vec<(String, f64, f64)> = (recipe.input_rates(facility))
        .map(|(i, rate)| (item(i), rate.scaled, rate.activation))
        .collect();
    inputs.sort_by(|a, b| a.0.cmp(&b.0));
    let mut outputs: Vec<(String, f64)> = (recipe.outputs.iter())
        .map(|(i, q)| (item(i), recipe.rate_per_min(*q, facility.speed)))
        .collect();
    outputs.sort_by(|a, b| a.0.cmp(&b.0));
    (inputs, outputs)
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0)
}

/// Checks that every recipe of `old` that `new` still has, under its ID or an alias, runs at the
/// same rates per minute on each of its facilities, activation inputs included, and that those
/// facilities take their inputs alike (an activation input, whole units). A trade is untimed, so
/// only its coupons per item count. Returns how many it compared and the IDs `new` has no recipe
/// for.
fn same_rates(old: &aic_data::Catalog, new: &aic_data::Catalog) -> (usize, Vec<String>) {
    let item = |id: &ItemId| -> String {
        new.item_alias(id.as_str())
            .unwrap_or(id)
            .as_str()
            .to_owned()
    };
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
            let (a_in, a_out) = rates(recipe, was, item);
            let (b_in, b_out) = rates(counterpart, is, |i| i.as_str().to_owned());
            assert_eq!(a_in.len(), b_in.len(), "{id}: {a_in:?} vs {b_in:?}");
            for (x, y) in a_in.iter().zip(&b_in) {
                assert!(
                    x.0 == y.0 && close(x.1, y.1) && close(x.2, y.2),
                    "{id}: {a_in:?} vs {b_in:?}"
                );
            }
            assert_eq!(a_out.len(), b_out.len(), "{id}: {a_out:?} vs {b_out:?}");
            for (x, y) in a_out.iter().zip(&b_out) {
                assert!(
                    x.0 == y.0 && close(x.1, y.1),
                    "{id}: {a_out:?} vs {b_out:?}"
                );
            }
            assert_eq!(was.inputs, is.inputs, "{id} on {facility_id}");
        }
        compared += 1;
    }
    (compared, left_behind)
}

/// Every v1.5.3 recipe that v1.6.0 still has runs the same there. The only recipes left behind are
/// the cross-region transfers, which the app hides, and the old data's own water rig.
#[test]
#[ignore = "reads the real data/v1.5.3.json and data/v1.6.0.json"]
fn real_v1_5_3_recipes_run_the_same_in_v1_6_0() {
    let (compared, left_behind) = same_rates(&real("v1.5.3"), &real("v1.6.0"));
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

/// Every v1.6.0 recipe is in v1.7.0, under the same ID, and runs the same there, the Gas
/// Dispersing Unit in whole units in both.
#[test]
#[ignore = "reads the real data/v1.6.0.json and data/v1.7.0.json"]
fn real_v1_6_0_recipes_run_the_same_in_v1_7_0() {
    let (old, new) = (real("v1.6.0"), real("v1.7.0"));
    let (compared, left_behind) = same_rates(&old, &new);
    assert!(left_behind.is_empty(), "{left_behind:?}");
    assert_eq!(compared, 387);
    assert!(
        old.recipes()
            .keys()
            .all(|id| new.recipe(id.as_str()).is_some())
    );
}

/// `data/v1.7.0.json`, written by `aic-extract` from the game's tables with the arrays (ADR 0015).
#[test]
#[ignore = "reads the real data/v1.7.0.json"]
fn real_v1_7_0_loads_named_with_its_arrays_and_whole_units() {
    let catalog = real("v1.7.0");
    assert_eq!(catalog.version(), "v1.7.0");
    // The extractor added every name the data needs.
    assert!(catalog.warnings().is_empty(), "{:?}", catalog.warnings());
    assert_eq!(catalog.items().len(), 269);
    assert_eq!(catalog.facilities().len(), 45);
    assert_eq!(catalog.recipes().len(), 426);
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/v1.7.0.json");
    let data = aic_data::file::DataFile::from_json(&std::fs::read(&file).unwrap(), &file).unwrap();
    assert_eq!(data.aliases.items.len(), 7);
    assert_eq!(data.aliases.recipes.len(), 155);

    let on = |facility: &str| -> Vec<&str> {
        (catalog.recipes().values())
            .filter(|r| r.facilities.iter().any(|f| f.as_str() == facility))
            .map(|r| r.id.as_str())
            .collect()
    };
    // The game lists the conversions as its crafts too; the extractor takes them off.
    let mut balancer = on("magic_balancer_1");
    balancer.sort_unstable();
    assert_eq!(
        balancer,
        [
            "magic_balancer_1_copper_bright_1",
            "magic_balancer_1_copper_enr",
            "magic_balancer_1_xiranite_enr"
        ]
    );
    assert!(
        (catalog.recipes().keys())
            .all(|id| !id.as_str().starts_with("formation_") || id.as_str().ends_with("_1")),
        "a conversion's higher tier surfaced as a recipe"
    );

    // Each conversion on each tier of its array, per minute: L and T 30 / 60 / 90 / 150, E 60 /
    // 120; as much in as out.
    let tiers_lt = [30.0, 60.0, 90.0, 150.0];
    let tiers_e = [60.0, 120.0];
    for (recipe, kind, from, to, expected) in [
        (
            "formation_magic_ming_item_copper_ming_1",
            "magic_ming",
            "copper_nugget",
            "copper_ming",
            &tiers_lt[..],
        ),
        (
            "formation_magic_ming_item_xiranite_ming_1",
            "magic_ming",
            "xiranite_powder",
            "xiranite_ming",
            &tiers_lt,
        ),
        (
            "formation_magic_hui_item_copper_hui_1",
            "magic_hui",
            "copper_nugget",
            "copper_hui",
            &tiers_lt,
        ),
        (
            "formation_magic_hui_item_xiranite_hui_1",
            "magic_hui",
            "xiranite_powder",
            "xiranite_hui",
            &tiers_lt,
        ),
        (
            "formation_magic_heng_item_copper_enr_heng_1",
            "magic_heng",
            "copper_nugget",
            "copper_enr_heng",
            &tiers_e,
        ),
        (
            "formation_magic_heng_item_xiranite_enr_heng_1",
            "magic_heng",
            "xiranite_powder",
            "xiranite_enr_heng",
            &tiers_e,
        ),
    ] {
        let conversion = catalog.recipe(recipe).expect("the conversion exists");
        let tiers: Vec<String> = (1..=expected.len())
            .map(|t| format!("{kind}_{t}"))
            .collect();
        let facilities: Vec<&str> = conversion.facilities.iter().map(|f| f.as_str()).collect();
        assert_eq!(facilities, tiers, "{recipe}");
        for (tier, &per_min) in tiers.iter().zip(expected) {
            let facility = catalog.facility(tier).unwrap();
            let (inputs, outputs) = rates(conversion, facility, |i| i.as_str().to_owned());
            assert_eq!(inputs.len(), 1, "{recipe} on {tier}");
            assert_eq!(outputs.len(), 1, "{recipe} on {tier}");
            assert_eq!((inputs[0].0.as_str(), outputs[0].0.as_str()), (from, to));
            assert!(
                close(inputs[0].1, per_min) && inputs[0].2 == 0.0 && close(outputs[0].1, per_min),
                "{recipe} on {tier}: {inputs:?} {outputs:?}"
            );
            assert_eq!(facility.power_kw, 0.0, "{tier}");
            assert!(!facility.whole_units(), "{tier}");
        }
    }

    // The units that run whole, and only they.
    let whole: Vec<&str> = (catalog.facilities().values())
        .filter(|f| f.whole_units())
        .map(|f| f.id.as_str())
        .collect();
    assert_eq!(whole, ["vaporizer_1", "magic_spawner_1", "magic_spawner_2"]);
    for id in ["vaporizer_1", "magic_spawner_1", "magic_spawner_2"] {
        let facility = catalog.facility(id).unwrap();
        for recipe in on(id) {
            let recipe = catalog.recipe(recipe).unwrap();
            assert!(recipe.outputs.is_empty(), "{}", recipe.id);
            for (item, rate) in recipe.input_rates(facility) {
                assert!(
                    rate.scaled == 0.0 && close(rate.activation, 6.0),
                    "{}: {item}",
                    recipe.id
                );
            }
        }
    }
    let mut dispersing: Vec<(&str, &str)> = (on("vaporizer_1").into_iter())
        .map(|id| {
            let gas = catalog.recipe(id).unwrap().creates_environment.as_ref();
            (id, gas.expect("creates an environment").as_str())
        })
        .collect();
    dispersing.sort_unstable();
    assert_eq!(
        dispersing,
        [
            ("vaporizer_1-gas_acid", "gas_acid"),
            ("vaporizer_1-gas_inert", "gas_inert"),
            ("vaporizer_1-gas_water", "gas_water"),
            ("vaporizer_1-gas_xiranite", "gas_xiranite")
        ]
    );

    // The forges: two and three recipes, each projecting its choice of T1 arrays.
    let projects = |recipe: &str| -> Vec<(&str, u32)> {
        (catalog.recipe(recipe).unwrap().projects.iter())
            .map(|(f, n)| (f.as_str(), *n))
            .collect()
    };
    assert_eq!(on("magic_spawner_1").len(), 2);
    assert_eq!(on("magic_spawner_2").len(), 3);
    assert_eq!(projects("magic_spawner_1-magic_hui"), [("magic_hui_1", 1)]);
    assert_eq!(
        projects("magic_spawner_2-magic_ming-magic_hui"),
        [("magic_ming_1", 1), ("magic_hui_1", 1)]
    );
    assert_eq!(
        projects("magic_spawner_2-magic_ming-magic_ming"),
        [("magic_ming_1", 2)]
    );
    let stack = |facility: &str| -> Vec<(&str, u32)> {
        (catalog.facility(facility).unwrap().stack.iter())
            .map(|(f, n)| (f.as_str(), *n))
            .collect()
    };
    assert_eq!(stack("magic_ming_3"), [("magic_ming_1", 3)]);
    assert_eq!(
        stack("magic_heng_2"),
        [("magic_ming_1", 2), ("magic_hui_1", 2)]
    );
}
