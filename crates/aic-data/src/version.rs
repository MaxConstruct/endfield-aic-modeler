//! Reading one data version, wherever its files are: a data folder on disk or a [`crate::Bundle`].
//!
//! A data version is one file, `<version>.json`, with an optional overlay beside it,
//! `<version>_overlay.json` (ADR 0012). The game names are not part of it: the caller passes
//! them in, from the app's own table.

use std::borrow::Cow;
use std::io;
use std::path::{Path, PathBuf};

use crate::catalog::{Catalog, LoadWarning};
use crate::error::LoadError;
use crate::parse::{Sources, parse};

const EXTENSION: &str = ".json";
const OVERLAY_SUFFIX: &str = "_overlay.json";

/// One data version: a `data/<version>.json` file, or the same file inside a bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataVersion {
    /// The file's name without `.json`, which is the version string (e.g. `v1.5.3`).
    pub name: String,
    /// The file on disk. For a bundled version, its name inside the bundle.
    pub path: PathBuf,
}

/// Game names in one language, in the names file format: display names keyed by game ID. The
/// same table names every data version.
#[derive(Debug, Clone, Copy)]
pub struct Names<'a> {
    /// What errors call the file, such as `names/en.json`.
    pub file: &'a str,
    pub json: &'a [u8],
}

/// The version a file in a data folder holds: `v1.5.3.json` holds `v1.5.3`. An overlay, or any
/// file that isn't JSON, holds none.
pub(crate) fn version_of(file_name: &str) -> Option<&str> {
    if file_name.ends_with(OVERLAY_SUFFIX) {
        return None;
    }
    file_name
        .strip_suffix(EXTENSION)
        .filter(|name| !name.is_empty())
}

/// The files of a data folder, by their names in it (`v1.5.3.json`).
pub(crate) trait DataFiles {
    /// A file's bytes, or `None` if there is no such file.
    fn read(&self, name: &str) -> Result<Option<Cow<'_, [u8]>>, LoadError>;
    /// Where a file is, for error messages.
    fn locate(&self, name: &str) -> PathBuf;
}

/// Reads `<version>.json` and, if present, `<version>_overlay.json`, named by `names`.
///
/// The catalog's version is `version`. If the file declares a different one, that is a
/// [`LoadWarning::VersionMismatch`].
pub(crate) fn load_version(
    files: &impl DataFiles,
    version: &str,
    names: Names<'_>,
) -> Result<Catalog, LoadError> {
    let data_file = format!("{version}{EXTENSION}");
    let overlay_file = format!("{version}{OVERLAY_SUFFIX}");
    let data = files.read(&data_file)?.ok_or_else(|| LoadError::Read {
        file: files.locate(&data_file),
        source: io::ErrorKind::NotFound.into(),
    })?;
    let overlay = files.read(&overlay_file)?;
    let (data_path, overlay_path) = (files.locate(&data_file), files.locate(&overlay_file));
    let sources = Sources {
        data: &data_path,
        overlay: &overlay_path,
        names: Path::new(names.file),
    };
    let mut catalog = parse(&data, overlay.as_deref(), names.json, &sources)?;
    let declared = std::mem::replace(&mut catalog.version, version.to_owned());
    if declared != version {
        catalog.warnings.push(LoadWarning::VersionMismatch {
            name: version.to_owned(),
            declared,
        });
    }
    Ok(catalog)
}

/// Sorts versions newest first. Names like `v1.10.0` sort by their numbers; any other name sorts
/// after them, by string.
pub(crate) fn sort_newest_first(versions: &mut [DataVersion]) {
    versions.sort_by(|a, b| version_key(&b.name).cmp(&version_key(&a.name)));
}

/// `v1.10.0` → `[1, 10, 0]`. Parsed names compare above unparsed ones, since `None < Some`.
fn version_key(name: &str) -> (Option<Vec<u64>>, &str) {
    let numbers = name
        .strip_prefix('v')
        .unwrap_or(name)
        .split('.')
        .map(|part| part.parse().ok())
        .collect();
    (numbers, name)
}

#[cfg(test)]
mod tests {
    use super::version_of;

    #[test]
    fn a_version_is_a_json_file_that_is_not_an_overlay() {
        assert_eq!(version_of("v1.5.3.json"), Some("v1.5.3"));
        assert_eq!(version_of("v1.5.3_overlay.json"), None);
        assert_eq!(version_of("README.md"), None);
        assert_eq!(version_of(".json"), None);
    }
}
