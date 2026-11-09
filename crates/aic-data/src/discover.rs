//! Finding and reading data folders on disk. Native only: the web build fetches the same files and
//! calls `Catalog::from_bytes` itself.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::catalog::{Catalog, ImageKind, LoadWarning};
use crate::error::LoadError;
use crate::parse::{Sources, parse};

/// Environment variable that points at the `data/` folder.
pub const DATA_DIR_ENV: &str = "AIC_DATA_DIR";

const DATA_FILE: &str = "data.json";
const OVERLAY_FILE: &str = "overlay.json";
const ICONS_FILE: &str = "icons.webp";
const IMAGES_DIR: &str = "images";

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

/// One `data/<version>/` folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataVersion {
    /// The folder name, which is the version string (e.g. `v1.5.3`).
    pub name: String,
    pub path: PathBuf,
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
    versions.sort_by(|a, b| version_key(&b.name).cmp(&version_key(&a.name)));
    Ok(versions)
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

/// Reads `data.json`, `i18n/<lang>.json` and, if present, `overlay.json`, `icons.webp` and the PNG
/// files under `images/` from one version folder.
///
/// The catalog's version is the folder name. If `data.json` declares a different one, that is a
/// [`LoadWarning::VersionMismatch`].
pub fn load(version_dir: &Path, lang: &str) -> Result<Catalog, LoadError> {
    let data_path = version_dir.join(DATA_FILE);
    let i18n_path = version_dir.join("i18n").join(format!("{lang}.json"));
    let overlay_path = version_dir.join(OVERLAY_FILE);
    let data = read(&data_path)?;
    let overlay = read_optional(&overlay_path)?;
    let i18n = read(&i18n_path)?;
    let icons = read_optional(&version_dir.join(ICONS_FILE))?;
    let sources = Sources {
        data: &data_path,
        overlay: &overlay_path,
        i18n: &i18n_path,
    };
    let mut catalog = parse(&data, overlay.as_deref(), &i18n, icons.as_deref(), &sources)?;
    read_images(&version_dir.join(IMAGES_DIR), &mut catalog)?;
    if let Some(folder) = version_dir.file_name().and_then(|name| name.to_str()) {
        let declared = std::mem::replace(&mut catalog.version, folder.to_owned());
        if declared != folder {
            catalog.warnings.push(LoadWarning::VersionMismatch {
                folder: folder.to_owned(),
                declared,
            });
        }
    }
    Ok(catalog)
}

/// The languages a version folder has names for: the stems of `i18n/*.json`, sorted.
pub fn list_languages(version_dir: &Path) -> Result<Vec<String>, LoadError> {
    let dir = version_dir.join("i18n");
    let read_error = |source| LoadError::Read {
        file: dir.clone(),
        source,
    };
    let mut languages = Vec::new();
    for entry in fs::read_dir(&dir).map_err(read_error)? {
        let path = entry.map_err(read_error)?.path();
        let is_json = path.extension().is_some_and(|ext| ext == "json");
        if let (true, Some(stem)) = (is_json, path.file_stem().and_then(|s| s.to_str())) {
            languages.push(stem.to_owned());
        }
    }
    languages.sort();
    Ok(languages)
}

/// Reads `images/<kind folder>/<id>.png` for every kind. A missing folder is fine; other files are
/// skipped.
fn read_images(dir: &Path, catalog: &mut Catalog) -> Result<(), LoadError> {
    for kind in ImageKind::ALL {
        let folder = dir.join(kind.folder());
        let entries = match fs::read_dir(&folder) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(source) => {
                return Err(LoadError::Read {
                    file: folder,
                    source,
                });
            }
        };
        for entry in entries {
            let path = entry
                .map_err(|source| LoadError::Read {
                    file: folder.clone(),
                    source,
                })?
                .path();
            let is_png = path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("png"));
            let Some(id) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            if is_png {
                let id = id.to_owned();
                catalog.add_image(kind, id, read(&path)?);
            }
        }
    }
    Ok(())
}

fn read(path: &Path) -> Result<Vec<u8>, LoadError> {
    fs::read(path).map_err(|source| LoadError::Read {
        file: path.to_path_buf(),
        source,
    })
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, LoadError> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(LoadError::Read {
            file: path.to_path_buf(),
            source,
        }),
    }
}
