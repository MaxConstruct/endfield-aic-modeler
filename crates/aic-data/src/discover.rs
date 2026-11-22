//! Finding and reading data folders on disk. Native only.

use std::borrow::Cow;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::catalog::Catalog;
use crate::error::LoadError;
use crate::version::{DataFiles, DataVersion, Names, load_version, sort_newest_first, version_of};

/// Environment variable that points at the `data/` folder.
pub const DATA_DIR_ENV: &str = "AIC_DATA_DIR";

/// Where to look for the `data/` folder, in order: the `--data` flag, `AIC_DATA_DIR`, `data/` next to
/// the executable, then `data/` in the working directory.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DataDirSearch {
    /// The `--data <path>` command-line flag.
    pub flag: Option<PathBuf>,
    /// The value of `AIC_DATA_DIR`.
    pub env: Option<PathBuf>,
    /// The folder that holds the executable.
    pub exe_dir: Option<PathBuf>,
    /// The working directory.
    pub cwd: Option<PathBuf>,
}

impl DataDirSearch {
    /// Fills in everything but the flag from the running process.
    pub fn from_process(flag: Option<&Path>) -> Self {
        Self {
            flag: flag.map(Path::to_path_buf),
            env: std::env::var_os(DATA_DIR_ENV)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from),
            exe_dir: std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(Path::to_path_buf)),
            cwd: std::env::current_dir().ok(),
        }
    }

    /// The first folder that exists. A flag or environment variable is final: if it points nowhere,
    /// that is an error rather than a silent fallback that would hide the typo.
    pub fn resolve(&self) -> Result<PathBuf, LoadError> {
        if let Some(explicit) = self.flag.as_ref().or(self.env.as_ref()) {
            return if explicit.is_dir() {
                Ok(explicit.clone())
            } else {
                Err(LoadError::NoDataDir {
                    tried: vec![explicit.clone()],
                })
            };
        }
        let tried: Vec<PathBuf> = [&self.exe_dir, &self.cwd]
            .into_iter()
            .flatten()
            .map(|dir| dir.join("data"))
            .collect();
        match tried.iter().find(|candidate| candidate.is_dir()) {
            Some(found) => Ok(found.clone()),
            None => Err(LoadError::NoDataDir { tried }),
        }
    }
}

/// Finds the `data/` folder for this process. See [`DataDirSearch`] for the order.
pub fn find_data_dir(flag: Option<&Path>) -> Result<PathBuf, LoadError> {
    DataDirSearch::from_process(flag).resolve()
}

/// Every version in `data_dir`, newest first: each `<version>.json` file that is not an overlay
/// (`<version>_overlay.json`).
///
/// Names like `v1.10.0` sort by their numbers; any other name sorts after them, by string.
pub fn list_versions(data_dir: &Path) -> Result<Vec<DataVersion>, LoadError> {
    let read_error = |source| LoadError::Read {
        file: data_dir.to_path_buf(),
        source,
    };
    let mut versions = Vec::new();
    for entry in fs::read_dir(data_dir).map_err(read_error)? {
        let path = entry.map_err(read_error)?.path();
        if !path.is_file() {
            continue;
        }
        // A file name that isn't UTF-8 can't be a version string.
        let Some(name) = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(version_of)
        else {
            continue;
        };
        versions.push(DataVersion {
            name: name.to_owned(),
            path,
        });
    }
    sort_newest_first(&mut versions);
    Ok(versions)
}

/// Reads a version file (`data/v1.5.3.json`) and, if present, the overlay beside it
/// (`data/v1.5.3_overlay.json`), named by `names`.
///
/// The catalog's version is the file's name without `.json`. If the file declares a different
/// one, that is a [`crate::LoadWarning::VersionMismatch`].
pub fn load(version_file: &Path, names: Names<'_>) -> Result<Catalog, LoadError> {
    let version = version_file
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| LoadError::Read {
            file: version_file.to_path_buf(),
            source: io::ErrorKind::InvalidInput.into(),
        })?;
    let dir = version_file.parent().unwrap_or(Path::new(""));
    load_version(&Folder(dir), version, names)
}

/// A data folder on disk.
struct Folder<'a>(&'a Path);

impl DataFiles for Folder<'_> {
    fn read(&self, name: &str) -> Result<Option<Cow<'_, [u8]>>, LoadError> {
        let file = self.locate(name);
        match fs::read(&file) {
            Ok(bytes) => Ok(Some(Cow::Owned(bytes))),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(LoadError::Read { file, source }),
        }
    }

    fn locate(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}
