# The app's icons

One set for every data version, keyed by game ID (ADR 0012), built into the app by
`crates/aic-app/build.rs`:

```
icons/
├─ items/<item id>.png
├─ facilities/<facility id>.png
└─ nodes/source.png, sink.png   # built-in node kinds (the depot unloader and loader)
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
  assumed, not checked). `vaporizer_1` matches no facility and is unused.
- `nodes/source.png` and `sink.png`: jambochen's `unloader_1` (Depot Unloader) and `loader_1`
  (Depot Loader), used for source and sink nodes.
- Items renamed: `fbottle_{copper,iron}_grass_{1,2}` →
  `{copper,iron}_bottle-liquid_plant_grass_{1,2}`.
- No icon yet: the mining rigs (`miner_2`, `miner_3`, `miner_4`, `__miner_pump_1`), outposts,
  `__domain_transfer`, coupons, `belt`, `pipe`. The app draws initials for those.

## License

The icons are images from *Arknights: Endfield* and belong to Hypergryph / Gryphline. They are
**not** covered by this repository's code license (MIT). They are included for
reference so the planner can work, as other community tools do. If you are a rights holder and want
something removed, open an issue.
