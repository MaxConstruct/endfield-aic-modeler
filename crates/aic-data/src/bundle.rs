//! Data versions built into the program, so it runs without a `data/` folder beside it.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::ops::Bound;
use std::path::PathBuf;

use crate::catalog::Catalog;
use crate::error::LoadError;
use crate::version::{
    DATA_FILE, DataVersion, VersionFiles, languages, load_version, sort_newest_first,
};

/// Where bundled files say they are in error messages.
const ORIGIN: &str = "<built-in data>";

/// The files of a `data/` folder held in memory, keyed by their path under `data/` with `/`
/// separators (`v1.5.3/data.json`). Holds the same versions [`crate::list_versions`] would find.
///
/// The files are either built into the program ([`Bundle::new`]) or read at run time, as the web
/// build fetches them ([`Bundle::from_owned`]).
#[derive(Debug, Clone, Default)]
pub struct Bundle {
    files: BTreeMap<Cow<'static, str>, Cow<'static, [u8]>>,
}

impl Bundle {
    pub fn new(files: &[(&'static str, &'static [u8])]) -> Self {
        Self {
            files: files
                .iter()
                .map(|&(path, bytes)| (Cow::Borrowed(path), Cow::Borrowed(bytes)))
                .collect(),
        }
    }

    /// A bundle that owns its files, keyed as in [`Bundle::new`].
    pub fn from_owned(files: impl IntoIterator<Item = (String, Vec<u8>)>) -> Self {
        Self {
            files: files
                .into_iter()
                .map(|(path, bytes)| (Cow::Owned(path), Cow::Owned(bytes)))
                .collect(),
        }
    }

    /// Every file and its path, in path order.
    pub fn files(&self) -> impl Iterator<Item = (&str, &[u8])> {
        self.files
            .iter()
            .map(|(path, bytes)| (path.as_ref(), bytes.as_ref()))
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Every top-level folder that holds a `data.json`, newest first. A version's `path` is its
    /// folder name.
    pub fn versions(&self) -> Vec<DataVersion> {
        let mut versions: Vec<DataVersion> = self
            .files
            .keys()
            .filter_map(|path| path.strip_suffix(DATA_FILE)?.strip_suffix('/'))
            .filter(|name| !name.contains('/'))
            .map(|name| DataVersion {
                name: name.to_owned(),
                path: PathBuf::from(name),
            })
            .collect();
        sort_newest_first(&mut versions);
        versions
    }

    /// The languages a version has names for, sorted.
    pub fn languages(&self, version: &str) -> Result<Vec<String>, LoadError> {
        languages(&self.version(version))
    }

    /// Loads a version, like [`crate::load`] does from disk.
    pub fn load(&self, version: &str, lang: &str) -> Result<Catalog, LoadError> {
        load_version(&self.version(version), lang)
    }

    fn version<'a>(&'a self, name: &'a str) -> BundledVersion<'a> {
        BundledVersion { bundle: self, name }
    }
}

struct BundledVersion<'a> {
    bundle: &'a Bundle,
    name: &'a str,
}

impl BundledVersion<'_> {
    fn key(&self, path: &str) -> String {
        format!("{}/{path}", self.name)
    }
}

impl VersionFiles for BundledVersion<'_> {
    fn name(&self) -> Option<&str> {
        Some(self.name)
    }

    fn read(&self, path: &str) -> Result<Option<Vec<u8>>, LoadError> {
        Ok(self
            .bundle
            .files
            .get(self.key(path).as_str())
            .map(|bytes| bytes.to_vec()))
    }

    fn list(&self, dir: &str) -> Result<Vec<String>, LoadError> {
        let prefix = format!("{}/", self.key(dir));
        Ok(self
            .bundle
            .files
            .range::<str, _>((Bound::Included(prefix.as_str()), Bound::Unbounded))
            .map(|(path, _)| path.as_ref())
            .take_while(|path| path.starts_with(&prefix))
            .filter_map(|path| path.strip_prefix(&prefix))
            .filter(|name| !name.contains('/'))
            .map(str::to_owned)
            .collect())
    }

    fn locate(&self, path: &str) -> PathBuf {
        PathBuf::from(format!("{ORIGIN}/{}", self.key(path)))
    }
}
