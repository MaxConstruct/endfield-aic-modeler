//! Reading one data version, wherever its files are: a folder on disk or a [`crate::Bundle`].

use std::io;
use std::path::PathBuf;

use crate::catalog::{Catalog, ImageKind, LoadWarning};
use crate::error::LoadError;
use crate::parse::{Sources, parse};

pub(crate) const DATA_FILE: &str = "data.json";
const OVERLAY_FILE: &str = "overlay.json";
const ICONS_FILE: &str = "icons.webp";
const IMAGES_DIR: &str = "images";
const I18N_DIR: &str = "i18n";

/// One data version: a `data/<version>/` folder, or the same files inside a bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataVersion {
    /// The folder name, which is the version string (e.g. `v1.5.3`).
    pub name: String,
    /// The folder on disk. For a bundled version, the folder inside the bundle.
    pub path: PathBuf,
}

/// The files of one data version, by their paths inside the version folder (`i18n/en.json`).
pub(crate) trait VersionFiles {
    /// The folder name, which is the version string.
    fn name(&self) -> Option<&str>;
    /// A file's bytes, or `None` if there is no such file.
    fn read(&self, path: &str) -> Result<Option<Vec<u8>>, LoadError>;
    /// The names of the files in a folder; empty if there is no such folder.
    fn list(&self, dir: &str) -> Result<Vec<String>, LoadError>;
    /// Where a file is, for error messages.
    fn locate(&self, path: &str) -> PathBuf;
}

/// Reads `data.json`, `i18n/<lang>.json` and, if present, `overlay.json`, `icons.webp` and the PNG
/// files under `images/`.
///
/// The catalog's version is the folder name. If `data.json` declares a different one, that is a
/// [`LoadWarning::VersionMismatch`].
pub(crate) fn load_version(files: &impl VersionFiles, lang: &str) -> Result<Catalog, LoadError> {
    let i18n_file = format!("{I18N_DIR}/{lang}.json");
    let data = required(files, DATA_FILE)?;
    let overlay = files.read(OVERLAY_FILE)?;
    let i18n = required(files, &i18n_file)?;
    let icons = files.read(ICONS_FILE)?;
    let (data_path, overlay_path, i18n_path) = (
        files.locate(DATA_FILE),
        files.locate(OVERLAY_FILE),
        files.locate(&i18n_file),
    );
    let sources = Sources {
        data: &data_path,
        overlay: &overlay_path,
        i18n: &i18n_path,
    };
    let mut catalog = parse(&data, overlay.as_deref(), &i18n, icons.as_deref(), &sources)?;
    read_images(files, &mut catalog)?;
    if let Some(folder) = files.name() {
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

/// The languages a version has names for: the stems of `i18n/*.json`, sorted.
pub(crate) fn languages(files: &impl VersionFiles) -> Result<Vec<String>, LoadError> {
    let mut languages: Vec<String> = files
        .list(I18N_DIR)?
        .iter()
        .filter_map(|name| name.strip_suffix(".json"))
        .map(str::to_owned)
        .collect();
    languages.sort();
    Ok(languages)
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

fn required(files: &impl VersionFiles, path: &str) -> Result<Vec<u8>, LoadError> {
    files.read(path)?.ok_or_else(|| LoadError::Read {
        file: files.locate(path),
        source: io::ErrorKind::NotFound.into(),
    })
}

/// Reads `images/<kind folder>/<id>.png` for every kind. A missing folder is fine; other files are
/// skipped.
fn read_images(files: &impl VersionFiles, catalog: &mut Catalog) -> Result<(), LoadError> {
    for kind in ImageKind::ALL {
        let dir = format!("{IMAGES_DIR}/{}", kind.folder());
        for name in files.list(&dir)? {
            let Some((id, extension)) = name.rsplit_once('.') else {
                continue;
            };
            if !extension.eq_ignore_ascii_case("png") {
                continue;
            }
            let path = format!("{dir}/{name}");
            if let Some(bytes) = files.read(&path)? {
                catalog.add_image(kind, id.to_owned(), bytes);
            }
        }
    }
    Ok(())
}
