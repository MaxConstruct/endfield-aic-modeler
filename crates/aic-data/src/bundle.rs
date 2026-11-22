//! Data versions built into the program, so it runs without a `data/` folder beside it.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::catalog::Catalog;
use crate::error::LoadError;
use crate::version::{DataFiles, DataVersion, Names, load_version, sort_newest_first, version_of};

/// Where bundled files say they are in error messages.
const ORIGIN: &str = "<built-in data>";

/// The files of a `data/` folder held in memory, keyed by their names (`v1.5.3.json`). Holds the
/// same versions [`crate::list_versions`] would find in that folder.
#[derive(Debug, Clone, Default)]
pub struct Bundle {
    files: BTreeMap<&'static str, &'static [u8]>,
}

impl Bundle {
    pub fn new(files: &[(&'static str, &'static [u8])]) -> Self {
        Self {
            files: files.iter().copied().collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Every version, newest first. A version's `path` is its file name.
    pub fn versions(&self) -> Vec<DataVersion> {
        let mut versions: Vec<DataVersion> = self
            .files
            .keys()
            .filter_map(|&file| {
                version_of(file).map(|name| DataVersion {
                    name: name.to_owned(),
                    path: PathBuf::from(file),
                })
            })
            .collect();
        sort_newest_first(&mut versions);
        versions
    }

    /// Loads a version, like [`crate::load`] does from disk.
    pub fn load(&self, version: &str, names: Names<'_>) -> Result<Catalog, LoadError> {
        load_version(self, version, names)
    }
}

impl DataFiles for Bundle {
    fn read(&self, name: &str) -> Result<Option<Cow<'_, [u8]>>, LoadError> {
        Ok(self.files.get(name).map(|&bytes| Cow::Borrowed(bytes)))
    }

    fn locate(&self, name: &str) -> PathBuf {
        PathBuf::from(format!("{ORIGIN}/{name}"))
    }
}
