//! Which data version and language the app loads, and switching between them while it runs.

use std::path::{Path, PathBuf};

use aic_data::{Bundle, Catalog, DataDirSearch, DataVersion, LoadError};
use serde::{Deserialize, Serialize};

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

/// Where the data versions come from.
#[derive(Debug, Clone)]
pub enum DataSource {
    /// A `data/` folder on disk.
    Folder(PathBuf),
    /// The data built into the program.
    BuiltIn(&'static Bundle),
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
    pub fn find(flag: Option<&Path>) -> Result<Self, LoadError> {
        let search = DataDirSearch::from_process(flag);
        let explicit = search.flag.is_some() || search.env.is_some();
        match search.resolve() {
            Ok(dir) => Self::at(dir),
            Err(error) if explicit => Err(error),
            Err(error) => Self::built_in().ok_or(error),
        }
    }

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
        let bundle = crate::embedded::bundle();
        let versions = bundle.versions();
        (!versions.is_empty()).then_some(Self {
            source: DataSource::BuiltIn(bundle),
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

    /// The languages the chosen version has names in.
    pub fn languages(&self, choice: &DataChoice) -> Vec<String> {
        let version = self.version(choice);
        match &self.source {
            DataSource::Folder(_) => aic_data::list_languages(&version.path),
            DataSource::BuiltIn(bundle) => bundle.languages(&version.name),
        }
        .unwrap_or_default()
    }

    /// Loads the chosen version in the chosen language, falling back to English when that
    /// version has no such language.
    pub fn load(&self, choice: &DataChoice) -> Result<Catalog, LoadError> {
        let version = self.version(choice);
        let wanted = choice.language.as_deref().unwrap_or(DEFAULT_LANGUAGE);
        let language = if self.languages(choice).iter().any(|l| l == wanted) {
            wanted
        } else {
            DEFAULT_LANGUAGE
        };
        match &self.source {
            DataSource::Folder(_) => aic_data::load(&version.path, language),
            DataSource::BuiltIn(bundle) => bundle.load(&version.name, language),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_data() -> DataFolder {
        DataFolder::at(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data")).unwrap()
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
    }

    #[test]
    fn without_a_data_folder_the_built_in_data_is_used() {
        // Tests run in crates/aic-app from target/…/deps: no data/ folder in either.
        if std::env::var_os(aic_data::DATA_DIR_ENV).is_some() {
            return;
        }
        let folder = DataFolder::find(None).expect("built-in data");
        assert!(matches!(folder.source, DataSource::BuiltIn(_)));
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
        assert_eq!(built_in.languages(&choice), repo_data().languages(&choice));
        let bundled = built_in.load(&choice).expect("loads");
        let from_disk = repo_data().load(&choice).expect("loads");
        assert_eq!(bundled.to_json(), from_disk.to_json());
    }
}
