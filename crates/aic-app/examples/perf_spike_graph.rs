//! Performance spike (throwaway): the `perf_spike` workload drawn with `aic_node_graph` (our copy
//! of egui_graph) instead of egui-snarl. 300 nodes, 600 links and 5 ports per node, each port and
//! link showing a rate label, in the same grid, views and scripted phases, so the two spikes' frame
//! times compare.
//!
//! ```sh
//! cargo run --release -p aic-app --example perf_spike_graph
//! ```
//!
//! Switches, all off by default, as in `perf_spike`:
//!
//! - `SPIKE_SCALE=1.0`: start zoomed in, with only part of the graph on screen.
//! - `SPIKE_CULL=1`: show nodes with `Node::show_or_cull`, which skips the content of nodes out of
//!   view. Without it, nodes show with `Node::show`.
//! - `SPIKE_NO_LABELS=1`: no link labels.
//! - `SPIKE_NO_LINKS=1`: no links at all.
//! - `SPIKE_EDGE_DPP=5`: flatten edges every 5 screen pixels. By default aic-node-graph flattens them
//!   every 5 graph units at any zoom (`Edge::DEFAULT_DISTANCE_PER_POINT`).
//! - `SPIKE_HEADLESS=1`: open no window; build and tessellate the UI for the same phases and print
//!   the CPU time per frame.

// Links `aic_app`, whose global allocator (mimalloc) the app runs on.
use aic_app as _;

use std::time::Instant;

use aic_node_graph::NodeId;
use aic_node_graph::edge::{Edge, EdgeShape};
use aic_node_graph::node::Node;
use eframe::egui::{self, Color32, FontId, Id, Pos2, Rect, Ui, emath::TSTransform, vec2};

const NODES: usize = 300;
const INPUTS: usize = 2;
const OUTPUTS: usize = 3;
const COLUMNS: usize = 20;
const WARMUP: usize = 30;
const FRAMES: usize = 240;
const CANVAS: &str = "spike-canvas";
/// egui-snarl's default zoom range, which the snarl spike uses.
const MIN_SCALE: f32 = 0.2;
const MAX_SCALE: f32 = 2.0;
/// The size of a socket: the snarl spike's pin size of 8.
const SOCKET_RADIUS: f32 = 4.0;
/// The content height that makes a node as tall as the snarl spike's (182 with the frame).
const CARD_HEIGHT: f32 = 168.0;
/// egui-snarl draws a node's frame around its position, out by the frame's margin (window margin 6
/// plus stroke 1); aic-node-graph draws it from its position. Placing nodes this much up and left
/// puts the frames where snarl puts them.
const SNARL_FRAME_MARGIN: f32 = 7.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Idle,
    Pan,
    Zoom,
    DragNode,
    NewWire,
    LiveValues,
}

const PHASES: [Phase; 6] = [
    Phase::Idle,
    Phase::Pan,
    Phase::Zoom,
    Phase::DragNode,
    Phase::NewWire,
    Phase::LiveValues,
];

/// A link from an output of one node to an input of another, by node index.
struct Link {
    from: usize,
    output: usize,
    to: usize,
    input: usize,
    selected: bool,
}

/// The scripted new wire: where it started, and whether the button is down yet.
#[derive(Clone, Copy)]
struct WireDrag {
    origin: Pos2,
    pressed: bool,
}

/// Frame interval, CPU and UI times of one phase, in ms.
type PhaseResult = (Phase, Vec<f64>, Vec<f64>, Vec<f64>);

struct Spike {
    view: aic_node_graph::View,
    rates: Vec<[f64; INPUTS + OUTPUTS]>,
    links: Vec<Link>,
    /// The scene's transform from graph to screen, as egui-snarl's `to_global`.
    to_global: Option<TSTransform>,
    start_transform: Option<TSTransform>,
    phase: usize,
    frame_in_phase: usize,
    last_frame: Option<Instant>,
    frame_ms: Vec<f64>,
    cpu_ms: Vec<f64>,
    ui_ms: Vec<f64>,
    results: Vec<PhaseResult>,
    adapter: String,
    wire_drag: Option<WireDrag>,
    /// The screen position and normal of the scripted wire's output, from the last frame.
    wire_pin: Option<(Pos2, egui::Vec2)>,
    /// `SPIKE_EDGE_DPP`: the distance between the points of a flattened edge, in screen pixels.
    edge_screen_distance: Option<f32>,
    cull: bool,
    headless: bool,
    /// Headless counters: node contents run, and frames with an edge in progress.
    contents_shown: usize,
    edge_in_progress: bool,
}

fn graph_id() -> Id {
    aic_node_graph::id(CANVAS)
}

fn rate_label(ui: &mut Ui, rate: f64) {
    ui.label(format!("{rate:.2} /min"));
}

/// The transform egui-snarl starts from: the nodes' positions, with a margin of 100, fit in the
/// canvas within its frame (`SnarlState::initial`).
fn snarl_initial_transform(ui_rect: Rect) -> TSTransform {
    let mut bb = Rect::NOTHING;
    for i in 0..NODES {
        bb.extend_with(node_pos(i));
    }
    let bb = bb.expand(100.0);
    let scaling = (ui_rect.size() / bb.size())
        .min_elem()
        .clamp(MIN_SCALE, MAX_SCALE);
    TSTransform::new(
        ui_rect.center().to_vec2() - bb.center().to_vec2() * scaling,
        scaling,
    )
}

fn node_pos(i: usize) -> Pos2 {
    let (col, row) = (i % COLUMNS, i / COLUMNS);
    Pos2::new(col as f32 * 260.0, row as f32 * 200.0)
}

/// The grid of the snarl spike's background (`BackgroundPattern::grid`, 50 by 50, no angle).
fn paint_grid(ui: &Ui, viewport: Rect) {
    let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
    let painter = ui.painter();
    let spacing = vec2(50.0, 50.0);
    let min_x = (viewport.min.x / spacing.x).ceil();
    let max_x = (viewport.max.x / spacing.x).floor();
    for x in 0..=f32::ceil(max_x - min_x) as i64 {
        let x = (x as f32 + min_x) * spacing.x;
        painter.line_segment(
            [Pos2::new(x, viewport.min.y), Pos2::new(x, viewport.max.y)],
            stroke,
        );
    }
    let min_y = (viewport.min.y / spacing.y).ceil();
    let max_y = (viewport.max.y / spacing.y).floor();
    for y in 0..=f32::ceil(max_y - min_y) as i64 {
        let y = (y as f32 + min_y) * spacing.y;
        painter.line_segment(
            [Pos2::new(viewport.min.x, y), Pos2::new(viewport.max.x, y)],
            stroke,
        );
    }
}

impl Spike {
    fn new(adapter: String) -> Self {
        let rates = (0..NODES)
            .map(|i| std::array::from_fn(|k| 1.0 + ((i * 7 + k * 13) % 97) as f64 * 0.37))
            .collect();
        let mut view = aic_node_graph::View::default();
        for i in 0..NODES {
            view.layout.insert(
                NodeId::from_u64(i as u64),
                node_pos(i) - vec2(SNARL_FRAME_MARGIN, SNARL_FRAME_MARGIN),
            );
        }
        // Every input gets one link: 600 links, some short, some across the canvas.
        let links = if std::env::var("SPIKE_NO_LINKS").is_ok() {
            0
        } else {
            NODES
        };
        let mut all = Vec::new();
        for n in 0..links {
            for input in 0..INPUTS {
                all.push(Link {
                    from: (n + NODES - 1 - input * (COLUMNS + 1)) % NODES,
                    output: (n + input) % OUTPUTS,
                    to: n,
                    input,
                    selected: false,
                });
            }
        }
        Self {
            view,
            rates,
            links: all,
            to_global: None,
            start_transform: None,
            phase: 0,
            frame_in_phase: 0,
            last_frame: None,
            frame_ms: Vec::new(),
            cpu_ms: Vec::new(),
            ui_ms: Vec::new(),
            results: Vec::new(),
            adapter,
            wire_drag: None,
            wire_pin: None,
            cull: std::env::var("SPIKE_CULL").is_ok(),
            edge_screen_distance: std::env::var("SPIKE_EDGE_DPP")
                .ok()
                .and_then(|v| v.parse().ok()),
            headless: false,
            contents_shown: 0,
            edge_in_progress: false,
        }
    }

    fn current(&self) -> Option<Phase> {
        PHASES.get(self.phase).copied()
    }

    fn scripted_transform(&self) -> Option<TSTransform> {
        let start = self.start_transform?;
        let t = self.frame_in_phase as f32;
        match self.current()? {
            Phase::Pan => Some(TSTransform::new(
                start.translation + vec2((t * 0.05).sin() * 400.0, t * 2.0),
                start.scaling,
            )),
            Phase::Zoom => {
                let scaling = 0.6 + 0.5 * (t * 0.05).sin();
                Some(TSTransform::new(start.translation, scaling.max(0.2)))
            }
            _ => std::env::var("SPIKE_SCALE").is_ok().then_some(start),
        }
    }

    fn show_nodes(&mut self, nctx: &mut aic_node_graph::NodesCtx, ui: &mut Ui) {
        let cull = self.cull;
        let mut shown = 0;
        for (i, rates) in self.rates.iter().enumerate() {
            let node = Node::from_id(NodeId::from_u64(i as u64))
                .inputs(INPUTS)
                .outputs(OUTPUTS)
                .socket_radius(SOCKET_RADIUS)
                .animation_time(0.0);
            let content = |node_ctx: aic_node_graph::NodeCtx<'_>| {
                shown += 1;
                node_ctx.framed(|ui, sockets| {
                    // The height snarl gives the same card, so the same nodes are in view.
                    ui.set_min_height(CARD_HEIGHT);
                    // The header, as the snarl spike's `show_header`.
                    ui.label(egui::RichText::new(format!("#{i}")).strong());
                    // Inputs, body and outputs side by side, as snarl's default node layout.
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            for (k, &rate) in rates[..INPUTS].iter().enumerate() {
                                sockets.row(ui, Some(k), None, |ui| rate_label(ui, rate));
                            }
                        });
                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new(format!("{:.2}", rates[0] / 10.0))
                                    .size(18.0)
                                    .strong(),
                            );
                            ui.label(format!("{:.2}", rates[1]));
                        });
                        ui.vertical(|ui| {
                            for (k, &rate) in rates[INPUTS..].iter().enumerate() {
                                sockets.row(ui, None, Some(k), |ui| rate_label(ui, rate));
                            }
                        });
                    });
                })
            };
            if cull {
                node.show_or_cull(nctx, ui, content);
            } else {
                node.show(nctx, ui, content);
            }
        }
        self.contents_shown = shown;
    }

    fn show_edges(&mut self, ectx: &mut aic_node_graph::EdgesCtx, ui: &mut Ui) {
        // egui_graph flattens edges at a fixed distance in graph space; SPIKE_EDGE_DPP sets it in
        // screen pixels instead.
        let scaling = self.to_global.map_or(1.0, |t| t.scaling);
        let distance = self
            .edge_screen_distance
            .map_or(Edge::DEFAULT_DISTANCE_PER_POINT, |px| px / scaling);
        for link in &mut self.links {
            let a = (NodeId::from_u64(link.from as u64), link.output);
            let b = (NodeId::from_u64(link.to as u64), link.input);
            Edge::new(a, b, &mut link.selected)
                .distance_per_point(distance)
                .show(ectx, ui);
        }
        self.edge_in_progress = false;
        if let Some(edge) = ectx.in_progress(ui) {
            edge.show(ui, EdgeShape::DEFAULT);
            self.edge_in_progress = true;
        }
    }

    /// Paints `flow · belts` at the midpoint of every link, in graph space as the snarl spike does.
    fn paint_link_labels(&self, ui: &Ui) {
        let painter = ui.painter();
        let text_color = ui.ctx().global_style().visuals.text_color();
        let mids: Vec<(Pos2, f64)> =
            aic_node_graph::with_graph_memory(ui.ctx(), graph_id(), |gmem| {
                let sockets = gmem.node_sockets();
                self.links
                    .iter()
                    .filter_map(|link| {
                        let a = sockets
                            .get(&NodeId::from_u64(link.from as u64))?
                            .output(link.output)?;
                        let b = sockets
                            .get(&NodeId::from_u64(link.to as u64))?
                            .input(link.input)?;
                        let flow = self.rates[link.from][INPUTS + link.output];
                        Some((a.0.lerp(b.0, 0.5), flow))
                    })
                    .collect()
            });
        for (mid, flow) in mids {
            let galley = painter.layout_no_wrap(
                format!("{flow:.2} /min · {:.2} belts", flow / 30.0),
                FontId::proportional(12.0),
                text_color,
            );
            let rect = Rect::from_center_size(mid, galley.size()).expand(2.0);
            painter.rect_filled(rect, 3.0, Color32::from_black_alpha(160));
            painter.galley(rect.min + vec2(2.0, 2.0), galley, text_color);
        }
    }

    /// The scripted wire's output, in graph space, from the graph's memory.
    fn wire_pin_graph(&self, ctx: &egui::Context) -> Option<(Pos2, egui::Vec2)> {
        let node = NodeId::from_u64((COLUMNS * 3 + 5) as u64);
        aic_node_graph::with_graph_memory(ctx, graph_id(), |gmem| {
            gmem.node_sockets().get(&node)?.output(0)
        })
    }

    fn finish_phase(&mut self, phase: Phase) {
        let frames = std::mem::take(&mut self.frame_ms);
        let cpu = std::mem::take(&mut self.cpu_ms);
        let ui = std::mem::take(&mut self.ui_ms);
        self.results.push((phase, frames, cpu, ui));
        self.phase += 1;
        self.frame_in_phase = 0;
    }

    fn report(&self) {
        println!(
            "perf spike (egui_graph, cull {}): {NODES} nodes, {} links, {} ports per node",
            self.cull,
            self.links.len(),
            INPUTS + OUTPUTS
        );
        println!("adapter: {}", self.adapter);
        println!(
            "{:<12} {:>9} {:>9} {:>9} {:>9} {:>9}",
            "phase", "mean ms", "p95 ms", "max ms", "cpu ms", "ui ms"
        );
        for (phase, frames, cpu, ui) in &self.results {
            let mut sorted = frames.clone();
            sorted.sort_by(f64::total_cmp);
            let mean = sorted.iter().sum::<f64>() / sorted.len() as f64;
            let p95 = sorted[(sorted.len() * 95 / 100).min(sorted.len() - 1)];
            let max = sorted.last().copied().unwrap_or_default();
            let cpu_mean = cpu.iter().sum::<f64>() / cpu.len().max(1) as f64;
            let ui_mean = ui.iter().sum::<f64>() / ui.len().max(1) as f64;
            println!(
                "{:<12} {mean:>9.2} {p95:>9.2} {max:>9.2} {cpu_mean:>9.2} {ui_mean:>9.2}",
                format!("{phase:?}")
            );
        }
    }
}

impl eframe::App for Spike {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        if self.current() != Some(Phase::NewWire) {
            if let Some(drag) = self.wire_drag.take() {
                raw_input.events.push(egui::Event::PointerButton {
                    pos: drag.origin,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            return;
        }
        if self.frame_in_phase < WARMUP {
            return;
        }
        // Hover just off the frame on an output socket (a press on the frame drags the node), press
        // there on the next frame, then move the pointer around while the new wire follows it.
        let Some((center, normal)) = self.wire_pin else {
            return;
        };
        let start = center + normal * 2.0;
        let t = (self.frame_in_phase - WARMUP) as f32;
        match self.wire_drag {
            None => {
                raw_input.events.push(egui::Event::PointerMoved(start));
                self.wire_drag = Some(WireDrag {
                    origin: start,
                    pressed: false,
                });
            }
            Some(WireDrag {
                origin,
                pressed: false,
            }) => {
                raw_input.events.push(egui::Event::PointerButton {
                    pos: origin,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                });
                self.wire_drag = Some(WireDrag {
                    origin,
                    pressed: true,
                });
            }
            Some(WireDrag { origin, .. }) => {
                let pos = origin + vec2(20.0 + t * 2.0, (t * 0.1).sin() * 150.0);
                raw_input.events.push(egui::Event::PointerMoved(pos));
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let now = Instant::now();
        let Some(phase) = self.current() else {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        };
        if self.frame_in_phase >= WARMUP
            && let Some(last) = self.last_frame
        {
            self.frame_ms
                .push(now.duration_since(last).as_secs_f64() * 1000.0);
            if let Some(cpu) = frame.info().cpu_usage {
                self.cpu_ms.push(f64::from(cpu) * 1000.0);
            }
        }
        self.last_frame = Some(now);

        if phase == Phase::DragNode {
            let t = self.frame_in_phase as f32;
            let id = NodeId::from_u64((COLUMNS * 4 + 6) as u64);
            if let Some(pos) = self.view.layout.get_mut(&id) {
                *pos += vec2((t * 0.1).cos() * 6.0, (t * 0.1).sin() * 6.0);
            }
        }
        if phase == Phase::LiveValues {
            let t = self.frame_in_phase as f64;
            for (i, rates) in self.rates.iter_mut().enumerate() {
                for (k, rate) in rates.iter_mut().enumerate() {
                    *rate = 10.0 + ((t + i as f64 + k as f64) * 0.37).sin() * 9.0;
                }
            }
        }

        let script_transform = self.scripted_transform();
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                // Set the view so that the scene has the transform snarl would: its initial fit,
                // then the scripted one when there is one, else the last one.
                let graph_rect = ui.available_rect_before_wrap();
                let snarl_rect = graph_rect - egui::Frame::canvas(ui.style()).total_margin();
                let to_global = script_transform
                    .or(self.to_global)
                    .unwrap_or_else(|| snarl_initial_transform(snarl_rect));
                self.view.scene_rect = to_global.inverse() * graph_rect;
                let mut view = std::mem::take(&mut self.view);
                aic_node_graph::Graph::from_id(graph_id())
                    .zoom_range(MIN_SCALE..=MAX_SCALE)
                    .dot_grid(false)
                    .auto_pan(None)
                    .show(&mut view, ui, |ui, show| {
                        // egui keeps no transform for a layer at identity.
                        self.to_global = Some(
                            ui.ctx()
                                .layer_transform_to_global(ui.layer_id())
                                .unwrap_or(TSTransform::IDENTITY),
                        );
                        paint_grid(ui, ui.clip_rect());
                        show.nodes(ui, |nctx, ui| self.show_nodes(nctx, ui))
                            .edges(ui, |ectx, ui| self.show_edges(ectx, ui));
                        if std::env::var("SPIKE_NO_LABELS").is_err() {
                            self.paint_link_labels(ui);
                        }
                    });
                self.view = view;
            });
        self.wire_pin = self
            .wire_pin_graph(ui.ctx())
            .and_then(|(pos, normal)| self.to_global.map(|to_global| (to_global * pos, normal)));
        if self.start_transform.is_none() && self.frame_in_phase > 5 {
            // SPIKE_SCALE=1.0 starts zoomed in, with only part of the graph on screen.
            let scale = std::env::var("SPIKE_SCALE")
                .ok()
                .and_then(|s| s.parse().ok());
            self.start_transform = match scale {
                Some(scaling) => Some(TSTransform::new(vec2(0.0, 0.0), scaling)),
                None => self.to_global,
            };
        }
        if self.frame_in_phase >= WARMUP {
            self.ui_ms.push(now.elapsed().as_secs_f64() * 1000.0);
        }

        self.frame_in_phase += 1;
        if self.frame_in_phase >= WARMUP + FRAMES {
            self.finish_phase(phase);
            if self.current().is_none() && !self.headless {
                self.report();
            }
        }
        ui.ctx().request_repaint();
    }
}

/// The headless run: UI build plus tessellation per frame, per phase, with no window and the
/// same scripted input as the windowed run.
fn headless() {
    let ctx = egui::Context::default();
    let mut spike = Spike::new(String::new());
    spike.headless = true;
    let screen = Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 900.0));
    let mut time = 0.0;
    let mut times: Vec<(Phase, Vec<f64>)> = PHASES.iter().map(|&p| (p, Vec::new())).collect();
    // Per phase: tessellation times, and the shapes, clipped primitives and vertices of its last frame.
    let mut tess: Vec<Vec<f64>> = vec![Vec::new(); PHASES.len()];
    let mut counts = vec![(0, 0, 0); PHASES.len()];
    let mut views = Vec::new();
    let mut wire_frames = 0;
    let mut passes = 0;
    let mut shown = vec![0; PHASES.len()];
    while let Some(phase) = spike.current() {
        let frame_in_phase = spike.frame_in_phase;
        time += 1.0 / 60.0;
        let mut input = egui::RawInput {
            screen_rect: Some(screen),
            time: Some(time),
            ..Default::default()
        };
        eframe::App::raw_input_hook(&mut spike, &ctx, &mut input);
        let passes_before = ctx.cumulative_pass_nr();
        let started = Instant::now();
        let output = ctx.run_ui(input, |ui| {
            eframe::App::ui(&mut spike, ui, &mut eframe::Frame::_new_kittest());
        });
        let shapes = output.shapes.len();
        let tess_started = Instant::now();
        let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
        let tess_ms = tess_started.elapsed().as_secs_f64() * 1000.0;
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        let index = PHASES.iter().position(|&p| p == phase).expect("a phase");
        if frame_in_phase >= WARMUP {
            times[index].1.push(ms);
            tess[index].push(tess_ms);
            let vertices: usize = primitives
                .iter()
                .map(|p| match &p.primitive {
                    egui::epaint::Primitive::Mesh(mesh) => mesh.vertices.len(),
                    egui::epaint::Primitive::Callback(_) => 0,
                })
                .sum();
            counts[index] = (shapes, primitives.len(), vertices);
            passes += ctx.cumulative_pass_nr() - passes_before;
            shown[index] += spike.contents_shown;
            if phase == Phase::NewWire && spike.edge_in_progress {
                wire_frames += 1;
            }
        }
        // The last, untimed warm-up frame of each phase: where the nodes are.
        if frame_in_phase == WARMUP - 1 {
            let to_global = spike.to_global.unwrap_or_default();
            let viewport = to_global.inverse() * screen;
            let rects: Vec<Rect> = aic_node_graph::with_graph_memory(&ctx, graph_id(), |gmem| {
                spike
                    .view
                    .layout
                    .iter()
                    .filter_map(|(id, &pos)| {
                        Some(Rect::from_min_size(pos, *gmem.node_sizes().get(id)?))
                    })
                    .collect()
            });
            let in_view = rects.iter().filter(|r| r.intersects(viewport)).count();
            let size = rects
                .iter()
                .map(|r| r.size())
                .fold(egui::Vec2::ZERO, |a, b| a + b)
                / rects.len().max(1) as f32;
            views.push((phase, to_global, in_view, rects.len(), size));
        }
    }
    println!(
        "egui_graph headless: {NODES} nodes, {} links, cull {}",
        spike.links.len(),
        spike.cull
    );
    for (phase, to_global, in_view, drawn, size) in views {
        println!(
            "  {:<10} at its start: scaling {:.4}, translation [{:.1} {:.1}], nodes in view \
             {in_view}/{drawn}, mean node size [{:.1} {:.1}]",
            format!("{phase:?}"),
            to_global.scaling,
            to_global.translation.x,
            to_global.translation.y,
            size.x,
            size.y
        );
    }
    for (index, phase) in PHASES.iter().enumerate() {
        println!(
            "  {:<10} node contents run per timed frame: {:.1}",
            format!("{phase:?}"),
            shown[index] as f64 / FRAMES as f64
        );
    }
    println!("new wire: an edge was in progress in {wire_frames}/{FRAMES} timed frames");
    println!(
        "egui passes per timed frame: {:.3}",
        passes as f64 / (FRAMES * PHASES.len()) as f64
    );
    for (index, phase) in PHASES.iter().enumerate() {
        let (shapes, primitives, vertices) = counts[index];
        println!(
            "  {:<10} tessellation {:.3} ms of the frame; last frame: {shapes} shapes, \
             {primitives} primitives, {vertices} vertices",
            format!("{phase:?}"),
            tess[index].iter().sum::<f64>() / tess[index].len().max(1) as f64
        );
    }
    print_headless(&times);
}

/// Mean, median, p95 and max per phase, in ms.
fn print_headless(times: &[(Phase, Vec<f64>)]) {
    println!(
        "{:<12} {:>9} {:>9} {:>9} {:>9}",
        "phase", "mean ms", "median ms", "p95 ms", "max ms"
    );
    for (phase, times) in times {
        let mut sorted = times.clone();
        sorted.sort_by(f64::total_cmp);
        let mean = sorted.iter().sum::<f64>() / sorted.len().max(1) as f64;
        let median = sorted.get(sorted.len() / 2).copied().unwrap_or_default();
        let p95 = sorted
            .get((sorted.len() * 95 / 100).min(sorted.len().saturating_sub(1)))
            .copied()
            .unwrap_or_default();
        let max = sorted.last().copied().unwrap_or_default();
        println!(
            "{:<12} {mean:>9.3} {median:>9.3} {p95:>9.3} {max:>9.3}",
            format!("{phase:?}")
        );
    }
}

fn main() -> eframe::Result {
    if std::env::var("SPIKE_HEADLESS").is_ok_and(|v| v == "1") {
        headless();
        return Ok(());
    }
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1600.0, 900.0]),
        ..Default::default()
    };
    options.wgpu_options.surface.present_mode = eframe::wgpu::PresentMode::AutoNoVsync;
    eframe::run_native(
        "perf spike graph",
        options,
        Box::new(|cc| {
            let adapter = cc
                .wgpu_render_state
                .as_ref()
                .map(|rs| {
                    let info = rs.adapter.get_info();
                    format!("{} ({:?})", info.name, info.backend)
                })
                .unwrap_or_default();
            Ok(Box::new(Spike::new(adapter)))
        }),
    )
}
