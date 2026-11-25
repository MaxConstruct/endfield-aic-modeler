---
name: AIC Modeler
description: Graph-first factory planner for Arknights Endfield's AIC; a dark node editor done well.
colors:
  canvas: "#111214"
  panel: "#18191C"
  raised: "#1F2024"
  inset: "#0F1012"
  stripe: "#1B1C20"
  control: "#26282D"
  control-hover: "#2E3036"
  control-active: "#36383F"
  divider: "#26272B"
  border: "#303238"
  border-strong: "#44474F"
  grid: "#1C1D21"
  text-strong: "#F3F4F6"
  text: "#D3D5DA"
  text-muted: "#A0A3AB"
  text-faint: "#868991"
  accent: "#6E9BFF"
  accent-fill: "#243761"
  accent-text: "#9CBBFF"
  depot-in: "#F07AAE"
  depot-out: "#6CC77E"
  linked: "#9AA0AB"
  warn: "#E9A23B"
  warn-fill: "#3A2B12"
  warn-stroke: "#6E501F"
  positive: "#6CC77E"
  negative: "#EF6461"
  node-recipe: "#283349"
  node-source: "#4A2A3C"
  node-sink: "#213A29"
  node-outpost-order: "#45361C"
  node-missing: "#522426"
  node-logistics: "#2E3036"
  node-workspace: "#3A3052"
  region-1-fill: "#3B360E"
  region-1-stroke: "#756A1B"
  region-1-text: "#F2E05A"
  region-2-fill: "#123D40"
  region-2-stroke: "#236E73"
  region-2-text: "#74E0E4"
typography:
  label:
    fontFamily: "Inter Display"
    fontSize: "12px"
    fontWeight: 300
  body:
    fontFamily: "Inter Display"
    fontSize: "13px"
    fontWeight: 300
  mono:
    fontFamily: "Inter Display Tabular"
    fontSize: "13px"
    fontWeight: 300
  title:
    fontFamily: "Inter Display"
    fontSize: "15px"
    fontWeight: 600
  number:
    fontFamily: "Inter Display Tabular"
    fontSize: "18px"
    fontWeight: 600
rounded:
  small: "4px"
  medium: "6px"
  large: "8px"
spacing:
  item-y: "3px"
  button-y: "2px"
  menu-margin: "4px"
  node-margin: "6px"
  item-x: "8px"
  button-x: "8px"
  window-margin: "10px"
  indent: "16px"
  interact-height: "20px"
  grid-cell: "40px"
components:
  button:
    backgroundColor: "{colors.control}"
    textColor: "{colors.text}"
    rounded: "{rounded.small}"
    padding: "2px 8px"
    height: "20px"
  button-hover:
    backgroundColor: "{colors.control-hover}"
    textColor: "{colors.text-strong}"
    rounded: "{rounded.small}"
  button-active:
    backgroundColor: "{colors.control-active}"
    textColor: "{colors.text-strong}"
    rounded: "{rounded.small}"
  button-selected:
    backgroundColor: "{colors.accent-fill}"
    textColor: "{colors.accent-text}"
    rounded: "{rounded.small}"
  text-field:
    backgroundColor: "{colors.inset}"
    textColor: "{colors.text}"
    rounded: "{rounded.small}"
    height: "20px"
  node-card:
    backgroundColor: "{colors.raised}"
    textColor: "{colors.text}"
    rounded: "{rounded.medium}"
    padding: "6px"
    width: "300px"
  node-header-recipe:
    backgroundColor: "{colors.node-recipe}"
    textColor: "{colors.text-strong}"
    typography: "{typography.body}"
  link-label:
    backgroundColor: "{colors.inset}"
    textColor: "{colors.text}"
    typography: "{typography.label}"
    padding: "3px"
  popover:
    backgroundColor: "{colors.raised}"
    textColor: "{colors.text}"
    rounded: "{rounded.medium}"
    padding: "4px"
  window:
    backgroundColor: "{colors.raised}"
    textColor: "{colors.text}"
    rounded: "{rounded.large}"
    padding: "10px"
---

# Design System: AIC Modeler

Native desktop app (Rust, egui 0.36, with the node canvas drawn by `aic-node-graph`, our copy of egui_graph; macOS and Windows). There is no CSS: every
token above is a constant in `crates/aic-editor/src/style.rs` (`style::color::*`, `style::radius::*`,
`style::text::*`; the egui app re-exports them from `theme.rs`), and the frontmatter's hex values mirror them. Token slugs map to Rust names by
kebab-case (`text-muted` = `color::TEXT_MUTED`, `node-sink` = `color::node::SINK`). Sizes are egui
points; at 1x a point is a pixel. `crates/aic-editor/src/style.rs` is the source of truth: change a value there, then here.

## Overview

**Creative North Star: "The Quiet Workbench"**

The category standard played straight: a dark node editor with quiet neutral chrome, so the graph and
its numbers are the only loud thing on screen. The craft bar is the Figma and Linear desktop apps
(neutral chrome, 1px dividers, restrained accent, polished popovers) with Satisfactory Modeler's
function. Dark because the app sits on a second monitor beside the running game.

Density is high and deliberate: 13pt text, 20pt controls, 3pt row gaps, built for 27" screens at
1440p and 4K with keyboard and mouse. Polish means precision (pixel-snapped hairlines, aligned
numbers, contrast that holds at 1x), not decoration. No glow, gradients or themed costume.

**Key Characteristics:**
- Cool graphite ramp: canvas, then panels, then cards and popovers, each one step lighter.
- One blue, for selection and focus only. Every other hue is a state.
- Inter Display Light for words, its tabular-figure cut for numbers that must line up; embedded, identical on macOS and Windows.
- 1pt hairlines, 4/6/8 radii, short soft shadows only on things that float.
- Act, don't ask: no confirm dialogs, no OK/Cancel; undo and autosave are the safety net.

## Colors

Cool-neutral graphite with semantic hues that each mean exactly one thing.

### Primary
- **Focus Blue** (`ACCENT`): selected node outline (2pt), selected link label outline (1.5pt), right-drag select box (1pt, fill at 15%), text cursor (2pt). Nothing else is blue.
- **Selection Wash** (`ACCENT_FILL`) with **Selection Ink** (`ACCENT_TEXT`): selected text, toggled selectable buttons and rows; `ACCENT_TEXT` is also the focused field outline (egui draws both with `selection.stroke`).

### Secondary (semantic states)
- **Depot Pink** (`DEPOT_IN`): an open input port pulling from the depot; Source nodes share the family.
- **Depot Green** (`DEPOT_OUT`): an open output port sending to the depot; Sink nodes share the family.
- **Link Grey** (`LINKED`): linked ports and every wire.
- **Throttle Amber** (`WARN`): throttle markers, a link over one belt, warnings. Mapped to `Visuals::warn_fg_color`. On a dim amber chip (`WARN_FILL`, outlined `WARN_STROKE`) it is a tag: the gas environment a recipe needs, a condition the player has to see to.
- **Fill Green** (`POSITIVE`): a storage filling up.
- **Drain Red** (`NEGATIVE`): a storage draining, input that doesn't parse, errors, a missing recipe. Mapped to `Visuals::error_fg_color`.

### Tertiary (node header tints)
Low-chroma, one per node kind, set in `Canvas::header_frame` via `theme::color::node`: Recipe (slate blue), Source (pink family), Sink (green family), Outpost order (amber family), Missing recipe (red family), Logistics (neutral; splitters, mergers, storage, control and workspace ports), Workspace (violet).

### Neutral
- **Canvas** (`CANVAS`): the node canvas (`canvas::paint_background`), darkest surface.
- **Panel** (`PANEL`): side panels, menu bar, toolbar, status bar (`Visuals::panel_fill`).
- **Raised** (`RAISED`): node cards, menus, popovers, windows (`window_fill`, `theme::node_frame()`).
- **Inset** (`INSET`): text fields and wells (`extreme_bg_color`, `text_edit_bg_color`), link labels at 90%.
- **Stripe** (`STRIPE`): alternate rows (`faint_bg_color`).
- **Control / Control Hover / Control Active**: button fills at rest, under the pointer or open, pressed.
- **Divider** (`DIVIDER`): panel dividers, separators, resting widget outlines. **Grid** (`GRID`): canvas grid lines, one step above the canvas.
- **Border / Border Strong**: card, window and field outlines; outlines under the pointer.
- **Text Strong / Text / Text Muted / Text Faint**: headings and key numbers; labels and body; units, facility names, captions and hyperlinks; hints and placeholders (`weak_text_color`).

### Named Rules
**The One Blue Rule.** The accent marks selection and focus, nothing else. Hyperlinks are `TEXT_MUTED`; status is never blue.

**The Meaning-Only Hue Rule.** Pink is pulled from the depot, green is sent to it (or a storage filling), amber is a warning, red is an error. A hue used for decoration breaks the reading of every number on screen.

**The Tested Contrast Rule.** Contrast is enforced by unit tests in `crates/aic-editor/src/style.rs`: every text and state color is ≥4.5:1 on canvas, panel, raised and inset; `TEXT_STRONG` is ≥7:1 and `TEXT_MUTED` ≥4.5:1 on every header tint; `ACCENT_TEXT` ≥4.5:1 on `ACCENT_FILL`. A new color or tint joins those tests.

## Typography

**UI Font:** Inter Display (SIL OFL), Light and SemiBold static cuts embedded from `crates/aic-editor/fonts/`, identical on macOS and Windows.
**Number Font:** Inter Display Tabular: the same two cuts with Inter's tabular figures (`tnum`) frozen in as the default digits by `scripts/tabular-fonts.sh`. egui shapes text without OpenType features, so the font carries them. It is egui's `Monospace` family.

**Character:** One typeface throughout. Every digit is the same width in the tabular cut, so columns of numbers line up; letters and punctuation keep Inter's proportional widths. egui's default fonts stay behind both as fallbacks for symbol and emoji glyphs.

### Hierarchy
egui `TextStyle`s set in `theme::apply`; steps about 1.15 apart.
- **Number** (tabular SemiBold, 18): the number a card is about (facility count, a source's supply). `theme::number(text::NUMBER)` in `TEXT_STRONG`; also `theme::number_style()`.
- **Title** (Inter Display SemiBold, 15): panel titles; `TextStyle::Heading`.
- **Body** (Inter Display Light, 13): labels, buttons, list rows; `Body` and `Button`.
- **Mono** (tabular Light, 13): numbers in panels and port rates; `Monospace`, `FontId::monospace(text::MONO)`.
- **Label** (Inter Display Light, 12): link labels, captions, node kind lines, status bar; `Small`.

### Named Rules
**The Real Bold Rule.** egui's `.strong()` only changes color. Emphasis is `theme::strong(text)` (semibold family + `TEXT_STRONG`); a heading-sized emphasis adds `.size(text::HEADING)`.

**The Aligned Numbers Rule.** Any number that sits in a column or updates in place is mono. Words are never mono.

## Layout

Canvas-first. First viewport: menu bar and toolbar on top (~52pt together), recipe browser left
(~240-260pt) under its one-row Settings section, canvas taking the rest, inspector right (~300pt),
status bar at the bottom (~22pt).
Panels toggle away; the canvas never does.

egui `Spacing` (in `theme::apply`): `item_spacing` 8x3, `button_padding` 8x2, `interact_size` 40x20,
`window_margin` 10, `menu_margin` 4, `indent` 16, `icon_width` 14 (inner 8, spacing 6),
`combo_height` 320, `ScrollStyle::thin()` (floating) with a `TEXT` handle at 50% (65% with the pointer
over the list, 85% on the bar; tested ≥3:1 at rest). Canvas: 40pt grid (`canvas::paint_background`),
cards fixed at 300pt wide with 6pt inner margin, 48pt item-icon pins, 64pt facility image, 2pt wires.
Sizes are points at 100%. The canvas is laid out at its 200% maximum and drawn shrunk (the canvas
scales the style with `egui-scale`), so card text is rasterized at the size shown and stays sharp when zoomed
in; `canvas::crisp` converts a size, `document::VIEW_SCALE` converts positions.
Toolbar buttons are 30x26.

**The 1x-First Rule.** Design at 1x (1440p, Windows 100%), then check 1.5x and 2x. Custom painting snaps to pixels; hairlines are 1pt. `subpixel_binning` is off so text renders the same everywhere.

## Elevation & Depth

Depth is tonal first: canvas < panel < raised. Shadows are short, soft and black, only on things that
sit above the canvas or the window:
- **Card** (offset 0,2; blur 6; black α90): `theme::node_frame()`, scaled by the canvas for each card.
- **Popover / menu** (offset 0,4; blur 14; black α120): `Visuals::popup_shadow`.
- **Window** (offset 0,8; blur 24; black α140): `Visuals::window_shadow`; `window_highlight_topmost` off.

**The Flat Chrome Rule.** Panels, bars and controls carry no shadow; a 1pt divider separates them.

## Shapes

Gently rounded, three steps: **small** (4) buttons, fields, chips, link labels (painted at 3);
**medium** (6) node cards, headers, menus, popovers; **large** (8) windows. Outlines are 1pt; the
selected node is 2pt. `WidgetVisuals::expansion` is 0 in every state: hover changes color, never size.

## Components

### Buttons
Quiet and flat. Rest: `CONTROL` fill, `DIVIDER` outline, `TEXT`. Hover/open: `CONTROL_HOVER`, `BORDER_STRONG`, `TEXT_STRONG`. Pressed: `CONTROL_ACTIVE`, `ACCENT` outline. All 1pt, radius small, built by the `control` closure in `theme::visuals`. Toggled selectable buttons use the selection wash.

### Inputs / Fields
`INSET` fill, radius small, 28pt high (`widgets::text_field`), on a card too. One as wide as its panel has its text in line with the buttons under it (`widgets::wide_field`); a label beside one goes in a `widgets::field_row`. Focus: `ACCENT_TEXT` outline, 2pt `ACCENT` cursor that doesn't blink. Unparseable input turns its text `NEGATIVE`; nothing pops up.

### Node Card (signature)
`RAISED`, 1pt `BORDER`, radius medium, card shadow, 300pt wide; text never wraps and can't be selected (a drag moves the node). Header filled with its kind's tint: a 12pt muted kind line (omitted on recipe cards) over a `theme::strong` title; a recipe card puts its facility's name under the title instead, 12pt `TEXT_MUTED`, with the header's full width for long names (a frameless button that cycles facilities when the recipe has several). A recipe that needs a gas environment has a tag beside the facility's name ("Stable Environment"): 12pt semibold `WARN` on `WARN_FILL`, radius small; it always shows whole, and the facility's name is cut short first. Body: facility image, the focal **Number**, a small limit button. Ports are item-icon pins at the edges with mono rates beside them, colored `LINKED`, `DEPOT_IN` or `DEPOT_OUT`; a throttled port gets a 10pt amber triangle. Selected: 2pt `ACCENT` outline.

### Links and Link Labels
Wires 2pt `LINKED`. The label is 12pt text on `INSET` at 90%, radius 3, 3pt pad, at the wire's midpoint; amber when over one belt; 1.5pt `ACCENT` outline when selected.

### Menus, Popovers, Windows
`RAISED`, 1pt `BORDER`; menus and popovers radius medium with the popover shadow, windows radius large with the window shadow. Popups open fully formed and focused in the frame they are called; the node picker opens on right-click with search focused.

### Toolbar
30x26 buttons; pressing a command's shortcut flashes its button for 400ms. The top bar has three icon toggles: `📖 Recipes` after the menus, and `🎓 Tutorial  🔍 Inspector` at its right end. Each is a symbol from egui's built-in fonts in front of its word (`widgets::icon_toggle`), lit while its panel or window is open, known to a screen reader by the word alone, and mirrored as a checkbox in the View menu, with Round up facilities and the frame rate. The region, the data version and the raw resources live in the Settings section.

### Tutorial
A small window (`crates/aic-app/src/tutorial.rs`) that teaches four things in four flat rows: adding a node, growing the chain from a port, the editing keys, saving and exporting. It opens by itself only at a start that reopened neither a file nor a draft (`aic_editor::tutorial::opens_at_start`), and from the top bar's Tutorial toggle; nothing about it is stored, and closing it lasts for the session. Not modal: it first sits with its bottom-right corner 16pt inside the canvas's, the canvas around it keeps working, and the picker and port menu open above it. The theme's window look as is, 300pt of content, no footer, buttons or step counter, and no animation, so an idle tutorial draws nothing.
- **Row:** a picture well, 10pt gap, then a `theme::strong` 13pt title over 13pt `TEXT` body wrapped to the row; 3pt between lines, 12pt between rows.
- **Picture well:** 56x44, `INSET` fill, 1pt `DIVIDER` outline, radius small. Inside, a drawing of the app's own objects (picker popover, recipe card and wire, key caps, file and picture) in theme neutrals, pixel-snapped 1pt lines, 2pt wires; no blue, no state hue.
- **Key cap:** a command's name in 13pt `TEXT`, then its keys on a chip: `CONTROL` fill, 1pt `BORDER`, radius small, 4x1pt padding, 12pt `TEXT` (words, not mono), on the name's baseline. Keys come from `format_shortcut`, so a Mac shows ⌘. Names and caps line up in columns.

### Settings Section
The top of the recipe browser's panel (`crates/aic-app/src/settings.rs`), above the Recipes title and the panel's 1pt separator. Closed by default, so it costs one row: an egui collapsing arrow, `Settings` in 13pt `TEXT` (`TEXT_STRONG` under the pointer; a click on it or the arrow opens or closes the section), and the graph's region right-aligned in `TEXT_MUTED` on the same baseline. egui remembers whether it is open. Open, indented 16pt with no frame or inner separator: label-and-picker rows for Region (the graph's; undo takes it back) and, when there is a data folder, Data version and Game names; the labels share one column and the combo boxes fill the rest of the row to one right edge. Then `theme::strong` "Raw resources", its long explanation as hover text, and one row per raw resource of the graph's region: 18pt icon, name cut short with the whole name on hover, a 56pt right-aligned mono supply field (red text when it doesn't parse) and a `TEXT_MUTED` `/min`, the fields in one column. Nothing overflows the panel at its 240pt default.

### Status Bar
Left: the app info, `v0.1.1 · MaxConstruct · GitHub · MIT`, in 13pt `TEXT_FAINT` except the GitHub hyperlink (`TEXT_MUTED`, the repository URL on hover, opening in the system browser or a new tab). Version, author, repository and license come from the workspace's Cargo metadata; only the words around them are Fluent strings, and the license's hover text says the game's data, names and icons are not under it. After a separator, the status message in `WARN`, cut short (whole on hover) where it would reach the right side. Right: the frame rate and solve time, off by default and turned on in the View menu (the choice is kept), then the zoom control and the save state.

## Do's and Don'ts

### Do:
- **Do** take every color, size and radius from `theme`; a literal `Color32` or font size in a widget is a bug.
- **Do** put every user-facing string through Fluent (`crates/aic-editor/i18n/en.ftl`), with terse labels.
- **Do** make one gesture do the thing: right-click opens the picker with search focused, one click places the node, edits apply at once.
- **Do** give every command a keyboard shortcut and light its toolbar button when the key is pressed.
- **Do** rely on undo and background autosave as the safety net; the app reopens the last document.
- **Do** verify through egui_kittest snapshots (1280x800 at 1x). After a token change, delete `crates/aic-app/tests/snapshots/*.png`, then run `UPDATE_SNAPSHOTS=1 cargo test -p aic-app`: kittest only rewrites images past its diff threshold.

### Don't:
- **Don't** add confirm dialogs, OK/Cancel pairs or "discard changes?" prompts.
- **Don't** let a panel, dialog or tool cover the work or wait to be dismissed.
- **Don't** use blue for anything but selection and focus, or a state hue for decoration.
- **Don't** use egui's `.strong()` for emphasis; use `theme::strong`.
- **Don't** add hover expansion, glow, gradients or shadows on chrome.
- **Don't** over-explain: no help copy where a label, icon or number does the job.

## Known Gaps (next component work)

Not yet the system; the build still carries the old treatment:
- A drawn icon set to replace the emoji and glyph icons in the toolbar, status bar and workspace card.
- Recipe browser and picker rows as frameless, left-aligned selectable rows instead of filled buttons.
- Status bar save state in neutral `TEXT_MUTED` instead of amber/green.
- Panel toggles as neutral `CONTROL_ACTIVE` instead of `ACCENT_FILL`.
- Inspector values in an aligned grid: right-aligned mono values, muted units.
- Popover placement anchored to its trigger.
- Zoom-to-fit clamped so card text stays readable.
- Card hover state with `BORDER_STRONG`.
- A selected node's links emphasized.
- Optional major grid line every 5 cells.
