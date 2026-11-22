//! The conventions this crate changed from egui_graph's (`SOURCE.md`), tested the way the app
//! drives the graph: one pass at a time with `RawInput` events, the selection kept outside the
//! graph and handed back in every pass.

use crate::node::{EdgeEvent, Node, NodeCtx, NodeId};
use crate::socket::{SocketHighlight, SocketKind};
use crate::{Graph, View};
use egui::emath::TSTransform;
use egui::epaint::ClippedShape;
use egui::{pos2, vec2, Modifiers, PointerButton, Pos2, Rect, Vec2};
use std::collections::{HashMap, HashSet};

const SCREEN: Rect = Rect::from_min_max(Pos2::ZERO, pos2(800.0, 600.0));
const GRAPH: &str = "graph";
/// The time between passes, in seconds.
const DT: f32 = 1.0 / 60.0;
/// The size of each node's content.
const CONTENT: Vec2 = vec2(120.0, 80.0);
/// The size of a socket's handle, against the edge of the content.
const HANDLE: Vec2 = vec2(16.0, 16.0);
/// The colour the sockets are painted in, to find them among the shapes.
const SOCKET_COLOR: egui::Color32 = egui::Color32::from_rgb(1, 2, 3);
/// A point on the background, clear of every node of [`three_nodes`].
const BACKGROUND: Pos2 = pos2(600.0, 450.0);

/// Has one output.
const A: NodeId = NodeId(0);
/// Has one input, to the right of `A`.
const B: NodeId = NodeId(1);
/// Has one output, below `A`.
const C: NodeId = NodeId(2);

/// How a node of the test graph shows.
#[derive(Clone, Copy)]
struct TestNode {
    id: NodeId,
    pos: Pos2,
    inputs: usize,
    outputs: usize,
    /// Whether each socket has a handle inside the content.
    handles: bool,
    /// Whether the node shows with [`Node::show_or_cull`].
    cull: bool,
    paint_sockets: bool,
}

impl TestNode {
    fn new(id: NodeId, pos: Pos2, inputs: usize, outputs: usize) -> Self {
        Self {
            id,
            pos,
            inputs,
            outputs,
            handles: true,
            cull: false,
            paint_sockets: true,
        }
    }
}

/// `A`, `B` and `C`, with handles.
fn three_nodes() -> Vec<TestNode> {
    vec![
        TestNode::new(A, pos2(100.0, 100.0), 0, 1),
        TestNode::new(B, pos2(400.0, 100.0), 1, 0),
        TestNode::new(C, pos2(100.0, 350.0), 0, 1),
    ]
}

/// A graph driven one pass at a time, with its selection kept here as an app keeps it.
struct Harness {
    ctx: egui::Context,
    view: View,
    time: f64,
    nodes: Vec<TestNode>,
    selection: HashSet<NodeId>,
    /// The modifiers held during each pass.
    modifiers: Modifiers,
    /// A window floating above the graph at this position.
    window: Option<Pos2>,
}

/// What a node showed in a pass, in screen space.
struct NodeSeen {
    rect: Rect,
    edge_event: Option<EdgeEvent>,
    selected: bool,
    /// What the content saw, or `None` if it did not run.
    highlight: Option<SocketHighlight>,
    outputs: Vec<(Pos2, Vec2)>,
    input_handles: Vec<Option<Rect>>,
    output_handles: Vec<Option<Rect>>,
}

/// What the graph showed in a pass.
struct Frame {
    nodes: HashMap<NodeId, NodeSeen>,
    selection_changed: Option<HashSet<NodeId>>,
    nodes_moved: bool,
    secondary_click: Option<Pos2>,
    scene_rect: Rect,
    layout: crate::Layout,
    edge_in_progress: bool,
    to_screen: TSTransform,
    shapes: Vec<ClippedShape>,
}

impl Frame {
    fn node(&self, id: NodeId) -> &NodeSeen {
        &self.nodes[&id]
    }

    /// The centre of the handle of output 0 of `id`.
    fn output_handle(&self, id: NodeId) -> Pos2 {
        self.node(id).output_handles[0]
            .expect("the output has a handle")
            .center()
    }

    /// The centre of the handle of input 0 of `id`.
    fn input_handle(&self, id: NodeId) -> Pos2 {
        self.node(id).input_handles[0]
            .expect("the input has a handle")
            .center()
    }

    /// A point on the frame of `id`, inside it, away from its handles.
    fn frame_point(&self, id: NodeId) -> Pos2 {
        self.node(id).rect.center()
    }

    /// The edge events of all nodes, by node.
    fn edge_events(&self) -> Vec<(NodeId, EdgeEvent)> {
        let mut events: Vec<_> = self
            .nodes
            .iter()
            .filter_map(|(&id, n)| n.edge_event.map(|e| (id, e)))
            .collect();
        events.sort_by_key(|(id, _)| *id);
        events
    }
}

/// Lay out a new graph of `nodes`, over two passes, since sizes settle after the first.
fn harness(nodes: Vec<TestNode>) -> (Harness, Frame) {
    let view = View {
        scene_rect: SCREEN,
        layout: nodes.iter().map(|n| (n.id, n.pos)).collect(),
    };
    let mut h = Harness {
        ctx: egui::Context::default(),
        view,
        time: 0.0,
        nodes,
        selection: HashSet::new(),
        modifiers: Modifiers::NONE,
        window: None,
    };
    h.pass(vec![]);
    let f = h.pass(vec![]);
    (h, f)
}

fn button(button: PointerButton, pos: Pos2, pressed: bool, modifiers: Modifiers) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers,
    }
}

fn delete() -> egui::Event {
    egui::Event::Key {
        key: egui::Key::Delete,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

fn backspace() -> egui::Event {
    egui::Event::Key {
        key: egui::Key::Backspace,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

impl Harness {
    /// Run one pass with the given input events.
    fn pass(&mut self, events: Vec<egui::Event>) -> Frame {
        self.time += f64::from(DT);
        let mut all = vec![egui::Event::ModifiersChanged(self.modifiers)];
        all.extend(events);
        let input = egui::RawInput {
            screen_rect: Some(SCREEN),
            time: Some(self.time),
            predicted_dt: DT,
            events: all,
            ..Default::default()
        };
        let nodes = self.nodes.clone();
        let selection = self.selection.clone();
        let window = self.window;
        let mut out = None;
        let mut response = None;
        let mut output = self.ctx.run_ui(input, |ui| {
            let graph = Graph::new(GRAPH)
                .snap(None)
                .selected_nodes(selection.clone());
            let resp = graph.show(&mut self.view, ui, |ui, show| {
                let mut seen = HashMap::new();
                let mut in_progress = false;
                show.nodes(ui, |nctx, ui| {
                    for n in &nodes {
                        let node = Node::from_id(n.id)
                            .inputs(n.inputs)
                            .outputs(n.outputs)
                            .socket_color(SOCKET_COLOR)
                            .paint_sockets(n.paint_sockets)
                            .animation_time(0.0);
                        let mut highlight = None;
                        let content = |c: NodeCtx| {
                            highlight = Some(c.highlighted_sockets());
                            c.framed(|ui, sockets| {
                                let (rect, _) =
                                    ui.allocate_exact_size(CONTENT, egui::Sense::hover());
                                for i in 0..n.inputs {
                                    let at = rect.left_top() + vec2(0.0, 8.0 + 30.0 * i as f32);
                                    let handle = Rect::from_min_size(at, HANDLE);
                                    sockets.input(i, handle);
                                    if n.handles {
                                        sockets.input_handle(i, handle);
                                    }
                                }
                                for i in 0..n.outputs {
                                    let at =
                                        rect.right_top() + vec2(-HANDLE.x, 8.0 + 30.0 * i as f32);
                                    let handle = Rect::from_min_size(at, HANDLE);
                                    sockets.output(i, handle);
                                    if n.handles {
                                        sockets.output_handle(i, handle);
                                    }
                                }
                            })
                        };
                        let (rect, edge_event, selected) = if n.cull {
                            let r = node.show_or_cull(nctx, ui, content);
                            (r.rect, r.edge_event(), r.selected())
                        } else {
                            let r = node.show(nctx, ui, content);
                            (r.rect, r.edge_event(), r.selected())
                        };
                        seen.insert(n.id, (rect, edge_event, selected, highlight));
                    }
                })
                .edges(ui, |ectx, ui| in_progress = ectx.in_progress(ui).is_some());
                let to_screen = ui
                    .ctx()
                    .layer_transform_to_global(ui.layer_id())
                    .unwrap_or_default();
                let nodes = crate::with_graph_memory(ui.ctx(), crate::id(GRAPH), |gmem| {
                    seen.into_iter()
                        .map(|(id, (rect, edge_event, selected, highlight))| {
                            let sockets = &gmem.node_sockets()[&id];
                            let n = nodes.iter().find(|n| n.id == id).expect("a test node");
                            let screen = |(p, normal)| (to_screen.mul_pos(p), normal);
                            let handle = |r: Option<Rect>| r.map(|r| to_screen * r);
                            let seen = NodeSeen {
                                rect: to_screen * rect,
                                edge_event,
                                selected,
                                highlight,
                                outputs: (0..n.outputs)
                                    .filter_map(|i| sockets.output(i).map(screen))
                                    .collect(),
                                input_handles: (0..n.inputs)
                                    .map(|i| handle(sockets.input_handle(i)))
                                    .collect(),
                                output_handles: (0..n.outputs)
                                    .map(|i| handle(sockets.output_handle(i)))
                                    .collect(),
                            };
                            (id, seen)
                        })
                        .collect()
                });
                out = Some((nodes, in_progress, to_screen));
            });
            response = Some((
                resp.selection_changed,
                resp.nodes_moved,
                resp.background_secondary_click,
            ));
            if let Some(pos) = window {
                egui::Window::new("window")
                    .fixed_pos(pos)
                    .fixed_size(vec2(200.0, 150.0))
                    .show(ui.ctx(), |ui| {
                        ui.allocate_space(ui.available_size());
                    });
            }
        });
        output.textures_delta.clear();
        let (nodes, edge_in_progress, to_screen) = out.expect("the graph ran");
        let (selection_changed, nodes_moved, secondary_click) = response.expect("the graph ran");
        if let Some(selection) = &selection_changed {
            self.selection = selection.clone();
        }
        Frame {
            nodes,
            selection_changed,
            nodes_moved,
            secondary_click,
            scene_rect: self.view.scene_rect,
            layout: self.view.layout.clone(),
            edge_in_progress,
            to_screen,
            shapes: output.shapes,
        }
    }

    /// Move the pointer to `pos`, then run a second pass, since pointer coverage of node frames
    /// takes effect on the next pass.
    fn hover(&mut self, pos: Pos2) -> Frame {
        self.pass(vec![egui::Event::PointerMoved(pos)]);
        self.pass(vec![])
    }

    fn press(&mut self, b: PointerButton, pos: Pos2) -> Frame {
        self.pass(vec![button(b, pos, true, self.modifiers)])
    }

    fn release(&mut self, b: PointerButton, pos: Pos2) -> Frame {
        self.pass(vec![button(b, pos, false, self.modifiers)])
    }

    /// Hover `from`, press `b` there, move to `to` in two steps, and return the pass with the
    /// pointer at `to` and the button still down.
    fn drag_to(&mut self, b: PointerButton, from: Pos2, to: Pos2) -> Frame {
        self.hover(from);
        self.press(b, from);
        self.pass(vec![egui::Event::PointerMoved(from.lerp(to, 0.5))]);
        self.pass(vec![egui::Event::PointerMoved(to)]);
        self.pass(vec![])
    }

    /// A whole drag of `b` from `from` to `to`, returning the pass of the release.
    fn drag(&mut self, b: PointerButton, from: Pos2, to: Pos2) -> Frame {
        self.drag_to(b, from, to);
        self.release(b, to)
    }

    /// A press and release of `b` at `pos`, returning the pass of the release.
    fn click(&mut self, b: PointerButton, pos: Pos2) -> Frame {
        self.hover(pos);
        self.press(b, pos);
        self.release(b, pos)
    }
}

fn set(ids: &[NodeId]) -> HashSet<NodeId> {
    ids.iter().copied().collect()
}

const OUT_0: (SocketKind, usize) = (SocketKind::Output, 0);
const IN_0: (SocketKind, usize) = (SocketKind::Input, 0);

/// A link starts from the handle of a socket inside the card (`SocketLayout::*_handle`).
mod handles {
    use super::*;

    /// A press on a handle starts an edge from its socket. The node does not move as the pointer
    /// does, and the selection stays as it was.
    #[test]
    fn press_on_a_handle_starts_an_edge() {
        let (mut h, f) = harness(three_nodes());
        h.selection = set(&[B]);
        let start = f.output_handle(A);
        h.hover(start);
        let pressed = h.press(PointerButton::Primary, start);
        assert_eq!(
            pressed.node(A).edge_event,
            Some(EdgeEvent::Started {
                kind: SocketKind::Output,
                index: 0,
            })
        );
        h.pass(vec![egui::Event::PointerMoved(start + vec2(60.0, 40.0))]);
        let moving = h.pass(vec![]);
        assert!(moving.edge_in_progress);
        assert_eq!(moving.layout[&A], f.layout[&A]);
        let released = h.release(PointerButton::Primary, start + vec2(60.0, 40.0));
        assert_eq!(released.layout[&A], f.layout[&A]);
        assert!(!released.nodes_moved);
        assert_eq!(h.selection, set(&[B]));
    }

    /// A press on the frame away from a handle drags the node.
    #[test]
    fn press_elsewhere_on_the_frame_drags_the_node() {
        let (mut h, f) = harness(three_nodes());
        let from = f.frame_point(A);
        let moving = h.drag_to(PointerButton::Primary, from, from + vec2(60.0, 40.0));
        assert!(!moving.edge_in_progress);
        assert_eq!(moving.layout[&A], f.layout[&A] + vec2(60.0, 40.0));
        assert!(
            h.release(PointerButton::Primary, from + vec2(60.0, 40.0))
                .nodes_moved
        );
    }

    /// An edge released on a handle of the opposite kind on another node ends on its socket.
    #[test]
    fn edge_released_on_an_opposite_handle_ends_there() {
        let (mut h, f) = harness(three_nodes());
        let end = f.input_handle(B);
        let released = h.drag(PointerButton::Primary, f.output_handle(A), end);
        assert_eq!(
            released.edge_events(),
            vec![(
                B,
                EdgeEvent::Ended {
                    kind: SocketKind::Input,
                    index: 0,
                }
            )]
        );
    }

    /// An edge released on a handle of the same kind is cancelled, and ends nowhere.
    #[test]
    fn edge_released_on_a_same_kind_handle_is_cancelled() {
        let (mut h, f) = harness(three_nodes());
        let released = h.drag(
            PointerButton::Primary,
            f.output_handle(A),
            f.output_handle(C),
        );
        assert_eq!(released.edge_events(), vec![(A, EdgeEvent::Cancelled)]);
    }

    /// An edge released on the background, or on a frame away from its handles, is cancelled.
    #[test]
    fn edge_released_off_any_handle_is_cancelled() {
        let (mut h, f) = harness(three_nodes());
        let released = h.drag(PointerButton::Primary, f.output_handle(A), BACKGROUND);
        assert_eq!(released.edge_events(), vec![(A, EdgeEvent::Cancelled)]);
        let released = h.drag(PointerButton::Primary, f.output_handle(A), f.frame_point(B));
        assert_eq!(released.edge_events(), vec![(A, EdgeEvent::Cancelled)]);
    }

    /// A socket with a handle is pressed only through its handle: a press on the background just
    /// outside the frame, on the socket, starts no edge. A socket with no handle still does.
    #[test]
    fn only_a_socket_without_a_handle_is_pressed_from_outside() {
        let mut nodes = three_nodes();
        nodes[2].handles = false;
        let (mut h, f) = harness(nodes);
        let outside = |id| {
            let (pos, normal) = f.node(id).outputs[0];
            pos + normal * 2.0
        };
        h.hover(outside(A));
        let pressed = h.press(PointerButton::Primary, outside(A));
        assert_eq!(pressed.node(A).edge_event, None);
        h.release(PointerButton::Primary, outside(A));
        h.hover(outside(C));
        let pressed = h.press(PointerButton::Primary, outside(C));
        assert_eq!(
            pressed.node(C).edge_event,
            Some(EdgeEvent::Started {
                kind: SocketKind::Output,
                index: 0,
            })
        );
    }

    /// The number of shapes painted in the socket colour.
    fn socket_shapes(f: &Frame) -> usize {
        f.shapes
            .iter()
            .filter(|s| matches!(&s.shape, egui::Shape::Path(p) if p.fill == SOCKET_COLOR))
            .count()
    }

    /// A node with its socket discs turned off paints no socket.
    #[test]
    fn a_node_can_paint_no_sockets() {
        let (_, f) = harness(three_nodes());
        assert_eq!(socket_shapes(&f), 3);
        let mut nodes = three_nodes();
        for n in &mut nodes {
            n.paint_sockets = false;
        }
        let (_, f) = harness(nodes);
        assert_eq!(socket_shapes(&f), 0);
    }

    /// The content sees the pressed socket, and the socket an edge in progress would end on.
    #[test]
    fn the_content_sees_the_highlighted_sockets() {
        let (mut h, f) = harness(three_nodes());
        let none = Some(SocketHighlight::default());
        assert_eq!(f.node(A).highlight, none);
        let moving = h.drag_to(PointerButton::Primary, f.output_handle(A), BACKGROUND);
        let a = moving.node(A).highlight.expect("A ran");
        assert_eq!(a.start, Some(OUT_0));
        assert!(a.contains(SocketKind::Output, 0));
        assert_eq!(moving.node(B).highlight, none);
        h.pass(vec![egui::Event::PointerMoved(f.input_handle(B))]);
        let over = h.pass(vec![]);
        let b = over.node(B).highlight.expect("B ran");
        assert_eq!(b.end, Some(IN_0));
        assert!(b.contains(SocketKind::Input, 0));
        // Over a handle of the same kind, there is no end.
        h.pass(vec![egui::Event::PointerMoved(f.output_handle(C))]);
        let over = h.pass(vec![]);
        assert_eq!(over.node(C).highlight, none);
    }

    /// The handle of a node out of view moves with the node, as its sockets do.
    #[test]
    fn a_culled_nodes_handle_moves_with_it() {
        let mut nodes = three_nodes();
        nodes[0].cull = true;
        let (mut h, f) = harness(nodes);
        let in_view = f.node(A).output_handles[0].expect("A's output has a handle");
        // Scroll A out of view, then move it.
        h.view.scene_rect = SCREEN.translate(vec2(0.0, 1000.0));
        h.pass(vec![]);
        *h.view.layout.get_mut(&A).expect("A is laid out") += vec2(0.0, 50.0);
        h.pass(vec![]);
        let f = h.pass(vec![]);
        assert_eq!(f.node(A).highlight, None, "A is culled");
        let moved = f.node(A).output_handles[0].expect("A's output keeps its handle");
        let back = f.to_screen.inverse() * moved;
        assert_eq!(back, in_view.translate(vec2(0.0, 50.0)));
    }

    /// A handle in the frame's margin, beside a label as in `perf_spike`, starts an edge.
    #[test]
    fn a_handle_beside_a_label_starts_an_edge() {
        let ctx = egui::Context::default();
        let mut view = View {
            scene_rect: SCREEN,
            layout: [(A, pos2(100.0, 100.0))].into(),
        };
        let mut time = 0.0;
        let mut pass = |events: Vec<egui::Event>| {
            time += f64::from(DT);
            let input = egui::RawInput {
                screen_rect: Some(SCREEN),
                time: Some(time),
                events,
                ..Default::default()
            };
            let mut seen = None;
            ctx.run_ui(input, |ui| {
                Graph::new(GRAPH).show(&mut view, ui, |ui, show| {
                    show.nodes(ui, |nctx, ui| {
                        let node = Node::from_id(A).outputs(1).animation_time(0.0);
                        let mut handle = None;
                        let r = node.show(nctx, ui, |c| {
                            c.framed(|ui, sockets| {
                                let row =
                                    sockets.row(ui, None, Some(0), |ui| ui.label("1.25 /min"));
                                let rect = row.response.rect;
                                let h = Rect::from_min_max(
                                    rect.right_top(),
                                    rect.right_bottom() + vec2(6.0, 0.0),
                                );
                                sockets.output_handle(0, h);
                                handle = Some(h);
                            })
                        });
                        seen = Some((handle.expect("A ran"), r.edge_event()));
                    });
                });
            })
            .textures_delta
            .clear();
            seen.expect("the graph ran")
        };
        pass(vec![]);
        let (handle, _) = pass(vec![]);
        let at = handle.center();
        pass(vec![egui::Event::PointerMoved(at)]);
        pass(vec![]);
        let (_, event) = pass(vec![button(
            PointerButton::Primary,
            at,
            true,
            Modifiers::NONE,
        )]);
        assert!(
            matches!(event, Some(EdgeEvent::Started { .. })),
            "{event:?}"
        );
    }

    /// Back at a scale of 1 with no pan, which egui stores as no transform, after the view was
    /// zoomed, a handle and a frame are still pressed where they are drawn.
    #[test]
    fn handles_and_frames_work_back_at_the_identity_view() {
        let (mut h, f) = harness(three_nodes());
        h.view.scene_rect = Rect::from_min_size(Pos2::ZERO, SCREEN.size() * 2.0);
        h.pass(vec![]);
        h.pass(vec![]);
        h.view.scene_rect = SCREEN;
        h.pass(vec![]);
        h.hover(f.output_handle(A));
        let pressed = h.press(PointerButton::Primary, f.output_handle(A));
        assert_eq!(
            pressed.node(A).edge_event,
            Some(EdgeEvent::Started {
                kind: SocketKind::Output,
                index: 0,
            })
        );
        h.release(PointerButton::Primary, f.output_handle(A));
        let from = f.frame_point(B);
        let moving = h.drag_to(PointerButton::Primary, from, from + vec2(20.0, 0.0));
        assert_eq!(moving.layout[&B], f.layout[&B] + vec2(20.0, 0.0));
    }

    /// A press on a window floating above a handle starts no edge.
    #[test]
    fn a_window_above_a_handle_takes_the_press() {
        let (mut h, f) = harness(three_nodes());
        let handle = f.output_handle(A);
        h.window = Some(handle - vec2(100.0, 60.0));
        h.pass(vec![]);
        h.hover(handle);
        let pressed = h.press(PointerButton::Primary, handle);
        assert_eq!(pressed.node(A).edge_event, None);
        let moving = h.pass(vec![egui::Event::PointerMoved(handle + vec2(40.0, 40.0))]);
        assert!(!moving.edge_in_progress);
        assert_eq!(moving.layout[&A], f.layout[&A]);
    }
}

/// Ctrl on Windows and Linux, which egui also reports as Cmd.
const CTRL_CMD: Modifiers = Modifiers {
    ctrl: true,
    command: true,
    ..Modifiers::NONE
};

/// A box from above and left of `A` to below and right of `B`, clear of `C`.
const BOX_A_B: (Pos2, Pos2) = (pos2(50.0, 50.0), pos2(560.0, 250.0));
/// A box over `A` alone.
const BOX_A: (Pos2, Pos2) = (pos2(50.0, 50.0), pos2(300.0, 250.0));

/// The mouse: a primary drag pans, clicks select, a box selects, a secondary click is reported.
mod mouse {
    use super::*;

    /// A primary drag on the background pans the view by the drag and selects nothing.
    #[test]
    fn a_primary_drag_on_the_background_pans_and_selects_nothing() {
        let (mut h, f) = harness(three_nodes());
        h.selection = set(&[A]);
        let delta = vec2(50.0, 30.0);
        let released = h.drag(PointerButton::Primary, BACKGROUND, BACKGROUND + delta);
        assert_eq!(released.scene_rect, SCREEN.translate(-delta));
        assert_eq!(released.layout, f.layout);
        assert_eq!(h.selection, set(&[A]));
    }

    /// A middle drag on the background pans too.
    #[test]
    fn a_middle_drag_on_the_background_pans() {
        let (mut h, _) = harness(three_nodes());
        let to = BACKGROUND + vec2(50.0, 30.0);
        let released = h.drag(PointerButton::Middle, BACKGROUND, to);
        assert_ne!(released.scene_rect, SCREEN);
    }

    /// A click makes a node the only one selected. Shift+click adds a node or takes it out.
    /// Cmd/Ctrl+click is a plain click. A click on the background clears the selection.
    #[test]
    fn clicks_select() {
        let (mut h, f) = harness(three_nodes());
        h.selection = set(&[A, C]);
        let clicked = h.click(PointerButton::Primary, f.frame_point(B));
        assert_eq!(clicked.selection_changed, Some(set(&[B])));
        assert!(clicked.node(B).selected);
        h.modifiers = Modifiers::SHIFT;
        h.click(PointerButton::Primary, f.frame_point(A));
        assert_eq!(h.selection, set(&[A, B]));
        h.click(PointerButton::Primary, f.frame_point(B));
        assert_eq!(h.selection, set(&[A]));
        h.modifiers = CTRL_CMD;
        h.click(PointerButton::Primary, f.frame_point(C));
        assert_eq!(h.selection, set(&[C]));
        h.modifiers = Modifiers::NONE;
        let background = h.click(PointerButton::Primary, BACKGROUND);
        assert_eq!(background.selection_changed, Some(set(&[])));
        assert_eq!(background.scene_rect, SCREEN);
    }

    /// A secondary drag on the background draws a box: on release, what it touches becomes the
    /// selection; with Shift held at release it is added; with Cmd/Ctrl, taken out. It does not
    /// pan.
    #[test]
    fn a_secondary_drag_box_replaces_adds_and_removes() {
        let (mut h, f) = harness(three_nodes());
        h.selection = set(&[C]);
        let released = h.drag(PointerButton::Secondary, BOX_A_B.0, BOX_A_B.1);
        assert_eq!(h.selection, set(&[A, B]));
        assert_eq!(released.scene_rect, SCREEN);
        assert_eq!(released.layout, f.layout);

        h.selection = set(&[C]);
        h.drag_to(PointerButton::Secondary, BOX_A.0, BOX_A.1);
        h.modifiers = Modifiers::SHIFT;
        h.release(PointerButton::Secondary, BOX_A.1);
        assert_eq!(h.selection, set(&[A, C]));

        h.modifiers = Modifiers::NONE;
        h.selection = set(&[A, B, C]);
        h.drag_to(PointerButton::Secondary, BOX_A.0, BOX_A.1);
        h.modifiers = CTRL_CMD;
        h.release(PointerButton::Secondary, BOX_A.1);
        assert_eq!(h.selection, set(&[B, C]));
    }

    /// A primary drag on the background with Shift held draws a box that adds to the selection,
    /// and does not pan.
    #[test]
    fn a_shift_primary_drag_box_adds_and_does_not_pan() {
        let (mut h, _) = harness(three_nodes());
        h.selection = set(&[C]);
        h.modifiers = Modifiers::SHIFT;
        let moving = h.drag_to(PointerButton::Primary, BOX_A_B.0, BOX_A_B.1);
        assert_eq!(moving.scene_rect, SCREEN);
        let released = h.release(PointerButton::Primary, BOX_A_B.1);
        assert_eq!(h.selection, set(&[A, B, C]));
        assert_eq!(released.scene_rect, SCREEN);
    }

    /// A secondary click on the background, without a drag, is reported with its position in
    /// graph space. A box drag and a secondary click on a node are not.
    #[test]
    fn a_secondary_click_reports_its_graph_position() {
        let (mut h, _) = harness(three_nodes());
        let offset = vec2(50.0, 20.0);
        h.view.scene_rect = SCREEN.translate(offset);
        let f = h.pass(vec![]);
        let clicked = h.click(PointerButton::Secondary, BACKGROUND);
        assert_eq!(clicked.secondary_click, Some(BACKGROUND + offset));
        assert_eq!(h.selection, set(&[]));
        // A move within the click distance is still a click.
        let nudge = vec2(3.0, 0.0);
        let nudged = h.drag(PointerButton::Secondary, BACKGROUND, BACKGROUND + nudge);
        assert_eq!(nudged.secondary_click, Some(BACKGROUND + nudge + offset));
        // A box drag is not.
        let (from, to) = (BOX_A.0 - offset, BOX_A.1 - offset);
        h.drag_to(PointerButton::Secondary, from, to);
        let boxed = h.release(PointerButton::Secondary, to);
        assert_eq!(boxed.secondary_click, None);
        assert_eq!(h.selection, set(&[A]));
        assert_eq!(h.pass(vec![]).secondary_click, None);
        // Nor is a click on a node.
        let on_node = h.click(PointerButton::Secondary, f.frame_point(B));
        assert_eq!(on_node.secondary_click, None);
    }

    /// Dragging a selected node moves the whole selection.
    #[test]
    fn dragging_a_selected_node_moves_the_selection() {
        let (mut h, f) = harness(three_nodes());
        h.selection = set(&[A, B]);
        let delta = vec2(30.0, 20.0);
        let from = f.frame_point(A);
        let released = h.drag(PointerButton::Primary, from, from + delta);
        assert!(released.nodes_moved);
        assert_eq!(released.layout[&A], f.layout[&A] + delta);
        assert_eq!(released.layout[&B], f.layout[&B] + delta);
        assert_eq!(released.layout[&C], f.layout[&C]);
        assert_eq!(h.selection, set(&[A, B]));
    }

    /// Dragging a node that is not selected moves that node alone and leaves the selection.
    #[test]
    fn dragging_an_unselected_node_moves_it_alone() {
        let (mut h, f) = harness(three_nodes());
        h.selection = set(&[B, C]);
        let delta = vec2(30.0, 20.0);
        let from = f.frame_point(A);
        let released = h.drag(PointerButton::Primary, from, from + delta);
        assert!(released.nodes_moved);
        assert_eq!(released.layout[&A], f.layout[&A] + delta);
        assert_eq!(released.layout[&B], f.layout[&B]);
        assert_eq!(released.layout[&C], f.layout[&C]);
        assert_eq!(h.selection, set(&[B, C]));
    }
}

/// Scrolling is the app's: the widget neither pans nor zooms on the wheel, a trackpad or a pinch.
mod scrolling {
    use super::*;

    fn wheel(unit: egui::MouseWheelUnit, delta: Vec2, modifiers: Modifiers) -> egui::Event {
        egui::Event::MouseWheel {
            unit,
            delta,
            phase: egui::TouchPhase::Move,
            modifiers,
        }
    }

    fn touch(id: u64, phase: egui::TouchPhase, pos: Pos2) -> egui::Event {
        egui::Event::Touch {
            device_id: egui::TouchDeviceId(0),
            id: egui::TouchId(id),
            phase,
            pos,
            force: None,
        }
    }

    #[test]
    fn wheel_zoom_and_pinch_leave_the_view_alone() {
        use egui::MouseWheelUnit::{Line, Point};
        let (mut h, _) = harness(three_nodes());
        h.hover(BACKGROUND);
        let none = Modifiers::NONE;
        let shift = Modifiers::SHIFT;
        let gestures = [
            (none, vec![wheel(Line, vec2(0.0, -3.0), none)]),
            (none, vec![wheel(Point, vec2(20.0, -40.0), none)]),
            (shift, vec![wheel(Line, vec2(0.0, 3.0), shift)]),
            (CTRL_CMD, vec![wheel(Line, vec2(0.0, -3.0), CTRL_CMD)]),
            (none, vec![egui::Event::Zoom(0.5)]),
            (none, vec![egui::Event::Zoom(1.5)]),
        ];
        for (modifiers, events) in gestures {
            h.modifiers = modifiers;
            let f = h.pass(events.clone());
            assert_eq!(f.scene_rect, SCREEN, "{events:?}");
            for _ in 0..10 {
                assert_eq!(h.pass(vec![]).scene_rect, SCREEN, "{events:?}");
            }
        }
        // Two fingers moving apart.
        h.modifiers = Modifiers::NONE;
        let (a, b) = (BACKGROUND - vec2(20.0, 0.0), BACKGROUND + vec2(20.0, 0.0));
        use egui::TouchPhase::{End, Move, Start};
        h.pass(vec![touch(0, Start, a), touch(1, Start, b)]);
        for step in 1..=5 {
            let d = vec2(10.0 * step as f32, 0.0);
            let f = h.pass(vec![touch(0, Move, a - d), touch(1, Move, b + d)]);
            assert_eq!(f.scene_rect, SCREEN);
        }
        let d = vec2(50.0, 0.0);
        h.pass(vec![touch(0, End, a - d), touch(1, End, b + d)]);
        assert_eq!(h.pass(vec![]).scene_rect, SCREEN);
    }

    /// A scene rect of the graph area's aspect ratio is shown exactly as given.
    #[test]
    fn a_scene_rect_is_shown_as_given() {
        let (mut h, _) = harness(three_nodes());
        let given = Rect::from_min_size(pos2(-100.0, -50.0), vec2(1600.0, 1200.0));
        h.view.scene_rect = given;
        let f = h.pass(vec![]);
        assert_eq!(f.scene_rect, given);
        assert_eq!(f.to_screen.scaling, 0.5);
        let shown = f.to_screen * given;
        let close = |a: Pos2, b: Pos2| (a - b).length() < 1e-3;
        assert!(
            close(shown.min, SCREEN.min) && close(shown.max, SCREEN.max),
            "{shown:?}"
        );
    }
}

/// The selection is the app's: it comes in each pass, changes go out, and keys do nothing.
mod selection {
    use super::*;

    #[test]
    fn delete_and_backspace_change_nothing() {
        let (mut h, f) = harness(three_nodes());
        h.selection = set(&[A]);
        h.hover(f.frame_point(A));
        for key in [delete(), backspace()] {
            let pressed = h.pass(vec![key]);
            assert_eq!(pressed.selection_changed, None);
            assert!(pressed.node(A).selected);
            assert_eq!(pressed.layout, f.layout);
        }
        assert_eq!(h.selection, set(&[A]));
    }

    /// A selection the app changes from outside is the one the graph adds to and drags.
    #[test]
    fn the_selection_given_from_outside_is_used() {
        let (mut h, f) = harness(three_nodes());
        h.click(PointerButton::Primary, f.frame_point(A));
        assert_eq!(h.selection, set(&[A]));
        h.selection = set(&[B, C]);
        h.modifiers = Modifiers::SHIFT;
        h.click(PointerButton::Primary, f.frame_point(A));
        assert_eq!(h.selection, set(&[A, B, C]));
        h.modifiers = Modifiers::NONE;
        h.selection = set(&[B, C]);
        let delta = vec2(0.0, 30.0);
        let from = f.frame_point(B);
        let released = h.drag(PointerButton::Primary, from, from + delta);
        assert_eq!(released.layout[&A], f.layout[&A]);
        assert_eq!(released.layout[&B], f.layout[&B] + delta);
        assert_eq!(released.layout[&C], f.layout[&C] + delta);
    }
}

/// What a canvas that draws its own cards leans on: a clickable picture on a socket's handle, and
/// nodes it places itself while showing them.
mod cards {
    use super::*;

    /// One node, `A`, whose output has a handle with a click-only widget on it, as a card puts an
    /// icon on a port. Driven one pass at a time.
    struct Card {
        ctx: egui::Context,
        view: View,
        time: f64,
        /// Where `A` is put while it shows, in place of its layout position.
        place: Option<Pos2>,
    }

    /// What a pass of [`Card`] showed.
    struct Shown {
        rect: Rect,
        handle: Rect,
        edge_event: Option<EdgeEvent>,
        clicked: bool,
        secondary_clicked: bool,
        double_clicked: bool,
        edge_in_progress: bool,
        layout: crate::Layout,
    }

    impl Card {
        fn new() -> (Self, Shown) {
            let mut card = Self {
                ctx: egui::Context::default(),
                view: View {
                    scene_rect: SCREEN,
                    layout: [(A, pos2(100.0, 100.0))].into(),
                },
                time: 0.0,
                place: None,
            };
            card.pass(vec![]);
            let shown = card.pass(vec![]);
            (card, shown)
        }

        fn pass(&mut self, events: Vec<egui::Event>) -> Shown {
            self.time += f64::from(DT);
            let input = egui::RawInput {
                screen_rect: Some(SCREEN),
                time: Some(self.time),
                predicted_dt: DT,
                events,
                ..Default::default()
            };
            let place = self.place;
            let mut shown = None;
            let mut output = self.ctx.run_ui(input, |ui| {
                Graph::new(GRAPH)
                    .snap(None)
                    .show(&mut self.view, ui, |ui, show| {
                        let mut node_shown = None;
                        let mut in_progress = false;
                        show.nodes(ui, |nctx, ui| {
                            if let Some(pos) = place {
                                nctx.set_position(A, pos);
                            }
                            let node = Node::from_id(A).outputs(1).animation_time(0.0);
                            let mut widget = None;
                            let r = node.show(nctx, ui, |c| {
                                c.framed(|ui, sockets| {
                                    let (rect, _) =
                                        ui.allocate_exact_size(CONTENT, egui::Sense::hover());
                                    let handle = Rect::from_min_size(
                                        rect.right_top() + vec2(-HANDLE.x, 8.0),
                                        HANDLE,
                                    );
                                    sockets.output(0, handle);
                                    sockets.output_handle(0, handle);
                                    let id = ui.id().with("icon");
                                    let icon = ui.interact(handle, id, egui::Sense::click());
                                    widget = Some((handle, icon));
                                })
                            });
                            let (handle, icon) = widget.expect("A ran");
                            node_shown = Some((r.rect, handle, r.edge_event(), icon));
                        })
                        .edges(ui, |ectx, ui| in_progress = ectx.in_progress(ui).is_some());
                        let (rect, handle, edge_event, icon) = node_shown.expect("the nodes ran");
                        shown = Some((rect, handle, edge_event, icon, in_progress));
                    });
            });
            output.textures_delta.clear();
            let (rect, handle, edge_event, icon, edge_in_progress) = shown.expect("the graph ran");
            Shown {
                rect,
                handle,
                edge_event,
                clicked: icon.clicked(),
                secondary_clicked: icon.secondary_clicked(),
                double_clicked: icon.double_clicked(),
                edge_in_progress,
                layout: self.view.layout.clone(),
            }
        }

        fn hover(&mut self, pos: Pos2) {
            self.pass(vec![egui::Event::PointerMoved(pos)]);
            self.pass(vec![]);
        }
    }

    /// A widget on a handle that senses clicks only leaves the press to the handle: a drag from
    /// it starts an edge, and does not move the node.
    #[test]
    fn a_drag_from_a_click_widget_on_a_handle_starts_an_edge() {
        let (mut card, first) = Card::new();
        let at = first.handle.center();
        card.hover(at);
        let pressed = card.pass(vec![button(
            PointerButton::Primary,
            at,
            true,
            Modifiers::NONE,
        )]);
        assert_eq!(
            pressed.edge_event,
            Some(EdgeEvent::Started {
                kind: SocketKind::Output,
                index: 0,
            })
        );
        card.pass(vec![egui::Event::PointerMoved(at + vec2(80.0, 30.0))]);
        let moving = card.pass(vec![]);
        assert!(moving.edge_in_progress);
        assert_eq!(moving.layout[&A], first.layout[&A]);
        assert!(!moving.clicked, "a drag is not a click on the widget");
    }

    /// The widget on the handle gets the clicks: a primary click, a secondary click and a
    /// double click. None of them leaves an edge behind or moves the node.
    #[test]
    fn a_click_widget_on_a_handle_gets_its_clicks() {
        let (mut card, first) = Card::new();
        let at = first.handle.center();
        card.hover(at);
        card.pass(vec![button(
            PointerButton::Primary,
            at,
            true,
            Modifiers::NONE,
        )]);
        let released = card.pass(vec![button(
            PointerButton::Primary,
            at,
            false,
            Modifiers::NONE,
        )]);
        assert!(released.clicked);
        assert_eq!(released.edge_event, Some(EdgeEvent::Cancelled));
        assert!(!card.pass(vec![]).edge_in_progress);

        // Straight after: the second click of a double click.
        card.pass(vec![button(
            PointerButton::Primary,
            at,
            true,
            Modifiers::NONE,
        )]);
        let again = card.pass(vec![button(
            PointerButton::Primary,
            at,
            false,
            Modifiers::NONE,
        )]);
        assert!(again.double_clicked);

        card.pass(vec![button(
            PointerButton::Secondary,
            at,
            true,
            Modifiers::NONE,
        )]);
        let secondary = card.pass(vec![button(
            PointerButton::Secondary,
            at,
            false,
            Modifiers::NONE,
        )]);
        assert!(secondary.secondary_clicked);
        assert_eq!(secondary.edge_event, None);
        assert_eq!(secondary.layout[&A], first.layout[&A]);
    }

    /// A position set while the nodes show is where the node shows in that same pass, so a caller
    /// can pin a node to the view it only knows once the graph has panned.
    #[test]
    fn a_position_set_while_showing_places_the_node_at_once() {
        let (mut card, first) = Card::new();
        let to = pos2(300.0, 220.0);
        card.place = Some(to);
        let placed = card.pass(vec![]);
        assert_eq!(placed.rect.min, to);
        assert_eq!(placed.layout[&A], to);
        assert_eq!(placed.rect.size(), first.rect.size());
        // Its handle follows in the same pass.
        assert_eq!(
            placed.handle.min - placed.rect.min,
            first.handle.min - first.rect.min
        );
    }
}

/// A tap: a press and its release in one pass, as a touchpad tap or a quick click arrives. It
/// does what a slower click does.
mod taps {
    use super::*;

    impl Harness {
        /// A press and release of `b` at `pos` within one pass, then a pass with no input.
        fn tap(&mut self, b: PointerButton, pos: Pos2) -> Frame {
            self.hover(pos);
            let m = self.modifiers;
            let tapped = self.pass(vec![button(b, pos, true, m), button(b, pos, false, m)]);
            let after = self.pass(vec![]);
            // What the tap did shows in one of the two passes.
            Frame {
                selection_changed: tapped.selection_changed.or(after.selection_changed.clone()),
                secondary_click: tapped.secondary_click.or(after.secondary_click),
                ..after
            }
        }
    }

    #[test]
    fn a_tap_on_a_node_selects_it() {
        let (mut h, f) = harness(three_nodes());
        h.tap(PointerButton::Primary, f.frame_point(A));
        assert_eq!(h.selection, set(&[A]));
        h.modifiers = Modifiers::SHIFT;
        h.tap(PointerButton::Primary, f.frame_point(B));
        assert_eq!(h.selection, set(&[A, B]));
    }

    #[test]
    fn a_tap_on_the_background_clears_the_selection() {
        let (mut h, f) = harness(three_nodes());
        h.selection = set(&[A, B]);
        let tapped = h.tap(PointerButton::Primary, BACKGROUND);
        assert_eq!(h.selection, set(&[]));
        assert_eq!(tapped.scene_rect, f.scene_rect, "a tap pans nothing");
        assert_eq!(tapped.layout, f.layout);
    }

    #[test]
    fn a_secondary_tap_on_the_background_is_reported() {
        let (mut h, f) = harness(three_nodes());
        let tapped = h.tap(PointerButton::Secondary, BACKGROUND);
        let at = tapped.secondary_click.expect("the tap is reported");
        assert!((f.to_screen * at - BACKGROUND).length() < 1e-3, "{at:?}");
        assert_eq!(tapped.selection_changed, None);
    }

    #[test]
    fn a_tap_on_a_handle_leaves_no_edge_behind() {
        let (mut h, f) = harness(three_nodes());
        let tapped = h.tap(PointerButton::Primary, f.output_handle(A));
        assert!(!tapped.edge_in_progress);
        assert_eq!(tapped.layout, f.layout);
        // The next press starts afresh: a drag from the handle makes an edge.
        let end = f.input_handle(B);
        let released = h.drag(PointerButton::Primary, f.output_handle(A), end);
        assert!(released.edge_events().contains(&(
            B,
            EdgeEvent::Ended {
                kind: SocketKind::Input,
                index: 0
            }
        )));
    }
}
