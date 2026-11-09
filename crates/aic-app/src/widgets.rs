//! Small drawing helpers shared by the canvas, the picker and the panels.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use aic_data::{Catalog, ImageKind};
use eframe::egui::{self, Color32, FontId, Id, Painter, Pos2, Rect, TextureHandle, Ui, Vec2};

use crate::format;
use crate::names;
use crate::settings::parse_amount;

/// Colors with a meaning across the app.
pub mod colors {
    use eframe::egui::Color32;

    /// An input port pulling from the depot.
    pub const DEPOT_IN: Color32 = Color32::from_rgb(236, 120, 170);
    /// An output port sending to the depot.
    pub const DEPOT_OUT: Color32 = Color32::from_rgb(110, 200, 120);
    /// A linked port.
    pub const LINKED: Color32 = Color32::from_rgb(150, 160, 175);
    /// Throttle markers and hints such as a link over one belt.
    pub const WARN: Color32 = Color32::from_rgb(240, 170, 60);
    pub const POSITIVE: Color32 = Color32::from_rgb(110, 200, 120);
    pub const NEGATIVE: Color32 = Color32::from_rgb(230, 90, 90);
}

/// A field for typing a limit. It shows `current` unless the player is typing in it, and returns
/// the typed limit (`None` for blank) on the frame the text changes to something that parses. Text
/// that doesn't parse shows red and changes nothing.
pub struct LimitField<'a> {
    pub id: Id,
    pub buffer: &'a mut String,
    pub current: Option<f64>,
    pub width: f32,
    pub hint: String,
    /// The name a screen reader (and a test) finds the field by.
    pub label: String,
}

impl LimitField<'_> {
    pub fn show(self, ui: &mut Ui) -> (egui::Response, Option<Option<f64>>) {
        let typing = ui.memory(|m| m.has_focus(self.id));
        if !typing {
            *self.buffer = self.current.map(format::editable).unwrap_or_default();
        }
        let valid = parse_amount(self.buffer).is_ok();
        let mut edit = egui::TextEdit::singleline(self.buffer)
            .id(self.id)
            .desired_width(self.width)
            .horizontal_align(egui::Align::Center)
            .hint_text(self.hint);
        if !valid {
            edit = edit.text_color(colors::NEGATIVE);
        }
        let response = ui.add(edit);
        let label = self.label;
        ui.ctx()
            .accesskit_node_builder(response.id, |node| node.set_label(label));
        let typed = response
            .changed()
            .then(|| parse_amount(self.buffer).ok())
            .flatten();
        (response, typed)
    }
}

/// The image of a source node, under `images/nodes/` (the game's Depot Unloader).
pub const SOURCE_IMAGE: &str = "source";
/// The image of a sink node, under `images/nodes/` (the game's Depot Loader).
pub const SINK_IMAGE: &str = "sink";

/// An image as an egui widget part, for buttons that show a picture next to their text.
pub fn image_atom(
    ctx: &egui::Context,
    catalog: &Catalog,
    kind: ImageKind,
    id: &str,
    size: f32,
) -> Option<egui::Image<'static>> {
    let texture = icon_texture(ctx, catalog, kind, id)?;
    Some(egui::Image::new((texture.id(), texture.size_vec2())).fit_to_exact_size(Vec2::splat(size)))
}

/// Decoded icons, kept in egui's memory so any painter can reach them. Each image is decoded the
/// first time it is drawn; `None` remembers that it has no usable image.
#[derive(Clone, Default)]
struct IconCache(Arc<Mutex<Textures>>);

type Textures = HashMap<(ImageKind, String), Option<TextureHandle>>;

/// Drops every decoded icon, after a switch to other data.
pub fn forget_icons(ctx: &egui::Context) {
    ctx.data_mut(|d| d.remove::<IconCache>(Id::new("icon-cache")));
}

/// The texture for an image of the data version, decoded on first use.
pub fn icon_texture(
    ctx: &egui::Context,
    catalog: &Catalog,
    kind: ImageKind,
    id: &str,
) -> Option<TextureHandle> {
    let cache = ctx.data_mut(|d| {
        d.get_temp_mut_or_default::<IconCache>(Id::new("icon-cache"))
            .clone()
    });
    let key = (kind, id.to_owned());
    if let Some(found) = cache.0.lock().expect("icon cache lock").get(&key) {
        return found.clone();
    }
    let texture = catalog.image(kind, id).and_then(decode).map(|image| {
        ctx.load_texture(
            format!("icon/{}/{id}", kind.folder()),
            image,
            egui::TextureOptions::LINEAR,
        )
    });
    cache
        .0
        .lock()
        .expect("icon cache lock")
        .insert(key, texture.clone());
    texture
}

fn decode(bytes: &[u8]) -> Option<egui::ColorImage> {
    let image = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
        .ok()?
        .to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    Some(egui::ColorImage::from_rgba_unmultiplied(
        size,
        image.as_raw(),
    ))
}

/// Paints an image of the data version into `rect`, keeping its shape. Returns false, painting
/// nothing, when there is no such image.
pub fn paint_image(
    painter: &Painter,
    catalog: &Catalog,
    kind: ImageKind,
    id: &str,
    rect: Rect,
) -> bool {
    if !rect.is_finite() {
        return false;
    }
    let Some(texture) = icon_texture(painter.ctx(), catalog, kind, id) else {
        return false;
    };
    let size = texture.size_vec2();
    let scale = (rect.width() / size.x).min(rect.height() / size.y);
    let fitted = Rect::from_center_size(rect.center(), size * scale);
    let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
    painter.image(texture.id(), fitted, uv, Color32::WHITE);
    true
}

/// An image laid out as a widget, or `None` when the data has no such image.
pub fn image(
    ui: &mut Ui,
    catalog: &Catalog,
    kind: ImageKind,
    id: &str,
    size: f32,
) -> Option<egui::Response> {
    catalog.image(kind, id)?;
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    paint_image(ui.painter(), catalog, kind, id, rect).then_some(response)
}

/// An item's icon, or a colored square with its initials when the data has no image for it.
pub fn paint_item(painter: &Painter, catalog: &Catalog, item: &str, rect: Rect) {
    ItemPicture::of(painter.ctx(), catalog, item).paint(painter, rect);
}

/// What an item looks like, ready to paint without the catalog: its icon, or a stand-in square.
#[derive(Clone)]
pub enum ItemPicture {
    Icon(TextureHandle),
    /// A colored square with the item's initials.
    Badge {
        fill: Color32,
        initials: String,
    },
}

impl ItemPicture {
    pub fn of(ctx: &egui::Context, catalog: &Catalog, item: &str) -> Self {
        match icon_texture(ctx, catalog, ImageKind::Item, item) {
            Some(texture) => Self::Icon(texture),
            None => Self::Badge {
                fill: item_color(catalog, item),
                initials: initials(names::item(catalog, item)),
            },
        }
    }

    /// Paints it into `rect`, keeping its shape.
    pub fn paint(&self, painter: &Painter, rect: Rect) {
        // egui-snarl lays a new node out once with unknown sizes, which can leave rects undefined.
        if !rect.is_finite() {
            return;
        }
        match self {
            Self::Icon(texture) => {
                let size = texture.size_vec2();
                let scale = (rect.width() / size.x).min(rect.height() / size.y);
                let fitted = Rect::from_center_size(rect.center(), size * scale);
                let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
                painter.image(texture.id(), fitted, uv, Color32::WHITE);
            }
            Self::Badge { fill, initials } => {
                painter.rect_filled(rect, 3.0, *fill);
                let text_color = if fill.intensity() > 0.55 {
                    Color32::BLACK
                } else {
                    Color32::WHITE
                };
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    initials,
                    FontId::proportional(rect.height() * 0.5),
                    text_color,
                );
            }
        }
    }
}

/// An item's icon laid out as a widget, with the item's name on hover.
pub fn item_icon(ui: &mut Ui, catalog: &Catalog, item: &str, size: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    paint_item(ui.painter(), catalog, item, rect);
    response.on_hover_text(names::item(catalog, item))
}

/// The item's icon color from the data, or a stable color derived from its ID.
fn item_color(catalog: &Catalog, item: &str) -> Color32 {
    catalog
        .icon(item)
        .and_then(|icon| icon.color.as_deref())
        .and_then(|hex| Color32::from_hex(hex).ok())
        .unwrap_or_else(|| {
            let hash = item.bytes().fold(2_166_136_261_u32, |h, b| {
                (h ^ u32::from(b)).wrapping_mul(16_777_619)
            });
            let hue = (hash % 360) as f32 / 360.0;
            egui::ecolor::Hsva::new(hue, 0.45, 0.7, 1.0).into()
        })
}

/// The first letters of the first two words: "Cuprium Ore" → "CO".
fn initials(name: &str) -> String {
    name.split_whitespace()
        .take(2)
        .filter_map(|word| word.chars().next())
        .flat_map(char::to_uppercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initials_take_two_words() {
        assert_eq!(initials("Cuprium Ore"), "CO");
        assert_eq!(initials("Yazhen"), "Y");
        assert_eq!(initials("LC Valley Battery"), "LV");
    }
}
