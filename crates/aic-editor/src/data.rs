//! Which data version and language the app loads, and switching between them while it runs.
//!
//! The data versions come from a `data/` folder on disk or from the program; the names, in every
//! language, always come from the program (`i18n/names/`, ADR 0012).

#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};
use std::sync::Arc;

use aic_data::{Bundle, Catalog, DataVersion, LoadError, Names};
use serde::{Deserialize, Serialize};

use crate::embedded;

/// The storage key for [`DataChoice`].
pub const STORAGE_KEY: &str = "data-choice";
/// The language of game names when none is chosen, or the chosen one is gone.
pub const DEFAULT_LANGUAGE: &str = "en";

/// The data version and game-name language the player picked. `None` is the newest version.
/// Kept in eframe storage.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataChoice {
    pub version: Option<String>,
    pub language: Option<String>,
}

/// The data [`DataFolder::open`] loaded.
#[derive(Debug)]
pub struct Loaded {
    pub catalog: Catalog,
    /// The version and language that loaded.
    pub choice: DataChoice,
    /// Why the chosen data didn't load, when the newest loaded in its place.
    pub failed: Option<LoadError>,
}

/// Where the data versions come from.
#[derive(Debug, Clone)]
pub enum DataSource {
    /// A `data/` folder on disk.
    #[cfg(not(target_arch = "wasm32"))]
    Folder(PathBuf),
    /// The files of a `data/` folder built into the program.
    Bundled(Arc<Bundle>),
}

/// A `data/` folder, on disk or built in, and the versions in it, newest first.
#[derive(Debug, Clone)]
pub struct DataFolder {
    pub source: DataSource,
    pub versions: Vec<DataVersion>,
}

impl DataFolder {
    /// Finds the data folder (the `--data` flag first, see `aic_data::DataDirSearch`), else uses the
    /// built-in data. A `--data` flag or `AIC_DATA_DIR` that points nowhere is an error, not a
    /// silent fallback that would hide the typo.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn find(flag: Option<&Path>) -> Result<Self, LoadError> {
        let search = aic_data::DataDirSearch::from_process(flag);
        let explicit = search.flag.is_some() || search.env.is_some();
        match search.resolve() {
            Ok(dir) => Self::at(dir),
            Err(error) if explicit => Err(error),
            Err(error) => Self::built_in().ok_or(error),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn at(dir: PathBuf) -> Result<Self, LoadError> {
        let versions = aic_data::list_versions(&dir)?;
        if versions.is_empty() {
            return Err(LoadError::NoDataDir { tried: vec![dir] });
        }
        Ok(Self {
            source: DataSource::Folder(dir),
            versions,
        })
    }

    /// The data built into the program, if any was.
    pub fn built_in() -> Option<Self> {
        Self::bundled(embedded::bundle())
    }

    /// The versions in `bundle`, if it has any.
    pub fn bundled(bundle: Arc<Bundle>) -> Option<Self> {
        let versions = bundle.versions();
        (!versions.is_empty()).then_some(Self {
            source: DataSource::Bundled(bundle),
            versions,
        })
    }

    /// The chosen version, or the newest when none is chosen or the chosen one is gone.
    pub fn version(&self, choice: &DataChoice) -> &DataVersion {
        choice
            .version
            .as_ref()
            .and_then(|name| self.versions.iter().find(|v| v.name == *name))
            .unwrap_or(&self.versions[0])
    }

    /// The languages game names come in: the program's names tables, the same for every version.
    pub fn languages(&self) -> Vec<String> {
        embedded::languages().map(str::to_owned).collect()
    }

    /// Opens the data last chosen: that version and language, or, when they are there but fail
    /// to load, the newest version in English. The choice it returns is the one that loaded, so
    /// that the settings and the next save name the data in use.
    pub fn open(&self, choice: DataChoice) -> Result<Loaded, LoadError> {
        match self.load(&choice) {
            Ok(catalog) => Ok(Loaded {
                catalog,
                choice,
                failed: None,
            }),
            Err(error) => {
                let newest = DataChoice::default();
                let catalog = self.load(&newest)?;
                Ok(Loaded {
                    catalog,
                    choice: newest,
                    failed: Some(error),
                })
            }
        }
    }

    /// Loads the chosen version with the names in the chosen language, falling back to English
    /// when the program has no names in that language.
    pub fn load(&self, choice: &DataChoice) -> Result<Catalog, LoadError> {
        let version = self.version(choice);
        let names = names(choice.language.as_deref().unwrap_or(DEFAULT_LANGUAGE));
        match &self.source {
            #[cfg(not(target_arch = "wasm32"))]
            DataSource::Folder(_) => aic_data::load(&version.path, names),
            DataSource::Bundled(bundle) => bundle.load(&version.name, names),
        }
    }
}

/// The built-in names in `language`, else in English, else none at all (every name then falls
/// back to the data's Chinese name).
fn names(language: &str) -> Names<'static> {
    embedded::names(language)
        .or_else(|| embedded::names(DEFAULT_LANGUAGE))
        .unwrap_or(Names {
            file: "<no built-in names>",
            json: b"{}",
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_data() -> DataFolder {
        DataFolder::at(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data")).unwrap()
    }

    /// The test data folder of `aic-data`: `v1.10.0.json`, `v1.9.0.json` with an overlay, and
    /// `broken.json`.
    fn fixture_data() -> DataFolder {
        DataFolder::at(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../aic-data/tests/fixtures/data"),
        )
        .unwrap()
    }

    #[test]
    fn a_choice_that_is_gone_falls_back_to_the_newest_version_in_english() {
        let folder = repo_data();
        let choice = DataChoice {
            version: Some("v0.0.0".into()),
            language: Some("xx".into()),
        };
        assert_eq!(folder.version(&choice).name, folder.versions[0].name);
        let catalog = folder.load(&choice).expect("loads");
        assert_eq!(catalog.version(), folder.versions[0].name);
        assert_eq!(catalog.item("copper_nugget").unwrap().name, "Cuprium");
    }

    #[test]
    fn a_chosen_version_that_fails_to_load_opens_the_newest_and_says_so() {
        let folder = fixture_data();
        let broken = DataChoice {
            version: Some("broken".into()),
            language: None,
        };
        let opened = folder.open(broken).expect("the newest loads");
        assert_eq!(opened.catalog.version(), folder.versions[0].name);
        assert_eq!(opened.choice, DataChoice::default());
        assert!(opened.failed.is_some());

        let newest = DataChoice::default();
        let opened = folder.open(newest.clone()).expect("loads");
        assert_eq!(opened.choice, newest);
        assert!(opened.failed.is_none());
    }

    #[test]
    fn without_a_data_folder_the_built_in_data_is_used() {
        // Tests run in crates/aic-editor from target/…/deps: no data/ folder in either.
        if std::env::var_os(aic_data::DATA_DIR_ENV).is_some() {
            return;
        }
        let folder = DataFolder::find(None).expect("built-in data");
        assert!(matches!(folder.source, DataSource::Bundled(_)));
    }

    #[test]
    fn a_data_folder_on_disk_takes_the_built_in_datas_place_named_by_the_programs_names() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../aic-data/tests/fixtures/data");
        let folder = DataFolder::find(Some(&dir)).expect("the folder is used");
        assert!(matches!(folder.source, DataSource::Folder(_)));
        let names: Vec<&str> = folder.versions.iter().map(|v| v.name.as_str()).collect();
        assert_eq!(names, ["v1.10.0", "v1.9.0", "broken"]);
        // `v1.9.0_overlay.json` adds an item that the program's names table names.
        let choice = DataChoice {
            version: Some("v1.9.0".into()),
            language: None,
        };
        let catalog = folder.load(&choice).expect("loads");
        assert_eq!(catalog.item("copper_ore").unwrap().name, "Cuprium Ore");
    }

    #[test]
    fn the_languages_are_the_programs_names_tables_whatever_the_folder() {
        assert_eq!(fixture_data().languages(), ["en"]);
        assert_eq!(repo_data().languages(), ["en"]);
    }

    #[test]
    fn a_data_flag_that_points_nowhere_is_an_error() {
        assert!(DataFolder::find(Some(Path::new("does-not-exist"))).is_err());
    }

    #[test]
    fn the_built_in_data_is_the_repository_data() {
        let built_in = DataFolder::built_in().expect("data/ is built in");
        let names = |folder: &DataFolder| -> Vec<String> {
            folder.versions.iter().map(|v| v.name.clone()).collect()
        };
        assert_eq!(names(&built_in), names(&repo_data()));
        let choice = DataChoice::default();
        let bundled = built_in.load(&choice).expect("loads");
        let from_disk = repo_data().load(&choice).expect("loads");
        assert_eq!(bundled.to_json(), from_disk.to_json());
        assert!(bundled.warnings().is_empty(), "{:?}", bundled.warnings());
    }
}
