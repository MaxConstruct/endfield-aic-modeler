# The app's icons

One set for every data version, keyed by game ID (ADR 0012), built into the app by
`crates/aic-app/build.rs`:

```
icons/
├─ items/<item id>.png
├─ facilities/<facility id>.png
└─ nodes/<name>.png             # built-in node kinds: source, sink, splitter, merger, storage, control port
```

An item or facility with no file here is drawn as a coloured square with its initials. Icons that
no data version uses stay here and do no harm.

Moved here from `data/v1.5.3/images/` in M13 (`docs/PLAN.md`); this note was that folder's
`SOURCE.md`.

## Where they came from

PNG icons copied from [jambochen/endfield-calc](https://github.com/jambochen/endfield-calc), then
renamed to our IDs (2026-11-09):

- `items/<item id>.png`: jambochen's item icons (their folder was `recipes`). About 90 files are
  jambochen-only items (bottle and jar variants, `plant_sp_*`, …) that data v1.5.3 doesn't have;
  they are unused.
- `facilities/<facility id>.png`. Renamed from jambochen's names: `component_mc_1` → `cmpt_mc_1`,
  `filling_powder_mc_1` → `filling_pd_mc_1`, `liquid_clean_gate_1` → `liquid_clean_gate`,
  `liquid_recycle_gate_1` → `liquid_recycle_gate`, `power_station_1` → `power_sta_1`,
  `seedcollector_1` → `seedcol_1`, `tools_assebling_mc_1` → `tools_asm_mc_1`, `transmuter_1` →
  `phase_trans_1`, `transmuter_2` → `phase_trans_2` (the order of the two transmuting units is
  the game's: `FactoryBuildingItemTable` places `transmuter_1` with `item_port_phase_trans_1`).
  `vaporizer_1` (the Gas Dispersing Unit) is used since data v1.6.0.
- `nodes/source.png` and `sink.png`: jambochen's `unloader_1` (Depot Unloader) and `loader_1`
  (Depot Loader), used for source and sink nodes.
- `nodes/splitter.png`, `merger.png` and `control_port.png` (the belt's Splitter, Converger and
  Item Control Port), `splitter_pipe.png`, `merger_pipe.png` and `control_port_pipe.png` (the
  pipe's), `storage.png` (Protocol Stash), `storage_liquid.png` (Fluid Tank) and `storage_gas.png`
  (Gas Tank): added by hand on 2026-11-25 as 256 × 256 PNGs and scaled to 125 × 125, since the
  sheet takes icons up to 128 pixels. A node's card and the node picker show the one for what the
  node carries: the pipe's parts and the tanks for a liquid or a gas, the belt's for a solid or
  while it carries nothing yet. A workspace has no picture: the picker shows the glyph of its card.
- Items renamed: `fbottle_{copper,iron}_grass_{1,2}` →
  `{copper,iron}_bottle-liquid_plant_grass_{1,2}`.
- Data v1.6.0 (M8, ADR 0014) calls those four items by the game's IDs again, so each is also here
  under `fbottle_{copper,iron}_grass_{1,2}.png`, a copy (2026-11-25): v1.5.3 still uses the old IDs.
  Its other new items (bottle and canister variants, `plant_sp_*`, …) use the jambochen-only icons
  that were unused before.
- No icon yet: the mining rigs (`miner_1` to `miner_4`, `__miner_pump_1`), outposts,
  `__domain_transfer`, coupons (`tundra_coupon`, `domain_tundra_coupon`, …), and 35 more items new
  in data v1.6.0 (crops and their seeds, ingredients, ranch and inn goods). The app draws initials
  for those.

## License

The icons are images from *Arknights: Endfield* and belong to Hypergryph / Gryphline. They are
**not** covered by this repository's code license (MIT). They are included for
reference so the planner can work, as other community tools do. If you are a rights holder and want
something removed, open an issue.
