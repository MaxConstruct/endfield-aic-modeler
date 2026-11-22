# Product

<!-- impeccable:product-schema 1 -->

## Platform

desktop

Native desktop app, macOS and Windows, built with Rust + egui (eframe 0.36) and `aic-node-graph`
(our copy of egui_graph) for the node canvas. Desktop first. The wasm build (PLAN.md M9) is a "maybe later": no design effort goes
to it, but choices that would block it outright need a fallback (for example runtime system-font
loading falls back to embedded fonts).

## Users

One user: Max, who builds and uses the app. Every requirement comes from Max; the app exists because
no other planner fit those needs. It may be shared with other Endfield players later, but nothing is
designed to please a general audience. The user knows the domain (AIC, recipes, facilities, belts,
depot) and knows the app: labels and descriptions never over-explain.

## Product Purpose

A graph-first factory planner for the AIC in *Arknights: Endfield*. The player places nodes, picks a
recipe for each, links outputs to inputs and sets limits; the solver runs everything as high as those
limits and the supply allow, and shows facility counts, link rates, belt counts, depot I/O, storage
balances and power. Success is planning a factory faster and more confidently than in-game trial and
error or other calculators allow.

## Positioning

Modeled on Satisfactory Modeler's graph-first, limits-and-maximize workflow (ADR 0003), applied to
Endfield's AIC: regions, depot I/O as the default for open ports (ADR 0001), storage that decouples
sections, workspaces with explicit ports (ADR 0007). Logical flow only, never spatial layout (ADR 0002).

## Operating Context

- Used **beside the game**: the app sits on a second monitor while Endfield runs. Max glances at
  numbers and makes small edits between play, as well as building graphs.
- Displays: 27" monitors at both 4K and 1440p, on macOS and on Windows. That covers 1x (1440p, Windows
  100%, macOS non-Retina), 1.5x (Windows 4K at 150%) and 2x (macOS 4K) pixels-per-point. Text and
  thin strokes must hold up at 1x.
- Keyboard and mouse. Shortcuts for every command; right-click opens the node picker; right-drag
  box-selects; double-click opens a port menu or a workspace.
- Files: `.aicm` graph files, autosaved in the background (drafts in the app's data folder). The app
  reopens the last document on start.

## Capabilities and Constraints

- See `docs/PLAN.md` for scope and milestones, `GLOSSARY.md` for vocabulary, `docs/adr/` for
  decisions. UI copy uses the glossary's terms.
- egui is immediate-mode and paints its own widgets: styling goes through `egui::Style` / `Visuals`,
  `FontDefinitions` and custom `Painter` drawing, not CSS. Anything can be drawn with the
  painter; standard widgets are styled globally.
- Every user-facing string goes through Fluent (`crates/aic-editor/i18n/en.ftl`).
- The UI never does math; it renders `aic_core::Solution`.
- No third-party network access at build or run time; assets (data, icons, fonts) are copied in by
  hand.

## Brand Commitments

- Productivity first. Panels, dialogs and tools never get in the way: no confirm dialogs, no OK /
  Cancel buttons, no "discard changes?". Actions apply at once and undo is the safety net.
- Plain and simple UI is often enough. Polish means precision, not decoration.
- Terse labels. The user knows what they are doing.
- Reference app: Satisfactory Modeler (`docs/ref/satisfactory-ui.png`,
  `docs/ref/satisfactory-modeler-recipe-selector.png`). Its function and flow are the model; its
  polish level is not the ceiling.
- Visual direction (chosen 2026-11-09): the category standard, a dark node editor done well, not a
  themed world. Craft bar: the Figma and Linear desktop apps (quiet neutral chrome, crisp 1px
  dividers, restrained accent, polished popovers), with Satisfactory Modeler's function.

## Evidence on Hand

- Game data and item/facility icons in `data/v1.5.3/` (Hypergryph/Gryphline's, not under the code
  license).
- Current UI: `docs/ref/aic-ui-0.png`. Reference screenshots in `docs/ref/`.
- No users other than Max, no testimonials, no metrics.

## Product Principles

1. **The graph is the work.** The canvas gets the screen; chrome stays thin and out of the way.
2. **Act, don't ask.** One gesture does the thing. No confirmation, no second step; undo and autosave
   carry the safety.
3. **Numbers first.** Rates, counts and warnings must be readable at a glance from across a desk,
   beside a running game.
4. **Expert terse.** No explanatory copy where a label, an icon or a number does the job.
5. **Use the desk.** Large, high-resolution screens and keyboard + mouse are the baseline, not an
   enhancement.
