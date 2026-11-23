//! The icons, from the sheets `build.rs` packs out of `assets/icons/`: one set for every data
//! version, keyed by kind and game ID (ADR 0012).
//!
//! [`install`] decodes the sheets and uploads them before the first frame, so no icon is decoded
//! on first use and every icon is drawn from one texture (PLAN.md, M13 step 3). Without it, as in
//! the tests on data that is not the game's, there are no icons, and items show their initials.

use std::sync::Arc;

use aic_data::ImageKind;
use eframe::egui::{self, Id, Pos2, Rect, TextureHandle, TextureId, Vec2};

mod table {
    include!(concat!(env!("OUT_DIR"), "/icons.rs"));
}

/// Where an icon is drawn from: its sheet's texture and its rectangle there.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Icon {
    pub texture: TextureId,
    /// The icon's rectangle in the sheet, from 0 to 1.
    pub uv: Rect,
    /// The icon's own size in pixels, for its shape.
    pub size: Vec2,
}

/// The uploaded sheets, kept in egui's memory so any painter reaches them.
#[derive(Clone)]
struct Sheets(Arc<Vec<TextureHandle>>);

fn sheets_id() -> Id {
    Id::new("icon-sheets")
}

/// A sheet's pixels: RGBA, unmultiplied, row by row from the top. The desktop decodes them from
/// the PNG built into it; the web build's loading page decodes the AVIF it fetched
/// (`web::icon_sheets`).
pub struct SheetPixels {
    /// Width and height.
    pub size: [usize; 2],
    pub rgba: Vec<u8>,
}

/// Decodes the sheets built into the program and uploads them to `ctx`. Call once, before the
/// first frame. The web build has none built in: its page decodes them ([`install_sheets`]).
#[cfg(not(target_arch = "wasm32"))]
pub fn install(ctx: &egui::Context) {
    let sheets = table::SHEETS.iter().map(|png| decode_png(png)).collect();
    install_sheets(ctx, sheets).expect("the icon sheets built into the program fit");
}

/// A PNG built into the program, decoded.
#[cfg(not(target_arch = "wasm32"))]
fn decode_png(png: &[u8]) -> SheetPixels {
    let image = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .expect("the icon sheets built into the program decode")
        .to_rgba8();
    SheetPixels {
        size: [image.width() as usize, image.height() as usize],
        rgba: image.into_raw(),
    }
}

/// Uploads `sheets` (every one the build wrote, in its order) to `ctx`. Returns why they don't
/// fit, and installs none then: not as many as the build wrote, or one whose size isn't the one
/// the build packed or whose pixels don't fill it.
pub fn install_sheets(ctx: &egui::Context, sheets: Vec<SheetPixels>) -> Result<(), String> {
    if sheets.len() != table::SHEET_SIZES.len() {
        return Err(format!(
            "{} icon sheets, not {}",
            sheets.len(),
            table::SHEET_SIZES.len()
        ));
    }
    for (index, (sheet, &built)) in sheets.iter().zip(table::SHEET_SIZES).enumerate() {
        let [width, height] = sheet.size;
        if sheet.size != built {
            return Err(format!(
                "icon sheet {index} is {width} × {height} pixels, not {} × {}",
                built[0], built[1]
            ));
        }
        if sheet.rgba.len() != width * height * 4 {
            return Err(format!(
                "icon sheet {index} has {} bytes of pixels, not {}",
                sheet.rgba.len(),
                width * height * 4
            ));
        }
    }
    let mut textures = Vec::new();
    for (index, sheet) in sheets.into_iter().enumerate() {
        let image = egui::ColorImage::from_rgba_unmultiplied(sheet.size, &sheet.rgba);
        textures.push(ctx.load_texture(
            format!("icon-sheet-{index}"),
            image,
            egui::TextureOptions::LINEAR,
        ));
    }
    ctx.data_mut(|d| d.insert_temp(sheets_id(), Sheets(Arc::new(textures))));
    Ok(())
}

/// The rectangle and sheet of an icon of `kind` for `id`, if the program has one.
fn cell(kind: ImageKind, id: &str) -> Option<(u8, [u16; 4])> {
    let folder = kind.folder();
    let at = table::CELLS
        .binary_search_by(|&(f, i, _, _)| (f, i).cmp(&(folder, id)))
        .ok()?;
    let (_, _, sheet, rect) = table::CELLS[at];
    Some((sheet, rect))
}

/// Whether the program has an icon of `kind` for `id`, as `Catalog::recipe_image` asks.
pub fn has(kind: ImageKind, id: &str) -> bool {
    cell(kind, id).is_some()
}

/// An icon, ready to draw, if the program has it and its sheets are installed in `ctx`.
pub fn get(ctx: &egui::Context, kind: ImageKind, id: &str) -> Option<Icon> {
    let (sheet, [x, y, w, h]) = cell(kind, id)?;
    let sheets = ctx.data(|d| d.get_temp::<Sheets>(sheets_id()))?;
    let texture = sheets.0.get(usize::from(sheet))?;
    let [width, height] = texture.size().map(|side| side as f32);
    let (x, y, w, h) = (f32::from(x), f32::from(y), f32::from(w), f32::from(h));
    Some(Icon {
        texture: texture.id(),
        uv: Rect::from_min_max(
            Pos2::new(x / width, y / height),
            Pos2::new((x + w) / width, (y + h) / height),
        ),
        size: Vec2::new(w, h),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_are_found_by_kind_and_id() {
        assert!(has(ImageKind::Node, "source"));
        assert!(has(ImageKind::Facility, "furnance_1"));
        assert!(has(ImageKind::Item, "copper_nugget"));
        // Kinds don't mix.
        assert!(!has(ImageKind::Facility, "copper_nugget"));
        assert!(!has(ImageKind::Item, "no_such_item"));
    }

    #[test]
    fn icons_no_data_version_uses_are_left_out() {
        // A jambochen-only item, and a facility no data has (assets/icons/README.md).
        assert!(!has(ImageKind::Item, "fbottle_copperenr_acid"));
        assert!(!has(ImageKind::Facility, "vaporizer_1"));
    }

    #[test]
    fn the_table_is_sorted_for_its_search() {
        assert!(
            table::CELLS.is_sorted_by_key(|&(folder, id, _, _)| (folder, id)),
            "build.rs sorts by folder, then ID"
        );
    }

    /// The built-in sheets, decoded, as the app is handed them.
    fn built_in() -> Vec<SheetPixels> {
        table::SHEETS.iter().map(|png| decode_png(png)).collect()
    }

    #[test]
    fn sheets_of_the_built_size_are_installed() {
        let ctx = egui::Context::default();
        install_sheets(&ctx, built_in()).expect("the built sheets fit");
        assert!(get(&ctx, ImageKind::Item, "copper_nugget").is_some());
    }

    #[test]
    fn another_count_of_sheets_is_refused() {
        let ctx = egui::Context::default();
        let error = install_sheets(&ctx, Vec::new()).unwrap_err();
        assert!(error.contains("icon sheets"), "{error}");
        let mut more = built_in();
        more.extend(built_in());
        assert!(install_sheets(&ctx, more).is_err());
        assert!(get(&ctx, ImageKind::Item, "copper_nugget").is_none());
    }

    #[test]
    fn a_sheet_of_another_size_is_refused() {
        let ctx = egui::Context::default();
        let mut sheets = built_in();
        let [width, height] = sheets[0].size;
        sheets[0] = SheetPixels {
            size: [width - 1, height],
            rgba: vec![0; (width - 1) * height * 4],
        };
        let error = install_sheets(&ctx, sheets).unwrap_err();
        assert!(error.contains("icon sheet 0"), "{error}");
        assert!(get(&ctx, ImageKind::Item, "copper_nugget").is_none());
    }

    #[test]
    fn pixels_short_of_the_sheet_are_refused() {
        let ctx = egui::Context::default();
        let mut sheets = built_in();
        let short = sheets[0].rgba.len() - 4;
        sheets[0].rgba.truncate(short);
        let error = install_sheets(&ctx, sheets).unwrap_err();
        assert!(error.contains("icon sheet 0"), "{error}");
    }

    #[test]
    fn every_icon_has_a_clear_gutter_and_no_sheet_is_over_2048_pixels() {
        let ctx = egui::Context::default();
        install(&ctx);
        let sheets: Vec<_> = table::SHEETS
            .iter()
            .map(|png| image::load_from_memory(png).unwrap().to_rgba8())
            .collect();
        assert!(
            sheets
                .iter()
                .all(|s| s.width() <= 2048 && s.height() <= 2048)
        );
        for &(folder, id, sheet, [x, y, w, h]) in table::CELLS {
            let sheet = &sheets[usize::from(sheet)];
            let (x, y, w, h) = (u32::from(x), u32::from(y), u32::from(w), u32::from(h));
            // The two pixels round the icon are clear.
            for gx in x - 2..x + w + 2 {
                for gy in y - 2..y + h + 2 {
                    let inside = (x..x + w).contains(&gx) && (y..y + h).contains(&gy);
                    if !inside && gx < sheet.width() && gy < sheet.height() {
                        assert_eq!(sheet.get_pixel(gx, gy)[3], 0, "{folder}/{id} at {gx},{gy}");
                    }
                }
            }
        }
        let icon = get(&ctx, ImageKind::Node, "source").expect("installed");
        assert!(icon.uv.min.x >= 0.0 && icon.uv.max.y <= 1.0 && icon.size.x > 0.0);
        // The icons of one sheet share its texture.
        let other = get(&ctx, ImageKind::Node, "sink").unwrap();
        assert_eq!(icon.texture, other.texture);
    }
}
