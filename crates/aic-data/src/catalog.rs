//! The normalized game data the rest of the app uses.

use std::fmt;

use indexmap::IndexMap;
use serde::Serialize;

use crate::ids::{CategoryId, FacilityId, ItemId, RecipeId, RegionId};

/// All game data for one data version.
///
/// Built only by the loaders in this crate, which check that every ID a recipe refers to exists. The
/// fields are private so that check can't be undone afterwards.
#[derive(Debug, Clone, Serialize)]
pub struct Catalog {
    pub(crate) items: IndexMap<ItemId, Item>,
    pub(crate) facilities: IndexMap<FacilityId, Facility>,
    pub(crate) recipes: IndexMap<RecipeId, Recipe>,
    pub(crate) regions: IndexMap<RegionId, Region>,
    pub(crate) categories: IndexMap<CategoryId, Category>,
    pub(crate) belt_per_min: f64,
    pub(crate) pipe_per_min: f64,
    pub(crate) icons: IndexMap<String, Icon>,
    #[serde(skip)]
    pub(crate) icons_webp: Option<Vec<u8>>,
    pub(crate) version: String,
    #[serde(skip)]
    pub(crate) warnings: Vec<LoadWarning>,
}

impl Catalog {
    /// Every item, facilities included, in data file order.
    pub fn items(&self) -> &IndexMap<ItemId, Item> {
        &self.items
    }

    pub fn item(&self, id: &str) -> Option<&Item> {
        self.items.get(id)
    }

    pub fn facilities(&self) -> &IndexMap<FacilityId, Facility> {
        &self.facilities
    }

    pub fn facility(&self, id: &str) -> Option<&Facility> {
        self.facilities.get(id)
    }

    pub fn recipes(&self) -> &IndexMap<RecipeId, Recipe> {
        &self.recipes
    }

    pub fn recipe(&self, id: &str) -> Option<&Recipe> {
        self.recipes.get(id)
    }

    pub fn regions(&self) -> &IndexMap<RegionId, Region> {
        &self.regions
    }

    pub fn region(&self, id: &str) -> Option<&Region> {
        self.regions.get(id)
    }

    pub fn categories(&self) -> &IndexMap<CategoryId, Category> {
        &self.categories
    }

    /// Belt capacity in items per minute.
    pub fn belt_per_min(&self) -> f64 {
        self.belt_per_min
    }

    /// Pipe capacity in items per minute.
    pub fn pipe_per_min(&self) -> f64 {
        self.pipe_per_min
    }

    /// Where an icon sits in `icons.webp`, keyed by the `icon` of an item or recipe.
    pub fn icon(&self, id: &str) -> Option<&Icon> {
        self.icons.get(id)
    }

    /// Raw bytes of `icons.webp`, if the data version has one.
    pub fn icons_webp(&self) -> Option<&[u8]> {
        self.icons_webp.as_deref()
    }

    /// The data version: the folder name when loaded from disk, otherwise what `data.json` declares
    /// (empty if it declares nothing).
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Problems that didn't stop loading, such as a missing English name.
    pub fn warnings(&self) -> &[LoadWarning] {
        &self.warnings
    }

    /// The catalog in our own normalized format, as pretty-printed JSON. Icon bytes are left out.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("a catalog has only string keys and plain values")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Item {
    pub id: ItemId,
    /// Display name in the loaded language, falling back to the Chinese name, then the ID.
    pub name: String,
    pub name_zh: String,
    pub category: CategoryId,
    /// Key into `Catalog::icon`.
    pub icon: String,
    /// Moves through pipes rather than on belts.
    pub is_fluid: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Facility {
    pub id: FacilityId,
    pub name: String,
    /// Craft speed multiplier. Always positive.
    pub speed: f64,
    /// Power draw in kW while running. Zero for burner facilities.
    pub power_kw: f64,
    /// Regions the facility can be built in. Empty means every region.
    pub regions: Vec<RegionId>,
}

impl Facility {
    pub fn available_in(&self, region: &RegionId) -> bool {
        self.regions.is_empty() || self.regions.contains(region)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Recipe {
    pub id: RecipeId,
    pub name: String,
    pub category: CategoryId,
    pub icon: String,
    /// Seconds per craft at speed 1. Always positive.
    pub time_s: f64,
    /// Quantity per craft, in data file order.
    pub inputs: Vec<(ItemId, f64)>,
    /// Quantity per craft, in data file order. The first is the main product; the rest are byproducts.
    pub outputs: Vec<(ItemId, f64)>,
    /// Facilities that can run this recipe. Never empty; the first is the default.
    pub facilities: Vec<FacilityId>,
    /// Replaces the facility's power in kW. Negative means generation.
    pub power_override_kw: Option<f64>,
    /// Extracts a resource, with no inputs.
    pub is_mining: bool,
    /// Left out of the recipe browser: outpost trades and cross-region transfers.
    pub hidden: bool,
    /// Regions the recipe exists in. Empty means every region.
    pub regions: Vec<RegionId>,
}

impl Recipe {
    /// Items per minute for one facility running at `speed`.
    pub fn rate_per_min(&self, qty: f64, speed: f64) -> f64 {
        qty * 60.0 / self.time_s * speed
    }

    pub fn available_in(&self, region: &RegionId) -> bool {
        self.regions.is_empty() || self.regions.contains(region)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Region {
    pub id: RegionId,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Category {
    pub id: CategoryId,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Icon {
    pub rect: IconRect,
    /// Average color as `#rrggbb`, used to tint port dots.
    pub color: Option<String>,
}

/// A pixel rectangle in `icons.webp`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct IconRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// What kind of thing a name belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameKind {
    Item,
    Recipe,
    Region,
    Category,
}

/// A problem found while loading that doesn't stop the catalog from being used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadWarning {
    /// The language file has no name for this ID. The Chinese name, or the ID, is shown instead.
    MissingName { kind: NameKind, id: String },
    /// The folder name and `data.json → version` disagree. The folder name wins.
    VersionMismatch { folder: String, declared: String },
}

impl fmt::Display for LoadWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingName { kind, id } => write!(f, "no name for {kind:?} `{id}`"),
            Self::VersionMismatch { folder, declared } => write!(
                f,
                "folder `{folder}` holds data.json declaring version `{declared}`"
            ),
        }
    }
}
