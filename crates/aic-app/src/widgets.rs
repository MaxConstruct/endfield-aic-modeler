//! Small drawing helpers shared by the canvas, the picker and the panels.

use std::sync::Arc;

use aic_data::{Catalog, ImageKind, Recipe, RegionId};
use eframe::egui::{self, Color32, FontId, Id, Painter, Pos2, Rect, Ui, Vec2};

use crate::icons::{self, Icon};
use crate::settings::parse_amount;
use aic_editor::format;
use aic_editor::names;
use aic_editor::picker::foreign_regions;

/// Colors with a meaning across the app.
pub use crate::theme::color as colors;

/// A limit as a [`LimitField`] shows it: up to four decimals, blank for none.
pub fn limit_text(limit: Option<f64>) -> String {
    limit.map(format::editable).unwrap_or_default()
}

/// A one-line text field, as every field in the app starts: as tall as a button
/// (`interact_size`), which the canvas scales with its text. Left to itself egui makes it as tall
/// as its text and a margin the canvas doesn't scale, so a field on a card would come out shorter.
pub fn text_field<'t>(ui: &Ui, text: &'t mut dyn egui::TextBuffer) -> egui::TextEdit<'t> {
    egui::TextEdit::singleline(text)
        .min_size(Vec2::new(0.0, ui.spacing().interact_size.y))
        .vertical_align(egui::Align::Center)
}

/// A field for typing a limit. It shows `current` unless the player is typing in it, and returns
/// the typed limit (`None` for blank) on the frame the text changes to something that parses. Text
/// that doesn't parse shows red and changes nothing.
pub struct LimitField<'a> {
    pub id: Id,
    pub buffer: &'a mut String,
    /// The limit as it is, written by [`limit_text`].
    pub current: &'a str,
    pub width: f32,
    pub hint: String,
    /// The name a screen reader (and a test) finds the field by.
    pub label: String,
}

impl LimitField<'_> {
    pub fn show(self, ui: &mut Ui) -> (egui::Response, Option<Option<f64>>) {
        let typing = ui.memory(|m| m.has_focus(self.id));
        // Copied only when it differs, so a card shown every frame doesn't allocate for it.
        if !typing && *self.buffer != self.current {
            self.current.clone_into(self.buffer);
        }
        let valid = parse_amount(self.buffer).is_ok();
        let mut edit = text_field(ui, self.buffer)
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

/// A top bar toggle: `icon`, a symbol egui's built-in fonts draw, in front of `name`, lit while
/// `on`. A screen reader (and a test) knows it by `name` alone.
pub fn icon_toggle(ui: &mut Ui, on: &mut bool, icon: &str, name: String) -> egui::Response {
    let response = ui.toggle_value(on, format!("{icon} {name}"));
    ui.ctx()
        .accesskit_node_builder(response.id, |node| node.set_label(name));
    response
}

/// The image of a source node, under `assets/icons/nodes/` (the game's Depot Unloader).
pub const SOURCE_IMAGE: &str = "source";
/// The image of a sink node, under `assets/icons/nodes/` (the game's Depot Loader).
pub const SINK_IMAGE: &str = "sink";
/// What stands for a workspace on its card and in the node picker: a symbol egui's built-in
/// fonts draw.
pub const WORKSPACE_GLYPH: &str = "🏭";

/// An image as an egui widget part, for buttons that show a picture next to their text.
pub fn image_atom(
    ctx: &egui::Context,
    kind: ImageKind,
    id: &str,
    size: f32,
) -> Option<egui::Image<'static>> {
    let icon = icons::get(ctx, kind, id)?;
    Some(
        egui::Image::new((icon.texture, icon.size))
            .uv(icon.uv)
            .fit_to_exact_size(Vec2::splat(size)),
    )
}

/// Paints `icon` into `rect`, keeping its shape.
fn paint_icon(painter: &Painter, icon: &Icon, rect: Rect) {
    let scale = (rect.width() / icon.size.x).min(rect.height() / icon.size.y);
    let fitted = Rect::from_center_size(rect.center(), icon.size * scale);
    painter.image(icon.texture, fitted, icon.uv, Color32::WHITE);
}

/// An icon laid out as a widget, or `None` when the program has no such icon.
pub fn image(ui: &mut Ui, kind: ImageKind, id: &str, size: f32) -> Option<egui::Response> {
    let icon = icons::get(ui.ctx(), kind, id)?;
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    if rect.is_finite() {
        paint_icon(ui.painter(), &icon, rect);
    }
    Some(response)
}

/// An item's icon, or a colored square with its initials when the program has no icon for it.
pub fn paint_item(painter: &Painter, catalog: &Catalog, item: &str, rect: Rect) {
    ItemPicture::of(painter.ctx(), catalog, item).paint(painter, rect);
}

/// What an item looks like, ready to paint without the catalog: its icon, or a stand-in square.
#[derive(Clone)]
pub enum ItemPicture {
    Icon(Icon),
    /// A colored square with the item's initials.
    Badge {
        fill: Color32,
        initials: String,
    },
}

impl ItemPicture {
    pub fn of(ctx: &egui::Context, catalog: &Catalog, item: &str) -> Self {
        match icons::get(ctx, ImageKind::Item, item) {
            Some(icon) => Self::Icon(icon),
            None => Self::Badge {
                fill: item_color(catalog, item),
                initials: initials(names::item(catalog, item)),
            },
        }
    }

    /// A port with no item yet: a grey square with a question mark.
    pub fn unknown() -> Self {
        Self::Badge {
            fill: colors::CONTROL_ACTIVE,
            initials: "?".to_owned(),
        }
    }

    /// Paints it into `rect`, keeping its shape.
    pub fn paint(&self, painter: &Painter, rect: Rect) {
        // A card not yet laid out can hand over a rect that isn't one.
        if !rect.is_finite() {
            return;
        }
        match self {
            Self::Icon(icon) => paint_icon(painter, icon, rect),
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
        .item(item)
        .and_then(|item| item.color.as_deref())
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

/// The least space inside a region badge around its letter, and between two badges, at 100%.
const BADGE_PADDING: Vec2 = Vec2::new(3.0, 1.0);
const BADGE_GAP: f32 = 4.0;

/// The badges of a recipe the graph's region lacks, laid out: a small square for each region that
/// has it, with the first letter of its name, `W` or `V`. Empty when the graph's region has the
/// recipe. The whole name is for hover text (`names::recipe_regions`). `scale` is how big 100% is
/// where they are drawn (the canvas lays out at `VIEW_SCALE`); the text size comes from the `Ui`'s
/// small style.
pub struct RegionBadges {
    letters: Vec<(
        Arc<egui::Galley>,
        &'static crate::theme::color::region::Tint,
    )>,
    scale: f32,
}

impl RegionBadges {
    pub fn new(
        ui: &Ui,
        catalog: &Catalog,
        graph_region: &RegionId,
        recipe: &Recipe,
        scale: f32,
    ) -> Self {
        let font = FontId::new(
            egui::TextStyle::Small.resolve(ui.style()).size,
            crate::theme::semibold(),
        );
        let tints = &colors::region::TINTS;
        let letters = foreign_regions(graph_region, recipe)
            .iter()
            .map(|region| {
                let index = catalog.regions().get_index_of(region).unwrap_or(0);
                let tint = &tints[index % tints.len()];
                let letter = names::region(catalog, region.as_str())
                    .chars()
                    .next()
                    .map(String::from)
                    .unwrap_or_default();
                let galley = ui.painter().layout_no_wrap(letter, font.clone(), tint.text);
                (galley, tint)
            })
            .collect();
        Self { letters, scale }
    }

    pub fn is_empty(&self) -> bool {
        self.letters.is_empty()
    }

    /// A square, wider only for a letter that needs it.
    fn badge_size(&self, letter: &egui::Galley) -> Vec2 {
        let size = letter.size() + 2.0 * BADGE_PADDING * self.scale;
        Vec2::new(size.x.max(size.y), size.y)
    }

    /// The space all of them take, side by side.
    pub fn size(&self) -> Vec2 {
        let gaps = self.letters.len().saturating_sub(1) as f32 * BADGE_GAP * self.scale;
        self.letters
            .iter()
            .fold(Vec2::new(gaps, 0.0), |size, (letter, _)| {
                let badge = self.badge_size(letter);
                Vec2::new(size.x + badge.x, size.y.max(badge.y))
            })
    }

    /// Paints them left to right from `left_center`, centred on its height.
    pub fn paint(&self, painter: &Painter, left_center: Pos2) {
        let mut x = left_center.x;
        for (letter, tint) in &self.letters {
            let size = self.badge_size(letter);
            let rect = Rect::from_min_size(Pos2::new(x, left_center.y - size.y / 2.0), size);
            painter.rect(
                rect,
                f32::from(crate::theme::radius::SMALL) * self.scale,
                tint.fill,
                egui::Stroke::new(self.scale, tint.stroke),
                egui::StrokeKind::Inside,
            );
            painter.galley(
                rect.center() - letter.size() / 2.0,
                letter.clone(),
                tint.text,
            );
            x = rect.right() + BADGE_GAP * self.scale;
        }
    }

    /// Lays them out as one widget.
    pub fn ui(&self, ui: &mut Ui) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(self.size(), egui::Sense::hover());
        if ui.is_rect_visible(rect) {
            self.paint(ui.painter(), rect.left_center());
        }
        response
    }
}

/// A warning sign: an amber triangle with a dark `!`, `size` points wide.
pub fn warning_icon(ui: &mut Ui, size: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        paint_warning(ui.painter(), rect);
    }
    response
}

/// Paints a warning sign filling `rect`'s width.
pub fn paint_warning(painter: &Painter, rect: Rect) {
    let side = rect.width();
    let height = side * 0.87;
    let top = rect.center().y - height / 2.0;
    let points = vec![
        Pos2::new(rect.center().x, top),
        Pos2::new(rect.right(), top + height),
        Pos2::new(rect.left(), top + height),
    ];
    painter.add(egui::Shape::convex_polygon(
        points,
        colors::WARN,
        egui::Stroke::NONE,
    ));
    let mark = egui::Stroke::new(side * 0.11, colors::CANVAS);
    let x = rect.center().x;
    painter.line_segment(
        [
            Pos2::new(x, top + height * 0.36),
            Pos2::new(x, top + height * 0.66),
        ],
        mark,
    );
    painter.circle_filled(
        Pos2::new(x, top + height * 0.82),
        side * 0.06,
        colors::CANVAS,
    );
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
