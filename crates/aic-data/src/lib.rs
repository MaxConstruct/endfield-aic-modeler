//! Game data for AIC Modeler: parses a data version into a normalized [`Catalog`].
//!
//! [`Catalog::from_bytes`] works on byte slices, so any platform can feed it. On native builds,
//! [`find_data_dir`], [`list_versions`] and [`load`] find and read the `data/<version>/` folders.
//!
//! All rates are items per minute. Only the private `raw` module knows the factoriolab file format.

mod catalog;
#[cfg(not(target_arch = "wasm32"))]
mod discover;
mod error;
mod ids;
mod parse;
mod raw;

pub use catalog::{
    Catalog, Category, Facility, Icon, IconRect, Item, LoadWarning, NameKind, Recipe, Region,
};
#[cfg(not(target_arch = "wasm32"))]
pub use discover::{DATA_DIR_ENV, DataDirSearch, DataVersion, find_data_dir, list_versions, load};
pub use error::LoadError;
pub use ids::{CategoryId, FacilityId, ItemId, RecipeId, RegionId};
