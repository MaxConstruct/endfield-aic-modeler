---
version: 1
slug: "crates-aic-app-src-editor-rs"
primary_target: "crates/aic-app/src/editor.rs"
related_targets: ["crates/aic-app/src/canvas.rs","crates/aic-app/src/lib.rs"]
---

# App shell (editor window)

Scope: the whole editor window of aic-app: menu bar, toolbar, recipe browser, node canvas, inspector, status bar, node picker, port menu, settings window. Mode: Operate.

Task: build and read factory graphs on a second 27" monitor beside the running game. Glance at counts, rates and warnings; make quick edits with mouse and keyboard; long sessions too.

Constraints: egui 0.36 and `aic-node-graph` (our copy of egui_graph, for the canvas) paint everything; styling goes through `Style`/`Visuals`, `FontDefinitions` and the painter. Session deliverable is the spec (DESIGN.md) plus a theme module wired into egui; component-by-component restyle comes later.

Craft bar: Figma and Linear desktop apps (quiet neutral chrome, crisp 1px dividers, restrained accent, polished popovers) with Satisfactory Modeler's function.

## Direction contract

THESIS: The category standard, played straight: a dark node editor with quiet neutral chrome, so the graph and its numbers are the only loud thing on screen. Refuses decoration, glow, gradients and themed costume.

OWN-WORLD: Cool-neutral graphite ramp (canvas darkest, panels one step up, cards and popovers one more), 1px dividers, 4/6/8 corner radii, one blue accent for selection and focus only, semantic pink (pulled from depot), green (sent to depot), amber (warning), red (error). Node headers carry a low-chroma tint per node kind. Inter (embedded, same on macOS and Windows) for UI text; embedded JetBrains Mono for numbers that must line up.

STORY: The player sees the graph, reads every count and rate at a glance from beside the game, and acts with one gesture; nothing asks for confirmation.

FIRST VIEWPORT: Thin top bar (menus + toolbar, ~52pt total), left recipe browser (~260pt), center canvas taking the rest, right inspector (~300pt), 22pt status bar. Canvas grid faint; cards are the brightest surfaces; selection draws in accent.

FORM: Canon (category standard), chosen by the user over the assigned roll; seed key 5df40d50.

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance
