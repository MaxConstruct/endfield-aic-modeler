//! Finding and reading data folders on disk. Native only: the web build fetches the same files and
//! calls `Catalog::from_bytes` itself.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::catalog::Catalog;
use crate::error::LoadError;
use crate::version::{DataVersion, VersionFiles, languages, load_version, sort_newest_first};

/// Environment variable that points at the `data/` folder.
pub const DATA_DIR_ENV: &str = "AIC_DATA_DIR";

const DATA_FILE: &str = "data.json";

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

/// Every subfolder of `data_dir` that holds a `data.json`, newest first.
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
        if !path.join(DATA_FILE).is_file() {
            continue;
        }
        // A folder name that isn't UTF-8 can't be a version string.
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
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

/// Reads `data.json`, `i18n/<lang>.json` and, if present, `overlay.json`, `icons.webp` and the PNG
/// files under `images/` from one version folder.
///
/// The catalog's version is the folder name. If `data.json` declares a different one, that is a
/// [`crate::LoadWarning::VersionMismatch`].
pub fn load(version_dir: &Path, lang: &str) -> Result<Catalog, LoadError> {
    load_version(&Folder(version_dir), lang)
}

/// The languages a version folder has names for: the stems of `i18n/*.json`, sorted.
pub fn list_languages(version_dir: &Path) -> Result<Vec<String>, LoadError> {
    let dir = version_dir.join("i18n");
    if !dir.is_dir() {
        // A version without names is broken; say which folder is missing.
        return Err(LoadError::Read {
            file: dir,
            source: io::ErrorKind::NotFound.into(),
        });
    }
    languages(&Folder(version_dir))
}

/// A version folder on disk.
struct Folder<'a>(&'a Path);

impl VersionFiles for Folder<'_> {
    fn name(&self) -> Option<&str> {
        self.0.file_name().and_then(|name| name.to_str())
    }

    fn read(&self, path: &str) -> Result<Option<Vec<u8>>, LoadError> {
        let file = self.locate(path);
        match fs::read(&file) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(LoadError::Read { file, source }),
        }
    }

    fn list(&self, dir: &str) -> Result<Vec<String>, LoadError> {
        let folder = self.locate(dir);
        let read_error = |source| LoadError::Read {
            file: folder.clone(),
            source,
        };
        let entries = match fs::read_dir(&folder) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => return Err(read_error(source)),
        };
        let mut names = Vec::new();
        for entry in entries {
            // A file name that isn't UTF-8 can't be an ID.
            if let Some(name) = entry.map_err(read_error)?.file_name().to_str() {
                names.push(name.to_owned());
            }
        }
        Ok(names)
    }

    fn locate(&self, path: &str) -> PathBuf {
        path.split('/')
            .filter(|part| !part.is_empty())
            .fold(self.0.to_path_buf(), |dir, part| dir.join(part))
    }
}
