# Game data

Each JSON file here is one **data version**, named after the game version it matches
(`v1.6.0.json`). The app takes every `*.json` file that is not an overlay as a version and uses the
newest one by default (ADR 0012).

```
data/
├─ README.md                  # this file: the format, the sources, the license note
├─ <version>.json             # e.g. v1.6.0.json: items, facilities, recipes, regions
├─ <version>_overlay.json     # optional: our own changes on top of <version>.json
├─ v1.5.3.json                # converted once from factoriolab's data (see Sources)
└─ v1.6.0.json                # written by aic-extract from the game's tables (the default)
```

A version holds no names and no pictures. They are the app's own, one set for every version, keyed
by the same game IDs:

- names: `crates/aic-editor/i18n/names/<lang>.json` (the languages a player can choose are these
  files); `items` names facilities too;
- icons: `crates/aic-app/assets/icons/{items,facilities,nodes}/<id>.png`, with their sources in the
  README there.

A version that adds an item needs its name and icon added there, or the item shows as its ID and
its initials. `aic-extract` adds the English names; icons are copied in by hand.

Data is copied in by hand. The app never downloads it, and the build never fetches it.

Every version here is built into the app (`crates/aic-editor/build.rs`), on the desktop and the
web alike, so a packaged app (`scripts/package-macos.sh`, `scripts/package-windows.ps1`) needs no
`data/` folder beside it. A data folder found on disk still takes the built-in data's place on the
desktop: the `--data` flag, `AIC_DATA_DIR`, `data/` next to the executable, then `data/` in the
working directory. It brings versions only; the names and icons stay the app's.

## The format (format 1)

Our own (ADR 0014), defined once in `crates/aic-data/src/file.rs`; `aic-extract` writes it through
the same types. IDs and numbers only. Unknown keys are errors everywhere, so a typo doesn't silently
do nothing; every error names the file and the path in it.

```jsonc
{
  "format": 1,                         // required; any other number is an error naming it
  "version": "v1.6.0",                 // optional; the file's name wins (a mismatch is a warning)
  "source": "free text",               // optional, ignored
  "belt_per_min": 30,                  // required, positive: items per minute on one belt
  "pipe_per_min": 120,                 // required, positive: units per minute in one pipe
  "regions": ["tundra", "jinlong"],
  "categories": ["type_1", "gen-power"],   // the recipe browser's categories, in its order
  "items": [
    { "id": "copper_nugget", "phase": "solid", "color": "#c87533" }
    // phase: solid | liquid | gas, default solid (liquids and gases go through pipes);
    // color: optional, for port dots and an item's initials when it has no icon
  ],
  "facilities": [
    { "id": "phase_trans_2", "speed": 1, "power_kw": 50, "regions": ["jinlong"],
      "activation": { "item": "gas_xiranite", "per_min": 6 } }
    // speed default 1; power_kw default 0; regions default [] = every region;
    // activation optional (ADR 0013): what each running facility needs whatever its throughput
  ],
  "recipes": [
    { "id": "furnance_copper_nugget_1", "category": "type_4", "time_s": 2,
      "in": { "copper_ore": 1 }, "out": { "copper_nugget": 1 },
      "facilities": ["furnance_1"],
      "power_kw": -220,                // optional: replaces the facility's power; negative = generation
      "kind": "mining",                // optional: "mining" | "trade"
      "hidden": true,                  // optional, default false: left out of the recipe browser
      "regions": ["jinlong"],          // default [] = every region
      "environment": "gas_inert" }     // optional: the gas environment the recipe needs
    // in/out: quantity per craft, in order; the first output is the main product.
    // facilities: never empty; the first is the default.
  ],
  "aliases": {                         // optional: an older version's IDs for this one's
    "items": { "tundra_coupon": "domain_tundra_coupon" },
    "recipes": { "copper_nugget": "furnance_copper_nugget_1" }
  }
}
```

- **Activation inputs are not in a recipe's `in`.** As in the game's tables, a facility's
  `activation` comes on top of what its recipes list; loading adds `per_min × time_s / (60 × speed)`
  of the item to every recipe of the facility (to the recipe's own entry for the item if it has one,
  else last). Facilities of one recipe must agree on their activation input, and on their speed if
  they have one.
- A **trade** (`"kind": "trade"`) takes exactly one item and gives at least one (the coupons); it is
  always hidden and is placed as an outpost order. A **mining** recipe's outputs are the region's raw
  resources; it may take an input too (a rig that needs water).
- An `environment` must be an item. Nothing uses it yet (`docs/PLAN.md`, Gas environments).
- An **alias** goes from an ID this version doesn't have to one it does. Opening a graph and
  switching data follow them (`Graph::rebind`), so graphs saved on an older version open on this one.
- An **overlay** (`<version>_overlay.json`) changes the file before it is checked, so its entries get
  the same checks: `{ "items": { "add": [...], "replace": [...], "remove": ["id"] }, "facilities":
  { same }, "recipes": { same } }`, with entries written as in the file. Adding an ID that exists, or
  replacing or removing one that doesn't, is an error naming the overlay. A `replace` is the whole
  entry. No version has one at the moment.

## Sources

- `v1.6.0.json`: written by `aic-extract` from the tables of the game client v1.6.0, which the user
  exported and copied in by hand (the exported tables are not in this repository):

  ```sh
  cargo run -p aic-extract -- <export>/game/Table --version v1.6.0 --previous data/v1.5.3.json
  ```

  It writes `data/<version>.json`, adds the English names the data needs to
  `crates/aic-editor/i18n/names/en.json`, and prints a report: counts, names that differ from the
  game's, outposts that disagree on a price, and the previous version's
  recipes that got no alias. `--previous` takes an older version in this format: its item colours
  are kept and its IDs get aliases. `--data-dir`, `--names` and `--lang` (`EN`) change where it
  writes and which text table it reads. The run checks that what it wrote loads.

  A name the table already has stays as it is, with two exceptions. A facility that is a building
  of the game takes the game's name for it (v1.6.0 renamed the two sewage gates to "Sewage Inlet"
  and "Byproduct Outlet"). And a recipe that fills a container with a fluid, or empties it again,
  is named `Packaged <fluid> (<container>)` or `Unpackage <fluid> (<container>)`, because the
  game's names don't tell them apart (every emptying of an Amethyst Bottle is "Amethyst Bottle
  Production"). Any other recipes the game gives one name are told apart in brackets, by the first
  input the others don't all take ("Carbon Powder Production (Buckflower Powder)") or, where they
  take the same items, by the gas whose environment they need ("Hetonite Gas Purification
  (Inergen)"); these names replace the table's too, and the report lists any name still shared.
  Other names of our own making, such as an outpost's, are only added where the table has none.

  The recipes are in the order the recipe browser lists them. With `--previous`, a recipe that
  version had keeps its place, a new one goes right after the last recipe that makes the same
  main product, and the rest follow in the game's order (by building, then its `sortId`).
  Fillings, emptyings and trades always come last.

  Where the tables leave something open, the extractor assumes (each a named constant in
  `crates/aic-extract/src/extract.rs`):

  - **Natural liquids.** A pump's `enableLiquidIds` lists every liquid it can lift, also the ones
    a player makes and keeps in a tank. Only Clean Water and Precipitation Acid (`liquid_water`,
    `liquid_acid`) are found in the world, so only they get mining recipes.
  - **The Gas Dispersing Unit's `consumeRate` is per minute**, as the transmuting units' is
    (their 6 matches the 6/min of ADR 0013). Its recipes take 6 of a gas over 60 s and make nothing.
  - **The sewage outlet's time.** The table gives 30 sewage in, 1 Xircon Effluent out, and no time.
    The plant (`FactorySewageTreatPlantStoreTable`) has three inlet levels and then the outlet;
    the outlet is taken to work up what the three inlets bring in at 2 a second each, so a craft
    takes 30 / 6 = 5 s, as the old data had.
  - **A trade takes 2 s.** Outposts are not timed; only coupons per item matter.
  - Recipes the extractor doesn't write: cross-region transfers (the app hides them), and anything of
    farmland, soil or battle buildings.
- `v1.5.3.json`: converted once (2026-11-25) from endfield-calc/factoriolab, `src/data/aef/data.json`,
  to this format, with the transmuting units' activation inputs from its former overlay taken out
  of their recipes and put on the facilities. It keeps factoriolab's IDs. Its English names
  (`src/data/aef/i18n/en.json`) were the start of `crates/aic-editor/i18n/names/en.json`; the icons
  came from jambochen/endfield-calc (see `crates/aic-app/assets/icons/README.md`).

## License

The game data, names and icons (names, recipes, numbers and images from *Arknights: Endfield*)
belong to Hypergryph / Gryphline. They are **not** covered by this repository's code license
(MIT). They are included for reference so the planner can work, as other community
tools do. If you are a rights holder and want something removed, open an issue.
