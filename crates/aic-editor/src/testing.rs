//! Fixtures for tests, in this crate and in the app (feature `testing`).

use std::path::{Path, PathBuf};

use aic_data::{Catalog, LoadError, Names};

use crate::data::{DataChoice, DataFolder};

/// The small Satisfactory data set the tests use: `fixtures/satisfactory/data/vtest.json`.
pub const FIXTURE_DATA: &[u8] = include_bytes!("../fixtures/satisfactory/data/vtest.json");

/// Its English names, `fixtures/satisfactory/names/en.json`: a names table of its own, since its
/// items are not the game's.
pub const FIXTURE_NAMES: Names<'static> = Names {
    file: "fixtures/satisfactory/names/en.json",
    json: include_bytes!("../fixtures/satisfactory/names/en.json"),
};

/// The small Satisfactory data set the tests use.
pub fn fixture_catalog() -> Catalog {
    Catalog::from_bytes(FIXTURE_DATA, FIXTURE_NAMES).expect("the fixture loads")
}

/// The fixture's version file on disk, for tests that load it through [`load`].
pub fn fixture_file() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/satisfactory/data/vtest.json")
}

/// Loads a version file as `aic_data::load` does, with the English names that sit beside its data
/// folder (`<data>/../names/en.json`), as the fixtures keep them, or else the program's own.
#[cfg(not(target_arch = "wasm32"))]
pub fn load(version_file: &Path) -> Result<Catalog, LoadError> {
    let beside = version_file
        .parent()
        .and_then(Path::parent)
        .map(|root| root.join("names/en.json"));
    match beside.and_then(|file| std::fs::read(file).ok()) {
        Some(json) => aic_data::load(
            version_file,
            Names {
                file: "names/en.json",
                json: &json,
            },
        ),
        None => {
            let names = crate::embedded::names(crate::data::DEFAULT_LANGUAGE)
                .expect("English names are built in");
            aic_data::load(version_file, names)
        }
    }
}

/// The built-in game data, newest version in English, or `None` if `data/` was missing at build
/// time.
pub fn real_catalog() -> Option<Catalog> {
    DataFolder::built_in()?.load(&DataChoice::default()).ok()
}
