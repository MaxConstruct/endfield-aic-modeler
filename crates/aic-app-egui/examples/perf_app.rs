//! M7 performance run: the real app, with real data, icons, the inspector and the totals, on a
//! generated graph of about 200 linked recipe nodes. Like the M4 spike, it drives itself through
//! scripted phases with vsync off, prints frame times per phase, and exits.
//!
//! ```sh
//! cargo run --release -p aic-app-egui --example perf_app
//! ```
//!
//! Pass: frames stay under about 8 ms (PLAN.md, M4). Keep the window in front: macOS stops drawing
//! a hidden window, and the run then never finishes. `PERF_NODES=300` changes the graph size.

use std::collections::HashMap;
use std::time::Instant;

use aic_app_egui::i18n::Strings;
use aic_app_egui::{App, load_catalog};
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
}

impl eframe::App for PerfApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
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

    let doc = editor.document_mut();
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
            &catalog,
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

fn main() -> eframe::Result {
    let catalog = load_catalog(None).map_err(|error| error.to_string());
    let target: usize = std::env::var("PERF_NODES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(200);
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1600.0, 900.0]),
        ..Default::default()
    };
    options.wgpu_options.surface.present_mode = eframe::wgpu::PresentMode::AutoNoVsync;
    eframe::run_native(
        "perf app",
        options,
        Box::new(move |_cc| {
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
            }))
        }),
    )
}
