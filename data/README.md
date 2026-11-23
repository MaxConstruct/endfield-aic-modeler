# Game data

Each JSON file here is one **data version**, named after the game version it matches
(`v1.5.3.json`). The app takes every `*.json` file that is not an overlay as a version and uses the
newest one by default (ADR 0012).

```
data/
├─ README.md                  # this file
├─ <version>.json             # e.g. v1.5.3.json: items, facilities, recipes, icon colours, regions
├─ <version>_overlay.json     # optional: our own patches on top of <version>.json
├─ v1.5.3.json
└─ v1.5.3_overlay.json        # the transmuting units' activation inputs (ADR 0013)
```

The files use the [factoriolab](https://github.com/endfield-calc/factoriolab) format. Only
`crates/aic-data/src/raw.rs` knows that format; the rest of the app uses the normalized `Catalog`.
An overlay's format is in `docs/PLAN.md` (M7): `items` and `recipes`, each with `add`, `replace`
and `remove`, `facility_speeds`, and `facility_activation`, a facility's activation input (an item
each running facility needs at a fixed rate, whatever its throughput; ADR 0013). The factoriolab
data folds that input into each of the facility's recipes as an ordinary input, so the data file
stays as copied and the overlay says which input it is.

A version holds no names and no pictures. They are the app's own, one set for every version, keyed
by the same game IDs:

- names: `crates/aic-editor/i18n/names/<lang>.json` (the languages a player can choose are these
  files);
- icons: `crates/aic-app/assets/icons/{items,facilities,nodes}/<id>.png`, with their sources in the
  README there.

A version that adds an item needs its name and icon added there, or the item shows as its ID and
its initials.

Data is copied in by hand. The app never downloads it, and the build never fetches it.

Every version here is built into the app (`crates/aic-editor/build.rs`), on the desktop and the
web alike, so a packaged app (`scripts/package-macos.sh`, `scripts/package-windows.ps1`) needs no
`data/` folder beside it. A data folder found on disk still takes the built-in data's place on the
desktop: the `--data` flag, `AIC_DATA_DIR`, `data/` next to the executable, then `data/` in the
working directory. It brings versions only; the names and icons stay the app's.

## Sources

- `v1.5.3_overlay.json`: ours. 6/min of Xiragen (`gas_xiranite`) for each running Solid-Gas
  Transmuting Unit (`phase_trans_2`) and 6/min of Liquid Xiranite (`liquid_xiranite`) for each
  running Fluid-Gas Transmuting Unit (`phase_trans_1`), whatever their throughput.
- `v1.5.3.json`: copied from endfield-calc/factoriolab, `src/data/aef/data.json`. Its English names
  (`src/data/aef/i18n/en.json`) are `crates/aic-editor/i18n/names/en.json`; the icons came from
  jambochen/endfield-calc (see `crates/aic-app/assets/icons/README.md`).

## License

The game data, names and icons (names, recipes, numbers and images from *Arknights: Endfield*)
belong to Hypergryph / Gryphline. They are **not** covered by this repository's code license
(MIT). They are included for reference so the planner can work, as other community
tools do. If you are a rights holder and want something removed, open an issue.
