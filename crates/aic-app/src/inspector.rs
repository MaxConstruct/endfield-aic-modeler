//! The right panel: the selected node or link, the graph's totals and its warnings.

use aic_core::{
    LinkId, NodeId, NodeKind, Port, RoundUp, Side, Warning, WorkspaceId, check_resources,
};
use aic_data::{Catalog, ItemId};
use eframe::egui::{self, RichText, Ui};
use indexmap::IndexMap;

use crate::canvas::Selection;
use crate::document::pins;
use crate::editor::Editor;
use crate::i18n::Strings;
use crate::settings::parse_amount;
use crate::theme;
use crate::widgets::{self, LimitField, colors};
use aic_editor::format;
use aic_editor::messages::{per_min, warning_text};
use aic_editor::names;

impl Editor {
    pub(crate) fn inspector(&mut self, ui: &mut Ui, strings: &Strings) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading(strings.get("inspector-title"));
            match self.session.selection().cloned() {
                Some(Selection::Link(link))
                    if self.session.document().graph().link(link).is_some() =>
                {
                    self.link_details(ui, strings, link);
                }
                Some(selection) => match (selection.single_node(), selection.single_workspace()) {
                    (Some(node), _) if self.session.document().graph().node(node).is_some() => {
                        self.node_details(ui, strings, node);
                    }
                    (_, Some(workspace))
                        if self
                            .session
                            .document()
                            .graph()
                            .workspace_info(workspace)
                            .is_some() =>
                    {
                        self.workspace_details(ui, strings, workspace);
                    }
                    _ => self.many_nodes(ui, strings, selection.items().len()),
                },
                None => {
                    ui.weak(strings.get("inspector-nothing"));
                }
            }
            ui.separator();
            self.totals(ui, strings);
            ui.separator();
            self.warnings(ui, strings);
        });
    }

    fn workspace_details(&mut self, ui: &mut Ui, strings: &Strings, workspace: WorkspaceId) {
        let catalog = self.session.catalog();
        let graph = self.session.document().graph();
        let info = graph
            .workspace_info(workspace)
            .expect("checked by caller")
            .clone();
        ui.label(RichText::new(strings.get("kind-workspace")).small().weak());
        let mut name = info.name.clone();
        let response = ui.add(widgets::wide_field(ui, &mut name));
        let label = strings.get("inspector-workspace-name");
        ui.ctx()
            .accesskit_node_builder(response.id, |node| node.set_label(label));
        let count = graph
            .nodes_in_workspace(workspace)
            .into_iter()
            .filter(|&node| !matches!(graph.nodes()[node].kind, NodeKind::WorkspacePort { .. }))
            .count();
        ui.label(names::node_count(strings, count));
        let solution = self.session.document().solution();
        ui.add_space(4.0);
        ui.label(theme::strong(strings.get("inspector-ports")));
        egui::Grid::new("workspace-ports")
            .num_columns(2)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                for side in [Side::Input, Side::Output] {
                    for port in graph.ports_of(workspace, side) {
                        let key = match side {
                            Side::Input => "inspector-port-in",
                            Side::Output => "inspector-port-out",
                        };
                        let item = graph.nodes()[port].kind.item();
                        let name = names::maybe_item(catalog, strings, item);
                        ui.label(strings.format(key, &[("item", &name)]));
                        ui.monospace(per_min(
                            strings,
                            solution.rate.get(&port).copied().flatten(),
                        ));
                        ui.end_row();
                    }
                }
            });
        let mut open = false;
        let mut dissolve = false;
        ui.horizontal(|ui| {
            open = ui.button(strings.get("workspace-open")).clicked();
            dissolve = ui.button(strings.get("workspace-dissolve")).clicked();
        });
        let remove = ui.button(strings.get("workspace-remove")).clicked();
        if response.changed() && !name.trim().is_empty() {
            let _ = self.session.rename_workspace(workspace, name.trim());
        }
        if open {
            self.show_level(Some(workspace));
        } else if dissolve {
            self.dissolve_selection();
        } else if remove {
            self.session.remove_workspace(workspace);
            self.select(None);
        }
    }

    fn many_nodes(&mut self, ui: &mut Ui, strings: &Strings, count: usize) {
        ui.label(
            RichText::new(strings.format("inspector-many", &[("count", &count.to_string())]))
                .family(theme::semibold())
                .color(theme::color::TEXT_STRONG)
                .size(theme::text::HEADING),
        );
        ui.horizontal(|ui| {
            if ui.button(strings.get("menu-duplicate")).clicked() {
                self.duplicate();
            }
            if ui.button(strings.get("inspector-remove-nodes")).clicked() {
                self.delete_selection();
            }
        });
    }

    fn node_details(&mut self, ui: &mut Ui, strings: &Strings, node: NodeId) {
        let catalog = self.session.catalog();
        let graph = self.session.document().graph();
        let solution = self.session.document().solution();
        let kind = graph.node(node).expect("checked by caller").kind.clone();
        ui.label(
            RichText::new(names::node_title(catalog, strings, &kind))
                .family(theme::semibold())
                .color(theme::color::TEXT_STRONG)
                .size(theme::text::HEADING),
        );

        if let NodeKind::Recipe { recipe, .. } = &kind
            && let Some(recipe) = catalog.recipe(recipe.as_str())
            && let Some(regions) = names::recipe_regions(catalog, strings, recipe)
        {
            ui.label(RichText::new(regions).small().weak());
        }

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
                    if let Some(gas) = (catalog.recipe(recipe.as_str()))
                        .and_then(|recipe| recipe.environment.as_ref())
                    {
                        ui.label(strings.get("inspector-environment"));
                        ui.label(
                            RichText::new(names::environment(catalog, strings, gas))
                                .color(colors::WARN),
                        );
                        ui.end_row();
                    }
                    ui.label(strings.get("inspector-count"));
                    let count = solution.facility_count.get(&node).copied().flatten();
                    ui.monospace(format::short(count));
                    ui.end_row();
                    if let Some(rounded) = count.map(RoundUp::of) {
                        ui.label(strings.get("inspector-round-up"));
                        let percent = format::short(Some(rounded.utilization * 100.0));
                        ui.monospace(strings.format(
                            "inspector-round-up-value",
                            &[
                                ("facilities", &rounded.facilities.to_string()),
                                ("percent", &percent),
                            ],
                        ));
                        ui.end_row();
                    }
                }
                if let Some(rate) = solution.rate.get(&node) {
                    ui.label(strings.get("inspector-rate"));
                    ui.monospace(per_min(strings, *rate));
                    ui.end_row();
                }
                for (coupon, earned) in solution.coupons.get(&node).into_iter().flatten() {
                    ui.label(strings.get("inspector-earns"));
                    ui.monospace(strings.format(
                        "inspector-earns-value",
                        &[
                            ("rate", &format::short(Some(*earned))),
                            ("coupon", names::item(catalog, coupon.as_str())),
                        ],
                    ));
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
            let _ = self.session.set_facility(node, facility);
        }

        self.limit_field(ui, strings, node, &kind);
        self.throttle_reason(ui, strings, node);
        self.port_list(ui, strings, node, &kind);
        if ui.button(strings.get("node-remove")).clicked() {
            self.session.remove_node(node);
        }
    }

    fn limit_field(&mut self, ui: &mut Ui, strings: &Strings, node: NodeId, kind: &NodeKind) {
        let required = match kind {
            NodeKind::Recipe { .. }
            | NodeKind::Sink { .. }
            | NodeKind::OutpostOrder { .. }
            | NodeKind::MissingRecipe { .. } => false,
            NodeKind::Source { .. } | NodeKind::ControlPort { .. } => true,
            _ => return,
        };
        let mut typed = None;
        widgets::field_row(ui, |ui| {
            ui.label(strings.get("inspector-limit"));
            let field = LimitField {
                id: egui::Id::new("inspector-limit"),
                buffer: &mut self.limit_edit,
                current: &widgets::limit_text(kind.limit()),
                width: 90.0,
                hint: strings.get("inspector-limit-hint"),
                label: strings.get("inspector-limit"),
            };
            typed = field.show(ui).1;
        });
        let problem = match parse_amount(&self.limit_edit) {
            Err(_) => Some("inspector-limit-invalid"),
            Ok(None) if required => Some("inspector-limit-required"),
            Ok(_) => None,
        };
        if let Some(key) = problem {
            ui.label(
                RichText::new(strings.get(key))
                    .color(colors::NEGATIVE)
                    .small(),
            );
        }
        if let Some(limit) = typed
            && (limit.is_some() || !required)
        {
            let _ = self.session.set_limit(node, limit);
        }
    }

    fn throttle_reason(&self, ui: &mut Ui, strings: &Strings, node: NodeId) {
        let Some(reason) = self.session.document().solution().throttle.get(&node) else {
            return;
        };
        let catalog = self.session.catalog();
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
        if let Some(by) = reason
            .limited_by
            .and_then(|id| self.session.document().graph().node(id))
        {
            let title = names::node_title(catalog, strings, &by.kind);
            ui.label(strings.format("inspector-limited-by", &[("node", &title)]));
        }
    }

    fn port_list(&self, ui: &mut Ui, strings: &Strings, node: NodeId, kind: &NodeKind) {
        let catalog = self.session.catalog();
        let graph = self.session.document().graph();
        let solution = self.session.document().solution();
        ui.add_space(4.0);
        ui.label(theme::strong(strings.get("inspector-ports")));
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
        let catalog = self.session.catalog();
        let graph = self.session.document().graph();
        let solution = self.session.document().solution();
        let item = &graph.link(link).expect("checked by caller").item;
        let fluid = catalog.item(item.as_str()).is_some_and(|i| i.is_fluid());
        ui.horizontal(|ui| {
            widgets::item_icon(ui, catalog, item.as_str(), 18.0);
            ui.label(
                RichText::new(strings.get("inspector-link"))
                    .family(theme::semibold())
                    .color(theme::color::TEXT_STRONG)
                    .size(theme::text::HEADING),
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
                let found = graph.link(link).expect("checked by caller");
                for (side, key) in [
                    (Side::Output, "inspector-share-out"),
                    (Side::Input, "inspector-share-in"),
                ] {
                    if let Some(share) = found.share(side) {
                        ui.label(strings.get(key));
                        ui.monospace(format::short(Some(share)));
                        ui.end_row();
                    }
                }
            });
        if ui.button(strings.get("inspector-remove-link")).clicked() {
            self.session.disconnect(link);
            self.select(None);
        }
    }

    fn totals(&self, ui: &mut Ui, strings: &Strings) {
        let catalog = self.session.catalog();
        let totals = &self.session.document().solution().totals;
        let region = self.session.document().graph().region();
        ui.heading(strings.get("totals-title"));
        if !totals.complete && !self.session.document().graph().nodes().is_empty() {
            ui.label(
                RichText::new(strings.get("totals-incomplete"))
                    .weak()
                    .small(),
            );
        }
        item_list(ui, catalog, strings, "totals-depot-in", &totals.depot_in);
        item_list(ui, catalog, strings, "totals-depot-out", &totals.depot_out);
        if !totals.coupons.is_empty() {
            item_list(ui, catalog, strings, "totals-coupons", &totals.coupons);
        }
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
                                .format("totals-power-kw", &[("kw", &format::short(Some(kw)))]);
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
                    .num_columns(3)
                    .show(ui, |ui| {
                        for (facility, count) in &totals.facilities {
                            ui.label(names::facility(catalog, facility.as_str()));
                            ui.monospace(format::short(Some(*count)));
                            let rounded = totals.facilities_rounded.get(facility).copied();
                            ui.monospace(strings.format(
                                "totals-facilities-rounded",
                                &[("count", &rounded.unwrap_or_default().to_string())],
                            ))
                            .on_hover_text(strings.get("totals-facilities-rounded-hint"));
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
                        widgets::item_icon(ui, catalog, item.as_str(), 16.0);
                        ui.label(names::item(catalog, item.as_str()));
                        let used_text = format::short(Some(*used));
                        let text = match self.session.availability().get(region, item) {
                            Some(available) => {
                                let over = *used > available + 1e-9;
                                let text = strings.format(
                                    "totals-raw-use",
                                    &[
                                        ("used", &used_text),
                                        ("available", &format::short(Some(available))),
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
        let solution = self.session.document().solution();
        let graph = self.session.document().graph();
        let region = graph.region();
        let resources = check_resources(
            &solution.totals,
            &self.session.availability().region(region),
        );
        let all: Vec<&Warning> = solution.warnings.iter().chain(&resources).collect();
        ui.heading(strings.get("warnings-title"));
        if all.is_empty() {
            ui.weak(strings.get("totals-none"));
        }
        for warning in all {
            let text = warning_text(self.session.catalog(), strings, graph, warning);
            ui.label(RichText::new(text).color(colors::WARN));
        }
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
                    widgets::item_icon(ui, catalog, item.as_str(), 16.0);
                    ui.label(names::item(catalog, item.as_str()));
                    ui.monospace(per_min(strings, Some(*rate)));
                    ui.end_row();
                }
            });
        });
}
