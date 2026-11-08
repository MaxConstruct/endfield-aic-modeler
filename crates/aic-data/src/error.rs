use std::io;
use std::path::PathBuf;

use thiserror::Error;

/// Why a catalog couldn't be loaded. Every variant names the file, and where it applies the JSON path
/// inside it, so the player can find and fix the problem.
#[derive(Debug, Error)]
pub enum LoadError {
    #[error("{file}: cannot read: {source}")]
    Read {
        file: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("{file}: at `{path}`: {source}")]
    Json {
        file: PathBuf,
        path: String,
        #[source]
        source: serde_json::Error,
    },

    /// Well-formed JSON whose content we can't use, such as a recipe naming a facility that doesn't exist.
    #[error("{file}: at `{path}`: {message}")]
    Invalid {
        file: PathBuf,
        path: String,
        message: String,
    },

    #[error("no data folder found (tried {})", list(.tried))]
    NoDataDir { tried: Vec<PathBuf> },
}

fn list(paths: &[PathBuf]) -> String {
    if paths.is_empty() {
        return "nothing".to_owned();
    }
    paths
        .iter()
        .map(|p| format!("`{}`", p.display()))
        .collect::<Vec<_>>()
        .join(", ")
}
