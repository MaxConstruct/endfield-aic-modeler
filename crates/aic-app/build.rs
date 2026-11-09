//! Builds the game data into the program: every file of every version folder under the repository's
//! `data/` goes into `$OUT_DIR/embedded_data.rs` as a table of `include_bytes!`, which
//! `src/embedded.rs` hands to `aic_data::Bundle`. A missing `data/` gives an empty table.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::{env, fs};

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("set by cargo"));
    // crates/aic-app → the repository root.
    let data = manifest
        .ancestors()
        .nth(2)
        .expect("aic-app sits two folders below the repository root")
        .join("data");
    println!("cargo:rerun-if-changed={}", data.display());

    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(&data) {
        for entry in entries.flatten() {
            let path = entry.path();
            // Only version folders; `data/README.md` is for people.
            if path.is_dir() {
                collect(&data, &path, &mut files);
            }
        }
    }
    files.sort();

    let mut table = String::from("pub static FILES: &[(&str, &[u8])] = &[\n");
    for (key, path) in &files {
        writeln!(
            table,
            "    ({key:?}, include_bytes!({:?})),",
            path.display().to_string()
        )
        .expect("writing to a String");
    }
    table.push_str("];\n");
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("set by cargo"));
    fs::write(out.join("embedded_data.rs"), table).expect("OUT_DIR is writable");
}

/// Every file below `dir` but notes (`*.md`), keyed by its `/`-separated path under `root`.
fn collect(root: &Path, dir: &Path, files: &mut Vec<(String, PathBuf)>) {
    // Cargo watches a directory's own entries, not its subfolders' files.
    println!("cargo:rerun-if-changed={}", dir.display());
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(root, &path, files);
            continue;
        }
        if path.extension().is_some_and(|ext| ext == "md") {
            continue;
        }
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let parts: Option<Vec<&str>> = relative
            .components()
            .map(|c| c.as_os_str().to_str())
            .collect();
        let Some(parts) = parts else {
            continue;
        };
        println!("cargo:rerun-if-changed={}", path.display());
        files.push((parts.join("/"), path.clone()));
    }
}
