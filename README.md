# AIC Modeler

A graph-first factory planner for the AIC in *Arknights: Endfield*.

You place nodes on a canvas, pick a recipe for each, link outputs to inputs and set limits where
you have them. The solver runs everything as high as those limits and the supply allow, and shows
what the factory needs: facility counts, link rates, belts, depot I/O and power.

**Aspiration.** To be for Endfield's AIC what Satisfactory Modeler is for Satisfactory: a tool that
lets you plan a factory faster and with more confidence than trial and error in the game. It plans
logical flow only, never the layout on the ground. It is meant to run beside the game, so it stays
light: an idle window draws nothing.

This is a fan-made tool, not affiliated with Hypergryph or Gryphline.

## Features

- **Node canvas.** Right-click to add a recipe; drag a port onto empty canvas to add a linked node
  that makes or uses its item.
- **Limits, and the solver maximizes.** Type a limit on a card or a port; everything else runs as
  high as the limits and the supply allow.
- **Numbers on the graph.** Facility counts (exact, or rounded up), the rate and belt or pipe count
  of every link, and warnings where a link needs more than one belt.
- **Totals.** Depot I/O, power draw and generation, facilities and raw resources for the whole
  graph.
- **Regions.** Valley IV and Wuling, each with its own raw resource supply; going over it is a
  warning.
- **Node kinds.** Recipes, sources, sinks, storage, splitters and mergers, outpost orders, and
  workspaces that fold part of a graph into one card with its own ports.
- **Views.** Split the canvas, or move a view to its own window.
- **Editing.** Undo and redo, copy and paste (between windows too), and a shortcut for every
  command.
- **Files.** `.aicm` graph files, saved to a draft as you edit; the last graph reopens on start.
  Export the canvas as a PNG.
- **Desktop and web.** Windows and macOS apps, and a web build that runs from any static site.
  Game data, names and icons are built in; nothing is downloaded.

## Build from source

### 1. Install the tools

- [Rust](https://rustup.rs), a recent stable toolchain (the project is developed on 1.99).
- A C toolchain for linking: on Windows the Visual Studio Build Tools with "Desktop development
  with C++"; on macOS the Xcode Command Line Tools (`xcode-select --install`).

Windows and macOS are supported. Linux may build but is not tested.

### 2. Get the source

```sh
git clone https://github.com/MaxConstruct/endfield-aic-modeler
cd endfield-aic-modeler
```

The game data is in the repository. The build fetches nothing but the Rust crates.

### 3. Build and run

```sh
cargo run --release -p aic-app
```

The first build compiles every dependency and packs the icons, so it takes a while. The program is
`target/release/aic-app` (`aic-app.exe` on Windows) and runs on its own, with no files beside it.

### Optional: other builds

| Build | Command | What you get |
|---|---|---|
| Smallest and fastest to start | `cargo build --profile packaged --locked -p aic-app` | `target/packaged/aic-app`, optimized as one unit. Slow to build. |
| For your own CPU (Windows) | `powershell -File scripts\build-for-this-cpu.ps1` | `target\native\packaged\aic-app.exe`, built with `-C target-cpu=native`. It may not run on another PC. |
| Portable Windows `.exe` | `pwsh scripts\package-windows.ps1` | `target\dist\AIC-Modeler-<version>-windows-x64.exe`, with the C runtime linked in. Needs PowerShell 7. |
| macOS app and `.dmg` | `scripts/package-macos.sh` | `target/dist/AIC Modeler.app` and a `.dmg`. Add `--universal` for Apple silicon and Intel in one app. |
| Web | `scripts/build-web.sh` | A static site in `target/web/`. `--serve` runs a debug build at `http://127.0.0.1:8080`. |

The web build needs a few more tools, installed once:

```sh
rustup target add wasm32-unknown-unknown
cargo install --locked trunk
cargo install --locked wasm-bindgen-cli --version 0.2.130
cargo install --locked wasm-opt
```

On Windows, run `scripts/build-web.sh` from Git Bash. Notes on hosting the site are at the top of
that script.

### Optional: settings when you run it

These are read when the app starts, not when it is built.

| Setting | Values | What it does |
|---|---|---|
| `WGPU_BACKEND` | `dx12`, `vulkan`, `gl`, `metal`, or a list such as `dx12,vulkan` | The graphics backend. On Windows the app starts DX12 only, because that starts fastest; set this if DX12 does not work on your PC. macOS uses Metal. |
| `WGPU_POWER_PREF` | `low`, `high` | Which GPU to use on a computer that has two. |
| `--data <folder>` or `AIC_DATA_DIR` | a folder of data versions | Uses the data versions in that folder in place of the built-in ones. See [`data/README.md`](data/README.md). |

For example, to start with Vulkan on Windows:

```powershell
# PowerShell
$env:WGPU_BACKEND = 'vulkan'; .\target\release\aic-app.exe
```

```sh
# Git Bash
WGPU_BACKEND=vulkan ./target/release/aic-app.exe
```

## License

The code is under the [MIT license](LICENSE-MIT).

The game data, names and icons belong to Hypergryph / Gryphline and are not covered by that
license; see [`data/README.md`](data/README.md).
