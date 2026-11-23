//! The normalized game data the rest of the app uses.

use std::fmt;

use indexmap::IndexMap;
use serde::Serialize;

use crate::ids::{CategoryId, FacilityId, ItemId, RecipeId, RegionId};

/// Two rates closer than this fraction of the larger are the same rate, apart from rounding.
pub(crate) const SAME_RATE: f64 = 1e-9;

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

    /// Raw resources of `region`: what its mining recipes extract, on a facility the region has, in
    /// data order.
    pub fn raw_resources(&self, region: &RegionId) -> Vec<&ItemId> {
        let mut raw: Vec<&ItemId> = Vec::new();
        let minable = self.recipes.values().filter(|recipe| {
            recipe.is_mining
                && recipe.available_in(region)
                && recipe.facilities.iter().any(|id| {
                    self.facilities
                        .get(id)
                        .is_some_and(|facility| facility.available_in(region))
                })
        });
        for (item, _) in minable.flat_map(|recipe| &recipe.outputs) {
            if !raw.contains(&item) {
                raw.push(item);
            }
        }
        raw
    }

    /// The image that stands for a recipe: its own icon, else its first output, else its first
    /// input, else its first facility; the first of those that has an image. The data's recipe
    /// icons are mostly generated IDs with no image, so this usually lands on the main output.
    ///
    /// The catalog holds no images: `has_image` says which the app has (ADR 0012).
    pub fn recipe_image(
        &self,
        id: &str,
        has_image: impl Fn(ImageKind, &str) -> bool,
    ) -> Option<(ImageKind, &str)> {
        let recipe = self.recipe(id)?;
        let items = std::iter::once(recipe.icon.as_str())
            .chain(recipe.outputs.iter().map(|(item, _)| item.as_str()))
            .chain(recipe.inputs.iter().map(|(item, _)| item.as_str()))
            .map(|id| (ImageKind::Item, id));
        let facility = recipe
            .facilities
            .first()
            .map(|id| (ImageKind::Facility, id.as_str()));
        items
            .chain(facility)
            .find(|&(kind, id)| has_image(kind, id))
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

    /// The data's entry for an icon, keyed by the `icon` of an item or recipe: its colour, and
    /// where factoriolab's sprite sheet had it (which the app doesn't read).
    pub fn icon(&self, id: &str) -> Option<&Icon> {
        self.icons.get(id)
    }

    /// The data version: the file name without `.json` when loaded through [`crate::load`] or a
    /// [`crate::Bundle`], otherwise what the data declares (empty if it declares nothing).
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Problems that didn't stop loading, such as a missing English name.
    pub fn warnings(&self) -> &[LoadWarning] {
        &self.warnings
    }

    /// The catalog in our own normalized format, as pretty-printed JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("a catalog has only string keys and plain values")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Item {
    pub id: ItemId,
    /// Display name in the names table's language, falling back to the Chinese name, then the ID.
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
    /// What each running facility needs whatever its throughput, if anything. Every recipe of the
    /// facility lists that item among its inputs at this rate or more.
    pub activation: Option<ActivationInput>,
}

impl Facility {
    pub fn available_in(&self, region: &RegionId) -> bool {
        self.regions.is_empty() || self.regions.contains(region)
    }
}

/// An input a facility needs at a fixed rate for each facility that is running, whatever its
/// throughput: a unit at half load still needs all of it.
///
/// Only the loaders in this crate make one, after checking that the item exists, the rate is
/// positive and every recipe of the facility takes at least that much of the item.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ActivationInput {
    item: ItemId,
    per_min: f64,
}

impl ActivationInput {
    pub(crate) fn new(item: ItemId, per_min: f64) -> Self {
        Self { item, per_min }
    }

    pub fn item(&self) -> &ItemId {
        &self.item
    }

    /// Items per minute for each running facility. Always positive.
    pub fn per_min(&self) -> f64 {
        self.per_min
    }
}

/// One input of a recipe on a facility, per minute: the part that scales with the facility
/// count, and the part each running facility needs whatever its throughput (its activation input).
/// The two add up to the recipe's rate for one facility running flat out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InputRate {
    /// Per facility, times the facility count.
    pub scaled: f64,
    /// Per running facility: the facility count rounded up. Zero for an item that is not the
    /// facility's activation input.
    pub activation: f64,
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
    /// A settlement trade: one item in, coupons out. Placed as an outpost order, not a recipe node.
    pub is_trade: bool,
    /// Regions the recipe exists in. Empty means every region.
    pub regions: Vec<RegionId>,
}

impl Recipe {
    /// Items per minute for one facility running at `speed`.
    pub fn rate_per_min(&self, qty: f64, speed: f64) -> f64 {
        qty * 60.0 / self.time_s * speed
    }

    /// Each input on `facility`, in data order, split into the part that scales with the facility
    /// count and the part that doesn't (the facility's activation input). Without an activation
    /// input, every rate scales.
    pub fn input_rates<'a>(
        &'a self,
        facility: &'a Facility,
    ) -> impl Iterator<Item = (&'a ItemId, InputRate)> + 'a {
        self.inputs.iter().map(move |(item, qty)| {
            let rate = self.rate_per_min(*qty, facility.speed);
            let split = match &facility.activation {
                Some(activation) if activation.item() == item => {
                    let scaled = rate - activation.per_min();
                    InputRate {
                        // Rounding in `rate` doesn't leave a sliver that scales.
                        scaled: if scaled > SAME_RATE * rate {
                            scaled
                        } else {
                            0.0
                        },
                        activation: activation.per_min(),
                    }
                }
                _ => InputRate {
                    scaled: rate,
                    activation: 0.0,
                },
            };
            (item, split)
        })
    }

    pub fn available_in(&self, region: &RegionId) -> bool {
        self.regions.is_empty() || self.regions.contains(region)
    }
}

/// What an image is of, and so which folder of the app's icons holds it
/// (`crates/aic-app/assets/icons/`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ImageKind {
    /// `items/<item id>.png`
    Item,
    /// `facilities/<facility id>.png`
    Facility,
    /// `nodes/<name>.png`, for node kinds that aren't in the data, such as `source` and `sink`.
    Node,
}

impl ImageKind {
    pub const ALL: [Self; 3] = [Self::Item, Self::Facility, Self::Node];

    /// The folder that holds images of this kind.
    pub fn folder(self) -> &'static str {
        match self {
            Self::Item => "items",
            Self::Facility => "facilities",
            Self::Node => "nodes",
        }
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

/// A pixel rectangle in factoriolab's `icons.webp`, which the app doesn't read.
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
    /// The names table has no name for this ID. The Chinese name, or the ID, is shown instead.
    MissingName { kind: NameKind, id: String },
    /// The version's file name and the `version` it declares disagree. The file name wins.
    VersionMismatch { name: String, declared: String },
}

impl fmt::Display for LoadWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingName { kind, id } => write!(f, "no name for {kind:?} `{id}`"),
            Self::VersionMismatch { name, declared } => {
                write!(f, "`{name}.json` declares version `{declared}`")
            }
        }
    }
}
