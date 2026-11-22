//! Serde structs that mirror the factoriolab data format exactly.
//!
//! This is the only module that knows that format and its names (`machine`, `locations`, `producers`,
//! `usage`). `parse` turns these into the normalized `Catalog`. Keys we don't use (`row`, `stack`,
//! `cost`, `size`, …) are ignored.

use std::collections::HashMap;

use indexmap::IndexMap;
use serde::Deserialize;

/// A data version's file, `data/<version>.json`.
#[derive(Debug, Deserialize)]
pub(crate) struct RawData {
    /// `{ "arknights-endfield": "v1.5.3" }`
    #[serde(default)]
    pub version: IndexMap<String, String>,
    #[serde(default)]
    pub icons: Vec<RawIcon>,
    #[serde(default)]
    pub categories: Vec<RawNamed>,
    /// Regions, in our terms.
    #[serde(default)]
    pub locations: Vec<RawNamed>,
    #[serde(default)]
    pub defaults: RawDefaults,
    pub items: Vec<RawItem>,
    pub recipes: Vec<RawRecipe>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawNamed {
    pub id: String,
    /// Chinese name.
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawIcon {
    pub id: String,
    /// CSS background offset into the sprite sheet, e.g. `"-64px 0px"`.
    pub position: String,
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RawDefaults {
    #[serde(default)]
    pub max_belt: Option<String>,
    #[serde(default)]
    pub max_pipe: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawItem {
    pub id: String,
    /// Chinese name.
    #[serde(default)]
    pub name: Option<String>,
    pub category: String,
    #[serde(default)]
    pub icon: Option<String>,
    /// Present when the item is a facility.
    #[serde(default)]
    pub machine: Option<RawMachine>,
    #[serde(default)]
    pub belt: Option<RawSpeed>,
    #[serde(default)]
    pub pipe: Option<RawSpeed>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawMachine {
    pub speed: f64,
    /// Power draw in kW. Missing for burner facilities.
    #[serde(default)]
    pub usage: Option<f64>,
    /// Regions the facility can be built in. Empty means every region.
    #[serde(default)]
    pub locations: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawSpeed {
    /// Items per second.
    pub speed: f64,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawRecipe {
    pub id: String,
    /// Chinese name.
    #[serde(default)]
    pub name: Option<String>,
    pub category: String,
    #[serde(default)]
    pub icon: Option<String>,
    /// Seconds per craft.
    pub time: f64,
    /// Quantity per craft. `IndexMap` keeps the file's order, which puts the main product first.
    #[serde(rename = "in", default)]
    pub inputs: IndexMap<String, f64>,
    #[serde(rename = "out", default)]
    pub outputs: IndexMap<String, f64>,
    /// Facilities that can run the recipe. The first is the default.
    #[serde(default)]
    pub producers: Vec<String>,
    /// `mining`, `hideProducer`.
    #[serde(default)]
    pub flags: Vec<String>,
    /// Overrides the facility's power in kW. Negative means generation.
    #[serde(default)]
    pub usage: Option<f64>,
    /// Empty means every region.
    #[serde(default)]
    pub locations: Vec<String>,
}

/// `<version>_overlay.json`: our own patches on top of the version's data, with items and recipes
/// written in the same format. Unknown keys are an error, so a typo doesn't silently do nothing.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawOverlay {
    #[serde(default)]
    pub items: RawOverlayList<RawItem>,
    #[serde(default)]
    pub recipes: RawOverlayList<RawRecipe>,
    /// Facility ID to its new speed.
    #[serde(default)]
    pub facility_speeds: IndexMap<String, f64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawOverlayList<T> {
    /// New entries; their IDs must not exist yet.
    #[serde(default = "Vec::new")]
    pub add: Vec<T>,
    /// Entries that take the place of the one with the same ID, which must exist.
    #[serde(default = "Vec::new")]
    pub replace: Vec<T>,
    /// IDs to drop.
    #[serde(default)]
    pub remove: Vec<String>,
}

impl<T> Default for RawOverlayList<T> {
    fn default() -> Self {
        Self {
            add: Vec::new(),
            replace: Vec::new(),
            remove: Vec::new(),
        }
    }
}

/// An item or recipe, found by its ID.
pub(crate) trait RawEntry {
    fn id(&self) -> &str;
}

impl RawEntry for RawItem {
    fn id(&self) -> &str {
        &self.id
    }
}

impl RawEntry for RawRecipe {
    fn id(&self) -> &str {
        &self.id
    }
}

/// A names table, `crates/aic-editor/i18n/names/<lang>.json`: display names keyed by the data's
/// IDs, the same for every data version.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct RawI18n {
    #[serde(default)]
    pub categories: HashMap<String, String>,
    #[serde(default)]
    pub items: HashMap<String, String>,
    #[serde(default)]
    pub recipes: HashMap<String, String>,
    #[serde(default)]
    pub locations: HashMap<String, String>,
}
