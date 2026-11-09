//! The node canvas: egui-snarl drawing the [`Document`]'s graph with the solver's numbers.
//!
//! The viewer only reads the graph. Everything the player does on the canvas comes back as
//! [`CanvasEvent`]s, which the workspace applies to the document after the frame's canvas is drawn,
//! so the graph and its view always change together.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use aic_core::{
    Graph, GroupId, LinkId, Member, NodeId, NodeKind, Port, PortRef, RoundUp, Side, Solution,
};
use aic_data::{Catalog, FacilityId, ImageKind, ItemId};
use eframe::egui::{
    self, Color32, CornerRadius, FontId, Id, LayerId, Pos2, Rect, RichText, Sense, Stroke, Ui,
    emath::TSTransform, vec2,
};
use egui_snarl::ui::{
    AnyPins, BackgroundPattern, NodeLayout, PinInfo, PinPlacement, PinWireInfo, SnarlPin,
    SnarlStyle, SnarlViewer, SnarlWidget, WireStyle,
};
use egui_snarl::{InPin, InPinId, OutPin, OutPinId, Snarl};

use crate::document::{Anchor, Document, PinTarget, ViewId, pin_targets, port_ref};
use crate::format;
use crate::i18n::Strings;
use crate::names;
use crate::picker::Origin;
use crate::widgets::{self, ItemPicture, LimitField, SINK_IMAGE, SOURCE_IMAGE, colors};

const CANVAS_ID: &str = "aic-canvas";
/// How far from a port a node added from it is placed, in graph units.
const PORT_GAP: f32 = 80.0;
/// Size of the facility picture on a recipe card (and the depot picture on a source or sink).
const FACILITY_IMAGE: f32 = 64.0;
/// Size of a port's item icon. The icon is the port's pin: it sits inside the card against its
/// edge, and a link is dragged from it.
const PIN_ICON: f32 = 32.0;
/// Room for a port's rate, like `100.00`.
const RATE_WIDTH: f32 = 54.0;
/// Space between a port's icon and its rate.
const RATE_GAP: f32 = 4.0;
/// The card's inner margin.
const NODE_MARGIN: i8 = 6;
/// Every card is this wide, whatever it shows, so cards line up.
const CARD_WIDTH: f32 = 300.0;
/// How far a port column reaches into the card, from inside its margin.
const PORT_COLUMN: f32 = PIN_ICON - NODE_MARGIN as f32 + RATE_GAP + RATE_WIDTH;

/// What the player has selected: nodes and collapsed groups as drawn on the canvas, or one link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    /// Never empty.
    Items(Vec<Member>),
    Link(LinkId),
}

impl Selection {
    pub fn node(id: NodeId) -> Self {
        Self::Items(vec![Member::Node(id)])
    }

    /// Some items, or `None` for none.
    pub fn of_items(items: Vec<Member>) -> Option<Self> {
        (!items.is_empty()).then_some(Self::Items(items))
    }

    /// Some nodes, or `None` for none.
    pub fn of_nodes(nodes: Vec<NodeId>) -> Option<Self> {
        Self::of_items(nodes.into_iter().map(Member::Node).collect())
    }

    pub fn items(&self) -> &[Member] {
        match self {
            Self::Items(items) => items,
            Self::Link(_) => &[],
        }
    }

    /// The selected nodes, leaving out groups.
    pub fn nodes(&self) -> Vec<NodeId> {
        self.items()
            .iter()
            .filter_map(|m| match m {
                Member::Node(id) => Some(*id),
                Member::Group(_) => None,
            })
            .collect()
    }

    pub fn contains(&self, member: Member) -> bool {
        self.items().contains(&member)
    }

    pub fn contains_node(&self, id: NodeId) -> bool {
        self.contains(Member::Node(id))
    }

    /// The node, when exactly one item is selected and it is a node.
    pub fn single_node(&self) -> Option<NodeId> {
        match self.items() {
            [Member::Node(one)] => Some(*one),
            _ => None,
        }
    }

    /// The group, when exactly one item is selected and it is a collapsed group.
    pub fn single_group(&self) -> Option<GroupId> {
        match self.items() {
            [Member::Group(one)] => Some(*one),
            _ => None,
        }
    }
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
    /// Shift+click on a node or group: add it to the selection, or take it out.
    Toggle(Member),
    /// Remove a node, or a group with everything in it.
    Remove(Member),
    /// Collapse a group into one card, or expand it.
    SetCollapsed(GroupId, bool),
    Ungroup(GroupId),
    SetFacility(NodeId, FacilityId),
    SetLimit(NodeId, Option<f64>),
    /// Open the port menu (a double-click on a port). `graph_pos` is the port's pin: the menu opens
    /// beside it, away from the node.
    OpenPortMenu {
        port: Port,
        graph_pos: Pos2,
    },
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
    /// What the player is typing in each card's limit field.
    limit_edits: HashMap<NodeId, String>,
    /// Graph space to screen space, as of the last frame.
    pub to_global: TSTransform,
    /// The canvas area on screen, as of the last frame.
    pub rect: Rect,
    /// Where each node and collapsed group was drawn last frame, in graph space.
    node_rects: HashMap<Member, Rect>,
    /// egui-snarl's own node selection as of the last frame. It changes on a box selection
    /// (Shift+drag on the background), which then becomes the player's selection.
    snarl_selection: Vec<ViewId>,
    /// What the player is dragging, found when it first moves and kept until the button is up.
    grabbed: Option<Member>,
    /// A view to show from the next frame, set when a file opens.
    pending_view: Option<aic_core::CanvasView>,
}

impl Default for Canvas {
    fn default() -> Self {
        Self {
            style: SnarlStyle {
                node_layout: Some(NodeLayout::coil()),
                pin_placement: Some(PinPlacement::Edge),
                pin_size: Some(PIN_ICON),
                wire_width: Some(2.0),
                collapsible: Some(false),
                bg_pattern: Some(BackgroundPattern::grid(vec2(40.0, 40.0), 0.0)),
                // The canvas draws the player's selection itself; egui-snarl's own is only used
                // for box selection.
                select_style: Some(egui_snarl::ui::SelectionStyle {
                    margin: egui::Margin::ZERO,
                    rounding: CornerRadius::ZERO,
                    fill: Color32::TRANSPARENT,
                    stroke: Stroke::NONE,
                }),
                ..SnarlStyle::new()
            },
            pins: PinCenters::default(),
            limit_edits: HashMap::new(),
            to_global: TSTransform::IDENTITY,
            rect: Rect::ZERO,
            node_rects: HashMap::new(),
            snarl_selection: Vec::new(),
            grabbed: None,
            pending_view: None,
        }
    }
}

impl Canvas {
    /// Where the canvas is looking: the graph point at its top-left, and the zoom. `None` before
    /// the canvas was first drawn.
    pub fn view(&self) -> Option<aic_core::CanvasView> {
        if !self.rect.is_positive() {
            return None;
        }
        let corner = self.to_global.inverse() * self.rect.min;
        Some(aic_core::CanvasView {
            x: corner.x,
            y: corner.y,
            zoom: self.to_global.scaling,
        })
    }

    /// Looks at `view` from the next frame on, as when a file is opened.
    pub fn show_view(&mut self, view: aic_core::CanvasView) {
        self.pending_view = Some(view);
    }

    /// The middle of the visible canvas, in graph space.
    pub fn center(&self) -> Pos2 {
        self.to_global.inverse() * self.rect.center()
    }

    pub fn to_screen(&self, graph_pos: Pos2) -> Pos2 {
        self.to_global * graph_pos
    }

    /// Where a node or collapsed group was drawn last frame, on screen.
    pub fn item_on_screen(&self, member: Member) -> Option<Rect> {
        let rect = self.node_rects.get(&member)?;
        Some(Rect::from_min_max(
            self.to_screen(rect.min),
            self.to_screen(rect.max),
        ))
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
        let member = doc.shown(node);
        let view_id = doc.view_id(member)?;
        let index = pin_targets(doc.graph(), catalog, member, side)
            .iter()
            .position(|pin| pin.is(node, item))?;
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
        selection: Option<&Selection>,
        round_up: bool,
    ) -> Vec<CanvasEvent> {
        // The snarl response spans everything; the panel's own rect is what's on screen.
        self.rect = ui.max_rect();
        let (graph, view, anchors, solution) = doc.parts();
        let before: HashMap<ViewId, Pos2> = view
            .nodes_pos_ids()
            .filter(|(_, _, member)| match member {
                Member::Node(id) => !anchors.contains_key(id),
                Member::Group(_) => true,
            })
            .map(|(view_id, pos, _)| (view_id, pos))
            .collect();
        let mut viewer = Viewer {
            catalog,
            strings,
            graph,
            solution,
            selection,
            round_up,
            anchors,
            limit_edits: &mut self.limit_edits,
            pins: &self.pins,
            to_global: &mut self.to_global,
            pending_view: self.pending_view.take().map(|view| {
                // Screen = zoom × graph + translation, with the view's corner at the canvas corner.
                let corner = Pos2::new(view.x, view.y).to_vec2() * view.zoom;
                TSTransform::new(self.rect.min.to_vec2() - corner, view.zoom)
            }),
            node_rects: &mut self.node_rects,
            pin_cache: HashMap::new(),
            on_group: false,
            events: Vec::new(),
        };
        let response = SnarlWidget::new()
            .id(Id::new(CANVAS_ID))
            .style(self.style)
            .show(view, &mut viewer, ui);
        let mut events = viewer.events;
        let node_clicked = events
            .iter()
            .any(|e| matches!(e, CanvasEvent::Select(_) | CanvasEvent::Toggle(_)));
        self.move_selection_together(ui, view, &before, selection);
        if let Some(event) = self.box_selection(ui, view, selection, node_clicked) {
            events.push(event);
        }

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
        let clicked_label = labels.paint(ui, layer, self.to_global, self.rect);
        let selected = events.iter().any(|e| matches!(e, CanvasEvent::Select(_)));
        let modifiers = ui.input(|i| i.modifiers);
        if let Some(link) = clicked_label {
            events.push(CanvasEvent::Select(Some(Selection::Link(link))));
        } else if response.clicked() && !selected && !modifiers.shift && !modifiers.command {
            events.push(CanvasEvent::Select(None));
        }
        // Only a real right click: egui-snarl opens its dropped-link menu by marking the canvas
        // long-touched, which also counts as a secondary click. The response is on the canvas's
        // zoomed and panned layer, so the click's position comes from the input, on screen.
        if response.clicked_by(egui::PointerButton::Secondary)
            && let Some(pos) = ui.input(|i| i.pointer.interact_pos())
        {
            events.push(CanvasEvent::OpenPicker {
                origin: Origin::Canvas,
                graph_pos: self.to_global.inverse() * pos,
            });
        }
        events
    }
}

impl Canvas {
    /// egui-snarl drags its own selection together, which can differ from the player's. Whatever
    /// snarl moved, move the player's selection with the grabbed node instead, or the grabbed node
    /// alone when it isn't selected.
    fn move_selection_together(
        &mut self,
        ui: &Ui,
        view: &mut Snarl<Member>,
        before: &HashMap<ViewId, Pos2>,
        selection: Option<&Selection>,
    ) {
        if !ui.input(|i| i.pointer.primary_down()) {
            self.grabbed = None;
        }
        let moved: Vec<ViewId> = view
            .nodes_pos_ids()
            .filter(|(view_id, pos, _)| before.get(view_id).is_some_and(|old| old != pos))
            .map(|(view_id, _, _)| view_id)
            .collect();
        if moved.is_empty() {
            return;
        }
        if self.grabbed.is_none() {
            // The first move: last frame's rects still hold the point where the drag began.
            let Some(press) = ui.input(|i| i.pointer.press_origin()) else {
                return;
            };
            let press = self.to_global.inverse() * press;
            self.grabbed = moved.iter().map(|view_id| view[*view_id]).find(|id| {
                self.node_rects
                    .get(id)
                    .is_some_and(|rect| rect.contains(press))
            });
        }
        let Some(grabbed_id) = self.grabbed else {
            return;
        };
        let Some(grabbed) = moved.iter().copied().find(|v| view[*v] == grabbed_id) else {
            return;
        };
        let delta = view
            .get_node_info(grabbed)
            .map(|n| n.pos)
            .unwrap_or_default()
            - before[&grabbed];
        let targets: Vec<Member> = match selection {
            Some(selection) if selection.contains(grabbed_id) => selection.items().to_vec(),
            _ => vec![grabbed_id],
        };
        let ids: Vec<(ViewId, Member)> = view
            .nodes_pos_ids()
            .map(|(view_id, _, &id)| (view_id, id))
            .collect();
        for (view_id, id) in ids {
            let Some(&old) = before.get(&view_id) else {
                continue;
            };
            if let Some(info) = view.get_node_info_mut(view_id) {
                info.pos = if targets.contains(&id) {
                    old + delta
                } else {
                    old
                };
            }
        }
    }

    /// A box selection drawn this frame, as the player's new selection: Shift adds to it,
    /// Cmd/Ctrl takes away from it.
    fn box_selection(
        &mut self,
        ui: &Ui,
        view: &Snarl<Member>,
        selection: Option<&Selection>,
        node_clicked: bool,
    ) -> Option<CanvasEvent> {
        let now = egui_snarl::ui::get_selected_nodes(Id::new(CANVAS_ID), ui.ctx());
        let last = std::mem::replace(&mut self.snarl_selection, now.clone());
        if now == last || node_clicked {
            return None;
        }
        let to_ids = |list: &[ViewId]| -> Vec<Member> {
            list.iter()
                .filter_map(|view_id| view.get_node(*view_id).copied())
                .collect()
        };
        let modifiers = ui.input(|i| i.modifiers);
        let current = selection.map(|s| s.items().to_vec()).unwrap_or_default();
        let nodes = if modifiers.command {
            let removed = to_ids(&last);
            let kept = to_ids(&now);
            current
                .into_iter()
                .filter(|id| kept.contains(id) || !removed.contains(id))
                .collect()
        } else if modifiers.shift {
            let mut nodes = current;
            for id in to_ids(&now) {
                if !nodes.contains(&id) {
                    nodes.push(id);
                }
            }
            nodes
        } else {
            to_ids(&now)
        };
        Some(CanvasEvent::Select(Selection::of_items(nodes)))
    }
}

/// A port's pin: its item's icon inside the card against the edge, on a tile colored like the
/// port (linked, or pulling from or sending to the depot). It records where snarl drew it.
struct RecordingPin {
    /// `None` for a logistics node's port before its first link: a plain circle.
    picture: Option<ItemPicture>,
    side: Side,
    linked: bool,
    throttled: bool,
    key: PinKey,
    centers: PinCenters,
}

impl RecordingPin {
    fn fill(&self) -> Color32 {
        match (self.linked, self.side) {
            (true, _) => colors::LINKED,
            (false, Side::Input) => colors::DEPOT_IN,
            (false, Side::Output) => colors::DEPOT_OUT,
        }
    }
}

impl SnarlPin for RecordingPin {
    /// Inside the card, flush with its edge; snarl gives `x` as the edge.
    fn pin_rect(&self, x: f32, y0: f32, y1: f32, size: f32) -> Rect {
        let y = (y0 + y1) * 0.5;
        let left = match self.side {
            Side::Input => x,
            Side::Output => x - size,
        };
        Rect::from_min_size(Pos2::new(left, y - size * 0.5), vec2(size, size))
    }

    fn draw(
        self,
        snarl_style: &SnarlStyle,
        style: &egui::Style,
        rect: Rect,
        painter: &egui::Painter,
    ) -> PinWireInfo {
        self.centers.borrow_mut().insert(self.key, rect.center());
        let fill = self.fill();
        let Some(picture) = &self.picture else {
            return PinInfo::circle()
                .with_fill(fill)
                .with_wire_color(colors::LINKED)
                .draw(snarl_style, style, rect.shrink(rect.width() * 0.3), painter);
        };
        if rect.is_finite() {
            // Links run behind cards, so a link seems to end at the card's edge, at this tile.
            painter.rect_filled(rect, 4.0, fill.gamma_multiply(0.35));
            picture.paint(painter, rect.shrink(3.0));
            if self.throttled {
                paint_throttle_marker(painter, rect);
            }
        }
        PinWireInfo {
            color: colors::LINKED,
            style: snarl_style.wire_style.unwrap_or(WireStyle::Bezier5),
        }
    }
}

/// A small warning triangle in the top corner of a port's tile.
fn paint_throttle_marker(painter: &egui::Painter, tile: Rect) {
    let rect = Rect::from_min_size(tile.right_top() - vec2(11.0, -1.0), vec2(10.0, 10.0));
    let points = vec![
        Pos2::new(rect.center().x, rect.top()),
        rect.right_bottom(),
        rect.left_bottom(),
    ];
    painter.add(egui::Shape::convex_polygon(
        points,
        colors::WARN,
        Stroke::NONE,
    ));
}

struct Viewer<'a> {
    catalog: &'a Catalog,
    strings: &'a Strings,
    graph: &'a Graph,
    solution: &'a Solution,
    selection: Option<&'a Selection>,
    /// Show whole facilities and how busy they are.
    round_up: bool,
    anchors: &'a mut HashMap<NodeId, Anchor>,
    limit_edits: &'a mut HashMap<NodeId, String>,
    pins: &'a PinCenters,
    to_global: &'a mut TSTransform,
    /// A view to switch to this frame.
    pending_view: Option<TSTransform>,
    node_rects: &'a mut HashMap<Member, Rect>,
    /// Each drawn item's pins this frame; a group's take a walk over the graph to find.
    pin_cache: HashMap<(Member, Side), Vec<PinTarget>>,
    /// Whether the pin being drawn is on a collapsed group's card.
    on_group: bool,
    events: Vec<CanvasEvent>,
}

impl Viewer<'_> {
    fn kind(&self, id: NodeId) -> Option<&NodeKind> {
        self.graph.node(id).map(|node| &node.kind)
    }

    fn pins_of(&mut self, member: Member, side: Side) -> &[PinTarget] {
        let (graph, catalog) = (self.graph, self.catalog);
        self.pin_cache
            .entry((member, side))
            .or_insert_with(|| pin_targets(graph, catalog, member, side))
    }

    /// The port a pin stands for: a node and its item, `None` for a logistics node without one.
    fn target(&mut self, member: Member, side: Side, index: usize) -> Option<PinTarget> {
        self.pins_of(member, side).get(index).cloned()
    }

    fn port_ref(&mut self, member: Member, side: Side, index: usize) -> Option<PortRef> {
        let target = self.target(member, side, index)?;
        let kind = self.kind(target.node)?;
        Some(port_ref(kind, target.node, target.item.as_ref()))
    }

    /// The links drawn as the wire between these two pins.
    fn links_between(&self, from: &PinTarget, to: &PinTarget) -> Vec<LinkId> {
        self.graph
            .links()
            .iter()
            .filter(|(_, link)| from.is(link.from, &link.item) && to.is(link.to, &link.item))
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

    /// A port's rate beside its pin, in a slot as wide on every card. Returns whether the solver
    /// throttled the node at this port.
    fn port_row(
        &mut self,
        ui: &mut Ui,
        pin: PinKey,
        id: NodeId,
        side: Side,
        item: Option<&ItemId>,
        linked: bool,
    ) -> bool {
        let flow = item.and_then(|item| {
            let port = Port {
                node: id,
                item: item.clone(),
                side,
            };
            // A copies group's card shows the flow of all its copies.
            let group_flow = self
                .solution
                .group_flow
                .get(&port)
                .filter(|_| self.on_group);
            group_flow
                .or_else(|| self.solution.port_flow.get(&port))
                .copied()
                .flatten()
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
        // The column starts inside the card's margin; snarl has already stepped past part of the
        // pin. Fill the rest of the column so every card is as wide.
        let column = ui.max_rect();
        let used = match side {
            Side::Input => ui.cursor().left() - column.left(),
            Side::Output => column.right() - ui.cursor().right(),
        };
        let rect = match side {
            Side::Input => Rect::from_min_max(
                Pos2::new(column.left() + PORT_COLUMN - RATE_WIDTH, column.top()),
                Pos2::new(column.left() + PORT_COLUMN, column.top() + PIN_ICON),
            ),
            Side::Output => Rect::from_min_max(
                Pos2::new(column.right() - PORT_COLUMN, column.top()),
                Pos2::new(
                    column.right() - PORT_COLUMN + RATE_WIDTH,
                    column.top() + PIN_ICON,
                ),
            ),
        };
        ui.allocate_exact_size(
            vec2((PORT_COLUMN - used).max(0.0), PIN_ICON),
            Sense::hover(),
        );
        let align = match side {
            Side::Input => egui::Align2::LEFT_CENTER,
            Side::Output => egui::Align2::RIGHT_CENTER,
        };
        // Right beside the icon, where snarl drew it last frame.
        let beside = PIN_ICON * 0.5 + RATE_GAP;
        let anchor = match (side, self.pins.borrow().get(&pin)) {
            (Side::Input, Some(center)) => Pos2::new(center.x + beside, rect.center().y),
            (Side::Output, Some(center)) => Pos2::new(center.x - beside, rect.center().y),
            (Side::Input, None) => rect.left_center(),
            (Side::Output, None) => rect.right_center(),
        };
        ui.painter().text(
            anchor,
            align,
            format::fixed(flow),
            FontId::monospace(13.0),
            color,
        );
        let Some(item) = item else {
            return throttled;
        };
        let mut hint = names::item(self.catalog, item.as_str()).to_owned();
        if throttled {
            hint.push('\n');
            hint.push_str(&self.strings.get("throttle-marker-hint"));
        }
        // A double-click on the port opens its menu: the limit as a rate, and clearing its links.
        let response = ui
            .interact(rect, Id::new(("port-row", id, side, item)), Sense::click())
            .on_hover_text(hint);
        // egui counts a quick third click as a triple click, not a double one; either opens it.
        if response.double_clicked() || response.triple_clicked() {
            // Beside the pin, on the node's edge; last frame's spot is close enough.
            let pin_center = self.pins.borrow().get(&pin).copied();
            let graph_pos = pin_center.unwrap_or(match side {
                Side::Input => rect.left_center(),
                Side::Output => rect.right_center(),
            });
            self.events.push(CanvasEvent::OpenPortMenu {
                port: Port {
                    node: id,
                    item: item.clone(),
                    side,
                },
                graph_pos,
            });
        }
        throttled
    }

    /// The port's pin, showing its item.
    fn pin(
        &self,
        ui: &Ui,
        key: PinKey,
        side: Side,
        item: Option<&ItemId>,
        linked: bool,
        throttled: bool,
    ) -> RecordingPin {
        RecordingPin {
            picture: item.map(|item| ItemPicture::of(ui.ctx(), self.catalog, item.as_str())),
            side,
            linked,
            throttled,
            key,
            centers: self.pins.clone(),
        }
    }

    /// The node's limit under its numbers, typed straight into the card. A source or control port
    /// needs one, so blanking theirs changes nothing.
    fn limit_field(&mut self, ui: &mut Ui, id: NodeId, limit: Option<f64>, required: bool) {
        let buffer = self.limit_edits.entry(id).or_default();
        let field = LimitField {
            id: Id::new(("node-limit", id)),
            buffer,
            current: limit,
            width: 76.0,
            hint: self.strings.get("node-no-limit"),
            label: self.strings.get("node-limit-label"),
        };
        if let (_, Some(typed)) = field.show(ui)
            && (typed.is_some() || !required)
        {
            self.events.push(CanvasEvent::SetLimit(id, typed));
        }
    }

    /// A collapsed group's card: how many nodes it holds, and a button to open it up.
    fn group_body(&mut self, ui: &mut Ui, group: GroupId) {
        let count = self.graph.nodes_in_group(group).len();
        ui.vertical_centered(|ui| {
            if let Some(&copies) = self.solution.copies.get(&group) {
                Self::big_number(ui, copies);
                ui.label(RichText::new(self.strings.get("group-copies")).small());
            }
            ui.label(RichText::new(crate::names::node_count(self.strings, count)).small());
            if ui.small_button(self.strings.get("group-expand")).clicked() {
                self.events.push(CanvasEvent::SetCollapsed(group, false));
            }
        });
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

/// A card's title, cut short with an ellipsis to fit the card; the whole title shows on hover.
fn title_label(ui: &mut Ui, text: RichText) {
    let width =
        CARD_WIDTH - 2.0 * f32::from(NODE_MARGIN) - 2.0 * ui.spacing().window_margin.leftf();
    ui.set_max_width(width);
    ui.add(egui::Label::new(text).truncate());
}

/// The width left for a card's body between its port columns, so every card is `CARD_WIDTH` wide.
fn body_width(ui: &Ui, inputs: &[InPin], outputs: &[OutPin]) -> f32 {
    let columns = [inputs.is_empty(), outputs.is_empty()]
        .iter()
        .filter(|empty| !**empty)
        .count() as f32;
    let spacing = ui.spacing().item_spacing.x;
    CARD_WIDTH - 2.0 * f32::from(NODE_MARGIN) - columns * PORT_COLUMN - 2.0 * spacing
}

impl SnarlViewer<Member> for Viewer<'_> {
    fn title(&mut self, member: &Member) -> String {
        match *member {
            Member::Node(id) => self
                .kind(id)
                .map(|kind| names::node_title(self.catalog, self.strings, kind))
                .unwrap_or_default(),
            Member::Group(id) => self
                .graph
                .group_info(id)
                .map(|g| g.name.clone())
                .unwrap_or_default(),
        }
    }

    fn inputs(&mut self, member: &Member) -> usize {
        self.pins_of(*member, Side::Input).len()
    }

    fn outputs(&mut self, member: &Member) -> usize {
        self.pins_of(*member, Side::Output).len()
    }

    fn node_frame(
        &mut self,
        default: egui::Frame,
        node: ViewId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        snarl: &Snarl<Member>,
    ) -> egui::Frame {
        let frame = default
            .corner_radius(CornerRadius::same(6))
            .inner_margin(egui::Margin::same(NODE_MARGIN));
        if self.selection.is_some_and(|s| s.contains(snarl[node])) {
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
        _snarl: &Snarl<Member>,
    ) -> bool {
        true
    }

    /// Node cards size to their content: text never wraps inside them. Their text can't be
    /// selected, so a drag on it moves the node.
    fn apply_node_style(
        &mut self,
        style: &mut egui::Style,
        _node: ViewId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        _snarl: &Snarl<Member>,
    ) {
        style.wrap_mode = Some(egui::TextWrapMode::Extend);
        style.interaction.selectable_labels = false;
    }

    fn header_frame(
        &mut self,
        default: egui::Frame,
        node: ViewId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        snarl: &Snarl<Member>,
    ) -> egui::Frame {
        let kind = match snarl[node] {
            Member::Node(id) => self.kind(id),
            Member::Group(_) => {
                return default
                    .fill(Color32::from_rgb(70, 55, 90))
                    .corner_radius(CornerRadius::same(6));
            }
        };
        let tint = match kind {
            Some(NodeKind::Recipe { .. }) => Color32::from_rgb(45, 60, 85),
            Some(NodeKind::Source { .. }) => Color32::from_rgb(40, 75, 50),
            Some(NodeKind::Sink { .. }) => Color32::from_rgb(85, 45, 70),
            Some(NodeKind::OutpostOrder { .. }) => Color32::from_rgb(90, 70, 35),
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
        snarl: &mut Snarl<Member>,
    ) {
        let id = match snarl[node] {
            Member::Node(id) => id,
            Member::Group(group) => {
                let name = self.title(&Member::Group(group));
                ui.vertical(|ui| {
                    ui.label(RichText::new(self.strings.get("kind-group")).small().weak());
                    title_label(ui, RichText::new(name).strong());
                });
                return;
            }
        };
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
            let subject = names::node_subject(self.catalog, self.strings, kind);
            title_label(ui, RichText::new(subject).strong());
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
    fn show_input(&mut self, pin: &InPin, ui: &mut Ui, snarl: &mut Snarl<Member>) -> RecordingPin {
        let linked = !pin.remotes.is_empty();
        let key = PinKey::In(pin.id);
        self.on_group = matches!(snarl[pin.id.node], Member::Group(_));
        let target = self.target(snarl[pin.id.node], Side::Input, pin.id.input);
        let item = target.as_ref().and_then(|t| t.item.clone());
        let throttled = target.is_some_and(|target| {
            self.port_row(
                ui,
                key,
                target.node,
                Side::Input,
                target.item.as_ref(),
                linked,
            )
        });
        self.pin(ui, key, Side::Input, item.as_ref(), linked, throttled)
    }

    #[allow(refining_impl_trait)]
    fn show_output(
        &mut self,
        pin: &OutPin,
        ui: &mut Ui,
        snarl: &mut Snarl<Member>,
    ) -> RecordingPin {
        let linked = !pin.remotes.is_empty();
        let key = PinKey::Out(pin.id);
        self.on_group = matches!(snarl[pin.id.node], Member::Group(_));
        let target = self.target(snarl[pin.id.node], Side::Output, pin.id.output);
        let item = target.as_ref().and_then(|t| t.item.clone());
        let throttled = target.is_some_and(|target| {
            self.port_row(
                ui,
                key,
                target.node,
                Side::Output,
                target.item.as_ref(),
                linked,
            )
        });
        self.pin(ui, key, Side::Output, item.as_ref(), linked, throttled)
    }

    fn has_body(&mut self, _node: &Member) -> bool {
        true
    }

    fn show_body(
        &mut self,
        node: ViewId,
        inputs: &[InPin],
        outputs: &[OutPin],
        ui: &mut Ui,
        snarl: &mut Snarl<Member>,
    ) {
        let id = match snarl[node] {
            Member::Node(id) => id,
            Member::Group(group) => {
                ui.set_width(body_width(ui, inputs, outputs));
                self.group_body(ui, group);
                return;
            }
        };
        let Some(kind) = self.kind(id).cloned() else {
            return;
        };
        let rate = self.solution.rate.get(&id).copied().flatten();
        let width = body_width(ui, inputs, outputs);
        ui.set_width(width);
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
                if let Some(image) = widgets::image(
                    ui,
                    self.catalog,
                    ImageKind::Facility,
                    facility.as_str(),
                    FACILITY_IMAGE,
                ) {
                    image.on_hover_text(name);
                }
                if facilities.len() > 1 {
                    // One click moves to the next facility; the inspector has the full list.
                    let button = egui::Button::new(RichText::new(name).small()).truncate();
                    if ui.add(button).clicked() {
                        let next = facilities
                            .iter()
                            .position(|f| f == facility)
                            .map_or(0, |i| (i + 1) % facilities.len());
                        self.events
                            .push(CanvasEvent::SetFacility(id, facilities[next].clone()));
                    }
                } else {
                    ui.add(egui::Label::new(RichText::new(name).small()).truncate());
                }
                let count = self.solution.facility_count.get(&id).copied().flatten();
                match count.filter(|_| self.round_up).map(RoundUp::of) {
                    Some(rounded) => {
                        ui.label(
                            RichText::new(rounded.facilities.to_string())
                                .size(20.0)
                                .strong(),
                        );
                        let percent = format::short(Some(rounded.utilization * 100.0));
                        ui.label(
                            RichText::new(
                                self.strings
                                    .format("round-up-utilization", &[("percent", &percent)]),
                            )
                            .small(),
                        );
                    }
                    None => Self::big_number(ui, count),
                }
                self.limit_field(ui, id, *limit, false);
            }
            NodeKind::Source { limit, .. } => {
                widgets::image(
                    ui,
                    self.catalog,
                    ImageKind::Node,
                    SOURCE_IMAGE,
                    FACILITY_IMAGE,
                );
                Self::big_number(ui, rate);
                self.limit_field(ui, id, Some(*limit), true);
            }
            NodeKind::ControlPort { limit, .. } => {
                Self::big_number(ui, rate);
                self.limit_field(ui, id, Some(*limit), true);
            }
            NodeKind::Sink { limit, .. } => {
                widgets::image(
                    ui,
                    self.catalog,
                    ImageKind::Node,
                    SINK_IMAGE,
                    FACILITY_IMAGE,
                );
                Self::big_number(ui, rate);
                self.limit_field(ui, id, *limit, false);
            }
            NodeKind::OutpostOrder { limit, .. } => {
                Self::big_number(ui, rate);
                for (coupon, earned) in self.solution.coupons.get(&id).into_iter().flatten() {
                    ui.horizontal(|ui| {
                        widgets::item_icon(ui, self.catalog, coupon.as_str(), 16.0);
                        let text = self
                            .strings
                            .format("outpost-earns", &[("rate", &format::fixed(Some(*earned)))]);
                        ui.label(RichText::new(text).small());
                    });
                }
                self.limit_field(ui, id, *limit, false);
            }
            NodeKind::Splitter { .. } | NodeKind::Merger { .. } => {
                Self::big_number(ui, rate);
            }
            NodeKind::Storage { .. } => {
                let through = self.solution.storage.get(&id).and_then(|s| s.pass_through);
                let text = self
                    .strings
                    .format("storage-pass-through", &[("rate", &format::fixed(through))]);
                ui.add(egui::Label::new(RichText::new(text).small()).truncate());
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
        snarl: &mut Snarl<Member>,
    ) {
        let member = snarl[node];
        // Move a node added from a port so the edge facing that port sits where it was dropped.
        if let Member::Node(id) = member
            && let Some(&anchor) = self.anchors.get(&id)
        {
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
        self.node_rects.insert(member, rect);
        let (clicked_at, modifiers) = ui.input(|i| {
            let at = i
                .pointer
                .primary_clicked()
                .then(|| i.pointer.interact_pos())
                .flatten();
            (at, i.modifiers)
        });
        if let Some(pos) = clicked_at
            && rect.contains(self.to_global.inverse() * pos)
        {
            self.events.push(if modifiers.shift {
                CanvasEvent::Toggle(member)
            } else {
                CanvasEvent::Select(Selection::of_items(vec![member]))
            });
        }
    }

    fn has_node_menu(&mut self, _node: &Member) -> bool {
        true
    }

    fn show_node_menu(
        &mut self,
        node: ViewId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        ui: &mut Ui,
        snarl: &mut Snarl<Member>,
    ) {
        let member = snarl[node];
        match member {
            Member::Node(id) => {
                if let Some(group) = self.graph.node(id).and_then(|n| n.group)
                    && ui.button(self.strings.get("group-collapse")).clicked()
                {
                    self.events.push(CanvasEvent::SetCollapsed(group, true));
                    ui.close();
                }
            }
            Member::Group(group) => {
                if ui.button(self.strings.get("group-expand")).clicked() {
                    self.events.push(CanvasEvent::SetCollapsed(group, false));
                    ui.close();
                }
                if ui.button(self.strings.get("group-ungroup")).clicked() {
                    self.events.push(CanvasEvent::Ungroup(group));
                    ui.close();
                }
            }
        }
        let remove = match member {
            Member::Node(_) => "node-remove",
            Member::Group(_) => "group-remove",
        };
        if ui.button(self.strings.get(remove)).clicked() {
            self.events.push(CanvasEvent::Remove(member));
            ui.close();
        }
    }

    fn has_dropped_wire_menu(&mut self, _src_pins: AnyPins, _snarl: &mut Snarl<Member>) -> bool {
        true
    }

    /// A link dragged from a port onto empty canvas opens the picker there.
    fn show_dropped_wire_menu(
        &mut self,
        pos: Pos2,
        ui: &mut Ui,
        src_pins: AnyPins,
        snarl: &mut Snarl<Member>,
    ) {
        let port = match src_pins {
            AnyPins::In(&[pin, ..]) => self
                .target(snarl[pin.node], Side::Input, pin.input)
                .map(|t| (t, Side::Input)),
            AnyPins::Out(&[pin, ..]) => self
                .target(snarl[pin.node], Side::Output, pin.output)
                .map(|t| (t, Side::Output)),
            _ => None,
        };
        let origin = match port {
            Some((
                PinTarget {
                    node,
                    item: Some(item),
                },
                side,
            )) => Origin::Port { node, side, item },
            _ => Origin::Canvas,
        };
        self.events.push(CanvasEvent::OpenPicker {
            origin,
            graph_pos: pos,
        });
        ui.close();
    }

    fn connect(&mut self, from: &OutPin, to: &InPin, snarl: &mut Snarl<Member>) {
        let (from_member, to_member) = (snarl[from.id.node], snarl[to.id.node]);
        if let (Some(from), Some(to)) = (
            self.port_ref(from_member, Side::Output, from.id.output),
            self.port_ref(to_member, Side::Input, to.id.input),
        ) {
            self.events.push(CanvasEvent::Connect { from, to });
        }
    }

    fn disconnect(&mut self, from: &OutPin, to: &InPin, snarl: &mut Snarl<Member>) {
        let (Some(from), Some(to)) = (
            self.target(snarl[from.id.node], Side::Output, from.id.output),
            self.target(snarl[to.id.node], Side::Input, to.id.input),
        ) else {
            return;
        };
        for link in self.links_between(&from, &to) {
            self.events.push(CanvasEvent::Disconnect(link));
        }
    }

    /// Right-click on an input port: unlink it, or, if it has no links, add a producer of its item.
    fn drop_inputs(&mut self, pin: &InPin, snarl: &mut Snarl<Member>) {
        let Some(target) = self.target(snarl[pin.id.node], Side::Input, pin.id.input) else {
            return;
        };
        if pin.remotes.is_empty() {
            let key = PinKey::In(pin.id);
            self.open_picker_at_port(target.node, Side::Input, target.item, key);
            return;
        }
        for remote in &pin.remotes {
            let Some(from) = self.target(snarl[remote.node], Side::Output, remote.output) else {
                continue;
            };
            for link in self.links_between(&from, &target) {
                self.events.push(CanvasEvent::Disconnect(link));
            }
        }
    }

    /// Right-click on an output port: unlink it, or, if it has no links, add a consumer of its item.
    fn drop_outputs(&mut self, pin: &OutPin, snarl: &mut Snarl<Member>) {
        let Some(target) = self.target(snarl[pin.id.node], Side::Output, pin.id.output) else {
            return;
        };
        if pin.remotes.is_empty() {
            let key = PinKey::Out(pin.id);
            self.open_picker_at_port(target.node, Side::Output, target.item, key);
            return;
        }
        for remote in &pin.remotes {
            let Some(to) = self.target(snarl[remote.node], Side::Input, remote.input) else {
                continue;
            };
            for link in self.links_between(&target, &to) {
                self.events.push(CanvasEvent::Disconnect(link));
            }
        }
    }

    fn current_transform(&mut self, to_global: &mut TSTransform, _snarl: &mut Snarl<Member>) {
        // egui-snarl fits the view to its nodes the first time it sees the canvas. With no nodes
        // that fit divides by zero; start from the plain view instead.
        let usable = to_global.scaling.is_finite()
            && to_global.scaling > 0.0
            && to_global.translation.is_finite();
        if !usable {
            *to_global = TSTransform::IDENTITY;
        }
        if let Some(view) = self.pending_view.take() {
            *to_global = view;
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
    view: &'a Snarl<Member>,
    pins: &'a HashMap<PinKey, Pos2>,
    selection: Option<&'a Selection>,
}

impl LinkLabels<'_> {
    /// Paints the labels on the canvas layer, in graph space, and returns a link whose label was
    /// clicked.
    fn paint(
        &self,
        ui: &Ui,
        layer: LayerId,
        to_global: TSTransform,
        canvas: Rect,
    ) -> Option<LinkId> {
        // The layer is drawn through the canvas transform, so its clip rect is in graph space too.
        let mut painter = ui.ctx().layer_painter(layer);
        painter.set_clip_rect(to_global.inverse() * canvas);
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
            // Two nodes can be linked by more than one item; each wire belongs to its pins' ports.
            let target = |member: Member, side: Side, index: usize| {
                pin_targets(self.graph, self.catalog, member, side)
                    .into_iter()
                    .nth(index)
            };
            let (Some(from), Some(to)) = (
                target(self.view[out_pin.node], Side::Output, out_pin.output),
                target(self.view[in_pin.node], Side::Input, in_pin.input),
            ) else {
                continue;
            };
            for (id, link) in self.graph.links_at(from.node, Side::Output) {
                if !from.is(link.from, &link.item) || !to.is(link.to, &link.item) {
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
                let selected = self.selection == Some(&Selection::Link(id));
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
