//! File > Export PNG: the canvas as the player sees it, cropped out of a window screenshot.

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui::{self, ColorImage, Rect};

use crate::editor::Editor;
use crate::files::Download;
use crate::i18n::Strings;

/// Tags our screenshot request, so a reply meant for someone else is left alone.
#[derive(Debug, Clone)]
struct CanvasExport(PathBuf);

impl Editor {
    /// Asks where to save, then takes a screenshot of the window; the next frames crop and save it.
    /// The browser downloads it instead.
    pub fn export_png(&mut self, strings: &Strings) {
        let name = format!("{}.png", strings.get("file-untitled"));
        if self.session.is_browser() {
            self.request_export(PathBuf::from(name));
        } else if let Some(path) = pick_png_path(name) {
            self.request_export(path);
        }
    }

    /// Saves the canvas to `path` once the next frame is drawn.
    pub fn request_export(&mut self, path: PathBuf) {
        self.export_to = Some(path);
    }

    /// Saves the canvas from a screenshot that came back this frame.
    pub(crate) fn finish_export(&mut self, ctx: &egui::Context, strings: &Strings) {
        if let Some(path) = self.export_to.take() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(
                CanvasExport(path),
            )));
        }
        let shots: Vec<(PathBuf, Arc<ColorImage>)> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|event| match event {
                    egui::Event::Screenshot {
                        user_data, image, ..
                    } => {
                        let data = user_data.data.as_ref()?;
                        let export = data.downcast_ref::<CanvasExport>()?;
                        Some((export.0.clone(), image.clone()))
                    }
                    _ => None,
                })
                .collect()
        });
        let pixels_per_point = ctx.pixels_per_point();
        for (path, image) in shots {
            let browser = self.session.is_browser();
            let file = self.session.file_label(&path);
            let result = canvas_png(&image, self.canvas.rect, pixels_per_point).and_then(|png| {
                if browser {
                    self.downloads.push(Download {
                        name: file.clone(),
                        bytes: png,
                    });
                    Ok(())
                } else {
                    std::fs::write(&path, png).map_err(|e| e.to_string())
                }
            });
            let done = if browser {
                "export-downloaded"
            } else {
                "export-done"
            };
            self.session.set_status(Some(match result {
                Ok(()) => strings.format(done, &[("file", &file)]),
                Err(error) => {
                    strings.format("export-failed", &[("file", &file), ("error", &error)])
                }
            }));
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn pick_png_path(name: String) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("PNG", &["png"])
        .set_file_name(name)
        .save_file()
}

/// The web build has no disk.
#[cfg(target_arch = "wasm32")]
fn pick_png_path(_name: String) -> Option<PathBuf> {
    None
}

/// The part of a window screenshot under `canvas` (in points), as PNG bytes.
pub fn canvas_png(
    screenshot: &ColorImage,
    canvas: Rect,
    pixels_per_point: f32,
) -> Result<Vec<u8>, String> {
    let [width, height] = screenshot.size;
    let window = Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(width as f32, height as f32) / pixels_per_point,
    );
    let area = canvas.intersect(window);
    if !area.is_positive() {
        return Err("the canvas is not on screen".to_owned());
    }
    let cropped = screenshot.region(&area, Some(pixels_per_point));
    let [w, h] = cropped.size;
    let rgba: Vec<u8> = cropped.pixels.iter().flat_map(|c| c.to_array()).collect();
    let buffer = image::RgbaImage::from_raw(w as u32, h as u32, rgba)
        .ok_or_else(|| "the screenshot has the wrong size".to_owned())?;
    let mut png = Vec::new();
    buffer
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(png)
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Color32, pos2};

    #[test]
    fn the_png_is_the_canvas_part_of_the_window_in_pixels() {
        // A 20×10 point window at 2 pixels per point; the canvas is its right half.
        let mut shot = ColorImage::filled([40, 20], Color32::BLACK);
        for y in 0..20 {
            for x in 20..40 {
                shot.pixels[y * 40 + x] = Color32::RED;
            }
        }
        let canvas = Rect::from_min_max(pos2(10.0, 0.0), pos2(20.0, 10.0));
        let png = canvas_png(&shot, canvas, 2.0).unwrap();

        let decoded = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(decoded.dimensions(), (20, 20));
        assert!(decoded.pixels().all(|p| p.0 == [255, 0, 0, 255]));
    }

    #[test]
    fn a_canvas_off_screen_is_an_error() {
        let shot = ColorImage::filled([10, 10], Color32::BLACK);
        let canvas = Rect::from_min_max(pos2(50.0, 50.0), pos2(60.0, 60.0));
        assert!(canvas_png(&shot, canvas, 1.0).is_err());
    }
}
