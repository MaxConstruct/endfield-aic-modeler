//! Finding `data/`, listing its versions and loading one from disk.

use std::path::{Path, PathBuf};

use aic_data::{DataDirSearch, ImageKind, LoadError, LoadWarning, list_versions, load};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// The repository root, which has a real `data/` folder.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn nowhere() -> PathBuf {
    fixtures().join("does-not-exist")
}

#[test]
fn lists_versions_newest_first_and_skips_other_folders() {
    let versions = list_versions(&fixtures()).unwrap();
    let names: Vec<_> = versions.iter().map(|v| v.name.as_str()).collect();
    // Numeric order: as strings, v1.9.0 would come first. A name that isn't a version goes last.
    // `not-a-version/` has no data.json, so it isn't listed.
    assert_eq!(names, ["v1.10.0", "v1.9.0", "broken"]);
    assert_eq!(versions[0].path, fixtures().join("v1.10.0"));
}

#[test]
fn listing_a_missing_folder_is_an_error() {
    assert!(matches!(
        list_versions(&nowhere()),
        Err(LoadError::Read { .. })
    ));
}

#[test]
fn load_uses_the_folder_name_as_version() {
    let catalog = load(&fixtures().join("v1.10.0"), "en").unwrap();
    assert_eq!(catalog.version(), "v1.10.0");
    assert_eq!(catalog.item("copper_nugget").unwrap().name, "Cuprium");
    assert!(
        !catalog
            .warnings()
            .iter()
            .any(|w| matches!(w, LoadWarning::VersionMismatch { .. }))
    );
}

#[test]
fn missing_icons_are_not_an_error() {
    let catalog = load(&fixtures().join("v1.10.0"), "en").unwrap();
    assert!(catalog.icons_webp().is_none());
}

#[test]
fn icons_are_read_when_present() {
    let catalog = load(&fixtures().join("v1.9.0"), "en").unwrap();
    assert!(
        catalog
            .icons_webp()
            .unwrap()
            .starts_with(b"not a real webp")
    );
}

#[test]
fn images_are_read_from_their_folders_by_id() {
    let catalog = load(&fixtures().join("v1.9.0"), "en").unwrap();
    let png = |kind, id| {
        catalog
            .image(kind, id)
            .map(|bytes| bytes.starts_with(b"\x89PNG"))
    };
    assert_eq!(png(ImageKind::Item, "copper_ore"), Some(true));
    assert_eq!(png(ImageKind::Facility, "furnance_1"), Some(true));
    assert_eq!(png(ImageKind::Node, "source"), Some(true));
    // Kinds don't mix, and files that aren't PNG are skipped.
    assert_eq!(png(ImageKind::Facility, "copper_ore"), None);
    assert_eq!(png(ImageKind::Item, "README"), None);
}

#[test]
fn missing_images_are_not_an_error() {
    let catalog = load(&fixtures().join("v1.10.0"), "en").unwrap();
    assert!(catalog.image(ImageKind::Item, "copper_ore").is_none());
}

#[test]
fn version_mismatch_is_a_warning() {
    let catalog = load(&fixtures().join("v1.9.0"), "en").unwrap();
    assert_eq!(catalog.version(), "v1.9.0");
    assert!(catalog.warnings().contains(&LoadWarning::VersionMismatch {
        folder: "v1.9.0".to_owned(),
        declared: "v1.8.0".to_owned(),
    }));
}

#[test]
fn missing_language_file_names_the_file() {
    let error = load(&fixtures().join("v1.10.0"), "fr").unwrap_err();
    let LoadError::Read { file, .. } = &error else {
        panic!("expected a Read error, got {error:?}");
    };
    assert_eq!(*file, fixtures().join("v1.10.0/i18n/fr.json"));
}

#[test]
fn missing_data_json_names_the_file() {
    let dir = fixtures().join("not-a-version");
    let error = load(&dir, "en").unwrap_err();
    let LoadError::Read { file, .. } = &error else {
        panic!("expected a Read error, got {error:?}");
    };
    assert_eq!(*file, dir.join("data.json"));
}

#[test]
fn json_errors_name_the_real_file() {
    let dir = fixtures().join("broken");
    let error = load(&dir, "en").unwrap_err();
    let LoadError::Json { file, path, .. } = &error else {
        panic!("expected a Json error, got {error:?}");
    };
    assert_eq!(*file, dir.join("data.json"));
    assert_eq!(path, "recipes[0].time");
}

#[test]
fn flag_wins_over_everything() {
    let search = DataDirSearch {
        flag: Some(fixtures()),
        env: Some(repo_root().join("data")),
        exe_dir: Some(repo_root()),
        cwd: Some(repo_root()),
    };
    assert_eq!(search.resolve().unwrap(), fixtures());
}

#[test]
fn env_comes_before_the_default_folders() {
    let search = DataDirSearch {
        flag: None,
        env: Some(fixtures()),
        exe_dir: Some(repo_root()),
        cwd: Some(repo_root()),
    };
    assert_eq!(search.resolve().unwrap(), fixtures());
}

#[test]
fn data_next_to_the_executable_comes_before_the_working_directory() {
    let search = DataDirSearch {
        flag: None,
        env: None,
        exe_dir: Some(repo_root()),
        cwd: Some(fixtures()),
    };
    assert_eq!(search.resolve().unwrap(), repo_root().join("data"));
}

#[test]
fn falls_back_to_data_in_the_working_directory() {
    let search = DataDirSearch {
        flag: None,
        env: None,
        exe_dir: Some(nowhere()),
        cwd: Some(repo_root()),
    };
    assert_eq!(search.resolve().unwrap(), repo_root().join("data"));
}

#[test]
fn a_wrong_flag_is_an_error_not_a_fallback() {
    let search = DataDirSearch {
        flag: Some(nowhere()),
        env: None,
        exe_dir: Some(repo_root()),
        cwd: Some(repo_root()),
    };
    let Err(LoadError::NoDataDir { tried }) = search.resolve() else {
        panic!("a flag that points nowhere must not fall back");
    };
    assert_eq!(tried, [nowhere()]);
}

#[test]
fn nothing_found_lists_every_place_tried() {
    let search = DataDirSearch {
        flag: None,
        env: None,
        exe_dir: Some(nowhere()),
        cwd: Some(fixtures()),
    };
    let Err(LoadError::NoDataDir { tried }) = search.resolve() else {
        panic!("expected NoDataDir");
    };
    assert_eq!(tried, [nowhere().join("data"), fixtures().join("data")]);
}
