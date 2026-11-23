//! Packs the icons into one sheet (ADR 0012, PLAN.md M13 step 3).
//!
//! Every `assets/icons/{items,facilities,nodes}/<id>.png` that some data version in the
//! repository's `data/` uses goes into a grid of cells: each icon at its own size in the top-left
//! of a cell, with a clear gutter all round, so an icon drawn smaller than it is never takes a
//! little of its neighbour. No sheet is over `MAX_SIDE` pixels on a side, which is all WebGL2
//! promises; more icons than fit make a second sheet. Icons no data version uses stay in the
//! folder and out of the sheet. The node pictures (`nodes/`) are always in it.
//!
//! Writes `$OUT_DIR/icons-<n>.png` and `$OUT_DIR/icons.rs`, which `src/icons.rs` reads: the
//! sheets' bytes (on the desktop; the web build fetches the PNGs, which `scripts/web-assets.sh`
//! copies out of here), and a table from kind and ID to sheet and rectangle, sorted for a binary
//! search.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::{env, fs};

use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{GenericImage, RgbaImage};

/// The folders under `assets/icons/`, as `aic_data::ImageKind::folder` names them.
const ITEMS: &str = "items";
const FACILITIES: &str = "facilities";
const NODES: &str = "nodes";

/// The largest icon's side.
const ICON: u32 = 128;
/// Clear pixels on each side of an icon.
const GUTTER: u32 = 2;
/// A cell: an icon and its gutter on both sides.
const CELL: u32 = ICON + 2 * GUTTER;
/// The largest side WebGL2 promises.
const MAX_SIDE: u32 = 2048;

struct Icon {
    folder: &'static str,
    id: String,
    image: RgbaImage,
}

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("set by cargo"));
    exe_icon(&manifest);
    let data = manifest
        .ancestors()
        .nth(2)
        .expect("aic-app sits two folders below the repository root")
        .join("data");
    let icons = manifest.join("assets").join("icons");
    let used = used_ids(&data);

    let mut found = Vec::new();
    for folder in [FACILITIES, ITEMS, NODES] {
        let dir = icons.join(folder);
        watch(&dir);
        for entry in fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            let Some(id) = path
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.strip_suffix(".png"))
            else {
                continue;
            };
            let wanted = match folder {
                ITEMS => used.items.contains(id),
                FACILITIES => used.facilities.contains(id),
                _ => true,
            };
            if !wanted {
                continue;
            }
            watch(&path);
            let image = image::open(&path)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
                .to_rgba8();
            assert!(
                image.width() <= ICON && image.height() <= ICON,
                "{} is over {ICON} pixels",
                path.display()
            );
            found.push(Icon {
                folder,
                id: id.to_owned(),
                image,
            });
        }
    }
    found.sort_by(|a, b| (a.folder, &a.id).cmp(&(b.folder, &b.id)));

    let out = PathBuf::from(env::var_os("OUT_DIR").expect("set by cargo"));
    let (sheets, cells) = pack(&found);
    // The web build fetches the sheets beside the page (scripts/web-assets.sh copies them from
    // here), so only the desktop builds them in.
    let web = env::var("CARGO_CFG_TARGET_ARCH").is_ok_and(|arch| arch == "wasm32");
    let mut table = format!("pub const SHEET_COUNT: usize = {};\n", sheets.len());
    table.push_str("#[allow(dead_code)]\npub static SHEETS: &[&[u8]] = &[\n");
    for (index, sheet) in sheets.iter().enumerate() {
        let file = out.join(format!("icons-{index}.png"));
        write_png(sheet, &file);
        if !web {
            writeln!(
                table,
                "    include_bytes!({:?}),",
                file.display().to_string()
            )
            .expect("String");
        }
    }
    table.push_str("];\n");
    table.push_str("/// Folder, ID, sheet, and the icon's rectangle in it: x, y, width, height.\n");
    table.push_str("pub static CELLS: &[(&str, &str, u8, [u16; 4])] = &[\n");
    for (icon, (sheet, x, y)) in found.iter().zip(&cells) {
        writeln!(
            table,
            "    ({:?}, {:?}, {sheet}, [{x}, {y}, {}, {}]),",
            icon.folder,
            icon.id,
            icon.image.width(),
            icon.image.height()
        )
        .expect("String");
    }
    table.push_str("];\n");
    fs::write(out.join("icons.rs"), table).expect("OUT_DIR is writable");
}

/// Puts the app icon into a Windows .exe as its first icon resource, which Explorer, the Start
/// menu and a pinned taskbar button show. The window's own icon is set at run time
/// (`native_options`). Needs the Windows SDK's `rc.exe`, which the MSVC toolchain brings.
fn exe_icon(manifest: &Path) {
    let ico = manifest.join("assets").join("app-icon").join("AppIcon.ico");
    watch(&ico);
    #[cfg(windows)]
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon(&ico.to_string_lossy())
            .compile()
            .expect("the app icon compiles into a Windows resource");
    }
}

/// Writes `sheet` as a PNG at the encoder's best compression: the web build downloads it.
fn write_png(sheet: &RgbaImage, file: &Path) {
    let out = std::io::BufWriter::new(fs::File::create(file).expect("OUT_DIR is writable"));
    let encoder = PngEncoder::new_with_quality(out, CompressionType::Best, FilterType::Adaptive);
    sheet
        .write_with_encoder(encoder)
        .expect("an RGBA sheet encodes as PNG");
}

/// Places the icons in cells, row by row, as many sheets as it takes. Returns the sheets and, for
/// each icon, its sheet and the top-left of its pixels.
fn pack(icons: &[Icon]) -> (Vec<RgbaImage>, Vec<(usize, u32, u32)>) {
    let columns = MAX_SIDE / CELL;
    let per_sheet = (columns * (MAX_SIDE / CELL)) as usize;
    let mut sheets = Vec::new();
    let mut cells = Vec::with_capacity(icons.len());
    for (sheet, chunk) in icons.chunks(per_sheet).enumerate() {
        let rows = (chunk.len() as u32).div_ceil(columns);
        let width = columns.min(chunk.len() as u32) * CELL;
        let mut image = RgbaImage::new(width, rows * CELL);
        for (i, icon) in chunk.iter().enumerate() {
            let (column, row) = (i as u32 % columns, i as u32 / columns);
            let (x, y) = (column * CELL + GUTTER, row * CELL + GUTTER);
            image
                .copy_from(&icon.image, x, y)
                .expect("a cell holds its icon");
            cells.push((sheet, x, y));
        }
        sheets.push(image);
    }
    (sheets, cells)
}

/// The IDs some data version draws an icon for: its items (facilities are items too) and
/// recipes' icons as items, its facilities as facilities.
#[derive(Default)]
struct Used {
    items: BTreeSet<String>,
    facilities: BTreeSet<String>,
}

fn used_ids(data: &Path) -> Used {
    watch(data);
    let mut used = Used::default();
    let versions = aic_data::list_versions(data).unwrap_or_default();
    let names = aic_data::Names {
        file: "<no names>",
        json: b"{}",
    };
    // `data/` itself is watched, which catches an overlay added or removed.
    for version in versions {
        watch(&version.path);
        let overlay = data.join(format!("{}_overlay.json", version.name));
        if overlay.is_file() {
            watch(&overlay);
        }
        let catalog = match aic_data::load(&version.path, names) {
            Ok(catalog) => catalog,
            Err(error) => {
                println!(
                    "cargo:warning=icons: data version {} left out: {error}",
                    version.name
                );
                continue;
            }
        };
        used.items
            .extend(catalog.items().keys().map(|id| id.to_string()));
        used.items
            .extend(catalog.recipes().values().map(|recipe| recipe.icon.clone()));
        used.facilities
            .extend(catalog.facilities().keys().map(|id| id.to_string()));
    }
    used
}

fn watch(path: &Path) {
    println!("cargo:rerun-if-changed={}", path.display());
}
