//! The game data and names built into the program by `build.rs`: the repository's `data/` folder
//! and `i18n/names/`.

use std::sync::{Arc, OnceLock};

use aic_data::{Bundle, Names};

mod table {
    include!(concat!(env!("OUT_DIR"), "/embedded.rs"));
}

/// The built-in data versions; empty if `data/` was missing at build time.
pub fn bundle() -> Arc<Bundle> {
    static BUNDLE: OnceLock<Arc<Bundle>> = OnceLock::new();
    BUNDLE
        .get_or_init(|| Arc::new(Bundle::new(table::DATA)))
        .clone()
}

/// What errors call a built-in names table.
const NAMES_ORIGIN: &str = "<built-in names>";

/// The built-in game names in `language` (`en`), if the program has them.
pub fn names(language: &str) -> Option<Names<'static>> {
    table::NAMES
        .iter()
        .find(|(lang, _)| *lang == language)
        .map(|&(_, json)| Names {
            file: NAMES_ORIGIN,
            json,
        })
}

/// The languages of the built-in names, sorted.
pub fn languages() -> impl Iterator<Item = &'static str> {
    table::NAMES.iter().map(|&(lang, _)| lang)
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_built_in_bundle_lists_the_repository_versions() {
        let data = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        if !data.join("v1.5.3.json").is_file() {
            return;
        }
        let versions = super::bundle().versions();
        assert!(versions.iter().any(|v| v.name == "v1.5.3"));
    }

    /// v1.5.3 stays built in beside the newer versions, with the transmuting units' activation
    /// inputs that its file now holds (ADR 0014).
    #[test]
    fn the_built_in_v1_5_3_has_its_activation_inputs() {
        let data = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        if !data.join("v1.5.3.json").is_file() {
            return;
        }
        let names = super::names("en").expect("English names are built in");
        let catalog = super::bundle().load("v1.5.3", names).expect("v1.5.3 loads");
        let unit = catalog.facility("phase_trans_2").expect("facility exists");
        let activation = unit.activation().expect("has an activation input");
        assert_eq!(activation.item().as_str(), "gas_xiranite");
    }

    /// The newest built-in version is the game's v1.7.0, extracted from its tables.
    #[test]
    fn the_newest_built_in_version_is_v1_7_0() {
        let data = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        if !data.join("v1.7.0.json").is_file() {
            return;
        }
        assert_eq!(super::bundle().versions()[0].name, "v1.7.0");
    }

    /// The built-in v1.7.0 loads with the built-in names, its units of whole units and arrays
    /// as ADR 0015 has them.
    #[test]
    fn the_built_in_v1_7_0_has_its_arrays_and_whole_units() {
        let data = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        if !data.join("v1.7.0.json").is_file() {
            return;
        }
        let names = super::names("en").expect("English names are built in");
        let catalog = super::bundle().load("v1.7.0", names).expect("v1.7.0 loads");
        assert!(catalog.warnings().is_empty(), "{:?}", catalog.warnings());
        for unit in ["vaporizer_1", "magic_spawner_1", "magic_spawner_2"] {
            let facility = catalog.facility(unit).expect("facility exists");
            assert!(facility.whole_units(), "{unit}");
        }
        assert_eq!(
            catalog.facility("magic_ming_1").expect("an array").speed,
            0.2
        );
    }

    #[test]
    fn english_names_are_built_in() {
        assert!(super::languages().any(|lang| lang == "en"));
        assert!(super::names("en").is_some());
        assert!(super::names("xx").is_none());
    }
}
