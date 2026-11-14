//! Where documents are kept: files on disk, or, in the web build, the browser's storage.
//!
//! The browser has no files the app can write, so the web build keeps its own small file system,
//! [`BrowserFiles`]: save files by `/`-separated paths, saved with the settings in eframe storage
//! (the browser's local storage). Drafts, the last document and the recent files work on it as they
//! do on disk. A file the player opens is uploaded into it, and Save also downloads a copy.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The storage key for [`BrowserFiles`].
pub const BROWSER_FILES_KEY: &str = "browser-files";

#[derive(Debug, Clone)]
pub enum FileStore {
    /// Files on disk, through `std::fs`.
    Disk,
    /// Files in the browser's storage.
    Browser(BrowserFiles),
}

/// Save files held in memory by path, kept in eframe storage between visits.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserFiles {
    files: BTreeMap<PathBuf, String>,
}

impl BrowserFiles {
    pub fn contains(&self, path: &Path) -> bool {
        self.files.contains_key(path)
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

impl Default for FileStore {
    /// Disk on desktop, the browser's storage on the web.
    fn default() -> Self {
        if cfg!(target_arch = "wasm32") {
            Self::Browser(BrowserFiles::default())
        } else {
            Self::Disk
        }
    }
}

impl FileStore {
    pub fn is_browser(&self) -> bool {
        matches!(self, Self::Browser(_))
    }

    pub fn read(&self, path: &Path) -> io::Result<String> {
        match self {
            Self::Disk => std::fs::read_to_string(path),
            Self::Browser(files) => files
                .files
                .get(path)
                .cloned()
                .ok_or_else(|| io::ErrorKind::NotFound.into()),
        }
    }

    /// Writes `text` to `path`, creating its folder. On disk the write goes to a temporary file
    /// that then replaces the real one, so a crash mid-write can't leave half a file.
    pub fn write(&mut self, path: &Path, text: &str) -> io::Result<()> {
        match self {
            Self::Disk => {
                if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
                    std::fs::create_dir_all(dir)?;
                }
                write_atomically(path, text.as_bytes())
            }
            Self::Browser(files) => {
                files.files.insert(path.to_owned(), text.to_owned());
                Ok(())
            }
        }
    }

    /// Deletes `path`. A file that isn't there is fine.
    pub fn remove(&mut self, path: &Path) {
        match self {
            Self::Disk => {
                let _ = std::fs::remove_file(path);
            }
            Self::Browser(files) => {
                files.files.remove(path);
            }
        }
    }

    pub fn exists(&self, path: &Path) -> bool {
        match self {
            Self::Disk => path.exists(),
            Self::Browser(files) => files.files.contains_key(path),
        }
    }

    /// How a file is named to the player: its whole path on disk, only its name in the browser,
    /// where the folders are the app's own.
    pub fn label(&self, path: &Path) -> String {
        match self {
            Self::Disk => path.display().to_string(),
            Self::Browser(_) => path.file_name().map_or_else(
                || path.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            ),
        }
    }
}

/// Writes `bytes` to a temporary file next to `path`, then puts it in `path`'s place.
fn write_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(".tmp");
    let temporary = PathBuf::from(temporary);
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(&temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_atomic_write_replaces_the_file_and_leaves_no_temporary() {
        let dir = std::env::temp_dir().join(format!("aic-atomic-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("plan.aicm");
        std::fs::write(&path, "old").unwrap();
        FileStore::Disk.write(&path, "new").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn browser_files_are_read_back_removed_and_named_without_their_folder() {
        let mut store = FileStore::Browser(BrowserFiles::default());
        let path = Path::new("/files/plan.aicm");
        assert!(!store.exists(path));
        assert_eq!(
            store.read(path).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        store.write(path, "{}").unwrap();
        assert_eq!(store.read(path).unwrap(), "{}");
        assert_eq!(store.label(path), "plan.aicm");
        store.remove(path);
        assert!(!store.exists(path));
    }
}
