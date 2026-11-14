//! Fixtures for tests, in this crate and in the UIs (feature `testing`).

use std::path::PathBuf;

use aic_data::Catalog;

#[cfg(not(target_arch = "wasm32"))]
use crate::data::{DataChoice, DataFolder};

const DATA: &[u8] = include_bytes!("../fixtures/satisfactory/vtest/data.json");
const EN: &[u8] = include_bytes!("../fixtures/satisfactory/vtest/i18n/en.json");

/// The small Satisfactory data set the tests use.
pub fn fixture_catalog() -> Catalog {
    Catalog::from_bytes(DATA, EN, None).expect("the fixture loads")
}

/// The fixture's version folder on disk, for tests that load it through `aic_data::load`.
pub fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/satisfactory/vtest")
}

/// The built-in game data, newest version in English, or `None` if `data/` was missing at build
/// time.
#[cfg(not(target_arch = "wasm32"))]
pub fn real_catalog() -> Option<Catalog> {
    DataFolder::built_in()?.load(&DataChoice::default()).ok()
}
