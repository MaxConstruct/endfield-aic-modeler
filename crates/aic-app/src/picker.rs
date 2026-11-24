//! The node picker: one popup for every way of adding a node, modeled on Satisfactory Modeler's
//! recipe selector. A search box, the built-in nodes on the left, recipes on the right. One click
//! adds the node.

use std::sync::atomic::{AtomicU64, Ordering};

use aic_data::{Catalog, ImageKind, Phase, Recipe, RegionId};
use eframe::egui::{self, Key, Pos2, Rect, Sense, Ui, Vec2, WidgetInfo, WidgetType};

pub use aic_editor::picker::{Builtin, Choice, Matching, Origin, builtin_kind, builtins, recipes};

use crate::i18n::Strings;
use crate::theme;
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

const WIDTH: f32 = 620.0;
const HEIGHT: f32 = 420.0;
const LEFT_WIDTH: f32 = 160.0;
const ROW_HEIGHT: f32 = 32.0;
const BADGE: f32 = 32.0;
/// From one item badge to the next.
const BADGE_STEP: f32 = BADGE + 3.0;
/// At a row's ends, and between the name and what is beside it.
const ROW_GAP: f32 = 6.0;
/// The input badges every row keeps room for before its name, so that names line up.
const INPUT_COLUMNS: usize = 2;

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

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        catalog: &Catalog,
        strings: &Strings,
        graph_region: &RegionId,
    ) -> Outcome {
        let mut outcome = Outcome::Open;
        let area = egui::Area::new(self.area)
            .order(egui::Order::Foreground)
            .fixed_pos(self.screen_pos)
            .constrain(true)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(WIDTH);
                    if let Some(choice) = self.contents(ui, catalog, strings, graph_region) {
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

    fn contents(
        &mut self,
        ui: &mut Ui,
        catalog: &Catalog,
        strings: &Strings,
        graph_region: &RegionId,
    ) -> Option<Choice> {
        let search = ui.add(
            widgets::text_field(ui, &mut self.query)
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
                // What the node would carry, when the picker was opened from a port.
                let phase = match &self.origin {
                    Origin::Port { item, .. } => catalog.item(item.as_str()).map(|i| i.phase),
                    Origin::Canvas => None,
                };
                for builtin in builtins(&self.origin) {
                    let label = self.builtin_label(catalog, strings, builtin);
                    let image = builtin_image(builtin, phase);
                    let image = widgets::image_atom(ui.ctx(), ImageKind::Node, image, BADGE);
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
                    // The glyph of a workspace's card, painted in a badge's place so that the
                    // names line up and the glyph is no part of the button's name.
                    let glyph_id = egui::Id::new("picker-workspace-glyph");
                    let glyph = egui::Atom::custom(glyph_id, Vec2::splat(BADGE));
                    let button = egui::Button::new((glyph, strings.get("kind-workspace")))
                        .min_size(Vec2::new(LEFT_WIDTH, ROW_HEIGHT));
                    let laid_out = button.atom_ui(ui);
                    if let Some(rect) = laid_out.rect(glyph_id) {
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            widgets::WORKSPACE_GLYPH,
                            egui::FontId::proportional(BADGE * 0.7),
                            ui.style().interact(&laid_out.response).text_color(),
                        );
                    }
                    if laid_out.response.clicked() {
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
                            let row = recipe_row(ui, catalog, graph_region, recipe);
                            let row = row.on_hover_ui(|ui| {
                                ui.label(&recipe.name);
                                if let Some(regions) =
                                    names::recipe_regions(catalog, strings, recipe)
                                {
                                    ui.label(regions);
                                }
                            });
                            if row.clicked() {
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

/// The picture of a built-in node, in `assets/icons/nodes/`: the game's part for what the node
/// carries. A solid goes through the belt's splitter, converger and control port and is kept in a
/// Protocol Stash; a liquid or a gas goes through the pipe's and is kept in a Fluid Tank or a Gas
/// Tank. With nothing to carry yet (`None`), the belt's.
pub(crate) fn builtin_image(builtin: Builtin, carries: Option<Phase>) -> &'static str {
    let fluid = matches!(carries, Some(Phase::Liquid | Phase::Gas));
    match (builtin, fluid) {
        (Builtin::Source, _) => widgets::SOURCE_IMAGE,
        (Builtin::Sink, _) => widgets::SINK_IMAGE,
        (Builtin::Splitter, false) => "splitter",
        (Builtin::Splitter, true) => "splitter_pipe",
        (Builtin::Merger, false) => "merger",
        (Builtin::Merger, true) => "merger_pipe",
        (Builtin::ControlPort, false) => "control_port",
        (Builtin::ControlPort, true) => "control_port_pipe",
        (Builtin::Storage, _) => match carries {
            Some(Phase::Liquid) => "storage_liquid",
            Some(Phase::Gas) => "storage_gas",
            Some(Phase::Solid) | None => "storage",
        },
    }
}

/// One recipe: input badges on the left, then the name, cut short where it would reach the output
/// badges on the right. A recipe the graph's region lacks is muted, with region badges after its
/// name.
fn recipe_row(
    ui: &mut Ui,
    catalog: &Catalog,
    graph_region: &RegionId,
    recipe: &Recipe,
) -> egui::Response {
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
            let x = if from_right {
                rect.right() - ROW_GAP - BADGE - index as f32 * BADGE_STEP
            } else {
                rect.left() + ROW_GAP + index as f32 * BADGE_STEP
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
        let inputs = recipe.inputs.len().max(INPUT_COLUMNS) as f32;
        let left = rect.left() + ROW_GAP + inputs * BADGE_STEP + ROW_GAP;
        let outputs = recipe.outputs.len() as f32;
        let mut right = rect.right() - ROW_GAP - outputs * BADGE_STEP - ROW_GAP;
        let badges = widgets::RegionBadges::new(ui, catalog, graph_region, recipe, 1.0);
        let color = if badges.is_empty() {
            visuals.text_color()
        } else {
            right -= badges.size().x;
            badges.paint(painter, Pos2::new(right, rect.center().y));
            right -= ROW_GAP;
            theme::color::TEXT_MUTED
        };
        let name = egui::WidgetText::from(recipe.name.as_str()).into_galley(
            ui,
            Some(egui::TextWrapMode::Truncate),
            (right - left).max(0.0),
            egui::TextStyle::Button,
        );
        let top = rect.center().y - name.size().y / 2.0;
        painter.galley(Pos2::new(left, top), name, color);
    }
    response
}

#[cfg(test)]
mod tests {
    use aic_data::Phase;

    use super::*;
    use crate::icons;

    #[test]
    fn a_builtin_node_has_the_picture_of_the_part_that_carries_its_item() {
        // From the canvas nothing says what it will carry: the belt part.
        assert_eq!(builtin_image(Builtin::Splitter, None), "splitter");
        assert_eq!(
            builtin_image(Builtin::Splitter, Some(Phase::Solid)),
            "splitter"
        );
        assert_eq!(
            builtin_image(Builtin::Splitter, Some(Phase::Liquid)),
            "splitter_pipe"
        );
        assert_eq!(
            builtin_image(Builtin::Merger, Some(Phase::Gas)),
            "merger_pipe"
        );
        assert_eq!(builtin_image(Builtin::Merger, None), "merger");
        assert_eq!(
            builtin_image(Builtin::ControlPort, Some(Phase::Solid)),
            "control_port"
        );
        assert_eq!(
            builtin_image(Builtin::ControlPort, Some(Phase::Gas)),
            "control_port_pipe"
        );
        // A storage is the Protocol Stash, the Fluid Tank or the Gas Tank.
        assert_eq!(builtin_image(Builtin::Storage, None), "storage");
        assert_eq!(
            builtin_image(Builtin::Storage, Some(Phase::Liquid)),
            "storage_liquid"
        );
        assert_eq!(
            builtin_image(Builtin::Storage, Some(Phase::Gas)),
            "storage_gas"
        );
        assert_eq!(builtin_image(Builtin::Source, None), "source");
        assert_eq!(builtin_image(Builtin::Sink, Some(Phase::Liquid)), "sink");
    }

    #[test]
    fn the_program_has_every_builtin_nodes_picture() {
        let builtins = [
            Builtin::Source,
            Builtin::Sink,
            Builtin::Splitter,
            Builtin::Merger,
            Builtin::Storage,
            Builtin::ControlPort,
        ];
        let phases = [
            None,
            Some(Phase::Solid),
            Some(Phase::Liquid),
            Some(Phase::Gas),
        ];
        for builtin in builtins {
            for phase in phases {
                let image = builtin_image(builtin, phase);
                assert!(
                    icons::has(ImageKind::Node, image),
                    "no icon nodes/{image}.png"
                );
            }
        }
    }
}
