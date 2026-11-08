//! The node canvas: egui-snarl drawing the [`Document`]'s graph with the solver's numbers.
//!
//! The viewer only reads the graph. Everything the player does on the canvas comes back as
//! [`CanvasEvent`]s, which the workspace applies to the document after the frame's canvas is drawn,
//! so the graph and its view always change together.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use aic_core::{Graph, LinkId, NodeId, NodeKind, Port, PortRef, Side, Solution};
use aic_data::{Catalog, FacilityId, ItemId};
use eframe::egui::{
    self, Color32, CornerRadius, FontId, Id, LayerId, Pos2, Rect, RichText, Sense, Stroke, Ui,
    emath::TSTransform, vec2,
};
use egui_snarl::ui::{
    AnyPins, BackgroundPattern, NodeLayout, PinInfo, PinPlacement, PinWireInfo, SnarlPin,
    SnarlStyle, SnarlViewer, SnarlWidget,
};
use egui_snarl::{InPin, InPinId, OutPin, OutPinId, Snarl};

use crate::document::{Anchor, Document, ViewId, pins, port_ref};
use crate::format;
use crate::i18n::Strings;
use crate::names;
use crate::picker::Origin;
use crate::widgets::{self, colors};

const CANVAS_ID: &str = "aic-canvas";
/// How far from a port a node added from it is placed, in graph units.
const PORT_GAP: f32 = 80.0;
const BADGE: f32 = 16.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    Node(NodeId),
    Link(LinkId),
}

/// What the player did on the canvas this frame.
#[derive(Debug, Clone, PartialEq)]
pub enum CanvasEvent {
    Connect {
        from: PortRef,
        to: PortRef,
    },
    Disconnect(LinkId),
    /// Open the node picker. `graph_pos` is where the new node goes.
    OpenPicker {
        origin: Origin,
        graph_pos: Pos2,
    },
    Select(Option<Selection>),
    RemoveNode(NodeId),
    SetFacility(NodeId, FacilityId),
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum PinKey {
    In(InPinId),
    Out(OutPinId),
}

/// Where snarl drew each pin last, in graph space. Link labels sit between two of them.
type PinCenters = Rc<RefCell<HashMap<PinKey, Pos2>>>;

/// View state that lives across frames.
pub struct Canvas {
    style: SnarlStyle,
    pins: PinCenters,
    /// Graph space to screen space, as of the last frame.
    pub to_global: TSTransform,
    /// The canvas area on screen, as of the last frame.
    pub rect: Rect,
}

impl Default for Canvas {
    fn default() -> Self {
        Self {
            style: SnarlStyle {
                node_layout: Some(NodeLayout::coil()),
                pin_placement: Some(PinPlacement::Edge),
                pin_size: Some(10.0),
                wire_width: Some(2.0),
                collapsible: Some(false),
                bg_pattern: Some(BackgroundPattern::grid(vec2(40.0, 40.0), 0.0)),
                ..SnarlStyle::new()
            },
            pins: PinCenters::default(),
            to_global: TSTransform::IDENTITY,
            rect: Rect::ZERO,
        }
    }
}

impl Canvas {
    /// The middle of the visible canvas, in graph space.
    pub fn center(&self) -> Pos2 {
        self.to_global.inverse() * self.rect.center()
    }

    pub fn to_screen(&self, graph_pos: Pos2) -> Pos2 {
        self.to_global * graph_pos
    }

    /// Where the port of `item` on `node`'s `side` was drawn last frame, on screen.
    pub fn port_on_screen(
        &self,
        doc: &Document,
        catalog: &Catalog,
        node: NodeId,
        side: Side,
        item: &ItemId,
    ) -> Option<Pos2> {
        let view_id = doc.view_id(node)?;
        let kind = &doc.graph().node(node)?.kind;
        let index = pins(catalog, kind, side)
            .iter()
            .position(|pin| pin.as_ref().is_none_or(|pin| pin == item))?;
        let key = match side {
            Side::Input => PinKey::In(InPinId {
                node: view_id,
                input: index,
            }),
            Side::Output => PinKey::Out(OutPinId {
                node: view_id,
                output: index,
            }),
        };
        let center = *self.pins.borrow().get(&key)?;
        Some(self.to_screen(center))
    }

    pub fn show(
        &mut self,
        ui: &mut Ui,
        doc: &mut Document,
        catalog: &Catalog,
        strings: &Strings,
        selection: Option<Selection>,
    ) -> Vec<CanvasEvent> {
        // The snarl response spans everything; the panel's own rect is what's on screen.
        self.rect = ui.max_rect();
        let (graph, view, anchors, solution) = doc.parts();
        let mut viewer = Viewer {
            catalog,
            strings,
            graph,
            solution,
            selection,
            anchors,
            pins: &self.pins,
            to_global: &mut self.to_global,
            events: Vec::new(),
        };
        let response = SnarlWidget::new()
            .id(Id::new(CANVAS_ID))
            .style(self.style)
            .show(view, &mut viewer, ui);
        let mut events = viewer.events;

        let labels = LinkLabels {
            catalog,
            strings,
            graph,
            solution,
            view,
            pins: &self.pins.borrow(),
            selection,
        };
        let layer = LayerId::new(ui.layer_id().order, Id::new(CANVAS_ID));
        let clicked_label = labels.paint(ui, layer, self.to_global);
        let selected = events.iter().any(|e| matches!(e, CanvasEvent::Select(_)));
        if let Some(link) = clicked_label {
            events.push(CanvasEvent::Select(Some(Selection::Link(link))));
        } else if response.clicked() && !selected {
            events.push(CanvasEvent::Select(None));
        }
        if response.secondary_clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
            events.push(CanvasEvent::OpenPicker {
                origin: Origin::Canvas,
                graph_pos: self.to_global.inverse() * pos,
            });
        }
        events
    }
}

/// A pin that records where snarl drew it.
struct RecordingPin {
    info: PinInfo,
    key: PinKey,
    centers: PinCenters,
}

impl SnarlPin for RecordingPin {
    fn draw(
        self,
        snarl_style: &SnarlStyle,
        style: &egui::Style,
        rect: Rect,
        painter: &egui::Painter,
    ) -> PinWireInfo {
        self.centers.borrow_mut().insert(self.key, rect.center());
        self.info.draw(snarl_style, style, rect, painter)
    }
}

struct Viewer<'a> {
    catalog: &'a Catalog,
    strings: &'a Strings,
    graph: &'a Graph,
    solution: &'a Solution,
    selection: Option<Selection>,
    anchors: &'a mut HashMap<NodeId, Anchor>,
    pins: &'a PinCenters,
    to_global: &'a mut TSTransform,
    events: Vec<CanvasEvent>,
}

impl Viewer<'_> {
    fn kind(&self, id: NodeId) -> Option<&NodeKind> {
        self.graph.node(id).map(|node| &node.kind)
    }

    /// The item of a pin, or `None` for a logistics node without one.
    fn pin_item(&self, id: NodeId, side: Side, index: usize) -> Option<ItemId> {
        let kind = self.kind(id)?;
        pins(self.catalog, kind, side)
            .into_iter()
            .nth(index)
            .flatten()
    }

    fn port_ref(&self, id: NodeId, side: Side, index: usize) -> Option<PortRef> {
        let kind = self.kind(id)?;
        Some(port_ref(kind, id, self.pin_item(id, side, index).as_ref()))
    }

    /// The links drawn as wires between these pins.
    fn links_between(&self, from: NodeId, to: NodeId, item: Option<&ItemId>) -> Vec<LinkId> {
        self.graph
            .links()
            .iter()
            .filter(|(_, link)| {
                link.from == from && link.to == to && item.is_none_or(|item| *item == link.item)
            })
            .map(|(id, _)| id)
            .collect()
    }

    /// Opens the picker from a port, placing the new node `PORT_GAP` away from it.
    fn open_picker_at_port(&mut self, id: NodeId, side: Side, item: Option<ItemId>, key: PinKey) {
        let (Some(item), Some(&center)) = (item, self.pins.borrow().get(&key)) else {
            return;
        };
        let offset = match side {
            Side::Input => -PORT_GAP,
            Side::Output => PORT_GAP,
        };
        self.events.push(CanvasEvent::OpenPicker {
            origin: Origin::Port {
                node: id,
                side,
                item,
            },
            graph_pos: center + vec2(offset, 0.0),
        });
    }

    fn port_row(
        &mut self,
        ui: &mut Ui,
        id: NodeId,
        side: Side,
        item: Option<&ItemId>,
        linked: bool,
    ) {
        let flow = item.and_then(|item| {
            let port = Port {
                node: id,
                item: item.clone(),
                side,
            };
            self.solution.port_flow.get(&port).copied().flatten()
        });
        let color = match (linked, side) {
            (true, _) => ui.visuals().text_color(),
            (false, Side::Input) => colors::DEPOT_IN,
            (false, Side::Output) => colors::DEPOT_OUT,
        };
        let throttled = self
            .solution
            .throttle
            .get(&id)
            .is_some_and(|reason| reason.side == side && Some(&reason.item) == item);
        let marker = |ui: &mut Ui, strings: &Strings| {
            if throttled {
                throttle_marker(ui).on_hover_text(strings.get("throttle-marker-hint"));
            }
        };
        let badge = |ui: &mut Ui, catalog: &Catalog| match item {
            Some(item) => {
                widgets::item_badge(ui, catalog, item.as_str(), BADGE);
            }
            None => {
                ui.label(self.strings.get("item-unset"));
            }
        };
        let rate = RichText::new(format::fixed(flow)).color(color).monospace();
        match side {
            Side::Input => {
                marker(ui, self.strings);
                badge(ui, self.catalog);
                ui.label(rate);
            }
            Side::Output => {
                ui.label(rate);
                badge(ui, self.catalog);
                marker(ui, self.strings);
            }
        }
    }

    fn pin_info(linked: bool, side: Side) -> PinInfo {
        let fill = match (linked, side) {
            (true, _) => colors::LINKED,
            (false, Side::Input) => colors::DEPOT_IN,
            (false, Side::Output) => colors::DEPOT_OUT,
        };
        PinInfo::circle()
            .with_fill(fill)
            .with_wire_color(colors::LINKED)
    }

    fn limit_line(&self, ui: &mut Ui, limit: Option<f64>) {
        let text = match limit {
            Some(limit) => self
                .strings
                .format("node-limit", &[("limit", &format::short(Some(limit)))]),
            None => self.strings.get("node-no-limit"),
        };
        ui.label(RichText::new(text).weak().small());
    }

    fn big_number(ui: &mut Ui, value: Option<f64>) {
        // A blank value still takes its line, so the card doesn't jump when it fills in.
        let text = format::fixed(value);
        ui.label(
            RichText::new(if text.is_empty() { " ".into() } else { text })
                .size(20.0)
                .strong(),
        );
    }
}

fn throttle_marker(ui: &mut Ui) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
    let points = vec![
        Pos2::new(rect.center().x, rect.top()),
        rect.right_bottom(),
        rect.left_bottom(),
    ];
    ui.painter().add(egui::Shape::convex_polygon(
        points,
        colors::WARN,
        Stroke::NONE,
    ));
    response
}

impl SnarlViewer<NodeId> for Viewer<'_> {
    fn title(&mut self, node: &NodeId) -> String {
        self.kind(*node)
            .map(|kind| names::node_title(self.catalog, self.strings, kind))
            .unwrap_or_default()
    }

    fn inputs(&mut self, node: &NodeId) -> usize {
        self.kind(*node)
            .map_or(0, |kind| pins(self.catalog, kind, Side::Input).len())
    }

    fn outputs(&mut self, node: &NodeId) -> usize {
        self.kind(*node)
            .map_or(0, |kind| pins(self.catalog, kind, Side::Output).len())
    }

    fn node_frame(
        &mut self,
        default: egui::Frame,
        node: ViewId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        snarl: &Snarl<NodeId>,
    ) -> egui::Frame {
        let frame = default.corner_radius(CornerRadius::same(6));
        if self.selection == Some(Selection::Node(snarl[node])) {
            frame.stroke(Stroke::new(2.0, colors::WARN))
        } else {
            frame
        }
    }

    fn has_node_style(
        &mut self,
        _node: ViewId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        _snarl: &Snarl<NodeId>,
    ) -> bool {
        true
    }

    /// Node cards size to their content: text never wraps inside them.
    fn apply_node_style(
        &mut self,
        style: &mut egui::Style,
        _node: ViewId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        _snarl: &Snarl<NodeId>,
    ) {
        style.wrap_mode = Some(egui::TextWrapMode::Extend);
    }

    fn header_frame(
        &mut self,
        default: egui::Frame,
        node: ViewId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        snarl: &Snarl<NodeId>,
    ) -> egui::Frame {
        let tint = match self.kind(snarl[node]) {
            Some(NodeKind::Recipe { .. }) => Color32::from_rgb(45, 60, 85),
            Some(NodeKind::Source { .. }) => Color32::from_rgb(40, 75, 50),
            Some(NodeKind::Sink { .. }) => Color32::from_rgb(85, 45, 70),
            Some(NodeKind::MissingRecipe { .. }) => Color32::from_rgb(90, 40, 40),
            _ => Color32::from_rgb(60, 60, 60),
        };
        default.fill(tint).corner_radius(CornerRadius::same(6))
    }

    fn show_header(
        &mut self,
        node: ViewId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        ui: &mut Ui,
        snarl: &mut Snarl<NodeId>,
    ) {
        let id = snarl[node];
        let Some(kind) = self.kind(id) else {
            return;
        };
        ui.vertical(|ui| {
            if !matches!(kind, NodeKind::Recipe { .. }) {
                ui.label(
                    RichText::new(self.strings.get(names::kind_key(kind)))
                        .small()
                        .weak(),
                );
            }
            ui.label(RichText::new(names::node_subject(self.catalog, self.strings, kind)).strong());
            if let NodeKind::Storage { .. } = kind {
                let balance = self.solution.storage.get(&id).and_then(|s| s.balance);
                if let Some(balance) = balance {
                    let color = if balance >= 0.0 {
                        colors::POSITIVE
                    } else {
                        colors::NEGATIVE
                    };
                    let mut text = format::fixed(Some(balance));
                    if balance >= 0.0 {
                        text.insert(0, '+');
                    }
                    ui.label(
                        RichText::new(
                            self.strings
                                .format("storage-balance", &[("balance", &text)]),
                        )
                        .color(color)
                        .strong(),
                    );
                }
            }
        });
    }

    #[allow(refining_impl_trait)]
    fn show_input(&mut self, pin: &InPin, ui: &mut Ui, snarl: &mut Snarl<NodeId>) -> RecordingPin {
        let id = snarl[pin.id.node];
        let item = self.pin_item(id, Side::Input, pin.id.input);
        let linked = !pin.remotes.is_empty();
        self.port_row(ui, id, Side::Input, item.as_ref(), linked);
        RecordingPin {
            info: Self::pin_info(linked, Side::Input),
            key: PinKey::In(pin.id),
            centers: self.pins.clone(),
        }
    }

    #[allow(refining_impl_trait)]
    fn show_output(
        &mut self,
        pin: &OutPin,
        ui: &mut Ui,
        snarl: &mut Snarl<NodeId>,
    ) -> RecordingPin {
        let id = snarl[pin.id.node];
        let item = self.pin_item(id, Side::Output, pin.id.output);
        let linked = !pin.remotes.is_empty();
        self.port_row(ui, id, Side::Output, item.as_ref(), linked);
        RecordingPin {
            info: Self::pin_info(linked, Side::Output),
            key: PinKey::Out(pin.id),
            centers: self.pins.clone(),
        }
    }

    fn has_body(&mut self, _node: &NodeId) -> bool {
        true
    }

    fn show_body(
        &mut self,
        node: ViewId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        ui: &mut Ui,
        snarl: &mut Snarl<NodeId>,
    ) {
        let id = snarl[node];
        let Some(kind) = self.kind(id).cloned() else {
            return;
        };
        let rate = self.solution.rate.get(&id).copied().flatten();
        ui.vertical_centered(|ui| match &kind {
            NodeKind::Recipe {
                recipe,
                facility,
                limit,
            } => {
                let facilities = self
                    .catalog
                    .recipe(recipe.as_str())
                    .map(|r| r.facilities.clone())
                    .unwrap_or_default();
                let name = names::facility(self.catalog, facility.as_str());
                if facilities.len() > 1 {
                    // One click moves to the next facility; the inspector has the full list.
                    if ui.small_button(name).clicked() {
                        let next = facilities
                            .iter()
                            .position(|f| f == facility)
                            .map_or(0, |i| (i + 1) % facilities.len());
                        self.events
                            .push(CanvasEvent::SetFacility(id, facilities[next].clone()));
                    }
                } else {
                    ui.label(RichText::new(name).small());
                }
                let count = self.solution.facility_count.get(&id).copied().flatten();
                Self::big_number(ui, count);
                self.limit_line(ui, *limit);
            }
            NodeKind::Source { limit, .. } | NodeKind::ControlPort { limit, .. } => {
                Self::big_number(ui, rate);
                self.limit_line(ui, Some(*limit));
            }
            NodeKind::Sink { limit, .. } => {
                Self::big_number(ui, rate);
                self.limit_line(ui, *limit);
            }
            NodeKind::Splitter { .. } | NodeKind::Merger { .. } => {
                Self::big_number(ui, rate);
            }
            NodeKind::Storage { .. } => {
                let through = self.solution.storage.get(&id).and_then(|s| s.pass_through);
                ui.label(
                    RichText::new(
                        self.strings
                            .format("storage-pass-through", &[("rate", &format::fixed(through))]),
                    )
                    .small(),
                );
            }
            NodeKind::MissingRecipe { .. } => {
                ui.label(
                    RichText::new(self.strings.get("kind-missing-recipe")).color(colors::NEGATIVE),
                );
            }
        });
    }

    fn final_node_rect(
        &mut self,
        node: ViewId,
        rect: Rect,
        ui: &mut Ui,
        snarl: &mut Snarl<NodeId>,
    ) {
        let id = snarl[node];
        // Move a node added from a port so the edge facing that port sits where it was dropped.
        if let Some(&anchor) = self.anchors.get(&id) {
            let dx = match anchor {
                Anchor::Left(x) => x - rect.left(),
                Anchor::Right(x) => x - rect.right(),
            };
            if dx.abs() < 0.5 {
                self.anchors.remove(&id);
            } else if let Some(info) = snarl.get_node_info_mut(node) {
                info.pos.x += dx;
                ui.ctx().request_repaint();
            }
        }
        let clicked_at = ui.input(|i| {
            i.pointer
                .primary_clicked()
                .then(|| i.pointer.interact_pos())
                .flatten()
        });
        if let Some(pos) = clicked_at
            && rect.contains(self.to_global.inverse() * pos)
        {
            self.events
                .push(CanvasEvent::Select(Some(Selection::Node(id))));
        }
    }

    fn has_node_menu(&mut self, _node: &NodeId) -> bool {
        true
    }

    fn show_node_menu(
        &mut self,
        node: ViewId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        ui: &mut Ui,
        snarl: &mut Snarl<NodeId>,
    ) {
        if ui.button(self.strings.get("node-remove")).clicked() {
            self.events.push(CanvasEvent::RemoveNode(snarl[node]));
            ui.close();
        }
    }

    fn has_dropped_wire_menu(&mut self, _src_pins: AnyPins, _snarl: &mut Snarl<NodeId>) -> bool {
        true
    }

    /// A link dragged from a port onto empty canvas opens the picker there.
    fn show_dropped_wire_menu(
        &mut self,
        pos: Pos2,
        ui: &mut Ui,
        src_pins: AnyPins,
        snarl: &mut Snarl<NodeId>,
    ) {
        let port = match src_pins {
            AnyPins::In(&[pin, ..]) => {
                let id = snarl[pin.node];
                Some((id, Side::Input, self.pin_item(id, Side::Input, pin.input)))
            }
            AnyPins::Out(&[pin, ..]) => {
                let id = snarl[pin.node];
                Some((
                    id,
                    Side::Output,
                    self.pin_item(id, Side::Output, pin.output),
                ))
            }
            _ => None,
        };
        let origin = match port {
            Some((node, side, Some(item))) => Origin::Port { node, side, item },
            _ => Origin::Canvas,
        };
        self.events.push(CanvasEvent::OpenPicker {
            origin,
            graph_pos: pos,
        });
        ui.close();
    }

    fn connect(&mut self, from: &OutPin, to: &InPin, snarl: &mut Snarl<NodeId>) {
        let (from_id, to_id) = (snarl[from.id.node], snarl[to.id.node]);
        if let (Some(from), Some(to)) = (
            self.port_ref(from_id, Side::Output, from.id.output),
            self.port_ref(to_id, Side::Input, to.id.input),
        ) {
            self.events.push(CanvasEvent::Connect { from, to });
        }
    }

    fn disconnect(&mut self, from: &OutPin, to: &InPin, snarl: &mut Snarl<NodeId>) {
        let (from_id, to_id) = (snarl[from.id.node], snarl[to.id.node]);
        let item = self.pin_item(from_id, Side::Output, from.id.output);
        for link in self.links_between(from_id, to_id, item.as_ref()) {
            self.events.push(CanvasEvent::Disconnect(link));
        }
    }

    /// Right-click on an input port: unlink it, or, if it has no links, add a producer of its item.
    fn drop_inputs(&mut self, pin: &InPin, snarl: &mut Snarl<NodeId>) {
        let id = snarl[pin.id.node];
        let item = self.pin_item(id, Side::Input, pin.id.input);
        if pin.remotes.is_empty() {
            self.open_picker_at_port(id, Side::Input, item, PinKey::In(pin.id));
            return;
        }
        for remote in &pin.remotes {
            for link in self.links_between(snarl[remote.node], id, item.as_ref()) {
                self.events.push(CanvasEvent::Disconnect(link));
            }
        }
    }

    /// Right-click on an output port: unlink it, or, if it has no links, add a consumer of its item.
    fn drop_outputs(&mut self, pin: &OutPin, snarl: &mut Snarl<NodeId>) {
        let id = snarl[pin.id.node];
        let item = self.pin_item(id, Side::Output, pin.id.output);
        if pin.remotes.is_empty() {
            self.open_picker_at_port(id, Side::Output, item, PinKey::Out(pin.id));
            return;
        }
        for remote in &pin.remotes {
            for link in self.links_between(id, snarl[remote.node], item.as_ref()) {
                self.events.push(CanvasEvent::Disconnect(link));
            }
        }
    }

    fn current_transform(&mut self, to_global: &mut TSTransform, _snarl: &mut Snarl<NodeId>) {
        // egui-snarl fits the view to its nodes the first time it sees the canvas. With no nodes
        // that fit divides by zero; start from the plain view instead.
        let usable = to_global.scaling.is_finite()
            && to_global.scaling > 0.0
            && to_global.translation.is_finite();
        if !usable {
            *to_global = TSTransform::IDENTITY;
        }
        *self.to_global = *to_global;
    }
}

/// Flow and belt count at the middle of every link, like `12.00 /min · 0.4 belts`.
struct LinkLabels<'a> {
    catalog: &'a Catalog,
    strings: &'a Strings,
    graph: &'a Graph,
    solution: &'a Solution,
    view: &'a Snarl<NodeId>,
    pins: &'a HashMap<PinKey, Pos2>,
    selection: Option<Selection>,
}

impl LinkLabels<'_> {
    /// Paints the labels on the canvas layer, in graph space, and returns a link whose label was
    /// clicked.
    fn paint(&self, ui: &Ui, layer: LayerId, to_global: TSTransform) -> Option<LinkId> {
        let painter = ui.ctx().layer_painter(layer);
        let clicked_at = ui.input(|i| {
            i.pointer
                .primary_clicked()
                .then(|| i.pointer.interact_pos())
                .flatten()
        });
        let clicked_at = clicked_at.map(|pos| to_global.inverse() * pos);
        let visuals = ui.visuals();
        let mut clicked = None;
        // egui-snarl keeps wires in a hash set; sort them so overlapping labels stack the same way
        // every frame.
        let mut wires: Vec<(OutPinId, InPinId)> = self.view.wires().collect();
        wires.sort();
        for (out_pin, in_pin) in wires {
            let (Some(&a), Some(&b)) = (
                self.pins.get(&PinKey::Out(out_pin)),
                self.pins.get(&PinKey::In(in_pin)),
            ) else {
                continue;
            };
            let from = self.view[out_pin.node];
            let to = self.view[in_pin.node];
            // Two nodes can be linked by more than one item; each wire belongs to its pin's item.
            let item = self
                .graph
                .node(from)
                .and_then(|node| {
                    pins(self.catalog, &node.kind, Side::Output)
                        .into_iter()
                        .nth(out_pin.output)
                })
                .flatten();
            for (id, link) in self.graph.links_at(from, Side::Output) {
                if link.to != to || item.as_ref().is_some_and(|item| *item != link.item) {
                    continue;
                }
                let fluid = self
                    .catalog
                    .item(link.item.as_str())
                    .is_some_and(|item| item.is_fluid);
                // An undetermined link shows no label at all.
                let Some(flow) = self.solution.link_flow.get(&id).copied().flatten() else {
                    continue;
                };
                let belts = self.solution.link_belts.get(&id).copied().flatten();
                let key = if fluid {
                    "link-label-fluid"
                } else {
                    "link-label"
                };
                let text = self.strings.format(
                    key,
                    &[
                        ("flow", &format::fixed(Some(flow))),
                        ("belts", &format::short(belts)),
                    ],
                );
                let over = belts.is_some_and(|b| b > 1.0 + 1e-9);
                let color = if over {
                    colors::WARN
                } else {
                    visuals.text_color()
                };
                let galley = painter.layout_no_wrap(text, FontId::proportional(12.0), color);
                let rect = Rect::from_center_size(a.lerp(b, 0.5), galley.size()).expand(3.0);
                let selected = self.selection == Some(Selection::Link(id));
                let stroke = if selected {
                    Stroke::new(1.5, colors::WARN)
                } else {
                    Stroke::NONE
                };
                painter.rect(
                    rect,
                    3.0,
                    visuals.extreme_bg_color.gamma_multiply(0.9),
                    stroke,
                    egui::StrokeKind::Outside,
                );
                painter.galley(rect.min + vec2(3.0, 3.0), galley, color);
                if clicked_at.is_some_and(|pos| rect.contains(pos)) {
                    clicked = Some(id);
                }
            }
        }
        clicked
    }
}
