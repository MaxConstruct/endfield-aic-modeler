//! The node canvas: egui-snarl drawing the [`Document`]'s graph with the solver's numbers.
//!
//! The viewer only reads the graph. Everything the player does on the canvas comes back as
//! [`CanvasEvent`]s, which the editor applies to the document after the frame's canvas is drawn,
//! so the graph and its view always change together.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use aic_core::{
    Graph, Issue, LinkId, Member, NodeId, NodeKind, Port, PortRef, RoundUp, Side, Solution,
    Warning, WorkspaceId,
};
use aic_data::{Catalog, FacilityId, ImageKind, ItemId, RecipeId};
use eframe::egui::{
    self, Color32, CornerRadius, FontId, Id, LayerId, Pos2, Rect, RichText, Sense, Stroke, Ui,
    Vec2, emath::TSTransform, vec2,
};
use egui_snarl::ui::{
    AnyPins, BackgroundPattern, NodeLayout, PinInfo, PinPlacement, PinWireInfo, SnarlPin,
    SnarlStyle, SnarlViewer, SnarlWidget, WireStyle,
};
use egui_snarl::{InPin, InPinId, OutPin, OutPinId, Snarl};

use crate::document::{
    Anchor, Document, PinTarget, VIEW_SCALE, ViewId, is_port, pin_targets, port_ref,
};
use crate::format;
use crate::i18n::Strings;
use crate::names;
use crate::picker::Origin;
use crate::theme;
use crate::widgets::{self, ItemPicture, LimitField, SINK_IMAGE, SOURCE_IMAGE, colors};

const CANVAS_ID: &str = "aic-canvas";
/// The warning sign before a card's title, at 100%.
const WARNING_ICON: f32 = 13.0;
// Sizes below are in points at 100%. Inside the canvas everything is laid out at the largest zoom
// (egui-snarl's crisp mode, so text stays sharp when zoomed in) and drawn shrunk; `crisp` turns a
// size into view space there. Text styles and egui's spacing are scaled by snarl itself.
/// How far from a port a node added from it is placed.
const PORT_GAP: f32 = 80.0;
/// Size of the facility picture on a recipe card (and the depot picture on a source or sink).
const FACILITY_IMAGE: f32 = 64.0;
/// Size of a port's item icon. The icon is the port's pin: it sits inside the card against its
/// edge, and a link is dragged from it.
const PIN_ICON: f32 = 48.0;
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
/// A card's corner radius, in view space.
const CARD_RADIUS: u8 = theme::radius::MEDIUM * VIEW_SCALE as u8;
/// Room left at the top of a workspace's canvas for the back button and the add-port buttons.
const EDGE_TOP: f32 = 48.0;
/// Space between a workspace's port tiles and the canvas edge, and between two tiles.
const EDGE_GAP: f32 = 8.0;
/// How far the pointer moves with the right button down before it draws a selection box.
const BOX_THRESHOLD: f32 = 4.0;
/// How far the canvas zooms out and in. The canvas is laid out at `MAX_ZOOM`, which is what the
/// document's view space is scaled by.
pub const MIN_ZOOM: f32 = 0.2;
pub const MAX_ZOOM: f32 = VIEW_SCALE;
/// The wheel and the slider zoom in whole steps of this: 65%, 70%, never 67%.
pub const ZOOM_STEP: f32 = 0.05;
/// The zooms the status bar's − and + step through.
const ZOOM_STEPS: [f32; 7] = [0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 2.0];

/// A size in points at 100% as a size in view space, where the canvas lays its cards out.
const fn crisp(points: f32) -> f32 {
    points * VIEW_SCALE
}

/// `zoom` to the nearest whole [`ZOOM_STEP`], within the range.
fn round_zoom(zoom: f32) -> f32 {
    ((zoom / ZOOM_STEP).round() * ZOOM_STEP).clamp(MIN_ZOOM, MAX_ZOOM)
}

/// The next step from `zoom`, in or out; past the last step, the end of the range.
pub fn zoom_step(zoom: f32, zoom_in: bool) -> f32 {
    // Within this of a step counts as on it.
    const ON: f32 = 0.005;
    if zoom_in {
        ZOOM_STEPS
            .into_iter()
            .find(|step| *step > zoom + ON)
            .unwrap_or(MAX_ZOOM)
    } else {
        ZOOM_STEPS
            .into_iter()
            .rev()
            .find(|step| *step < zoom - ON)
            .unwrap_or(MIN_ZOOM)
    }
}

/// egui-snarl's id for one view's canvas of one level. Each keeps its own camera and selection.
fn snarl_id(view: Id, level: Option<WorkspaceId>) -> Id {
    view.with(level)
}

/// What the player has selected: nodes and workspaces as drawn on the canvas, or one link.
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

    /// The selected nodes, leaving out workspaces.
    pub fn nodes(&self) -> Vec<NodeId> {
        self.items()
            .iter()
            .filter_map(|m| match m {
                Member::Node(id) => Some(*id),
                Member::Workspace(_) => None,
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

    /// The workspace, when exactly one item is selected and it is a workspace.
    pub fn single_workspace(&self) -> Option<WorkspaceId> {
        match self.items() {
            [Member::Workspace(one)] => Some(*one),
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
    /// Shift+click on a node or workspace: add it to the selection, or take it out.
    Toggle(Member),
    /// Remove a node, or a workspace with everything in it.
    Remove(Member),
    /// Show another level: a workspace (a double-click on its card), or `None` for the top.
    ShowLevel(Option<WorkspaceId>),
    /// Add a port to the workspace shown.
    AddPort(Side),
    Rename(WorkspaceId, String),
    Dissolve(WorkspaceId),
    Cut(Member),
    Copy(Member),
    /// Paste what was copied into this workspace.
    PasteInto(WorkspaceId),
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
    /// Sets this view's ids apart from other views' of the same graph.
    id: Id,
    style: SnarlStyle,
    pins: PinCenters,
    /// What the player is typing in each card's limit field.
    limit_edits: HashMap<NodeId, String>,
    /// Graph space to screen space, as of the last frame.
    pub to_global: TSTransform,
    /// The canvas area on screen, as of the last frame.
    pub rect: Rect,
    /// Where each node and workspace card was drawn last frame, in graph space.
    node_rects: HashMap<Member, Rect>,
    /// egui-snarl's own node selection as of the last frame. It changes on a box selection
    /// (Shift+drag on the background), which then becomes the player's selection.
    snarl_selection: Vec<ViewId>,
    /// What the player is dragging, found when it first moves and kept until the button is up.
    grabbed: Option<Member>,
    /// A camera to show from the next frame at a level, set when a file or a view opens.
    pending_view: Option<(Option<WorkspaceId>, aic_core::CanvasView)>,
    /// A zoom to show from the next frame, around the middle of the canvas.
    pending_zoom: Option<f32>,
    /// Wheel scroll not yet turned into a zoom step, as a factor. Small scrolls add up here until
    /// they reach the next 5%.
    wheel_rest: f32,
    /// The level shown last frame.
    level: Option<WorkspaceId>,
    /// Where the top of the graph was looked at last, kept while a workspace is open so a save
    /// still records it.
    top_view: Option<aic_core::CanvasView>,
    /// Where a right-button press on the background began, on screen, and the camera then. The
    /// camera holds still until the button is up: egui-snarl would pan with the drag.
    right_press: Option<(Pos2, TSTransform)>,
    /// The workspace name being typed in a card's menu.
    title_edit: String,
    /// The levels shown so far; egui-snarl places its camera the first time it sees one.
    shown_levels: std::collections::HashSet<Option<WorkspaceId>>,
}

impl Default for Canvas {
    fn default() -> Self {
        Self::new(0)
    }
}

impl Canvas {
    /// The canvas of the view numbered `view`.
    pub fn new(view: u64) -> Self {
        Self {
            id: Id::new((CANVAS_ID, view)),
            style: SnarlStyle {
                node_layout: Some(NodeLayout::coil()),
                pin_placement: Some(PinPlacement::Edge),
                pin_size: Some(PIN_ICON),
                wire_width: Some(2.0),
                collapsible: Some(false),
                // Lay the canvas out at the largest zoom and shrink it, so text is drawn at the
                // size it is shown and stays sharp up to 200%. Positions in the view are scaled to
                // match (`VIEW_SCALE`).
                crisp_magnified_text: Some(true),
                min_scale: Some(MIN_ZOOM),
                max_scale: Some(MAX_ZOOM),
                bg_pattern: Some(BackgroundPattern::grid(vec2(40.0, 40.0), 0.0)),
                bg_pattern_stroke: Some(Stroke::new(1.0, theme::color::GRID)),
                bg_frame: Some(egui::Frame::new().fill(theme::color::CANVAS)),
                node_frame: Some(theme::node_frame()),
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
            pending_zoom: None,
            wheel_rest: 1.0,
            level: None,
            top_view: None,
            right_press: None,
            title_edit: String::new(),
            shown_levels: Default::default(),
        }
    }

    /// Where the canvas is looking: the graph point at its top-left, and the zoom. `None` before
    /// the canvas was first drawn.
    pub fn view(&self) -> Option<aic_core::CanvasView> {
        if !self.rect.is_positive() {
            return None;
        }
        let corner = self.to_global.inverse() * self.rect.min / VIEW_SCALE;
        Some(aic_core::CanvasView {
            x: corner.x,
            y: corner.y,
            zoom: self.zoom(),
        })
    }

    /// The zoom shown: 1 at 100%. The camera's own scaling is in view space.
    fn zoom(&self) -> f32 {
        self.to_global.scaling * VIEW_SCALE
    }

    /// Where the top of the graph is looked at, even while a workspace is open. Saved with the file.
    pub fn top_view(&self) -> Option<aic_core::CanvasView> {
        if self.level.is_none() {
            self.view()
        } else {
            self.top_view
        }
    }

    /// Zooms to `zoom` from the next frame on, around the middle of the canvas.
    pub fn zoom_to(&mut self, zoom: f32) {
        self.pending_zoom = Some(zoom);
    }

    /// Looks at `view` from the next frame on, as when a file is opened.
    pub fn show_view(&mut self, view: aic_core::CanvasView) {
        self.show_view_at(None, view);
    }

    /// Looks at `view` once `level` is next shown.
    pub fn show_view_at(&mut self, level: Option<WorkspaceId>, view: aic_core::CanvasView) {
        self.pending_view = Some((level, view));
    }

    /// The middle of the visible canvas, in graph space.
    pub fn center(&self) -> Pos2 {
        self.to_global.inverse() * self.rect.center() / VIEW_SCALE
    }

    /// A point in graph space, on screen.
    pub fn to_screen(&self, graph_pos: Pos2) -> Pos2 {
        self.to_global * (graph_pos * VIEW_SCALE)
    }

    /// Where a node or workspace card was drawn last frame, on screen.
    pub fn item_on_screen(&self, member: Member) -> Option<Rect> {
        let rect = self.node_rects.get(&member)?;
        Some(self.to_global * *rect)
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
        let member = doc.shown(node)?;
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
        Some(self.to_global * center)
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
        let last_rect = std::mem::replace(&mut self.rect, ui.max_rect());
        let level = doc.level();
        // egui-snarl's camera is on screen: when the view's place moves (its tab dragged to
        // another split, a split resized), the graph moves along with it.
        let moved = if last_rect.is_positive() && level == self.level {
            self.rect.min - last_rect.min
        } else {
            Vec2::ZERO
        };
        if level != self.level {
            if self.level.is_none() {
                self.top_view = self.view();
            }
            self.level = level;
            self.node_rects.clear();
            self.snarl_selection.clear();
        }
        let path = doc.level_path();
        let (graph, view, anchors, solution) = doc.parts();
        let before: HashMap<ViewId, Pos2> = view
            .nodes_pos_ids()
            .filter(|(_, _, member)| match member {
                Member::Node(id) => !anchors.contains_key(id) && !is_port(graph, *id),
                Member::Workspace(_) => true,
            })
            .map(|(view_id, pos, _)| (view_id, pos))
            .collect();
        // egui-snarl fits its camera to the nodes the first time it sees a level. With none to fit,
        // start at 100% instead.
        let fresh = self.shown_levels.insert(level) && view.nodes().next().is_none();
        let zoom = match self.pending_zoom.take() {
            Some(zoom) => Some((self.rect.center(), zoom)),
            None => self.wheel_zoom(ui),
        };
        let mut viewer = Viewer {
            id: self.id,
            moved,
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
            hold: self.right_press.map(|(_, camera)| camera),
            fresh,
            zoom,
            level,
            canvas_rect: self.rect,
            ctx: ui.ctx().clone(),
            pending_view: self
                .pending_view
                .take_if(|(at, _)| *at == level)
                .map(|(_, view)| {
                    // Screen = zoom × graph + translation, with the view's corner at the canvas
                    // corner; the camera's scaling is in view space.
                    let corner = Pos2::new(view.x, view.y).to_vec2() * view.zoom;
                    TSTransform::new(self.rect.min.to_vec2() - corner, view.zoom / VIEW_SCALE)
                }),
            node_rects: &mut self.node_rects,
            pin_cache: HashMap::new(),
            title_edit: &mut self.title_edit,
            events: Vec::new(),
        };
        let response = SnarlWidget::new()
            .id(snarl_id(self.id, level))
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

        let pins = self.pins.borrow().clone();
        let labels = LinkLabels {
            catalog,
            strings,
            graph,
            solution,
            view,
            pins: &pins,
            selection,
        };
        let layer = LayerId::new(ui.layer_id().order, snarl_id(self.id, level));
        let clicked_label = labels.paint(ui, layer, self.to_global, self.rect);
        let selected = events.iter().any(|e| matches!(e, CanvasEvent::Select(_)));
        let modifiers = ui.input(|i| i.modifiers);
        if let Some(link) = clicked_label {
            events.push(CanvasEvent::Select(Some(Selection::Link(link))));
        } else if response.clicked() && !selected && !modifiers.shift && !modifiers.command {
            events.push(CanvasEvent::Select(None));
        }
        let boxed = self.right_drag_selection(ui, view);
        let right_dragged = boxed.is_some();
        events.extend(boxed);
        self.level_bar(ui, graph, &path, strings, &mut events);
        // Only a real right click: egui-snarl opens its dropped-link menu by marking the canvas
        // long-touched, which also counts as a secondary click. The response is on the canvas's
        // zoomed and panned layer, so the click's position comes from the input, on screen.
        if response.clicked_by(egui::PointerButton::Secondary)
            && !right_dragged
            && let Some(pos) = ui.input(|i| i.pointer.interact_pos())
        {
            events.push(CanvasEvent::OpenPicker {
                origin: Origin::Canvas,
                graph_pos: self.to_global.inverse() * pos / VIEW_SCALE,
            });
        }
        events
    }
}

impl Canvas {
    /// The back button and the way down from the top, and the buttons that add ports, over the
    /// canvas of a workspace.
    fn level_bar(
        &self,
        ui: &Ui,
        graph: &Graph,
        path: &[WorkspaceId],
        strings: &Strings,
        events: &mut Vec<CanvasEvent>,
    ) {
        let Some(&shown) = path.last() else {
            return;
        };
        let name = |id: WorkspaceId| {
            graph
                .workspace_info(id)
                .map(|w| w.name.clone())
                .unwrap_or_default()
        };
        egui::Area::new(self.id.with("level-bar"))
            .fixed_pos(self.rect.left_top() + vec2(EDGE_GAP, EDGE_GAP))
            .show(ui.ctx(), |ui| {
                ui.horizontal(|ui| {
                    let parent = path.len().checked_sub(2).map(|i| path[i]);
                    if ui
                        .button(strings.get("workspace-back"))
                        .on_hover_text(strings.get("workspace-back-hint"))
                        .clicked()
                    {
                        events.push(CanvasEvent::ShowLevel(parent));
                    }
                    if ui.link(strings.get("workspace-top")).clicked() {
                        events.push(CanvasEvent::ShowLevel(None));
                    }
                    for &id in path {
                        ui.label("›");
                        if id == shown {
                            ui.label(RichText::new(name(id)).strong());
                        } else if ui.link(name(id)).clicked() {
                            events.push(CanvasEvent::ShowLevel(Some(id)));
                        }
                    }
                    if ui
                        .button(strings.get("workspace-add-input"))
                        .on_hover_text(strings.get("workspace-add-input-hint"))
                        .clicked()
                    {
                        events.push(CanvasEvent::AddPort(Side::Input));
                    }
                });
            });
        egui::Area::new(self.id.with("level-bar-outputs"))
            .pivot(egui::Align2::RIGHT_TOP)
            .fixed_pos(self.rect.right_top() + vec2(-EDGE_GAP, EDGE_GAP))
            .show(ui.ctx(), |ui| {
                if ui
                    .button(strings.get("workspace-add-output"))
                    .on_hover_text(strings.get("workspace-add-output-hint"))
                    .clicked()
                {
                    events.push(CanvasEvent::AddPort(Side::Output));
                }
            });
    }

    /// A scroll over the canvas zooms around the pointer instead of panning: the scroll is taken
    /// from the input before egui-snarl sees it, and returned as the pointer on screen and the new
    /// zoom, a whole 5% step. Whatever the wheel reports, lines or points (a Magic Mouse, a
    /// smooth-scrolling wheel, a trackpad), it zooms; dragging the background pans.
    fn wheel_zoom(&mut self, ui: &Ui) -> Option<(Pos2, f32)> {
        let (delta, pos, modifiers) =
            ui.input(|i| (i.smooth_scroll_delta, i.pointer.latest_pos(), i.modifiers));
        let pos = pos?;
        let over_canvas = self.rect.contains(pos)
            && ui
                .ctx()
                .layer_id_at(pos)
                .is_none_or(|layer| layer.order == ui.layer_id().order);
        // Shift scrolls sideways, as everywhere.
        if !over_canvas || modifiers.shift || delta == Vec2::ZERO {
            return None;
        }
        ui.ctx().input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO);
        let speed = ui.ctx().options(|o| o.input_options.scroll_zoom_speed);
        let current = self.zoom();
        let wanted = current * self.wheel_rest * (speed * (delta.x + delta.y)).exp();
        let zoom = round_zoom(wanted);
        // Past either end of the range, the rest of the scroll is dropped.
        self.wheel_rest = if (MIN_ZOOM..=MAX_ZOOM).contains(&wanted) {
            wanted / zoom
        } else {
            1.0
        };
        ((zoom - current).abs() > ZOOM_STEP * 0.01).then_some((pos, zoom))
    }

    /// A box drawn by dragging with the right button on the background, as on a desktop: what it
    /// touches becomes the selection. A right click without a drag still opens the picker.
    fn right_drag_selection(&mut self, ui: &Ui, view: &Snarl<Member>) -> Option<CanvasEvent> {
        let (pressed, down, pos) = ui.input(|i| {
            (
                i.pointer.button_pressed(egui::PointerButton::Secondary),
                i.pointer.button_down(egui::PointerButton::Secondary),
                i.pointer.interact_pos(),
            )
        });
        let from_screen = self.to_global.inverse();
        if pressed
            && let Some(pos) = pos
            && self.rect.contains(pos)
            && !self
                .node_rects
                .values()
                .any(|rect| rect.contains(from_screen * pos))
            && ui
                .ctx()
                .layer_id_at(pos)
                .is_none_or(|layer| layer.order == ui.layer_id().order)
        {
            self.right_press = Some((pos, self.to_global));
        }
        let (start, _) = self.right_press?;
        let end = pos.unwrap_or(start);
        let dragged = (end - start).length() > BOX_THRESHOLD;
        if down {
            if dragged {
                let rect = Rect::from_two_pos(start, end);
                let painter = ui.ctx().layer_painter(LayerId::new(
                    egui::Order::Foreground,
                    self.id.with("right-drag-box"),
                ));
                painter.rect(
                    rect,
                    0.0,
                    colors::SELECT_BOX.gamma_multiply(0.15),
                    Stroke::new(1.0, colors::SELECT_BOX),
                    egui::StrokeKind::Inside,
                );
            }
            return None;
        }
        self.right_press = None;
        if !dragged {
            return None;
        }
        let area = Rect::from_two_pos(from_screen * start, from_screen * end);
        let items: Vec<Member> = view
            .nodes()
            .copied()
            .filter(|member| {
                self.node_rects
                    .get(member)
                    .is_some_and(|rect| rect.intersects(area))
            })
            .collect();
        Some(CanvasEvent::Select(Selection::of_items(items)))
    }

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
        let now = egui_snarl::ui::get_selected_nodes(snarl_id(self.id, self.level), ui.ctx());
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

/// Puts the shown workspace's own ports on the canvas's edges, whatever the camera: inputs down
/// the left, outputs down the right, below the buttons at the top. Runs with this frame's camera,
/// before the cards are drawn, so a tile never trails a pan or zoom.
fn pin_ports_to_edges(
    ctx: &egui::Context,
    graph: &Graph,
    view: &mut Snarl<Member>,
    level: Option<WorkspaceId>,
    canvas: Rect,
    node_rects: &HashMap<Member, Rect>,
    to_global: TSTransform,
) {
    let Some(workspace) = level else {
        return;
    };
    let from_screen = to_global.inverse();
    let ids: Vec<(ViewId, Member)> = view.nodes_pos_ids().map(|(v, _, m)| (v, *m)).collect();
    for side in [Side::Input, Side::Output] {
        let mut top = canvas.top() + EDGE_TOP;
        for port in graph.ports_of(workspace, side) {
            let member = Member::Node(port);
            let Some(&(view_id, _)) = ids.iter().find(|(_, m)| *m == member) else {
                continue;
            };
            // Last frame's size; a new tile starts with a guess.
            let size = node_rects
                .get(&member)
                .map_or(vec2(80.0, 64.0), |r| r.size() * to_global.scaling);
            let left = match side {
                Side::Input => canvas.left() + EDGE_GAP,
                Side::Output => canvas.right() - EDGE_GAP - size.x,
            };
            if let Some(info) = view.get_node_info_mut(view_id) {
                let pos = from_screen * Pos2::new(left, top);
                // A new tile changes size as it settles; draw again until it stops.
                if (info.pos - pos).length() > 0.5 {
                    ctx.request_repaint();
                }
                info.pos = pos;
            }
            top += size.y + EDGE_GAP;
        }
    }
}

/// A port's pin: its item's icon inside the card against the edge, on a tile colored like the
/// port (linked, or pulling from or sending to the depot). It records where snarl drew it.
struct RecordingPin {
    /// `None` for a logistics node's port before its first link: a plain circle.
    picture: Option<ItemPicture>,
    /// The port's rate, painted beside the icon where it is drawn this frame.
    rate: Option<Rate>,
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
            painter.rect_filled(rect, crisp(4.0), fill.gamma_multiply(0.35));
            picture.paint(painter, rect.shrink(crisp(3.0)));
            if self.throttled {
                paint_throttle_marker(painter, rect);
            }
            if let Some(rate) = &self.rate {
                // The icon's own rect, not one hovered larger.
                let icon = Rect::from_center_size(rect.center(), Vec2::splat(crisp(PIN_ICON)));
                let (anchor, align) = match self.side {
                    Side::Input => (
                        icon.right_center() + vec2(crisp(RATE_GAP), 0.0),
                        egui::Align2::LEFT_CENTER,
                    ),
                    Side::Output => (
                        icon.left_center() - vec2(crisp(RATE_GAP), 0.0),
                        egui::Align2::RIGHT_CENTER,
                    ),
                };
                painter.text(
                    anchor,
                    align,
                    &rate.text,
                    egui::TextStyle::Monospace.resolve(style),
                    rate.color,
                );
            }
        }
        PinWireInfo {
            color: colors::LINKED,
            style: snarl_style.wire_style.unwrap_or(WireStyle::Bezier5),
        }
    }
}

/// A port's rate as its pin paints it.
struct Rate {
    text: String,
    color: Color32,
}

/// A small warning triangle in the top corner of a port's tile.
fn paint_throttle_marker(painter: &egui::Painter, tile: Rect) {
    let rect = Rect::from_min_size(
        tile.right_top() - vec2(crisp(11.0), crisp(-1.0)),
        Vec2::splat(crisp(10.0)),
    );
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
    /// The view's id ([`Canvas::id`]), which the ids of its widgets start from.
    id: Id,
    /// How far the canvas moved on screen since the last frame.
    moved: Vec2,
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
    /// A camera to keep, whatever snarl's pan and zoom did this frame.
    hold: Option<TSTransform>,
    /// The level is shown for the first time, empty: its camera starts at 100%.
    fresh: bool,
    /// A zoom this frame: the point on screen that holds still, and the new zoom.
    zoom: Option<(Pos2, f32)>,
    /// The workspace shown, whose ports sit on the canvas's edges.
    level: Option<WorkspaceId>,
    /// The canvas area on screen.
    canvas_rect: Rect,
    ctx: egui::Context,
    node_rects: &'a mut HashMap<Member, Rect>,
    /// Each drawn item's pins this frame; a workspace's take a walk over the graph to find.
    pin_cache: HashMap<(Member, Side), Vec<PinTarget>>,
    /// The workspace name being typed in a card's menu.
    title_edit: &'a mut String,
    events: Vec<CanvasEvent>,
}

impl Viewer<'_> {
    /// A recipe card's facility, under its title where a long name has the card's width. With a
    /// choice of facilities it is a button: one click moves to the next, and the inspector has the
    /// full list.
    fn facility_line(&mut self, ui: &mut Ui, id: NodeId, recipe: &RecipeId, facility: &FacilityId) {
        let name = names::facility(self.catalog, facility.as_str());
        let text = RichText::new(name).small().color(theme::color::TEXT_MUTED);
        let facilities = self
            .catalog
            .recipe(recipe.as_str())
            .map(|r| r.facilities.as_slice())
            .unwrap_or_default();
        if facilities.len() > 1 {
            let button = egui::Button::new(text).frame(false).truncate();
            if ui.add(button).clicked() {
                let next = facilities
                    .iter()
                    .position(|f| f == facility)
                    .map_or(0, |i| (i + 1) % facilities.len());
                self.events
                    .push(CanvasEvent::SetFacility(id, facilities[next].clone()));
            }
        } else {
            ui.add(egui::Label::new(text).truncate());
        }
    }

    /// A node's title, after a warning sign when the region lacks its recipe or facility, and the
    /// recipe's region badges when only some regions have it.
    fn title_row(&self, ui: &mut Ui, id: NodeId, kind: &NodeKind, title: RichText) {
        let catalog = self.catalog;
        let badges = match kind {
            NodeKind::Recipe { recipe, .. } => catalog
                .recipe(recipe.as_str())
                .map(|recipe| widgets::RegionBadges::new(ui, catalog, recipe, VIEW_SCALE)),
            _ => None,
        }
        .filter(|badges| !badges.is_empty());
        let warnings: Vec<String> = self
            .solution
            .warnings
            .iter()
            .filter_map(|warning| match warning {
                Warning::Issue(
                    issue @ (Issue::RecipeOutsideRegion { node, .. }
                    | Issue::FacilityOutsideRegion { node, .. }),
                ) if *node == id => {
                    Some(crate::inspector::issue_text(catalog, self.strings, issue))
                }
                _ => None,
            })
            .collect();
        if badges.is_none() && warnings.is_empty() {
            title_label(ui, title);
            return;
        }
        ui.set_max_width(title_width(ui));
        ui.horizontal(|ui| {
            if !warnings.is_empty() {
                widgets::warning_icon(ui, crisp(WARNING_ICON)).on_hover_text(warnings.join("\n"));
            }
            if let Some(badges) = &badges {
                badges.ui(ui);
            }
            ui.add(egui::Label::new(title).truncate());
        });
    }

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
            graph_pos: center / VIEW_SCALE + vec2(offset, 0.0),
        });
    }

    /// Room for a port's rate beside its pin, as wide on every card. Returns whether the solver
    /// throttled the node at this port, and the rate, which the pin paints beside itself.
    fn port_row(
        &mut self,
        ui: &mut Ui,
        pin: PinKey,
        id: NodeId,
        side: Side,
        item: Option<&ItemId>,
        linked: bool,
    ) -> (bool, Rate) {
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
        // The column starts inside the card's margin; snarl has already stepped past part of the
        // pin. Fill the rest of the column so every card is as wide.
        let column = ui.max_rect();
        let used = match side {
            Side::Input => ui.cursor().left() - column.left(),
            Side::Output => column.right() - ui.cursor().right(),
        };
        let rect = match side {
            Side::Input => Rect::from_min_max(
                Pos2::new(
                    column.left() + crisp(PORT_COLUMN - RATE_WIDTH),
                    column.top(),
                ),
                Pos2::new(
                    column.left() + crisp(PORT_COLUMN),
                    column.top() + crisp(PIN_ICON),
                ),
            ),
            Side::Output => Rect::from_min_max(
                Pos2::new(column.right() - crisp(PORT_COLUMN), column.top()),
                Pos2::new(
                    column.right() - crisp(PORT_COLUMN - RATE_WIDTH),
                    column.top() + crisp(PIN_ICON),
                ),
            ),
        };
        ui.allocate_exact_size(
            vec2((crisp(PORT_COLUMN) - used).max(0.0), crisp(PIN_ICON)),
            Sense::hover(),
        );
        let rate = Rate {
            text: format::short(flow),
            color,
        };
        let Some(item) = item else {
            return (throttled, rate);
        };
        let mut hint = names::item(self.catalog, item.as_str()).to_owned();
        if throttled {
            hint.push('\n');
            hint.push_str(&self.strings.get("throttle-marker-hint"));
        }
        // A double-click on the port opens its menu: the limit as a rate, and clearing its links.
        let response = ui
            .interact(
                rect,
                self.id.with(("port-row", id, side, item)),
                Sense::click(),
            )
            .on_hover_text(hint);
        // Beside the pin, on the node's edge; last frame's spot is close enough.
        let pin_center = self.pins.borrow().get(&pin).copied();
        // egui-snarl owns the pin's own clicks, and a primary click on a pin does nothing there,
        // so a double-click on the icon is read from the pointer.
        let on_icon = pin_center.is_some_and(|center| {
            let icon = Rect::from_center_size(center, Vec2::splat(crisp(PIN_ICON)));
            let from_screen = self.to_global.inverse();
            ui.input(|i| {
                let pointer = &i.pointer;
                let multi = pointer.button_double_clicked(egui::PointerButton::Primary)
                    || pointer.button_triple_clicked(egui::PointerButton::Primary);
                multi
                    && pointer
                        .interact_pos()
                        .is_some_and(|pos| icon.contains(from_screen * pos))
            })
        });
        // egui counts a quick third click as a triple click, not a double one; either opens it.
        if response.double_clicked() || response.triple_clicked() || on_icon {
            let graph_pos = pin_center.unwrap_or(match side {
                Side::Input => rect.left_center(),
                Side::Output => rect.right_center(),
            }) / VIEW_SCALE;
            self.events.push(CanvasEvent::OpenPortMenu {
                port: Port {
                    node: id,
                    item: item.clone(),
                    side,
                },
                graph_pos,
            });
        }
        (throttled, rate)
    }

    /// The port's pin, showing its item.
    fn pin(
        &self,
        ui: &Ui,
        key: PinKey,
        side: Side,
        item: Option<&ItemId>,
        linked: bool,
        row: Option<(bool, Rate)>,
    ) -> RecordingPin {
        let (throttled, rate) = row.map_or((false, None), |(t, r)| (t, Some(r)));
        RecordingPin {
            rate,
            picture: match item {
                Some(item) => Some(ItemPicture::of(ui.ctx(), self.catalog, item.as_str())),
                // A port without an item yet asks for one.
                None => Some(ItemPicture::unknown()),
            },
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
            id: self.id.with(("node-limit", id)),
            buffer,
            current: limit,
            width: crisp(76.0),
            hint: self.strings.get("node-no-limit"),
            label: self.strings.get("node-limit-label"),
        };
        if let (_, Some(typed)) = field.show(ui)
            && (typed.is_some() || !required)
        {
            self.events.push(CanvasEvent::SetLimit(id, typed));
        }
    }

    /// A workspace's card: how many nodes it holds, and a button to open it.
    fn workspace_body(&mut self, ui: &mut Ui, workspace: WorkspaceId) {
        let count = self
            .graph
            .nodes_in_workspace(workspace)
            .into_iter()
            .filter(|&node| !is_port(self.graph, node))
            .count();
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("🏭").size(crisp(FACILITY_IMAGE * 0.6)));
            ui.label(RichText::new(crate::names::node_count(self.strings, count)).small());
            if ui
                .small_button(self.strings.get("workspace-open"))
                .clicked()
            {
                self.events.push(CanvasEvent::ShowLevel(Some(workspace)));
            }
        });
    }

    fn big_number(ui: &mut Ui, value: Option<f64>) {
        // A blank value still takes its line, so the card doesn't jump when it fills in.
        let text = format::short(value);
        ui.label(
            RichText::new(if text.is_empty() { " ".into() } else { text })
                .text_style(theme::number_style())
                .color(theme::color::TEXT_STRONG),
        );
    }
}

/// A card's title, cut short with an ellipsis to fit the card; the whole title shows on hover.
fn title_label(ui: &mut Ui, text: RichText) {
    ui.set_max_width(title_width(ui));
    ui.add(egui::Label::new(text).truncate());
}

/// How wide a card's title can be.
fn title_width(ui: &Ui) -> f32 {
    crisp(CARD_WIDTH - 2.0 * f32::from(NODE_MARGIN)) - 2.0 * ui.spacing().window_margin.leftf()
}

/// The width left for a card's body between its port columns, so every card is `CARD_WIDTH` wide.
fn body_width(ui: &Ui, inputs: &[InPin], outputs: &[OutPin]) -> f32 {
    let columns = [inputs.is_empty(), outputs.is_empty()]
        .iter()
        .filter(|empty| !**empty)
        .count() as f32;
    let spacing = ui.spacing().item_spacing.x;
    crisp(CARD_WIDTH - 2.0 * f32::from(NODE_MARGIN) - columns * PORT_COLUMN) - 2.0 * spacing
}

/// Fills the body's width and lays `add` out in a column centered on the card. With ports on one
/// side only, the body also takes the missing side's room, so the column steps off that side by a
/// port column: the facility sits in the middle of every card, whichever ports it has.
fn centered_body<R>(
    ui: &mut Ui,
    inputs: &[InPin],
    outputs: &[OutPin],
    add: impl FnOnce(&mut Ui) -> R,
) -> R {
    let width = body_width(ui, inputs, outputs);
    ui.set_width(width);
    let (left, column) = match (inputs.is_empty(), outputs.is_empty()) {
        (true, false) => (crisp(PORT_COLUMN), width - crisp(PORT_COLUMN)),
        (false, true) => (0.0, width - crisp(PORT_COLUMN)),
        _ => (0.0, width),
    };
    let rect = Rect::from_min_size(
        ui.cursor().min + vec2(left, 0.0),
        vec2(column, ui.available_height()),
    );
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Center)),
        add,
    )
    .inner
}

impl SnarlViewer<Member> for Viewer<'_> {
    fn title(&mut self, member: &Member) -> String {
        match *member {
            Member::Node(id) => self
                .kind(id)
                .map(|kind| names::node_title(self.catalog, self.strings, kind))
                .unwrap_or_default(),
            Member::Workspace(id) => self
                .graph
                .workspace_info(id)
                .map(|w| w.name.clone())
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
            .corner_radius(CornerRadius::same(CARD_RADIUS))
            .inner_margin(egui::Margin::same(crisp(f32::from(NODE_MARGIN)) as i8));
        if self.selection.is_some_and(|s| s.contains(snarl[node])) {
            frame.stroke(Stroke::new(crisp(2.0), colors::ACCENT))
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
            Member::Workspace(_) => {
                return default
                    .fill(theme::color::node::WORKSPACE)
                    .corner_radius(CornerRadius::same(CARD_RADIUS));
            }
        };
        use theme::color::node;
        let tint = match kind {
            Some(NodeKind::Recipe { .. }) => node::RECIPE,
            Some(NodeKind::Source { .. }) => node::SOURCE,
            Some(NodeKind::Sink { .. }) => node::SINK,
            Some(NodeKind::OutpostOrder { .. }) => node::OUTPOST_ORDER,
            Some(NodeKind::MissingRecipe { .. }) => node::MISSING,
            _ => node::LOGISTICS,
        };
        default
            .fill(tint)
            .corner_radius(CornerRadius::same(CARD_RADIUS))
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
            Member::Workspace(workspace) => {
                let name = self.title(&Member::Workspace(workspace));
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(self.strings.get("kind-workspace"))
                            .small()
                            .weak(),
                    );
                    title_label(ui, theme::strong(name));
                });
                return;
            }
        };
        // Borrowed from the graph, not the viewer, so the facility line can push events.
        let graph = self.graph;
        let Some(kind) = graph.node(id).map(|node| &node.kind) else {
            return;
        };
        if let NodeKind::WorkspacePort { side, .. } = kind {
            let key = match side {
                Side::Input => "kind-workspace-input",
                Side::Output => "kind-workspace-output",
            };
            ui.label(RichText::new(self.strings.get(key)).small().weak());
            return;
        }
        ui.vertical(|ui| {
            if !matches!(kind, NodeKind::Recipe { .. }) {
                ui.label(
                    RichText::new(self.strings.get(names::kind_key(kind)))
                        .small()
                        .weak(),
                );
            }
            let subject = names::node_subject(self.catalog, self.strings, kind);
            self.title_row(ui, id, kind, theme::strong(subject));
            if let NodeKind::Recipe {
                recipe, facility, ..
            } = kind
            {
                self.facility_line(ui, id, recipe, facility);
            }
            if let NodeKind::Storage { .. } = kind {
                let balance = self.solution.storage.get(&id).and_then(|s| s.balance);
                if let Some(balance) = balance {
                    let color = if balance >= 0.0 {
                        colors::POSITIVE
                    } else {
                        colors::NEGATIVE
                    };
                    let mut text = format::short(Some(balance));
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
        let target = self.target(snarl[pin.id.node], Side::Input, pin.id.input);
        let item = target.as_ref().and_then(|t| t.item.clone());
        let row = target.map(|target| {
            self.port_row(
                ui,
                key,
                target.node,
                Side::Input,
                target.item.as_ref(),
                linked,
            )
        });
        self.pin(ui, key, Side::Input, item.as_ref(), linked, row)
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
        let target = self.target(snarl[pin.id.node], Side::Output, pin.id.output);
        let item = target.as_ref().and_then(|t| t.item.clone());
        let row = target.map(|target| {
            self.port_row(
                ui,
                key,
                target.node,
                Side::Output,
                target.item.as_ref(),
                linked,
            )
        });
        self.pin(ui, key, Side::Output, item.as_ref(), linked, row)
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
            Member::Workspace(workspace) => {
                centered_body(ui, inputs, outputs, |ui| {
                    self.workspace_body(ui, workspace);
                });
                return;
            }
        };
        let Some(kind) = self.kind(id).cloned() else {
            return;
        };
        // A port tile is its pin and rate alone; this keeps the rate inside the tile.
        if let NodeKind::WorkspacePort { .. } = kind {
            ui.set_min_width(crisp(PIN_ICON * 0.5));
            return;
        }
        let rate = self.solution.rate.get(&id).copied().flatten();
        centered_body(ui, inputs, outputs, |ui| match &kind {
            NodeKind::Recipe {
                facility, limit, ..
            } => {
                // The facility's name is in the header, under the title.
                let name = names::facility(self.catalog, facility.as_str());
                if let Some(image) = widgets::image(
                    ui,
                    self.catalog,
                    ImageKind::Facility,
                    facility.as_str(),
                    crisp(FACILITY_IMAGE),
                ) {
                    image.on_hover_text(name);
                }
                let count = self.solution.facility_count.get(&id).copied().flatten();
                match count.filter(|_| self.round_up).map(RoundUp::of) {
                    Some(rounded) => {
                        ui.label(
                            RichText::new(rounded.facilities.to_string())
                                .text_style(theme::number_style())
                                .color(theme::color::TEXT_STRONG),
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
                    crisp(FACILITY_IMAGE),
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
                    crisp(FACILITY_IMAGE),
                );
                Self::big_number(ui, rate);
                self.limit_field(ui, id, *limit, false);
            }
            NodeKind::OutpostOrder { limit, .. } => {
                Self::big_number(ui, rate);
                for (coupon, earned) in self.solution.coupons.get(&id).into_iter().flatten() {
                    ui.horizontal(|ui| {
                        widgets::item_icon(ui, self.catalog, coupon.as_str(), crisp(16.0));
                        let text = self
                            .strings
                            .format("outpost-earns", &[("rate", &format::short(Some(*earned)))]);
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
                    .format("storage-pass-through", &[("rate", &format::short(through))]);
                ui.add(egui::Label::new(RichText::new(text).small()).truncate());
            }
            NodeKind::WorkspacePort { .. } => {}
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
                Anchor::Left(x) => x * VIEW_SCALE - rect.left(),
                Anchor::Right(x) => x * VIEW_SCALE - rect.right(),
            };
            if dx.abs() < 0.5 {
                self.anchors.remove(&id);
            } else if let Some(info) = snarl.get_node_info_mut(node) {
                info.pos.x += dx;
                ui.ctx().request_repaint();
            }
        }
        self.node_rects.insert(member, rect);
        let (clicked_at, double, modifiers) = ui.input(|i| {
            let at = i
                .pointer
                .primary_clicked()
                .then(|| i.pointer.interact_pos())
                .flatten();
            let double = i
                .pointer
                .button_double_clicked(egui::PointerButton::Primary)
                .then(|| i.pointer.interact_pos())
                .flatten();
            (at, double, i.modifiers)
        });
        // A double-click on a workspace's card opens it.
        if let Member::Workspace(workspace) = member
            && let Some(pos) = double
            && rect.contains(self.to_global.inverse() * pos)
        {
            self.events.push(CanvasEvent::ShowLevel(Some(workspace)));
        }
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
        let strings = self.strings;
        let mut pick = |ui: &mut Ui, key: &str, event: CanvasEvent| {
            if ui.button(strings.get(key)).clicked() {
                self.events.push(event);
                ui.close();
            }
        };
        if let Member::Workspace(workspace) = member {
            pick(
                ui,
                "workspace-open",
                CanvasEvent::ShowLevel(Some(workspace)),
            );
            ui.horizontal(|ui| {
                ui.label(strings.get("workspace-title"));
                let name = self
                    .graph
                    .workspace_info(workspace)
                    .map(|w| w.name.clone())
                    .unwrap_or_default();
                let id = self.id.with(("workspace-title", workspace));
                if !ui.memory(|m| m.has_focus(id)) {
                    *self.title_edit = name.clone();
                }
                let field = ui.add(egui::TextEdit::singleline(self.title_edit).id(id));
                let typed = self.title_edit.trim();
                if field.changed() && !typed.is_empty() && typed != name {
                    self.events
                        .push(CanvasEvent::Rename(workspace, typed.to_owned()));
                }
            });
        }
        let mut pick = |ui: &mut Ui, key: &str, event: CanvasEvent| {
            if ui.button(strings.get(key)).clicked() {
                self.events.push(event);
                ui.close();
            }
        };
        pick(ui, "menu-cut", CanvasEvent::Cut(member));
        pick(ui, "menu-copy", CanvasEvent::Copy(member));
        if let Member::Workspace(workspace) = member {
            pick(
                ui,
                "workspace-paste-into",
                CanvasEvent::PasteInto(workspace),
            );
            pick(ui, "workspace-dissolve", CanvasEvent::Dissolve(workspace));
        }
        let remove = match member {
            Member::Node(_) => "node-remove",
            Member::Workspace(_) => "workspace-remove",
        };
        pick(ui, remove, CanvasEvent::Remove(member));
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
            graph_pos: pos / VIEW_SCALE,
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

    fn current_transform(&mut self, to_global: &mut TSTransform, snarl: &mut Snarl<Member>) {
        // egui-snarl fits the view to its nodes the first time it sees the canvas. With no nodes
        // there is nothing to fit; start from the plain view at 100% instead.
        let usable = to_global.scaling.is_finite()
            && to_global.scaling > 0.0
            && to_global.translation.is_finite();
        if !usable || self.fresh {
            *to_global = TSTransform::from_scaling(1.0 / VIEW_SCALE);
        }
        to_global.translation += self.moved;
        if let Some((fixed, zoom)) = self.zoom {
            // The graph point at `fixed` stays there. The camera's scaling is in view space.
            let under = to_global.inverse() * fixed;
            let zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM) / VIEW_SCALE;
            *to_global = TSTransform::new(fixed.to_vec2() - under.to_vec2() * zoom, zoom);
        }
        if let Some(view) = self.pending_view.take() {
            *to_global = view;
        }
        if let Some(camera) = self.hold {
            *to_global = camera;
        }
        // egui lays text out on whole pixels and then moves it by this transform; an offset between
        // pixels would smear every glyph on the canvas. At 100% the text is then as sharp as the
        // panels'; at other zooms egui scales the laid-out text.
        let ppp = self.ctx.pixels_per_point();
        to_global.translation = (to_global.translation * ppp).round() / ppp;
        pin_ports_to_edges(
            &self.ctx,
            self.graph,
            snarl,
            self.level,
            self.canvas_rect,
            self.node_rects,
            *to_global,
        );
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
                        ("flow", &format::short(Some(flow))),
                        ("belts", &format::short(belts)),
                    ],
                );
                let over = belts.is_some_and(|b| b > 1.0 + 1e-9);
                let color = if over {
                    colors::WARN
                } else {
                    visuals.text_color()
                };
                let font = FontId::proportional(crisp(theme::text::SMALL));
                let galley = painter.layout_no_wrap(text, font, color);
                let pad = crisp(3.0);
                let rect = Rect::from_center_size(a.lerp(b, 0.5), galley.size()).expand(pad);
                let selected = self.selection == Some(&Selection::Link(id));
                let stroke = if selected {
                    Stroke::new(crisp(1.5), colors::ACCENT)
                } else {
                    Stroke::NONE
                };
                painter.rect(
                    rect,
                    pad,
                    visuals.extreme_bg_color.gamma_multiply(0.9),
                    stroke,
                    egui::StrokeKind::Outside,
                );
                painter.galley(rect.min + Vec2::splat(pad), galley, color);
                if clicked_at.is_some_and(|pos| rect.contains(pos)) {
                    clicked = Some(id);
                }
            }
        }
        clicked
    }
}
