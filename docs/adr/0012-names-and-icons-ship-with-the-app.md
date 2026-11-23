# Names and icons ship with the app; a data version is one JSON file

A data version was a folder: `data/<version>/data.json`, `i18n/<lang>.json` and `images/`. It becomes one file, `data/<version>.json`, with an optional overlay beside it, `data/<version>_overlay.json`. The game names move to `crates/aic-editor/i18n/names/<lang>.json` and the icons to `crates/aic-app/assets/icons/`. Both are built into the app, one set for every data version. And the app loads all of it before the editor opens, on the web as on the desktop: nothing is fetched or decoded on first use.

Two reasons:

- **Start-up on the web.** The web build fetched every file of `data/` before it opened: 240 requests and 4.5 MB on 2026-11-23, of which 238 were icons and 89 of those belonged to no item of the data. With the names and the data inside the `.wasm` and the icons packed into one sheet, it is four requests: the page, its JS, the `.wasm` and the sheet.
- **What a data version is.** Names and icons are keyed by game IDs, which stay the same from one game version to the next. Only the recipes and numbers change. Keeping a copy of both per version meant copying 4.4 MB of the same pictures for each new version, and a `Catalog` that carried PNG bytes it never looked at.

The web build is a folder of static files (`target/web/`), uploaded by hand to the user's own server behind Cloudflare, which caches and compresses them. So every file but `index.html` has a hash of its content in its name, and the build compresses nothing itself.

## Considered Options

- **Icons inside the `.wasm`.** Rejected: 2.6 MB of PNG that a new build of the app would make every browser download again, though the icons did not change. As its own file with its hash in its name, the sheet stays in the browser's and Cloudflare's cache across builds.
- **Icons fetched after the editor opens, with initials until they come.** Rejected by the user: the app should be ready when it opens, as the desktop build is. A loading page of a few seconds is fine.
- **Keep the icons per data version, as one sheet each.** Rejected: the versions share nearly all their icons, and it keeps the web's version switch waiting on a download.
- **A word on the loading page** ("Loading…"). Rejected: Fluent is inside the `.wasm`, so the page's text before it could not go through it (ground rule 7). The page shows one number and one bar, which need no language.

## Consequences

- An ID with no name shows as its ID, and an item with no icon as its initials on a coloured square, as before. A new data version that adds items needs their names and icons added to the app's tables, or shows them so.
- When a game version renames something, every data version shows the new name.
- A `data/` folder on disk (`--data`, `AIC_DATA_DIR`, beside the executable, the working directory) still takes the place of the built-in data, and can no longer bring names or icons.
- The languages a player can choose are the app's name tables, not a version's.
- `Catalog` holds no image bytes. What picture stands for a recipe is still decided from which IDs have an icon, which the app now tells it.
- The web build's loading page shows one percentage and one bar over every byte (the `.wasm`, which carries the fonts, names and data, and the icon sheet), and the editor appears at 100%. A file that fails to load shows its name and HTTP status.
- The sheet is encoded twice (2026-11-23, PLAN.md M13 step 5): the desktop builds in a lossless PNG, and the web build downloads a lossy AVIF, about a quarter of its bytes, which the loading page decodes with the browser's own decoder and hands to the app as pixels. A browser that can't decode AVIF shows the sheet's name as a failure; there is no PNG fallback.
- factoriolab's `icons.webp` sprite sheet, read but never drawn, is no longer read. The `icons` list of the data stays, for its colours.
- The term stays **overlay** (GLOSSARY: "patch" is avoided for a data version).
