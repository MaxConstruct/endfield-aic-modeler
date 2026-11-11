//! A bundle holds the same versions as the folder it was made from, and loads them the same.

use std::path::{Path, PathBuf};

use aic_data::{Bundle, ImageKind, list_languages, list_versions, load};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Every file under `dir`, keyed by its `/`-separated path below `dir`, the way the app's build
/// script embeds `data/`.
fn bundle_of(dir: &Path) -> Bundle {
    fn walk(root: &Path, dir: &Path, files: &mut Vec<(&'static str, &'static [u8])>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, files);
                continue;
            }
            let key = path
                .strip_prefix(root)
                .unwrap()
                .components()
                .map(|part| part.as_os_str().to_str().unwrap())
                .collect::<Vec<_>>()
                .join("/");
            let bytes = std::fs::read(&path).unwrap();
            files.push((key.leak(), bytes.leak()));
        }
    }
    let mut files = Vec::new();
    walk(dir, dir, &mut files);
    Bundle::new(&files)
}

#[test]
fn lists_the_same_versions_newest_first() {
    let bundle = bundle_of(&fixtures());
    let names = |versions: Vec<aic_data::DataVersion>| -> Vec<String> {
        versions.into_iter().map(|v| v.name).collect()
    };
    assert_eq!(
        names(bundle.versions()),
        names(list_versions(&fixtures()).unwrap())
    );
}

#[test]
fn loads_the_same_catalog_as_the_folder() {
    let bundle = bundle_of(&fixtures());
    let from_disk = load(&fixtures().join("v1.10.0"), "en").unwrap();
    let bundled = bundle.load("v1.10.0", "en").unwrap();
    assert_eq!(bundled.to_json(), from_disk.to_json());
    assert_eq!(bundled.version(), "v1.10.0");
    assert_eq!(
        bundle.languages("v1.10.0").unwrap(),
        list_languages(&fixtures().join("v1.10.0")).unwrap()
    );
}

#[test]
fn loads_the_images_of_the_real_data() {
    let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
    let newest = &list_versions(&data).unwrap()[0];
    let from_disk = load(&newest.path, "en").unwrap();
    let bundled = bundle_of(&data).load(&newest.name, "en").unwrap();
    assert_eq!(bundled.to_json(), from_disk.to_json());
    for item in from_disk.items().keys() {
        assert_eq!(
            bundled.image(ImageKind::Item, item.as_str()),
            from_disk.image(ImageKind::Item, item.as_str()),
            "{item}"
        );
    }
    assert!(bundled.image(ImageKind::Node, "source").is_some());
}

#[test]
fn a_missing_file_names_where_it_was_looked_for() {
    let error = bundle_of(&fixtures()).load("v1.10.0", "xx").unwrap_err();
    assert!(
        error.to_string().contains("v1.10.0/i18n/xx.json"),
        "{error}"
    );
}

/// The web build fetches the files over HTTP, so its bundle owns them.
#[test]
fn a_bundle_of_owned_files_loads_the_same_catalog() {
    let borrowed = bundle_of(&fixtures());
    let owned = Bundle::from_owned(
        borrowed
            .files()
            .map(|(path, bytes)| (path.to_owned(), bytes.to_vec())),
    );
    assert_eq!(
        owned.load("v1.10.0", "en").unwrap().to_json(),
        borrowed.load("v1.10.0", "en").unwrap().to_json()
    );
    assert_eq!(
        owned.languages("v1.10.0").unwrap(),
        borrowed.languages("v1.10.0").unwrap()
    );
}
