//! The node canvas: `aic-node-graph` drawing the [`Document`]'s graph with the solver's numbers.
//!
//! The canvas only reads the graph. Everything the player does on it comes back as
//! [`CanvasEvent`]s, which the editor applies to the document after the frame's canvas is drawn,
//! so the graph and its view always change together. Where cards sit is the one thing the widget
//! changes itself, in the [`DocView`]; the editor hands those moves to the document.
//!
//! `aic-node-graph` speaks of nodes, sockets and edges (ADR 0011). Here a card is one of its
//! nodes, a port's pin is a socket with the port's icon as its handle, and a wire is an edge.

use std::cell::OnceCell;
use std::collections::{HashMap, HashSet};

use aic_core::{
    Graph, Issue, LinkId, Member, NodeId, NodeKind, Port, PortRef, RoundUp, Side, Solution,
    Warning, WorkspaceId,
};
use aic_data::{Catalog, FacilityId, ImageKind, ItemId, RecipeId};
use aic_node_graph::edge::{Edge, EdgeShape};
use aic_node_graph::node::{EdgeEvent, Node, NodeResponse};
use aic_node_graph::{
    EdgesCtx, NodeCtx, NodesCtx, ResizeBehavior, SocketHighlight, SocketKind, SocketLayout,
};
use eframe::egui::{
    self, Color32, CornerRadius, FontId, Id, LayerId, Pos2, Rect, RichText, Sense, Stroke, Ui,
    Vec2, emath::TSTransform, pos2, vec2,
};
use egui_scale::EguiScale as _;

use crate::document::{
    Anchor, Card, CardId, DocView, PinTarget, VIEW_SCALE, Wire, is_port, port_ref,
};
use crate::i18n::Strings;
use crate::picker::Origin;
use crate::theme;
use crate::widgets::{self, ItemPicture, LimitField, SINK_IMAGE, SOURCE_IMAGE, colors};
use aic_editor::format;
use aic_editor::names;
use aic_editor::{Document, Session};

const CANVAS_ID: &str = "aic-canvas";
/// The warning sign before a card's title, at 100%.
const WARNING_ICON: f32 = 13.0;
// Sizes below are in points at 100%. Inside the canvas everything is laid out at the largest zoom
// and drawn shrunk, so text is drawn at the size it is shown and stays sharp up to that zoom;
// `crisp` turns a size into view space there. Text styles and egui's spacing are scaled once a
// frame, for the whole canvas (`Cards::scale_style`).
/// How far from a port a node added from it is placed.
const PORT_GAP: f32 = 80.0;
/// Size of the facility picture on a recipe card (and the depot picture on a source or sink).
const FACILITY_IMAGE: f32 = 64.0;
/// Size of a port's item icon. The icon is the port's pin: it sits inside the card against its
/// edge, and a link is dragged from it.
const PIN_ICON: f32 = 48.0;
/// Space between two ports of one side.
const PIN_GAP: f32 = 4.0;
/// Room for a port's rate, like `100.00`.
const RATE_WIDTH: f32 = 54.0;
/// Space between a port's icon and its rate.
const RATE_GAP: f32 = 4.0;
/// The card's inner margin.
const NODE_MARGIN: f32 = 6.0;
/// The card's outline.
const CARD_STROKE: f32 = 1.0;
/// Every card is this wide inside its outline, whatever it shows, so cards line up.
const CARD_WIDTH: f32 = 300.0;
/// A workspace's own port, on the canvas's edge, is a smaller card: this wide inside its margin.
const PORT_CARD_WIDTH: f32 = 140.0;
/// How far a port column reaches into the card, from inside its margin.
const PORT_COLUMN: f32 = PIN_ICON - NODE_MARGIN + RATE_GAP + RATE_WIDTH;
/// Room left of a card's title, and the least a header is high.
const HEADER_INDENT: f32 = 22.0;
const HEADER_MIN: f32 = 14.0;
/// A card's corner radius, in view space.
const CARD_RADIUS: u8 = theme::radius::MEDIUM * VIEW_SCALE as u8;
/// How wide a link is drawn, and how much wider under the pointer.
const LINK_WIDTH: f32 = 2.0;
const LINK_HOVER: f32 = 1.5;
/// How a link bends: it leaves each port level, by half the way to the other port and no more
/// than this.
const LINK_BEND: f32 = 144.0;
/// Room left at the top of a workspace's canvas for the back button and the add-port buttons.
const EDGE_TOP: f32 = 48.0;
/// Space between a workspace's port tiles and the canvas edge, and between two tiles.
const EDGE_GAP: f32 = 8.0;
/// The canvas grid's spacing.
const GRID: f32 = 40.0;
/// Room left around the cards when the canvas is fitted to them, in view space.
const FIT_MARGIN: f32 = 100.0;
/// How far the canvas zooms out and in. The canvas is laid out at `MAX_ZOOM`, which is what the
/// document's view space is scaled by.
pub const MIN_ZOOM: f32 = 0.2;
pub const MAX_ZOOM: f32 = VIEW_SCALE;
/// The wheel and the slider zoom in whole steps of this: 65%, 70%, never 67%.
pub const ZOOM_STEP: f32 = 0.05;
/// Below this zoom a card is drawn plain: its header tint, its pins' pictures and its number,
/// without the texts nobody can read there and without its limit field. Its small text is
/// [`theme::text::SMALL`] points at 100% (12), so 5.4 points high at 45% and 4.8 at 40%, the next
/// 5% step down: under about 5 pixels on a screen of one pixel per point. A plain card keeps its
/// size and its pins' places, since it is laid out as before and only not painted.
/// Link labels go too, but for the selected link's.
pub const PLAIN_BELOW: f32 = 0.45;

/// The zooms the status bar's − and + step through.
const ZOOM_STEPS: [f32; 7] = [0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 2.0];

/// A size in points at 100% as a size in view space, where the canvas lays its cards out.
const fn crisp(points: f32) -> f32 {
    points * VIEW_SCALE
}

/// How far a card's outline is from its content, in view space.
const CARD_OUTSET: f32 = crisp(NODE_MARGIN + CARD_STROKE);

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

pub use aic_editor::Selection;

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
    /// The selection is now this: a click, a Shift+click or a box changed it.
    Select(Option<Selection>),
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

/// One pin on the canvas: its card, its side and its place on that side.
type PinKey = (Member, Side, usize);

/// What to fit the canvas to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fit {
    /// Where the cards are placed: known before they are first drawn.
    Places,
    /// The cards as drawn last.
    Cards,
}

/// View state that lives across frames.
pub struct Canvas {
    /// Sets this view's ids apart from other views' of the same graph.
    id: Id,
    /// What the player is typing in each card's limit field.
    limit_edits: HashMap<NodeId, String>,
    /// View space to screen space, as of the last frame.
    to_global: TSTransform,
    /// The canvas area on screen, as of the last frame.
    pub rect: Rect,
    /// Where each node and workspace card was drawn last frame, in view space.
    node_rects: HashMap<Member, Rect>,
    /// The middle of each pin's icon as last drawn, from its card's corner, in view space.
    pins: HashMap<PinKey, Vec2>,
    /// Each level's camera, from the canvas's corner: a level keeps where it was looked at while
    /// another is shown, wherever the canvas is on screen by then.
    cameras: HashMap<Option<WorkspaceId>, TSTransform>,
    /// The pin a link is being dragged from.
    linking: Option<PinKey>,
    /// A camera to show from the next frame at a level, set when a file or a view opens.
    pending_view: Option<(Option<WorkspaceId>, aic_core::CanvasView)>,
    /// A zoom to show from the next frame, around the middle of the canvas.
    pending_zoom: Option<f32>,
    /// A fit of the canvas to its cards, from the next frame.
    pending_fit: Option<Fit>,
    /// Wheel scroll not yet turned into a zoom step, as a factor. Small scrolls add up here until
    /// they reach the next 5%.
    wheel_rest: f32,
    /// The level shown last frame.
    level: Option<WorkspaceId>,
    /// Where the top of the graph was looked at last, kept while a workspace is open so a save
    /// still records it.
    top_view: Option<aic_core::CanvasView>,
    /// The workspace name being typed in a card's menu.
    title_edit: String,
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
            limit_edits: HashMap::new(),
            to_global: TSTransform::IDENTITY,
            rect: Rect::ZERO,
            node_rects: HashMap::new(),
            pins: HashMap::new(),
            cameras: HashMap::new(),
            linking: None,
            pending_view: None,
            pending_zoom: None,
            pending_fit: None,
            wheel_rest: 1.0,
            level: None,
            top_view: None,
            title_edit: String::new(),
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
            zoom: self.to_global.scaling * VIEW_SCALE,
        })
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

    /// Fits the canvas to where its cards are placed, from the next frame on: what a level shows
    /// the first time it is looked at.
    pub fn fit(&mut self) {
        self.pending_fit = Some(Fit::Places);
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

    /// Where the port of `item` on `node`'s `side` was drawn last, on screen: the middle of its
    /// icon.
    pub fn port_on_screen(
        &self,
        view: &DocView,
        doc: &Document,
        node: NodeId,
        side: Side,
        item: &ItemId,
    ) -> Option<Pos2> {
        let pin = view.pin_of(doc, node, side, item)?;
        let card = self.node_rects.get(&pin.member)?;
        let offset = self.pins.get(&(pin.member, side, pin.index))?;
        Some(self.to_global * (card.min + *offset))
    }

    pub fn show(
        &mut self,
        ui: &mut Ui,
        view: &mut DocView,
        session: &Session,
        strings: &Strings,
    ) -> Vec<CanvasEvent> {
        let (doc, catalog) = (session.document(), session.catalog());
        let selection = session.selection();
        self.rect = ui.max_rect();
        let level = view.level();
        if level != self.level {
            if self.level.is_none() {
                self.top_view = self.view();
            }
            self.level = level;
            self.node_rects.clear();
            self.pins.clear();
            self.linking = None;
        }
        Faces::refresh(view, doc, session.round_up());
        let path = doc.level_path(level);
        let graph = doc.graph();
        let camera = self.camera(ui, level, view);
        self.to_global = camera;

        let selected: HashSet<CardId> = selection
            .map(|selection| {
                (selection.items().iter())
                    .filter_map(|member| view.card(*member).map(|card| card.id))
                    .collect()
            })
            .unwrap_or_default();
        // The widget takes the places and moves them as the player drags; the rest of the view
        // is only read while it draws.
        let scene_rect = camera.inverse() * self.rect;
        let mut placed = aic_node_graph::View {
            scene_rect,
            layout: std::mem::take(&mut view.layout),
        };
        let mut anchors = std::mem::take(&mut view.anchors);
        let graph_id = self.id.with(level);
        let labels = LayerId::new(ui.layer_id().order, graph_id.with("link-labels"));
        let mut cards = Cards {
            id: self.id,
            catalog,
            strings,
            graph,
            read: Read {
                catalog,
                strings,
                graph,
                solution: doc.solution(),
            },
            round_up: session.round_up(),
            // Within a hundredth of a step of the threshold counts as on it.
            plain: camera.scaling * VIEW_SCALE < PLAIN_BELOW - ZOOM_STEP * 0.01,
            selection,
            view,
            anchors: &mut anchors,
            limit_edits: &mut self.limit_edits,
            pins: &mut self.pins,
            node_rects: &mut self.node_rects,
            linking: &mut self.linking,
            title_edit: &mut self.title_edit,
            to_global: camera,
            level,
            canvas_rect: self.rect,
            ended: None,
            cancelled: false,
            events: Vec::new(),
            limit_texts: (
                strings.get("node-no-limit"),
                strings.get("node-limit-label"),
            ),
        };
        let mut clicked_label = None;
        let mut link_clicked = false;
        let response = aic_node_graph::Graph::from_id(graph_id)
            .background(false)
            .dot_grid(false)
            .zoom_range(MIN_ZOOM / VIEW_SCALE..=MAX_ZOOM / VIEW_SCALE)
            // The camera is the canvas's: the view is shown as given, at any size.
            .resize_behavior(ResizeBehavior::MaintainView)
            .auto_pan(None)
            .snap(None)
            .align(false)
            .selected_nodes(selected)
            .selection_area(
                colors::SELECT_BOX.gamma_multiply(0.15),
                Stroke::new(1.0, colors::SELECT_BOX),
            )
            .show(&mut placed, ui, |ui, show| {
                Cards::scale_style(ui);
                // The view this frame shows, after any pan the widget made.
                cards.to_global = layer_transform(ui);
                paint_background(ui);
                let painter = label_painter(ui, labels, cards.to_global);
                show.nodes(ui, |nodes, ui| cards.show(nodes, ui))
                    .edges(ui, |edges, ui| {
                        (clicked_label, link_clicked) = cards.wires(edges, ui, &painter);
                    });
            });
        let mut events = std::mem::take(&mut cards.events);
        view.layout = placed.layout;
        view.anchors = anchors;
        if placed.scene_rect != scene_rect {
            // The widget panned: the same zoom, looking at another place.
            let middle = placed.scene_rect.center().to_vec2();
            let translation = self.rect.center().to_vec2() - camera.scaling * middle;
            self.set_camera(ui, level, TSTransform::new(translation, camera.scaling));
        }

        if let Some(changed) = &response.selection_changed {
            let items: Vec<Member> = (view.cards().iter())
                .filter(|card| changed.contains(&card.id))
                .map(|card| card.member)
                .collect();
            events.push(CanvasEvent::Select(Selection::of_items(items)));
        }
        let background = &response.response;
        let modifiers = ui.input(|i| i.modifiers);
        if let Some(link) = clicked_label {
            events.push(CanvasEvent::Select(Some(Selection::Link(link))));
        } else if background.clicked()
            && response.selection_changed.is_none()
            && !modifiers.shift
            && !modifiers.command
        {
            // A link stays selected until a click on the background lets go of it.
            events.push(CanvasEvent::Select(None));
        }
        if background.double_clicked() {
            self.pending_fit = Some(Fit::Cards);
            ui.ctx().request_repaint();
        }
        // A right click on a link removes the link; anywhere else on the background it opens the
        // picker there.
        if let Some(pos) = response.background_secondary_click
            && !link_clicked
        {
            events.push(CanvasEvent::OpenPicker {
                origin: Origin::Canvas,
                graph_pos: pos / VIEW_SCALE,
            });
        }
        self.level_bar(ui, graph, &path, strings, &mut events);
        events
    }
}

impl Canvas {
    /// The camera this frame: the level's own, or one made for a level shown for the first
    /// time, then moved by what was asked for since the last frame (a fit, a zoom, a view to
    /// look at) and by the wheel.
    fn camera(&mut self, ui: &Ui, level: Option<WorkspaceId>, view: &DocView) -> TSTransform {
        let corner = self.rect.min.to_vec2();
        let mut camera = match self.cameras.get(&level) {
            Some(kept) => TSTransform::new(kept.translation + corner, kept.scaling),
            // A level with cards is first shown fitted to them. An empty one starts at 100%.
            None if view.cards().is_empty() => TSTransform::from_scaling(1.0 / VIEW_SCALE),
            None => self.fitted(Fit::Places, view).unwrap_or_default(),
        };
        if let Some(fit) = self.pending_fit.take()
            && let Some(fitted) = self.fitted(fit, view)
        {
            camera = fitted;
        }
        match self.pending_zoom.take() {
            Some(zoom) => zoom_about(&mut camera, self.rect.center(), zoom),
            None => self.scroll(ui, &mut camera),
        }
        if let Some((_, view)) = self.pending_view.take_if(|(at, _)| *at == level) {
            // Screen = zoom × graph + translation, with the view's corner at the canvas corner;
            // the camera's scaling is in view space.
            let at = Pos2::new(view.x, view.y).to_vec2() * view.zoom;
            camera = TSTransform::new(corner - at, view.zoom / VIEW_SCALE);
        }
        self.set_camera(ui, level, camera)
    }

    /// Keeps `camera` as the level's, on whole pixels, and returns it so.
    ///
    /// egui lays text out on whole pixels and then moves it by the camera; an offset between
    /// pixels would smear every glyph on the canvas. At 100% the text is then as sharp as the
    /// panels'; at other zooms egui scales the laid-out text.
    fn set_camera(
        &mut self,
        ui: &Ui,
        level: Option<WorkspaceId>,
        mut camera: TSTransform,
    ) -> TSTransform {
        let ppp = ui.ctx().pixels_per_point();
        camera.translation = (camera.translation * ppp).round() / ppp;
        let kept = TSTransform::new(camera.translation - self.rect.min.to_vec2(), camera.scaling);
        if self.cameras.insert(level, kept) != Some(kept) {
            // What was drawn before the canvas this frame, such as the status bar's zoom, shows
            // the camera of the frame before: draw once more.
            ui.ctx().request_repaint();
        }
        self.to_global = camera;
        camera
    }

    /// The camera that shows all the cards, with room around them, as large as they fit and the
    /// zoom allows. `None` with no cards to fit.
    fn fitted(&self, fit: Fit, view: &DocView) -> Option<TSTransform> {
        let mut area = Rect::NOTHING;
        match fit {
            Fit::Places => view.layout.values().for_each(|pos| area.extend_with(*pos)),
            Fit::Cards => (self.node_rects.values()).for_each(|rect| area = area.union(*rect)),
        }
        if !area.is_finite() {
            return None;
        }
        let area = area.expand(FIT_MARGIN);
        let scaling = (self.rect.size() / area.size())
            .min_elem()
            .clamp(MIN_ZOOM / VIEW_SCALE, MAX_ZOOM / VIEW_SCALE);
        let translation = self.rect.center().to_vec2() - area.center().to_vec2() * scaling;
        Some(TSTransform::new(translation, scaling))
    }

    /// What the wheel, a trackpad or a pinch does to the camera this frame. The widget leaves
    /// them all to the canvas.
    ///
    /// A scroll over the canvas zooms around the pointer in whole 5% steps, whatever the wheel
    /// reports, lines or points (a Magic Mouse, a smooth-scrolling wheel, a trackpad). With Shift
    /// it pans, as everywhere. A pinch, or Ctrl with the wheel, zooms smoothly. Dragging the
    /// background pans; that is the widget's.
    fn scroll(&mut self, ui: &Ui, camera: &mut TSTransform) {
        let (delta, pinch, pos, modifiers) = ui.input(|i| {
            (
                i.smooth_scroll_delta,
                i.zoom_delta(),
                i.pointer.latest_pos(),
                i.modifiers,
            )
        });
        let Some(pos) = pos else {
            return;
        };
        let over_canvas = self.rect.contains(pos)
            && ui
                .ctx()
                .layer_id_at(pos)
                .is_none_or(|layer| layer.order == ui.layer_id().order);
        if !over_canvas {
            return;
        }
        let current = camera.scaling * VIEW_SCALE;
        if pinch != 1.0 {
            zoom_about(camera, pos, current * pinch);
            return;
        }
        if delta == Vec2::ZERO {
            return;
        }
        ui.ctx().input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO);
        if modifiers.shift {
            camera.translation += delta;
            return;
        }
        let speed = ui.ctx().options(|o| o.input_options.scroll_zoom_speed);
        let wanted = current * self.wheel_rest * (speed * (delta.x + delta.y)).exp();
        let zoom = round_zoom(wanted);
        // Past either end of the range, the rest of the scroll is dropped.
        self.wheel_rest = if (MIN_ZOOM..=MAX_ZOOM).contains(&wanted) {
            wanted / zoom
        } else {
            1.0
        };
        if (zoom - current).abs() > ZOOM_STEP * 0.01 {
            zoom_about(camera, pos, zoom);
        }
    }

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
}

/// Zooms `camera` to `zoom` (1 is 100%), holding still the point at `fixed` on screen.
fn zoom_about(camera: &mut TSTransform, fixed: Pos2, zoom: f32) {
    let under = camera.inverse() * fixed;
    // The camera's scaling is in view space.
    let scaling = zoom.clamp(MIN_ZOOM, MAX_ZOOM) / VIEW_SCALE;
    *camera = TSTransform::new(fixed.to_vec2() - under.to_vec2() * scaling, scaling);
}

/// How the layer `ui` is in is drawn on screen. egui keeps no transform for a layer drawn as it
/// is.
fn layer_transform(ui: &Ui) -> TSTransform {
    ui.ctx()
        .layer_transform_to_global(ui.layer_id())
        .unwrap_or_default()
}

/// The canvas's colour and its grid, behind everything, over what the canvas shows.
fn paint_background(ui: &Ui) {
    let viewport = ui.clip_rect();
    let painter = ui.painter();
    painter.rect_filled(viewport, 0.0, theme::color::CANVAS);
    let stroke = Stroke::new(crisp(1.0), theme::color::GRID);
    let spacing = crisp(GRID);
    let lines = |from: f32, to: f32| {
        let first = (from / spacing).ceil() as i64;
        let last = (to / spacing).floor() as i64;
        (first..=last).map(move |line| line as f32 * spacing)
    };
    for x in lines(viewport.left(), viewport.right()) {
        painter.line_segment(
            [pos2(x, viewport.top()), pos2(x, viewport.bottom())],
            stroke,
        );
    }
    for y in lines(viewport.top(), viewport.bottom()) {
        painter.line_segment(
            [pos2(viewport.left(), y), pos2(viewport.right(), y)],
            stroke,
        );
    }
}

/// A painter over the cards, in view space: link labels are read over a card they cross.
///
/// The widget draws its wires under the cards, on the layer `ui` is in, and each card on a layer
/// of its own above that one. This layer goes above them all.
fn label_painter(ui: &Ui, layer: LayerId, to_global: TSTransform) -> egui::Painter {
    let ctx = ui.ctx();
    ctx.set_sublayer(ui.layer_id(), layer);
    ctx.move_to_top(layer);
    ctx.set_transform_layer(layer, to_global);
    // The layer is drawn through the camera, so its clip rect is in view space too. Set, not
    // narrowed: a layer's painter starts clipped to the screen, which is in other coordinates.
    let mut painter = ctx.layer_painter(layer);
    painter.set_clip_rect(ui.clip_rect());
    painter
}

fn side_of(kind: SocketKind) -> Side {
    match kind {
        SocketKind::Input => Side::Input,
        SocketKind::Output => Side::Output,
    }
}

fn kind_of(side: Side) -> SocketKind {
    match side {
        Side::Input => SocketKind::Input,
        Side::Output => SocketKind::Output,
    }
}

/// What the cards and wires of a level show, worked out once per edit, solve or switch of the
/// round-up view rather than every frame (PLAN.md, M12): the canvas draws from it and looks
/// nothing up while it draws. Kept in the [`DocView`], in its cards' and wires' order. Each is
/// worked out the first time it is drawn after a change, so a card out of view costs nothing.
#[derive(Default)]
pub(crate) struct Faces {
    /// The document's generation and count of solves, and the round-up view, it is for.
    key: Option<(u64, u64, bool)>,
    cards: Vec<OnceCell<Face>>,
    /// For each wire, the label of each of its links; `None` for a link with no flow to show.
    wires: Vec<OnceCell<Vec<Option<LinkLabel>>>>,
}

impl Faces {
    /// Forgets every face if the document, its solution or the round-up view changed since they
    /// were worked out. The strings are the editor's for its whole life.
    fn refresh(view: &mut DocView, doc: &Document, round_up: bool) {
        let key = Some((doc.generation(), doc.solves(), round_up));
        if view.faces.key == key && view.faces.cards.len() == view.cards().len() {
            return;
        }
        view.faces = Faces {
            key,
            cards: std::iter::repeat_with(OnceCell::new)
                .take(view.cards().len())
                .collect(),
            wires: std::iter::repeat_with(OnceCell::new)
                .take(view.wires().len())
                .collect(),
        };
    }
}

/// What one card shows.
#[derive(Default)]
struct Face {
    /// The small line over the title: what kind of node it is.
    kind: Option<String>,
    /// What the card is about, or the workspace's name; `None` on a port's tile.
    title: Option<String>,
    /// The recipe whose region badges go before the title, when only some regions have it.
    badges: Option<RecipeId>,
    /// The region warnings, a line each, on the sign before the title.
    warnings: Option<String>,
    /// A recipe card's facility, and whether a click on it moves to another.
    facility: Option<(String, bool)>,
    /// A storage's balance, and its colour.
    balance: Option<(String, Color32)>,
    body: Body,
    /// Each pin's rate as text and whether its node is held back there: inputs, then outputs.
    pins: [Vec<PinFace>; 2],
}

/// What a card shows between its port columns.
#[derive(Default)]
enum Body {
    /// A workspace's port tile: nothing.
    #[default]
    Empty,
    /// A picture, the number the card is about, what goes under it, and the limit.
    Numbers {
        /// The facility's or the depot's picture, and its name on hover.
        image: Option<(ImageKind, String, Option<String>)>,
        /// The count or rate, or whole facilities in the round-up view; blank if undetermined.
        number: String,
        /// How busy the facilities are, in the round-up view.
        busy: Option<String>,
        /// What an outpost order earns, per coupon.
        coupons: Vec<(ItemId, String)>,
        /// The limit as its field shows it, and whether the node needs one.
        limit: Option<(String, bool)>,
    },
    /// A storage: how much goes straight through.
    PassThrough(String),
    /// A recipe node whose recipe the data lacks.
    Missing(String),
    /// A workspace: how many nodes it holds.
    Workspace(String),
}

/// One pin's rate as text, and whether its node is throttled at this port.
struct PinFace {
    rate: String,
    throttled: bool,
}

/// The label at the middle of a link: its flow and belts, and whether it is over one belt.
struct LinkLabel {
    text: String,
    over: bool,
}

/// What the faces are worked out from.
#[derive(Clone, Copy)]
struct Read<'a> {
    catalog: &'a Catalog,
    strings: &'a Strings,
    graph: &'a Graph,
    solution: &'a Solution,
}

impl Read<'_> {
    fn face(&self, card: &Card, round_up: bool) -> Face {
        let (catalog, strings) = (self.catalog, self.strings);
        let pins = [Side::Input, Side::Output].map(|side| {
            (card.pins(side).iter())
                .map(|pin| self.pin(pin, side))
                .collect()
        });
        let id = match card.member {
            Member::Workspace(workspace) => {
                let name = (self.graph.workspace_info(workspace))
                    .map(|w| w.name.clone())
                    .unwrap_or_default();
                let count = (self.graph.nodes_in_workspace(workspace).into_iter())
                    .filter(|&node| !is_port(self.graph, node))
                    .count();
                return Face {
                    kind: Some(strings.get("kind-workspace")),
                    title: Some(name),
                    body: Body::Workspace(names::node_count(strings, count)),
                    pins,
                    ..Face::default()
                };
            }
            Member::Node(id) => id,
        };
        let Some(kind) = self.graph.node(id).map(|node| &node.kind) else {
            return Face::default();
        };
        if let NodeKind::WorkspacePort { side, .. } = kind {
            let key = match side {
                Side::Input => "kind-workspace-input",
                Side::Output => "kind-workspace-output",
            };
            return Face {
                kind: Some(strings.get(key)),
                pins,
                ..Face::default()
            };
        }
        let warnings: Vec<String> = (self.solution.warnings.iter())
            .filter_map(|warning| match warning {
                Warning::Issue(
                    issue @ (Issue::RecipeOutsideRegion { node, .. }
                    | Issue::FacilityOutsideRegion { node, .. }),
                ) if *node == id => Some(aic_editor::messages::issue_text(catalog, strings, issue)),
                _ => None,
            })
            .collect();
        let mut face = Face {
            kind: (!matches!(kind, NodeKind::Recipe { .. }))
                .then(|| strings.get(names::kind_key(kind))),
            title: Some(names::node_subject(catalog, strings, kind)),
            warnings: (!warnings.is_empty()).then(|| warnings.join("\n")),
            body: self.body(id, kind, round_up),
            pins,
            ..Face::default()
        };
        if let NodeKind::Recipe {
            recipe, facility, ..
        } = kind
        {
            let found = catalog.recipe(recipe.as_str());
            face.badges = found
                .filter(|found| !widgets::recipe_regions(catalog, found).is_empty())
                .map(|_| recipe.clone());
            let choices = found.map_or(0, |found| found.facilities.len());
            face.facility = Some((
                names::facility(catalog, facility.as_str()).to_owned(),
                choices > 1,
            ));
        }
        if let NodeKind::Storage { .. } = kind
            && let Some(balance) = self.solution.storage.get(&id).and_then(|s| s.balance)
        {
            let color = if balance >= 0.0 {
                colors::POSITIVE
            } else {
                colors::NEGATIVE
            };
            let mut text = format::short(Some(balance));
            if balance >= 0.0 {
                text.insert(0, '+');
            }
            let text = strings.format("storage-balance", &[("balance", &text)]);
            face.balance = Some((text, color));
        }
        face
    }

    fn pin(&self, pin: &PinTarget, side: Side) -> PinFace {
        let (node, item) = (pin.node, pin.item.as_ref());
        let flow = item.and_then(|item| {
            let port = Port {
                node,
                item: item.clone(),
                side,
            };
            self.solution.port_flow.get(&port).copied().flatten()
        });
        let throttled = (self.solution.throttle.get(&node))
            .is_some_and(|reason| reason.side == side && Some(&reason.item) == item);
        PinFace {
            rate: format::short(flow),
            throttled,
        }
    }

    fn body(&self, id: NodeId, kind: &NodeKind, round_up: bool) -> Body {
        let (catalog, strings, solution) = (self.catalog, self.strings, self.solution);
        let rate = solution.rate.get(&id).copied().flatten();
        let limit = |limit: Option<f64>, required| Some((widgets::limit_text(limit), required));
        let numbers = |image, number: Option<f64>, limit| Body::Numbers {
            image,
            number: format::short(number),
            busy: None,
            coupons: Vec::new(),
            limit,
        };
        match kind {
            NodeKind::Recipe {
                facility,
                limit: most,
                ..
            } => {
                let name = names::facility(catalog, facility.as_str()).to_owned();
                let image = Some((
                    ImageKind::Facility,
                    facility.as_str().to_owned(),
                    Some(name),
                ));
                let count = solution.facility_count.get(&id).copied().flatten();
                match count.filter(|_| round_up).map(RoundUp::of) {
                    Some(rounded) => Body::Numbers {
                        image,
                        number: rounded.facilities.to_string(),
                        busy: Some(strings.format(
                            "round-up-utilization",
                            &[("percent", &format::short(Some(rounded.utilization * 100.0)))],
                        )),
                        coupons: Vec::new(),
                        limit: limit(*most, false),
                    },
                    None => numbers(image, count, limit(*most, false)),
                }
            }
            NodeKind::Source { limit: most, .. } => {
                let image = Some((ImageKind::Node, SOURCE_IMAGE.to_owned(), None));
                numbers(image, rate, limit(Some(*most), true))
            }
            NodeKind::ControlPort { limit: most, .. } => {
                numbers(None, rate, limit(Some(*most), true))
            }
            NodeKind::Sink { limit: most, .. } => {
                let image = Some((ImageKind::Node, SINK_IMAGE.to_owned(), None));
                numbers(image, rate, limit(*most, false))
            }
            NodeKind::OutpostOrder { limit: most, .. } => Body::Numbers {
                image: None,
                number: format::short(rate),
                busy: None,
                coupons: (solution.coupons.get(&id).into_iter().flatten())
                    .map(|(coupon, earned)| {
                        let text = strings
                            .format("outpost-earns", &[("rate", &format::short(Some(*earned)))]);
                        (coupon.clone(), text)
                    })
                    .collect(),
                limit: limit(*most, false),
            },
            NodeKind::Splitter { .. } | NodeKind::Merger { .. } => numbers(None, rate, None),
            NodeKind::Storage { .. } => {
                let through = solution.storage.get(&id).and_then(|s| s.pass_through);
                Body::PassThrough(
                    strings.format("storage-pass-through", &[("rate", &format::short(through))]),
                )
            }
            NodeKind::WorkspacePort { .. } => Body::Empty,
            NodeKind::MissingRecipe { .. } => Body::Missing(strings.get("kind-missing-recipe")),
        }
    }

    /// Flow and belt count at the middle of a link, like `12.00 /min · 0.4 belts`. An
    /// undetermined link shows no label at all.
    fn label(&self, id: LinkId) -> Option<LinkLabel> {
        let link = self.graph.link(id)?;
        let flow = self.solution.link_flow.get(&id).copied().flatten()?;
        let fluid = (self.catalog.item(link.item.as_str())).is_some_and(|item| item.is_fluid);
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
        Some(LinkLabel {
            text,
            over: belts.is_some_and(|b| b > 1.0 + 1e-9),
        })
    }
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

/// Where the parts of a card go, in view space. A card is laid out by hand: its parts line up
/// across cards, and nothing is measured twice.
struct CardLayout {
    /// The top-left of the card's content, inside its margin.
    origin: Pos2,
    /// How wide the content is.
    width: f32,
}

impl CardLayout {
    /// The card's outline, left and right.
    fn left(&self) -> f32 {
        self.origin.x - CARD_OUTSET
    }

    fn right(&self) -> f32 {
        self.origin.x + self.width + CARD_OUTSET
    }

    fn top(&self) -> f32 {
        self.origin.y - CARD_OUTSET
    }

    /// The card's corner, which its position is.
    fn corner(&self) -> Pos2 {
        pos2(self.left(), self.top())
    }

    /// Where a port's icon goes: against the card's edge, over its margin.
    fn icon(&self, side: Side, top: f32) -> Rect {
        let size = crisp(PIN_ICON);
        let left = match side {
            Side::Input => self.left(),
            Side::Output => self.right() - size,
        };
        Rect::from_min_size(pos2(left, top), Vec2::splat(size))
    }

    /// Where a port's rate goes: beside its icon, to the end of the port column.
    fn rate(&self, side: Side, top: f32) -> Rect {
        let (near, far) = (crisp(PORT_COLUMN - RATE_WIDTH), crisp(PORT_COLUMN));
        let (left, right) = match side {
            Side::Input => (self.origin.x + near, self.origin.x + far),
            Side::Output => (
                self.origin.x + self.width - far,
                self.origin.x + self.width - near,
            ),
        };
        Rect::from_min_max(pos2(left, top), pos2(right, top + crisp(PIN_ICON)))
    }

    /// The room between the port columns, from `top` down. It is as wide and in the same place
    /// whichever ports the card has, so the facility sits in the middle of every card.
    fn body(&self, top: f32, gap: f32) -> Rect {
        let column = crisp(PORT_COLUMN) + gap;
        Rect::from_min_max(
            pos2(self.origin.x + column, top),
            pos2(self.origin.x + self.width - column, f32::INFINITY),
        )
    }
}

/// Draws the cards and wires of one frame, and collects what the player did on them.
struct Cards<'a> {
    /// The view's id ([`Canvas::id`]), which the ids of its widgets start from.
    id: Id,
    catalog: &'a Catalog,
    strings: &'a Strings,
    graph: &'a Graph,
    /// What the cards' faces are worked out from, the first time each is drawn.
    read: Read<'a>,
    /// Show whole facilities and how busy they are.
    round_up: bool,
    /// The canvas is zoomed out below [`PLAIN_BELOW`]: cards are drawn plain, links without
    /// their labels.
    plain: bool,
    selection: Option<&'a Selection>,
    /// The level's cards and what they show ([`Faces`]).
    view: &'a DocView,
    anchors: &'a mut HashMap<NodeId, Anchor>,
    limit_edits: &'a mut HashMap<NodeId, String>,
    pins: &'a mut HashMap<PinKey, Vec2>,
    node_rects: &'a mut HashMap<Member, Rect>,
    linking: &'a mut Option<PinKey>,
    /// The workspace name being typed in a card's menu.
    title_edit: &'a mut String,
    /// View space to screen space, this frame.
    to_global: TSTransform,
    /// The workspace shown, whose ports sit on the canvas's edges.
    level: Option<WorkspaceId>,
    /// The canvas area on screen.
    canvas_rect: Rect,
    /// The pin a link was dropped on this frame.
    ended: Option<PinKey>,
    /// A link was dropped where it can't end this frame.
    cancelled: bool,
    events: Vec<CanvasEvent>,
    /// Every card's limit field's hint and name, looked up once a frame.
    limit_texts: (String, String),
}

impl Cards<'_> {
    /// Lays the canvas out at its largest zoom, so text is drawn at the size it is shown at up
    /// to that zoom. Cards size to their content: text never wraps inside them, and it can't be
    /// selected, so a drag on it moves the card.
    fn scale_style(ui: &mut Ui) {
        let style = ui.style_mut();
        style.scale(VIEW_SCALE);
        style.wrap_mode = Some(egui::TextWrapMode::Extend);
        style.interaction.selectable_labels = false;
    }

    /// The frame of every card: a raised surface with a hairline outline and a short shadow, as
    /// thick whether or not the card is selected, so selecting moves nothing.
    fn frame() -> egui::Frame {
        theme::node_frame()
            .scaled(VIEW_SCALE)
            .corner_radius(CornerRadius::same(CARD_RADIUS))
            .inner_margin(egui::Margin::same(crisp(NODE_MARGIN) as i8))
            .stroke(Stroke::new(crisp(CARD_STROKE), theme::color::BORDER))
    }

    fn limit_id(&self, node: NodeId) -> Id {
        self.id.with(("node-limit", node))
    }

    /// Shows every card of the level, and settles what the frame's link drag came to.
    fn show(&mut self, nodes: &mut NodesCtx, ui: &mut Ui) {
        self.pin_ports_to_edges(nodes, ui.ctx());
        let view = self.view;
        let focused = ui.memory(|m| m.focused());
        let (read, round_up) = (self.read, self.round_up);
        for (card, face) in view.cards().iter().zip(&view.faces.cards) {
            let face = || face.get_or_init(|| read.face(card, round_up));
            let node = Node::from_id(card.id)
                .inputs(card.inputs.len())
                .outputs(card.outputs.len())
                .max_width(crisp(CARD_WIDTH))
                // The card draws its pins itself: each port's icon is its socket's handle.
                .paint_sockets(false)
                .animation_time(0.0);
            // The card being typed in keeps its field, in view or not.
            let typing =
                matches!(card.member, Member::Node(id) if focused == Some(self.limit_id(id)));
            if typing {
                let shown = node.show(nodes, ui, |ctx| self.card(ctx, card, face(), false));
                self.after(ui, nodes, card, &shown);
            } else {
                let plain = self.plain;
                let shown = node.show_or_cull(nodes, ui, |ctx| self.card(ctx, card, face(), plain));
                self.after(ui, nodes, card, &shown);
            }
        }
        self.settle_link(ui);
    }

    /// What follows a card once it has shown: where it is, the edge it should land on, its menu,
    /// and what the pointer did to a link at it.
    fn after<R>(&mut self, ui: &Ui, nodes: &mut NodesCtx, card: &Card, shown: &NodeResponse<R>) {
        let rect = shown.rect;
        self.node_rects.insert(card.member, rect);
        // Move a node added from a port so the edge facing that port sits where it was dropped.
        if let Member::Node(id) = card.member
            && let Some(&anchor) = self.anchors.get(&id)
        {
            let dx = match anchor {
                Anchor::Left(x) => x * VIEW_SCALE - rect.left(),
                Anchor::Right(x) => x * VIEW_SCALE - rect.right(),
            };
            if dx.abs() < 0.5 {
                self.anchors.remove(&id);
            } else if let Some(pos) = nodes.position(card.id) {
                nodes.set_position(card.id, pos + vec2(dx, 0.0));
                ui.ctx().request_repaint();
            }
        }
        // A double-click on a workspace's card opens it.
        if let Member::Workspace(workspace) = card.member
            && shown.double_clicked()
        {
            self.events.push(CanvasEvent::ShowLevel(Some(workspace)));
        }
        match shown.edge_event() {
            Some(EdgeEvent::Started { kind, index }) => {
                *self.linking = Some((card.member, side_of(kind), index));
            }
            Some(EdgeEvent::Ended { kind, index }) => {
                self.ended = Some((card.member, side_of(kind), index));
            }
            Some(EdgeEvent::Cancelled) => self.cancelled = true,
            None => {}
        }
        shown.context_menu(|ui| self.menu(ui, card.member));
    }

    /// A link dropped on a pin links the two ports. Dropped on empty canvas, it opens the picker
    /// there, for a node to link to.
    fn settle_link(&mut self, ui: &Ui) {
        let dropped = self.ended.take();
        if dropped.is_none() && !std::mem::take(&mut self.cancelled) {
            return;
        }
        let Some(start) = self.linking.take() else {
            return;
        };
        if let Some(end) = dropped {
            let (from, to) = match start.1 {
                Side::Output => (start, end),
                Side::Input => (end, start),
            };
            if let (Some(from), Some(to)) = (self.port_ref(from), self.port_ref(to)) {
                self.events.push(CanvasEvent::Connect { from, to });
            }
            return;
        }
        let Some(pos) = ui.input(|i| i.pointer.interact_pos()) else {
            return;
        };
        let at = self.to_global.inverse() * pos;
        let on_canvas = self.canvas_rect.contains(pos)
            && !self.node_rects.values().any(|rect| rect.contains(at))
            && ui
                .ctx()
                .layer_id_at(pos)
                .is_none_or(|layer| layer.order == ui.layer_id().order);
        if !on_canvas {
            return;
        }
        let origin = match self.target(start) {
            Some(PinTarget {
                node,
                item: Some(item),
            }) => Origin::Port {
                node,
                side: start.1,
                item,
            },
            _ => Origin::Canvas,
        };
        self.events.push(CanvasEvent::OpenPicker {
            origin,
            graph_pos: at / VIEW_SCALE,
        });
    }

    /// Puts the shown workspace's own ports on the canvas's edges, whatever the camera: inputs
    /// down the left, outputs down the right, below the buttons at the top. Runs with this
    /// frame's camera, before the cards are drawn, so a tile never trails a pan or zoom.
    fn pin_ports_to_edges(&self, nodes: &mut NodesCtx, ctx: &egui::Context) {
        let Some(workspace) = self.level else {
            return;
        };
        let (canvas, from_screen) = (self.canvas_rect, self.to_global.inverse());
        for side in [Side::Input, Side::Output] {
            let mut top = canvas.top() + EDGE_TOP;
            for port in self.graph.ports_of(workspace, side) {
                let member = Member::Node(port);
                let Some(card) = self.view.card(member) else {
                    continue;
                };
                // Last frame's size; a new tile starts with a guess.
                let size = (self.node_rects.get(&member))
                    .map_or(vec2(80.0, 64.0), |r| r.size() * self.to_global.scaling);
                let left = match side {
                    Side::Input => canvas.left() + EDGE_GAP,
                    Side::Output => canvas.right() - EDGE_GAP - size.x,
                };
                let pos = from_screen * Pos2::new(left, top);
                // A new tile changes size as it settles; draw again until it stops.
                if (nodes.position(card.id)).is_none_or(|old| (old - pos).length() > 0.5) {
                    ctx.request_repaint();
                }
                nodes.set_position(card.id, pos);
                top += size.y + EDGE_GAP;
            }
        }
    }

    fn kind(&self, id: NodeId) -> Option<&NodeKind> {
        self.graph.node(id).map(|node| &node.kind)
    }

    /// The port a pin stands for: a node and its item, `None` for a logistics node without one.
    fn target(&self, (member, side, index): PinKey) -> Option<PinTarget> {
        self.view.card(member)?.pins(side).get(index).cloned()
    }

    fn port_ref(&self, pin: PinKey) -> Option<PortRef> {
        let target = self.target(pin)?;
        let kind = self.kind(target.node)?;
        Some(port_ref(kind, target.node, target.item.as_ref()))
    }

    /// The wires that end on a pin.
    fn wires_at(&self, (member, side, index): PinKey) -> impl Iterator<Item = &Wire> {
        self.view.wires().iter().filter(move |wire| {
            let end = match side {
                Side::Input => wire.to,
                Side::Output => wire.from,
            };
            end.member == member && end.index == index
        })
    }

    /// One card: its frame, its header, a pin for each port down its sides, and its body between
    /// them.
    fn card(
        &mut self,
        ctx: NodeCtx<'_>,
        card: &Card,
        face: &Face,
        plain: bool,
    ) -> aic_node_graph::FramedResponse<()> {
        let highlight = ctx.highlighted_sockets();
        ctx.framed_with(Self::frame(), |ui, sockets| {
            self.card_content(ui, sockets, (card, face, plain), highlight);
        })
    }

    fn card_content(
        &mut self,
        ui: &mut Ui,
        sockets: &mut SocketLayout,
        (card, face, plain): (&Card, &Face, bool),
        highlight: SocketHighlight,
    ) {
        // Borrowed from the graph, not from `self`, which the parts below draw with.
        let graph = self.graph;
        let kind = match card.member {
            Member::Node(id) => graph.node(id).map(|node| &node.kind),
            Member::Workspace(_) => None,
        };
        let is_port = matches!(kind, Some(NodeKind::WorkspacePort { .. }));
        let layout = CardLayout {
            origin: ui.cursor().min,
            width: crisp(if is_port {
                PORT_CARD_WIDTH
            } else {
                CARD_WIDTH - 2.0 * NODE_MARGIN
            }),
        };
        let spacing = ui.spacing().item_spacing;

        // The header's tint reaches the card's outline, under the header's text.
        let tint = ui.painter().add(egui::Shape::Noop);
        let indent = crisp(HEADER_INDENT);
        let room = Rect::from_min_size(
            layout.origin + vec2(indent, 0.0),
            vec2(layout.width - indent, f32::INFINITY),
        );
        let mut header = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(room)
                .layout(egui::Layout::top_down(egui::Align::Min))
                .id_salt("header"),
        );
        // Plain, the header is laid out as ever, for its height, and not drawn.
        if plain {
            header.set_invisible();
        }
        self.header(&mut header, card.member, face);
        let header_height = header.min_rect().height().max(crisp(HEADER_MIN));
        let header_bottom = layout.origin.y + header_height + CARD_OUTSET;
        ui.painter().set(
            tint,
            egui::epaint::RectShape::new(
                Rect::from_min_max(layout.corner(), pos2(layout.right(), header_bottom)),
                CornerRadius::same(CARD_RADIUS),
                header_tint(card.member, kind),
                Stroke::new(crisp(CARD_STROKE), theme::color::BORDER),
                egui::StrokeKind::Inside,
            ),
        );
        // A selected card's outline goes over the header and under the pins.
        let outline = ui.painter().add(egui::Shape::Noop);

        let top = header_bottom + spacing.y;
        let step = crisp(PIN_ICON + PIN_GAP);
        let mut bottom = top;
        for side in [Side::Input, Side::Output] {
            for (index, pin) in card.pins(side).iter().enumerate() {
                let row = top + index as f32 * step;
                let icon = layout.icon(side, row);
                match side {
                    Side::Input => {
                        sockets.input(index, icon);
                        sockets.input_handle(index, icon);
                    }
                    Side::Output => {
                        sockets.output(index, icon);
                        sockets.output_handle(index, icon);
                    }
                }
                let lit = highlight.contains(kind_of(side), index);
                let rate = layout.rate(side, row);
                let shows = &face.pins[usize::from(side == Side::Output)][index];
                self.pin(
                    ui,
                    &layout,
                    (card.member, side, index),
                    (pin, shows),
                    icon,
                    rate,
                    (lit, plain),
                );
                bottom = bottom.max(icon.bottom());
            }
        }

        if !is_port {
            let mut body = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(layout.body(top, spacing.x))
                    .layout(egui::Layout::top_down(egui::Align::Center))
                    .id_salt("body"),
            );
            self.body(&mut body, card.member, face, plain);
            bottom = bottom.max(body.min_rect().bottom());
        }
        let content =
            Rect::from_min_max(layout.origin, pos2(layout.origin.x + layout.width, bottom));
        ui.allocate_rect(content, Sense::hover());
        if self.selection.is_some_and(|s| s.contains(card.member)) {
            ui.painter().set(
                outline,
                egui::epaint::RectShape::stroke(
                    content.expand(CARD_OUTSET),
                    CornerRadius::same(CARD_RADIUS),
                    Stroke::new(crisp(2.0), colors::ACCENT),
                    egui::StrokeKind::Inside,
                ),
            );
        }
    }

    /// A card's header: what kind of node it is, what it is about, and for a recipe its facility.
    fn header(&mut self, ui: &mut Ui, member: Member, face: &Face) {
        if let Some(kind) = &face.kind {
            ui.label(RichText::new(kind).small().weak());
        }
        let Some(title) = &face.title else {
            return;
        };
        let Member::Node(id) = member else {
            title_label(ui, theme::strong(title));
            return;
        };
        self.title_row(ui, face, theme::strong(title));
        if let Some((name, cycles)) = &face.facility {
            self.facility_line(ui, id, name, *cycles);
        }
        if let Some((balance, color)) = &face.balance {
            ui.label(RichText::new(balance).color(*color).strong());
        }
    }

    /// A recipe card's facility, under its title where a long name has the card's width. With a
    /// choice of facilities it is a button: one click moves to the next, and the inspector has the
    /// full list.
    fn facility_line(&mut self, ui: &mut Ui, id: NodeId, name: &str, cycles: bool) {
        let text = RichText::new(name).small().color(theme::color::TEXT_MUTED);
        if !cycles {
            ui.add(egui::Label::new(text).truncate());
            return;
        }
        let button = egui::Button::new(text).frame(false).truncate();
        if !ui.add(button).clicked() {
            return;
        }
        let Some(NodeKind::Recipe {
            recipe, facility, ..
        }) = self.kind(id)
        else {
            return;
        };
        let facilities = (self.catalog.recipe(recipe.as_str()))
            .map(|r| r.facilities.as_slice())
            .unwrap_or_default();
        let next = (facilities.iter())
            .position(|f| f == facility)
            .map_or(0, |i| (i + 1) % facilities.len().max(1));
        if let Some(next) = facilities.get(next) {
            self.events.push(CanvasEvent::SetFacility(id, next.clone()));
        }
    }

    /// A node's title, after a warning sign when the region lacks its recipe or facility, and the
    /// recipe's region badges when only some regions have it.
    fn title_row(&self, ui: &mut Ui, face: &Face, title: RichText) {
        let catalog = self.catalog;
        let badges = (face.badges.as_ref())
            .and_then(|recipe| catalog.recipe(recipe.as_str()))
            .map(|recipe| widgets::RegionBadges::new(ui, catalog, recipe, VIEW_SCALE));
        if badges.is_none() && face.warnings.is_none() {
            title_label(ui, title);
            return;
        }
        ui.horizontal(|ui| {
            if let Some(warnings) = &face.warnings {
                widgets::warning_icon(ui, crisp(WARNING_ICON)).on_hover_text(warnings.as_str());
            }
            if let Some(badges) = &badges {
                badges.ui(ui);
            }
            ui.add(egui::Label::new(title).truncate());
        });
    }

    /// One port's pin: its item's icon inside the card against the edge, on a tile colored like
    /// the port (linked, or pulling from or sending to the depot), with its rate beside it.
    ///
    /// A link is dragged from the icon: it is the handle of the pin's socket, and the widget on
    /// it senses clicks only, which leaves the drag to the handle.
    #[allow(clippy::too_many_arguments)]
    fn pin(
        &mut self,
        ui: &mut Ui,
        layout: &CardLayout,
        key: PinKey,
        (target, shows): (&PinTarget, &PinFace),
        icon: Rect,
        rate_rect: Rect,
        (lit, plain): (bool, bool),
    ) {
        let (member, side, index) = key;
        let (node, item) = (target.node, target.item.as_ref());
        let linked = (self.view.card(member)).is_some_and(|card| card.is_linked(side, index));
        self.pins.insert(key, icon.center() - layout.corner());
        let throttled = shows.throttled;
        let (fill, color) = match (linked, side) {
            (true, _) => (colors::LINKED, ui.visuals().text_color()),
            (false, Side::Input) => (colors::DEPOT_IN, colors::DEPOT_IN),
            (false, Side::Output) => (colors::DEPOT_OUT, colors::DEPOT_OUT),
        };

        let response = ui.interact(
            icon,
            self.id.with(("pin", member, side, index)),
            Sense::click(),
        );
        // Under the pointer, or where a link being dragged would end, the icon grows.
        let tile = if response.hovered() || lit {
            icon.scale_from_center(1.2)
        } else {
            icon
        };
        let painter = ui.painter();
        // Links run behind cards, so a link seems to end at the card's edge, at this tile.
        painter.rect_filled(tile, crisp(4.0), fill.gamma_multiply(0.35));
        let picture = match item {
            Some(item) => ItemPicture::of(ui.ctx(), self.catalog, item.as_str()),
            // A port without an item yet asks for one.
            None => ItemPicture::unknown(),
        };
        picture.paint(painter, tile.shrink(crisp(3.0)));
        if throttled {
            paint_throttle_marker(painter, tile);
        }
        let (anchor, align) = match side {
            Side::Input => (
                icon.right_center() + vec2(crisp(RATE_GAP), 0.0),
                egui::Align2::LEFT_CENTER,
            ),
            Side::Output => (
                icon.left_center() - vec2(crisp(RATE_GAP), 0.0),
                egui::Align2::RIGHT_CENTER,
            ),
        };
        if !plain {
            painter.text(
                anchor,
                align,
                &shows.rate,
                egui::TextStyle::Monospace.resolve(ui.style()),
                color,
            );
        }

        // A right click on the icon unlinks the port or, if it has no links, adds a producer or
        // consumer of its item.
        if response.secondary_clicked() {
            if linked {
                let links: Vec<LinkId> = (self.wires_at(key))
                    .flat_map(|wire| wire.links.iter().copied())
                    .collect();
                self.events
                    .extend(links.into_iter().map(CanvasEvent::Disconnect));
            } else if let Some(item) = item {
                let offset = match side {
                    Side::Input => -PORT_GAP,
                    Side::Output => PORT_GAP,
                };
                self.events.push(CanvasEvent::OpenPicker {
                    origin: Origin::Port {
                        node,
                        side,
                        item: item.clone(),
                    },
                    graph_pos: icon.center() / VIEW_SCALE + vec2(offset, 0.0),
                });
            }
        }
        let Some(item) = item else {
            return;
        };
        let mut beside = ui.interact(
            rate_rect,
            self.id.with(("port-row", node, side, item)),
            Sense::click(),
        );
        // Every card has ports: their hints are only built for the one under the pointer.
        if beside.hovered() || beside.is_tooltip_open() {
            let mut hint = names::item(self.catalog, item.as_str()).to_owned();
            if throttled {
                hint.push('\n');
                hint.push_str(&self.strings.get("throttle-marker-hint"));
            }
            beside = beside.on_hover_text(hint);
        }
        // A double-click on the port, its icon or its rate, opens its menu: the limit as a rate,
        // and clearing its links. egui counts a quick third click as a triple click, not a
        // double one; either opens it.
        let twice = |r: &egui::Response| r.double_clicked() || r.triple_clicked();
        if twice(&response) || twice(&beside) {
            self.events.push(CanvasEvent::OpenPortMenu {
                port: Port {
                    node,
                    item: item.clone(),
                    side,
                },
                graph_pos: icon.center() / VIEW_SCALE,
            });
        }
    }

    /// A card's body, between its port columns: the picture, the number the card is about, and
    /// its limit.
    ///
    /// Plain, it draws the picture and the number, and lays out the rest without drawing it, so
    /// the card keeps its height.
    fn body(&mut self, ui: &mut Ui, member: Member, face: &Face, plain: bool) {
        match (&face.body, member) {
            (Body::Workspace(count), Member::Workspace(workspace)) => {
                self.workspace_body(ui, workspace, count, plain);
            }
            (
                Body::Numbers {
                    image,
                    number,
                    busy,
                    coupons,
                    limit,
                },
                Member::Node(id),
            ) => {
                if let Some((kind, image, name)) = image
                    && let Some(shown) =
                        widgets::image(ui, self.catalog, *kind, image, crisp(FACILITY_IMAGE))
                    && let Some(name) = name
                {
                    shown.on_hover_text(name.as_str());
                }
                big_number(ui, number);
                if plain {
                    ui.set_invisible();
                }
                if let Some(busy) = busy {
                    ui.label(RichText::new(busy).small());
                }
                for (coupon, earned) in coupons {
                    ui.horizontal(|ui| {
                        widgets::item_icon(ui, self.catalog, coupon.as_str(), crisp(16.0));
                        ui.label(RichText::new(earned).small());
                    });
                }
                if let Some((limit, required)) = limit {
                    self.limit_field(ui, id, limit, *required);
                }
            }
            (Body::PassThrough(text), _) => {
                if plain {
                    ui.set_invisible();
                }
                ui.add(egui::Label::new(RichText::new(text).small()).truncate());
            }
            (Body::Missing(text), _) => {
                if plain {
                    ui.set_invisible();
                }
                ui.label(RichText::new(text).color(colors::NEGATIVE));
            }
            _ => {}
        }
    }

    /// The node's limit under its numbers, typed straight into the card. A source or control port
    /// needs one, so blanking theirs changes nothing.
    fn limit_field(&mut self, ui: &mut Ui, id: NodeId, limit: &str, required: bool) {
        let field = LimitField {
            id: self.limit_id(id),
            buffer: self.limit_edits.entry(id).or_default(),
            current: limit,
            width: crisp(76.0),
            hint: self.limit_texts.0.clone(),
            label: self.limit_texts.1.clone(),
        };
        if let (_, Some(typed)) = field.show(ui)
            && (typed.is_some() || !required)
        {
            self.events.push(CanvasEvent::SetLimit(id, typed));
        }
    }

    /// A workspace's card: how many nodes it holds, and a button to open it.
    fn workspace_body(&mut self, ui: &mut Ui, workspace: WorkspaceId, count: &str, plain: bool) {
        ui.label(RichText::new("🏭").size(crisp(FACILITY_IMAGE * 0.6)));
        if plain {
            ui.set_invisible();
        }
        ui.label(RichText::new(count).small());
        if ui
            .small_button(self.strings.get("workspace-open"))
            .clicked()
        {
            self.events.push(CanvasEvent::ShowLevel(Some(workspace)));
        }
    }

    /// A card's menu, on a right click.
    fn menu(&mut self, ui: &mut Ui, member: Member) {
        let strings = self.strings;
        if let Member::Workspace(workspace) = member {
            self.menu_item(
                ui,
                "workspace-open",
                CanvasEvent::ShowLevel(Some(workspace)),
            );
            ui.horizontal(|ui| {
                ui.label(strings.get("workspace-title"));
                let name = (self.graph.workspace_info(workspace))
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
        self.menu_item(ui, "menu-cut", CanvasEvent::Cut(member));
        self.menu_item(ui, "menu-copy", CanvasEvent::Copy(member));
        if let Member::Workspace(workspace) = member {
            self.menu_item(
                ui,
                "workspace-paste-into",
                CanvasEvent::PasteInto(workspace),
            );
            self.menu_item(ui, "workspace-dissolve", CanvasEvent::Dissolve(workspace));
        }
        let remove = match member {
            Member::Node(_) => "node-remove",
            Member::Workspace(_) => "workspace-remove",
        };
        self.menu_item(ui, remove, CanvasEvent::Remove(member));
    }

    fn menu_item(&mut self, ui: &mut Ui, key: &str, event: CanvasEvent) {
        if ui.button(self.strings.get(key)).clicked() {
            self.events.push(event);
            ui.close();
        }
    }

    /// Draws the level's wires, the link being dragged, and each link's label over the cards.
    /// Returns the link whose label was clicked, and whether a link was right-clicked.
    fn wires(
        &mut self,
        edges: &mut EdgesCtx,
        ui: &mut Ui,
        painter: &egui::Painter,
    ) -> (Option<LinkId>, bool) {
        let stroke = Stroke::new(crisp(LINK_WIDTH), colors::LINKED);
        let hovered = Stroke::new(crisp(LINK_WIDTH * LINK_HOVER), colors::LINKED);
        let shape = EdgeShape::Bezier {
            curvature: 1.0,
            max_handle: crisp(LINK_BEND),
        };
        let clicked_at = ui.input(|i| {
            i.pointer
                .primary_clicked()
                .then(|| i.pointer.interact_pos())
                .flatten()
        });
        let clicked_at = clicked_at.map(|pos| self.to_global.inverse() * pos);
        let mut clicked = None;
        let mut removed = false;
        let (view, plain, selection) = (self.view, self.plain, self.selection);
        for (wire, labels) in view.wires().iter().zip(&view.faces.wires) {
            let mut middle = None;
            let response = Edge::new(
                (wire.from.card, wire.from.index),
                (wire.to.card, wire.to.index),
            )
            .shape(shape)
            .stroke(stroke)
            .hovered_stroke(hovered)
            .show_with(edges, ui, |ui, edge| {
                middle = edge.points.get(edge.points.len() / 2).copied();
                ui.painter()
                    .add(egui::Shape::line(edge.points.to_vec(), edge.stroke));
            });
            // A right click on a link removes it.
            if response.secondary_clicked() {
                removed = true;
                self.events
                    .extend(wire.links.iter().copied().map(CanvasEvent::Disconnect));
            }
            // A wire out of view has no label either.
            let Some(middle) = middle else {
                continue;
            };
            // Plain, a label can't be read: only the selected link keeps its own, which is what
            // shows the selection.
            let shown = |link: LinkId| !plain || selection == Some(&Selection::Link(link));
            if !wire.links.iter().any(|&link| shown(link)) {
                continue;
            }
            let read = self.read;
            let labels =
                labels.get_or_init(|| (wire.links.iter()).map(|&link| read.label(link)).collect());
            for (&link, label) in wire.links.iter().zip(labels) {
                if let Some(label) = label
                    && shown(link)
                    && self.label(ui, painter, (link, label), middle, clicked_at)
                {
                    clicked = Some(link);
                }
            }
        }
        if let Some(edge) = edges.in_progress(ui) {
            edge.show_styled(ui, Some(stroke), shape);
        }
        (clicked, removed)
    }

    /// A link's label at its middle ([`LinkLabel`]). Returns whether the label was clicked.
    fn label(
        &self,
        ui: &Ui,
        painter: &egui::Painter,
        (id, label): (LinkId, &LinkLabel),
        middle: Pos2,
        clicked_at: Option<Pos2>,
    ) -> bool {
        let visuals = ui.visuals();
        let color = if label.over {
            colors::WARN
        } else {
            visuals.text_color()
        };
        let font = FontId::proportional(crisp(theme::text::SMALL));
        let galley = painter.layout_no_wrap(label.text.clone(), font, color);
        let pad = crisp(3.0);
        let rect = Rect::from_center_size(middle, galley.size()).expand(pad);
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
        clicked_at.is_some_and(|pos| rect.contains(pos))
    }
}

/// The tint of a card's header: one per kind of node.
fn header_tint(member: Member, kind: Option<&NodeKind>) -> Color32 {
    use theme::color::node;
    if let Member::Workspace(_) = member {
        return node::WORKSPACE;
    }
    match kind {
        Some(NodeKind::Recipe { .. }) => node::RECIPE,
        Some(NodeKind::Source { .. }) => node::SOURCE,
        Some(NodeKind::Sink { .. }) => node::SINK,
        Some(NodeKind::OutpostOrder { .. }) => node::OUTPOST_ORDER,
        Some(NodeKind::MissingRecipe { .. }) => node::MISSING,
        _ => node::LOGISTICS,
    }
}

fn big_number(ui: &mut Ui, text: &str) {
    // A blank value still takes its line, so the card doesn't jump when it fills in.
    ui.label(
        RichText::new(if text.is_empty() { " " } else { text })
            .text_style(theme::number_style())
            .color(theme::color::TEXT_STRONG),
    );
}

/// A card's title, cut short with an ellipsis to fit the card; the whole title shows on hover.
fn title_label(ui: &mut Ui, text: RichText) {
    ui.add(egui::Label::new(text).truncate());
}
