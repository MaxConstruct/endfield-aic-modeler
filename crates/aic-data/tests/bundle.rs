//! A bundle holds the same versions as the folder it was made from, and loads them the same.

use std::path::{Path, PathBuf};

use aic_data::{Bundle, Names, list_versions, load};

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

/// Every file in `dir`, keyed by its name, the way the editor's build script embeds `data/`.
fn bundle_of(dir: &Path) -> Bundle {
    let mut files: Vec<(&'static str, &'static [u8])> = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            let name = path.file_name().unwrap().to_str().unwrap().to_owned();
            let bytes = std::fs::read(&path).unwrap();
            files.push((name.leak(), bytes.leak()));
        }
    }
    Bundle::new(&files)
}

#[test]
fn lists_the_same_versions_newest_first() {
    let bundle = bundle_of(&fixtures());
    let names = |versions: Vec<aic_data::DataVersion>| -> Vec<String> {
        versions.into_iter().map(|v| v.name).collect()
    };
    assert_eq!(names(bundle.versions()), ["v1.10.0", "v1.9.0", "broken"]);
    assert_eq!(
        names(bundle.versions()),
        names(list_versions(&fixtures()).unwrap())
    );
}

#[test]
fn loads_the_same_catalog_as_the_folder_overlay_included() {
    let bundle = bundle_of(&fixtures());
    for version in ["v1.10.0", "v1.9.0"] {
        let from_disk = load(&fixtures().join(format!("{version}.json")), english()).unwrap();
        let bundled = bundle.load(version, english()).unwrap();
        assert_eq!(bundled.to_json(), from_disk.to_json());
        assert_eq!(bundled.version(), version);
    }
    assert!(
        bundle
            .load("v1.9.0", english())
            .unwrap()
            .item("copper_ore")
            .is_some()
    );
}

#[test]
fn loads_the_real_data_as_the_folder_does() {
    let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
    let newest = &list_versions(&data).unwrap()[0];
    let from_disk = load(&newest.path, english()).unwrap();
    let bundled = bundle_of(&data).load(&newest.name, english()).unwrap();
    assert_eq!(bundled.to_json(), from_disk.to_json());
}

#[test]
fn a_missing_version_names_where_it_was_looked_for() {
    let error = bundle_of(&fixtures())
        .load("v0.0.1", english())
        .unwrap_err();
    assert!(error.to_string().contains("v0.0.1.json"), "{error}");
}
