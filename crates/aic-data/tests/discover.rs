//! Finding `data/`, listing its versions and loading one from disk.

use std::path::{Path, PathBuf};

use aic_data::{DataDirSearch, LoadError, LoadWarning, Names, list_versions, load};

/// The fixtures' data folder: `v1.10.0.json`, `v1.9.0.json` with its overlay, `broken.json` and a
/// README.
fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/data")
}

const EN: &[u8] = include_bytes!("fixtures/names/en.json");

fn english() -> Names<'static> {
    Names {
        file: "names/en.json",
        json: EN,
    }
}

/// The repository root, which has a real `data/` folder.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn nowhere() -> PathBuf {
    fixtures().join("does-not-exist")
}

#[test]
fn a_version_is_a_json_file_that_is_not_an_overlay_newest_first() {
    let versions = list_versions(&fixtures()).unwrap();
    let names: Vec<_> = versions.iter().map(|v| v.name.as_str()).collect();
    // Numeric order: as strings, v1.9.0 would come first. A name that isn't a version goes last.
    // `v1.9.0_overlay.json` belongs to v1.9.0, and `README.md` is not JSON.
    assert_eq!(names, ["v1.10.0", "v1.9.0", "broken"]);
    assert_eq!(versions[0].path, fixtures().join("v1.10.0.json"));
}

#[test]
fn listing_a_missing_folder_is_an_error() {
    assert!(matches!(
        list_versions(&nowhere()),
        Err(LoadError::Read { .. })
    ));
}

#[test]
fn load_uses_the_file_name_as_version_and_the_names_it_is_given() {
    let catalog = load(&fixtures().join("v1.10.0.json"), english()).unwrap();
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
fn other_names_name_the_same_data_otherwise() {
    let names = Names {
        file: "names/xx.json",
        json: br#"{ "items": { "copper_nugget": "Kupfer" } }"#,
    };
    let catalog = load(&fixtures().join("v1.10.0.json"), names).unwrap();
    assert_eq!(catalog.item("copper_nugget").unwrap().name, "Kupfer");
}

#[test]
fn the_overlay_beside_a_version_is_merged_into_it() {
    let catalog = load(&fixtures().join("v1.9.0.json"), english()).unwrap();
    // `v1.9.0_overlay.json` adds Cuprium Ore, named by the names table.
    assert_eq!(catalog.item("copper_ore").unwrap().name, "Cuprium Ore");
    // And no other version takes it.
    let other = load(&fixtures().join("v1.10.0.json"), english()).unwrap();
    assert_eq!(other.items().len(), 9);
}

#[test]
fn version_mismatch_is_a_warning() {
    let catalog = load(&fixtures().join("v1.9.0.json"), english()).unwrap();
    assert_eq!(catalog.version(), "v1.9.0");
    assert!(catalog.warnings().contains(&LoadWarning::VersionMismatch {
        name: "v1.9.0".to_owned(),
        declared: "v1.8.0".to_owned(),
    }));
}

#[test]
fn a_missing_version_file_names_the_file() {
    let file = fixtures().join("v0.0.1.json");
    let error = load(&file, english()).unwrap_err();
    let LoadError::Read { file: named, .. } = &error else {
        panic!("expected a Read error, got {error:?}");
    };
    assert_eq!(*named, file);
}

#[test]
fn json_errors_name_the_real_file() {
    let file = fixtures().join("broken.json");
    let error = load(&file, english()).unwrap_err();
    let LoadError::Json {
        file: named, path, ..
    } = &error
    else {
        panic!("expected a Json error, got {error:?}");
    };
    assert_eq!(*named, file);
    assert_eq!(path, "recipes[0].time_s");
}

#[test]
fn a_bad_names_table_names_its_file() {
    let names = Names {
        file: "names/en.json",
        json: br#"{ "items": { "belt": 3 } }"#,
    };
    let error = load(&fixtures().join("v1.10.0.json"), names).unwrap_err();
    let LoadError::Json { file, path, .. } = &error else {
        panic!("expected a Json error, got {error:?}");
    };
    assert_eq!(file.to_str(), Some("names/en.json"));
    assert_eq!(path, "items.belt");
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

/// A data folder in the temp directory with the v1.10.0 fixture, plus `extra` files.
fn temp_data(name: &str, extra: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("aic-data-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(fixtures().join("v1.10.0.json"), dir.join("v1.10.0.json")).unwrap();
    for (file, text) in extra {
        std::fs::write(dir.join(file), text).unwrap();
    }
    dir
}

#[test]
fn load_merges_the_versions_overlay_and_its_errors_name_it() {
    let dir = temp_data(
        "overlay",
        &[(
            "v1.10.0_overlay.json",
            r#"{ "recipes": { "remove": ["liquid_sewage-mix"] } }"#,
        )],
    );
    let catalog = load(&dir.join("v1.10.0.json"), english()).expect("loads");
    assert!(catalog.recipe("liquid_sewage-mix").is_none());

    std::fs::write(dir.join("v1.10.0_overlay.json"), r#"{ "oops": 1 }"#).unwrap();
    match load(&dir.join("v1.10.0.json"), english()) {
        Err(LoadError::Json { file, .. }) => {
            assert_eq!(file, dir.join("v1.10.0_overlay.json"));
        }
        other => panic!("expected the overlay's error, got {other:?}"),
    }
    let _ = std::fs::remove_dir_all(dir);
}
