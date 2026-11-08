# AIC Modeler — Implementation Plan

A personal, desktop, Satisfactory-Modeler-style factory planner for the AIC in *Arknights: Endfield*.
You place machine nodes, pick a recipe for each, wire outputs to inputs, and the app solves the
flow: machines needed per node (fractional, shown as %), rate on every edge, raw inputs, byproducts and power.

- Language: Rust (stable, edition 2024)
- UI: `egui` via `eframe` + `egui-snarl` for the node canvas (prototype; may move to GPUI/iced later)
- Solver: linear programming via `good_lp` (pure-Rust `microlp` backend by default)
- Data: copied by hand into `data/<version>/`. The first set is taken from
  [endfield-calc/factoriolab](https://github.com/endfield-calc/factoriolab) `src/data/aef/`, but the app
  doesn't depend on that repo.

---

## 1. Ground rules (copy these into `CLAUDE.md`)

1. **The UI never does math.** All solving lives in `aic-core`, which has no egui dependency. The UI
   builds a `Graph`, calls `solve(&catalog, &graph)`, and renders the `Solution`. This is what makes
   a later switch to GPUI or iced cheap.
2. **Internal IDs are game IDs** (`copper_nugget`, `furnance_1`, `mix_pool_1`, …), never display names.
   Display names come from the i18n table at render time.
3. **All rates are items per minute** inside the app. Convert once, in the data layer.
4. **Data loading goes through our own schema.** `data/<version>/data.json` uses the factoriolab
   format for now, but only `aic-data/src/raw.rs` knows that. Everything else uses `Catalog`. Data
   from other sources gets converted into the same folder layout by a future extractor CLI.
5. **Data is copied in by hand, not fetched.** No submodules, no clone scripts, no network access at build or run time.
6. Every milestone ends with `cargo test --workspace` and `cargo clippy --workspace -- -D warnings`
   passing.

---

## 2. Facts about the source data (verified 2026-11-08)

### Data folder layout

```
data/
└─ <version>/                 # e.g. v1.5.3, named after the game version
   ├─ data.json               # required: items, machines, recipes, icon positions, locations
   ├─ i18n/
   │  └─ en.json              # required: English names keyed by the same IDs
   ├─ icons.webp              # optional until M5: sprite sheet that data.json → icons[] points into
   ├─ overlay.json            # optional (M7): our own patches on top of data.json
   └─ SOURCE.md               # optional: where it came from, commit/date, any hand edits
```

- Max copies these files in by hand. Initial source: `endfield-calc/factoriolab`, `src/data/aef/`
  (`data.json`, `i18n/en.json`, `icons.webp`). In that repo, ignore `data-dsp.json`, `data-sfy.json`,
  `data-spa.json` and `defaults-spa.json`. Those are Dyson Sphere Program / Satisfactory / Factorio test
  fixtures from upstream, not Endfield.
- More languages can go in later as `i18n/<lang>.json`, using the same shape as `en.json`.
- **Version selection:** the app scans `data/` for subfolders that contain `data.json`, picks the highest
  version by default (compare with semver-ish parsing on the `vX.Y.Z` name, falling back to string order),
  and lets you switch in Settings. The folder name is the version string. If the folder name doesn't match
  `data.json → version`, log it.
- **Locating `data/`:** the `--data <path>` CLI flag, then `AIC_DATA_DIR`, then `./data` next to the
  executable, then `./data` in the working directory. Show a clear error screen if none is found. Don't
  `include_bytes!` the data, so you can drop in a new version without rebuilding.
- **Saved graphs record the data version they were made with.** On load, if that version isn't the
  active one, show a banner, and still load by ID (see M2).

### Facts about the first data set (factoriolab `aef`, verified 2026-11-08)

`data.json` files:

| Key | Use |
|---|---|
| `items` / `recipes` / `locations` / `categories` | game data, below |
| `icons[]` | `{ id, position: "-64px 0px", color }` into `icons.webp` (1024×1024, 64×64 cells) |
| `version` | `{ "arknights-endfield": "v1.5.3" }` |

`i18n/en.json` has keys `name`, `version`, `categories`, `items` (161), `recipes` (257) and `locations`,
each a map from ID to English name.

This data set is **v1.5.3** (so it goes in `data/v1.5.3/`), and that repo's last commit was 2026-09-07.
The game is already at v1.6, so expect a few missing or changed recipes. A `data/v1.6/` folder, from
another source or with an overlay, is the fix.

Shape:

- `items[]`: 161 entries. Keys: `id, name (zh), category, row, icon, stack`, plus optional
  `machine`, `belt`, `pipe`, `buildIcon`.
  - 34 items have `machine: { speed, type: "electric", usage, size?: [w,h], locations?, hideRate?, totalRecipe? }`.
    `usage` is power draw. `size` is the footprint (not needed for the modeler).
  - `belt` item: `belt.speed = 0.5` (items/sec, so **30/min**). `pipe` item: `pipe.speed = 2` (**120/min**).
    Confirm both against the game.
- `recipes[]`: 257 entries, IDs unique. Keys: `id, name, category, row, icon, time (sec), in {item: qty},
  out {item: qty}, producers [machine ids]`, plus optional `flags, usage, cost, locations`.
  - `producers` has 1 entry for 247 recipes and 2 for 10 (e.g. `mix_pool_1` / `mix_pool_2`).
    A node picks recipe + producer, defaulting to the first.
  - `flags`: `mining` (9 recipes, empty `in`, e.g. `originium_ore`), `hideProducer` (28, outpost/settlement
    "recipes" like `tundra_coupon-iron_cmpt`).
  - **Byproducts exist:** 6 recipes have more than one output, e.g. `copper_nugget`:
    `{copper_ore:1, liquid_water:1} → {copper_nugget:1, liquid_sewage:1}`.
  - **Recipe-level `usage` overrides machine usage.** Negative means power *generation*:
    `power_proc_battery_1..5` consume a battery over 40 s and have `usage` −220 … −3200, with empty `out`.
  - **Cycles exist:** `phase_trans_1-liquid_xiranite` and `phase_trans_2-gas_xiranite` have the same item on
    both sides, and 79 items can be made by more than one recipe (seed ↔ plant loops). The solver must
    handle loops, which is a big reason to use LP instead of tree propagation.
  - Odd times worth a test: `sewage-treat-export` (`liquid_sewage: 360 → liquid_xiranite_poly: 12`), some 3600 s recipes.
- `locations`: `tundra` (Valley IV) and `jinlong` (Wuling). Items, recipes and machines can be restricted to one.
- To check: `miner_2` and `miner_3` both have `speed: 1` and the flag `totalRecipe`. Check in-game whether the
  higher-tier miner is actually faster, and add a per-machine speed override in the overlay if so.

Rate formula (per machine, per minute): `qty * 60 / time * machine.speed`.

License note: the code is ours. Game data and icons belong to Hypergryph/Gryphline. Fine for a personal
tool. If the repo is ever public, keep `data/*/` out of git (add a `.gitignore` rule plus a `data/README.md`
that explains the layout), or at least don't put it under the code's license.

---

## 3. Workspace layout

```
aic-modeler/
├─ Cargo.toml                 # [workspace]
├─ CLAUDE.md
├─ PLAN.md
├─ data/
│  ├─ README.md               # explains the layout below
│  └─ v1.5.3/
│     ├─ data.json
│     ├─ i18n/en.json
│     └─ icons.webp
└─ crates/
   ├─ aic-data/               # discover versions, load + normalize -> Catalog. deps: serde, serde_json, thiserror
   ├─ aic-core/               # Graph model, validation, solver. deps: aic-data, good_lp, serde, slotmap
   ├─ aic-app/                # eframe + egui-snarl UI. deps: aic-core, eframe, egui-snarl, image(webp), rfd
   └─ aic-extract/            # LATER (M8): CLI that converts other sources into data/<version>/
```

`aic-data` should expose a writer for its normalized format too (`Catalog` → JSON), even if unused at
first. The extractor CLI will need it, and it makes the internal format easy to inspect.

Crate versions (crates.io, checked 2026-11-08):

- `egui-snarl = "0.12"` requires `egui ^0.36`, so **pin `eframe = "0.36"`** even though 0.37 is out.
  Upgrade only when egui-snarl does.
- `good_lp = { version = "1.15", default-features = false, features = ["microlp"] }`. Add a `highs`
  feature later only if microlp is slow or unstable on big graphs.
- `serde`/`serde_json` 1.x, `slotmap`, `thiserror`, `image` with `webp`, `rfd` for file dialogs.

---

## 4. Milestones

### M0: Skeleton
- Workspace with the three crates (`aic-extract` comes later), `CLAUDE.md`, and `data/README.md`
  describing the layout.
- Max copies `data/v1.5.3/` in by hand. Claude Code should not try to download it.
- `aic-app` opens an empty eframe window.
- **Done when:** `cargo run -p aic-app` shows a window, and `cargo test --workspace` passes.

### M1: `aic-data`, load the catalog
- `discover.rs`: `find_data_dir()` (resolution order from section 2), `list_versions(dir) -> Vec<DataVersion>`
  (folders with a `data.json`, sorted newest first), and `load(version_dir, lang) -> Result<Catalog>`.
  A missing `icons.webp` is not an error. `Catalog.icons_path` is then `None`.
- Errors name the exact file and JSON path that failed (`serde_path_to_error` helps here).
- `raw.rs`: serde structs that mirror factoriolab exactly. Use `#[serde(default)]` for optional keys
  and `BTreeMap<String, f64>` for `in`/`out`.
- `catalog.rs`: the normalized model the rest of the app uses:

```rust
pub struct Catalog {
    pub items: IndexMap<ItemId, Item>,       // name_en, name_zh, category, icon, is_fluid
    pub machines: IndexMap<MachineId, Machine>, // speed, power_kw, locations
    pub recipes: IndexMap<RecipeId, Recipe>,
    pub belt_per_min: f64,                    // 30
    pub pipe_per_min: f64,                    // 120
    pub icons: HashMap<String, IconRect>,     // pixel rect in icons.webp
    pub icons_path: Option<PathBuf>,          // data/<version>/icons.webp if present
    pub version: String,                      // folder name, e.g. "v1.5.3"
}
pub struct Recipe {
    pub id: RecipeId, pub name_en: String, pub time_s: f64,
    pub inputs: Vec<(ItemId, f64)>, pub outputs: Vec<(ItemId, f64)>, // qty per craft
    pub producers: Vec<MachineId>,
    pub power_override_kw: Option<f64>,       // recipe.usage (negative = generation)
    pub is_mining: bool, pub hidden_producer: bool,
    pub locations: Vec<LocationId>,
}
impl Recipe { pub fn rate_per_min(&self, qty: f64, speed: f64) -> f64 { qty * 60.0 / self.time_s * speed } }
```

- Item IDs and machine IDs are newtypes around `String` (or interned `u32` later if profiling says so).
- Fluid detection: the item is `liquid_*` or `gas_*`, or appears in a recipe that a pipe-only machine uses.
  Start with the prefix rule and adjust.
- Fall back to the zh name, then the id, when an English name is missing, and log it once.
- **Tests:** use small hand-written fixtures in `crates/aic-data/tests/fixtures/<version>/` (a few
  items and recipes, including a byproduct, a mining recipe and a power recipe), so tests don't depend
  on the real data being present. Add one `#[ignore]`d test that loads the real `data/v1.5.3/` and
  checks: 161 items, 257 recipes, 34 machines. `copper_nugget` has outputs `copper_nugget` and
  `liquid_sewage`. `power_proc_battery_1` power override is −220. `belt_per_min == 30`. Every recipe
  producer exists in `machines`. Every item referenced by a recipe exists.

### M2: `aic-core` graph model
```rust
pub struct Graph { pub nodes: SlotMap<NodeId, Node>, pub edges: SlotMap<EdgeId, Edge> }
pub enum NodeKind {
    Recipe { recipe: RecipeId, machine: MachineId, count: CountMode },
    Source { item: ItemId, cap_per_min: Option<f64> },  // raw supply, e.g. ore from miners/depot
    Sink   { item: ItemId, target: SinkMode },          // what you want out
}
pub enum CountMode { Auto, Fixed(f64), AtMost(f64) }
pub enum SinkMode  { Fixed(f64), Maximize }
pub struct Edge { pub from: (NodeId, ItemId), pub to: (NodeId, ItemId), pub max_per_min: Option<f64> }
```
- Ports are derived from the recipe, one per item, so an edge is identified by `(node, item)` on each end.
- `validate()` rejects edges whose item doesn't exist on both ends or that connect a node to itself.
  Duplicate edges between the same two ports are rejected too.
- Save/load is serde JSON with a `format_version` field and a `data_version` field (for example `"v1.5.3"`). The file stores IDs only, so it survives data updates.
  Unknown recipe IDs on load turn into a "missing recipe" node rather than an error.
- **Tests:** round-trip save/load, and validation cases.

### M3: Solver (the important part)
Model the whole graph as one LP. Rebuild it on every change, since graphs are small.

Variables (all ≥ 0):
- `m_n`: machine count for each Recipe node
- `f_e`: flow per minute on each edge
- `x_p`: implicit external flow on each **unconnected** port (unconnected input = pulled from depot,
  unconnected output = sent to depot)
- `s_p`: surplus on each **connected** output port (overproduction that would back up in-game)
- `src_n`, `snk_n`: totals for Source/Sink nodes

Constraints:
- Recipe output port (item i, connected): `Σ f_out + s_p = m_n · rate_out(i)`
- Recipe output port (unconnected): `x_p = m_n · rate_out(i)`
- Recipe input port (connected): `Σ f_in = m_n · rate_in(i)`, with **no** deficit variable, so a wired input must be fully fed
- Recipe input port (unconnected): `x_p = m_n · rate_in(i)`
- Source: `Σ f_out = src_n`, and `src_n ≤ cap` if a cap is set
- Sink: `Σ f_in = snk_n`, and `snk_n = target` if `Fixed`
- `CountMode::Fixed(k)` gives `m_n = k`, and `AtMost(k)` gives `m_n ≤ k`
- Edge with `max_per_min`: `f_e ≤ max`

Objective (minimize), with clearly separated weights:
`1e6·Σ s_p  +  1e3·Σ x_p(unconnected inputs)  +  1·Σ m_n  −  1e4·Σ snk_n(Maximize sinks)`
- Surplus is the worst outcome. Pulling from the depot is allowed but discouraged. Fewer machines is better.
- If the weights fight each other in practice, switch to a two-pass lexicographic solve
  (1: maximize sinks, 2: fix that and minimize the rest).

Result:
```rust
pub struct Solution {
    pub status: SolveStatus,            // Optimal | Infeasible | Unbounded | Empty
    pub machines: HashMap<NodeId, f64>, // 1.75 → shown as "1.75 (2 × 88%)"
    pub edge_flow: HashMap<EdgeId, f64>,
    pub port_external: HashMap<(NodeId, ItemId), f64>,
    pub surplus: HashMap<(NodeId, ItemId), f64>,
    pub totals: Totals,                 // raw inputs, depot outputs, byproducts, power draw/generation
    pub warnings: Vec<Warning>,
}
```
Warnings:
- `Surplus { port }`: in Endfield a blocked output (including sewage) **stalls the machine**, so make this loud.
- `BeltOverCapacity { edge, belts_needed }`: flow > 30/min solid or > 120/min fluid. Show `⌈flow/cap⌉` lanes.
- `NothingDrivesProduction`: no Fixed sink, Fixed count or Maximize sink, so everything solves to 0.
- `Unbounded`: a Maximize sink with nothing limiting it. Suggest adding a source cap or a fixed count.
- `Infeasible`: list the Fixed constraints involved so the user knows what to relax.

Power: `Σ m_n · (recipe.power_override_kw.unwrap_or(machine.power_kw))`, reported as draw and generation separately.

**Tests (golden graphs, built in code):**
1. Single recipe with a fixed sink target. Machines = target / rate.
2. `copper_nugget` with sewage unconnected. Sewage shows up in totals as depot output, with no warning.
3. Same with sewage wired to a sink capped too low. Expect a `Surplus` warning.
4. One output split to two consumers. Flows sum correctly.
5. Seed ↔ plant loop (`planter_1` / `seedcol_1`). Converges, no unbounded result.
6. A source capped at 60/min feeding a three-step chain with a Maximize sink. Final rate matches hand math.
7. Reproduce the user's reference screenshot: 60/min ore source → refinery ×2 splitting 33.75 / 26.25 →
   … → final assembler 1.25 producing 2.5/min. Pick the matching recipe IDs, then assert node counts
   1.13 / 1.75 / 1.13 / 0.75 / 1.25 within 0.01 and edge flows 22.5 / 45 / 11.25 / 15 / 3.75.

### M4: egui shell + node canvas
- `eframe` app with three areas: left **recipe browser** (search by English name, filter by category and
  location), center **`egui-snarl` canvas**, right **inspector** (selected node or edge, plus totals).
- `SnarlViewer` implementation:
  - Header: icon, recipe name, machine picker (when `producers.len() > 1`), and a count badge like `1.75`
    with `(2 × 88%)` underneath.
  - Input and output pins, one per item, each showing icon and rate. Pins are colored solid vs fluid.
  - `connect()` refuses mismatched items, and also solid ↔ fluid.
  - Right-click on the canvas opens a searchable "Add recipe / source / sink" menu.
  - Right-click on an unconnected input pin opens "Add producer of X", which creates the upstream node
    and wires it. This is the most-used action in Modeler-style tools, so build it early.
- Wire labels: the flow rate at the wire midpoint, colored red when over belt capacity.
- Re-solve on every graph change. Synchronous is fine at first. Measure, and move to a worker thread
  only if a solve takes more than about 16 ms.
- The UI keeps a mapping between snarl node IDs and `aic-core` `NodeId`s, or stores the `NodeId` in the
  snarl node payload. Snarl is only the view and `aic-core::Graph` is the source of truth.
- **Done when:** you can rebuild the screenshot graph by hand and see the same numbers.

### M5: Icons
- If `Catalog.icons_path` is `None`, draw a colored square with the item's initials instead, so the app
  works without icons.
- Load `icons.webp` once with `image`, upload as one `egui::TextureHandle`, and draw each icon as an
  `egui::Image` with a UV rect from `IconRect`. Parse `"-64px 0px"` into `(64, 0)`, with cell size 64.
- Fall back to the `missing-icon` cell, and use the icon `color` as a tint for pin dots.

### M6: Usability
- File menu with New / Open / Save / Save As (`rfd`), plus a recent-files list kept in eframe storage.
- Undo/redo with a command stack of `Graph` snapshots. Graphs are small, so snapshot clones are fine.
- Box selection, delete, duplicate, Ctrl+C/V within the app.
- Totals panel: raw inputs per minute, depot outputs, byproducts, power (draw / generation / net),
  and machine counts per machine type (rounded up).
- Location toggle (Valley IV / Wuling / both) that filters the recipe browser.
- "Round up" view: show `ceil(m)` machines and the resulting % utilization per machine.

### M7: Data updates & extras (later)
- `data/<version>/overlay.json`: add, replace or remove recipes and items, and override machine speeds.
  Merge it at load time after `data.json`. Use it for small fixes; a whole new game version gets its own folder.
- Settings: switch data version and language (from the `i18n/*.json` files present).
- Black-box groups: collapse a selection into one node that exposes its external ports, like
  Satisfactory Modeler's factories.
- Ratio constraints on splits (for example 50/50 on one output port).
- Export the canvas as a PNG.
- Performance check with about 200 nodes. If egui feels slow, the core is already UI-free, so porting to
  GPUI or iced only replaces `aic-app`.

### M8: `aic-extract` CLI (later)
- `aic-extract <source-kind> <input> --out data/<version>/` writes `data.json` (factoriolab format,
  for now) plus `i18n/*.json`, and appends to `SOURCE.md`.
- Source kinds to consider: factoriolab `aef` folder (a copy and check, useful as the reference
  implementation), warfarin.wiki facility pages (the zh source kokobird uses, which already has v1.6),
  and endfield.wiki.gg (English).
- `aic-extract diff data/v1.5.3 data/v1.6` lists added, removed and changed recipes. That's handy before
  switching versions.
- Validate output by loading it with `aic-data` and running the same consistency checks as the M1 tests.

---

## 5. Suggested Claude Code session order

1. "Read PLAN.md. Do M0 and M1. Don't fetch any data; I've put it in data/v1.5.3/. Write the M1 tests
   with small fixtures first."
2. "Do M2."
3. "Do M3. Write golden tests 1–6 first and make them pass, then I'll give you recipe IDs for test 7."
4. "Do M4 using egui-snarl 0.12 with eframe 0.36." Then M5 and M6, one at a time.

Keep each milestone as its own commit or branch so a bad UI experiment is easy to throw away.

## 6. Open questions to settle while building

- Exact belt and pipe throughput in-game (30 and 120 per minute assumed from the data).
- Whether higher-tier miners are faster (the data says both have speed 1).
- How settlement / `hideProducer` recipes should appear. Maybe as a special Sink kind ("outpost order")
  instead of a machine node.
- Whether to model depot (core) I/O as explicit nodes instead of implicit unconnected ports.