//! The small popup a double-click on a port opens: the node's limit typed as a flow through that
//! port, a ratio lock on the port's links when it has several, and a button that clears its links.

use std::collections::HashMap;

use aic_core::{Graph, LinkId, Port, Side};
use aic_data::Catalog;
use eframe::egui::{self, Id, Key, Pos2, RichText};

use crate::i18n::Strings;
use crate::names;
use crate::widgets::LimitField;

/// The menu's width in points, so one opened from an input can sit left of its port.
pub const WIDTH: f32 = 220.0;

pub struct PortMenu {
    pub port: Port,
    /// Top-left of the popup, on screen. The workspace moves it with the port.
    pub screen_pos: Pos2,
    buffer: String,
    /// What the player is typing in each link's share field.
    shares: HashMap<LinkId, String>,
    just_opened: bool,
}

pub enum Outcome {
    Open,
    Close,
    /// The player typed a limit for the port; `None` clears it.
    SetLimit(Option<f64>),
    /// The player typed a link's share of the port's ratio lock; `None` frees the link.
    SetShare(LinkId, Option<f64>),
    /// Free every link of the port: back to the even split.
    ClearShares,
    ClearLinks,
}

impl PortMenu {
    pub fn new(port: Port, screen_pos: Pos2) -> Self {
        Self {
            port,
            screen_pos,
            buffer: String::new(),
            shares: HashMap::new(),
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
        let links: Vec<LinkId> = graph
            .links_at(self.port.node, self.port.side)
            .filter(|(_, link)| link.item == self.port.item)
            .map(|(id, _)| id)
            .collect();
        let linked = !links.is_empty();
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
                    if links.len() > 1 {
                        ui.separator();
                        if let Some(lock) = self.ratio_lock(ui, catalog, strings, graph, &links) {
                            outcome = lock;
                        }
                        ui.separator();
                    }
                    let clear = egui::Button::new(strings.get("port-menu-clear"))
                        .min_size(egui::vec2(ui.available_width(), 0.0));
                    if ui.add_enabled(linked, clear).clicked() {
                        outcome = Outcome::ClearLinks;
                    }
                });
            });
        // Consumed, so the Escape that closes the menu doesn't also clear the selection.
        let done = ctx.input_mut(|i| {
            i.consume_key(egui::Modifiers::NONE, Key::Escape) || i.key_pressed(Key::Enter)
        });
        let clicked_away = !self.just_opened && area.response.clicked_elsewhere();
        self.just_opened = false;
        if matches!(outcome, Outcome::Open) && (done || clicked_away) {
            outcome = Outcome::Close;
        }
        outcome
    }

    /// One share field per link, named by the node at its other end. Blank leaves a link free.
    fn ratio_lock(
        &mut self,
        ui: &mut egui::Ui,
        catalog: &Catalog,
        strings: &Strings,
        graph: &Graph,
        links: &[LinkId],
    ) -> Option<Outcome> {
        let mut outcome = None;
        let side = self.port.side;
        ui.label(RichText::new(strings.get("port-menu-ratio")).strong())
            .on_hover_text(strings.get("port-menu-ratio-hint"));
        egui::Grid::new("port-menu-ratio")
            .num_columns(2)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                for &id in links {
                    let Some(link) = graph.link(id) else {
                        continue;
                    };
                    let other = match side {
                        Side::Output => link.to,
                        Side::Input => link.from,
                    };
                    let name = graph
                        .node(other)
                        .map(|n| names::node_title(catalog, strings, &n.kind))
                        .unwrap_or_default();
                    ui.label(name.as_str());
                    let field = LimitField {
                        id: Id::new(("port-menu-share", id)),
                        buffer: self.shares.entry(id).or_default(),
                        current: link.share(side),
                        width: 60.0,
                        hint: strings.get("port-menu-share-free"),
                        label: strings.format("port-menu-share-label", &[("node", &name)]),
                    };
                    if let (_, Some(share)) = field.show(ui)
                        && share.is_none_or(|s| s > 0.0)
                    {
                        outcome = Some(Outcome::SetShare(id, share));
                    }
                    ui.end_row();
                }
            });
        let locked = links
            .iter()
            .any(|id| graph.link(*id).is_some_and(|l| l.share(side).is_some()));
        let unlock = egui::Button::new(strings.get("port-menu-unlock"))
            .min_size(egui::vec2(ui.available_width(), 0.0));
        if ui.add_enabled(locked, unlock).clicked() {
            outcome = Some(Outcome::ClearShares);
        }
        outcome
    }
}
