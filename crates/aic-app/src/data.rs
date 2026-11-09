//! Which data version and language the app loads, and switching between them while it runs.

use std::path::{Path, PathBuf};

use aic_data::{Catalog, DataVersion, LoadError};
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

/// The `data/` folder and the versions in it, newest first.
#[derive(Debug, Clone)]
pub struct DataFolder {
    pub dir: PathBuf,
    pub versions: Vec<DataVersion>,
}

impl DataFolder {
    /// Finds the data folder (the `--data` flag first, see `aic_data::find_data_dir`).
    pub fn find(flag: Option<&Path>) -> Result<Self, LoadError> {
        Self::at(aic_data::find_data_dir(flag)?)
    }

    pub fn at(dir: PathBuf) -> Result<Self, LoadError> {
        let versions = aic_data::list_versions(&dir)?;
        if versions.is_empty() {
            return Err(LoadError::NoDataDir { tried: vec![dir] });
        }
        Ok(Self { dir, versions })
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
        aic_data::list_languages(&self.version(choice).path).unwrap_or_default()
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
        aic_data::load(&version.path, language)
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
}
