//! The game data built into the program by `build.rs`, from the repository's `data/` folder.

mod table {
    include!(concat!(env!("OUT_DIR"), "/embedded_data.rs"));
}

/// The built-in data versions; empty if `data/` was missing at build time.
#[cfg(not(target_arch = "wasm32"))]
pub fn bundle() -> std::sync::Arc<aic_data::Bundle> {
    use std::sync::{Arc, OnceLock};

    static BUNDLE: OnceLock<Arc<aic_data::Bundle>> = OnceLock::new();
    BUNDLE
        .get_or_init(|| Arc::new(aic_data::Bundle::new(table::FILES)))
        .clone()
}

/// The paths of every data file, `/`-separated under `data/`. The web build fetches them from its
/// site instead of embedding the bytes.
#[cfg(target_arch = "wasm32")]
pub fn paths() -> &'static [&'static str] {
    table::FILES
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    #[test]
    fn the_built_in_bundle_lists_the_repository_versions() {
        let data = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        if !data.join("v1.5.3").is_dir() {
            return;
        }
        let versions = super::bundle().versions();
        assert!(versions.iter().any(|v| v.name == "v1.5.3"));
    }
}
