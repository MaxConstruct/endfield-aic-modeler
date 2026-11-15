//! The node picker: one popup for every way of adding a node, modeled on Satisfactory Modeler's
//! recipe selector. A search box, the built-in nodes on the left, recipes on the right. One click
//! adds the node.

use std::sync::atomic::{AtomicU64, Ordering};

use aic_data::{Catalog, ImageKind, Recipe};
use eframe::egui::{self, Align2, Key, Pos2, Rect, Sense, Ui, Vec2, WidgetInfo, WidgetType};

pub use aic_editor::picker::{Builtin, Choice, Matching, Origin, builtin_kind, builtins, recipes};

use crate::i18n::Strings;
use crate::widgets::{self, paint_item};
use aic_editor::names;

/// An open picker.
pub struct Picker {
    pub origin: Origin,
    /// Top-left of the popup, in screen space.
    pub screen_pos: Pos2,
    /// Where the new node goes, in graph space.
    pub graph_pos: Pos2,
    query: String,
    matching: Matching,
    just_opened: bool,
    /// The popup's egui area. Each opening gets its own, so nothing egui remembers about an
    /// earlier one, such as a size squeezed by a window that was resized, carries over.
    area: egui::Id,
}

pub enum Outcome {
    Open,
    Close,
    Chosen(Choice),
}

/// Counts openings, to give each its own area.
static OPENINGS: AtomicU64 = AtomicU64::new(0);

const WIDTH: f32 = 560.0;
const HEIGHT: f32 = 420.0;
const LEFT_WIDTH: f32 = 160.0;
const ROW_HEIGHT: f32 = 32.0;
const BADGE: f32 = 32.0;
/// Between a recipe's region badges and its name.
const REGION_GAP: f32 = 6.0;

impl Picker {
    pub fn new(origin: Origin, screen_pos: Pos2, graph_pos: Pos2) -> Self {
        Self {
            origin,
            screen_pos,
            graph_pos,
            query: String::new(),
            matching: Matching::default(),
            just_opened: true,
            area: egui::Id::new(("node-picker", OPENINGS.fetch_add(1, Ordering::Relaxed))),
        }
    }

    /// The popup's egui area.
    pub fn area_id(&self) -> egui::Id {
        self.area
    }

    pub fn show(&mut self, ctx: &egui::Context, catalog: &Catalog, strings: &Strings) -> Outcome {
        let mut outcome = Outcome::Open;
        let area = egui::Area::new(self.area)
            .order(egui::Order::Foreground)
            .fixed_pos(self.screen_pos)
            .constrain(true)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(WIDTH);
                    if let Some(choice) = self.contents(ui, catalog, strings) {
                        outcome = Outcome::Chosen(choice);
                    }
                });
            });
        let escape = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape));
        // The click that opened the picker lands outside it on its first frame.
        let clicked_away = !self.just_opened && area.response.clicked_elsewhere();
        self.just_opened = false;
        if matches!(outcome, Outcome::Open) && (escape || clicked_away) {
            outcome = Outcome::Close;
        }
        outcome
    }

    fn contents(&mut self, ui: &mut Ui, catalog: &Catalog, strings: &Strings) -> Option<Choice> {
        let search = ui.add(
            egui::TextEdit::singleline(&mut self.query)
                .hint_text(strings.get("picker-search-hint"))
                .desired_width(f32::INFINITY),
        );
        if self.just_opened {
            search.request_focus();
        }
        ui.horizontal(|ui| {
            ui.toggle_value(&mut self.matching.name, strings.get("picker-match-name"));
            ui.toggle_value(
                &mut self.matching.inputs,
                strings.get("picker-match-inputs"),
            );
            ui.toggle_value(
                &mut self.matching.outputs,
                strings.get("picker-match-outputs"),
            );
        });
        ui.separator();

        let recipes = recipes(catalog, &self.origin, &self.query, self.matching);
        let enter = search.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
        let mut chosen = enter
            .then(|| {
                recipes
                    .first()
                    .map(|recipe| Choice::Recipe(recipe.id.clone()))
            })
            .flatten();

        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(LEFT_WIDTH);
                for builtin in builtins(&self.origin) {
                    let label = self.builtin_label(catalog, strings, builtin);
                    let image = match builtin {
                        Builtin::Source => Some(widgets::SOURCE_IMAGE),
                        Builtin::Sink => Some(widgets::SINK_IMAGE),
                        _ => None,
                    }
                    .and_then(|id| {
                        widgets::image_atom(ui.ctx(), catalog, ImageKind::Node, id, BADGE)
                    });
                    let button = match image {
                        Some(image) => egui::Button::new((image, label)),
                        None => egui::Button::new(label),
                    }
                    .min_size(Vec2::new(LEFT_WIDTH, ROW_HEIGHT));
                    if ui.add(button).clicked() {
                        chosen = Some(Choice::Builtin(builtin));
                    }
                }
                if self.origin == Origin::Canvas {
                    let button = egui::Button::new(strings.get("kind-workspace"))
                        .min_size(Vec2::new(LEFT_WIDTH, ROW_HEIGHT));
                    if ui.add(button).clicked() {
                        chosen = Some(Choice::Workspace);
                    }
                }
            });
            ui.separator();
            ui.vertical(|ui| {
                egui::ScrollArea::vertical()
                    .max_height(HEIGHT)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        if recipes.is_empty() {
                            ui.weak(strings.get("picker-no-recipes"));
                        }
                        for recipe in &recipes {
                            if recipe_row(ui, catalog, recipe).clicked() {
                                chosen = Some(Choice::Recipe(recipe.id.clone()));
                            }
                        }
                    });
            });
        });
        chosen
    }

    fn builtin_label(&self, catalog: &Catalog, strings: &Strings, builtin: Builtin) -> String {
        let item = match &self.origin {
            Origin::Port { item, .. } => names::item(catalog, item.as_str()),
            Origin::Canvas => "",
        };
        match builtin {
            Builtin::Source => strings.format("picker-source-of", &[("item", item)]),
            Builtin::Sink => strings.format("picker-sink-of", &[("item", item)]),
            Builtin::Splitter => strings.get("kind-splitter"),
            Builtin::Merger => strings.get("kind-merger"),
            Builtin::Storage => strings.get("kind-storage"),
            Builtin::ControlPort => strings.get("kind-control-port"),
        }
    }
}

/// One recipe: input badges on the left, the name in the middle, output badges on the right.
fn recipe_row(ui: &mut Ui, catalog: &Catalog, recipe: &Recipe) -> egui::Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, ROW_HEIGHT), Sense::click());
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, &recipe.name));
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&response);
        let painter = ui.painter();
        painter.rect(
            rect.shrink(1.0),
            visuals.corner_radius,
            visuals.weak_bg_fill,
            visuals.bg_stroke,
            egui::StrokeKind::Inside,
        );
        let badge = |index: usize, from_right: bool| {
            let step = BADGE + 3.0;
            let x = if from_right {
                rect.right() - 6.0 - BADGE - index as f32 * step
            } else {
                rect.left() + 6.0 + index as f32 * step
            };
            Rect::from_min_size(
                Pos2::new(x, rect.center().y - BADGE / 2.0),
                Vec2::splat(BADGE),
            )
        };
        for (index, (item, _)) in recipe.inputs.iter().enumerate() {
            paint_item(painter, catalog, item.as_str(), badge(index, false));
        }
        for (index, (item, _)) in recipe.outputs.iter().rev().enumerate() {
            paint_item(painter, catalog, item.as_str(), badge(index, true));
        }
        let name = painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            &recipe.name,
            egui::TextStyle::Button.resolve(ui.style()),
            visuals.text_color(),
        );
        let badges = widgets::RegionBadges::new(ui, catalog, recipe, 1.0);
        if !badges.is_empty() {
            let left = name.left() - REGION_GAP - badges.size().x;
            badges.paint(painter, Pos2::new(left, rect.center().y));
        }
    }
    response
}
