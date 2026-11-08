# Game data

Each subfolder is one **data version**, named after the game version it matches (for example
`v1.5.3`). The app scans this folder for subfolders that contain a `data.json` and uses the newest one
by default.

```
data/
├─ README.md                  # this file
└─ <version>/                 # e.g. v1.5.3
   ├─ data.json               # required: items, facilities, recipes, icon positions, regions
   ├─ i18n/
   │  └─ en.json              # required: English names keyed by the same IDs
   ├─ icons.webp              # optional: sprite sheet that data.json → icons[] points into
   ├─ overlay.json            # optional (later): our own patches on top of data.json
   └─ SOURCE.md               # optional: where it came from, commit/date, any hand edits
```

`data.json` and `i18n/*.json` currently use the
[factoriolab](https://github.com/endfield-calc/factoriolab) format. Only `crates/aic-data/src/raw.rs`
knows that format; the rest of the app uses the normalized `Catalog`.

Data is copied in by hand. The app never downloads it, and the build never fetches it.

## Sources

- `v1.5.3/`: `data.json` and `i18n/en.json` copied from endfield-calc/factoriolab, `src/data/aef/`.

## License

The game data and icons in this folder (names, recipes, numbers and images from *Arknights: Endfield*)
belong to Hypergryph / Gryphline. They are **not** covered by this repository's code license
(MIT OR Apache-2.0). They are included for reference so the planner can work, as other community tools
do. If you are a rights holder and want something removed, open an issue.
