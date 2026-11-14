//! M4 performance spike (throwaway). An egui-snarl canvas with 300 nodes, 600 links and 5 ports per
//! node, each port and link showing a rate label. It drives itself through scripted phases with vsync
//! off, prints frame times per phase, and exits.
//!
//! Run a release build on the machine to measure:
//!
//! ```sh
//! cargo run --release -p aic-app-egui --example perf_spike
//! ```
//!
//! Pass: frames stay under about 8 ms (PLAN.md, M4). Keep the window in front: macOS stops drawing
//! a hidden window, and the run then never finishes.
//!
//! Switches, all off by default:
//!
//! - `SPIKE_SCALE=1.0`: start zoomed in, with only part of the graph on screen. The default fits all
//!   300 nodes on screen, the worst case.
//! - `SPIKE_CULL=1`: skip the widgets of off-screen nodes, allocating their last size instead.
//! - `SPIKE_NO_LABELS=1`: no link labels.
//! - `SPIKE_NO_LINKS=1`: no links at all.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

use eframe::egui::{self, Color32, FontId, Id, LayerId, Pos2, Rect, Ui, emath::TSTransform, vec2};
use egui_snarl::ui::{
    BackgroundPattern, PinInfo, PinPlacement, SnarlPin, SnarlStyle, SnarlViewer, SnarlWidget,
};
use egui_snarl::{InPin, InPinId, NodeId, OutPin, OutPinId, Snarl};

const NODES: usize = 300;
const INPUTS: usize = 2;
const OUTPUTS: usize = 3;
const COLUMNS: usize = 20;
const WARMUP: usize = 30;
const FRAMES: usize = 240;
const CANVAS: &str = "spike-canvas";

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

struct SpikeNode {
    rates: [f64; INPUTS + OUTPUTS],
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum PinKey {
    In(InPinId),
    Out(OutPinId),
}

type PinCenters = Rc<RefCell<HashMap<PinKey, Pos2>>>;

/// A pin that records where snarl drew it, so link labels can sit at the wire's midpoint.
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
    ) -> egui_snarl::ui::PinWireInfo {
        self.centers.borrow_mut().insert(self.key, rect.center());
        self.info.draw(snarl_style, style, rect, painter)
    }
}

struct Viewer<'a> {
    centers: &'a PinCenters,
    to_global: &'a mut TSTransform,
    script_transform: Option<TSTransform>,
    cull: Option<&'a mut Cull>,
}

/// Skips the widgets of nodes that were off screen last frame, allocating their last size instead.
struct Cull {
    viewport: Rect,
    node_rects: HashMap<NodeId, Rect>,
    sizes: HashMap<(NodeId, u8, usize), egui::Vec2>,
}

impl Viewer<'_> {
    fn content(&mut self, node: NodeId, slot: (u8, usize), ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
        let Some(cull) = self.cull.as_deref_mut() else {
            add(ui);
            return;
        };
        let key = (node, slot.0, slot.1);
        let off_screen = cull
            .node_rects
            .get(&node)
            .is_some_and(|rect| !rect.intersects(cull.viewport));
        if off_screen && let Some(&size) = cull.sizes.get(&key) {
            ui.allocate_exact_size(size, egui::Sense::hover());
            return;
        }
        let start = ui.cursor().min;
        add(ui);
        cull.sizes.insert(key, ui.min_rect().max - start);
    }
}

fn rate_label(ui: &mut Ui, rate: f64) {
    ui.label(format!("{rate:.2} /min"));
}

impl SnarlViewer<SpikeNode> for Viewer<'_> {
    fn title(&mut self, _node: &SpikeNode) -> String {
        String::new()
    }

    fn show_header(
        &mut self,
        node: NodeId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        ui: &mut Ui,
        _snarl: &mut Snarl<SpikeNode>,
    ) {
        self.content(node, (0, 0), ui, |ui| {
            ui.label(egui::RichText::new(format!("#{}", node.0)).strong());
        });
    }

    fn inputs(&mut self, _node: &SpikeNode) -> usize {
        INPUTS
    }

    fn outputs(&mut self, _node: &SpikeNode) -> usize {
        OUTPUTS
    }

    #[allow(refining_impl_trait)]
    fn show_input(
        &mut self,
        pin: &InPin,
        ui: &mut Ui,
        snarl: &mut Snarl<SpikeNode>,
    ) -> RecordingPin {
        let rate = snarl[pin.id.node].rates[pin.id.input];
        self.content(pin.id.node, (1, pin.id.input), ui, |ui| {
            rate_label(ui, rate)
        });
        RecordingPin {
            info: PinInfo::circle().with_fill(Color32::from_rgb(230, 120, 170)),
            key: PinKey::In(pin.id),
            centers: self.centers.clone(),
        }
    }

    #[allow(refining_impl_trait)]
    fn show_output(
        &mut self,
        pin: &OutPin,
        ui: &mut Ui,
        snarl: &mut Snarl<SpikeNode>,
    ) -> RecordingPin {
        let rate = snarl[pin.id.node].rates[INPUTS + pin.id.output];
        self.content(pin.id.node, (2, pin.id.output), ui, |ui| {
            rate_label(ui, rate)
        });
        RecordingPin {
            info: PinInfo::circle().with_fill(Color32::from_rgb(110, 200, 120)),
            key: PinKey::Out(pin.id),
            centers: self.centers.clone(),
        }
    }

    fn has_body(&mut self, _node: &SpikeNode) -> bool {
        true
    }

    fn show_body(
        &mut self,
        node: NodeId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        ui: &mut Ui,
        snarl: &mut Snarl<SpikeNode>,
    ) {
        let rates = snarl[node].rates;
        self.content(node, (3, 0), ui, |ui| {
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(format!("{:.2}", rates[0] / 10.0))
                        .size(18.0)
                        .strong(),
                );
                ui.label(format!("{:.2}", rates[1]));
            });
        });
    }

    fn final_node_rect(
        &mut self,
        node: NodeId,
        rect: Rect,
        _ui: &mut Ui,
        _snarl: &mut Snarl<SpikeNode>,
    ) {
        if let Some(cull) = self.cull.as_deref_mut() {
            cull.node_rects.insert(node, rect);
        }
    }

    fn draw_background(
        &mut self,
        background: Option<&BackgroundPattern>,
        viewport: &Rect,
        snarl_style: &SnarlStyle,
        style: &egui::Style,
        painter: &egui::Painter,
        _snarl: &Snarl<SpikeNode>,
    ) {
        if let Some(cull) = self.cull.as_deref_mut() {
            cull.viewport = *viewport;
        }
        if let Some(background) = background {
            background.draw(viewport, snarl_style, style, painter);
        }
    }

    fn current_transform(&mut self, to_global: &mut TSTransform, _snarl: &mut Snarl<SpikeNode>) {
        if let Some(scripted) = self.script_transform {
            *to_global = scripted;
        }
        *self.to_global = *to_global;
    }
}

/// Frame interval, CPU and UI times of one phase, in ms.
type PhaseResult = (Phase, Vec<f64>, Vec<f64>, Vec<f64>);

struct Spike {
    snarl: Snarl<SpikeNode>,
    style: SnarlStyle,
    centers: PinCenters,
    to_global: TSTransform,
    start_transform: Option<TSTransform>,
    phase: usize,
    frame_in_phase: usize,
    last_frame: Option<Instant>,
    frame_ms: Vec<f64>,
    cpu_ms: Vec<f64>,
    ui_ms: Vec<f64>,
    results: Vec<PhaseResult>,
    adapter: String,
    wire_drag: Option<Pos2>,
    cull: Option<Cull>,
}

impl Spike {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let adapter = cc
            .wgpu_render_state
            .as_ref()
            .map(|rs| {
                let info = rs.adapter.get_info();
                format!("{} ({:?})", info.name, info.backend)
            })
            .unwrap_or_default();
        let mut snarl = Snarl::new();
        let mut ids = Vec::with_capacity(NODES);
        for i in 0..NODES {
            let (col, row) = (i % COLUMNS, i / COLUMNS);
            let rates = std::array::from_fn(|k| 1.0 + ((i * 7 + k * 13) % 97) as f64 * 0.37);
            ids.push(snarl.insert_node(
                Pos2::new(col as f32 * 260.0, row as f32 * 200.0),
                SpikeNode { rates },
            ));
        }
        // Every input gets one link: 600 links, some short, some across the canvas.
        let links = if std::env::var("SPIKE_NO_LINKS").is_ok() {
            0
        } else {
            NODES
        };
        for n in 0..links {
            for input in 0..INPUTS {
                let from = (n + NODES - 1 - input * (COLUMNS + 1)) % NODES;
                snarl.connect(
                    OutPinId {
                        node: ids[from],
                        output: (n + input) % OUTPUTS,
                    },
                    InPinId {
                        node: ids[n],
                        input,
                    },
                );
            }
        }
        let style = SnarlStyle {
            pin_placement: Some(PinPlacement::Edge),
            pin_size: Some(8.0),
            collapsible: Some(false),
            bg_pattern: Some(BackgroundPattern::grid(vec2(50.0, 50.0), 0.0)),
            ..SnarlStyle::new()
        };
        Self {
            snarl,
            style,
            centers: PinCenters::default(),
            to_global: TSTransform::IDENTITY,
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
            cull: std::env::var("SPIKE_CULL").is_ok().then(|| Cull {
                viewport: Rect::EVERYTHING,
                node_rects: HashMap::new(),
                sizes: HashMap::new(),
            }),
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

    /// Paints `flow · belts` at the midpoint of every link, on top of the snarl layer.
    fn paint_link_labels(&self, ctx: &egui::Context) {
        let layer = LayerId::new(egui::Order::Background, Id::new(CANVAS));
        let painter = ctx.layer_painter(layer);
        let centers = self.centers.borrow();
        let text_color = ctx.global_style().visuals.text_color();
        for (out_pin, in_pin) in self.snarl.wires() {
            let (Some(a), Some(b)) = (
                centers.get(&PinKey::Out(out_pin)),
                centers.get(&PinKey::In(in_pin)),
            ) else {
                continue;
            };
            let flow = self.snarl[out_pin.node].rates[INPUTS + out_pin.output];
            let mid = a.lerp(*b, 0.5);
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
            "perf spike: {NODES} nodes, {} links, {} ports per node",
            self.snarl.wires().count(),
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
            if let Some(pos) = self.wire_drag.take() {
                raw_input.events.push(egui::Event::PointerButton {
                    pos,
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
        // Press on an output pin, then move the pointer around while the new wire follows it.
        let pin = OutPinId {
            node: NodeId(COLUMNS * 3 + 5),
            output: 0,
        };
        let Some(&center) = self.centers.borrow().get(&PinKey::Out(pin)) else {
            return;
        };
        let start = self.to_global * center;
        let t = (self.frame_in_phase - WARMUP) as f32;
        match self.wire_drag {
            None => {
                raw_input.events.push(egui::Event::PointerMoved(start));
                raw_input.events.push(egui::Event::PointerButton {
                    pos: start,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                });
                self.wire_drag = Some(start);
            }
            Some(origin) => {
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
            if let Some(node) = self.snarl.get_node_info_mut(NodeId(COLUMNS * 4 + 6)) {
                node.pos += vec2((t * 0.1).cos() * 6.0, (t * 0.1).sin() * 6.0);
            }
        }
        if phase == Phase::LiveValues {
            let t = self.frame_in_phase as f64;
            for (i, node) in self.snarl.nodes_mut().enumerate() {
                for (k, rate) in node.rates.iter_mut().enumerate() {
                    *rate = 10.0 + ((t + i as f64 + k as f64) * 0.37).sin() * 9.0;
                }
            }
        }

        let script_transform = self.scripted_transform();
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                let mut viewer = Viewer {
                    centers: &self.centers,
                    to_global: &mut self.to_global,
                    script_transform,
                    cull: self.cull.as_mut(),
                };
                SnarlWidget::new()
                    .id(Id::new(CANVAS))
                    .style(self.style)
                    .show(&mut self.snarl, &mut viewer, ui);
            });
        if self.start_transform.is_none() && self.frame_in_phase > 5 {
            // SPIKE_SCALE=1.0 starts zoomed in, with only part of the graph on screen.
            let scale = std::env::var("SPIKE_SCALE")
                .ok()
                .and_then(|s| s.parse().ok());
            self.start_transform = Some(match scale {
                Some(scaling) => TSTransform::new(vec2(0.0, 0.0), scaling),
                None => self.to_global,
            });
        }
        if std::env::var("SPIKE_NO_LABELS").is_err() {
            self.paint_link_labels(ui.ctx());
        }
        if self.frame_in_phase >= WARMUP {
            self.ui_ms.push(now.elapsed().as_secs_f64() * 1000.0);
        }

        self.frame_in_phase += 1;
        if self.frame_in_phase >= WARMUP + FRAMES {
            self.finish_phase(phase);
            if self.current().is_none() {
                self.report();
            }
        }
        ui.ctx().request_repaint();
    }
}

fn main() -> eframe::Result {
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1600.0, 900.0]),
        ..Default::default()
    };
    options.wgpu_options.surface.present_mode = eframe::wgpu::PresentMode::AutoNoVsync;
    eframe::run_native(
        "perf spike",
        options,
        Box::new(|cc| Ok(Box::new(Spike::new(cc)))),
    )
}
