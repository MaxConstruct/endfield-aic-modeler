//! The data file format: `data/<version>.json` and its overlay, `data/<version>_overlay.json`.
//!
//! This module is the format's only definition (ADR 0014). The loader reads these structs and
//! turns them into a checked [`crate::Catalog`]; `aic-extract` writes them from the game's tables.
//! The format's reference is `data/README.md`.
//!
//! A file holds IDs and numbers, never names: those come from the caller's names table
//! ([`crate::Names`]). Unknown keys are errors everywhere, so a typo doesn't silently do nothing.
//! All rates are items per minute; a recipe's quantities are per craft.

use std::path::Path;

use indexmap::IndexMap;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::catalog::Phase;
use crate::error::LoadError;

/// The format this build reads and writes.
pub const FORMAT: u32 = 1;

/// A data version's file, `data/<version>.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataFile {
    /// Always [`FORMAT`]; any other number is a load error.
    pub format: u32,
    /// The game version the data matches. The file's name wins over it when the two disagree.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Where the data came from. Free text; the loader ignores it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Belt capacity, items per minute.
    pub belt_per_min: f64,
    /// Pipe capacity, items per minute.
    pub pipe_per_min: f64,
    #[serde(default)]
    pub regions: Vec<String>,
    /// The recipe browser's categories, in the order it lists them.
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub items: Vec<ItemEntry>,
    #[serde(default)]
    pub facilities: Vec<FacilityEntry>,
    #[serde(default)]
    pub recipes: Vec<RecipeEntry>,
    #[serde(default, skip_serializing_if = "Aliases::is_empty")]
    pub aliases: Aliases,
}

/// An item: anything that moves on a belt or through a pipe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemEntry {
    pub id: String,
    #[serde(default, skip_serializing_if = "Phase::is_solid")]
    pub phase: Phase,
    /// `#rrggbb`, for the item's port dots and its initials when it has no icon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// A facility: a building type that performs recipes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FacilityEntry {
    pub id: String,
    /// Craft speed multiplier.
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub speed: f64,
    /// Power draw in kW while running.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub power_kw: f64,
    /// Regions it can be built in. Empty means every region.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub regions: Vec<String>,
    /// What each running facility needs whatever its throughput (ADR 0013). It comes on top of
    /// the inputs its recipes list here; the loader adds it to them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activation: Option<ActivationEntry>,
    /// The facility runs in whole units (ADR 0015): every input of its recipes is an activation
    /// input of which nothing scales, as listed in the recipe. Not with `activation`.
    #[serde(default, skip_serializing_if = "is_false")]
    pub whole_units: bool,
    /// For an array: the T1 arrays (projections) it is stacked from, by facility ID, and how many
    /// of each (ADR 0015). A T1 array is one of itself.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub stack: IndexMap<String, u32>,
}

/// `{ "item": "gas_xiranite", "per_min": 6 }`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationEntry {
    pub item: String,
    pub per_min: f64,
}

/// A recipe. Quantities are per craft; the maps keep the file's order, which puts the main product
/// first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeEntry {
    pub id: String,
    pub category: String,
    /// Seconds per craft at speed 1.
    pub time_s: f64,
    /// Without the facility's activation input, which the loader adds.
    #[serde(rename = "in", default)]
    pub inputs: IndexMap<String, f64>,
    #[serde(rename = "out", default)]
    pub outputs: IndexMap<String, f64>,
    /// Never empty; the first is the default.
    pub facilities: Vec<String>,
    /// Replaces the facility's power in kW. Negative means generation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power_kw: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<RecipeKind>,
    /// Left out of the recipe browser. A trade always is.
    #[serde(default, skip_serializing_if = "is_false")]
    pub hidden: bool,
    /// Empty means every region.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub regions: Vec<String>,
    /// The item ID of the gas whose environment the recipe needs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    /// The item ID of the gas whose environment the recipe spreads: a Gas Dispersing Unit's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creates_environment: Option<String>,
    /// What one facility running the recipe projects: T1 arrays by facility ID, and how many
    /// (ADR 0015). A Forge of Luminance's.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub projects: IndexMap<String, u32>,
}

/// What sets a recipe apart from an ordinary craft.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecipeKind {
    /// Extracts a raw resource.
    Mining,
    /// A settlement trade: one item in, coupons out. Placed as an outpost order.
    Trade,
}

/// IDs an older data version used, mapped to this version's, so graphs saved on it still open.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Aliases {
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub items: IndexMap<String, String>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub recipes: IndexMap<String, String>,
}

impl Aliases {
    pub fn is_empty(&self) -> bool {
        self.items.is_empty() && self.recipes.is_empty()
    }
}

/// `<version>_overlay.json`: our own changes on top of the version's file, in the same entries.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Overlay {
    #[serde(default)]
    pub items: OverlayList<ItemEntry>,
    #[serde(default)]
    pub facilities: OverlayList<FacilityEntry>,
    #[serde(default)]
    pub recipes: OverlayList<RecipeEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OverlayList<T> {
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

impl<T> Default for OverlayList<T> {
    fn default() -> Self {
        Self {
            add: Vec::new(),
            replace: Vec::new(),
            remove: Vec::new(),
        }
    }
}

/// An item, facility or recipe, found by its ID.
pub(crate) trait Entry {
    fn id(&self) -> &str;
}

impl Entry for ItemEntry {
    fn id(&self) -> &str {
        &self.id
    }
}

impl Entry for FacilityEntry {
    fn id(&self) -> &str {
        &self.id
    }
}

impl Entry for RecipeEntry {
    fn id(&self) -> &str {
        &self.id
    }
}

/// Only the format number, read before the rest so a file of another format says so rather than
/// failing on the first key it doesn't know.
#[derive(Deserialize)]
struct FormatOnly {
    format: Option<serde_json::Value>,
}

impl DataFile {
    /// Reads a data file; errors call it `file`. A file of another format is an error that names
    /// its number.
    pub fn from_json(bytes: &[u8], file: &Path) -> Result<Self, LoadError> {
        let probe: FormatOnly = from_json(bytes, file)?;
        let invalid = |message: String| LoadError::Invalid {
            file: file.to_path_buf(),
            path: "format".to_owned(),
            message,
        };
        match probe.format {
            None => {
                return Err(invalid(format!(
                    "missing; this build reads format {FORMAT}"
                )));
            }
            Some(serde_json::Value::Number(n)) if n.as_u64() == Some(u64::from(FORMAT)) => {}
            Some(other) => {
                return Err(invalid(format!(
                    "format {other} is not one this build reads (format {FORMAT})"
                )));
            }
        }
        from_json(bytes, file)
    }

    /// The file as pretty-printed JSON, ending in a newline.
    pub fn to_json(&self) -> String {
        let mut json =
            serde_json::to_string_pretty(self).expect("a data file has only string keys");
        json.push('\n');
        json
    }
}

impl Overlay {
    /// Reads an overlay; errors call it `file`.
    pub fn from_json(bytes: &[u8], file: &Path) -> Result<Self, LoadError> {
        from_json(bytes, file)
    }
}

/// Deserializes JSON, naming the path inside it on an error.
pub(crate) fn from_json<T: DeserializeOwned>(bytes: &[u8], file: &Path) -> Result<T, LoadError> {
    let mut de = serde_json::Deserializer::from_slice(bytes);
    let json_error = |path: String, source| LoadError::Json {
        file: file.to_path_buf(),
        path,
        source,
    };
    let value = serde_path_to_error::deserialize(&mut de)
        .map_err(|e| json_error(e.path().to_string(), e.into_inner()))?;
    de.end().map_err(|e| json_error(".".to_owned(), e))?;
    Ok(value)
}

fn one() -> f64 {
    1.0
}

fn is_one(value: &f64) -> bool {
    *value == 1.0
}

fn is_zero(value: &f64) -> bool {
    *value == 0.0
}

fn is_false(value: &bool) -> bool {
    !*value
}
