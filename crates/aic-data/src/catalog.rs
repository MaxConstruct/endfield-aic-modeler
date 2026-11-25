//! The normalized game data the rest of the app uses.

use std::fmt;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

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
    /// Old item IDs to this version's, from the data file's `aliases`.
    pub(crate) item_aliases: IndexMap<String, ItemId>,
    /// Old recipe IDs to this version's.
    pub(crate) recipe_aliases: IndexMap<String, RecipeId>,
    pub(crate) version: String,
    #[serde(skip)]
    pub(crate) warnings: Vec<LoadWarning>,
}

impl Catalog {
    /// Every item, in data file order. Facilities are not items (ADR 0014).
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
    /// data order. A mining recipe may consume something too (a rig that needs water); only its
    /// outputs are raw resources.
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

    /// The image that stands for a recipe: its first output, else its first input, else its first
    /// facility; the first of those that has an image.
    ///
    /// The catalog holds no images: `has_image` says which the app has (ADR 0012).
    pub fn recipe_image(
        &self,
        id: &str,
        has_image: impl Fn(ImageKind, &str) -> bool,
    ) -> Option<(ImageKind, &str)> {
        let recipe = self.recipe(id)?;
        let items = (recipe.outputs.iter().map(|(item, _)| item.as_str()))
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

    /// The recipe an older data version called `old_id`, if this version's data names one. Only
    /// IDs this version doesn't have are aliases.
    pub fn recipe_alias(&self, old_id: &str) -> Option<&RecipeId> {
        self.recipe_aliases.get(old_id)
    }

    /// The item an older data version called `old_id`, if this version's data names one.
    pub fn item_alias(&self, old_id: &str) -> Option<&ItemId> {
        self.item_aliases.get(old_id)
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
    /// Display name in the names table's language, falling back to the ID.
    pub name: String,
    pub phase: Phase,
    /// `#rrggbb`, used to tint port dots and the initials of an item without an icon.
    pub color: Option<String>,
}

impl Item {
    /// Moves through pipes rather than on belts: a liquid or a gas.
    pub fn is_fluid(&self) -> bool {
        self.phase != Phase::Solid
    }
}

/// Whether an item is carried on belts (solid) or through pipes (liquid or gas).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    #[default]
    Solid,
    Liquid,
    Gas,
}

impl Phase {
    pub fn is_solid(&self) -> bool {
        *self == Self::Solid
    }
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
    /// Which of its inputs scale with the facility count: all, all but an activation input, or
    /// none (whole units). [`Facility::activation`] and [`Facility::whole_units`] read it.
    pub inputs: FacilityInputs,
    /// For an array (ADR 0015): the T1 arrays (projections) it is stacked from, by facility ID,
    /// and how many of each, in data order. A T1 array is one of itself; `magic_heng_2` is two
    /// `magic_ming_1` and two `magic_hui_1`. Empty for any other facility. Every ID is a facility
    /// and every count positive.
    pub stack: Vec<(FacilityId, u32)>,
}

impl Facility {
    pub fn available_in(&self, region: &RegionId) -> bool {
        self.regions.is_empty() || self.regions.contains(region)
    }

    /// What each running facility needs of one input whatever its throughput, if anything. Every
    /// recipe of the facility lists that item among its inputs at this rate or more. Never one
    /// for a facility of [`whole units`](Facility::whole_units).
    pub fn activation(&self) -> Option<&ActivationInput> {
        match &self.inputs {
            FacilityInputs::Activation(activation) => Some(activation),
            FacilityInputs::Scaled | FacilityInputs::WholeUnits => None,
        }
    }

    /// Whether the facility runs in whole units (ADR 0015): every input of its recipes is an
    /// activation input of which nothing scales, so its count is a whole number. The Gas
    /// Dispersing Unit and the Forges of Luminance.
    pub fn whole_units(&self) -> bool {
        self.inputs == FacilityInputs::WholeUnits
    }
}

/// Which of a facility's inputs scale with its count (ADR 0013, ADR 0015). One of the three, so a
/// facility of whole units can't also have an activation input.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum FacilityInputs {
    /// Every input scales with the facility count.
    Scaled,
    /// One input is partly or wholly an activation input; the rest scale.
    Activation(ActivationInput),
    /// Whole units: every input is an activation input of which nothing scales.
    WholeUnits,
}

/// An input a facility needs at a fixed rate for each facility that is running, whatever its
/// throughput: a unit at half load still needs all of it.
///
/// Only the loaders in this crate make one, after checking that the item exists and the rate is
/// positive. The loader adds it to every recipe of the facility, so each takes at least that much.
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
    /// Per facility, times the facility count. Zero on a facility of whole units.
    pub scaled: f64,
    /// Per running facility: the facility count rounded up. Zero for an item that is not the
    /// facility's activation input; the whole rate on a facility of whole units.
    pub activation: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Recipe {
    pub id: RecipeId,
    pub name: String,
    pub category: CategoryId,
    /// Seconds per craft at speed 1. Always positive.
    pub time_s: f64,
    /// Quantity per craft, in data file order: the totals for one facility running flat out. The
    /// facility's activation input, which the data file lists apart, is folded in (to its entry
    /// if the recipe takes the item anyway, else last).
    pub inputs: Vec<(ItemId, f64)>,
    /// Quantity per craft, in data file order. The first is the main product; the rest are byproducts.
    pub outputs: Vec<(ItemId, f64)>,
    /// Facilities that can run this recipe. Never empty; the first is the default.
    pub facilities: Vec<FacilityId>,
    /// Replaces the facility's power in kW. Negative means generation.
    pub power_override_kw: Option<f64>,
    /// Extracts a raw resource. Its outputs are raw resources; it may take an input too, such as
    /// the water a rig needs.
    pub is_mining: bool,
    /// Left out of the recipe browser: outpost trades and cross-region transfers.
    pub hidden: bool,
    /// A settlement trade: one item in, coupons out. Placed as an outpost order, not a recipe node.
    pub is_trade: bool,
    /// Regions the recipe exists in. Empty means every region.
    pub regions: Vec<RegionId>,
    /// The gas whose environment the recipe needs around its facility, if any (ADR 0015): a
    /// condition, not a flow. The recipe runs either way; a graph without a running node that
    /// creates it is warned about.
    pub environment: Option<ItemId>,
    /// The gas whose environment the recipe spreads, if any: a Gas Dispersing Unit's recipe.
    pub creates_environment: Option<ItemId>,
    /// What one facility running the recipe projects (ADR 0015): T1 arrays by facility ID, and
    /// how many, in data order. A Forge of Luminance's recipe; empty for any other. Every ID is a
    /// facility and every count positive.
    pub projects: Vec<(FacilityId, u32)>,
}

impl Recipe {
    /// Items per minute for one facility running at `speed`.
    pub fn rate_per_min(&self, qty: f64, speed: f64) -> f64 {
        qty * 60.0 / self.time_s * speed
    }

    /// Each input on `facility`, in data order, split into the part that scales with the facility
    /// count and the part that doesn't (the facility's activation input). Without an activation
    /// input, every rate scales; on a facility of whole units, none does.
    pub fn input_rates<'a>(
        &'a self,
        facility: &'a Facility,
    ) -> impl Iterator<Item = (&'a ItemId, InputRate)> + 'a {
        self.inputs.iter().map(move |(item, qty)| {
            let rate = self.rate_per_min(*qty, facility.speed);
            let split = match &facility.inputs {
                FacilityInputs::WholeUnits => InputRate {
                    scaled: 0.0,
                    activation: rate,
                },
                FacilityInputs::Activation(activation) if activation.item() == item => {
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
    /// The names table has no name for this ID. The ID is shown instead.
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
