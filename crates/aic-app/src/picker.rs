//! The node picker: one popup for every way of adding a node, modeled on Satisfactory Modeler's
//! recipe selector. A search box, the built-in nodes on the left, recipes on the right. One click
//! adds the node.

use std::sync::atomic::{AtomicU64, Ordering};

use aic_core::{NodeId, NodeKind, Side};
use aic_data::{Catalog, ImageKind, ItemId, Recipe, RecipeId, RegionId};
use eframe::egui::{self, Align2, Key, Pos2, Rect, Sense, Ui, Vec2, WidgetInfo, WidgetType};

use crate::i18n::Strings;
use crate::names;
use crate::widgets::{self, paint_item};

/// Where the picker was opened.
#[derive(Debug, Clone, PartialEq)]
pub enum Origin {
    /// Right-click on empty canvas: add a node on its own.
    Canvas,
    /// From a port of `item`: on an input, add a producer of it; on an output, a consumer. The new
    /// node is linked to the port.
    Port {
        node: NodeId,
        side: Side,
        item: ItemId,
    },
}

/// The node kinds that are not recipes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Builtin {
    Source,
    Sink,
    Splitter,
    Merger,
    Storage,
    ControlPort,
}

const LOGISTICS: [Builtin; 4] = [
    Builtin::Splitter,
    Builtin::Merger,
    Builtin::Storage,
    Builtin::ControlPort,
];

#[derive(Debug, Clone, PartialEq)]
pub enum Choice {
    Recipe(RecipeId),
    Builtin(Builtin),
    /// An empty workspace, offered from the canvas.
    Workspace,
}

/// Which parts of a recipe the search text is matched against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Matching {
    pub name: bool,
    pub inputs: bool,
    pub outputs: bool,
}

impl Default for Matching {
    fn default() -> Self {
        Self {
            name: true,
            inputs: false,
            outputs: true,
        }
    }
}

/// The built-in nodes offered for `origin`. A source or sink needs an item, so they are only
/// offered from a port.
pub fn builtins(origin: &Origin) -> Vec<Builtin> {
    let end = match origin {
        Origin::Canvas => None,
        Origin::Port {
            side: Side::Input, ..
        } => Some(Builtin::Source),
        Origin::Port {
            side: Side::Output, ..
        } => Some(Builtin::Sink),
    };
    end.into_iter().chain(LOGISTICS).collect()
}

/// The recipes offered for `origin`, in data order: those of the region, not hidden, that make
/// (from an input port) or take (from an output port) the port's item, and match `query`.
/// Settlement trades are offered as outpost orders, except from an input port: they make only
/// coupons.
pub fn recipes<'a>(
    catalog: &'a Catalog,
    region: &RegionId,
    origin: &Origin,
    query: &str,
    matching: Matching,
) -> Vec<&'a Recipe> {
    let query = query.trim().to_lowercase();
    let contains = |name: &str| name.to_lowercase().contains(&query);
    let any_item = |items: &[(ItemId, f64)]| {
        items
            .iter()
            .any(|(item, _)| contains(names::item(catalog, item.as_str())))
    };
    catalog
        .recipes()
        .values()
        .filter(|recipe| {
            let trade_ok = recipe.is_trade
                && !matches!(
                    origin,
                    Origin::Port {
                        side: Side::Input,
                        ..
                    }
                );
            (!recipe.hidden || trade_ok) && recipe.available_in(region)
        })
        .filter(|recipe| match origin {
            Origin::Canvas => true,
            Origin::Port { side, item, .. } => {
                let items = match side {
                    Side::Input => &recipe.outputs,
                    Side::Output => &recipe.inputs,
                };
                items.iter().any(|(id, _)| id == item)
            }
        })
        .filter(|recipe| {
            query.is_empty()
                || (matching.name && contains(&recipe.name))
                || (matching.inputs && any_item(&recipe.inputs))
                || (matching.outputs && any_item(&recipe.outputs))
        })
        .collect()
}

/// The node a built-in choice adds. A source and a control port need a limit; they start at one
/// belt (or pipe) of their item, for the player to change.
pub fn builtin_kind(catalog: &Catalog, builtin: Builtin, item: Option<&ItemId>) -> NodeKind {
    let one_lane = || {
        let fluid = item
            .and_then(|id| catalog.item(id.as_str()))
            .is_some_and(|item| item.is_fluid);
        if fluid {
            catalog.pipe_per_min()
        } else {
            catalog.belt_per_min()
        }
    };
    let item = item.cloned();
    match builtin {
        Builtin::Source => NodeKind::Source {
            item: item.expect("a source is offered only from a port"),
            limit: one_lane(),
        },
        Builtin::Sink => NodeKind::Sink {
            item: item.expect("a sink is offered only from a port"),
            limit: None,
        },
        Builtin::Splitter => NodeKind::Splitter { item },
        Builtin::Merger => NodeKind::Merger { item },
        Builtin::Storage => NodeKind::Storage { item },
        Builtin::ControlPort => NodeKind::ControlPort {
            item,
            limit: one_lane(),
        },
    }
}

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
const ROW_HEIGHT: f32 = 30.0;
const BADGE: f32 = 22.0;

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
        region: &RegionId,
    ) -> Outcome {
        let mut outcome = Outcome::Open;
        let area = egui::Area::new(self.area)
            .order(egui::Order::Foreground)
            .fixed_pos(self.screen_pos)
            .constrain(true)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(WIDTH);
                    if let Some(choice) = self.contents(ui, catalog, strings, region) {
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
        region: &RegionId,
    ) -> Option<Choice> {
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

        let recipes = recipes(catalog, region, &self.origin, &self.query, self.matching);
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
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            &recipe.name,
            egui::TextStyle::Button.resolve(ui.style()),
            visuals.text_color(),
        );
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    const DATA: &[u8] = include_bytes!("../tests/fixtures/satisfactory/vtest/data.json");
    const EN: &[u8] = include_bytes!("../tests/fixtures/satisfactory/vtest/i18n/en.json");

    fn catalog() -> Catalog {
        Catalog::from_bytes(DATA, EN, None).expect("fixture loads")
    }

    fn ids(recipes: Vec<&Recipe>) -> Vec<&str> {
        recipes.into_iter().map(|r| r.id.as_str()).collect()
    }

    fn port(side: Side, item: &str) -> Origin {
        Origin::Port {
            node: NodeId::default(),
            side,
            item: item.into(),
        }
    }

    #[test]
    fn canvas_lists_the_regions_visible_recipes_in_data_order() {
        let catalog = catalog();
        let tundra = ids(recipes(
            &catalog,
            &"tundra".into(),
            &Origin::Canvas,
            "",
            Matching::default(),
        ));
        assert_eq!(
            tundra,
            [
                "iron_ore",
                "iron_ingot",
                "iron_plate",
                "iron_rod",
                "screw",
                "reinforced_plate",
                "modular_frame",
                "coupon-iron_plate"
            ]
        );
        let wuling = ids(recipes(
            &catalog,
            &"jinlong".into(),
            &Origin::Canvas,
            "",
            Matching::default(),
        ));
        assert!(wuling.contains(&"iron_wire"));
        assert!(
            wuling.contains(&"coupon-iron_plate"),
            "a settlement trade is offered as an outpost order"
        );
    }

    #[test]
    fn an_input_port_lists_producers_including_mining() {
        let catalog = catalog();
        let origin = port(Side::Input, "iron_ore");
        assert_eq!(
            ids(recipes(
                &catalog,
                &"tundra".into(),
                &origin,
                "",
                Matching::default()
            )),
            ["iron_ore"]
        );
        assert_eq!(builtins(&origin)[0], Builtin::Source);
    }

    #[test]
    fn an_output_port_lists_consumers() {
        let catalog = catalog();
        let origin = port(Side::Output, "iron_ingot");
        assert_eq!(
            ids(recipes(
                &catalog,
                &"tundra".into(),
                &origin,
                "",
                Matching::default()
            )),
            ["iron_plate", "iron_rod"]
        );
        assert_eq!(builtins(&origin)[0], Builtin::Sink);
        assert!(!builtins(&Origin::Canvas).contains(&Builtin::Source));
    }

    #[test]
    fn search_matches_the_chosen_parts_case_insensitively() {
        let catalog = catalog();
        let region = "tundra".into();
        let by_name = Matching {
            name: true,
            inputs: false,
            outputs: false,
        };
        assert_eq!(
            ids(recipes(
                &catalog,
                &region,
                &Origin::Canvas,
                "PLATE",
                by_name
            )),
            ["iron_plate", "reinforced_plate", "coupon-iron_plate"]
        );
        let by_inputs = Matching {
            name: false,
            inputs: true,
            outputs: false,
        };
        assert_eq!(
            ids(recipes(
                &catalog,
                &region,
                &Origin::Canvas,
                "iron rod",
                by_inputs
            )),
            ["screw", "modular_frame"]
        );
    }

    #[test]
    fn a_source_starts_at_one_belt() {
        let catalog = catalog();
        let kind = builtin_kind(&catalog, Builtin::Source, Some(&"iron_ore".into()));
        assert_eq!(
            kind,
            NodeKind::Source {
                item: "iron_ore".into(),
                limit: 30.0
            }
        );
    }
}
