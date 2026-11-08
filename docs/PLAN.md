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

Tech:

- Language: Rust (stable, edition 2024)
- UI: `egui` via `eframe`, with `egui-snarl` for the node canvas. This is the shipping UI; there is no
  plan to move to another framework.
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
- Node kinds: recipe, source, sink, splitter, merger, storage, control port, and groups (M7).
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
- Targets: macOS and Windows. Linux may build but isn't supported. A wasm web build comes later (M9).
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
   ├─ icons.webp              # optional until M5: sprite sheet that data.json → icons[] points into
   ├─ overlay.json            # optional (M7): our own patches on top of data.json
   └─ SOURCE.md               # optional: where it came from, commit/date, any hand edits
```

- The repo has the data at `data/v1.5.3/` (no `icons.webp` yet). M0 moved it there from `src/data/1.5.3/`.
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
   ├─ aic-app/                # eframe + egui-snarl UI, Fluent strings. deps: aic-core, eframe, egui-snarl, image(webp), rfd, fluent-bundle
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
| Copies-mode group (M7) | copies | optional |

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
- `CopiesUndetermined { group }` (M7): nothing inside the group defines one copy.
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
  nodes (with canvas positions), links, and later groups. IDs only, so files survive data updates.
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
- Three areas: left **recipe browser** (search by English name, filter by category; only the graph's
  region; hidden recipes left out), center **`egui-snarl` canvas**, right **inspector** (selected node or
  link, its throttle reason, plus totals).
- Fluent setup: `crates/aic-app/i18n/en.ftl` embedded with `include_str!`. Every UI string goes through it.
- `SnarlViewer`:
  - Recipe node: icon, recipe name, facility picker (when there is more than one facility), actual count
    on top, limit below.
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
    built-in nodes (source, sink, splitter, merger, storage, control port). Right: the recipe list, one
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
  only if a solve takes more than about 16 ms.
- Snarl is only the view and `aic-core::Graph` is the source of truth. Store the `NodeId` in the snarl
  node payload.
- **Done when:** you can rebuild test 7's graph (with fixture data loaded) and test 5's graph by hand and
  see the same numbers, adding every upstream node through the node picker from an input port, one
  click each.

### M5: Icons
- If there are no icons, draw a colored square with the item's initials instead, so the app works without them.
- Load `icons.webp` once with `image`, upload it as one `egui::TextureHandle`, and draw each icon as an
  `egui::Image` with a UV rect from `IconRect`. Parse `"-64px 0px"` into `(64, 0)`, with cell size 64.
- Fall back to the `missing-icon` cell, and use the icon `color` as a tint for port dots.
- Icons for built-in logistics nodes are drawn by the app.

### M6: Usability
- File menu: New (asks for the region), Open, Save, Save As (`rfd`), plus a recent-files list kept in
  eframe storage.
- Undo/redo with a command stack of `Graph` snapshots. Graphs are small, so snapshot clones are fine.
- Box selection, delete, duplicate, Ctrl+C/V within the app.
- Totals panel: depot inputs, depot outputs, storage balances (a separate section), power
  (draw / generation / net), and facility counts per facility type (rounded up).
- Round-up view: `ceil(count)` facilities and the resulting utilization per facility.

### M7: Data updates & extras
- `data/<version>/overlay.json`: add, replace or remove recipes and items, and override facility speeds.
  Merge it at load time after `data.json`. Use it for small fixes; a whole new game version gets its own folder.
- Settings: switch data version and game-name language (from the `i18n/*.json` files present).
- **Groups, step 1 (folder):** collapse a selection into one node. Every unconnected port inside shows as
  an external port of the group; if it's also unconnected outside, it's depot I/O. Groups nest without
  limit. They are a view only: the solver sees the flat graph. Saved as a group tree next to nodes and links.
- **Groups, step 2 (copies):** a group can stand for N identical copies. Limits inside define one copy:
  solve the inside once on its own (its external ports treated as depot) to get per-copy port rates, then
  treat the group as a single recipe-like node with those rates and a copies count (optional limit).
  If one copy is undetermined, the copies count is undetermined, with `CopiesUndetermined`.
- **Ratio locks** on fan-outs and fan-ins.
- **Outpost order:** a sink kind for settlement trades. Items go in, coupons count in totals. Unhides the
  `hideProducer` recipes as outpost orders.
- Export the canvas as a PNG.
- Re-run the M4 performance spike against the real app at about 200 nodes, with icons, inspector and
  totals. Fix any regression inside egui.

### M8: `aic-extract` CLI
- `aic-extract <source-kind> <input> --out data/<version>/` writes `data.json` (factoriolab format,
  for now) plus `i18n/*.json`, and appends to `SOURCE.md`.
- Source kinds to consider: factoriolab `aef` folder (a copy and check, useful as the reference
  implementation), warfarin.wiki facility pages (zh, already has v1.6), and endfield.wiki.gg (English).
- `aic-extract diff data/v1.5.3 data/v1.6` lists added, removed and changed recipes.
- Validate output by loading it with `aic-data` and running the same consistency checks as the M1 tests.

### M9: Web build
- `wasm32-unknown-unknown` build of `aic-app` with eframe's web runner.
- Load `data/<version>/` from the same site with `fetch`, then `Catalog::from_bytes`.
- Save/open becomes download/upload. Recent files and settings use eframe storage.
- Must stay on the `microlp` backend.

---

## 7. Suggested Claude Code session order

1. "Read docs/PLAN.md, GLOSSARY.md and docs/adr/. Do M0 and M1. Don't fetch any data. Write the M1 tests
   with small fixtures first."
2. "Do M2."
3. "Do M3. Write the golden tests first, settle the pass-1 objective against them, and make them pass."
4. "Do the M4 performance spike and report the numbers." Then "Do the rest of M4 using egui-snarl 0.12
   with eframe 0.36." Then M5 and M6, one at a time.
5. M7 features one per session: groups (folder, then copies) and ratio locks are the big ones.

Keep each milestone as its own commit or branch, so a bad UI experiment is easy to throw away.

## 8. Open questions to settle while building

- Exact belt and pipe throughput in-game (30 and 120 per minute assumed from the data).
- Whether higher-tier miners are faster (the data says both have speed 1).
