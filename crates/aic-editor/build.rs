//! Builds the game data and the names into the program, on the desktop and the web alike
//! (ADR 0012): every `<version>.json` and `<version>_overlay.json` in the repository's `data/`,
//! and every names table in `i18n/names/<lang>.json`, go into `$OUT_DIR/embedded.rs` as tables of
//! `include_bytes!`, which `src/embedded.rs` reads. A missing folder gives an empty table.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::{env, fs};

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("set by cargo"));
    // crates/aic-editor → the repository root.
    let data = manifest
        .ancestors()
        .nth(2)
        .expect("aic-editor sits two folders below the repository root")
        .join("data");
    let names = manifest.join("i18n").join("names");

    let mut table = String::new();
    write_table(&mut table, "DATA", &json_files(&data), |name| name);
    write_table(&mut table, "NAMES", &json_files(&names), |name| {
        name.strip_suffix(".json").unwrap_or(name)
    });
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("set by cargo"));
    fs::write(out.join("embedded.rs"), table).expect("OUT_DIR is writable");
}

/// The `.json` files directly in `dir` (not in its subfolders), by name, sorted. Cargo is told to
/// run this again when the folder or any of them changes.
fn json_files(dir: &Path) -> Vec<(String, PathBuf)> {
    // Cargo watches a directory's own entries, not what is in its files.
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if path.is_file() && name.ends_with(".json") {
            println!("cargo:rerun-if-changed={}", path.display());
            files.push((name.to_owned(), path.clone()));
        }
    }
    files.sort();
    files
}

/// `pub static <name>: &[(&str, &[u8])]`, keyed by `key(file name)`.
fn write_table(table: &mut String, name: &str, files: &[(String, PathBuf)], key: fn(&str) -> &str) {
    writeln!(table, "pub static {name}: &[(&str, &[u8])] = &[").expect("writing to a String");
    for (file, path) in files {
        writeln!(
            table,
            "    ({:?}, include_bytes!({:?})),",
            key(file),
            path.display().to_string()
        )
        .expect("writing to a String");
    }
    table.push_str("];\n");
}
