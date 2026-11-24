use std::io;
use std::path::PathBuf;

use thiserror::Error;

/// Why the extractor stopped. Each names the file and, where it applies, the key in it.
#[derive(Debug, Error)]
pub enum ExtractError {
    #[error("{file}: cannot read: {source}")]
    Read {
        file: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("{file}: {source}")]
    Json {
        file: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    /// A row refers to something its table, or another table, doesn't have, or holds a value the
    /// extractor doesn't understand.
    #[error("{table}: `{key}`: {message}")]
    Table {
        /// The table's file name, such as `FactoryMachineCraftTable.json`.
        table: String,
        key: String,
        message: String,
    },

    #[error("{file}: cannot write: {source}")]
    Write {
        file: PathBuf,
        #[source]
        source: io::Error,
    },

    /// The data file the extractor wrote doesn't load. A bug in the extractor, or tables it
    /// misreads.
    #[error("the extracted data does not load: {0}")]
    Invalid(#[source] aic_data::LoadError),

    /// The `--previous` data version doesn't load.
    #[error("previous data version: {0}")]
    Previous(#[source] aic_data::LoadError),

    #[error("{0}")]
    Usage(String),
}

impl ExtractError {
    pub(crate) fn table(table: &str, key: &str, message: impl Into<String>) -> Self {
        Self::Table {
            table: format!("{table}.json"),
            key: key.to_owned(),
            message: message.into(),
        }
    }
}
