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
//! `PERF_HEADLESS=1` opens no window: it builds and tessellates the UI 600 times for each phase
//! (`PERF_FRAMES` changes that), and prints the CPU time per frame, then the medians of its parts:
//! building the UI, the solve inside that, and tessellating. With no GPU and no compositor in the
//! way its numbers vary much less from run to run.
//!
//! `PERF_ZOOM=1.0` first zooms the canvas to that zoom (1 is 100%) around its middle, so only part
//! of the graph is on screen, in the windowed and the headless run alike.
//!
//! `PERF_GRAPH=chain` builds one section in place of the usual graph of many small ones: pairs of
//! a producer and a consumer of one item, each producer also feeding the next pair's consumer,
//! round in a ring. `PERF_NODES` nodes in all.
//!
//! With the `dhat-heap` feature, the headless run also counts allocations per frame and writes
//! dhat's file of where they were made (`dhat-heap.json`, for dhat's viewer), from the measured
//! phases only. dhat is slow, so it draws 20 frames a phase unless `PERF_FRAMES` says otherwise,
//! and its times are not worth reading. `PERF_PHASE=Idle` (or `Pan`, `LiveValues`) runs that phase
//! alone, so the file is about it alone.
//!
//! ```sh
//! PERF_HEADLESS=1 PERF_ZOOM=0.25 PERF_PHASE=Idle \
//!     cargo run --release -p aic-app --features dhat-heap --example perf_app
//! ```

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
    /// Shift with the wheel pans the canvas sideways, 8 points a frame, turning every 60 frames.
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
    /// The zoom to set once the first solve and the auto-fit have settled (`PERF_ZOOM`).
    zoom: Option<f32>,
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
        if self.frame == WARMUP / 2
            && let Some(zoom) = self.zoom.take()
            && let Some(editor) = self.app.editor_mut()
        {
            editor.zoom_canvas_to(zoom);
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
        let Some(&phase) = PHASES.get(self.phase) else {
            return;
        };
        input(ctx, raw_input, phase, self.frame);
    }
}

/// A frame's scripted input. In the pan phase: the pointer below the middle of the window, and
/// Shift with the wheel, which pans the canvas (the wheel alone zooms it). egui keeps Shift down
/// until told otherwise, so the next phase lets go of it, once egui's smoothed scroll has run
/// out: what is left of it would zoom without Shift.
fn input(ctx: &egui::Context, raw_input: &mut egui::RawInput, phase: Phase, frame: usize) {
    if phase != Phase::Pan {
        if !ctx.input(|i| i.is_scrolling()) {
            raw_input
                .events
                .push(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
        }
        return;
    }
    let center = ctx.content_rect().center();
    raw_input
        .events
        .push(egui::Event::PointerMoved(center + vec2(0.0, 100.0)));
    let step = if (frame / 60).is_multiple_of(2) {
        8.0
    } else {
        -8.0
    };
    raw_input
        .events
        .push(egui::Event::ModifiersChanged(egui::Modifiers::SHIFT));
    raw_input.events.push(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(step, 0.0),
        modifiers: egui::Modifiers::SHIFT,
        phase: egui::TouchPhase::Move,
    });
}

impl PerfApp {
    fn report(&self) {
        if let Some(view) = self.app.editor().and_then(|editor| editor.canvas_view()) {
            println!("canvas at {:.0}%", view.zoom * 100.0);
        }
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

/// The graph `PERF_GRAPH` asks for, with about `target` nodes. Returns a node whose limit the live
/// phase changes.
fn build(app: &mut App, target: usize) -> Option<NodeId> {
    if std::env::var("PERF_GRAPH").is_ok_and(|v| v == "chain") {
        build_chain(app, target)
    } else {
        build_many(app, target)
    }
}

/// The recipes of the graph's region, and a way to find a producer of an item among them.
fn region_recipes(catalog: &Catalog, region: &aic_data::RegionId) -> Vec<RecipeId> {
    catalog
        .recipes()
        .values()
        .filter(|r| !r.hidden && r.available_in(region))
        .map(|r| r.id.clone())
        .collect()
}

fn producer_of<'a>(
    catalog: &Catalog,
    recipes: &'a [RecipeId],
    item: &ItemId,
) -> Option<&'a RecipeId> {
    recipes.iter().find(|id| {
        catalog
            .recipe(id.as_str())
            .is_some_and(|r| r.outputs.iter().any(|(out, _)| out == item))
    })
}

/// One section of `target` recipe nodes: `target / 2` pairs of a producer and a consumer of one
/// item, each limited to one facility. Producer `i` feeds consumer `i` and consumer `i + 1`, and
/// the last feeds the first, so every node is linked to every other through some path.
fn build_chain(app: &mut App, target: usize) -> Option<NodeId> {
    let editor = app.editor_mut()?;
    let catalog: Catalog = editor.catalog().clone();
    let region = editor.document().graph().region().clone();
    let recipes = region_recipes(&catalog, &region);
    let (consumer, item, producer) = recipes.iter().find_map(|consumer| {
        let recipe = catalog.recipe(consumer.as_str())?;
        recipe.inputs.iter().find_map(|(item, _)| {
            let producer = producer_of(&catalog, &recipes, item)?;
            (producer != consumer).then(|| (consumer.clone(), item.clone(), producer.clone()))
        })
    })?;
    let kind = |recipe: &RecipeId| -> Option<NodeKind> {
        Some(NodeKind::Recipe {
            recipe: recipe.clone(),
            facility: catalog.recipe(recipe.as_str())?.facilities.first()?.clone(),
            limit: Some(1.0),
        })
    };
    let mut doc = editor.document_mut();
    let pairs = (target / 2).max(1);
    let mut producers = Vec::with_capacity(pairs);
    let mut consumers = Vec::with_capacity(pairs);
    for pair in 0..pairs {
        let at = |row: usize| {
            Pos2::new(
                (pair % COLUMNS) as f32 * 360.0,
                ((pair / COLUMNS) * 2 + row) as f32 * 260.0,
            )
        };
        producers.push(doc.add_node(kind(&producer)?, at(0)));
        consumers.push(doc.add_node(kind(&consumer)?, at(1)));
    }
    for (pair, &from) in producers.iter().enumerate() {
        for to in [pair, (pair + 1) % pairs] {
            let _ = doc.connect(
                PortRef::new(from, item.clone()),
                PortRef::new(consumers[to], item.clone()),
            );
        }
    }
    println!(
        "chain: {} nodes, {} links",
        doc.graph().nodes().len(),
        doc.graph().links().len()
    );
    producers.first().copied()
}

/// About `target` recipe nodes in the graph's region: each recipe next to a producer of each of its
/// inputs, linked, every recipe node limited to one facility so the whole graph solves.
fn build_many(app: &mut App, target: usize) -> Option<NodeId> {
    let editor = app.editor_mut()?;
    let catalog: Catalog = editor.catalog().clone();
    let region = editor.document().graph().region().clone();
    let recipes = region_recipes(&catalog, &region);
    let producer_of = |item: &ItemId| producer_of(&catalog, &recipes, item);

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

/// One headless frame, taken apart: building the UI, the solve inside it, tessellating, and with
/// the `dhat-heap` feature the allocations of each.
#[derive(Clone, Copy, Default)]
struct Parts {
    /// Building the UI and tessellating it, in ms.
    total: f64,
    /// Building the UI, the solve included.
    ui: f64,
    /// The solve at the end of the frame, if it solved.
    solve: Option<f64>,
    tessellate: f64,
    /// Allocations while building the UI, and while tessellating.
    ui_allocs: u64,
    tessellate_allocs: u64,
}

/// Whether dhat's profiler runs: it starts after the set-up and the warm-up frames.
#[cfg(feature = "dhat-heap")]
static PROFILING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Blocks allocated so far, while dhat's profiler runs; 0 before it starts.
fn allocations() -> u64 {
    #[cfg(feature = "dhat-heap")]
    {
        if PROFILING.load(std::sync::atomic::Ordering::Relaxed) {
            dhat::HeapStats::get().total_blocks
        } else {
            0
        }
    }
    #[cfg(not(feature = "dhat-heap"))]
    {
        0
    }
}

/// The middle value of `values`, sorted.
fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    values.get(values.len() / 2).copied().unwrap_or_default()
}

/// The headless run: the UI's CPU cost per frame, without painting it.
fn headless(catalog: Result<Catalog, String>, target: usize) {
    let ctx = egui::Context::default();
    aic_app::theme::install(&ctx);
    let mut app = App::new(Strings::english(), catalog, None);
    let live = build(&mut app, target);
    let mut time = 0.0;
    let mut frames = |app: &mut App, count: usize, phase: Phase, live: Option<NodeId>| {
        let mut parts = Vec::with_capacity(count);
        for frame in 0..count {
            let mut solves = false;
            if phase == Phase::LiveValues
                && let (Some(node), Some(editor)) = (live, app.editor_mut())
            {
                let limit = 30.0 + (frame % 60) as f64;
                solves = editor.document_mut().set_limit(node, Some(limit)).is_ok();
            }
            time += 1.0 / 60.0;
            let mut raw_input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 900.0))),
                time: Some(time),
                ..Default::default()
            };
            input(&ctx, &mut raw_input, phase, frame);
            let before = allocations();
            let started = Instant::now();
            let output = ctx.run_ui(raw_input, |ui| {
                eframe::App::ui(app, ui, &mut eframe::Frame::_new_kittest());
            });
            let built = started.elapsed();
            let after_ui = allocations();
            let _ = ctx.tessellate(output.shapes, output.pixels_per_point);
            let total = started.elapsed();
            let solve = solves
                .then(|| app.editor().and_then(|e| e.session().last_solve()))
                .flatten();
            parts.push(Parts {
                total: total.as_secs_f64() * 1000.0,
                ui: built.as_secs_f64() * 1000.0,
                solve: solve.map(|took| took.as_secs_f64() * 1000.0),
                tessellate: (total - built).as_secs_f64() * 1000.0,
                ui_allocs: after_ui - before,
                tessellate_allocs: allocations() - after_ui,
            });
        }
        parts
    };
    let _ = frames(&mut app, 60, Phase::Idle, None);
    if let (Some(zoom), Some(editor)) = (zoom(), app.editor_mut()) {
        editor.zoom_canvas_to(zoom);
        let _ = frames(&mut app, 30, Phase::Idle, None);
    }
    if let Some(editor) = app.editor() {
        if let Some(view) = editor.canvas_view() {
            println!("canvas at {:.0}%", view.zoom * 100.0);
        }
        println!("{} sections", editor.document().solution().sections.len());
    }
    let only: Option<String> = std::env::var("PERF_PHASE").ok();
    let phases: Vec<Phase> = (PHASES.into_iter())
        .filter(|phase| {
            only.as_ref()
                .is_none_or(|name| *name == format!("{phase:?}"))
        })
        .collect();
    let count = std::env::var("PERF_FRAMES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(if cfg!(feature = "dhat-heap") { 20 } else { 600 });
    // Only the measured phases go in dhat's file: the profiler starts after the set-up.
    // Whole backtraces: dhat's trimming left none at all on Windows. Build with
    // `--profile profiling` for them to have names.
    #[cfg(feature = "dhat-heap")]
    let profiler = dhat::Profiler::builder().trim_backtraces(None).build();
    #[cfg(feature = "dhat-heap")]
    PROFILING.store(true, std::sync::atomic::Ordering::Relaxed);
    println!(
        "{:<12} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9}",
        "phase",
        "mean ms",
        "median ms",
        "p95 ms",
        "ui ms",
        "solve ms",
        "tess ms",
        "ui alloc",
        "tess alloc"
    );
    for phase in phases {
        let parts = frames(&mut app, count, phase, live);
        let of = |part: fn(&Parts) -> f64| median(parts.iter().map(part).collect());
        let mut totals: Vec<f64> = parts.iter().map(|p| p.total).collect();
        totals.sort_by(f64::total_cmp);
        let mean = totals.iter().sum::<f64>() / totals.len() as f64;
        let p95 = totals[totals.len() * 95 / 100];
        let solves: Vec<f64> = parts.iter().filter_map(|p| p.solve).collect();
        let solve = if solves.is_empty() {
            "-".to_owned()
        } else {
            format!("{:.2}", median(solves))
        };
        println!(
            "{:<12} {mean:>9.2} {:>9.2} {p95:>9.2} {:>9.2} {solve:>9} {:>9.2} {:>9.0} {:>9.0}",
            format!("{phase:?}"),
            of(|p| p.total),
            of(|p| p.ui),
            of(|p| p.tessellate),
            of(|p| p.ui_allocs as f64),
            of(|p| p.tessellate_allocs as f64),
        );
    }
    #[cfg(feature = "dhat-heap")]
    drop(profiler);
}

/// The zoom `PERF_ZOOM` asks for, if any.
fn zoom() -> Option<f32> {
    std::env::var("PERF_ZOOM").ok().and_then(|v| v.parse().ok())
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
                zoom: zoom(),
            }))
        }),
    )
}
