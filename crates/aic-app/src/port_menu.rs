//! The small popup a double-click on a port opens: the node's limit typed as a flow through that
//! port, and a button that clears the port's links.

use aic_core::{Graph, Port};
use aic_data::Catalog;
use eframe::egui::{self, Id, Key, Pos2, RichText};

use crate::i18n::Strings;
use crate::widgets::LimitField;

/// The menu's width in points, so one opened from an input can sit left of its port.
pub const WIDTH: f32 = 220.0;

pub struct PortMenu {
    pub port: Port,
    /// Top-left of the popup, on screen. The workspace moves it with the port.
    pub screen_pos: Pos2,
    buffer: String,
    just_opened: bool,
}

pub enum Outcome {
    Open,
    Close,
    /// The player typed a limit for the port; `None` clears it.
    SetLimit(Option<f64>),
    ClearLinks,
}

impl PortMenu {
    pub fn new(port: Port, screen_pos: Pos2) -> Self {
        Self {
            port,
            screen_pos,
            buffer: String::new(),
            just_opened: true,
        }
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        catalog: &Catalog,
        strings: &Strings,
        graph: &Graph,
    ) -> Outcome {
        let mut outcome = Outcome::Open;
        let field_id = Id::new("port-menu-limit");
        let has_limit = graph.node(self.port.node).is_some_and(|node| {
            !matches!(
                node.kind,
                aic_core::NodeKind::Splitter { .. }
                    | aic_core::NodeKind::Merger { .. }
                    | aic_core::NodeKind::Storage { .. }
            )
        });
        let linked = graph
            .links_at(self.port.node, self.port.side)
            .any(|(_, link)| link.item == self.port.item);
        let area = egui::Area::new(Id::new("port-menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(self.screen_pos)
            .constrain(true)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(WIDTH);
                    if has_limit {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(strings.get("port-menu-limit")).strong());
                            let field = LimitField {
                                id: field_id,
                                buffer: &mut self.buffer,
                                current: graph.port_limit(catalog, &self.port),
                                width: 90.0,
                                hint: strings.get("node-no-limit"),
                                label: strings.get("port-menu-limit-label"),
                            };
                            let (response, typed) = field.show(ui);
                            if self.just_opened {
                                response.request_focus();
                            }
                            if let Some(limit) = typed {
                                outcome = Outcome::SetLimit(limit);
                            }
                            ui.label(strings.get("settings-per-min"));
                        });
                    }
                    let clear = egui::Button::new(strings.get("port-menu-clear"))
                        .min_size(egui::vec2(ui.available_width(), 0.0));
                    if ui.add_enabled(linked, clear).clicked() {
                        outcome = Outcome::ClearLinks;
                    }
                });
            });
        let done = ctx.input(|i| i.key_pressed(Key::Escape) || i.key_pressed(Key::Enter));
        let clicked_away = !self.just_opened && area.response.clicked_elsewhere();
        self.just_opened = false;
        if matches!(outcome, Outcome::Open) && (done || clicked_away) {
            outcome = Outcome::Close;
        }
        outcome
    }
}
