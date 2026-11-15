# AIC Modeler — Implementation Plan

A desktop, Satisfactory-Modeler-style factory planner for the AIC in *Arknights: Endfield*.
You place nodes, pick a recipe for each, link outputs to inputs, and set limits where you have them.
The solver runs everything as high as those limits and the supply allow, and shows the facility count
of every node (fractional), the rate on every link, depot I/O, storage balances and power.

Vocabulary is defined in [`GLOSSARY.md`](../GLOSSARY.md); use those terms in code, UI and docs.
Decisions that are hard to reverse are in [`docs/adr/`](adr/):

- [0001](adr/0001-implicit-depot-ports.md): unconnected ports are implicit depot I/O.
- [0002](adr/0002-logical-planning-only.md): logical flow planning only, no spatial layout.
- [0003](adr/0003-limits-and-maximize.md): player values are limits, and the solver maximizes.
- [0004](adr/0004-throughput-measured-at-origins-and-sinks.md): pass-1 throughput is measured at origins and sinks.
- [0008](adr/0008-views-are-layout-not-document.md): views are layout state, not part of the document.
- [0010](adr/0010-one-frontend-editor-crate-stays.md): one UI (egui) over a GUI-free editor crate; supersedes [0009](adr/0009-two-frontends-one-editor-crate.md).

Tech:

- Language: Rust (stable, edition 2024)
- UI: `egui` via `eframe`, with `egui-snarl` for the node canvas. This is the shipping UI. Its
  app logic lives in the GUI-free `aic-editor` crate. An iced frontend (iced 0.14 + iced_nodegraph)
  was tried beside it and dropped after its performance spike failed; see
  [ADR 0010](adr/0010-one-frontend-editor-crate-stays.md) and
  [`archive/TASK_GUI_ICED.md`](archive/TASK_GUI_ICED.md).
- Solver: linear programming via `good_lp` (pure-Rust `microlp` backend by default)
- Data: copied by hand into `data/<version>/`. The first set comes from
  [endfield-calc/factoriolab](https://github.com/endfield-calc/factoriolab) `src/data/aef/`, but the app
  doesn't depend on that repo.

---

## 1. Scope

**In scope**

- Graph-first planning. You build the graph; the app never builds a whole production tree for you.
  "Add producer of X" on an input port and "Add consumer of X" on an output port are the only auto-build
  help, and each adds one node.
- One graph is one region (Valley IV or Wuling). Cross-region flow shows up as depot I/O.
- Node kinds: recipe, source, sink, splitter, merger, storage, control port, and workspaces with their
  ports (M7, ADR 0007).
- Power is reported (draw, generation, net). It is never a limit the solver uses.
- Belt and pipe capacity is a hint on a link, never a limit: every link shows how many belts (or pipes)
  its flow needs, as `flow / capacity` (e.g. `0.4 belts`), and warns when it needs more than one.
- Raw resources have a per-region availability setting (e.g. Wuling: 420 Cuprium Ore/min). Going over it
  is a warning, never a limit the solver uses.
- Game names are translatable through `data/<version>/i18n/<lang>.json`. UI strings go through Fluent,
  but only English (`en.ftl`) is written. Translations are left for contributors.

**Out of scope**

- Spatial layout of any kind: grid, footprints, belt routing, bridges, power poles, blueprints (ADR 0002).
- Bootstrapping. The planner models steady state only, and never warns that a seed loop needs starting seeds.
- Power as a limit (for example a power budget).
- Cross-region planning in one graph.

**Audience and release**

- The goal is a public tool. The repo stays private (or at least unannounced) until Max is happy with it.
- Targets: macOS and Windows. Linux may build but isn't supported. A wasm web build (M9) runs in the
  browser from any static site.
- Until the repo goes public there are no CI binaries; everything is built from source. After that,
  releases are unsigned binaries on GitHub Releases, with README steps for Gatekeeper and SmartScreen.
- Code license: MIT OR Apache-2.0. Game data and icons belong to Hypergryph/Gryphline and are committed to
  the repo (as factoriolab does), with a disclaimer in `../data/README.md` saying they are not under the
  code's license.

---

## 2. Ground rules (mirrored in `CLAUDE.md`)

1. **The UI never does math.** All solving lives in `aic-core`, which has no egui dependency. The UI
   builds a `Graph`, calls `solve(&catalog, &graph)`, and renders the `Solution`.
2. **Internal IDs are game IDs** (`copper_nugget`, `furnance_1`, `mix_pool_1`, …), never display names.
   Display names come from the i18n table at render time.
3. **All rates are items per minute** inside the app. Convert once, in the data layer.
4. **Data loading goes through our own schema.** `data/<version>/data.json` uses the factoriolab
   format for now, but only `aic-data/src/raw.rs` knows that. Everything else uses `Catalog`.
5. **Data is copied in by hand, not fetched.** No submodules, no clone scripts, no third-party network
   access at build or run time. (The future web build loads `data/` from its own site; that is not a
   third-party fetch.)
6. **Use the glossary's terms in code.** `Facility`, not `Machine`. `Link`, not `Edge`. `Region`, not
   `Location`. Only `raw.rs` uses factoriolab's names (`machine`, `locations`, `producers`).
7. **Every user-facing UI string goes through Fluent.** No string literals in widgets.
8. Every milestone ends with `cargo test --workspace` and `cargo clippy --workspace -- -D warnings`
   passing.

---

## 3. Facts about the source data (verified 2026-11-08)

### Data folder layout

```
data/
├─ README.md                  # layout, and the license disclaimer for game data
└─ <version>/                 # e.g. v1.5.3, named after the game version
   ├─ data.json               # required: items, facilities, recipes, icon positions, regions
   ├─ i18n/
   │  └─ en.json              # required: English names keyed by the same IDs
   ├─ images/                 # optional: PNG icons named by our IDs (M5)
   │  ├─ items/<item id>.png
   │  ├─ facilities/<facility id>.png
   │  └─ nodes/source.png, sink.png   # built-in node kinds (the depot unloader and loader)
   ├─ icons.webp              # optional, unused: factoriolab's sprite sheet
   ├─ overlay.json            # optional (M7): our own patches on top of data.json
   └─ SOURCE.md               # optional: where it came from, commit/date, any hand edits
```

- The repo has the data at `data/v1.5.3/`. M0 moved it there from `src/data/1.5.3/`. Its `images/` came
  from [jambochen/endfield-calc](https://github.com/jambochen/endfield-calc), renamed to our IDs; see
  `data/v1.5.3/SOURCE.md`.
- Max copies these files in by hand. Initial source: `endfield-calc/factoriolab`, `src/data/aef/`
  (`data.json`, `i18n/en.json`, `icons.webp`). In that repo, ignore `data-dsp.json`, `data-sfy.json`,
  `data-spa.json` and `defaults-spa.json`. Those are Dyson Sphere Program / Satisfactory / Factorio test
  fixtures from upstream, not Endfield.
- **Version selection:** on native builds, the app scans `data/` for subfolders that contain `data.json`,
  picks the highest version by default (semver-ish parsing on the `vX.Y.Z` name, falling back to string
  order), and lets you switch in Settings. The folder name is the version string. If it doesn't match
  `data.json → version`, log it.
- **Locating `data/` (native only):** the `--data <path>` CLI flag, then `AIC_DATA_DIR`, then `./data` next
  to the executable, then `./data` in the working directory. Show a clear error screen if none is found.
  Don't `include_bytes!` the data, so a new version can be dropped in without rebuilding.
- **Parsing works from bytes.** `aic-data` parses `Catalog` from byte slices. Folder discovery is a thin
  native-only layer on top, so the web build can fetch the same files over HTTP.
- **Saved graphs record the data version they were made with.** On load, if that version isn't the
  active one, show a banner and still load by ID (see M2).

### Facts about the first data set (factoriolab `aef`, verified 2026-11-08)

`data.json` keys:

| Key | Use |
|---|---|
| `items` / `recipes` / `locations` / `categories` | game data, below |
| `icons[]` | `{ id, position: "-64px 0px", color }` into `icons.webp` (1024×1024, 64×64 cells) |
| `version` | `{ "arknights-endfield": "v1.5.3" }` |

`i18n/en.json` has keys `name`, `version`, `categories`, `items` (161), `recipes` (257) and `locations`,
each a map from ID to English name.

This data set is **v1.5.3**, and that repo's last commit was 2026-09-07. The game is already at v1.6, so
expect a few missing or changed recipes. A `data/v1.6/` folder, from another source or with an overlay,
is the fix.

Shape:

- `items[]`: 161 entries. Keys: `id, name (zh), category, row, icon, stack`, plus optional
  `machine`, `belt`, `pipe`, `buildIcon`.
  - 34 items have `machine: { speed, type, usage, size?, locations?, hideRate?, totalRecipe? }`. These are
    facilities. `usage` is power draw. `size` is the footprint and is ignored (ADR 0002).
  - `belt` item: `belt.speed = 0.5` (items/sec, so **30/min**). `pipe` item: `pipe.speed = 2` (**120/min**).
    Confirm both against the game.
- `recipes[]`: 257 entries, IDs unique. Keys: `id, name, category, row, icon, time (sec), in {item: qty},
  out {item: qty}, producers [facility ids]`, plus optional `flags, usage, cost, locations`.
  - `producers` has 1 entry for 247 recipes and 2 for 10 (e.g. `mix_pool_1` / `mix_pool_2`).
    A recipe node picks a recipe and a facility, defaulting to the first.
  - `flags`: `mining` (9 recipes, empty `in`, e.g. `originium_ore`, `liquid_water` on `pump_1`),
    `hideProducer` (28 outpost "recipes" like `tundra_coupon-iron_cmpt`).
  - **Byproducts exist:** 6 recipes have more than one output, e.g. `copper_nugget`:
    `{copper_ore:1, liquid_water:1} → {copper_nugget:1, liquid_sewage:1}`.
  - **Recipe-level `usage` overrides facility usage.** Negative means power *generation*:
    `power_proc_battery_1..5` consume a battery over 40 s and have `usage` −220 … −3200, with empty `out`.
  - **Cycles exist:** seed ↔ plant loops (`plant_grass_2` / `plant_grass_seed_2` and friends, 79 items
    with more than one recipe), and `phase_trans_*` recipes with the same item on both sides. The LP
    handles these as ordinary flow equations.
  - Odd times worth a test: `sewage-treat-export` (`liquid_sewage: 360 → liquid_xiranite_poly: 12`), some 3600 s recipes.
- `locations`: `tundra` (Valley IV) and `jinlong` (Wuling). In our code these are **regions**.
- **Hidden from the recipe browser:** `hideProducer` outpost recipes (until M7's outpost order), and the
  `__domain_transfer` facility ("Metastorage Transfer", cross-region transfer).
- **Not in the data:** splitters, mergers, storage and control ports. These are built-in node kinds in
  `aic-core`, with app-drawn icons, no facility, no recipe and no power.
- To check: `miner_2` and `miner_3` both have `speed: 1` and the flag `totalRecipe`. Check in-game whether
  the higher-tier miner is faster, and add a per-facility speed override in the overlay if so.

Rate formula (per facility, per minute): `qty * 60 / time * facility.speed`.

---

## 4. Workspace layout

```
endfield-aic-modeler/
├─ Cargo.toml                 # [workspace]
├─ CLAUDE.md
├─ GLOSSARY.md
├─ LICENSE-MIT, LICENSE-APACHE
├─ docs/
│  ├─ PLAN.md
│  ├─ adr/
│  └─ ref/                    # reference screenshots used by golden tests
├─ data/
│  ├─ README.md
│  └─ v1.5.3/
└─ crates/
   ├─ aic-data/               # parse + normalize -> Catalog; native discovery. deps: serde, serde_json, thiserror, indexmap
   ├─ aic-core/               # Graph model, validation, sections, solver. deps: aic-data, good_lp, serde, slotmap
   ├─ aic-editor/             # App logic, no GUI dependency: document, session, selection, commands, files, Fluent strings, style tokens, built-in data (ADR 0010)
   ├─ aic-app/                # eframe + egui-snarl UI. deps: aic-editor, eframe, egui-snarl, egui_dock, rfd
   └─ aic-extract/            # LATER (M8): CLI that converts other sources into data/<version>/
```

`aic-data` should also expose a writer for its normalized format (`Catalog` → JSON). The extractor CLI
will need it, and it makes the internal format easy to inspect.

Crate versions (crates.io, checked 2026-11-08):

- `egui-snarl = "0.12"` requires `egui ^0.36`, so **pin `eframe = "0.36"`** even though 0.37 is out.
  Upgrade only when egui-snarl does.
- `good_lp = { version = "1.15", default-features = false, features = ["microlp"] }`. Add a `highs`
  feature later only if microlp is slow or unstable on big graphs (`highs` won't build for wasm).
- `serde`/`serde_json` 1.x, `slotmap`, `indexmap`, `thiserror`, `image` with `webp`, `rfd`, `fluent-bundle`.

---

## 5. Solver semantics

This is the heart of the app. ADR 0003 has the reasoning; this section has the rules.

### Limits

Every value the player types is an **upper limit**:

| On | Limit | Required? |
|---|---|---|
| Recipe node | facility count | optional |
| Source node | supply per minute | **required** (an uncapped supply is just an unconnected port) |
| Sink node | consumption per minute | optional |
| Control port | flow per minute | required |

Links have no settings. They carry whatever flow the solver gives them.

### Flow rules

- **Linked input port:** the links must fully feed it. `Σ link flow = count · rate_in`.
- **Linked output port:** the links must carry away everything. `Σ link flow = count · rate_out`. There is
  no surplus variable. A backed-up output **throttles** its facility, as the in-game stall does.
- **Unconnected port:** depot I/O, unlimited (ADR 0001). `depot flow = count · rate`.
- **Splitter, merger:** `Σ in = Σ out`. One item, any number of links. An unconnected side is depot.
- **Control port:** `in = out ≤ limit`. One item. Any number can be chained. An unconnected side is depot.
- **Storage:** `balance = Σ in − Σ out`, free (positive or negative), no modes. One item. Pass-through is
  `min(in, out)`. Demand downstream does not pass upstream. Its sides are never depot: an unconnected
  side carries 0.
- **Ratio lock (M7):** a fixed share for each link of a fan-out or fan-in.
- Logistics nodes (splitter, merger, storage, control port) take their item from the first link and show
  "?" until then. A link with a different item is refused. Solid ↔ fluid links are refused.

### Sections

Cut the graph at every storage node (storage belongs to both sides). Each connected piece is a
**section**, solved as its own LP. A section that fails (unbounded, or no driver) goes blank and gets a
warning, while every other section still shows numbers. This also keeps each LP small.

### Two passes per section

1. **Maximize throughput.** Run everything as high as limits and supply allow, with no throttling that
   could be avoided. Throughput is the output of every **origin** plus the consumption of every sink
   (ADR 0004). An origin is a node nothing in the graph feeds: a source, a recipe node with no linked
   input (measured by its inputs if it has no outputs), a storage's output side, or a splitter, merger
   or control port pulling from the depot. A section with neither is a closed loop and maximizes the
   sum of node activity instead.
2. **Split evenly among equally good answers.** Hold the pass-1 optimum (relative tolerance 1e-9), then
   minimize, over every fan group (the links sharing one port), its largest link minus its mean link.
   This fills links like water: a branch that can't take an equal share takes what it can, and the
   rest split evenly. Ratio locks (M7) are hard constraints and override this.

Decided in M3 against the golden tests. Rejected: the sum of facility counts (fails test 4) and
lexicographic max-min over node utilization (equalizes the consumers' counts in test 4 instead of
their links). See ADR 0004.

### Undetermined

If a section's LP is unbounded (nothing limits it), every value in that section is **undetermined**:
blank, never 0 or ∞. This covers "no driver at all" and "a sink fed by draining storage with no limit".
A section with no limits at all is caught structurally, before solving.

Since every limit is an upper bound and 0 always satisfies them, an infeasible result should never
happen. Keep `Infeasible` as a status for numerical failure only, and report it as a bug.

### Throttle reasons

When a node runs below its limit, the solution says why: the binding limit or port that holds it back
(for example "input `iron_ingot` from Refining Unit A, which is at its limit 1"). `microlp` exposes no duals, so the
solver asks the LP: for each linked port of the throttled node, would the node run higher with that port
on the depot? The port that frees it most is the reason (or, if none does, a port it shares with a fan
group). The limit behind it is the nearest node at its limit, following links upstream from an input or
downstream from an output, stopping at storage. This costs one LP per linked port of each throttled
node; measure it in M4. On the canvas, mark the binding port. The inspector shows the full reason.

### Solution

```rust
pub struct Solution {
    pub sections: Vec<SectionResult>,           // status per section: Solved | Undetermined | Failed
    pub facility_count: HashMap<NodeId, Option<f64>>,   // None = undetermined
    pub rate: HashMap<NodeId, Option<f64>>,     // source supply, sink consumption, logistics flow
    pub link_flow: HashMap<LinkId, Option<f64>>,
    pub link_belts: HashMap<LinkId, Option<f64>>, // M4: flow / belt or pipe capacity, e.g. 0.4
    pub port_flow: HashMap<Port, Option<f64>>,  // M4: flow through every port, linked or not
    pub depot: HashMap<Port, Option<f64>>,      // Port { node, item, side }: Input = pulled, Output = sent
    pub storage: HashMap<NodeId, StorageFlow>,  // inflow, outflow, balance, pass-through
    pub throttle: HashMap<NodeId, ThrottleReason>,
    pub totals: Totals,      // depot inputs, depot outputs, storage balances, power draw/generation/net,
                             // raw resource use (M4),
                             // facility counts per facility type
    pub warnings: Vec<Warning>,
}
```

Warnings:

- `Undetermined { section, reason }`: no driver, or unbounded. Say what to add (a source limit, a
  facility count limit, a sink limit or a control port).
- `PortUnlinked { port, outside }` (ADR 0007): a workspace port with an end that has no link.
- `ResourceOverUse { item, used, available }` (M4): from the resource check, not from `solve` itself.
- `BeltOverCapacity { link, lanes }`: flow above 30/min (solid) or 120/min (fluid). Shown as a hint, not
  an error.
- Load-time: data version mismatch, and missing recipe or item IDs.

Power: `Σ count · (recipe.power_override_kw.unwrap_or(facility.power_kw))`, reported as draw and
generation separately.

### Display

- Values are stored at full precision. The canvas shows 2 decimals, rounded to nearest (`1.125` → `1.13`,
  `1/3` → `0.33`). The inspector shows more. Other planners round differently (factoriolab rounds up to
  `0.4`, jambochen's calculator shows `0.3`), so golden tests compare full-precision values.
- A recipe node card shows actual count on top and its limit below (blank if none), like Satisfactory Modeler.
- The round-up view (M6) shows `ceil(count)` and the utilization per facility, e.g. `1.75 → 2 × 88%`.

---

## 6. Milestones

### M0: Skeleton
- Convert the single crate into the workspace from section 4. Move `src/data/1.5.3/` to `data/v1.5.3/`.
- Add `../data/README.md` (layout and the game data disclaimer) and `LICENSE-MIT` / `LICENSE-APACHE`.
  `CLAUDE.md` already exists; update its paths if anything moves.
- Max copies `icons.webp` in by hand when needed. Claude Code should not try to download any data.
- `aic-app` opens an empty eframe window.
- **Done when:** `cargo run -p aic-app` shows a window, and `cargo test --workspace` passes.

### M1: `aic-data`, load the catalog
- `parse.rs`: `Catalog::from_bytes(data_json, i18n_json, icons: Option<&[u8]>) -> Result<Catalog>`.
- `discover.rs` (native only, `#[cfg(not(target_arch = "wasm32"))]`): `find_data_dir()` (resolution order
  from section 3), `list_versions(dir) -> Vec<DataVersion>` (folders with a `data.json`, newest first), and
  `load(version_dir, lang) -> Result<Catalog>`, which reads files and calls `from_bytes`. A missing
  `icons.webp` is not an error.
- Errors name the exact file and JSON path that failed (`serde_path_to_error` helps here).
- `raw.rs`: serde structs that mirror factoriolab exactly. Use `#[serde(default)]` for optional keys
  and `BTreeMap<String, f64>` for `in`/`out`.
- `catalog.rs`: the normalized model the rest of the app uses:

```rust
pub struct Catalog {
    pub items: IndexMap<ItemId, Item>,               // name_en, name_zh, category, icon, is_fluid
    pub facilities: IndexMap<FacilityId, Facility>,  // speed, power_kw, regions
    pub recipes: IndexMap<RecipeId, Recipe>,
    pub belt_per_min: f64,                           // 30
    pub pipe_per_min: f64,                           // 120
    pub icons: HashMap<String, IconRect>,            // pixel rect in icons.webp
    pub icons_webp: Option<Vec<u8>>,                 // raw bytes of icons.webp if present
    pub version: String,                             // folder name, e.g. "v1.5.3"
}
pub struct Recipe {
    pub id: RecipeId, pub name_en: String, pub time_s: f64,
    pub inputs: Vec<(ItemId, f64)>, pub outputs: Vec<(ItemId, f64)>, // qty per craft
    pub facilities: Vec<FacilityId>,
    pub power_override_kw: Option<f64>,       // recipe.usage (negative = generation)
    pub is_mining: bool,
    pub hidden: bool,                         // hideProducer outposts, __domain_transfer
    pub regions: Vec<RegionId>,
}
impl Recipe { pub fn rate_per_min(&self, qty: f64, speed: f64) -> f64 { qty * 60.0 / self.time_s * speed } }
```

- Item, facility, recipe and region IDs are newtypes around `String` (interned `u32` later if profiling says so).
- Fluid detection: the item is `liquid_*` or `gas_*`, or appears in a recipe that a pipe-only facility
  uses. Start with the prefix rule and adjust.
- Fall back to the zh name, then the id, when an English name is missing, and log it once.
- **Tests:** use small hand-written fixtures in `crates/aic-data/tests/fixtures/<version>/` (a few
  items and recipes, including a byproduct, a mining recipe, a power recipe and the Yazhen seed loop),
  so tests don't depend on the real data. Add one `#[ignore]`d test that loads the real `data/v1.5.3/`
  and checks: 161 items, 257 recipes, 34 facilities. `copper_nugget` has outputs `copper_nugget` and
  `liquid_sewage`. `power_proc_battery_1` power override is −220. `belt_per_min == 30`. Every recipe
  facility exists. Every item referenced by a recipe exists.

### M2: `aic-core` graph model
```rust
pub struct Graph {
    pub region: RegionId,
    pub nodes: SlotMap<NodeId, Node>,
    pub links: SlotMap<LinkId, Link>,
}
pub enum NodeKind {
    Recipe      { recipe: RecipeId, facility: FacilityId, limit: Option<f64> }, // facility count
    Source      { item: ItemId, limit: f64 },
    Sink        { item: ItemId, limit: Option<f64> },
    Splitter    { item: Option<ItemId> },   // None = "?" until the first link
    Merger      { item: Option<ItemId> },
    Storage     { item: Option<ItemId> },
    ControlPort { item: Option<ItemId>, limit: f64 },
    MissingRecipe { recipe: RecipeId },     // loaded from a save whose recipe no longer exists
}
pub struct Link { pub from: (NodeId, ItemId), pub to: (NodeId, ItemId) }
```
- Recipe nodes have one port per recipe item. Source has one output, sink one input. Logistics nodes have
  one item and accept any number of links on each side.
- `validate()` rejects: links whose item doesn't exist on both ends, a node linked to itself, duplicate
  links between the same two ports, solid ↔ fluid links, a second item on a logistics node, and recipes
  or facilities outside the graph's region.
- Save/load: pretty-printed JSON, extension `.aicm`, with `format_version`, `data_version`, `region`,
  nodes (with canvas positions), links, and later workspaces. IDs only, so files survive data updates.
  Unknown recipe IDs on load become `MissingRecipe` nodes rather than an error.
- **Tests:** round-trip save/load, and validation cases.

### M3: Solver
Implement section 5. Rebuild the LPs on every change, since graphs are small.

**Golden tests (built in code, full-precision values, tolerance 1e-6 unless noted):**

1. **Single recipe:** sink limited to 10. Count = 10 / rate.
2. **Byproduct to depot:** `copper_nugget` with sewage unconnected. Sewage shows as depot output, no warning.
3. **Byproduct throttles:** the same, with sewage linked to a sink limited below the sewage output. The
   copper node is throttled, and the throttle reason names the sewage port.
4. **Fan-out evens out:** A makes 30/min (count limit). It fans out to B (20/min input per facility) and C
   (10/min per facility), both limited to 5, outputs to depot. Expect 15 / 15, B = 0.75, C = 1.5.
5. **Seed loop** (v1.5.3 recipes `plant_grass_2`, `plant_grass_seed_2`, `liquid_water`): Yazhen sink
   limited to 10. Expect Planting Unit 1/3, Seed-Picking Unit 1/3, Fluid Pump 1/6. Yazhen 20/min, split
   10 to the sink and 10 back to seed picking. Seeds 10, water 10. References:
   `docs/ref/Screenshot-golden-test-planting*.png` (those tools round 0.333 to 0.4 and 0.3).
6. **Limited source, open end:** source limited to 60/min → three-step chain → sink with no limit. The
   final rate matches hand math.
7. **Satisfactory reference** (fixture catalog of Satisfactory recipes; the solver doesn't care which
   game): source limited to 60 iron ore → Smelter (1 ore → 1 ingot, 2 s) → fan-out to Iron Plate
   (3 ingot → 2 plate, 6 s) and Iron Rod (1 → 1, 4 s). Rods fan out to Screw (1 rod → 4 screws, 6 s) and
   Modular Frame. Reinforced Iron Plate (6 plate + 12 screw → 1, 12 s). Modular Frame (3 reinforced plate +
   12 rod → 2, 60 s), output to depot. No other limits. Expect counts smelter 2, plate 1.125, rod 1.75,
   screw 1.125, reinforced plate 0.75, frame 1.25. Expect flows 60, 33.75 / 26.25, 22.5, 11.25 / 15, 45,
   3.75, and 2.5 frames/min out. The split is uneven because the recipe ratios force it. References:
   `docs/ref/Screenshot-golden-test-7.png` (Satisfactory Modeler), `…-7-v2.png` (SatisfactoryTools).
8. **Undetermined:** storage → recipe → storage, no limits anywhere. Every value is `None`, plus an
   `Undetermined` warning. The same for a lone recipe node with all ports on the depot and no limit.
9. **Storage decouples:** two refiners limited to 1 each feed a storage. A downstream node limited to 1
   pulls less than they make. Balance is positive and equals the difference. Upstream is not throttled.
10. **Lone node runs at its limit:** recipe node limited to 5, all ports on the depot. Count = 5.
11. **Control ports in series:** 40 and 25 on one line. Flow = 25.
12. **Fan-in evens out:** two feeders with spare capacity, and a consumer that takes less than both
    could give. Each feeder supplies half.
13. **Sections are independent:** one undetermined section next to one solved section. The solved one
    keeps its numbers.

### M4: egui shell + node canvas
- **Start with a performance spike, before any real UI.** It's throwaway code: an egui-snarl canvas with
  300 nodes, 600 links and 5 ports per node, each port and link showing a live rate label. Measure frame
  time while panning, zooming, dragging a node and creating a link, **on the Windows PC** (the slower
  machine, and where most players are). Use a release build.
  - **Pass:** frames stay under about 8 ms, which leaves room for 120 Hz.
  - **Fail:** profile and fix it inside egui (skip off-screen nodes, cache text layout, fork egui-snarl if
    needed). Only a failure measured after that reopens the framework question, and then the comparison
    is the same spike in iced + iced_nodegraph.
  - When it passes, record ADR 0005: egui + egui-snarl is the shipping UI. iced + iced_nodegraph was
    considered and rejected because the node editor crate is young with a single maintainer, and the
    graph sizes don't need it.
  - The spike is `crates/aic-app/examples/perf_spike.rs`
    (`cargo run --release -p aic-app --example perf_spike`). It drives itself through idle, pan, zoom,
    node drag, new-link drag and live-value phases with vsync off, prints frame times, and exits.
    Measured 2026-11-09 on an M1 Pro (Metal), all 300 nodes on screen (the worst case): mean frame
    5.2–8.1 ms, p95 5.7–8.8 ms; about 4.5 ms of each frame is building the UI. Skipping the widgets of
    off-screen nodes saved only about 0.5 ms, because egui-snarl still lays out every node frame and
    pin. **The Windows run is still to do**, and ADR 0005 waits for it.
- Three areas: left **recipe browser** (search by English name, filter by category; only the graph's
  region; hidden recipes left out), center **`egui-snarl` canvas**, right **inspector** (selected node or
  link, its throttle reason, plus totals). Both side panels collapse from toggles in the top bar.
- Fluent setup: `crates/aic-editor/i18n/en.ftl` embedded with `include_str!`. Every UI string goes through it.
- `SnarlViewer`:
  - Recipe node: icon, recipe name, facility picker (when there is more than one facility), actual count
    on top, limit below. On the card the facility picker is one button that steps to the next facility;
    the inspector has the full list. The limit below the count is a field typed straight into the card
    (blank: no limit), like Satisfactory Modeler; sources, sinks and control ports get the same field.
  - **Port menu:** a double-click on a port opens a small menu beside it: the node's limit typed as a
    rate through that port (`Limit [225] /min`), and "Clear connections". On a recipe node the rate is
    converted to a facility count limit in `aic-core` (`Graph::set_port_limit`), so the graph keeps one
    limit per node; it keeps that count if the facility changes later.
  - Ports: one per item, with icon and rate. Unconnected ports are colored by depot direction:
    **pink = pulled from the depot, green = sent to the depot**, with the rate beside them. A small marker
    on the port that throttles the node.
  - Storage: balance in the header (green +, red −), pass-through below.
  - Splitter, merger, control port: compact nodes. Control port shows its limit.
  - Undetermined values are blank.
  - `connect()` refuses mismatched items and solid ↔ fluid, and sets the item of an unset logistics node.
- **Node picker.** One popup for every way of adding a node, modeled on Satisfactory Modeler's recipe
  selector (`docs/ref/satisfactory-modeler-recipe-selector.png`). It must be fast: one click adds the
  node. No dropdowns and no second step.
  - Opens on right-click on the canvas, and when a link is dragged from a port and dropped on empty
    canvas (egui-snarl's dropped-wire menu). Right-click on an unconnected port opens it too.
  - Layout: a search box at the top, focused on open, so typing filters at once. Left column: the
    built-in nodes (source, sink, splitter, merger, storage, control port; a source or sink needs an
    item, so those two appear only when opened from a port). Right: the recipe list, one
    row per recipe, with input icons on the left, the name in the middle and output icons on the right.
    Toggles choose whether the search matches recipe names, inputs, outputs, or any mix.
  - Only recipes of the graph's region; hidden recipes left out.
  - **From an input port of item X ("Add producer of X"):** the list shows only recipes that output X,
    including mining recipes, and the left column offers a Source of X. Every recipe is listed the same
    way, in data order, all visible at once; no expanding. (Satisfactory Modeler highlights alternative
    recipes. The data has no main/alternative flag, so that waits until we know how to tell them apart.)
  - **From an output port of item X ("Add consumer of X"):** the mirror case. Only recipes that take X
    as input, and the left column offers a Sink of X.
  - One click adds the node and, when opened from a port, links it to that port. The new node gets no
    limit, so the solver sizes it from the rest of the graph. Its facility is the recipe's first one;
    the node card's facility picker changes it later. Logistics nodes added from a port take its item.
    A source or control port needs a limit, so a new one starts at one belt (or pipe) of its item.
  - Placement: at the drop point (or the right-click point). When opened from a port, the new node's
    side that faces the original node lands on that point, so the link doesn't cross the new node.
  - A plain add from the canvas also gets no limit; the lone node shows blank with the "no limit"
    warning until the player sets one.
- **Link labels:** flow and belt count at the midpoint, like `12.00 /min · 0.4 belts`
  (`docs/ref/jambochen-mining-node.png`), using "pipes" for fluids. Over one belt, the label shows the
  warning colour. `aic-core` computes the belt count; the UI only formats it.
- **Raw resource availability.** A per-region table of raw resources (the items any mining recipe in that
  region outputs) with how much the region can supply per minute, like jambochen's settings
  (`docs/ref/jambochen-setting-resource.png`). A blank entry is unlimited. It's an app-wide setting kept
  in eframe storage, not part of a graph, and is edited from a settings window with a tab per region.
  The data set has no mining caps, so every entry starts blank until the player fills it in (or an
  `overlay.json` provides defaults, M7).
  - `aic-core` adds up each raw resource's use in a solved graph: the output of mining recipe nodes,
    the supply of source nodes, and depot pulls of that item. A separate function compares that with the
    availability table and returns `ResourceOverUse { item, used, available }` warnings, so `solve`
    keeps its `(catalog, graph)` signature. The inspector's totals show use against availability, and
    the warning appears like the others.
- Re-solve on every graph change. Synchronous is fine at first. Measure, and move to a worker thread
  only if a solve takes more than about 16 ms. The status bar says so when a solve takes longer.
- Snarl is only the view and `aic-core::Graph` is the source of truth. Store the `NodeId` in the snarl
  node payload.
- **Done when:** you can rebuild test 7's graph (with fixture data loaded) and test 5's graph by hand and
  see the same numbers, adding every upstream node through the node picker from an input port, one
  click each.

### M5: Icons
Done with per-file PNGs in `data/<version>/images/` instead of factoriolab's `icons.webp` sprite sheet,
since the PNGs (from jambochen/endfield-calc) cover more of the game and are named by ID.
- `aic-data` reads `images/{items,facilities,nodes}/*.png` as bytes (`Catalog::image`), never decoding
  them; `Catalog::add_image` lets the web build add them one by one. `Catalog::recipe_image` picks a
  recipe's picture: its own icon, else its first output, else its first input, else its facility. The
  data's recipe icons are mostly generated IDs with no image, so this usually lands on the main output.
- The app decodes each PNG with `image` the first time it is drawn and keeps the texture.
- Where icons show: port rows, the facility picture on recipe cards, the depot unloader and loader on
  source and sink cards, picker rows and its Source/Sink buttons, the recipe browser, the inspector and
  the raw resources window.
- An item with no image is drawn as a colored square with its initials (the icon `color` from
  `data.json` when there is one), so the app works without images.
- Splitter, merger, storage and control port have no picture yet.

### M6: Usability
Done.
- File menu: New, Open, Open Recent, Save, Save As (`rfd`). The region belongs to the graph and is saved
  in it; the top bar's region picker changes it at any time (an undo step), and what the new region
  lacks shows as warnings on the cards.
- Autosave (changed after M6/M7): every document is saved somewhere, at most 5 s after a change. The
  throttle fires on both edges, like VueUse's `useThrottleFn`: the first change after a quiet spell saves
  at once, then at most one save per 5 s, and the last change always lands. It waits while the pointer
  is down or a field has focus, so saves don't split a drag or typing, and saving never ends an undo
  step. States:
  - *Draft*: a new graph's first change creates `untitled-<time>.aicm` in the app's data folder
    (`eframe::storage_dir("AIC Modeler")/drafts`, like Excel's AutoRecover) and autosaves there. An
    untouched new graph has no file, and an empty draft is deleted when left.
  - *File*: an opened file, or one picked with Save As, autosaves into that file. Save As from a draft
    moves the work there and deletes the draft. Cmd/Ctrl+S on a draft is Save As.
  - New, Open and closing the window save first, so there is no "discard changes?" dialog.
  - On start the app reopens the last document (eframe storage key `last-document`). A missing file, or
    one that doesn't parse, is left alone: the app starts a new draft and says why in the status bar.
  - Writes go to `<file>.tmp`, then replace the file, so a crash can't leave half a file.
  - The file keeps node and group positions and the canvas view (`CanvasView`: the graph point at the
    canvas's top-left and the zoom). Panning or zooming is saved too, but isn't an undo step.
  - Pretty JSON stays: 200 nodes is about 60 KB and writes in well under a millisecond, and the files
    stay readable and diffable.
- Toolbar under the menus with a button for every keyboard command (file, undo/redo, copy, paste,
  duplicate, delete, select all, group, ungroup) and the round-up toggle; the tooltip names the key. A
  key press lights its button for 0.4 s, as Excel does, so the toolbar teaches the keys. Icons are
  symbols egui's built-in fonts draw; several common ones (📋, ⧉, ●) are missing, so check new ones in a
  snapshot. The status bar's right end says whether the work is saved (Saved, Saved as a draft,
  Saving…, Not saved), with the file and time on hover.
- Undo/redo keeps whole `Graph` snapshots (200 steps). Edits until the pointer is up and no field has
  focus make one step, so a drag or a typed number undoes at once. Undo and redo rebuild the canvas from
  the graph.
- Selection: a click selects one node, Shift+click adds or removes one, Shift+drag on the background is
  egui-snarl's box selection, which becomes the selection. Dragging a selected node moves the whole
  selection. Delete/Backspace, Cmd/Ctrl+D (duplicate), Cmd/Ctrl+A, Cmd/Ctrl+C/X/V. Copied nodes go to the
  system clipboard as save-file JSON, so they paste into another window too; links leaving the copy are
  dropped (`Graph::copy_nodes`, `Graph::paste`).
- Totals panel: depot inputs, depot outputs, storage balances, power, raw resources, and facility counts
  per type with the rounded-up total (`Totals::facilities_rounded`: each node rounded up, then added).
- Round-up view (View menu or the top bar toggle): cards show `ceil(count)` facilities and how busy they
  are; the inspector always shows it. The rounding lives in `aic_core::RoundUp`.

### M7: Data updates & extras
Done, except where noted.
- `data/<version>/overlay.json`, merged into `data.json` before it is checked, so overlay entries get the
  same checks. Format: `{ "items": { "add": [...], "replace": [...], "remove": ["id"] }, "recipes": { same },
  "facility_speeds": { "<facility id>": <speed> } }`, with items and recipes written as in `data.json`.
  Adding an ID that exists, or replacing or removing one that doesn't, is an error that names
  `overlay.json` and the path; unknown keys are errors too. `Catalog::from_bytes_with_overlay` for the web.
- Settings (renamed from Raw resources): data version and game-name language (from `i18n/*.json`), kept in
  eframe storage. Switching keeps the open graph and its undo steps: recipe nodes are matched against the
  new data by ID (`Graph::rebind`), so a recipe the new data lacks shows as missing until switched back.
  A chosen version that is gone falls back to the newest, a language that is gone to English.
- **Workspaces** (ADR 0007, replacing groups and their copies mode): a workspace holds nodes and other
  workspaces (`Graph::workspace_from`, `add_workspace`, `dissolve`, `Member`), shown from outside as one
  card whose pins are its ports. A double-click, Open in its menu or the inspector shows only what is in
  it (`Document::set_level`), with Back and the path to the top over the canvas, and + Input / + Output
  buttons that add ports. Ports (`NodeKind::WorkspacePort`) are pinned to the canvas's left and right
  edges; a port with an end unlinked carries nothing and warns. Cmd/Ctrl+G makes a workspace from the
  selection, routing crossing links through new ports; Cmd/Ctrl+Shift+G dissolves one. The canvas picker
  offers an empty workspace. Cut, copy and paste carry workspaces with their contents; a workspace's menu
  also pastes into it. Save format 2; version 1 groups open as workspaces.
- **Right-drag selection:** dragging with the right button on the background draws a box and selects
  what it touches, as on a desktop; a right click without a drag still opens the picker.
- **Ratio locks:** each link has a share at each end (`Link::out_share`, `in_share`); links with a share at
  the same port keep their flows in proportion. Typed in the port menu, which lists the port's links when
  it has several; blank links stay free and share the rest evenly. A lock holds even when it costs
  throughput.
- **Every region's recipes:** the recipe browser and the node picker list the recipes of every
  region, not only the graph's. A recipe only some regions have shows their names as badges
  ("Wuling", "Valley IV") before its name there and on its card. A card whose recipe or facility the
  graph's region lacks shows a warning sign before its title, from the solver's region warnings.
  Hidden recipes (cross-region transfers) stay hidden.
- **Outpost orders:** `NodeKind::OutpostOrder`, a sink for a settlement trade (`Recipe::is_trade`: the
  `hideProducer` recipes). Offered in the picker from an output port and in the recipe browser. The
  coupons earned show on the card, in the inspector and in a Coupons total; they never go to the depot.
- Export the canvas as a PNG (File menu): a window screenshot cropped to the canvas, so it is what is on
  screen, not the whole graph.
- Performance run against the real app: `cargo run --release -p aic-app --example perf_app` (201 recipe
  nodes, 109 links, real data and icons, inspector and totals). On the M1 Pro on 2026-11-09 the UI took
  4.1 ms per frame idle, 4.1 ms panning and 6.3 ms with a solve every frame. The frame interval was
  about 16 ms, but the M4 spike measured the same 16 ms that day (5.0–5.7 ms UI), so the cap is outside
  the app (it measured 5–8 ms during M4). No regression in egui. Re-measure on Windows with both
  examples before writing ADR 0005.
  - 2026-11-15, same Mac. Idle: `PERF_IDLE=20` (vsync on, as in the app) draws no frames in 20 s
    and `ps` shows 0.0% CPU, minimized too (`PERF_MINIMIZE=1`); the released app idles at 0.0%. The
    one wake-up found was autosave with nowhere to save, every 5 s; fixed. Per frame, headless
    (`PERF_HEADLESS=1`, UI build and tessellation, 5 runs each): idle 3.78 → 3.55 ms mean, live
    values 6.01 → 5.86 ms, from integer rate formatting, borrowed icon lookups, port hints built
    only on hover and Fluent lookups once a frame. Windowed frames stay about 6 ms mean, 8 ms p95,
    within run-to-run noise. Culling the recipe browser's off-screen rows (about 5% of a frame)
    was tried and dropped: it hides them from AccessKit and moved the snapshots.

### M8: `aic-extract` CLI
- `aic-extract <source-kind> <input> --out data/<version>/` writes `data.json` (factoriolab format,
  for now) plus `i18n/*.json`, and appends to `SOURCE.md`.
- Source kinds to consider: factoriolab `aef` folder (a copy and check, useful as the reference
  implementation), warfarin.wiki facility pages (zh, already has v1.6), and endfield.wiki.gg (English).
- `aic-extract diff data/v1.5.3 data/v1.6` lists added, removed and changed recipes.
- Validate output by loading it with `aic-data` and running the same consistency checks as the M1 tests.

### M9: Web build
Done.
- `wasm32-unknown-unknown` build of `aic-app` with eframe's web runner, built by Trunk:
  `scripts/build-web.sh` writes `target/web/` (`--serve` serves a debug build with live reload).
  `crates/aic-app/index.html` and `Trunk.toml` hold the setup; Trunk uses the `wasm-bindgen-cli`
  installed with Cargo at the version in `Cargo.lock`, so the build downloads no tools of its own.
- Data comes from the same site: Trunk copies `data/` next to the page, and on `wasm32` `build.rs`
  writes the list of its files (paths only, no bytes). The app fetches them all at startup with a
  loading screen, puts them in an `aic_data::Bundle::from_owned`, and from there loads exactly as the
  desktop app loads its built-in data, version switching included.
- Files live in a `FileStore` (`store.rs`): the disk on the desktop, the browser's storage on the
  web (`BrowserFiles`, kept in eframe storage, which is the browser's local storage). Drafts, the
  last document and Open Recent work the same on both. In the browser, Save As asks for a name, keeps
  the file and downloads a copy; Save downloads it again; Open uploads a `.aicm` into the browser's
  storage and opens it; Export PNG downloads the image.
- Stays on the `microlp` backend; `aic-core` builds for `wasm32` unchanged. Times use `web-time`,
  since `std::time::Instant` panics in the browser.

### M10: Multiple views
Done, except detached windows not yet tried as real OS windows (the tests run them embedded). Brief
settled 2026-11-14 (ADR 0008). Built on egui_dock 0.21 (`views.rs`); the focused view's canvas and
level live in the editor and the document, the others wait parked and are swapped in to be drawn
(`Document::swap_level`), drawn afresh when the graph changed meanwhile. A tab egui_dock would float
in an egui window becomes a native window. The camera moves with its view's place on screen, so a
dragged tab or a resized split keeps what it showed. Shortcuts: Split right Cmd/Ctrl+\\, Split down
Cmd/Ctrl+Shift+\\, Duplicate view Cmd/Ctrl+Shift+D, Move to new window Cmd/Ctrl+Shift+N, Close view
Cmd/Ctrl+W. With one view there is no tab bar.
- The canvas area becomes a dock: a tree of splits, each a group of tabs. Every tab is a **view** of
  the one open graph with its own level (the top, or a workspace) and camera. The recipe browser and
  the inspector stay fixed side panels around the dock. The default layout is one tab, so nothing
  changes until the player splits.
- A tab is named after its level: the region at the top, the workspace's name inside one. Two tabs on
  the same level get a number ("Valley IV · 2").
- Dragging a tab onto a view's left, right, top or bottom edge splits it; onto its middle joins that
  tab group. A translucent accent fill shows where it lands. Splits nest without limit and their
  dividers drag.
- On the desktop, dropping a tab outside the window, or Move to new window in the tab's menu, opens it
  in its own OS window (an egui immediate viewport, on the UI thread). A detached window shows only
  the canvas, with its own tabs and splits; the menus, toolbar, side panels and status bar stay in the
  main window. Closing it closes its views, never the document. The web build has no new windows: the
  menu item is hidden and a drop outside the canvas does nothing.
- The tab menu has Split right, Split down, Duplicate view, Move to new window and Close, each with a
  shortcut (`edit.rs`). The last view can't be closed. Closing a view loses nothing, so it never asks.
- Opening a workspace (double-click, Open) changes the level of the view it was opened from;
  middle-click or Cmd/Ctrl+click opens it in a new tab beside it.
- One document, one solve, one undo history, one autosave and **one selection** for all views: a node
  selected in one view is highlighted in every view that shows it, the inspector shows it, and Cut,
  Copy, Paste, Delete and Make workspace act on it from any view. Each view keeps its own camera, pin
  centres, limit fields being typed and egui-snarl state; egui-snarl's id becomes view and level.
- The view last clicked has focus, marked by a 1px accent line under its tab. The status bar's zoom
  and the toolbar's view commands act on it.
- The layout, each view's level and camera, and detached windows' places are kept in eframe storage
  with the settings, not in the `.aicm` file. A file still saves one camera, the focused view's.
  Opening a file puts every view at the top, the focused one at the file's camera. A view whose
  workspace is dissolved or gone moves to the top.
- Tests: kittest snapshots of a split layout and of the drop preview; an edit in one view shows in the
  other the same frame; a shared selection; a dissolved workspace's view falls back to the top.
- Not in M10: several documents open at once, a selection per view, dockable side panels, synced
  scrolling between views, minimaps.

---

## 7. Suggested Claude Code session order

1. "Read docs/PLAN.md, GLOSSARY.md and docs/adr/. Do M0 and M1. Don't fetch any data. Write the M1 tests
   with small fixtures first."
2. "Do M2."
3. "Do M3. Write the golden tests first, settle the pass-1 objective against them, and make them pass."
4. "Do the M4 performance spike and report the numbers." Then "Do the rest of M4 using egui-snarl 0.12
   with eframe 0.36." Then M5 and M6, one at a time.
5. M7 features one per session: workspaces and ratio locks are the big ones.

Keep each milestone as its own commit or branch, so a bad UI experiment is easy to throw away.

## 8. Open questions to settle while building

- Exact belt and pipe throughput in-game (30 and 120 per minute assumed from the data).
- Whether higher-tier miners are faster (the data says both have speed 1).
