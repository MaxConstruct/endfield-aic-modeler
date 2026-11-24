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
- [0011](adr/0011-canvas-on-our-own-copy-of-egui-graph.md): the canvas is drawn by `aic-node-graph`, our own copy of egui_graph, in place of egui-snarl (M11).
- [0013](adr/0013-activation-inputs-run-whole-facilities.md): a facility's activation input is needed in full by every running facility; where links feed it, pass 1 settles the running facilities as whole numbers with a search capped at 200 nodes.
- [0014](adr/0014-our-own-data-format-extracted-from-the-game.md): a data version is our own format, written by `aic-extract` from the game's tables, with aliases from an older version's IDs.

Tech:

- Language: Rust (stable, edition 2024)
- UI: `egui` via `eframe`, with `aic-node-graph`, our copy of egui_graph, for the node canvas
  ([ADR 0011](adr/0011-canvas-on-our-own-copy-of-egui-graph.md)). This is the shipping UI. Up
  to M10 the canvas was egui-snarl, and the milestones up to then say so. Its
  app logic lives in the GUI-free `aic-editor` crate. An iced frontend (iced 0.14 + iced_nodegraph)
  was tried beside it and dropped after its performance spike failed; see
  [ADR 0010](adr/0010-one-frontend-editor-crate-stays.md) and
  [`archive/TASK_GUI_ICED.md`](archive/TASK_GUI_ICED.md).
- Solver: linear programming via `good_lp` (pure-Rust `microlp` backend by default); pass 1 of a
  section with a link-fed activation input is mixed-integer, solved by microlp's branch and bound
  with a node limit (ADR 0013)
- Data: one file per version, `data/<version>.json`, in our own format (ADR 0014). `v1.6.0` and
  later are written by `aic-extract` from the tables of the game client, which the user exports and
  copies in by hand. The first set, `v1.5.3`, came from
  [endfield-calc/factoriolab](https://github.com/endfield-calc/factoriolab) `src/data/aef/` and was
  converted once; the app depends on neither.

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
- Code license: MIT. Game data and icons belong to Hypergryph/Gryphline and are committed to
  the repo (as factoriolab does), with a disclaimer in `../data/README.md` saying they are not under the
  code's license.

---

## 2. Ground rules (mirrored in `CLAUDE.md`)

1. **The UI never does math.** All solving lives in `aic-core`, which has no egui dependency. The UI
   builds a `Graph`, calls `solve(&catalog, &graph)`, and renders the `Solution`.
2. **Internal IDs are game IDs** (`copper_nugget`, `furnance_1`, `mix_pool_1`, …), never display names.
   Display names come from the i18n table at render time.
3. **All rates are items per minute** inside the app. Convert once, in the data layer.
4. **Data loading goes through our own schema.** `data/<version>.json` is our own format, defined
   only in `aic-data/src/file.rs` (ADR 0014); `aic-extract` writes it through those types.
   Everything else uses `Catalog`.
5. **Data is copied in by hand, not fetched.** No submodules, no clone scripts, no third-party network
   access at build or run time. (The web build's files come from its own site; that is not a
   third-party fetch.)
6. **Use the glossary's terms in code.** `Facility`, not `Machine`. `Link`, not `Edge`. `Region`, not
   `Location`. Only `aic-node-graph`, a general widget, keeps egui_graph's (edge, socket).
7. **Every user-facing UI string goes through Fluent.** No string literals in widgets.
8. Every milestone ends with `cargo test --workspace` and `cargo clippy --workspace -- -D warnings`
   passing.

---

## 3. Facts about the source data (verified 2026-11-08)

### Data folder layout

Since M13 (ADR 0012) a data version is one file, and the names and icons are the app's:

```
data/
├─ README.md                  # layout, the format's reference, sources, the license disclaimer
├─ <version>.json             # e.g. v1.6.0.json: items, facilities, recipes, regions (ADR 0014)
└─ <version>_overlay.json     # optional (M7): our own changes on top of <version>.json
crates/aic-editor/i18n/names/
└─ <lang>.json                # game names keyed by game ID, one table for every version
crates/aic-app/assets/icons/
├─ README.md                  # where the icons came from, and their license
├─ items/<item id>.png
├─ facilities/<facility id>.png
└─ nodes/source.png, sink.png # built-in node kinds (the depot unloader and loader)
```

- The repo has `data/v1.5.3.json` and `data/v1.6.0.json`. M0 moved v1.5.3 to `data/v1.5.3/` from
  `src/data/1.5.3/`, M13 to its own file, and M8 converted it to our format. The icons came from
  [jambochen/endfield-calc](https://github.com/jambochen/endfield-calc), renamed to our IDs; see
  `crates/aic-app/assets/icons/README.md`.
- Max copies these files in by hand. Since M8 a version is written by `aic-extract` from the game
  client's tables, exported by the user (`data/README.md` says how, and what it assumes). The
  initial source was `endfield-calc/factoriolab`, `src/data/aef/` (`data.json`, `i18n/en.json`).
- **Version selection:** the app takes every `*.json` in `data/` that is not a `*_overlay.json` as a
  version, picks the highest by default (semver-ish parsing on the `vX.Y.Z` name, falling back to
  string order), and lets you switch in Settings. The file's name is the version string. If it doesn't
  match the file's `version`, that is a load warning.
- **Locating `data/` (native only):** the `--data <path>` CLI flag, then `AIC_DATA_DIR`, then `./data` next
  to the executable, then `./data` in the working directory; else the data built into the program
  (`crates/aic-editor/build.rs`), which the web build always uses. A folder on disk brings versions
  only: the names and icons stay the app's.
- **Parsing works from bytes.** `aic-data` parses `Catalog` from byte slices and a names table, which
  every load takes from the caller. Folder discovery is a thin native-only layer on top.
- **Saved graphs record the data version they were made with.** On load, if that version isn't the
  active one, show a banner and still load by ID (see M2).

### Facts about the game's tables (v1.6.0, verified 2026-11-25)

The extractor's sources, each a JSON object keyed by ID, with text as an ID into
`I18nTextTable_<LANG>.json` (M8 lists them in full):

- Crafts: `FactoryMachineCraftTable` (342 rows; `time = progressRound × msPerRound` of its
  formula group), on 18 buildings. The same craft on both mixing pools is two rows; the data
  merges them into one recipe with two facilities (11 such pairs).
- Items: `FactoryItemTable` (`phaseType` 1 solid, 2 liquid, 4 gas; `showInHubDomainIds`, which
  gives a recipe's regions), `ItemTable` (name, `showingType`, which gives its category).
- Buildings: `FactoryBuildingTable` (power, regions), `FactoryBuildingItemTable` (the item that
  places each, which gives our facility IDs), `FactoryTransmuterTable` (activation inputs:
  6/min each), `FactoryVaporizerTable` (the Gas Dispersing Unit's four gases), the miner, pump,
  power station, fluid-consume and sewage tables, `SettlementBasicDataTable` (outpost prices),
  `DomainDataTable` (regions and their coupons), `FactoryGridBeltTable` and
  `FactoryLiquidPipeTable` (one item per 2000 ms on a belt: **30/min**; one unit per 500 ms in a
  pipe: **120/min**).
- All four mining rigs and the gas extractor give 1 per 3 s; pumps 1 per s.

### Facts about the first data set (factoriolab `aef`, verified 2026-11-08)

History: this is the data v1.5.3 came from. M8 converted it to our own format (ADR 0014) and
dropped factoriolab's format from the app; what follows describes the file as it was copied.

`data.json` (now `data/v1.5.3.json`) keys:

| Key | Use |
|---|---|
| `items` / `recipes` / `locations` / `categories` | game data, below |
| `icons[]` | `{ id, position: "-64px 0px", color }` into `icons.webp` (1024×1024, 64×64 cells); only the colour is used |
| `version` | `{ "arknights-endfield": "v1.5.3" }` |

`i18n/en.json` (now `crates/aic-editor/i18n/names/en.json`) has keys `name`, `version`, `categories`,
`items` (161), `recipes` (257) and `locations`, each a map from ID to English name.

This data set is **v1.5.3**, and that repo's last commit was 2026-09-07. The game was already at
v1.6; M8 added `data/v1.6.0.json` from the game's own tables.

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
  (Answered by the game's tables in M8: every rig gives 1 per 3 s.)

Rate formula (per facility, per minute): `qty * 60 / time * facility.speed`.

---

## 4. Workspace layout

```
endfield-aic-modeler/
├─ Cargo.toml                 # [workspace]
├─ CLAUDE.md
├─ GLOSSARY.md
├─ LICENSE-MIT
├─ docs/
│  ├─ PLAN.md
│  ├─ adr/
│  └─ ref/                    # reference screenshots used by golden tests
├─ data/
│  ├─ README.md
│  ├─ v1.5.3.json             # a data version (ADR 0012, 0014); its names and icons are in the crates
│  └─ v1.6.0.json             # the newest, written by aic-extract from the game's tables
└─ crates/
   ├─ aic-data/               # the data file format (file.rs) and its loader -> Catalog; native discovery. deps: serde, serde_json, serde_path_to_error, thiserror, indexmap
   ├─ aic-core/               # Graph model, validation, sections, solver. deps: aic-data, good_lp, serde, slotmap
   ├─ aic-editor/             # App logic, no GUI dependency: document, session, selection, commands, files, Fluent strings and the game names (i18n/), style tokens, built-in data (ADR 0010, 0012)
   ├─ aic-node-graph/         # The node canvas widget, our copy of egui_graph 0.19.1 (ADR 0011). deps: egui, siphasher
   ├─ aic-app/                # eframe UI and the icons (assets/icons/). deps: aic-editor, aic-node-graph, eframe, egui-scale, egui_dock, rfd
   └─ aic-extract/            # CLI: the game's exported tables -> data/<version>.json and English names (M8, ADR 0014). deps: aic-data, serde, serde_json, indexmap, thiserror
```

`aic-data` exposes the data file format as public serde types with a writer
(`aic_data::file::DataFile::to_json`), which `aic-extract` writes through, and `Catalog::to_json`
dumps the normalized catalog for inspection.

Crate versions (crates.io, checked 2026-11-08):

- **Pin `eframe = "0.36"`** even though 0.37 is out. The pin was egui-snarl's; now that the
  canvas is `aic-node-graph` (on `egui` 0.36), moving to a newer egui means porting that crate
  ourselves (ADR 0011), along with `egui_dock`, `egui-scale` and `egui_kittest`.
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
- **Activation input (ADR 0013):** on a facility with one (the transmuting units, from the overlay),
  each running facility needs it in full whatever its throughput: the port carries
  `count · (rate_in − activation) + ceil(count) · activation`. A recipe's inputs still list the total
  at one facility flat out.
  - On the depot, the program has only `count · (rate_in − activation)`; the rest is added to the
    port's flow after the solve.
  - Fed by links, pass 1 gives the node a whole number `running` in place of `ceil(count)`, tied by
    `running ≥ count` and `running ≤ count + 1 − 1e-5`, and stops its branch and bound after 200 nodes
    with the best answer found (`NODE_LIMIT`). If it found none, those nodes are switched off and
    pass 1 is solved again as a linear program. Pass 2 and the throttle analysis take `running` as
    given and keep each count within `(running − 1, running]`.
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
**section**, solved as its own LP (pass 1 mixed-integer if a node's activation input has links, ADR
0013). A section that fails (unbounded, or no driver) goes blank and gets a
warning, while every other section still shows numbers. This also keeps each LP small.

### Two passes per section

1. **Maximize throughput.** Run everything as high as limits and supply allow, with no throttling that
   could be avoided. Throughput is the output of every **origin** plus the consumption of every sink
   (ADR 0004). An origin is a node nothing in the graph feeds: a source, a recipe node with no linked
   input (measured by its inputs if it has no outputs), a storage's output side, or a splitter, merger
   or control port pulling from the depot. Links into an activation input that has no part that scales
   don't count as feeding the node (ADR 0013). A section with neither is a closed loop and maximizes the
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

microlp's whole numbers need finite bounds, so a node whose activation input has links and that has
no limit runs at most 1,000,000 facilities in pass 1; a solve that reaches that bound counts as
unbounded (ADR 0013).

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
A unit whose activation input can't reach one facility's worth names that input; the source behind it
sends nothing and so isn't at its limit, and no limiting node is named (ADR 0013).

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
    (`cargo run --release -p aic-app --example perf_spike`). (Since M11 step 4 that file is the
    egui_graph spike, once `perf_spike_graph.rs`; this egui-snarl one is in git history.) It
    drives itself through idle, pan, zoom, node drag, new-link drag and live-value phases with
    vsync off, prints frame times, and exits.
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
  - Storage: balance in the header (green +, red −); pass-through is in the inspector only.
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
- M13 moved the icons out of the data into the app (`crates/aic-app/assets/icons/`, ADR 0012).

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
  "facility_speeds": { "<facility id>": <speed> }, "facility_activation": { "<facility id>": { "item":
  "<item id>", "per_min": <rate> } } }`, with items and recipes written as in `data.json`.
  `facility_activation` (ADR 0013) names a facility's activation input: the facility must exist, the
  item too, the rate must be positive, and every recipe of the facility must take at least that much
  of the item at one facility; otherwise it is a load error. `data/v1.5.3_overlay.json` declared the
  two transmuting units'. (M8 replaced this format: an overlay now has `items`, `facilities` and
  `recipes` in the data file's own entries, and activation inputs are in the data file; ADR 0014.)
  Adding an ID that exists, or replacing or removing one that doesn't, is an error that names
  `overlay.json` and the path; unknown keys are errors too. `Catalog::from_bytes_with_overlay` for the web.
- Settings (renamed from Raw resources): data version and game-name language (from `i18n/*.json`), kept in
  eframe storage. Switching keeps the open graph and its undo steps: recipe nodes are matched against the
  new data by ID (`Graph::rebind`), so a recipe the new data lacks shows as missing until switched back.
  A chosen version that is gone falls back to the newest, a language that is gone to English.
- M13 made the overlay `data/<version>_overlay.json` and the languages the app's own names tables
  (ADR 0012).
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
  region, not only the graph's. A recipe the graph's region has carries no mark; one it lacks is muted
  there, with a one-letter badge ("W", "V") for each region that has it, also on its card. Hovering
  names the region, and the inspector says "Wuling only" of any recipe limited to some regions. A card
  whose recipe or facility the graph's region lacks shows a warning sign before its title, from the
  solver's region warnings.
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
  - 2026-11-16, Windows 11: i5-11400 (6 cores), RTX 4070 Ti, Balanced power plan, with Chrome
    (10+ tabs), Discord and music open, as when the game runs beside the app. Background load makes
    single runs noisy, so compare builds only in alternating runs. `perf_app` only; `perf_spike` not
    yet run here.
    - Headless was 12.5–13.7 ms idle and 19.9–21.4 ms live values: 3–3.5x the Mac and over the
      8 ms budget. Windowed (vsync off) 17 / 14 / 20 ms for idle / pan / live values.
    - GPU is not the cost: 2.4% mean, 4.6% max of the 4070 Ti while redrawing nonstop, 0% idle.
      Idle draws about 0.8 frames a second (5 of 16 from input events, the rest uncaused, likely
      from Windows itself); the released app idles at 0.1–1% of one core, with spikes to 6% from
      background noise whichever allocator is in.
    - Profiled the headless run with samply (`cargo build --profile profiling`; record with
      Windows' own `wpr -start CPU -filemode` as administrator, then `samply import` the `.etl`;
      samply's own recording needs xperf from the Windows Performance Toolkit). About 32% of
      CPU time was in the Windows heap (`ntdll` allocate and free), spread over hundreds of call
      sites in epaint text and meshes, microlp and `aic_core::solve`. egui-snarl drawing nodes
      was about 49% (it lays out every node each frame, on screen or not), the solve 20% (the live
      half solves every frame on purpose), egui's context lock about 10%, SipHash in our
      `HashMap`s about 3%. egui deep-copies every text galley each frame (`TextShape::transform`
      calls `Arc::make_mut` because the snarl layer is transformed for zoom): about 2–3%.
    - **mimalloc** as the global allocator in `aic-app` (lib.rs, so examples and tests use it
      too; not on wasm). Headless, 8 alternating runs with the background apps open: idle 12.9 →
      10.3 ms (−20%), live values 19.9 → 14.6 ms (−26%). Again with them closed (only the IDE),
      where runs vary under 1%: idle 11.7 → 9.0 ms (−23%; p95 13.7 → 10.1), live values 17.9 →
      13.0 ms (−27%; p95 19.4 → 14.6). So background load cost about 10–15%. The released app
      idles under 1% of one core with either allocator (the spread is a few frames' worth).
      Memory, quiet PC: working set 176–185 → about 200 MB, private 401–414 → 465 MB, since
      mimalloc keeps freed pages for reuse. The Mac numbers above predate it; re-measure there.
    - `lto = "fat"` + `codegen-units = 1`: headless idle 13.7 → 12.3 ms (−10%), live values 21.4 →
      20.0 ms (−7%), at about 6 minutes a release build. Kept out of `release` so it and `profiling`
      stay quick; applied last, to packaged builds only (see Next).
  - 2026-11-17, same Windows PC, background apps open: egui-snarl against egui_graph 0.19.1 on
    the M4 spike's graph (300 nodes, 600 links, a rate label on every port and link).
    `SPIKE_HEADLESS=1` on `perf_spike` (egui-snarl) and `perf_spike_graph` (egui_graph) times UI
    build plus tessellation with no window; both on mimalloc. (M11 step 4 deleted the egui-snarl
    spike and renamed `perf_spike_graph` to `perf_spike`.) Means of 6 alternating runs:
    - Zoomed in (`SPIKE_SCALE=1.0`, 35 of 300 nodes on screen): idle 10.75 ms for egui-snarl,
      8.86 ms with the spike's own culling (`SPIKE_CULL=1`), 4.73 ms for egui_graph's
      `Node::show_or_cull`. Live values 12.02 / 9.71 / 5.40 ms. egui_graph without culling
      (`Node::show`) was 9.93 ms idle, so the gain is the culling.
    - All 300 on screen: idle 15.54 ms for egui-snarl, 18.66 ms for egui_graph. The gap is
      tessellating links: egui_graph cuts a link into a point every 5 graph units at any zoom,
      418k vertices a frame against egui-snarl's 180k. A point every 5 screen pixels
      (`SPIKE_EDGE_DPP=5`, which is `Edge::distance_per_point(5.0 / zoom)`) gave 15.39 ms against
      15.55 ms. Without links the two were level (11.86 / 11.73 ms).
    - egui_graph's two sublayers per node, its memory lock and its position animation were each
      timed and removed in a scratch copy: together under 0.5 ms, inside the noise.
    - So culling brings a zoomed-in view under the 8 ms budget, and nothing here helps a view of
      the whole graph, about 15 ms in both. The spike's cards are plainer than the app's (no
      icons, fewer widgets); the app's numbers will be higher on both sides.
  - Next, in order:
    1. Draw only the nodes on screen: M11, which moves the canvas to our own copy of egui_graph.
       Items 2 and 3 went to M12, which then left item 2 out: see there.
    2. Faster hashing for our own maps: `FxHashMap` (`rustc-hash` 2, already in the tree) where
       keys are IDs, per the perf book's hashing chapter; then `clippy::disallowed_types` to keep
       std `HashMap` out.
    3. Last, when the optimizing is done: a release-like Cargo profile with `lto = "fat"` and
       `codegen-units = 1` for `scripts/package-*`. Not named `dist`: that is `target/dist/`, where
       the packaging scripts already write.

### M8: Our own data format and `aic-extract`
Done (2026-11-25, [ADR 0014](adr/0014-our-own-data-format-extracted-from-the-game.md)). The data
is in our own format, read by one loader, and `v1.6.0` comes from the game client's own tables.

- **The format** (`aic_data::file`, reference in `data/README.md`): `format` 1, belt and pipe per
  minute, regions, categories, items (phase, colour), facilities (speed, power, regions, activation
  input), recipes (time, inputs and outputs in order, facilities, power override, `kind` mining or
  trade, hidden, regions, gas environment) and aliases. Unknown keys and any other `format` are load
  errors. Public serde types with `DataFile::to_json`, so `aic-extract` writes the format through
  `aic-data`'s own types; `raw.rs` and factoriolab's format are gone.
- **Activation inputs** are listed apart from the recipe in the file, as the game does, and the
  loader adds them to every recipe of the facility, so `Catalog` and `aic-core` are as ADR 0013 has
  them. Facilities of one recipe that disagree on one are a load error.
- **`Catalog`**: `items()` no longer holds facilities; `Item` has `phase` (`is_fluid()` is a
  method) and `color`, and lost `name_zh`, `category` and `icon`; `Catalog::icon` and the sprite
  rectangles are gone; `Recipe` lost `icon` and has `environment`; `recipe_image` is first output,
  else first input, else first facility. `recipe_alias` and `item_alias` give an older version's IDs'
  counterparts; an alias's target must exist and its source must not. A missing name falls back to
  the ID (no more Chinese names).
- **Overlay**: `items`, `facilities` and `recipes`, each `add` / `replace` / `remove`.
  `facility_speeds` and `facility_activation` are gone, and with them `data/v1.5.3_overlay.json`.
- **v1.5.3 converted** once by a throwaway script, with the activation inputs taken out of the
  transmuting units' recipes and put on the facilities. Checked against a dump of the catalog before
  the change: every item (but `belt` and `pipe`, which no recipe used), facility and recipe, in the
  same order, with the same numbers, flags, regions, names and colours.
- **`aic-extract`** (`crates/aic-extract`, a library and a thin `main.rs`; deps `aic-data`, serde,
  serde_json, indexmap, thiserror): `aic-extract <Table folder> --version v1.6.0 [--previous
  data/v1.5.3.json] [--data-dir data] [--names …/en.json] [--lang EN]`. It writes the version,
  adds the English names the table lacks to `en.json` (changing none but a facility's, which takes
  the game's name for its building, and a filling's or emptying's, below; keeping the table's key
  order and line endings), checks that the result loads, and prints a report. What it reads and how:
  - recipe IDs are the game's craft IDs; items the game's ID without `item_`; facilities the ID of
    the item that places them without `item_port_` (two buildings without one keep the app's
    `liquid_clean_gate` and `liquid_recycle_gate`); regions `domain_1` → `tundra`, `domain_2` →
    `jinlong`;
  - the same craft, mining output or sink on several buildings is one recipe on several facilities;
  - a craft that fills a container with a fluid, and the craft that empties it again, are named
    `Packaged <fluid> (<container>)` and `Unpackage <fluid> (<container>)` (150 of v1.6.0's
    recipes): the game calls every emptying of an Amethyst Bottle "Amethyst Bottle Production"
    and every filling with water "Clean Water (Bottled)";
  - other recipes the game gives one name ("Carbon Powder Production" is six) take, in
    brackets, the first input the others of that name don't all take, or, with the same items,
    the gas whose environment they need; one that has neither keeps the plain name (40 of
    v1.6.0's recipes renamed, none left sharing a name; any that are go in the report);
  - recipe order: with `--previous`, a recipe that version had (by ID or alias) goes where it
    was; a new one goes right after the last recipe that makes the same main product (or, making
    nothing, takes it), else after them all in the game's order (building, then `sortId`). So
    v1.6.0 keeps v1.5.3's order, which follows each raw resource through what is made of it.
    Fillings, then emptyings, then trades always come last. Without `--previous`: mining, crafts
    by building, the other buildings, then those;
  - mining from the rigs, the gas extractor and pumps (natural liquids only), power from the
    Thermal Bank's fuels, the Water Treatment Unit, the sewage inlet and outlet (5 s per 30, from
    the plant's three inlets), the Gas Dispersing Unit (6/min of each of its gases), outpost trades
    at each outpost's highest level (the larger price where two disagree; none did);
  - a recipe's regions are what its items share; its category the showing type of its first output
    (`type_<n>`, named from `ItemShowingTypeTable`), `gen-power` and `settlement`;
  - where the game calls items the same (a filled bottle is called as its empty bottle), one made by
    filling a container of that name is called `<container>(<contents>)`;
  - aliases from `--previous`: seven item IDs the old data made up, and each old recipe whose ID is
    gone to the one new recipe on its facilities with the same items at the same rates. The
    previous version's own aliases come first and are carried forward, re-pointed where their
    target was renamed in this run (no alias leads to another); one whose target is gone with
    nothing in its place is left out and reported.
  Tests on hand-written tables in `crates/aic-extract/tests/fixtures/`.
- **`data/v1.6.0.json`**: 247 items, 32 facilities, 387 recipes, 7 item aliases and 155 recipe
  aliases. Every v1.5.3 recipe that v1.6.0 has, by ID (42) or alias (155), runs at the same rates per
  minute on each of its facilities, activation inputs included (an `#[ignore]`d test in
  `aic-data/tests/real_data.rs`); trades compare coupons per item, since a trade is untimed (2 s
  now, 2 or 10 s in v1.5.3). Left behind: the 59 cross-region transfers, which the app hides, and
  `__miner_water`, the old data's own water rig. It is the newest version, so the app's default.
- **`Graph::rebind` follows aliases** for recipe nodes, outpost orders and every item a graph
  holds (sources, sinks, logistics nodes, workspace ports, links), and `Graph::from_json` rebinds,
  so a graph saved on v1.5.3 opens on v1.6.0. Switching back finds the new IDs missing: aliases go
  one way.
- **A new recipe node starts on the first facility its graph's region has** (`picker::recipe_node`
  takes the region), else on the recipe's first. Originium Ore now lists the Portable Originium Rig
  first, which only Valley IV has, so a Wuling graph would have started on it with a region warning.
- Icons: the four bottle items v1.5.3 named otherwise are copied under the game's IDs; v1.6.0's other
  new items use jambochen's icons that were unused, or none (35, and the coupons).
- Left for later: a diff between two versions (`aic-extract diff`), other languages' names, and the
  gas environment itself (M14).

### M9: Web build
Done.
- `wasm32-unknown-unknown` build of `aic-app` with eframe's web runner, built by Trunk:
  `scripts/build-web.sh` writes `target/web/` (`--serve` serves a debug build with live reload).
  `crates/aic-app/index.html` and `Trunk.toml` hold the setup; Trunk uses the `wasm-bindgen-cli`
  installed with Cargo at the version in `Cargo.lock`, so the build downloads no tools of its own.
- Data comes from the same site: Trunk copies `data/` next to the page, and on `wasm32` `build.rs`
  writes the list of its files (paths only, no bytes). The app fetches them all at startup with a
  loading screen, puts them in an `aic_data::Bundle::from_owned`, and from there loads exactly as the
  desktop app loads its built-in data, version switching included. (M13 built the data and the
  names into the `.wasm` and took this fetch out: ADR 0012.)
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

### M11: The canvas on `aic-node-graph`
Steps 1 to 4 are done. Step 5 is measured on Windows; the Mac, the full web build and live
values under 8 ms in a window are to do
([ADR 0011](adr/0011-canvas-on-our-own-copy-of-egui-graph.md)); the spike's numbers are
under M7. The canvas moves from egui-snarl to `crates/aic-node-graph`, a copy of egui_graph 0.19.1
that is ours to change. The player should notice nothing but the speed: every card, port, link,
label, menu, shortcut and mouse habit stays as M4 to M10 left it.

Work on the `exp/node-graph-optimize` branch, one commit per step. Steps 1 and 2 leave the app on
egui-snarl and pass the milestone checks on their own. Step 3 is the switch.

1. **Copy the crate in.**
   - Copy `src/` of egui_graph at commit `1a5e2b5` (v0.19.1) to `crates/aic-node-graph/src/`,
     with its tests and without `layout/` (the automatic layout), `examples/` and the `layout`
     and `serde` features. Copy `LICENSE-MIT` unchanged. `Cargo.toml`: `license = "MIT"`, its
     own `edition = "2021"`, dependencies `egui` 0.36 and `siphasher`.
   - `SOURCE.md` in the crate: where it came from, the commit, what was left out, and a list of
     the changes made here, kept up to date from step 2 on.
   - Add it to the workspace. `scripts/check-frontends.sh` forbids it for `aic-data`,
     `aic-core` and `aic-editor`. `CLAUDE.md` rule 6 names it beside `raw.rs` as code that keeps
     another vocabulary (edge, socket), and rule 9's checks cover it like any crate.
   - Point `perf_spike_graph` at the crate and drop the dev-dependency on the clone outside the
     repo, which must never be committed.
   - **Done when:** its own tests pass under `cargo test --workspace`, clippy is clean, and
     `SPIKE_HEADLESS=1` on `perf_spike_graph` gives the numbers of 2026-11-17.
2. **Change the widget to suit the canvas.** Each change test-first in the crate, the way its own
   tests drive a graph one pass at a time with `RawInput`. It is our widget now, so the
   conventions are changed in place, not made into options.
   - **A link starts inside the card.** A node's content can name a rect as the handle of one of
     its sockets. A press on a handle starts an edge from that socket; a press anywhere else on
     the frame still drags the node; a release on a handle ends an edge there. The socket's own
     half-disc can be turned off, since the card draws its ports itself.
   - **The mouse.** A primary drag on the background pans. A click selects one node, Shift+click
     adds or removes one, a click on the background clears. A box is drawn by a secondary drag
     on the background, or a primary drag with Shift; Shift adds what it touches to the
     selection and Cmd/Ctrl takes it away. A secondary click without a drag is reported, with
     its place, for the picker. Dragging a selected node moves the selection; dragging any other
     node moves that node alone.
   - **Scrolling is the app's.** The widget neither pans nor zooms on the wheel; the app sets
     `View::scene_rect` (whole 5% zoom steps around the pointer, Shift+wheel pans sideways).
   - **The selection is the app's.** It goes in every frame and changes come back, as now
     (`selected_nodes`, `selection_changed`). The widget stops handling Delete and Backspace.
   - **Links by the pixel.** An edge is cut into a point every 5 screen pixels, not every 5
     graph units.
   - **Done when:** the crate's tests cover each of these, and `perf_spike_graph`, moved to the
     new conventions, is no slower.
3. **Switch the canvas.** `canvas.rs` keeps its `CanvasEvent`s, so `editor.rs` and everything
   behind it stay as they are.
   - `DocView` loses its second graph. A member's widget id is made from its `NodeId` or
     `WorkspaceId`; what is left to keep in step is each member's position, and a node drag is
     handed to the document when it ends (`GraphResponse::nodes_moved`). `ViewId` and
     `reconcile`'s adding and removing of view nodes and wires go.
   - A card is one closure: the tinted header, a port row per item (the item's icon as the
     socket's handle, the rate beside it, `SocketLayout::row` lining the socket up), and the
     body. The drawing code moves over from the `SnarlViewer` callbacks nearly as it is.
     Cards show with `Node::show_or_cull`; the card whose limit field has focus shows with
     `Node::show`, so typing survives the card leaving the view.
   - Sharp text as now: cards are laid out at `VIEW_SCALE` and the zoom stops at 1 in view
     space. egui-snarl scaled the style for us; the canvas now does it, with `egui-scale` as a
     dependency of `aic-app` itself (today it comes in through egui-snarl).
   - One edge per pair of ports shown, as there is one wire now. Link labels are painted with
     the edge, at the middle of its path; a click on a label selects the link.
   - Port clicks are the card's own widgets: a double-click opens the port menu, a right-click
     unlinks the port or opens the picker for it. A link dropped on empty canvas
     (`EdgeEvent::Cancelled` with the pointer over the background) opens the picker there. A
     card's menu is its response's context menu.
   - The camera is `View::scene_rect`, one per view and level. `Canvas::view`, `show_view`,
     `zoom_to`, `to_screen`, `item_on_screen` and `port_on_screen` are rewritten on it and on
     the widget's node sizes and socket places. A workspace's ports are placed on the canvas
     edges from the camera before the graph is shown. A view that moves on screen (a tab
     dragged, a split resized) keeps showing what it showed.
   - Snapping, alignment guides and panning near the edge during a drag come with egui_graph.
     Off at first, so the switch changes nothing the player sees; turning them on is a later
     choice.
   - Tests: every canvas snapshot is deleted, regenerated and looked at. A test that finds an
     off-screen card by its label brings it into view first. New: a snapshot of a zoomed-in
     view with cards cut by the edge, and a test that a link to a card out of view still ends
     at its port.
   - **Done when:** the M4 "done when" graphs can be rebuilt by hand as before, the canvas
     tests of M4 to M10 pass, and each view of a split shows and edits the graph.
   - **As built** (2026-11-22), where it differs from the plan above:
     - A card's position is its corner. With egui-snarl it was the corner of its content, 7
       points in, so a file from before opens with every card 7 points up and to the left.
     - A level is laid out once per change, not mirrored: `DocView` keeps a `Card` per member
       with its pins and which are linked, and a `Wire` per pair of pins. The canvas reads them
       each frame and looks nothing up in the graph for them.
     - A drag is handed to the document every frame, as before, not when it ends: a second view
       of the level follows it live.
     - A level's camera is kept from the canvas's corner, so it holds when a tab moves or a
       split is resized, for the level shown and for the ones waiting.
     - A level first shown with cards on it is fitted to all of them, down to 20%. egui-snarl
       stopped at 40% and showed the middle of a large graph. `Editor::fit_canvas` asks for the
       same fit; a double-click on the background fits to the cards as drawn.
     - Link labels are painted on a layer of their own over the cards, as they were read before.
     - The widget gained `NodesCtx::set_position` (a workspace's ports are pinned with this
       frame's view, after its pan), `Graph::selection_area` (the box keeps the accent colour)
       and taps: a press and release inside one frame now selects, clears and opens the picker
       as a slower click does. A tap did nothing after step 2.
     - Canvas tests added, all driven with pointer events: a link dragged from pin to pin, both
       ways; a link dropped on a pin of another item; a right click on a link and on a linked
       port; clicks, Shift+clicks and a background click; a click on a link's label; a card's
       menu; a double-click on a workspace's card and on the background; Shift with the wheel
       and a pinch; typing in a card that has left the view; a link to a card out of view; a
       link's label far from the graph's origin. Snapshots compare exactly now: wires are drawn
       in the order of the graph's links, not a hash set's.
   - **Measured** the same day on the Windows PC, background apps open, `perf_app` headless
     (201 nodes, 109 links), alternating runs against egui-snarl built from the commit before
     with the same `PERF_ZOOM`:
     - Zoomed to 100% (`PERF_ZOOM=1.0`, part of the graph on screen), 4 runs each: idle
       8.2–8.4 ms on egui-snarl, 2.6–2.8 ms now; live values 12.3–12.5 ms, 6.8–7.0 ms now.
     - The whole graph on screen at 25% (`PERF_ZOOM=0.25`), 3 runs each: idle 11.0–11.3 ms,
       10.1–10.7 ms now; live values 16.0–16.1 ms, 15.1–15.9 ms now.
     - `PERF_IDLE=12`, windowed: 1 frame in 12 s.
4. **Take egui-snarl out.** The dependency, `examples/perf_spike.rs` (the egui_graph spike
   takes its name), the egui-snarl style in `theme.rs`, and its name in this file's Tech,
   workspace and crate-version sections, `CLAUDE.md`, `DESIGN.md` and `PRODUCT.md`.
   - **Done** (2026-11-22). As built: `theme.rs` had no egui-snarl style left: the
     `SnarlStyle` was built in `canvas.rs` and went with it in step 3, which also fixed
     `theme.rs`'s module comment; nothing in it was dead. `Cargo.lock` lost egui-snarl alone,
     and no feature it switched on was missed. The `.impeccable/` notes stopped naming it too.
     Older text naming `perf_spike_graph` or the egui-snarl `perf_spike` has a note beside it.
5. **Measure and write it down** here, on Windows and the Mac, with `perf_app`.
   - Zoomed in to part of the 201-node graph: under 8 ms a frame while panning and with live
     values, headless and windowed. `PERF_ZOOM` zooms `perf_app`'s headless run; the windowed
     run still needs a zoomed-in phase.
   - The whole graph on screen: no slower than egui-snarl was (headless on the quiet PC,
     9.0 ms idle and 13.0 ms live values).
   - `PERF_IDLE=20` and `PERF_MINIMIZE=1` still draw no frames. egui_graph asks for repaints
     while it animates a node's position or pans near an edge; neither may fire at rest.
   - The web build runs (`scripts/build-web.sh`).
   - **As built** (2026-11-22): `PERF_ZOOM` zooms the windowed run too, and the headless run
     gained a pan phase. The pan phase holds Shift with the wheel. Before, it sent the wheel
     alone, which the canvas takes as zoom, so its windowed "Pan" row timed zooming. A test
     (`a_drag_held_at_the_edge_and_a_moved_card_ask_for_no_frames`) checks that a card moved by
     an undo is in place the next frame and that a drag held at the edge asks for no frames:
     the canvas turns the widget's animation and auto-pan off. It fails with either turned on.
   - **Measured** 2026-11-22 on the Windows PC, background apps open, `perf_app` with the
     201-node graph (109 links):
     - Zoomed to 100% (`PERF_ZOOM=1.0`), headless, 4 runs alternating with egui-snarl: idle
       2.6–2.7 ms (egui-snarl 8.2–9.5 ms), pan 2.6–2.9 ms, live values 6.8–7.2 ms with p95
       7.7–9.5 ms (egui-snarl 12.2–12.6 ms). Under 8 ms.
     - Zoomed to 100%, windowed with vsync off, 3 runs: idle 4.4–4.6 ms, pan 4.3–4.6 ms, of which
       the UI is about 2 ms. Live values 8.6–8.8 ms (UI 6.2–6.4 ms), p95 9.9–10.6 ms: over the
       8 ms budget, missed.
     - The whole graph at 25% (`PERF_ZOOM=0.25`), headless, 4 runs alternating with egui-snarl:
       idle 9.9–11.6 ms (egui-snarl 10.9–13.9 ms), live values 14.9–17.6 ms (16.1–18.6 ms), pan
       9.5–11.7 ms. No slower than egui-snarl run by run. The quiet-PC numbers above (9.0 and
       13.0 ms) were not re-measured with the background apps closed.
     - The whole graph at 25%, windowed, 3 runs: idle 12.8–13.6 ms, pan 11.1–11.5 ms, live
       values 15.3–15.7 ms. Over 8 ms, as expected until cards get cheaper zoomed out.
     - `PERF_IDLE=20`, 3 runs: 1 frame in 20 s each, with no repaint asked for by the app; a
       window event drew it. `PERF_IDLE=20 PERF_MINIMIZE=1`, 3 runs: 0 frames.
     - Web: trunk and wasm-bindgen-cli are not installed here, so `scripts/build-web.sh` was not
       run. `cargo check` and `cargo clippy -- -D warnings` for `-p aic-app --target
       wasm32-unknown-unknown` pass with no warnings. To do: the full web build.
     - To do: the Mac. Nothing was measured there for M11.
     - Found while checking, and fixed: the `golden_07` snapshot failed in most runs with 18
       pixels off, since step 3 made snapshots compare exactly. The cards were in the same place
       to the bit; the font atlas was not. kittest draws a frame as it builds a harness, before
       the tests hid the status bar's frame rate and solve time, so those digits entered the
       atlas in a different order each run, and a glyph drawn smaller than it was rasterized
       takes a little of its neighbour in the atlas. The tests now hide them before that frame
       (`steady` in `snapshots.rs`). The app is unchanged.

Not in M11: a cheaper card for a view zoomed far out, which is what the whole-graph budget needs
and is easier now that a card is ours to draw; automatic layout; selecting a link by clicking the
link itself. The M7 list goes on in M12.

### M12: Less work per frame
Steps 1 to 6 are done (2026-11-22 and 23), with two targets missed: a 200-node section still
solves in about 26 ms (step 2, left to the user), and a whole-graph idle frame makes 30.6k
allocations, not under 16.5k, after step 3 (21.2k after step 4). The whole graph on screen is
under 8 ms headless, idle and panning. Rewritten on 2026-11-22, after M11 and a second read of the
[Rust Performance Book](https://nnethercote.github.io/perf-book/). Its general tips say the
biggest gains come from algorithms and data structures, and that a hot function is sped up
either by making it faster or by calling it less. The first plan for this milestone (integer
IDs, `FxHashMap`, rates formatted once) made the per-frame work faster. This one does less of
it: the two things below are redone far more often than their inputs change.

**Measured** 2026-11-22 on the Windows PC, background apps open, headless, with a scratch copy of
`perf_app` that timed a frame's parts and counted allocations (medians of 600 frames, one or two
runs each, so the times are rough; the counts were the same in every run). 201 nodes, 109 links:

| Case | UI build | of which solve | Tessellation | Allocations |
|---|---|---|---|---|
| Chrome only (2 nodes) | 0.75 ms | – | 0.12 ms | 2.5k |
| 100% zoom, idle | 2.0 ms | – | 0.7 ms | 5.5k |
| 100% zoom, live values | 6.5 ms | 4.0 ms | 0.7 ms | 57k |
| Whole graph (25%), idle | 7.1–7.4 ms | – | 2.3 ms | 27k |
| Whole graph, live values | 12.2–12.7 ms | 4.1–4.2 ms | 2.5 ms | 79k |

- **The solve.** Any change re-solves the whole graph, on the UI thread, at the end of the
  frame. The `perf_app` graph is 92 small sections, which hid what one large section costs. One
  section, a ring of producer and consumer pairs (one shape of graph, so a first look only):
  50 nodes 3.5 ms, 100 nodes 9.3 ms, 200 nodes 27.6 ms, 400 nodes 104 ms. A real factory is
  mostly one section. The solve is also M11's missed target: 4 of the 8.6–8.8 ms.
- **The cards.** Every card on screen is built every frame from the graph, the catalog and the
  solution: about 120 allocations and 40 µs a card, though what it shows changes only with a
  solve or an edit. Each frame clones an item ID to look up every port's flow, formats every
  rate, builds every title, and reads the solution's warnings through for every card.
- Hashing was about 3% of CPU time in M7's profile. The cost is around the lookup (the key's
  `String`, the formatting), not in it.
- egui copies a text's whole layout when its layer is moved or scaled
  (`TextShape::transform`), and every card is on such a layer: by estimate a third of a card's
  allocations. Only fewer texts on a card avoid it.

Each step is measured on its own with `perf_app`, headless, in alternating runs against the
build before it, and its numbers are written here. A step that gains nothing is dropped, not
kept for tidiness. Solver semantics (section 5) and every golden test stay as they are.

1. **Measure what a frame is made of.**
   - Headless `perf_app` prints, for each phase, the UI build, the solve inside it and the
     tessellation apart.
   - `PERF_GRAPH=chain` builds the one-section graph above in place of the usual one, with
     `PERF_NODES` nodes.
   - [dhat-rs](https://crates.io/crates/dhat) behind a feature of `aic-app` (it replaces the
     global allocator, so mimalloc is off while it is on; it works on Windows). With it,
     `perf_app` prints allocations per frame and writes dhat's file of allocation sites.
   - **Done when:** the table above and the one-section solve times are measured again with it,
     3 runs each, and the top allocation sites of a whole-graph idle frame and of a solve are
     listed here.
   - **Done** (2026-11-22). As built: headless `perf_app` prints a frame's mean, median and p95 as
     before, then the medians of the UI build, the solve and the tessellation. `PERF_FRAMES` sets
     the frames a phase, `PERF_PHASE=Idle` (or `Pan`, `LiveValues`) runs one phase. The feature
     is `dhat-heap`, an optional dependency on the desktop only; it takes mimalloc's place as the
     global allocator. dhat's profiler starts after the set-up and warm-up, so its file holds the
     measured frames only, 20 a phase by default. Its trimming of backtraces left none at all on
     Windows, so the run keeps whole stacks: build with `--profile profiling` for their names.
     The site lists below were summed by a scratch script over dhat's file, not kept in the repo.
   - **Measured** the same day on the Windows PC, background apps open, headless, 3 runs each
     (600 frames a phase), alternating with the build before the step. Its frames took as long
     as the old build's, run by run (100% idle 2.68–2.78 ms against 2.65–2.76 ms, whole-graph live
     values 14.17–14.25 ms against 14.11–14.28 ms). Times are ranges of medians; allocations are
     dhat's, the same in every run, UI build + tessellation:

     | Case | Frame (mean) | UI build | of which solve | Tessellation | Allocations |
     |---|---|---|---|---|---|
     | Chrome only (2 nodes), idle | 0.89–1.03 ms | 0.74–0.76 ms | – | 0.12–0.13 ms | 2.5k + 0.3k |
     | 100% zoom, idle | 2.68–2.78 ms | 1.92–1.96 ms | – | 0.65–0.66 ms | 5.5k + 0.7k |
     | 100% zoom, live values | 6.98–7.01 ms | 6.03–6.04 ms | 3.62–3.63 ms | 0.70 ms | 57.5k + 0.7k |
     | Whole graph (25%), idle | 9.51–9.67 ms | 6.87–6.98 ms | – | 2.28–2.30 ms | 27.1k + 5.9k |
     | Whole graph, pan | 8.40–8.66 ms | 6.17–6.43 ms | – | 2.03–2.06 ms | 26.5k + 5.8k |
     | Whole graph, live values | 14.17–14.25 ms | 11.20–11.33 ms | 3.65–3.68 ms | 2.34–2.36 ms | 77.9k + 5.6k |

     - One section (`PERF_GRAPH=chain`, live values, 200 frames; 60 for 400 nodes), the solve:
       50 nodes 3.2–3.3 ms, 100 nodes 8.7 ms, 200 nodes 25.3–25.7 ms, 400 nodes 95–99 ms. About
       the probe's numbers. A 200-node frame is 28.6–28.9 ms in all.
     - Top allocation sites of a whole-graph idle frame, 33.0k with tessellation: egui copying
       text moved to a card's layer (`TextShape::transform`) 7.9k (24%), tessellating 5.9k (18%),
       `LimitField::show` 2.0k, `Cards::card_content` 1.9k (its child `Ui`s and sockets), the
       recipe browser 1.4k (a side panel), `ItemPicture::paint` 1.4k, `aic-node-graph`'s
       `Node::show_impl` 1.2k and socket layout and painting 2.0k, `Cards::header` 1.0k,
       `format::short` 0.9k, `big_number` 0.8k, `Strings::format` 0.7k, `Catalog::image` 0.5k.
     - Of a solve: in a whole-graph live-values frame (84k), `throttle::max_activity` 15.6k
       (18%: one LP per linked port of a throttled node), `passes::run` 11.5k (14%), building the
       model 16.6k (20%, half of it `Builder::side`), `ItemId` clones for port keys 3.1k. In the
       200-node section's frame (43.5k), `passes::run` 11.2k (26%), building the model 13.8k
       (32%), `max_activity` 4.0k (9%), `ItemId` clones 2.8k. A frame that changes a limit also
       clones the graph for the undo step (0.8k); that is the edit, not the solve.
     - Not done: a CPU profile. `wpr` needs administrator, which this session did not have.
2. **Solve less.** Profile the one-section solve first (`--profile profiling`, as in M7): is the
   time in building the model, in the LP, or in what is recorded after it? Then, in this order,
   stopping when a 200-node section solves in under 8 ms:
   - **Keep each section's result.** A section whose nodes, links and limits are as they were
     at the last solve keeps its result; only the others are solved. In `aic-core`, behind
     `solve`'s signature or beside it, with no GUI dependency. It does nothing for a graph that
     is one section, and everything for an edit in one of many.
   - **Fix what the profile shows** in our own code: the model built with fewer allocations,
     the two passes sharing what they can.
   - **A smaller model.** Where a link is the only one at both of its ports, the two ports'
     flows are one number: merge such variables before the LP sees them. A special case in
     front of the general solver, which a factory's long chains are full of. Every golden test
     must give the same numbers.
   - **Not without asking:** solving on another thread. It changes ground rule 1's "calls
     `solve`, renders the `Solution`" into something that waits, the web build has no threads,
     and it wants an ADR. If the steps above leave a 200-node section over 8 ms, write down the
     numbers and stop there.
   - **Done, over budget** (2026-11-22): a 200-node section still takes about 26 ms. The profile
     was not samply's: `wpr` needs administrator, which this session did not have, so a scratch
     build timed the parts of a solve with `Instant` (300 solves, medians of the per-100 means):
     - The 200-node chain, 25.4 ms: the second pass's LP (the even split) 20.6 ms, the first
       pass's LP 2.6 ms, throttle reasons 1.2 ms (2 LPs), building both passes' models 0.7 ms,
       validating, splitting, recording and the totals 0.5 ms together. 100 nodes: 8.8 ms, of
       which the second pass 6.7 ms. So four fifths is inside microlp's second pass, which holds
       the first pass's optimum and evens out 200 fan groups; our own code is under a tenth.
     - The usual graph's live values (92 sections), 3.7 ms: throttle reasons 1.8 ms (130 LPs a
       solve), the first passes 1.0 ms, recording 0.2 ms; no section there has a fan group.
   - As built: **each section's result is kept.** `aic_core::solve_keeping(catalog, graph,
     &mut KeptSections)` keeps a section whose nodes (by id, in order), their kinds (limits
     included) and its links are what they were at the last solve, with its throttle reasons;
     the others are solved. `solve` is `solve_keeping` with an empty `KeptSections`. Warnings,
     storage, coupons, belts and totals are worked out afresh each time, so a section that
     moves in the list keeps the right number in its warnings. Sections not seen in a solve are
     dropped. `Document` keeps one and clears it on `rebind`, the one way the catalog changes.
     Tests: in `aic-core`, a kept section's solution equals a full solve's in every map, to the
     bit, after a limit change, a node added and a node removed; in `aic-editor`, a data switch
     (the smelter made twice as fast by an overlay) solves the section again. Every golden test
     keeps its numbers.
   - **Fix what the profile shows** was not done: our own code is under a tenth of the 200-node
     solve, and the cheapest share between the passes, reusing pass 1's model in pass 2, is
     bounded by the 0.4 ms pass 2's model takes to build (under 2%). Checked in passing: pass 1
     leaves 198–200 of the chain's 200 fan groups uneven, so pass 2 cannot be skipped there.
   - **A smaller model** was not built. It merges ports that have one link at both ends; every
     port of the chain has two, so it changes nothing there, and pass 2, the cost, only exists
     for fan groups, which such ports are never part of. It would shrink pass 1 and the throttle
     LPs of real chains. Not measured, since no `perf_app` graph is such a chain.
   - **Measured** 2026-11-22, Windows PC, background apps open, headless, 3 runs each alternating
     with step 1's build (live values; the solve is a median):
     - 100% zoom: frame 7.6–7.8 ms → 4.3–4.4 ms mean, solve 3.8 → 0.71 ms (one of 92 sections
       solved, the rest kept). Idle and pan unchanged (2.7–2.9 ms).
     - Whole graph at 25%: frame 15.0–15.4 ms → 11.6–12.1 ms, solve 3.8 → 0.72 ms.
     - The 200-node chain: solve 26.5–26.8 ms → 26.1–27.2 ms, the same.
   - **Left to the user:** the 200-node section stays at about 26 ms, over the 8 ms budget, and
     what is left is microlp's second pass. What would bring it down is outside this step's
     bounds: solving on another thread (an ADR; the web build has none), another LP solver for
     the desktop (a native dependency), or an even split that costs less to compute (solver
     semantics, ADR 0004).
3. **A card's display data, built once per change.** `DocView` already lays a level out once
   per document generation (its `Card`s and `Wire`s). Each card also gets, at that time and
   after each solve: its title and facility name, its warnings, the number it is about, and for
   each pin its picture, its rate as text, its colours, whether it is throttled. Each wire gets
   its label's text. The canvas then draws from those fields and looks nothing up by ID while it
   draws: no `String` cloned for a key, no rate formatted, no warning list read through, per
   frame. Strings still come from Fluent (ground rule 7) and numbers from the `Solution` (ground
   rule 1); this only moves when they are read.
   - `Canvas::node_rects` and `Canvas::pins` become `Vec`s in the cards' order if the profile
     of step 1 shows their hashing.
   - **Done when:** a whole-graph idle frame makes under half the allocations of step 1's
     measure, and the canvas snapshots are unchanged.
   - **Done, allocations missed** (2026-11-22). As built: `Document::generation` counts edits,
     not solves, so the document gained `Document::solves`. The canvas keeps a `Faces` in the
     `DocView` beside its cards and wires: each card's kind line, title, region badges' recipe,
     warnings, facility name, storage balance, body (picture, number, round-up busyness, coupons,
     pass-through, node count, the limit as its field shows it) and each pin's rate and throttle
     mark; each wire's link labels. It is dropped when the document's generation, its solve
     count or the round-up view changes, and when the view lays its level out again. Each face
     is worked out the first time its card is drawn after that, so a card out of view costs
     nothing: working out all 201 after each solve made 100% live values 0.5 ms slower, the
     first try. Hover hints are still built under the pointer only, and the selection is still
     read as the card draws. `LimitField` takes the limit as text and copies it only when it
     differs; the facility picture asks the icon cache, not the catalog, which built a key each
     frame. `Canvas::node_rects` and `pins` stay `HashMap`s: no CPU profile was taken (step 1).
   - Tests: a new one (`a_cards_text_follows_each_solve_the_round_up_setting_and_the_pointer`)
     checks a card's count after two solves, with round-up on and off again, and that a port's
     hint appears under the pointer; it passed before the change and after. Every snapshot is
     unchanged, compared exactly.
   - **Measured** 2026-11-22, Windows PC, background apps open but quieter than for steps 1 and
     2, headless, 3 runs each alternating with step 2's build:
     - Whole graph at 25%: idle 9.20–9.63 → 8.54–8.64 ms, pan 8.12–8.84 → 7.55–7.60 ms, live
       values 10.44–10.80 → 10.60–10.81 ms (the same).
     - 100%: idle 2.54–3.13 → 2.49–2.63 ms, pan 2.60–2.77 → 2.53–2.57 ms, live values the same
       (3.65–3.73 ms).
     - Allocations (dhat), UI build: whole-graph idle 27.1k → 24.7k, plus 5.9k tessellating
       either way, so 33.0k → 30.6k; live values at 25% 77.9k → 33.2k and at 100% 57.5k → 11.6k,
       with step 2.
     - **Missed:** under half of step 1's 33.0k would be 16.5k. What is left is egui's own work
       per text and widget: copying each card's text to its layer (7.9k), tessellating (5.9k),
       the limit fields' `TextEdit`s (2.0k), child `Ui`s and labels in `card_content` (2.8k),
       `aic-node-graph`'s node and socket layout (about 4k), item pictures' meshes (1.4k). Only
       fewer texts and widgets on a card cut those, which is step 4.
4. **A cheaper card zoomed out.** Below the zoom where a card's small text is under about 5
   pixels high, the card draws its header tint, its pins' pictures and its number, and leaves
   out the texts nobody can read and the limit field. It keeps its size, its pins and every
   mouse habit: drag, select, link, menus. The inspector still edits its limit.
   - New snapshots, one each side of the zoom where it changes. A test that finds a card by
     its text zooms in first.
   - **Done when:** the whole graph on screen, idle and panning, is under 8 ms headless on the
     Windows PC (9.5 ms in the table above).
   - **Done** (2026-11-22). As built: `canvas::PLAIN_BELOW` is 45%. The small text is 12 points
     at 100%, so 5.4 points high at 45% and 4.8 at 40%, the next 5% step down: below 45% it is
     under 5 pixels on a screen of one pixel per point. (On a screen of two pixels per point it
     is still 9.6 pixels at 40%; the threshold is in points, as the zoom is.) Below it, a card is
     laid out exactly as before and only part of it is drawn: its header is laid out in an
     invisible `Ui` (egui drops the shapes and disables the widgets), the pins' rates are not
     painted, and the body draws its picture and its number and then turns invisible for the
     busyness, coupons, limit field, pass-through and node count. So a card keeps its size and
     its pins' places to the pixel, and its menu, selection, drag and links are the card's own,
     as before. The card being typed in is drawn whole at any zoom. Left out with the texts:
     the region warning sign and the region badges (in the header), and a storage's balance.
     Link labels go with them (2026-11-23, at the user's word; the plan named only the card):
     below the threshold only the selected link's label is drawn, since it is what shows the
     selection. A label is what a click selects a link by, so a link is selected at 45% or
     above; a right click on the wire still removes it at any zoom.
   - It is still laid out in full: the gain is in drawing (egui's copy of each text to its layer
     and the tessellation), not in building the UI. Laying out a plain card from its last full
     size would save more and was not tried; a card first seen zoomed out has no last size.
   - Tests: the card keeps its size and its output pin's place, in graph space, from 45% to 40%
     (to under a point); below the threshold a click selects, a pin dragged to a pin links, a
     drag moves the card and its menu removes it; the card being typed in keeps its field from
     100% down to 40%; the inspector sets a plain card's limit; a link's label is painted at 45%,
     not at 40%, and at 40% again once the link is selected. No other test needed to zoom in:
     none finds a card by its text below 45%.
   - Snapshots: new `zoomed_out_full` (45%) and `zoomed_out_plain` (40%), the plate chain on
     each side of the threshold. `logistics_and_throttle`, fitted at 40%, changed: its cards
     lost their texts and kept their places, and both pictures at 40% lost their link labels.
     Every other picture is unchanged.
   - **Measured** 2026-11-22, Windows PC, background apps open, headless, 3 runs each
     alternating with step 3's build. Whole graph at 25%: idle 8.84–8.96 → 7.20–7.22 ms (p95
     8.3–8.5 ms), pan 7.77–7.78 → 6.51–6.60 ms, live values 10.88–10.92 → 9.31–9.46 ms. At
     100% nothing changed (idle 2.6 ms, live values 3.8 ms). Allocations of a whole-graph idle
     frame (dhat): 24.7k + 5.9k → 17.5k + 3.7k; copying text to layers 7.9k → 1.8k (the link
     labels and the numbers), tessellating 5.9k → 3.7k.
   - **Measured** without the link labels, 2026-11-23, the same way (medians, 3 runs each
     alternating with the build that drew them; 109 links). Whole graph at 25%: idle 6.97–7.04
     → 6.71–6.91 ms, pan 6.29–6.33 → 6.10–6.14 ms, live values 8.93–9.13 → 8.61–8.67 ms;
     tessellation 1.57–1.59 → 1.48–1.49 ms. Allocations not counted again.
5. **A test that counts.** dhat-rs's heap usage testing, behind the feature of step 1: one test
   draws a fixed graph for a frame and fails when the frame allocates more than it does at the
   end of step 4, so the count doesn't creep back up. It runs with
   `cargo test -p aic-app --features <the feature>`, not in the workspace's plain test run,
   since it swaps the allocator.
   - **Done** (2026-11-23). As built: `crates/aic-app/tests/allocations.rs`, compiled only with
     `dhat-heap` (`cargo test -p aic-app --features dhat-heap --test allocations`; the whole
     `-p aic-app` run with the feature works too). It builds the app headless on the test
     fixture with a plate chain (a source, a smelter, a constructor and a sink, three links),
     zooms to 100%, draws 30 frames to let the solve, the fit and egui's caches settle, then
     counts one idle frame, UI and tessellation, with dhat's testing profiler. Ceiling: 1113
     allocations, the count in three runs out of three, in the test profile; a release build
     counts differently. The failure message gives the new count, to lower the ceiling when a
     change lowers it. egui wants a frame's texture changes handled even headless, so the test
     clears them.
6. **The packaging profile** (M7 item 3): `lto = "fat"` and `codegen-units = 1` for
   `scripts/package-*`, and try `panic = "abort"` with them. Check first that nothing relies on
   unwinding (a `catch_unwind`, a test harness), and that a panic still leaves a message the
   player can report. Not named `dist`. Last, when the rest is done.
   - **Done** (2026-11-23), `panic = "abort"` kept. As built: `[profile.packaged]` in the
     workspace's `Cargo.toml` (`package` is a name Cargo keeps for itself), inheriting `release`
     with `lto = "fat"`, `codegen-units = 1` and `panic = "abort"`. `scripts/package-windows.ps1`
     and `scripts/package-macos.sh` build with `--profile packaged` and take the binary from
     `target/[<target>/]packaged/`. The Mac script is changed the same way and **not run**: no Mac
     here. The Windows script was not run either: its build is a second fat-LTO build of about 6
     minutes, and only its profile flag and the folder it copies from changed.
   - Unwinding: nothing in the code calls `catch_unwind`, sets a panic hook or spawns a thread;
     the tests and kittest use the test profile, never this one. The panic message: a scratch
     program built both ways printed the same message (`thread 'main' panicked at …`) before it
     ended; only the exit code differs. So a panic leaves what it left before: a message on
     stderr. The packaged Windows build has no console, so a player sees neither, as before.
     A message the player can see and report (a panic hook writing a file or showing a box) is
     not there with either setting; that would be new work, not part of this step.
   - **Measured** 2026-11-23, Windows PC, background apps open, `perf_app` built once with the
     profile (5 min 43 s; 18.0 MB against 23.8 MB for `release`), headless, 3 runs each
     alternating with the `release` build of the same code:
     - Whole graph at 25%: idle 7.18–7.40 → 6.03–6.25 ms, pan 6.36–6.44 → 5.40–5.44 ms, live
       values 9.20–9.24 → 7.81–7.98 ms.
     - 100%: idle 2.57–2.63 → 2.21–2.25 ms, pan 2.57–2.58 → 2.23–2.44 ms, live values
       3.76–3.84 → 3.26–3.32 ms.
     - About 14–17% less a frame, more than M7's 7–10% for fat LTO alone; `panic = "abort"` was
       not measured apart from it.

Looked at and left out:
- **Integer interned IDs** (the first plan's step 2). They touch four crates and the save
  format's edges to stop the frame cloning and hashing ID strings; step 3 stops the frame
  reading IDs at all, in one file.
- **`FxHashMap` with a clippy rule** (M7 item 2). Hashing was 3% before step 3 takes most
  lookups out of the frame. Back in only if a profile after step 3 still shows it.
- **`slotmap::SecondaryMap` for the `Solution`'s maps.** Indexing in place of hashing, but after
  step 3 they are read once per solve, not per frame.
- **A card's picture kept between frames** (its shapes or mesh, redrawn only when it changes).
  The largest possible gain for the whole-graph view and the most to get wrong (hover, focus,
  tooltips, AccessKit). Its own milestone if step 4 is not enough.
- **Item icons in one texture.** The whole-graph frame is about 1,700 draw primitives, one
  texture switch per icon, and a window costs 2 to 3 ms over headless there. Measure windowed
  after step 4, which changes the count.
- `-C target-cpu=native` (the build must run on players' older CPUs). That reason no longer
  holds: on 2026-11-23 the user said older CPUs are not a goal, no prebuilt binaries are
  released for now (others build from source or use the web version), and a build for the
  user's own CPU is for the user's own use. So M13 step 6 measures it as an opt-in build,
  never the default. Profile-guided
  optimization (much setup for a build nobody downloads yet); `parking_lot` and a single lock
  for `aic-node-graph`'s memory (its locking measured under 0.1 ms for 300 nodes in the M11
  spike); `SmallVec`, inlining attributes, bounds checks and type sizes (no profile points
  there; revisit if step 1 does).

### M13: Assets and start-up
Planned 2026-11-23 ([ADR 0012](adr/0012-names-and-icons-ship-with-the-app.md)). Two aims: the
app starts faster, in the browser and on Windows, and a data version becomes one JSON file, with
the names and icons moved into the app.

Steps 1 to 6 are done (2026-11-23). A cold load in the browser went from 243 files and 7.6 MB
(brotli) to 4 files and 6.4 MB, and the editor showed at 0.6–0.9 s on this PC in place of about
1.4–3.2 s. A Windows start-up went from about 1.25 s to about 0.95 s, all of it from starting
wgpu's DX12 backend alone. Decided by the user (2026-11-23): DX12 alone for now, with no
fallback without `WGPU_BACKEND` (step 6), and the size profile for the web stays out (step 5,
smaller but slower).

**Measured** 2026-11-23 on the Windows PC, as things are before this milestone:

| What | Number |
|---|---|
| Web: fetched before the editor opens | 240 requests, 4.5 MB (238 PNGs, `data.json`, `en.json`) |
| Web: item PNGs that no item of the data uses | 89 files, 1.2 MB |
| Web: the `.wasm`, `release`, before wasm-bindgen (wasm-opt is off in `index.html`) | 17.3 MB; 5.3 MB gzip -9; 3.9 MB brotli -9 |
| Windows: `aic-app.exe`, `release` | 24.2 MB |
| Windows: finding the built-in data, loading the catalog | 1.1–1.5 ms, 8.7–10.0 ms |
| Windows: decoding an icon, on its first draw | 0.23 ms (34–36 ms for the 148 in use, 2.6 MB of PNG) |

Not measured: the window and the renderer on Windows, likely most of its start-up, and anything
in a browser (Trunk and `wasm-bindgen-cli` were not installed).

Decided with the user, 2026-11-23:
- **Nothing loads lazily.** The editor opens when the app, its fonts and every icon are loaded
  and decoded, on the web as on the desktop. A loading page of some seconds is fine; an app that
  fills in after it opens is not.
- **The loading page has no words**: one percentage and one bar under it, over every byte (the
  `.wasm`, which carries the fonts, names and data, and the icon sheet).
- **Files:** `data/v1.5.3.json` and `data/v1.5.3_overlay.json`. The version string stays
  `v1.5.3`, which saves and settings already hold. The term stays "overlay".
- **Hosting:** `target/web/` is uploaded by hand to the user's own server (static files, served
  by Dokploy), behind Cloudflare, which caches and compresses. No CI.
- The tools may be installed with `cargo install`: Trunk, `wasm-bindgen-cli` 0.2.130, `wasm-opt`.

Each step is measured on its own, as in M12, and its numbers and an "As built" note are written
here. A step that gains nothing is dropped. Solver semantics and the golden tests are not
touched.

1. **Tools and a baseline.**
   - Install Trunk, `wasm-bindgen-cli` 0.2.130 (the version in `Cargo.lock`) and `wasm-opt`, by
     hand, once. The build itself still downloads nothing (M9).
   - Windows: the time from the process's start to the end of the first frame, in parts: finding
     and loading the data, the window and the renderer, the fonts, the first frame. Behind an
     environment variable or in `perf_app`, not in a player's way.
   - Web: `scripts/build-web.sh` as it is, served from a local static server: the requests, the
     bytes, and the time until the editor can be used.
   - **Done when:** both are in a table here, 3 runs each.
   - **Done** (2026-11-23). Installed with `cargo install --locked`, from
     source: Trunk 0.22.2, `wasm-bindgen-cli` 0.2.130 and `wasm-opt` 0.133.0 (binaryen version
     133). As built: `PERF_STARTUP=1` on `perf_app` starts the app as `main.rs` does
     (`DataFolder::find`, `App::from_folder` with eframe's storage, vsync on) and prints the
     time from the process's creation (Windows' `GetProcessTimes`) to the start of the second
     frame, which is when the first has been painted and presented, in parts. eframe's storage
     is a folder of `PERF_STORAGE`'s choosing, so it is a first start with an empty graph and a
     player's own window state and last document are never read. After the window closes it
     times, apart, loading the catalog once more and a first pass of egui with its own fonts
     and with the app's. The start-up cloak of `main.rs` is not in it (it is the binary's, not
     the library's); it costs a hook and one call.
   - **Measured** 2026-11-23 on the Windows PC, background apps open, `release` `perf_app`
     of HEAD, 3 runs, run from a folder with no `data/` so the built-in data loads:

     | Windows start-up, part | ms |
     |---|---|
     | Process created to `main` | 30–51 |
     | Finding the data | 1.9–2.2 |
     | Window and renderer (eframe, before the app's creator runs) | 1,092–1,721 |
     | Theme and fonts set | 0.1–0.2 |
     | Data loaded (the catalog alone: 3.3–4.1), editor opened | 12.5–12.7 |
     | First pass begins (egui's fonts loaded) | 2.1–2.9 |
     | First frame's UI built (148 icons decoded on first draw among it) | 60–73 |
     | First frame painted and presented | 92–191 |
     | **Process created to first frame shown** | **1,292–1,947** |

     A first pass of egui alone takes 1.2–1.3 ms with egui's own fonts and 2.0–2.3 ms with the
     app's four. So the window and the renderer are four fifths of a start-up, and its
     run-to-run spread; the rest is under 300 ms.

     | Web, `scripts/build-web.sh` of HEAD (`release`, wasm-opt off) | Files | Raw | brotli -q 9 |
     |---|---|---|---|
     | `index.html` | 1 | 1.5 KB | 0.7 KB |
     | JS glue | 1 | 158 KB | 21.7 KB |
     | `.wasm` (after wasm-bindgen) | 1 | 15.5 MB | 3.65 MB |
     | `data/` files the app fetches (238 PNGs, `data.json`, `en.json`) | 240 | 4.1 MB | 3.9 MB |
     | **A cold load, before the editor opens** | **243** | **19.8 MB** | **7.6 MB** |

     The brotli column compresses each file on its own, as a server would. The PNGs do not
     compress (3.92 MB of the 4.1 MB).
   - **In a browser**, Chrome on the same PC (window 2560×1215 CSS px at 1.5 px per point),
     served from `127.0.0.1` by `python -m http.server` (which sends `.wasm` as
     `application/wasm`). Each of the 3 runs was on a new port, so a new origin: nothing cached,
     no stored settings, an empty graph. Times are from the navigation's start, read from the
     page's Resource Timing and Long Animation Frame entries; over loopback, so they are the
     browser's work and no network's.
     - Requests (the server's log): 244 or 245 GETs: the page, the JS (twice in two runs, once
       for Trunk's `modulepreload` link), the `.wasm`, the 240 data files and a `favicon.ico`
       that is not there (404).
     - The `.wasm` had arrived at 317–349 ms, the last data file at 1.20–1.37 s.
     - The editor's first frame (the last long animation frame, which builds the editor and
       decodes the icons it shows) ended at 3.15 s, 1.40 s and 1.57 s. The first run was the
       first page Chrome loaded after it started.
     - "Usable" is read from long frames, not marked by the app, so it is only as close as the
       frames are long (a frame under 50 ms is not listed).
2. **The new layout** (ADR 0012).
   - `data/<version>.json` is the old `data.json`, unchanged; `data/<version>_overlay.json` the
     old `overlay.json`, same format. A version is every `*.json` in the folder that is not an
     overlay; its name is the file's stem. `--data`, `AIC_DATA_DIR`, `data/` beside the
     executable and in the working directory still take the built-in data's place.
   - The names: `crates/aic-editor/i18n/names/<lang>.json`, beside the Fluent file, in the
     factoriolab format, which only `raw.rs` reads (ground rule 4). One table for every version;
     the languages offered are these files.
   - The icons: `crates/aic-app/assets/icons/{items,facilities,nodes}/<id>.png`, with their
     source and licence notes (the old `SOURCE.md` and `data/README.md`'s licence section) in a
     README beside them. Moved with `git mv`, never fetched.
   - `Catalog` holds no image bytes (`image`, `add_image`, `icons_webp` go). `recipe_image` is
     told which IDs have an icon.
   - The test fixtures (`crates/aic-data/tests/fixtures`, `crates/aic-editor/fixtures`) take
     the same shape. `data/README.md`, sections 3 and 4 of this plan and M8's text follow.
   - **Done when:** the desktop app and its tests run on the new layout with the snapshots
     unchanged, and no file is left under `data/` but the JSON files and the README.
   - **Done, one snapshot changed** (2026-11-23). As built: everything was moved with `git mv`:
     `data/v1.5.3/data.json` to `data/v1.5.3.json`, `i18n/en.json` to
     `crates/aic-editor/i18n/names/en.json`, `images/` to `crates/aic-app/assets/icons/`, and
     `SOURCE.md` to that folder's `README.md`, with the licence note added. `data/` holds
     `README.md` and `v1.5.3.json`.
     - `aic-data`: a version is `<version>.json` minus `*_overlay.json` (`version_of`), on disk
       (`list_versions`, `load(file, names)`) and in a `Bundle` keyed by file name. Every load
       takes a `Names` (the names file's bytes and what errors call it), so the names are the
       caller's; `Catalog::from_bytes(data, names)`. `Catalog` lost `image`, `add_image` and
       `icons_webp`; `recipe_image(id, has_image)` is told which icons there are.
       `LoadWarning::VersionMismatch` names the file. `list_languages` and `Bundle::from_owned`
       went (nothing fetches files any more).
     - `aic-editor`: `build.rs` builds `data/*.json` and `i18n/names/*.json` into the program on
       both targets; its wasm list of paths went. `embedded::names(lang)` and `languages()`.
       `DataFolder::languages()` is the program's tables, whatever the folder; `load` names any
       version, built in or on disk, with them, falling back to English.
     - `aic-app`: a `build.rs` builds `assets/icons/` into a sorted table (`icons::png`,
       `icons::has`), still decoded one by one on first draw as before (step 3 replaces this).
       The icons no longer depend on the data, so a data switch keeps the decoded ones.
     - The web build: with the data and names built in, nothing was left to fetch, so this step
       already took out what step 4 lists: `web.rs`'s `DataDownload`, the app's `Loading` state
       and its strings (`data-loading`, `data-fetch-failed`), and `index.html`'s copy of
       `data/`. Until step 4 the `.wasm` also carries the PNGs. `load-failed-hint-web` now says
       the build has no data in it, the one way the web build can fail to load.
     - Fixtures take the same shape: `crates/aic-data/tests/fixtures/{data,names}/` (with a
       `v1.9.0_overlay.json` and a README that is not a version) and
       `crates/aic-editor/fixtures/satisfactory/{data/vtest.json,names/en.json}`.
       `testing::load(file)` loads a version with the names beside its data folder, else the
       program's.
   - Tests (written before the code): a version is a JSON file that is not an overlay, on disk
     and in a bundle; the overlay beside a version is merged into it and no other; the names
     given name the data, and a bad names table names its file; a data folder on disk takes the
     built-in data's place and is named by the program's names; the languages are the
     program's tables whatever the folder; the icon table is sorted and found by kind and ID.
   - **Snapshots.** Two things changed what fixtures look like, both from the ADR:
     - The program's icons name every data version, so the Satisfactory fixture's `iron_ore`
       took the game's ore icon, and `aic-data`'s test versions (game IDs) took theirs, where
       they had none before (their folders had no `images/`). `aic_app::hide_icons(ctx)` draws
       no icons in one egui context; the snapshot tests on data that is not the game's call it,
       so their pictures stay as they were. The tests on `data/v1.5.3.json` keep the icons.
     - A data folder on disk no longer brings names: `aic-data`'s fixture named three recipes
       the game's table doesn't (`tundra_coupon-copper_nugget` and two more), which now show
       their Chinese names. `settings_data` changed for it: one row of the recipe browser, half
       behind the settings window. Looked at; nothing else in it moved. Every other snapshot is
       unchanged, compared exactly.
     - The golden tests' helper (`aic-core/tests/common`) only changed its call to
       `Catalog::from_bytes`; no test or number changed. The allocation test's ceiling is
       unchanged (it hides the icons too, as its fixture had none).
   - **Measured** 2026-11-23, Windows PC, background apps open, `PERF_STARTUP=1`, 3 runs each
     alternating with step 1's build: data loaded and editor opened 9.5–9.6 → 4.1–5.2 ms (the
     catalog alone 3.3–5.3 → 2.2–2.3 ms: it no longer copies 4.4 MB of PNG). The rest is within
     noise (window and renderer 1,049–1,100 → 1,044–1,185 ms, first UI 52–55 → 55–57 ms).
3. **One icon sheet.**
   - The build packs every icon that some data version uses into one image, each in its own
     cell with a clear gutter around it (a cell drawn smaller than it is takes a little of its
     neighbour otherwise: M11's `golden_07`), and writes an index from kind and ID to cell.
     No side over 2048 pixels, which is all WebGL2 promises; more icons than fit make a second
     sheet. Icons no data version uses stay in the folder and out of the sheet.
   - Both builds load the sheet whole before the first frame of the editor and draw every icon
     from it: one texture in place of one per icon (M12's left-out "item icons in one texture").
     The desktop has it built in; the web fetches it (step 4).
   - **Done when:** every snapshot that shows an icon is looked at (they may change by a pixel's
     blend at an icon's edge, no more), and a whole-graph frame is measured against M12's in a
     window as well as headless.
   - **Done** (2026-11-23). As built: `crates/aic-app/build.rs` packs the sheet, with `aic-data`
     and `image` (PNG only) as build-dependencies. It loads every data version in `data/` (with
     no names) to learn the IDs in use: items and recipes' icons as items, facilities as
     facilities, and every node picture. Cells are 132 pixels: an icon at its own size (at most
     128, none is scaled) in the top-left, with 2 clear pixels on every side. 15 columns, so
     1,980 pixels wide, and 15 rows a sheet at most; past 225 icons a second sheet starts. The
     148 icons in use make one sheet of 1,980 × 1,320, 2.72 MB as PNG at the encoder's best
     compression (3.03 MB at its default). The 89 item icons and the facility icon no data uses
     stay in the folder, out of the sheet. The script also writes the table from kind and ID to
     sheet and rectangle, sorted for a binary search.
     - `icons::install(ctx)` decodes the sheets and uploads one texture each; `main.rs` (desktop
       and web), `perf_app` and the tests on the real data call it before the first frame.
       `icons::get` hands out the texture, the rectangle as UV and the icon's size;
       `ItemPicture`, `image`, `image_atom` draw from it. Nothing is decoded on first use any
       more. Until step 4 the web build has the sheet inside its `.wasm` too.
     - Without `install` there are no icons, which is how the tests on data that is not the
       game's now leave them out; step 2's `hide_icons` went.
     - The best compression takes about 21 s in the build script, which runs only when the
       icons or the data change. Build scripts are never optimized, so the PNG crates are set to
       `opt-level = 3` for `dev` and `release` in the workspace's `Cargo.toml` (about 34 s
       without; the debug tests decode the sheet faster too).
   - Tests: icons are found by kind and ID and kinds don't mix; the icons no data uses are left
     out; the table is sorted; every icon has 2 clear pixels round it and no sheet is over 2,048
     pixels; two icons of one sheet share its texture.
   - **Snapshots.** Every one was made again from nothing, on this PC, for step 2's code (a copy
     of HEAD with step 2's patch) and for this step's, and compared pixel by pixel. Seven changed,
     all on the real data: `card_at_200`, `centered_bodies`, `facility_in_header`, `golden_05`,
     `icons_picker`, `limit_in_the_middle` and `link_labels`, 24 to 769 pixels each, all inside
     icons, each by 1 of 255 in a channel (3 in `limit_in_the_middle`): an icon drawn as a part of
     a larger texture is sampled a hair differently. Each was looked at; no icon is cut or takes
     its neighbour. Three more files differed from the committed ones (`outpost_order`,
     `ratio_lock`, `workspace_card`, by 1 to 28 pixels) but just as much between the committed
     ones and step 2's, made here: noise from where they were made, so the committed files were
     kept.
   - **Measured** 2026-11-23, Windows PC, background apps open, 3 runs each alternating with step
     2's build, whole graph at 25% (frame means; p95 in brackets):
     - Headless: idle 7.18–7.26 → 6.93–6.98 ms (p95 9.34–9.63 → 8.09–8.23), pan 6.19–6.32 →
       6.19–6.21 ms, live values 8.90–8.94 → 8.84–8.93 ms.
     - In a window, vsync off: idle 9.67–10.09 → 9.54–9.68 ms, pan 8.55–8.86 → 8.50–9.01 ms,
       live values 10.19–10.39 → 10.14–10.75 ms. Within the noise.
     - Start-up (`PERF_STARTUP=1`): the sheet is decoded and uploaded in 45–46 ms before the first
       frame, and the first frame's UI went from 55–62 ms to 15 ms. Process to first frame shown
       1,259–1,328 → 1,258–1,267 ms: the same. All 148 icons are decoded now where the first
       frame decoded only those on screen.
4. **The web's loading page.**
   - The data and the names are built into the `.wasm` (about 185 KB), like the fonts. The
     fetch of 240 files, `build.rs`'s list of paths for `wasm32` and the app's own loading
     screen go.
   - `index.html` shows a percentage and a bar from the first byte, and no words. One count
     from 0 to 100% over the `.wasm` and the icon sheet, by bytes. The build writes each file's
     size into the page: Cloudflare's compression hides a download's length. Decoding the sheet
     is the last stretch; the page goes away and the editor shows at 100%, ready.
   - Whether Trunk's loader reports the `.wasm`'s progress is to be checked once it is
     installed. If it does not, the page fetches the `.wasm` itself and counts.
   - A file that fails shows its name and HTTP status on the page: no sentence.
   - **Done when:** a cold load is four requests (the page, its JS, the `.wasm`, the sheet), the
     bar runs once without going back, and the editor never shows an icon's initials for an
     icon that the sheet has.
   - **Done** (2026-11-23). Step 2 had already built the data and names in and taken out the old
     fetch, its list of paths and the app's `Loading` state. As built:
     - **Trunk's loader does report progress**, through a `data-initializer` module's
       `onProgress`, with the built file's size as the total. It was tried and left: the module
       is a fifth file. The page's own loader takes its place (`pattern_script` in
       `Trunk.toml`, with `pattern_preload` empty): the inline script at the end of
       `index.html` starts the `.wasm` and the sheet downloading at once, counting the bytes as
       the browser hands them over (decompressed), and the module script hands the `.wasm` to
       wasm-bindgen as a streamed `Response`, so it still compiles while it arrives.
     - **Sizes.** `scripts/web-assets.sh`, Trunk's `post_build` hook, writes the `.wasm`'s name
       and size and each sheet's into the page (`data-wasm`, `data-sheets`). It finds the sheets
       in `aic-app`'s `OUT_DIR` by running Trunk's own cargo build again, a no-op, with
       `--message-format=json`, and copies them in as `icons-<n>-<16 hex of SHA-256>.png` (`.avif`
       since step 5's AVIF sheet). The desktop has the sheets built in; for `wasm32` `build.rs`
       leaves them out and tells the app how many there are, and `icons::install_sheets` refuses
       another count (and, since step 5, another size).
     - **The page**: a percentage and a bar under it, no words, in the app's colours. One count
       over every byte, which stops at 99% until the app has decoded the sheet and drawn its
       first frame, when it removes the page (`web::remove_loading_page`, from `App::ui`). So
       the editor appears ready, icons and all: `main.rs` awaits the sheets before it starts
       eframe, and installs them before the app exists.
     - **Failures** show the file's name and status, in the bar's red: a sheet or `.wasm` from
       its response, the JS glue or a `.wasm` that doesn't compile by asking the server (a
       `HEAD`), since an import gives no status. A page built without the hook shows its own
       name.
     - The hook is started as `sh -c "exec bash …"`: on Windows a plain `bash` found WSL's in
       System32 first, which doesn't see Trunk's variables.
     - `App::loading` and its Fluent strings went in step 2; no string was added (the page has
       none). The app leaves a `performance.mark("aic-editor-shown")` when the page goes, and
       the page keeps the percentages it showed (`aicLoading.steps`), for measuring.
   - **Measured** 2026-11-23, Chrome on the Windows PC as in step 1, `scripts/build-web.sh`
     (`release`, wasm-opt still off), 3 cold loads each on a new origin:
     - Requests: four, by the page's Resource Timing and by Chrome's own record of the tab: the
       page, the `.wasm`, the sheet and the JS. The server's log had the JS a second time about a
       second later in every run, from outside the page's requests; step 1's build showed the
       same.
     - The bar went up in 43 to 48 steps to 99% and never back. The editor showed, with its
       icons, at 912, 709 and 626 ms after the navigation began (step 1: about 3.15, 1.40 and
       1.57 s, read from long frames). The `.wasm` had arrived at 367–461 ms, the sheet at
       275–300 ms.
     - A missing sheet showed `icons-0-ed87b342e37ebcf8.png 404`; a missing JS glue showed
       `aic-app-26fffc1d72068114.js 404`. Both were checked on copies of the build with the file
       taken out.

     | Web, a cold load | Files | Raw | brotli -q 9 |
     |---|---|---|---|
     | Step 1 | 243 | 19.8 MB | 7.6 MB |
     | Step 4: page 7.5 KB, JS 157 KB, `.wasm` 15.7 MB, sheet 2.72 MB | 4 | 18.6 MB | 6.4 MB |

     The `.wasm` grew by 141 KB raw (the data and the names; 5 KB with brotli). The sheet,
     already PNG, gains nothing from brotli.
5. **A smaller download, and caching.**
   - wasm-opt on, with the installed binary. A profile for the web that favours size
     (`opt-level`, fat LTO, one codegen unit, `panic = "abort"`), kept only if a frame in the
     browser is not slower for it: measure both.
   - The four font files (1.7 MB, two of them tabular copies of the other two): what can go
     without changing a glyph on screen.
   - Every file but `index.html` has a hash of its content in its name, the icon sheet
     included, so a new upload is never served stale from a cache.
   - A deploy note in the header of `scripts/build-web.sh`: what to upload, that `.wasm` must
     be served as `application/wasm`, and what to check at Cloudflare (that it caches `.wasm`,
     which may need a cache rule, and compresses it).
   - **Done when:** the bytes of a cold load, compressed as Cloudflare would (brotli), are
     written here against step 1's.
   - **Done** (2026-11-23). As built: `index.html` asks for wasm-opt `-O2` (`data-wasm-opt="2"`)
     and `Trunk.toml` names the installed binaryen (`wasm_opt = "version_133"`), which Trunk then
     uses; with `TRUNK_OFFLINE=true` a mismatch would fail rather than download. Trunk already
     names the JS and the `.wasm` by a hash of their content, and step 4's hook the sheet; only
     `index.html` keeps its name. The deploy note is in `scripts/build-web.sh`'s header.
   - **Candidates**, each a whole site built with Trunk (`release` + wasm-opt `-O2` unless said),
     the profile and flags set through Cargo's environment variables in a target folder of their
     own, so nothing of a dropped one is in the repository:

     | Build | `.wasm` raw | `.wasm` brotli -q 9 | Frame, mean of 600 (3 runs) |
     |---|---|---|---|
     | Step 4: `release`, wasm-opt off | 15.67 MB | 3.656 MB | 4.60–4.78 ms |
     | wasm-opt `-O2` (kept) | 11.86 MB | 3.638 MB | 4.39–4.58 ms |
     | `-Os` / `-Oz`, by hand on step 4's `.wasm` | 11.73 / 11.56 MB | 3.601 / 3.602 MB | not run |
     | Size profile: `opt-level = "s"`, fat LTO, one codegen unit | 9.47 MB | 3.081 MB | 4.68–4.78 ms |
     | `-C target-feature=+simd128` (wasm32 only) | 11.64 MB | 3.590 MB | 4.34–4.67 ms |

     - The frame: Chrome on this PC, the four builds served from one origin so they open the same
       graph from the browser's storage: 25 recipe cards added from the recipe browser, which lay
       them on one spot in the middle, at 100%, with both side panels. A script wraps
       `requestAnimationFrame` to time eframe's callback (its main-thread work: UI, tessellation
       and WebGL calls), scrolls the canvas with Shift+wheel every frame for 720 frames (turning
       every 60, so the view ends where it began) and keeps the last 600. Three rounds,
       alternating the four builds. Chrome rounds `performance.now()` to 0.1 ms here. A light
       frame, so it shows the code's speed, not a whole graph's.
     - **wasm-opt `-O2`** is kept: 24% smaller raw, a little faster. Its brotli size barely
       moves: most of what it takes out compressed well anyway.
     - **The size profile** is left out: 0.56 MB less compressed, but its frames were slower than
       `-O2`'s in every round (4.68–4.78 against 4.39–4.58 ms), and the plan keeps it only if they
       are not. `panic = "abort"` is wasm32's default already. The user confirmed leaving it out
       (2026-11-23); worth another look if download size comes to matter more than 3–5% of a
       frame.
     - **wasm SIMD** is left off: 0.05 MB less compressed, and its frames (4.34–4.67 ms) overlap
       `-O2`'s, so it is not shown faster.
     - **Fonts: nothing changed.** The four files are 1.66 MB raw and 712 KB with brotli one by
       one, but inside the `.wasm` they lie next to each other, and the two tabular cuts are near
       copies of the others: all four compress to 285 KB together against 279 KB for the two
       originals, so the tabular cuts cost about 6 KB of a download. What a renderer never reads
       (glyph names in `post`, 33 KB a file; `GSUB`, 24 KB, which egui does not apply) could go
       without changing a glyph, for a few tens of KB compressed; that needs a font tool
       (fonttools), which this session could not install, so it is not done.
   - **Measured**, a cold load, raw and with brotli -q 9 a file:

     | Web, a cold load | Files | Raw | brotli -q 9 |
     |---|---|---|---|
     | Step 1 | 243 | 19.8 MB | 7.60 MB |
     | Step 5: page 7.5 KB, JS 157 KB, `.wasm` 11.86 MB, sheet 2.72 MB | 4 | 14.7 MB | 6.39 MB |

     The sheet is now 43% of the compressed bytes: it is PNG, which brotli doesn't shrink (the
     step-1 icons were PNG too). A smaller encoding of the sheet would be the next saving.
   - **The web sheet as AVIF** (2026-11-23, asked for by the user as "AVIF at 80% quality"). As
     built:
     - `aic-app` has a feature, `avif-sheets`, that adds `ravif` 0.13 (rav1e, pure Rust, without
       its `asm`, which needs nasm) as an optional build-dependency. `index.html` asks Trunk for
       it (`data-cargo-features`) and `web-assets.sh` builds with the page's features to find the
       same `OUT_DIR`. For `wasm32` with the feature, `build.rs` writes `icons-<n>.avif` and no
       PNG; without it (a plain `cargo check --target wasm32-unknown-unknown`) no sheet at all.
       The desktop never compiles the encoder and still builds in its lossless PNG. rav1e and
       ravif are `opt-level = 3` in the workspace's profiles, as the PNG crates are.
     - The setting: ravif quality 87 for colour and alpha, speed 4, 4:4:4 (ravif's only chroma),
       unassociated alpha with the colour of clear pixels cleaned. ravif's scale is not
       libaom's: its 80 gave 38.2 dB at 421 KB, below what the user had seen of libaom's q80
       (41.4 dB, 629 KB, with Pillow); 87 gives the same outcome, 41.5 dB PSNR on premultiplied
       RGBA against the PNG, at 598 KB. The encode takes about 13 s in the build script, which
       runs only when the icons or the data change.
     - The loading page decodes the sheet, as the browser can and an AV1 decoder in the `.wasm`
       would cost more than it saves: `createImageBitmap` (no premultiplying, no colour
       conversion), an `OffscreenCanvas` and `getImageData`, and hands `{ width, height, rgba }`
       to the app (`web::icon_sheets`). `icons::install_sheets` takes pixels on both builds and
       refuses a count or size other than the build's (`SHEET_SIZES` in the generated table);
       the desktop decodes its PNG first (`icons::install`). A sheet that downloads but doesn't
       decode shows its name, as a failed download shows its name and status. No PNG fallback:
       old browsers are not a target.
     - The `.wasm` lost the PNG decoder (94 KB raw); the PNG encoder stays, for exporting the
       canvas as an image.
     - Tests (written before the code): the built sheets' pixels are installed; another count of
       sheets, a sheet of another size, and pixels short of their sheet are refused. The
       snapshots, all on the desktop, did not change.
   - **Measured** 2026-11-23 on the Windows PC:

     | Web, a cold load | Files | Raw | brotli -q 9 |
     |---|---|---|---|
     | Before: page 7.8 KB, JS 157 KB, `.wasm` 11.86 MB, sheet (PNG) 2.72 MB | 4 | 14.75 MB | 6.36 MB |
     | After: page 9.1 KB, JS 158 KB, `.wasm` 11.76 MB, sheet (AVIF) 0.61 MB | 4 | 12.54 MB | 4.21 MB |

     - A third less to download with brotli (2.15 MB less: 2.11 MB of sheet, 35 KB of `.wasm`).
     - The browser's decoding of the AVIF against the PNG: 41.51 dB, the same as the encoder's
       own check; where the PNG is clear, the AVIF's alpha reaches at most 12 of 255 (5 in the
       gutters between cells). The recipe list's icons on screen differed from the PNG build's
       by at most 24 of 255 in a channel, inside icons only; magnified, no icon showed a
       difference to the eye and none took anything from its neighbour.
     - Time to `aic-editor-shown`, Chrome 3 alternating cold loads each, over loopback: before
       707, 706, 599 ms; after 786, 718, 642 ms. Over loopback the bytes cost nothing and the
       decode is the browser's (about 108 ms for the sheet, partly while the `.wasm` compiles),
       so it is 10–80 ms slower there. On an emulated 50 Mbit/s link (20 ms latency, files
       served uncompressed): before 3,100, 3,076, 3,053 ms; after 2,675, 2,659, 2,685 ms.
       Headless Chrome (`--headless=new`, driven over its DevTools protocol, a new browser
       context a load), not a window: no Windows browser was connected to the session's tools.
     - Clean builds, each in an empty target folder, one run each: web (`build-web.sh`) 282 →
       223 s, likely (not timed apart) because the wasm32 build no longer writes the PNG at its
       best compression (about 21 s) and rav1e compiles beside the rest, and from noise; desktop `cargo build --release -p aic-app` 199 →
       195 s, the same 222 crates (rav1e and ravif are not among them).
6. **Windows, from step 1's numbers.** Candidates, each only if the baseline points at it:
   which renderer and backend eframe starts (the lock file has both wgpu and glow), a faster
   encoding for the built-in sheet than PNG, the size of the `.exe`. Measured with the
   `packaged` profile as well.
   - Added 2026-11-23 at the user's word: older CPUs are not a goal, so **a build for this
     machine's CPU** (`-C target-cpu=native`) is a candidate too, opt-in only, never the default
     of `cargo build --release`, never in `scripts/package-*`, never for wasm.
   - **Done** (2026-11-23). Step 1 points at the window and the renderer: four fifths of a
     start-up. Decoding the sheet (45–47 ms since step 3) and the time to `main` (29–31 ms) are
     small beside it, so the sheet's encoding and the `.exe`'s size were not tried.
   - **The backend.** eframe 0.36 starts wgpu only (glow is not built in), and egui-wgpu asks
     for every backend: Vulkan, DX12 and GL. wgpu reads `WGPU_BACKEND`, so each was timed on one
     binary, `PERF_STARTUP=1`, 3 alternating rounds:

     | Backends | Window and renderer | Process to first frame |
     |---|---|---|
     | All (the default) | 1,051–1,440 ms | 1,259–1,674 ms |
     | DX12 | 758–865 ms | 945–1,069 ms |
     | Vulkan | 843–879 ms | 1,038–1,078 ms |
     | DX12 and Vulkan | 928–965 ms | 1,120–1,163 ms |
     | GL | 354–540 ms | 570–781 ms |

     GL starts fastest but draws slower: in a window, whole graph at 25%, vsync off, 3 rounds,
     idle 13.8–14.2 ms against 10.0–10.1 ms for the default and 9.8–10.0 ms for DX12 (pan and
     live values the same way), and its first paint is slower (117–142 ms against about 100).
     So it is out. DX12 alone draws as the default does.
   - As built: `aic_app::native_options(title)`, which `main.rs` and `perf_app` (its start-up
     and its windowed run) both use, sets wgpu's backends on Windows to
     `Backends::from_env().unwrap_or(DX12)`: DX12, unless `WGPU_BACKEND` names others.
   - **Measured** 2026-11-23, background apps open, 3 runs alternating with the build before:
     process to first frame 1,245–1,307 → 951–1,193 ms (`release`; the window and renderer
     1,044–1,092 → 757–953 ms). With `--profile packaged` (one binary, `WGPU_BACKEND=dx12`
     standing in for the change): 1,231–1,519 → 946–1,084 ms. The `packaged` `.exe` is 17.8 MB
     against 23.7 MB and reaches `main` in the same 29–31 ms.
   - **DX12 alone for now** (the user's decision, 2026-11-23). It leaves no fallback on a PC
     whose DX12 doesn't work (a GPU or driver without it); winit can't be started twice in one process, so the app can't retry
     another backend after a failure. Such a player sets `WGPU_BACKEND=vulkan` (or `gl`).
     DX12 and Vulkan together would keep a fallback but give back about 190 ms of the 300 ms
     saved.
   - **`-C target-cpu=native`**: `release` built with and without it, `perf_app` headless, 3
     alternating runs (frame means):
     - Whole graph at 25%: idle 6.88–7.10 → 6.76–7.05 ms, pan 6.12–6.23 → 6.00–6.17 ms, live
       values 8.85–8.98 → 8.59–8.70 ms.
     - 100%: idle 2.57–2.67 → 2.48–2.53 ms, pan 2.61–2.66 → 2.48–2.52 ms, live values
       3.79–3.81 → 3.68–3.70 ms.
     - The 200-node section (`PERF_GRAPH=chain PERF_NODES=200`, live values): solve 25.03–25.18
       → 25.06–25.25 ms, the frame 33.0–33.6 → 32.9–33.2 ms: the same.
     - So 2–5% on drawing, nothing on the solve. Kept as opt-in only:
       `scripts/build-for-this-cpu.ps1` builds `--profile packaged` with it into
       `target\native\`, for the user's own PC; it runs in Windows PowerShell 5.1 too (this PC
       has no `pwsh`). `packaged` and `native` together were not timed; each was measured
       against `release`.

Looked at and left out:
- **Pre-compressed files** (`.br`, `.gz`) in the build: Cloudflare compresses.
- **A service worker** for offline use: its own milestone if wanted.
- **Loading the icons after the editor opens**, or each on first use: decided against, above.
- **Names in Fluent**: the game's names stay in the format they come in.

### M14: Gas environments
Planned 2026-11-25; not started. Needs an ADR and the glossary term (**Gas environment**, added in
M8) before code.

What the game does (v1.6.0 tables):

- The Gas Dispersing Unit (`vaporizer_1`) takes 6 a minute of one of four gases, Inergen
  (`gas_inert`), Aquagen (`gas_water`), Acridgen (`gas_acid`) or Xiragen (`gas_xiranite`), and sets
  that gas's environment around itself (`FactoryVaporizerTable`: `genEnv` 1 to 4,
  `rangeExtend` 5 × 5 × 5). Taking `consumeRate` as per minute is M8's assumption.
- Recipes that need an environment (`gasEnv` of the craft), with their gas:
  - `gas_reactor_activity_copper_poly_gas_1`, Proto Xiran-Cuprium Gas Production (Gas Reactor
    Globe): Inergen;
  - `gas_reactor_gas_copper_enr2_1`, Pyrrolite Gas Production (Gas Reactor Globe): Acridgen;
  - `liquid_purifier_gas_xiranite_enr_2`, Heavy Xiragen Purification (Purification Unit): Inergen;
  - `liquid_purifier_gas_copper_enr_2`, Hetonite Gas Purification (Purification Unit): Inergen;
  - `xiranite_oven_xiranite_powder_2`, Xiranite Production (Forge of the Sky): Inergen.
  The two purifier recipes also exist without an environment, where they take 2 Filter Cores a
  craft in place of 1.
- The facility modes the game flags as environment modes (`FactoryMachineCrafterTable`,
  `isEnvMode`): the Gas Reactor Globe's gas mode, the Purification Unit's gas mode and the Forge of
  the Sky's gas & fluid mode.

What M8 already put in the data: `Recipe::environment` (the gas's item ID, from the file's
`environment`), and the unit's four recipes, `vaporizer_1-<gas>`, each 6 of its gas over 60 s and
nothing out. The solver and the UI ignore `environment` today: those five recipes run as if the
environment were there.

Open questions:

- An environment is not a flow. The recipe doesn't consume it, and one unit serves every facility
  in its range, which is spatial (ADR 0002 keeps layout out). So it can't be a port or a link.
- Candidates: a badge on the card naming the gas a recipe needs, and a warning when a graph has such
  a recipe and no dispersing unit of that gas anywhere in it (the solver's warnings, as for regions).
  The unit's 6/min would be an input every running unit needs in full, whatever it serves: an
  activation input of whole units (ADR 0013's machinery) of which nothing scales.
- Whether a graph should say how many units it needs (it can't know the layout), or leave the count
  to the player.

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

Both questions here were answered by the game's tables in M8:

- Belt and pipe throughput: 30 and 120 per minute (`FactoryGridBeltTable`: one item per 2000 ms;
  `FactoryLiquidPipeTable`: one unit per 500 ms).
- Higher-tier miners are not faster: all four mining rigs give 1 per 3 s (`FactoryMinerTable`).
