//! M7 performance run: the real app, with real data, icons, the inspector and the totals, on a
//! generated graph of about 200 linked recipe nodes. Like the M4 spike, it drives itself through
//! scripted phases with vsync off, prints frame times per phase, and exits.
//!
//! ```sh
//! cargo run --release -p aic-app --example perf_app
//! ```
//!
//! Pass: frames stay under about 8 ms (PLAN.md, M4). Keep the window in front: macOS stops drawing
//! a hidden window, and the run then never finishes. `PERF_NODES=300` changes the graph size.
//!
//! `PERF_IDLE=20` instead leaves the app alone for 20 seconds, with vsync on as in the real app,
//! and prints how many frames it drew and what asked for them: an idle app should draw none.
//! `PERF_MINIMIZE=1` minimizes the window for that time.
//!
//! `PERF_HEADLESS=1` opens no window: it builds and tessellates the UI 600 times, then 600 more
//! with the live values changing, and prints the CPU time per frame. With no GPU and no compositor
//! in the way its numbers vary much less from run to run.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use aic_app::i18n::Strings;
use aic_app::{App, load_catalog};
use aic_core::{NodeId, NodeKind, PortRef};
use aic_data::{Catalog, ItemId, RecipeId};
use eframe::egui::{self, Pos2, vec2};

const WARMUP: usize = 30;
const FRAMES: usize = 240;
const COLUMNS: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// Nothing changes; the app redraws every frame.
    Idle,
    /// The pointer scrolls the canvas.
    Pan,
    /// A source limit changes every frame, so every frame solves and redraws new numbers.
    LiveValues,
}

const PHASES: [Phase; 3] = [Phase::Idle, Phase::Pan, Phase::LiveValues];

struct PerfApp {
    app: App,
    phase: usize,
    frame: usize,
    last: Option<Instant>,
    times: HashMap<usize, Vec<f64>>,
    /// Time spent building the UI each frame, without painting it.
    ui_times: HashMap<usize, Vec<f64>>,
    live: Option<NodeId>,
    idle: Option<Idle>,
}

/// The idle run: frames drawn after the warm-up, and what asked for each.
struct Idle {
    seconds: u64,
    minimize: bool,
    started: bool,
    seen: Arc<Mutex<Seen>>,
}

#[derive(Default)]
struct Seen {
    frames: usize,
    causes: HashMap<String, usize>,
}

impl PerfApp {
    fn idle_ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        eframe::App::ui(&mut self.app, ui, frame);
        let ctx = ui.ctx();
        let idle = self.idle.as_mut().expect("idle run");
        self.frame += 1;
        if idle.started {
            let mut seen = idle.seen.lock().expect("not poisoned");
            seen.frames += 1;
            for cause in ctx.repaint_causes() {
                *seen.causes.entry(cause.to_string()).or_default() += 1;
            }
        } else if self.frame < WARMUP {
            // Let the first solve, icons and the auto-fit settle, then leave it alone.
            ctx.request_repaint();
        } else {
            idle.started = true;
            if idle.minimize {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }
            // A minimized window draws nothing, so the count is read and the run ended from here.
            let (seen, seconds) = (idle.seen.clone(), idle.seconds);
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(seconds));
                let seen = seen.lock().expect("not poisoned");
                println!(
                    "idle {seconds} s: {} frames ({:.2} per second)",
                    seen.frames,
                    seen.frames as f64 / seconds as f64
                );
                for (cause, count) in &seen.causes {
                    println!("  {count:>5}  {cause}");
                }
                std::process::exit(0);
            });
        }
    }
}

impl eframe::App for PerfApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if self.idle.is_some() {
            return self.idle_ui(ui, frame);
        }
        let now = Instant::now();
        if let Some(last) = self.last
            && self.frame >= WARMUP
        {
            let ms = now.duration_since(last).as_secs_f64() * 1000.0;
            self.times.entry(self.phase).or_default().push(ms);
        }
        self.last = Some(now);

        let Some(&phase) = PHASES.get(self.phase) else {
            return;
        };
        if phase == Phase::LiveValues
            && let (Some(node), Some(editor)) = (self.live, self.app.editor_mut())
        {
            let limit = 30.0 + (self.frame % 60) as f64;
            let _ = editor.document_mut().set_limit(node, Some(limit));
        }
        let started = Instant::now();
        eframe::App::ui(&mut self.app, ui, frame);
        if self.frame >= WARMUP {
            let ms = started.elapsed().as_secs_f64() * 1000.0;
            self.ui_times.entry(self.phase).or_default().push(ms);
        }

        self.frame += 1;
        if self.frame == WARMUP + FRAMES {
            self.frame = 0;
            self.phase += 1;
            if self.phase == PHASES.len() {
                self.report();
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
        }
        ui.ctx().request_repaint();
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        if self.idle.is_some() {
            return;
        }
        let Some(phase) = PHASES.get(self.phase) else {
            return;
        };
        if *phase == Phase::Pan {
            let center = ctx.content_rect().center();
            raw_input
                .events
                .push(egui::Event::PointerMoved(center + vec2(0.0, 100.0)));
            let step = if (self.frame / 60).is_multiple_of(2) {
                8.0
            } else {
                -8.0
            };
            raw_input.events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: vec2(step, 0.0),
                modifiers: egui::Modifiers::NONE,
                phase: egui::TouchPhase::Move,
            });
        }
    }
}

impl PerfApp {
    fn report(&self) {
        println!(
            "{:<12} {:>9} {:>9} {:>9} {:>9}",
            "phase", "mean ms", "p95 ms", "max ms", "ui ms"
        );
        for (index, phase) in PHASES.iter().enumerate() {
            let mut sorted = self.times.get(&index).cloned().unwrap_or_default();
            if sorted.is_empty() {
                continue;
            }
            sorted.sort_by(f64::total_cmp);
            let mean = sorted.iter().sum::<f64>() / sorted.len() as f64;
            let p95 = sorted[(sorted.len() * 95 / 100).min(sorted.len() - 1)];
            let max = sorted.last().copied().unwrap_or_default();
            let ui = self.ui_times.get(&index).cloned().unwrap_or_default();
            let ui_mean = ui.iter().sum::<f64>() / ui.len().max(1) as f64;
            println!(
                "{:<12} {mean:>9.2} {p95:>9.2} {max:>9.2} {ui_mean:>9.2}",
                format!("{phase:?}")
            );
        }
    }
}

/// About `target` recipe nodes in the graph's region: each recipe next to a producer of each of its
/// inputs, linked, every recipe node limited to one facility so the whole graph solves. Returns a
/// node whose limit the live phase changes.
fn build(app: &mut App, target: usize) -> Option<NodeId> {
    let editor = app.editor_mut()?;
    let catalog: Catalog = editor.catalog().clone();
    let region = editor.document().graph().region().clone();
    let recipes: Vec<RecipeId> = catalog
        .recipes()
        .values()
        .filter(|r| !r.hidden && r.available_in(&region))
        .map(|r| r.id.clone())
        .collect();
    let producer_of = |item: &ItemId| {
        recipes.iter().find(|id| {
            catalog
                .recipe(id.as_str())
                .is_some_and(|r| r.outputs.iter().any(|(out, _)| out == item))
        })
    };

    // Plan first: which recipe each node runs, and which node feeds which, by index.
    let mut planned: Vec<RecipeId> = Vec::new();
    let mut feeds: Vec<(usize, usize, ItemId)> = Vec::new();
    'fill: loop {
        for recipe in &recipes {
            let consumer = planned.len();
            planned.push(recipe.clone());
            let inputs = catalog.recipe(recipe.as_str()).map(|r| r.inputs.clone());
            for (item, _) in inputs.unwrap_or_default() {
                if let Some(producer) = producer_of(&item) {
                    feeds.push((planned.len(), consumer, item));
                    planned.push(producer.clone());
                }
            }
            if planned.len() >= target {
                break 'fill;
            }
        }
    }

    let mut doc = editor.document_mut();
    let mut nodes = Vec::new();
    for (index, recipe) in planned.iter().enumerate() {
        let facility = catalog.recipe(recipe.as_str())?.facilities.first()?.clone();
        let pos = Pos2::new(
            (index % COLUMNS) as f32 * 360.0,
            (index / COLUMNS) as f32 * 260.0,
        );
        nodes.push(doc.add_node(
            NodeKind::Recipe {
                recipe: recipe.clone(),
                facility,
                limit: Some(1.0),
            },
            pos,
        ));
    }
    for (producer, consumer, item) in feeds {
        let _ = doc.connect(
            PortRef::new(nodes[producer], item.clone()),
            PortRef::new(nodes[consumer], item),
        );
    }
    println!(
        "{} nodes, {} links",
        doc.graph().nodes().len(),
        doc.graph().links().len()
    );
    nodes.first().copied()
}

/// The headless run: the UI's CPU cost per frame, without painting it.
fn headless(catalog: Result<Catalog, String>, target: usize) {
    let ctx = egui::Context::default();
    aic_app::theme::install(&ctx);
    let mut app = App::new(Strings::english(), catalog, None);
    let live = build(&mut app, target);
    let mut time = 0.0;
    let mut frames = |app: &mut App, count: usize, live: Option<NodeId>| {
        let mut times = Vec::with_capacity(count);
        for frame in 0..count {
            if let (Some(node), Some(editor)) = (live, app.editor_mut()) {
                let limit = 30.0 + (frame % 60) as f64;
                let _ = editor.document_mut().set_limit(node, Some(limit));
            }
            time += 1.0 / 60.0;
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 900.0))),
                time: Some(time),
                ..Default::default()
            };
            let started = Instant::now();
            let output = ctx.run_ui(input, |ui| {
                eframe::App::ui(app, ui, &mut eframe::Frame::_new_kittest());
            });
            let _ = ctx.tessellate(output.shapes, output.pixels_per_point);
            times.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        times
    };
    let _ = frames(&mut app, 60, None);
    println!(
        "{:<12} {:>9} {:>9} {:>9}",
        "phase", "mean ms", "median ms", "p95 ms"
    );
    for (name, live) in [("Idle", None), ("LiveValues", live)] {
        let mut times = frames(&mut app, 600, live);
        times.sort_by(f64::total_cmp);
        let mean = times.iter().sum::<f64>() / times.len() as f64;
        let median = times[times.len() / 2];
        let p95 = times[times.len() * 95 / 100];
        println!("{name:<12} {mean:>9.2} {median:>9.2} {p95:>9.2}");
    }
}

fn main() -> eframe::Result {
    let catalog = load_catalog(None).map_err(|error| error.to_string());
    let target: usize = std::env::var("PERF_NODES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(200);
    if std::env::var("PERF_HEADLESS").is_ok_and(|v| v == "1") {
        headless(catalog, target);
        return Ok(());
    }
    let idle = std::env::var("PERF_IDLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .map(|seconds| Idle {
            seconds,
            minimize: std::env::var("PERF_MINIMIZE").is_ok_and(|v| v == "1"),
            started: false,
            seen: Arc::default(),
        });
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1600.0, 900.0]),
        ..Default::default()
    };
    if idle.is_none() {
        options.wgpu_options.surface.present_mode = eframe::wgpu::PresentMode::AutoNoVsync;
    }
    eframe::run_native(
        "perf app",
        options,
        Box::new(move |cc| {
            aic_app::theme::install(&cc.egui_ctx);
            let mut app = App::new(Strings::english(), catalog, None);
            let live = build(&mut app, target);
            Ok(Box::new(PerfApp {
                app,
                phase: 0,
                frame: 0,
                last: None,
                times: HashMap::new(),
                ui_times: HashMap::new(),
                live,
                idle,
            }))
        }),
    )
}
