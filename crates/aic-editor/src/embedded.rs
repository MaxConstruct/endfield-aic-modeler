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

    /// The overlay beside a version is built in with it, on the desktop and the web alike.
    #[test]
    fn the_built_in_v1_5_3_has_its_overlay() {
        let data = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        if !data.join("v1.5.3_overlay.json").is_file() {
            return;
        }
        let names = super::names("en").expect("English names are built in");
        let catalog = super::bundle().load("v1.5.3", names).expect("v1.5.3 loads");
        let unit = catalog.facility("phase_trans_2").expect("facility exists");
        let activation = unit.activation.as_ref().expect("has an activation input");
        assert_eq!(activation.item().as_str(), "gas_xiranite");
    }

    #[test]
    fn english_names_are_built_in() {
        assert!(super::languages().any(|lang| lang == "en"));
        assert!(super::names("en").is_some());
        assert!(super::names("xx").is_none());
    }
}
