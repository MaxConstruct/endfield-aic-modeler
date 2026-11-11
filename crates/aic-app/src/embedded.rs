//! The game data built into the program by `build.rs`, from the repository's `data/` folder.

use std::sync::{Arc, OnceLock};

use aic_data::Bundle;

mod table {
    include!(concat!(env!("OUT_DIR"), "/embedded_data.rs"));
}

/// The built-in data versions; empty if `data/` was missing at build time.
pub fn bundle() -> Arc<Bundle> {
    static BUNDLE: OnceLock<Arc<Bundle>> = OnceLock::new();
    BUNDLE
        .get_or_init(|| Arc::new(Bundle::new(table::FILES)))
        .clone()
}
