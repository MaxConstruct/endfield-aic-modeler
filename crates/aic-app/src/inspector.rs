//! The right panel: the selected node or link, the graph's totals and its warnings.

use aic_core::{
    Graph, Issue, LinkId, NodeId, NodeKind, Port, Side, UndeterminedReason, Warning,
    check_resources,
};
use aic_data::{Catalog, ItemId};
use eframe::egui::{self, RichText, Ui};
use indexmap::IndexMap;

use crate::canvas::Selection;
use crate::document::pins;
use crate::format;
use crate::i18n::Strings;
use crate::names;
use crate::settings::parse_amount;
use crate::widgets::{self, colors};
use crate::workspace::{Workspace, link_error};

impl Workspace {
    pub(crate) fn inspector(&mut self, ui: &mut Ui, strings: &Strings) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading(strings.get("inspector-title"));
            match self.selection {
                Some(Selection::Node(node)) if self.doc.graph().node(node).is_some() => {
                    self.node_details(ui, strings, node);
                }
                Some(Selection::Link(link)) if self.doc.graph().link(link).is_some() => {
                    self.link_details(ui, strings, link);
                }
                _ => {
                    ui.weak(strings.get("inspector-nothing"));
                }
            }
            ui.separator();
            self.totals(ui, strings);
            ui.separator();
            self.warnings(ui, strings);
        });
    }

    fn node_details(&mut self, ui: &mut Ui, strings: &Strings, node: NodeId) {
        let catalog = &self.catalog;
        let graph = self.doc.graph();
        let solution = self.doc.solution();
        let kind = graph.node(node).expect("checked by caller").kind.clone();
        ui.label(
            RichText::new(names::node_title(catalog, strings, &kind))
                .strong()
                .size(16.0),
        );

        let mut facility_choice = None;
        egui::Grid::new("node-details")
            .num_columns(2)
            .spacing([12.0, 4.0])
            .show(ui, |ui| {
                if let NodeKind::Recipe {
                    recipe, facility, ..
                } = &kind
                {
                    ui.label(strings.get("inspector-facility"));
                    let facilities = catalog
                        .recipe(recipe.as_str())
                        .map(|r| r.facilities.clone())
                        .unwrap_or_default();
                    egui::ComboBox::from_id_salt("inspector-facility")
                        .selected_text(names::facility(catalog, facility.as_str()))
                        .show_ui(ui, |ui| {
                            for option in &facilities {
                                let name = names::facility(catalog, option.as_str());
                                if ui.selectable_label(option == facility, name).clicked() {
                                    facility_choice = Some(option.clone());
                                }
                            }
                        });
                    ui.end_row();
                    ui.label(strings.get("inspector-count"));
                    let count = solution.facility_count.get(&node).copied().flatten();
                    ui.monospace(format::fixed(count));
                    ui.end_row();
                }
                if let Some(rate) = solution.rate.get(&node) {
                    ui.label(strings.get("inspector-rate"));
                    ui.monospace(per_min(strings, *rate));
                    ui.end_row();
                }
                if let Some(flow) = solution.storage.get(&node) {
                    ui.label(strings.get("inspector-balance"));
                    ui.monospace(per_min(strings, flow.balance));
                    ui.end_row();
                    ui.label(strings.get("inspector-pass-through"));
                    ui.monospace(per_min(strings, flow.pass_through));
                    ui.end_row();
                }
            });
        if let Some(facility) = facility_choice {
            let _ = self.doc.set_facility(node, facility);
        }

        self.limit_field(ui, strings, node, &kind);
        self.throttle_reason(ui, strings, node);
        self.port_list(ui, strings, node, &kind);
        if ui.button(strings.get("node-remove")).clicked() {
            self.remove_node(node);
        }
    }

    fn limit_field(&mut self, ui: &mut Ui, strings: &Strings, node: NodeId, kind: &NodeKind) {
        let (limit, required) = match kind {
            NodeKind::Recipe { limit, .. }
            | NodeKind::Sink { limit, .. }
            | NodeKind::MissingRecipe { limit, .. } => (*limit, false),
            NodeKind::Source { limit, .. } | NodeKind::ControlPort { limit, .. } => {
                (Some(*limit), true)
            }
            _ => return,
        };
        if self.limit_edit.as_ref().is_none_or(|(id, _)| *id != node) {
            let text = limit.map(|v| v.to_string()).unwrap_or_default();
            self.limit_edit = Some((node, text));
        }
        let (_, text) = self.limit_edit.as_mut().expect("just set");
        let parsed = parse_amount(text);
        let problem = match parsed {
            Err(_) => Some("inspector-limit-invalid"),
            Ok(None) if required => Some("inspector-limit-required"),
            Ok(_) => None,
        };
        let mut changed = false;
        ui.horizontal(|ui| {
            let label = ui.label(strings.get("inspector-limit"));
            let mut edit = egui::TextEdit::singleline(text)
                .desired_width(90.0)
                .hint_text(strings.get("inspector-limit-hint"));
            if problem.is_some() {
                edit = edit.text_color(colors::NEGATIVE);
            }
            changed = ui.add(edit).labelled_by(label.id).changed();
        });
        if let Some(key) = problem {
            ui.label(
                RichText::new(strings.get(key))
                    .color(colors::NEGATIVE)
                    .small(),
            );
        }
        if changed
            && let Ok(value) = parse_amount(text)
            && (value.is_some() || !required)
        {
            let _ = self.doc.set_limit(node, value);
        }
    }

    fn throttle_reason(&self, ui: &mut Ui, strings: &Strings, node: NodeId) {
        let Some(reason) = self.doc.solution().throttle.get(&node) else {
            return;
        };
        let catalog = &self.catalog;
        let item = names::item(catalog, reason.item.as_str());
        let key = match reason.side {
            Side::Input => "inspector-throttle-input",
            Side::Output => "inspector-throttle-output",
        };
        ui.add_space(4.0);
        ui.label(
            RichText::new(strings.get("inspector-throttled"))
                .color(colors::WARN)
                .strong(),
        );
        ui.label(strings.format(key, &[("item", item)]));
        if let Some(by) = reason.limited_by.and_then(|id| self.doc.graph().node(id)) {
            let title = names::node_title(catalog, strings, &by.kind);
            ui.label(strings.format("inspector-limited-by", &[("node", &title)]));
        }
    }

    fn port_list(&self, ui: &mut Ui, strings: &Strings, node: NodeId, kind: &NodeKind) {
        let catalog = &self.catalog;
        let graph = self.doc.graph();
        let solution = self.doc.solution();
        ui.add_space(4.0);
        ui.label(RichText::new(strings.get("inspector-ports")).strong());
        egui::Grid::new("node-ports")
            .num_columns(3)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                for side in [Side::Input, Side::Output] {
                    for item in pins(catalog, kind, side).into_iter().flatten() {
                        let port = Port {
                            node,
                            item: item.clone(),
                            side,
                        };
                        let key = match side {
                            Side::Input => "inspector-port-in",
                            Side::Output => "inspector-port-out",
                        };
                        let name = names::item(catalog, item.as_str());
                        ui.label(strings.format(key, &[("item", name)]));
                        let flow = solution.port_flow.get(&port).copied().flatten();
                        ui.monospace(per_min(strings, flow));
                        let linked = graph.links_at(node, side).any(|(_, l)| l.item == item);
                        if linked {
                            ui.allocate_exact_size(egui::Vec2::ZERO, egui::Sense::hover());
                        } else {
                            let color = match side {
                                Side::Input => colors::DEPOT_IN,
                                Side::Output => colors::DEPOT_OUT,
                            };
                            ui.label(
                                RichText::new(strings.get("inspector-depot"))
                                    .color(color)
                                    .small(),
                            );
                        }
                        ui.end_row();
                    }
                }
            });
    }

    fn link_details(&mut self, ui: &mut Ui, strings: &Strings, link: LinkId) {
        let catalog = &self.catalog;
        let graph = self.doc.graph();
        let solution = self.doc.solution();
        let item = &graph.link(link).expect("checked by caller").item;
        let fluid = catalog.item(item.as_str()).is_some_and(|i| i.is_fluid);
        ui.horizontal(|ui| {
            widgets::item_badge(ui, catalog, item.as_str(), 18.0);
            ui.label(
                RichText::new(strings.get("inspector-link"))
                    .strong()
                    .size(16.0),
            );
            ui.label(names::item(catalog, item.as_str()));
        });
        egui::Grid::new("link-details")
            .num_columns(2)
            .spacing([12.0, 4.0])
            .show(ui, |ui| {
                ui.label(strings.get("inspector-flow"));
                ui.monospace(per_min(
                    strings,
                    solution.link_flow.get(&link).copied().flatten(),
                ));
                ui.end_row();
                ui.label(strings.get(if fluid {
                    "inspector-pipes"
                } else {
                    "inspector-belts"
                }));
                ui.monospace(format::short(
                    solution.link_belts.get(&link).copied().flatten(),
                ));
                ui.end_row();
            });
        if ui.button(strings.get("inspector-remove-link")).clicked() {
            self.doc.remove_link(&self.catalog, link);
            self.select(None);
        }
    }

    fn totals(&self, ui: &mut Ui, strings: &Strings) {
        let catalog = &self.catalog;
        let totals = &self.doc.solution().totals;
        let region = self.doc.graph().region();
        ui.heading(strings.get("totals-title"));
        if !totals.complete && !self.doc.graph().nodes().is_empty() {
            ui.label(
                RichText::new(strings.get("totals-incomplete"))
                    .weak()
                    .small(),
            );
        }
        item_list(ui, catalog, strings, "totals-depot-in", &totals.depot_in);
        item_list(ui, catalog, strings, "totals-depot-out", &totals.depot_out);
        if !totals.storage_balance.is_empty() {
            item_list(
                ui,
                catalog,
                strings,
                "totals-storage",
                &totals.storage_balance,
            );
        }

        egui::CollapsingHeader::new(strings.get("totals-power"))
            .default_open(true)
            .show(ui, |ui| {
                egui::Grid::new("totals-power")
                    .num_columns(2)
                    .show(ui, |ui| {
                        for (key, kw) in [
                            ("totals-power-draw", totals.power_draw_kw),
                            ("totals-power-generation", totals.power_generation_kw),
                            ("totals-power-net", totals.power_net_kw),
                        ] {
                            ui.label(strings.get(key));
                            let text = strings
                                .format("totals-power-kw", &[("kw", &format::fixed(Some(kw)))]);
                            ui.monospace(text);
                            ui.end_row();
                        }
                    });
            });

        egui::CollapsingHeader::new(strings.get("totals-facilities"))
            .default_open(true)
            .show(ui, |ui| {
                if totals.facilities.is_empty() {
                    ui.weak(strings.get("totals-none"));
                }
                egui::Grid::new("totals-facilities")
                    .num_columns(2)
                    .show(ui, |ui| {
                        for (facility, count) in &totals.facilities {
                            ui.label(names::facility(catalog, facility.as_str()));
                            ui.monospace(format::fixed(Some(*count)));
                            ui.end_row();
                        }
                    });
            });

        egui::CollapsingHeader::new(strings.get("totals-raw"))
            .default_open(true)
            .show(ui, |ui| {
                if totals.raw_resource_use.is_empty() {
                    ui.weak(strings.get("totals-none"));
                }
                egui::Grid::new("totals-raw").num_columns(3).show(ui, |ui| {
                    for (item, used) in &totals.raw_resource_use {
                        widgets::item_badge(ui, catalog, item.as_str(), 16.0);
                        ui.label(names::item(catalog, item.as_str()));
                        let used_text = format::fixed(Some(*used));
                        let text = match self.availability.get(region, item) {
                            Some(available) => {
                                let over = *used > available + 1e-9;
                                let text = strings.format(
                                    "totals-raw-use",
                                    &[
                                        ("used", &used_text),
                                        ("available", &format::fixed(Some(available))),
                                    ],
                                );
                                if over {
                                    RichText::new(text).color(colors::WARN)
                                } else {
                                    RichText::new(text)
                                }
                            }
                            None => RichText::new(
                                strings.format("totals-raw-use-unlimited", &[("used", &used_text)]),
                            ),
                        };
                        ui.label(text.monospace());
                        ui.end_row();
                    }
                });
            });
    }

    fn warnings(&self, ui: &mut Ui, strings: &Strings) {
        let solution = self.doc.solution();
        let graph = self.doc.graph();
        let region = graph.region();
        let resources = check_resources(&solution.totals, &self.availability.region(region));
        let all: Vec<&Warning> = solution.warnings.iter().chain(&resources).collect();
        ui.heading(strings.get("warnings-title"));
        if all.is_empty() {
            ui.weak(strings.get("totals-none"));
        }
        for warning in all {
            let text = warning_text(&self.catalog, strings, graph, warning);
            ui.label(RichText::new(text).color(colors::WARN));
        }
    }
}

fn per_min(strings: &Strings, value: Option<f64>) -> String {
    match value {
        Some(_) => strings.format("totals-per-min", &[("rate", &format::fixed(value))]),
        None => String::new(),
    }
}

fn item_list(
    ui: &mut Ui,
    catalog: &Catalog,
    strings: &Strings,
    title_key: &str,
    items: &IndexMap<ItemId, f64>,
) {
    egui::CollapsingHeader::new(strings.get(title_key))
        .default_open(true)
        .show(ui, |ui| {
            if items.is_empty() {
                ui.weak(strings.get("totals-none"));
            }
            egui::Grid::new(title_key).num_columns(3).show(ui, |ui| {
                for (item, rate) in items {
                    widgets::item_badge(ui, catalog, item.as_str(), 16.0);
                    ui.label(names::item(catalog, item.as_str()));
                    ui.monospace(per_min(strings, Some(*rate)));
                    ui.end_row();
                }
            });
        });
}

/// A warning in the player's words.
fn warning_text(catalog: &Catalog, strings: &Strings, graph: &Graph, warning: &Warning) -> String {
    let item = |id: &ItemId| names::item(catalog, id.as_str()).to_owned();
    match warning {
        Warning::Undetermined { reason, .. } => strings.get(match reason {
            UndeterminedReason::NoLimit => "warning-undetermined-no-limit",
            UndeterminedReason::Unbounded => "warning-undetermined-unbounded",
        }),
        Warning::Failed { message, .. } => {
            strings.format("warning-failed", &[("message", message)])
        }
        Warning::BeltOverCapacity { link, lanes } => {
            let link_item = graph.link(*link).map(|l| l.item.clone());
            let fluid = link_item
                .as_ref()
                .and_then(|id| catalog.item(id.as_str()))
                .is_some_and(|i| i.is_fluid);
            let name = link_item.as_ref().map(item).unwrap_or_default();
            let key = if fluid {
                "warning-pipe-over"
            } else {
                "warning-belt-over"
            };
            strings.format(key, &[("item", &name), ("lanes", &lanes.to_string())])
        }
        Warning::ResourceOverUse {
            item: id,
            used,
            available,
        } => strings.format(
            "warning-resource-over",
            &[
                ("item", &item(id)),
                ("used", &format::fixed(Some(*used))),
                ("available", &format::fixed(Some(*available))),
            ],
        ),
        Warning::Issue(issue) => issue_text(catalog, strings, issue),
    }
}

fn issue_text(catalog: &Catalog, strings: &Strings, issue: &Issue) -> String {
    match issue {
        Issue::BadLink { error, .. } => strings.format(
            "warning-issue-bad-link",
            &[("error", &link_error(catalog, strings, error))],
        ),
        Issue::MissingRecipe { recipe, .. } => strings.format(
            "warning-issue-missing-recipe",
            &[("recipe", recipe.as_str())],
        ),
        Issue::UnknownItem { item, .. } => {
            strings.format("warning-issue-unknown-item", &[("item", item.as_str())])
        }
        Issue::RecipeOutsideRegion { recipe, .. } => strings.format(
            "warning-issue-recipe-region",
            &[("recipe", names::recipe(catalog, recipe.as_str()))],
        ),
        Issue::FacilityOutsideRegion { facility, .. } => strings.format(
            "warning-issue-facility-region",
            &[("facility", names::facility(catalog, facility.as_str()))],
        ),
        Issue::FacilityCannotRun { facility, .. } => strings.format(
            "warning-issue-facility-cannot-run",
            &[("facility", names::facility(catalog, facility.as_str()))],
        ),
        Issue::InvalidLimit { .. } => strings.get("warning-issue-invalid-limit"),
    }
}
