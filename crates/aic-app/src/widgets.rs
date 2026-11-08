//! Small drawing helpers shared by the canvas, the picker and the panels.

use aic_data::Catalog;
use eframe::egui::{self, Color32, FontId, Id, Painter, Rect, Ui, Vec2};

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

/// Stands in for an item icon until icons load (M5): a colored square with the item's initials.
pub fn paint_item_badge(painter: &Painter, catalog: &Catalog, item: &str, rect: Rect) {
    // egui-snarl lays a new node out once with unknown sizes, which can leave rects undefined.
    if !rect.is_finite() {
        return;
    }
    let fill = item_color(catalog, item);
    painter.rect_filled(rect, 3.0, fill);
    let text_color = if fill.intensity() > 0.55 {
        Color32::BLACK
    } else {
        Color32::WHITE
    };
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        initials(names::item(catalog, item)),
        FontId::proportional(rect.height() * 0.5),
        text_color,
    );
}

/// An item badge laid out as a widget, with the item's name on hover.
pub fn item_badge(ui: &mut Ui, catalog: &Catalog, item: &str, size: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    paint_item_badge(ui.painter(), catalog, item, rect);
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
