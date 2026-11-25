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

Almost every icon is the game's own picture as the [Endfield Warfarin Wiki](https://warfarin.wiki)
shows it. Its Items and Facilities pages were saved by hand from a browser on 2026-11-26, and the
icons were converted from the WebP images of those saved pages; nothing was fetched by a script.
That replaced the earlier set (PNGs from
[jambochen/endfield-calc](https://github.com/jambochen/endfield-calc), about 100 pixels and often
not square) wherever the wiki has the picture, so the set has one source, one style and one size.

- `items/<item id>.png`: the game's item `item_<item id>` has an `iconId` in `ItemTable`, and the
  wiki's file is `<iconId>.webp`. Each was checked against the saved Items page, which shows that
  same file for the item. The game gives every fill of one bottle or canister the same picture
  (`fbottle_copper_*` all show `item_copper_bottle_liquid`, every `gasjar_copper_gas_*` shows
  `item_copper_jar_gas`), so those icons are identical, as they are in the game.
- `facilities/<facility id>.png`: a facility's ID is its building's item ID without `item_port_`
  (`aic-extract`), and `FactoryBuildingItemTable` lists those items, so a facility shows
  `item_port_<facility id>.webp` (`cmpt_mc_1` is the building `component_mc_1`, `phase_trans_1` the
  building `transmuter_1`, and so on). The saved Facilities page links each picture to its
  building, which agrees.
- `nodes/source.png` and `sink.png` are the Depot Unloader (`item_port_unloader_1`) and Depot
  Loader (`item_port_loader_1`); `storage.png`, `storage_liquid.png` and `storage_gas.png` are the
  Protocol Stash (`item_port_storager_1`), Fluid Tank (`item_port_liquid_storager_1`) and Gas Tank
  (`item_port_gas_storager_1`). A node's card and the node picker show the one for what the node
  carries: the pipe's parts and the tanks for a liquid or a gas, the belt's for a solid or while it
  carries nothing yet. A workspace has no picture: the picker shows the glyph of its card.
- IDs that are aliases of each other across data versions each have a file with the same picture:
  `fbottle_{copper,iron}_grass_{1,2}` and v1.5.3's `{copper,iron}_bottle-liquid_plant_grass_{1,2}`,
  `domain_{tundra,jinlong}_coupon` and v1.5.3's `{tundra,jinlong}_coupon`, `liquid_water` and
  v1.5.3's `__miner_water`.
- Conversion: the wiki's pictures are square RGBA, 254 to 260 pixels. Each became a 128 × 128 RGBA
  PNG, scaled down with Lanczos in premultiplied alpha so the transparent edges stay clean (a
  picture that wasn't square would have been padded to a square with transparency, centred; none
  was). 128 pixels is the most `build.rs` takes (`ICON`): a facility picture, the largest the app
  draws, is 64 points on a card at 100% zoom, so 128 pixels are sharp there on a screen at 2× and at
  200% zoom on a screen at 1×.

### Still from before

The wiki has no picture for these, so they keep theirs:

- `nodes/splitter.png`, `merger.png` and `control_port.png` (the belt's Splitter, Converger and
  Item Control Port), `splitter_pipe.png`, `merger_pipe.png` and `control_port_pipe.png` (the
  pipe's): added by hand on 2026-11-25 as 256 × 256 PNGs and scaled to 125 × 125. The wiki's
  Facilities page has no logistics parts.
- `facilities/liquid_clean_gate.png` and `liquid_recycle_gate.png` (60 × 60): jambochen's
  `liquid_clean_gate_1` and `liquid_recycle_gate_1`, renamed. These buildings have no item in the
  game (`aic-extract`'s `ITEMLESS_FACILITIES`), and the wiki doesn't list them.

### No icon yet

The app draws initials for these:

- Data v1.7.0: the ten arrays (`magic_ming_1` to `magic_ming_4`, `magic_hui_1` to `magic_hui_4`,
  `magic_heng_1`, `magic_heng_2`), which are formations, not buildings (the game's
  `FactoryFormationTable` names pictures `item_magic_*` and `icon_port_magic_*` that the saved pages
  don't have), and the two outposts (`settlement-tundra_coupon`, `settlement-jinlong_coupon`).
- Data v1.6.0: the two outposts.
- Data v1.5.3 also: `domain_key_tundra`, `__domain_transfer`, `__miner_pump_1`,
  `settlement-tundra` and `settlement-jinlong`: IDs factoriolab made up, with no item or building
  in the game to take a picture from.

## License

The icons are images from *Arknights: Endfield* and belong to Hypergryph / Gryphline. They are
**not** covered by this repository's code license (MIT). They are included for
reference so the planner can work, as other community tools do. If you are a rights holder and want
something removed, open an issue.
