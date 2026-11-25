//! Game data for AIC Modeler: parses a data version into a normalized [`Catalog`].
//!
//! A data version is one file, `data/<version>.json`, with an optional overlay beside it,
//! `data/<version>_overlay.json`. The game names are the caller's: every load takes a [`Names`]
//! table (ADR 0012).
//!
//! [`Catalog::from_bytes`] works on byte slices, so any platform can feed it. On native builds,
//! [`find_data_dir`], [`list_versions`] and [`load`] find and read a `data/` folder. A [`Bundle`]
//! reads the same files from memory, for data built into the program.
//!
//! All rates are items per minute. The data file format is defined in [`file`] and nowhere else
//! (ADR 0014); `aic-extract` writes it through the same types.

mod bundle;
mod catalog;
#[cfg(not(target_arch = "wasm32"))]
mod discover;
mod error;
pub mod file;
mod ids;
mod parse;
mod version;

pub use bundle::Bundle;
pub use catalog::{
    ActivationInput, Catalog, Category, Facility, FacilityInputs, ImageKind, InputRate, Item,
    LoadWarning, NameKind, Phase, Recipe, Region,
};
#[cfg(not(target_arch = "wasm32"))]
pub use discover::{DATA_DIR_ENV, DataDirSearch, find_data_dir, list_versions, load};
pub use error::LoadError;
pub use ids::{CategoryId, FacilityId, ItemId, RecipeId, RegionId};
pub use version::{DataVersion, Names};
