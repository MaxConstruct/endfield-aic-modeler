# Task plan: an iced frontend beside the egui one

Planned 2026-11-15 on branch `exp/iced`. This file is the work queue for agents building a second
desktop UI for AIC Modeler in [iced](https://iced.rs) 0.14 with
[iced_nodegraph](https://crates.io/crates/iced_nodegraph) 0.5, next to the existing egui UI. The
two UIs share one GUI-free crate and never depend on each other.

Max's decisions (2026-11-15):

1. **The egui UI stays.** iced is added beside it, not instead of it.
2. **One GUI-free crate both frontends use**, `aic-editor`. It holds the document, undo, the
   selection, editing commands, files and autosave, Fluent strings, number formatting and settings
   models. The first tasks move the egui app onto it.
3. **Crate names:** `aic-app` is renamed `aic-app-egui`; the new crate is `aic-app-iced`.
   `cargo run -p aic-app-egui` and `cargo run -p aic-app-iced`.
4. **The iced app is desktop only** (macOS, Windows). `aic-editor` stays wasm-clean so a web build
   stays possible later, and the egui web build (M9) keeps working.
5. **iced_nodegraph draws the canvas, but a spike decides first** (task B2). If the spike says
   no-go, the canvas tasks are re-planned on a custom iced widget before any are started.

The iced app should end up roughly the same as the egui app in features and look. Small
differences forced by the toolkit are fine if they are written down (section 9).

---

## 1. How to work from this file (read first)

You are probably one of several agents taking these tasks one at a time.

1. **Pick the first task in the status table (section 10) that is `todo` and whose dependencies are
   `done`.** Set it to `doing` with the date when you start, and to `done` when its "Done when" list
   holds. Don't start a task whose dependencies aren't done. Don't do two tasks in one go.
2. **Read before coding:** `CLAUDE.md`, `GLOSSARY.md`, `docs/adr/` (all of it), the sections of
   `docs/PLAN.md` your task names, `DESIGN.md` for anything visual, and every file in your task's
   "Read first" list.
3. **Load skills before coding:** `m01-ownership` through `m15-anti-pattern` (all of them) and
   `tdd`. Then `egui` + `egui-snarl` for egui tasks, `iced` + `iced_nodegraph` for iced tasks.
   iced breaks its API every release: check the `iced` skill's references before using an iced API
   from memory.
4. **Test first** (the `tdd` skill). Each task lists the tests it needs. Write them, watch them fail,
   then make them pass.
5. **Every task ends green:**
   ```sh
   cargo test --workspace
   cargo clippy --workspace --all-targets -- -D warnings
   scripts/check-frontends.sh        # from task A7 on
   ```
   The egui web build must still compile from task A2 on:
   `cargo check -p aic-app-egui --target wasm32-unknown-unknown` (and `-p aic-editor`).
6. **Stay in scope.** Don't refactor what your task doesn't name. If you find a bug or a gap
   outside your task, add it to section 9 ("Deviations and open questions"). Don't fix it on the
   side.
7. **Stop and ask Max** when a task's instructions conflict with an ADR, when a "Done when" item
   can't be met, or when the spike (B2) raises a question it says to escalate.
8. **Commits:** one commit per task, only when Max or the agent that sent you asks for it. Follow the
   repo's message style (a short sentence-case summary, e.g. "Every region's recipes, region badges,
   and a region that can change"), end with the `Co-Authored-By` line, and add no session link.

---

## 2. Ground rules for this work

Everything in `CLAUDE.md` holds. On top of it:

- **No entanglement.** `aic-app-egui` never depends on iced, and `aic-app-iced` never depends on
  egui, eframe, egui-snarl, egui_dock or egui_kittest, not even as a dev-dependency.
  `aic-editor`, `aic-core` and `aic-data` depend on no GUI toolkit at all. `scripts/check-frontends.sh`
  (task A7) enforces this.
- **The UI never does math** (CLAUDE.md rule 1) applies to both frontends. Anything that decides
  what a number, a list or a refusal is lives in `aic-editor` or `aic-core`. A frontend only lays it
  out. Where two frontends would write the same non-drawing logic, it belongs in `aic-editor`.
- **One string table.** Both frontends read `crates/aic-editor/i18n/en.ftl` through
  `aic_editor::i18n::Strings`. An iced-only string still goes in that file, under a comment
  `# iced only`.
- **One design.** `DESIGN.md` is the spec for both. Colors, radii, text sizes and spacing come from
  `aic_editor::style` (task A3). A literal color or font size in either frontend's widgets is a bug.
- **Files are interchangeable.** A `.aicm` saved by one frontend opens in the other. Neither changes
  the save format.
- **Each frontend has its own app data.** The egui app keeps eframe storage and its drafts folder
  where they are. The iced app keeps its settings and drafts in its own folder (section 6.5), so
  running both at once never has them autosave over each other's draft.
- **Existing egui behaviour is frozen during phase A.** Every egui test and kittest snapshot must
  pass unchanged. A snapshot that changes means the refactor changed behaviour: fix the refactor,
  don't re-record the image.

---

## 3. Target architecture

```
crates/
├─ aic-data/        unchanged. Catalog, Bundle, discovery.
├─ aic-core/        unchanged. Graph, solver, file format.
├─ aic-editor/      NEW, no GUI deps. Document, Session, Selection, Command, files/autosave,
│                   Strings + i18n/en.ftl, format, names, picker/browser models, settings models,
│                   style tokens, fonts, built-in data (build.rs), test fixtures.
├─ aic-app-egui/    RENAMED from aic-app. eframe + egui-snarl + egui_dock. Native + wasm.
└─ aic-app-iced/    NEW. iced 0.14 + iced_nodegraph 0.5. Native only.
```

Dependency rules (enforced by `scripts/check-frontends.sh`):

| Crate | May depend on | Must not depend on |
|---|---|---|
| `aic-data` | serde & co. | any other aic crate, any GUI crate |
| `aic-core` | `aic-data`, good_lp & co. | `aic-editor`, any GUI crate |
| `aic-editor` | `aic-core`, `aic-data`, fluent-bundle, unic-langid, serde, serde_json, web-time, image (png encode only) | egui, eframe, egui-snarl, egui_dock, iced*, wgpu, winit, rfd |
| `aic-app-egui` | the above, eframe, egui-snarl, egui_dock, rfd, image | iced* |
| `aic-app-iced` | the above, iced, iced_nodegraph, rfd, directories | egui*, eframe |

Two wgpu versions end up in `Cargo.lock` (eframe's and iced's). That's expected: they're never
linked into one binary. `cargo build --workspace` builds both and takes longer. Use `-p` when you
only need one.

---

## 4. What moves where (phase A)

From `crates/aic-app/src/` (named by its current path) to `crates/aic-editor/src/`:

| Today | Goes to | Notes |
|---|---|---|
| `format.rs` | `aic_editor::format` | Moves as is. |
| `names.rs` | `aic_editor::names` | Moves as is. |
| `throttle.rs` | `aic_editor::throttle` | Moves as is (uses `web_time::Instant`). |
| `i18n.rs` + `i18n/en.ftl` | `aic_editor::i18n`, `crates/aic-editor/i18n/en.ftl` | The comment about egui drawing isolation marks as boxes becomes "UIs draw them as boxes". |
| `data.rs` (`DataChoice`, `DataSource`, `DataFolder`) | `aic_editor::data` | GUI-free already. |
| `embedded.rs` + `build.rs` | `aic_editor::embedded` + `crates/aic-editor/build.rs` | The data path becomes `ancestors().nth(2)` from `crates/aic-editor`. Same table, same wasm branch. |
| `store.rs` (`FileStore`, `BrowserFiles`) | `aic_editor::store` | `BrowserFiles` stays serde-only; the egui app keeps it in eframe storage under the same key. |
| `settings.rs`: `Availability`, `BadAmount`, `parse_amount`, `STORAGE_KEY` | `aic_editor::settings` | `SettingsWindow` (the egui window) stays in egui. |
| `picker.rs`: `Origin`, `Builtin`, `Choice`, `Matching`, `builtins`, `recipes`, `builtin_kind` | `aic_editor::picker` | The egui popup (`Picker`, drawing) stays. |
| `browser.rs`: the search/filter part | `aic_editor::browser` | A `fn recipes(catalog, query, category) -> Vec<&Recipe>` plus the category list. Drawing stays. |
| `editor.rs`: `recipe_node`, `link_error` | `aic_editor::session` / `aic_editor::messages` | |
| `inspector.rs`: `warning_text`, `issue_text`, `per_min` | `aic_editor::messages` | Pure `Strings` formatting of core types. |
| `canvas.rs`: `Selection` | `aic_editor::selection` | Moves as is. |
| `edit.rs`: `Shortcut` (the enum, `ALL`, `works_while_typing`) | `aic_editor::command::Command` + `Chord` | Key bindings become toolkit-neutral (task A5). |
| `export.rs`: crop + PNG encode | `aic_editor::export::crop_png(rgba, size, rect, ppp) -> Vec<u8>` | Window screenshot requests stay per frontend. |
| `theme.rs`: every color/radius/size/text constant + the contrast tests | `aic_editor::style` | egui's `theme.rs` keeps only the egui `Style`/`Visuals`/`SnarlStyle` building (task A3). |
| `fonts/*` | `crates/aic-editor/fonts/*`, exposed as `aic_editor::style::fonts::*` byte slices | `scripts/tabular-fonts.sh` path updated. |
| `document.rs`: graph, undo, solve, save state | `aic_editor::Document` | Without the snarl view and without the level (task A4). |
| `editor.rs`, `edit.rs`, `files.rs`: everything that isn't drawing | `aic_editor::Session` | Tasks A5 and A6. |
| `tests/fixtures/satisfactory/` | `crates/aic-editor/fixtures/satisfactory/` | Exposed via `aic_editor::testing` behind a `testing` feature. |

What stays egui-only: `canvas.rs` (drawing, `CanvasEvent`, pins), the snarl side of
`document.rs`, `views.rs`, `widgets.rs`, `toolbar.rs`, `inspector.rs` drawing, the egui
`picker.rs`/`port_menu.rs`/`browser.rs`/`settings.rs` drawing, `export.rs` screenshot plumbing,
`web.rs`, `lib.rs`, `main.rs`, `theme.rs` (egui styles built from `aic_editor::style`).

---

## 5. `aic-editor` interface (target shape)

This is the shape to aim for, not a contract to copy line by line. Keep names from the glossary.
Keep it deep: a frontend should need few calls to do anything.

```rust
/// Where a canvas looks: which level of the graph. Views (ADR 0008) are frontend layout; the
/// session never stores the focused level. Every call that depends on a level takes it.
pub type Level = Option<WorkspaceId>;

pub struct Document { /* graph, committed, undo/redo, solution, stale, edits, saved_edits,
                         generation, path, draft */ }
impl Document {
    pub fn new(region: RegionId) -> Self;
    pub fn from_graph(graph: Graph, path: Option<PathBuf>) -> Self;
    pub fn graph(&self) -> &Graph;
    pub fn solution(&self) -> &Solution;
    /// Counts every change to what a canvas draws; a view redraws when it moves on.
    pub fn generation(&self) -> u64;
    pub fn checkpoint(&mut self); pub fn undo(&mut self, c: &Catalog) -> bool; /* redo, can_* */
    pub fn is_modified(&self) -> bool; pub fn mark_saved(&mut self, path: PathBuf, draft: bool);
    pub fn solve_if_stale(&mut self, c: &Catalog) -> Option<Duration>;
    // Edits: every current Document edit, positions as aic_core::Pos, levels passed in.
    pub fn add_node(&mut self, kind: NodeKind, pos: Pos, level: Level) -> NodeId;
    /// A move: an undo step, but not a re-solve.
    pub fn move_member(&mut self, member: Member, pos: Pos);
    // connect, remove_link, remove_node, add_port(level, side), add_workspace, ...
}

pub struct Session { /* catalog, strings, doc, selection, clipboard, availability, files,
                        data folder + choice, status, other_data_version, round_up, last_solve */ }
impl Session {
    // Reading
    pub fn catalog(&self) -> &Catalog; pub fn strings(&self) -> &Strings;
    pub fn document(&self) -> &Document; pub fn selection(&self) -> Option<&Selection>;
    pub fn status(&self) -> Option<&str>; pub fn title(&self) -> String;
    // Editing (each returns what the frontend needs to show next, never a drawing)
    pub fn run(&mut self, command: Command, at: Level) -> Outcome;
    pub fn add_from_picker(&mut self, origin: &Origin, pos: Pos, choice: Choice, at: Level)
        -> Option<(NodeId, Option<Anchor>)>;
    pub fn connect(&mut self, from: PortRef, to: PortRef);   // a refusal sets status()
    pub fn select(&mut self, s: Option<Selection>); pub fn toggle(&mut self, m: Member);
    pub fn copy_text(&self) -> Option<String>; pub fn paste_text(&mut self, text: &str, into: Level);
    // Files: dialogs are the frontend's; the session decides everything else.
    pub fn open_file(&mut self, path: &Path); pub fn save_to(&mut self, path: &Path, view: Option<CanvasView>);
    pub fn autosave(&mut self, now: Instant, busy: bool, view: Option<CanvasView>) -> Poll;
    pub fn switch_data(&mut self, choice: DataChoice);
    pub fn settings(&self) -> Settings;                       // what a frontend persists
}

/// Something the player can do from a menu, the toolbar or a key. Toolkit-neutral.
pub enum Command { New, Open, Save, SaveAs, Undo, Redo, Cut, Copy, Paste, Duplicate, Delete,
                   SelectAll, MakeWorkspace, Dissolve, SplitRight, SplitDown, DuplicateView,
                   MoveToWindow, CloseView, /* … */ }
pub struct Chord { pub command: bool, pub shift: bool, pub key: ChordKey }  // Cmd on macOS, Ctrl elsewhere
impl Command { pub const ALL: [Command; N]; pub fn chord(self) -> Chord;
               pub fn works_while_typing(self) -> bool; pub fn label_key(self) -> &'static str; }

/// What a frontend must do after a command that the session can't do itself.
pub enum Outcome { Done, PickOpenPath, PickSavePath { offered: String }, WriteClipboard(String),
                   ReadClipboard, View(ViewCommand), Nothing }
```

Commands that need the toolkit (file dialogs, the system clipboard, view layout) come back as an
`Outcome`. The frontend finishes them and calls back in (`open_file`, `save_to`, `paste_text`).

---

## 6. The iced app (`aic-app-iced`)

### 6.1 Dependencies

```toml
[dependencies]
aic-core.workspace = true
aic-data.workspace = true
aic-editor.workspace = true
iced = { version = "0.14", features = ["image", "advanced", "tokio"] }  # add "canvas" if a task needs it
iced_nodegraph = "0.5"
rfd = "0.18"          # AsyncFileDialog, run through Task::perform
directories = "6"     # the app's data folder

[dev-dependencies]
iced_test = "0.14"
aic-editor = { workspace = true, features = ["testing"] }
```

Pin these minor versions. Upgrading iced means upgrading iced_nodegraph to match.

### 6.2 Modules

```
crates/aic-app-iced/
├─ src/main.rs        args (--data), DataFolder::find, settings load, iced::daemon(...).run()
├─ src/lib.rs         pub App, Message, update, view, subscription (main.rs stays thin; tests drive lib)
├─ src/theme.rs       aic_editor::style → iced Theme + one style fn per widget kind
├─ src/icons.rs       image::Handle cache keyed by (data version, ImageKind, id); placeholder squares
├─ src/menu.rs        menu bar + dropdowns (iced has no menu widget: buttons + an overlay layer)
├─ src/toolbar.rs     command buttons, flash on key press
├─ src/status.rs      status bar: status text, save state, zoom, frame rate / solve time
├─ src/browser.rs     recipe browser panel
├─ src/inspector.rs   inspector, totals, warnings
├─ src/canvas/        mod.rs (builds NodeGraph from Session), card.rs, labels.rs, edges_ports.rs
├─ src/picker.rs      node picker popup
├─ src/port_menu.rs   port menu popup
├─ src/settings.rs    settings modal (raw resources, data version, language)
├─ src/files.rs       dialogs (rfd async), app data folder, settings.json persistence
├─ src/views.rs       pane_grid of views, tabs, detached windows
├─ src/export.rs      window::screenshot → aic_editor::export::crop_png
└─ tests/             update tests + iced_test snapshots
```

### 6.3 State and messages

- `App { session: aic_editor::Session, views: Views, ui: UiState }`. `UiState` holds only
  presentation state: panel visibility, the open picker/port menu/settings window and their typed
  text, the limit fields being typed, the toolbar flash, the frame-time history.
- `Message` is one enum with sub-enums per area (`Message::Canvas(ViewId, CanvasMsg)`,
  `Message::Picker(PickerMsg)`, `Message::Command(Command)`, `Message::File(FileMsg)`, …).
- `update` translates a message into `Session` calls and returns `Task`s for dialogs, clipboard,
  screenshots and window ops. After every message: `session.solve_if_stale()`.
- **Undo steps:** egui ends a step when no pointer button is down and no field has focus. In iced,
  track `pointer_down` from `event::listen_with` (mouse pressed/released, any window) and
  `typing: Option<FieldKey>` (set when a limit field gets input, cleared on submit, on Escape and on
  any canvas press). Call `checkpoint()` at the end of `update` when neither holds.
- **Autosave:** a `Subscription` ticking every 250 ms (`iced::time::every`, needs `tokio`) feeds
  `session.autosave(now, busy, focused_camera)`. On a window close request (`window::close_requests`,
  with `exit_on_close_request(false)`), save first, then close.

### 6.4 The look

Match `DESIGN.md` and the egui app (`docs/ref/aic-ui-0.png` and egui's kittest snapshots in
`crates/aic-app-egui/tests/snapshots/`). Notes for iced:

- Load the four Inter Display cuts with `.font(bytes)` on the daemon. The default font is Inter
  Display Light (`Font { family: Family::Name("Inter Display"), weight: Weight::Light, .. }`).
  Check the actual family names inside the TTFs (`fc-scan` or the `tabular-fonts.sh` script) before
  hard-coding them. Numbers use the tabular cut.
- Build one `Theme::custom` from the tokens, and give every widget kind a style fn in `theme.rs`
  (button rest/hover/pressed/selected, field, card, popover, window, panel, divider). Widgets call
  those fns; they never build a `Style` inline.
- 1px dividers are `rule::horizontal(1)` / `rule::vertical(1)` styled `DIVIDER`. Panels and bars
  carry no shadow; cards, popovers and windows carry DESIGN.md's shadows.
- DESIGN.md's "Known Gaps" list describes where the design is going. Building the iced version to
  that target instead of egui's current look is allowed, never required.

### 6.5 App data

- Folder: `directories::ProjectDirs::from("", "", "AIC Modeler iced")` → `data_dir()`. Drafts in
  `<data_dir>/drafts`; settings in `<data_dir>/settings.json`.
- `settings.json` holds `aic_editor::Settings` (data choice, raw resource availability, recent
  files, last document, show performance) plus the iced view layout. Write it through a temp file
  and rename, like save files. Missing or unreadable settings fall back to defaults and say so in
  the status bar, never as an error screen.

### 6.6 Testing the iced app

- **Behaviour through `update`:** most tests build an `App` on the fixture catalog
  (`aic_editor::testing`), send `Message`s, and check `app.session` (graph, selection, solution).
  These are fast and exact.
- **Interaction through `iced_test::Simulator`:** clicks and typing on `app.view(window)`; feed
  `into_messages()` back into `update`. Shortcuts with modifiers need hand-built
  `keyboard::Event::KeyPressed` events (`tap_key` can't add modifiers); use `LOGO` on macOS and
  `CTRL` elsewhere.
- **Snapshots:** `ui.snapshot(&theme)?.matches_image("tests/snapshots/<name>")` writes
  `<name>-wgpu.png`. Comparison is exact. A missing file is written and passes, so delete it to
  re-record. iced_nodegraph draws node bodies, edges and pins only under wgpu, so snapshots must run
  on a machine with a GPU (Max's Mac). Snapshot tests carry `#[cfg_attr(not(target_os = "macos"), ignore)]`.
  Look at every new snapshot before committing it, since a snapshot is how this work gets seen.
  Snapshot size 1280×800 at scale 1.

---

## 7. Feature parity checklist

What "roughly the same" means. Each row names where egui does it and the task that brings it to
iced. Task C16 walks this list against the running app.

| Feature | egui source | iced task |
|---|---|---|
| Data folder search (`--data`, `AIC_DATA_DIR`, built-in), error screen | `main.rs`, `lib.rs`, `data.rs` | B1 |
| Theme, fonts, tokens | `theme.rs` | B1 |
| Menu bar: File, Edit, View; region picker; data version; Settings… | `editor.rs` `top_bar` | C1 |
| Toolbar with every command, tooltip naming the key, flash on key press | `toolbar.rs` | C1, C9 |
| Status bar: status message, save state, zoom slider, frame rate + solve time | `editor.rs` `status_bar`, `toolbar.rs` | C1, C13 |
| Collapsible recipe browser (left) and inspector (right) | `editor.rs` `ui` | C1 |
| Recipe browser: search, category filter, region badges, click adds at canvas centre | `browser.rs` | C2 |
| Cards: recipe (icon, title, facility line cycling, count, limit field), source, sink, splitter, merger, storage (balance, pass-through), control port, outpost order (coupons), missing recipe, workspace | `canvas.rs` | C3, C7, C12 |
| Ports: item icon pins, rates, depot colours (pink in, green out), throttle marker | `canvas.rs` | C3 |
| Region warning sign on a card's title | `canvas.rs` `title_row` | C3 |
| Move nodes (selection moves together), connect, refuse bad links with a status message, Cmd+drag on a pin removes its links | `canvas.rs`, `editor.rs` `connect` | C4 |
| Click select, Shift+click toggle, box select (right-drag), select all | `canvas.rs` | C4 |
| Pan, wheel zoom, zoom limits, fit | `canvas.rs` | C4, C13 |
| Link labels `12 /min · 0.4 belts`/`pipes`, amber over one belt, selectable links | `canvas.rs` | C5 |
| Node picker: right-click canvas, dropped wire, right-click open port; search, Name/Inputs/Outputs toggles, built-ins, one click adds and links; placement anchored to the port side | `picker.rs`, `editor.rs` `add_from_picker` | C6 |
| Port menu: double-click a port; limit as rate; ratio lock shares; clear connections | `port_menu.rs` | C7 |
| Inspector: node, several nodes, workspace, link; throttle reason; ports; limit field; remove | `inspector.rs` | C8 |
| Totals: depot in/out, power, facilities (+ rounded-up), raw resources vs availability, coupons; warnings | `inspector.rs` | C8 |
| Shortcuts (section 5 `Command`), undo steps, clipboard as save-file JSON across windows/apps | `edit.rs` | C9 |
| File: New, Open, Open Recent, Save, Save As; drafts; autosave (5 s throttle, waits while busy); reopen last; save on close; window title with modified mark; data version banner | `files.rs` | C10 |
| Settings window: raw resource availability per region (tabs), data version, game-name language | `settings.rs` | C11 |
| Workspaces: card, open (double-click / Open), Back + path bar, ports pinned to canvas edges, + Input / + Output, make (Cmd+G) / dissolve (Cmd+Shift+G), rename, paste into | `canvas.rs`, `edit.rs`, `inspector.rs` | C12 |
| Round-up view toggle | `editor.rs`, `canvas.rs` | C13 |
| Export PNG of the canvas | `export.rs` | C13 |
| Multiple views: tabs, splits, focus line, view commands, per-view level + camera, shared selection, layout persisted | `views.rs` | C14 |
| Detached view windows | `views.rs` | C15 |
| Packaging (.dmg / portable .exe) | `scripts/package-*.sh/ps1` | C16 (optional) |

Not in scope for iced: the web build (M9), the perf examples (B2 has its own spike), `showcase`.

---

## 8. Tasks

Each task: **Goal**, **Depends on**, **Read first**, **Steps**, **Tests**, **Done when**,
**Pitfalls**. Paths are relative to the repo root.

### Phase 0: record the decision

#### T0. ADR and plan updates
- **Goal:** the docs say there are two frontends over one editor crate, and why.
- **Depends on:** nothing.
- **Read first:** `docs/adr/*`, `docs/PLAN.md` (Tech, sections 2, 4, M4 spike notes), `CLAUDE.md`, the
  `domain-modeling` skill (ADR format).
- **Steps:**
  1. Write `docs/adr/0009-two-frontends-one-editor-crate.md` in the existing ADR style: context
     (iced experiment, the M4 note that a failed spike would reopen the framework question), the
     decision (section 3 and the dependency rules), considered options (iced replaces egui,
     separate crates with copied logic, shared crate with egui untouched), consequences (two wgpu
     versions in the lockfile, `aic-editor` must stay GUI-free and wasm-clean, strings shared,
     each frontend has its own app data, the egui UI stays the shipping UI until Max says
     otherwise).
  2. `docs/PLAN.md`: in "Tech", say egui is the shipping UI and an iced frontend is being built
     beside it (link ADR 0009 and this file). Add `aic-editor` and `aic-app-iced` to section 4.
     Add ADR 0009 to the list at the top.
  3. `CLAUDE.md`: rule 1 becomes "All solving lives in `aic-core` and all non-drawing app logic in
     `aic-editor`; neither has a GUI dependency." Add the ADR to "Read before working" if it isn't
     covered by `docs/adr/`.
  4. Add a line to `GLOSSARY.md` only if a new domain term appears. Session and Document are code
     names, not domain terms; don't add them.
- **Done when:** the three files read consistently. No code changed.

### Phase A: the shared crate (egui keeps working throughout)

#### A1. Rename `aic-app` to `aic-app-egui`
- **Goal:** a pure rename, no behaviour change.
- **Depends on:** T0.
- **Read first:** every file `grep -rlI 'aic-app\|aic_app' --exclude-dir=target --exclude-dir=.git .`
  lists. Today these are: `Cargo.toml`, `crates/aic-app/{Cargo.toml,build.rs,index.html,Trunk.toml}`,
  `crates/aic-app/src/{main.rs,data.rs}`, `crates/aic-app/i18n/en.ftl`,
  `crates/aic-app/tests/*.rs`, `crates/aic-app/examples/*.rs`, `scripts/*`, `package.yml`,
  `docs/PLAN.md`, `DESIGN.md`, `PRODUCT.md`, `data/README.md`, `.impeccable/design.json`,
  `.impeccable/surfaces/crates-aic-app-src-editor-rs.md`.
- **Steps:**
  1. `git mv crates/aic-app crates/aic-app-egui`. Package name `aic-app-egui`; the lib is
     `aic_app_egui`. Keep the binary's output name what packaging scripts expect, or update the
     scripts. Keep the window title and app id ("AIC Modeler") so eframe storage and drafts carry
     over.
  2. Update every reference from the list above. In docs, change paths only. Don't rewrite history
     sections (a milestone that says "`cargo run -p aic-app`" in the past tense may keep it, but
     current instructions must be right).
  3. Rename the `.impeccable/surfaces/` file to match the new path if the tool keys on it.
- **Tests:** none new. All existing tests must pass with no snapshot change.
- **Done when:** `cargo run -p aic-app-egui` opens the app; `scripts/build-web.sh` builds;
  `cargo test --workspace` and clippy are green; `grep -rn 'aic-app[^-]' --exclude-dir=target --exclude-dir=.git .`
  finds only historical mentions.
- **Pitfalls:** `build.rs` finds `data/` with `ancestors().nth(2)`, which still holds. Trunk's
  `index.html` names the crate in a `data-bin`/`link` tag. Search for both spellings,
  `aic-app` and `aic_app`.

#### A2. Create `aic-editor` with the pure modules
- **Goal:** a new GUI-free crate holding the modules that need no redesign.
- **Depends on:** A1.
- **Read first:** section 4; the listed source files; `crates/aic-app-egui/Cargo.toml`.
- **Steps:**
  1. New crate `crates/aic-editor` (workspace member, `aic-editor.workspace = true` in the root
     `[workspace.dependencies]`). `description = "App logic shared by the UIs. No GUI dependency."`
  2. Move, with `git mv` so history follows: `format.rs`, `names.rs`, `throttle.rs`, `i18n.rs` +
     `i18n/en.ftl`, `data.rs`, `embedded.rs` + `build.rs`, `store.rs`. Move from `settings.rs`:
     `Availability`, `BadAmount`, `parse_amount`, `STORAGE_KEY`. Move from `picker.rs`: `Origin`,
     `Builtin`, `Choice`, `Matching`, `builtins`, `recipes`, `builtin_kind` and their tests. Move the
     browser's filtering into `aic_editor::browser`. Move `link_error` (editor.rs) and
     `warning_text`, `issue_text`, `per_min` (inspector.rs) into `aic_editor::messages`.
  3. In `aic-app-egui`, depend on `aic-editor` and replace the moved code with `use aic_editor::…`.
     Re-export from the old module paths only where the egui tests import them
     (`aic_app_egui::data::DataFolder`, `aic_app_egui::picker::Origin`,
     `aic_app_egui::store::BrowserFiles`, `aic_app_egui::i18n::Strings`), so test files don't churn.
  4. Move `crates/aic-app-egui/tests/fixtures/satisfactory/` to `crates/aic-editor/fixtures/` and
     add `aic_editor::testing` (feature `testing`): `fixture_catalog() -> Catalog`,
     `real_catalog() -> Option<Catalog>` (built-in data). Point egui tests at it via the
     dev-dependency with `features = ["testing"]`.
  5. Unit tests that lived in the moved files move with them.
- **Tests:** the moved unit tests run in `aic-editor`. Add one test that `Strings::english()`
  loads in `aic-editor`, and one that `embedded::bundle()` lists `v1.5.3` when `data/` exists.
- **Done when:** `cargo tree -p aic-editor` shows no egui, eframe or iced; everything is green;
  `cargo check -p aic-editor --target wasm32-unknown-unknown` and the same for `-p aic-app-egui`
  pass; the egui snapshots are unchanged.
- **Pitfalls:** `web.rs` uses `embedded`'s wasm table (paths only). Keep the
  `#[cfg(target_arch = "wasm32")]` split in `build.rs` and `embedded.rs` exactly as it is.
  `include_str!("../i18n/en.ftl")` paths change. Don't change any `.ftl` message ids.

#### A3. Design tokens and fonts in `aic_editor::style`
- **Goal:** one source for DESIGN.md's values.
- **Depends on:** A2.
- **Read first:** `crates/aic-app-egui/src/theme.rs`, `DESIGN.md`, `scripts/tabular-fonts.sh`.
- **Steps:**
  1. `aic_editor::style` holds plain data: `pub struct Rgba(pub u8, pub u8, pub u8, pub u8)` (or
     `[u8; 4]`), `color::*` and `color::node::*` constants with the same names as today,
     `radius::*`, `text::*` (sizes), spacing constants, shadow specs (offset, blur, alpha), and
     `fonts::{INTER_LIGHT, INTER_SEMIBOLD, INTER_TABULAR_LIGHT, INTER_TABULAR_SEMIBOLD}: &[u8]`
     (`include_bytes!`), plus the font license file.
  2. Move the contrast-ratio tests from `theme.rs` into `aic_editor::style` (they test values,
     not egui).
  3. egui's `theme.rs` converts `style` values to `Color32`/`FontId`/`Visuals` and keeps its egui
     builders and the `strong`/`number` helpers. Its public constant names (`theme::color::ACCENT`
     etc.) can be re-exports so egui call sites don't churn.
  4. Move `fonts/` to `crates/aic-editor/fonts/`; update `scripts/tabular-fonts.sh` and DESIGN.md's
     paths ("`theme.rs` is the source of truth" becomes `crates/aic-editor/src/style.rs`).
- **Tests:** the moved contrast tests pass; one test that each font slice is a TTF (starts with
  `0x00010000`).
- **Done when:** green, egui snapshots unchanged, DESIGN.md points at the new source.

#### A4. Split the document: `aic_editor::Document` + egui's view layer
- **Goal:** the graph, undo and solve state live in `aic-editor` with no snarl and no level; egui
  keeps the snarl view in step by itself.
- **Depends on:** A2.
- **Read first:** `crates/aic-app-egui/src/document.rs` (all of it, including tests),
  `views.rs` (how `swap_level` and `LevelView` are used), `canvas.rs` (`Anchor` use, `parts()`),
  `aic_core::Graph` (`members_of`, `add_node_in`, `set_pos`), section 5.
- **Steps:**
  1. `aic_editor::Document`: everything in today's `Document` except `view`, `view_ids`, `level`
     and `anchors`. Positions in `aic_core::Pos`. Every method that used the level takes
     `level: Level`. Add `move_member(member, pos)`: an undo step (pending, edits, generation) but
     not stale, as `sync_from_view` does today. `generation()` is public. `restore` no longer
     touches a level; a view whose workspace is gone falls back to the top on the frontend side.
     Keep `Anchor` in `aic-editor` as graph-space data.
  2. In egui, `document.rs` becomes the view layer: a `DocView` (keep the old name `Document` if
     that keeps the egui code and tests readable) wrapping `aic_editor::Document` plus `view`,
     `view_ids`, `level`, `anchors` and `synced: u64`. Edits go to the inner document, then
     `reconcile(catalog)` brings the snarl in line: add view nodes for new members, remove view
     nodes for gone ones, move view nodes whose graph position changed, add or remove wires to
     match the links. Unchanged members keep their snarl `NodeId`, so egui-snarl's state survives.
     `swap_level`, `LevelView` and the reconcile-on-generation rule keep today's behaviour.
  3. `sync_from_view` calls `move_member` for moved nodes and `remove_link` for lost wires.
- **Tests:** move today's document tests that concern undo, saving and edits into `aic-editor`,
  rewritten without the view (e.g. `edits_between_checkpoints_undo_as_one_step`,
  `every_change_after_a_save_needs_saving_undo_included`, `a_new_edit_drops_the_redo_steps`). Keep
  the view tests (`links_and_wires_stay_in_step`, `a_wire_lost_in_the_view_drops_its_link`,
  `a_view_swapped_in_draws_what_changed_while_it_waited`, …) in egui. Add:
  `reconcile_keeps_view_ids_of_unchanged_members`, `reconcile_after_undo_matches_a_fresh_build`
  (compare against a view built from scratch).
- **Done when:** green; egui snapshots unchanged; `aic_editor::Document` has no `pub(crate)` field
  that egui reaches into.
- **Pitfalls:** `VIEW_SCALE` (×2) is egui-only: the graph stores unscaled positions. A removed
  workspace port rebuilds its workspace card's pins, so reconcile must notice pin count changes on a
  workspace card, not only membership changes. Two links between the same two ports share one wire;
  keep that rule.

#### A5. `aic_editor::Session`: selection, editing commands, `Command`
- **Goal:** everything the egui `Editor` decides that isn't drawing or files, behind one type.
- **Depends on:** A4.
- **Read first:** `editor.rs`, `edit.rs`, `canvas.rs` (`Selection`, `CanvasEvent`, `apply`), section 5.
- **Steps:**
  1. Move `Selection` into `aic_editor::selection`.
  2. `Session` owns `catalog`, `strings`, `doc: Document`, `selection`, `clipboard: Option<(Graph, u32)>`,
     `availability`, `status: Option<String>`, `other_data_version`, `round_up`, `last_solve`.
     Port these from `Editor`, levels as parameters: `select`, `toggle`, `remove_node`, `connect`
     (status on refusal), `add_from_picker` (returns the node and its `Anchor`), copy, cut, paste,
     paste_into, duplicate, delete_selection, select_all, make_workspace,
     `new_workspace_name`, dissolve_selection (returns what to select; egui filters by what it
     shows), `switch_data`, `set_region`, `recipe_node`, and every `CanvasEvent` arm that only
     changes the graph or selection.
  3. `Command` (renamed from `Shortcut`) with a toolkit-neutral `Chord` and the same order rule
     (`ALL` checked in order so Cmd+Shift+S isn't Save). `ChordKey` covers the keys used today
     (letters, `Delete`, `Backspace`, `Backslash`). egui maps `Chord` to `KeyboardShortcut` in
     one function; the toolbar and menus keep working.
  4. `Session::run(command, level) -> Outcome` for the commands the session can finish. File,
     clipboard and view commands return an `Outcome` the frontend completes.
  5. egui's `Editor` holds a `Session` and forwards. Keep its public methods the tests use
     (`document()`, `selection()`, `select_nodes`, `add_from_picker`, …) as thin forwards.
- **Tests:** in `aic-editor`: paste offsets grow per paste; duplicate selects the copy; delete of a
  link selection; make workspace refuses an empty or invalid selection with the status text;
  dissolve selects what was inside; a refused link sets `status()`; `add_from_picker` from an input
  port links the new node as producer and returns `Anchor::Right`; `Command::ALL` order puts every
  shift chord before its plain twin; copy text round-trips through paste text.
- **Done when:** green; egui snapshots unchanged; `Editor`'s non-drawing methods are one-line
  forwards or gone.

#### A6. `Session` files, autosave and settings
- **Goal:** the file rules (drafts, autosave, recent, reopen, title) in `aic-editor`; only dialogs
  and storage plumbing stay in egui.
- **Depends on:** A5.
- **Read first:** `files.rs` (all), `lib.rs` (`App::new`, `from_folder`, `reopen_last`, `save`),
  `store.rs`, PLAN M6 "Autosave", M9 (browser files).
- **Steps:**
  1. Move into `Session` (or a `Files` part it owns): `drafts_dir`, `store`, `recent_files`,
     `autosave: Throttle`, `saved_view`, `last_saved`, `save_failed`, and the methods `start_new`,
     `open_file`, `reopen`, `read_file`, `save`, `save_now`, `save_to`, `save_as_named`,
     `offered_name`, `needs_saving(view)`, `write(view)`, `new_draft_path`, `is_in_drafts`,
     `drop_empty_draft`, `remember_recent`, `forget_recent`, `title()`, `file_label`,
     `file_name`. The camera is passed in (`Option<CanvasView>`), never read from a canvas.
  2. `Session::autosave(now, busy, view) -> Poll` wraps the throttle; egui computes `busy` from
     pointer and focus as today and handles `Poll::Wait` with `request_repaint_after`.
  3. `aic_editor::Settings` (serde): data choice, availability, recent files, last document, show
     performance. Keep the separate eframe keys the egui app uses today (so existing users keep
     their settings); `Settings` is what the iced app writes as one file.
  4. egui keeps: `rfd` dialogs, the browser upload/download (`web.rs`, `Upload`, `Download`), the
     Save As prompt window, `handle_close_request`, sending the title to the viewport.
- **Tests:** in `aic-editor`, against a temp dir (`std::env::temp_dir()` + unique name; no new
  dev-dependency needed): first change on a new graph creates a draft; Save As from a draft deletes
  it; an empty draft is deleted when left; reopening a missing last document starts a new draft
  and sets the status; autosave waits while busy and saves the last change; the title marks a
  modified file; writes go through `<file>.tmp`.
- **Done when:** green; egui snapshots unchanged; egui's `files.rs` holds only dialogs, browser
  plumbing and viewport calls.

#### A7. Enforce the separation
- **Goal:** a check that fails when a crate gets a dependency it must not have.
- **Depends on:** A2 (extend it once `aic-app-iced` exists in B1).
- **Steps:**
  1. `scripts/check-frontends.sh`: for each crate, `cargo tree -p <crate> -e normal,dev,build --prefix none`
     and grep for forbidden crates per the table in section 3 (`egui`, `eframe`, `egui-snarl`,
     `egui_dock`, `egui_kittest`, `iced`, `iced_*`, `wgpu`, `winit`, `rfd`). Fail with the crate and
     the offending dependency named. If `aic-app-iced` doesn't exist yet, skip it.
  2. Mention the script in `CLAUDE.md` rule 9's list of end-of-milestone checks.
- **Tests:** run it; then add `egui = "0.36"` to `aic-editor` temporarily and see it fail; remove.
- **Done when:** the script passes on the tree and fails on a planted violation.

### Phase B: iced foundation and the gate

#### B1. `aic-app-iced` skeleton: boot, data, theme, fonts
- **Goal:** `cargo run -p aic-app-iced` opens a themed, empty window on the real data, or an error
  screen.
- **Depends on:** A3, A6, A7.
- **Read first:** the `iced` skill (application/daemon, theme, fonts), section 6, `main.rs` and
  `lib.rs` of the egui app, `DESIGN.md`.
- **Steps:**
  1. Create the crate (section 6.1). `main.rs` parses `--data`, finds the data with
     `DataFolder::find`, loads `settings.json` (6.5), builds a `Session`, and runs
     `iced::daemon(App::boot, App::update, App::view)` with one main window (1280×800, title from
     `session.title()`). Use a daemon from the start: C15 needs extra windows.
  2. `theme.rs`: `Theme::custom("AIC Modeler", Palette { .. })` from `aic_editor::style`, and the
     style fns listed in 6.4. Load the four fonts and set the default font.
  3. A data failure shows an error screen with the message (Fluent string the egui app uses).
  4. Add `aic-app-iced` to `scripts/check-frontends.sh`.
  5. On Windows release builds, no console window (`windows_subsystem`), as in egui's `main.rs`.
- **Tests:** `App` boots on the fixture catalog; the title is "Untitled — AIC Modeler" (from
  Fluent); a snapshot of the empty window (`empty_window`); a snapshot of the error screen.
- **Done when:** the window opens with the right fonts and colours (compare to egui's
  `empty_graph.png`); `cargo tree -p aic-app-iced | grep -i egui` is empty; checks green.

#### B2. Spike: can iced_nodegraph carry this canvas? (gate)
- **Goal:** a measured go/no-go before any canvas task starts.
- **Depends on:** B1.
- **Read first:** the `iced_nodegraph` skill and its `references/` (all of it, especially the
  callback contract and `example-basic.md`), PLAN M4 (the egui perf spike and its numbers),
  `crates/aic-app-egui/examples/perf_spike.rs`, egui's `canvas.rs` (the features below).
- **Steps:** build `crates/aic-app-iced/examples/spike.rs` (throwaway, not wired into the app) and
  answer each question with evidence (a test, a measurement or a screenshot):
  1. **Size:** 300 cards (300 px wide, 5 ports a side, item icon pins, a text field), 600 edges,
     a live rate on every port and every link. Release build. Mean and p95 frame time idle,
     panning, zooming, dragging a node and dragging a new edge (use `on_info` timings plus
     wall-clock frame intervals, vsync off if iced allows). **Pass:** ≤ 8 ms on Max's M1 Pro, as
     in M4.
  2. **Link labels:** a label at each edge's midpoint that pans and zooms with the graph. iced_nodegraph
     has no edge labels. Try, in order: (a) deterministic card layout so the host knows each pin's
     world position, plus an overlay layer (`stack!`) drawn with the camera from `on_camera`;
     (b) routing every edge through an anchor at its midpoint and labelling the anchor. Report which
     works and how far the label drifts from the drawn curve.
  3. **Workspace ports pinned to the canvas edges** while panning and zooming: port nodes whose world
     position is recomputed from the camera each frame, or another way. Edges must still attach.
  4. **Mouse map:** can we get egui's map (left-drag on empty canvas pans, right-click opens the
     picker, right-drag box-selects, Shift+click toggles) by rebinding `Keymap::pan_button` and
     wrapping the graph in `mouse_area`? If not, report the closest we can get without forking.
  5. **Port gestures:** double-click on a pin (port menu), right-click on an open pin (picker), and
     Cmd+drag on a pin (remove its links), alongside edge drags from the same pin.
  6. **Dropped wire:** an edge drag released on empty canvas → the picker at that point, from that
     port (via `on_drag_start` with `DragInfo::Edge`, `on_drag_update`, `on_drag_end` and no connect).
  7. **Live connect semantics:** `on_connect` fires on snap, not release. Show the host can turn it
     into one link on release (and nothing for a drag that ends elsewhere).
  8. **Fields in cards:** typing in a `text_input` inside a card doesn't drag the card or trigger
     graph keys (Delete, Cmd+D, Cmd+A).
  9. **Text at zoom:** card text at 25%, 100% and 200%. Is it sharp at 200% (egui renders at 200%
     and shrinks)? Screenshots at scale 1 and 2.
  10. **Host-owned state:** selection fed from the host (`Node::selected`) and camera fed from the host
      (`.camera`), so two graph widgets on one graph (two views) and a zoom slider work.
  11. **Two graphs at once** in two panes and in a second window (`daemon` + `window::open`), each
      with its own camera.
- **Output:** append "B2 spike results (date)" to section 9 of this file: a table of the 11
  questions (pass / workaround / fail, with the evidence), the frame times, and a verdict:
  - **Go:** every question passes or has a workaround that doesn't fork iced_nodegraph.
  - **Go with fork:** something needs a small patch to iced_nodegraph. Name it. Max decides
    whether to vendor a fork (a `[patch.crates-io]` git dependency is a third-party fetch at build
    time; ask first).
  - **No-go:** the size test fails after reasonable tuning, or labels/ports/gestures can't be done.
    Then stop: tasks C3–C7 and C12 get re-planned on a custom iced widget, with Max.
- **Done when:** the results are written and Max has read the verdict. Leave the spike in
  `examples/` until C16; it's how a later iced_nodegraph upgrade gets re-checked.
- **Pitfalls:** iced_nodegraph renders only under wgpu, so run it on the Mac, not in CI. Node
  ids must be unique per frame. `can_connect` replaces the built-in rules, so combine with
  `default_can_connect`.

### Phase C: features (start only after B2 says go)

Unless a task says otherwise, every C task adds Fluent strings only under `# iced only` when egui
has no matching string, takes all styling from `theme.rs`, and adds at least one snapshot of what it
built.

#### C1. Shell: menu bar, toolbar, status bar, side panels
- **Goal:** the window frame of the egui app, with commands wired to `Session::run`.
- **Depends on:** B2 (go).
- **Read first:** egui `editor.rs` (`top_bar`, `file_menu`, `edit_menu`, `region_picker`,
  `status_bar`), `toolbar.rs`, DESIGN.md (Layout, Toolbar), `docs/ref/aic-ui-0.png`.
- **Steps:**
  1. Menu bar: File (New, Open, Open Recent ▸, Save, Save As, Export PNG), Edit (every edit
     command), View (Round up, panels, show performance, view commands), the panel toggles,
     region picker (`pick_list`), the data version label, Settings… (iced has no menu widget:
     a row of buttons; a click opens a dropdown in an overlay layer, closed by a click outside or
     Escape).
  2. Toolbar: one 30×26 button per `Command` in egui's order, tooltip = label + chord text. The
     chord text comes from `Chord` (⌘ on macOS, Ctrl elsewhere), formatted in `aic-editor` so both
     frontends agree.
  3. Status bar: status message (left), save state (right) as egui shows it. Zoom and performance
     come in C13; leave room.
  4. Left panel ~240 px and right panel ~300 px, collapsible from the toggles. Resizable panels are
     nice to have; if you do them, use a 4 px drag handle (`mouse_area` + `on_move`), not `pane_grid`.
  5. The canvas area shows a placeholder until C3.
- **Tests:** clicking each menu item sends the right `Message` (Simulator); a disabled Undo when
  there's nothing to undo; snapshots `shell`, `file_menu_open`, `panels_hidden`.
- **Done when:** the shell matches egui's layout within a few pixels at 1280×800.

#### C2. Recipe browser
- **Depends on:** C1.
- **Read first:** egui `browser.rs`, `aic_editor::browser`, DESIGN.md "Known Gaps" (frameless rows).
- **Steps:** search field (focus on Cmd+F is a nice-to-have), category `pick_list`, a `scrollable`
  list of rows (icon, region badges, name). A click adds the recipe's node at the focused view's
  canvas centre and selects it. Icons via `icons.rs` (`image::Handle::from_bytes(catalog.image(..))`,
  cached; an item with no image is a coloured square with initials, as egui does).
- **Tests:** typing filters to the expected recipes (same as egui's test data); a click adds a node
  (`session.document().graph().nodes().len()` grows by one) and selects it; snapshot `browser`.

#### C3. Canvas: cards and ports (drawing only)
- **Goal:** the graph and its solution drawn with iced_nodegraph, read-only.
- **Depends on:** B2 (go), C1.
- **Read first:** egui `canvas.rs` (`Viewer`, `show_header`, `show_input`/`show_output`,
  `port_row`, `pin`, `title_row`, `workspace_body`, `big_number`), DESIGN.md "Node Card", the spike's
  findings, egui snapshots `golden_07.png`, `logistics_and_throttle.png`, `outpost_order.png`.
- **Steps:**
  1. `canvas::view(session, view_state) -> Element<CanvasMsg>`: one `node(...)` per member of the
     view's level, using a custom `Ids` (`NodeId = Member`, `PinId = (Side, usize)` or a `PinTarget`
     key, `EdgeId = LinkId`). One `edge(...)` per link drawn at this level (use the same
     pin-index rule as egui: `pin_targets`; move `pin_targets`, `shown` and `port_ref` into
     `aic-editor` if they aren't there yet, since both frontends need them).
  2. Cards per kind, as listed in section 7, with header tints from `style::color::node`, the
     kind line, title (semibold), facility line, facility image (64 px), the big number,
     undetermined values blank. Limit fields are drawn but read-only until C7.
  3. Ports: 48 px item-icon pins, mono rates, colour by state (linked, depot in, depot out), throttle
     marker on the throttling port, region warning sign on the title.
  4. Follow the spike's deterministic layout if labels (C5) depend on it.
- **Tests:** a graph with every node kind renders without panic; the golden 7 graph shows the
  counts the solver gives (find the texts `"1.13"`, `"1.75"`); snapshots `cards_every_kind`,
  `golden_07`.

#### C4. Canvas editing: move, link, select, delete, camera
- **Depends on:** C3.
- **Read first:** egui `canvas.rs` (`move_selection_together`, `right_drag_selection`,
  `box_selection`, `wheel_zoom`, `connect` refusals), `editor.rs` `apply`, spike results 4, 5, 7, 10.
- **Steps:**
  1. `on_move` → `Session::move_members` (one undo step per drag, see 6.3).
  2. Connect on release (spike 7): a refused link sets the status bar text from `aic_editor::messages`.
     `on_disconnect` of an existing link removes it. Cmd+drag on a pin removes its links.
  3. Selection is host-owned and shared across views: `Node::selected` from `session.selection()`;
     click selects, Shift+click toggles, the box gesture selects (mapped as the spike decided).
     Clicking empty canvas clears.
  4. Delete/Backspace, Cmd+D, Cmd+A go through `Command`, not iced_nodegraph's keymap: set
     `Keymap` to leave them to the app so behaviour matches egui (deleting a workspace removes its
     contents, etc.).
  5. Camera per view in `Views`, fed back with `.camera(..)`; wheel zoom limits match egui
     (`ZOOM_STEPS`, min/max).
- **Tests:** (Simulator or messages) dragging a node changes its `pos` and is one undo step; a
  link between mismatched items is refused with the status text; Shift+click toggles; Delete removes
  the selection; undo restores it.

#### C5. Link labels and link selection
- **Depends on:** C4.
- **Read first:** egui `canvas.rs` link label drawing, DESIGN.md "Links and Link Labels",
  `docs/ref/aic-defect-link-label.png`, spike result 2.
- **Steps:** label `strings.format("link-label", flow, belts)` (pipes for fluids) at the midpoint,
  12 px on `INSET` at 90%, radius 3, pad 3; amber text over one belt; a click on the label selects
  the link (1.5 px accent outline); Delete removes it.
- **Tests:** test 7's graph shows `"33.75 /min · 1.13 belts"` (or whatever the shared formatter
  prints); selecting a label sets `Selection::Link`; snapshot `link_labels`.

#### C6. Node picker
- **Depends on:** C4.
- **Read first:** egui `picker.rs` (drawing part), `aic_editor::picker`, `editor.rs`
  `add_from_picker`/`picker_ui`, PLAN M4 "Node picker", `docs/ref/satisfactory-modeler-recipe-selector.png`,
  spike results 4–6.
- **Steps:** a popup (overlay layer, `RAISED`, popover shadow) opened by right-click on empty
  canvas, by a dropped wire, and by right-click on an open port. The search field is focused on
  open (`widget::operation::focus`). Name/Inputs/Outputs toggles, built-ins column, recipe rows
  (input icons, name, output icons, region badges), and from the canvas the empty workspace entry.
  One click calls `Session::add_from_picker` and closes. Apply the returned `Anchor` once the new
  card's size is known (or from the deterministic layout). Escape or a click outside closes it.
- **Tests:** from an input port of `iron_ingot` (fixture) only producers are listed plus Source;
  one click adds and links (graph has the link); typing filters; snapshots `picker_canvas`,
  `picker_from_port`.

#### C7. Card fields, facility cycling, port menu, ratio locks
- **Depends on:** C4.
- **Read first:** egui `widgets.rs` (limit field), `canvas.rs` (`limit_field`, `facility_line`),
  `port_menu.rs`, PLAN M4 "Port menu", M7 "Ratio locks".
- **Steps:** limit field on recipe/source/sink/control port cards: shows the limit unless being
  typed; applies on each parse (`aic_editor::settings::parse_amount`); unparseable text turns red
  and changes nothing; typing is one undo step (6.3). Facility line cycles facilities on click when
  the recipe has several. Double-click a pin opens the port menu beside it (away from the card): limit
  as a rate (`Session` → `set_port_limit`), share fields per link when the port has several, Clear
  connections.
- **Tests:** typing `5` sets the limit and re-solves; `abc` changes nothing; a whole typed number is
  one undo step; a share typed on one of two links locks the ratio (solution flows follow); snapshot
  `port_menu`, `ratio_lock`.

#### C8. Inspector, totals, warnings
- **Depends on:** C4.
- **Read first:** egui `inspector.rs` (all), `aic_editor::messages`.
- **Steps:** the right panel as egui shows it: nothing selected, one node (kind, facility picker,
  count, round-up line, rate, coupons, balance/pass-through, limit field, throttle reason, port list
  with depot rows, Remove), several nodes (Duplicate, Remove), a workspace (name field, ports,
  Open, Dissolve, Remove), a link (flow, belts, Remove). Totals: incomplete note, From the depot, To
  the depot, Power, Facilities (with rounded-up total), Raw resources vs availability, Coupons;
  Warnings. Use collapsible sections (a button with ▸/▾ and a bool in `UiState`).
- **Tests:** snapshots `inspector_node`, `inspector_link`, `totals_golden_07`; the throttle text on
  golden test 3's graph names the sewage port.

#### C9. Keyboard, undo steps, clipboard, toolbar flash
- **Depends on:** C4.
- **Read first:** egui `edit.rs` `keyboard`, `paste_text`, `flash`; section 6.3.
- **Steps:** `keyboard::listen()` → match `Command::ALL` in order against the chord; skip commands
  that don't `works_while_typing` while a field is focused; skip while a popup is open the same way
  egui does. Copy/Cut write `session.copy_text()` with `iced::clipboard::write`; Paste reads with
  `clipboard::read` and calls `paste_text`. A key press flashes its toolbar button 400 ms
  (a `time::every` tick only while a flash is live).
- **Tests:** hand-built key events (6.6): Cmd+Z undoes, Cmd+Shift+Z redoes, Cmd+Shift+S is Save As
  not Save, Delete while typing in a limit field doesn't delete the node; copy then paste adds
  offset copies.

#### C10. Files and autosave
- **Depends on:** C1, A6.
- **Read first:** egui `files.rs`, `lib.rs`, PLAN M6 "Autosave"; section 6.5.
- **Steps:** File menu actions through `Outcome`: `rfd::AsyncFileDialog` for open/save (filter
  `.aicm`), run in `Task::perform`; Open Recent; the autosave subscription (6.3); reopen the last
  document on start; save on close request; window title from `session.title()`
  (`window::set_title`... check the 0.14 API, or `title(&self, window)` on the daemon); the data
  version banner when a file was made with other data; persist `settings.json`.
- **Tests:** with a temp drafts dir: a first edit creates a draft; Save As (bypass the dialog by
  sending its result message) writes the file and deletes the draft; the title shows the modified
  mark; a close request saves.

#### C11. Settings window
- **Depends on:** C10.
- **Read first:** egui `settings.rs` (window part), PLAN M4 "Raw resource availability", M7 Settings.
- **Steps:** a modal-free window over the editor (overlay, `RAISED`, window shadow, radius 8, close
  with Escape or ✕): Data (version `pick_list`, language `pick_list`, switch through
  `Session::switch_data`) and Raw resources (a tab per region, one row per raw resource with icon,
  name and a supply field; blank is unlimited; unparseable text red). Changes apply at once and
  persist.
- **Tests:** typing a supply below use raises `ResourceOverUse` in the warnings; switching version
  keeps the graph; snapshot `settings_data`, `settings_resources`.

#### C12. Workspaces
- **Depends on:** C4, C8, spike result 3.
- **Read first:** ADR 0007, egui `canvas.rs` (`level_bar`, `pin_ports_to_edges`, `workspace_body`),
  `edit.rs` workspace commands, PLAN M7 "Workspaces".
- **Steps:** workspace cards (violet header, port pins); double-click or Open shows its level in
  the view (per-view level); middle-click or Cmd+click opens it in a new tab (needs C14, so
  leave a TODO linked to C14); Back and the path bar over the canvas; ports pinned to the left and
  right canvas edges as the spike showed; + Input / + Output; Cmd+G / Cmd+Shift+G; rename in the
  inspector; Paste into from the workspace's menu; an unlinked port end shows the warning.
- **Tests:** make a workspace from two linked nodes and a crossing link (routes through new ports);
  open it, the view's level changes and the selection clears; dissolve restores the direct link;
  snapshots `workspace_card`, `workspace_inside`.

#### C13. Round-up, zoom control, performance readout, export PNG
- **Depends on:** C4.
- **Read first:** egui `editor.rs` (`zoom_control`, `performance`), `export.rs`, PLAN M6 round-up.
- **Steps:** Round up toggle (menu + toolbar) switches cards to `ceil(count) × utilization` via
  `aic_core::RoundUp`. Zoom slider and `−`/`+`/percent in the status bar acting on the focused
  view; fit selection or all (the `F`/Home behaviour from iced_nodegraph's `focus` task is fine).
  Frame rate (mean over the last second) and last solve time when show performance is on; a solve
  over 16 ms is reported. Export PNG: `window::screenshot(main)` → crop to the focused canvas's
  bounds (`widget::selector::find` or a `sensor`) → `aic_editor::export::crop_png` → save dialog.
- **Tests:** the round-up card text on a 1.75 node reads `2 × 88%`; zoom slider messages change
  the view's camera; `crop_png` gets its own unit tests in A-phase code (move egui's).

#### C14. Multiple views: tabs and splits
- **Depends on:** C4, C12.
- **Read first:** ADR 0008, egui `views.rs`, PLAN M10, the `iced` skill's `api-pane-grid.md`.
- **Steps:** `Views` = `pane_grid::State<TabGroup>` where a `TabGroup` holds tabs, each tab a view
  (`level`, `camera`). One view: no tab bar. Tab names as egui (region at the top, workspace name
  inside, "· 2" for duplicates). Split right (Cmd+\\), Split down (Cmd+Shift+\\), Duplicate view
  (Cmd+Shift+D), Close view (Cmd+W, never the last). The focused view (last clicked) gets the 1 px
  accent line under its tab and receives view commands. One selection, one solve, one undo for all
  views. A view whose workspace is gone shows the top. Layout persisted in `settings.json`; a file
  saves the focused view's camera. Dragging a tab onto a pane edge to split is a stretch goal: first
  do it from commands and the tab menu.
- **Tests:** an edit in one view shows in the other in the same `view` call; selecting in one
  view selects in both; dissolving the workspace a view shows moves it to the top; snapshot
  `views_split`.

#### C15. Detached view windows
- **Depends on:** C14.
- **Read first:** ADR 0008 (detached windows show only the canvas), egui `views.rs` window part,
  the `iced` skill (daemon, `window::open`, `close_events`).
- **Steps:** Move to new window (Cmd+Shift+N) opens a `window::open` window showing only that view's
  tabs and splits. Closing it closes its views, never the document. Window places persisted.
  Commands stay in the main window; keys pressed in a detached window still reach the session.
- **Tests:** moving a view to a window removes it from the main pane grid and `view(window)` for the
  new id shows it; closing the window drops its views; the app quits only when the main window closes.

#### C16. Parity review and wrap-up
- **Depends on:** all C tasks.
- **Steps:** walk section 7 against the running iced app side by side with the egui app on the same
  `.aicm` file; fix small gaps, write the rest in section 9. Rebuild golden test 5's and test 7's
  graphs by hand in the iced app (PLAN M4 "Done when") and check the numbers. Measure the iced app
  with a release build on the 201-node graph egui's `perf_app` uses and record the frame times next to
  egui's. Optionally add `scripts/package-macos.sh` / `package-windows.ps1` support for the iced
  binary. Delete or keep `examples/spike.rs` (Max decides). Update `docs/PLAN.md` with a short
  "iced frontend" section saying what was built and how it compares.
- **Done when:** Max has the comparison and decides what happens to the experiment.

---

## 9. Deviations and open questions

Add to this list as you go: date, task, what differs from egui or the plan, and why.

- (2026-11-15, plan) iced has no menu bar or menu widget; C1 builds dropdowns from buttons and an
  overlay. Native OS menus are out of scope.
- (2026-11-15, plan) The iced app keeps its own drafts and settings (6.5), so it doesn't reopen the
  egui app's last document. Files themselves are shared.
- (2026-11-15, plan) No web build for iced (decision 4).

---

## 10. Status

| Task | Title | Depends on | Status |
|---|---|---|---|
| T0 | ADR 0009 and plan updates | — | todo |
| A1 | Rename `aic-app` → `aic-app-egui` | T0 | todo |
| A2 | `aic-editor` with the pure modules | A1 | todo |
| A3 | Design tokens and fonts in `aic_editor::style` | A2 | todo |
| A4 | Split the document | A2 | todo |
| A5 | `Session`: selection, editing, `Command` | A4 | todo |
| A6 | `Session`: files, autosave, settings | A5 | todo |
| A7 | `scripts/check-frontends.sh` | A2 | todo |
| B1 | `aic-app-iced` skeleton | A3, A6, A7 | todo |
| B2 | iced_nodegraph spike (gate) | B1 | todo |
| C1 | Shell | B2 | todo |
| C2 | Recipe browser | C1 | todo |
| C3 | Canvas: cards and ports | B2, C1 | todo |
| C4 | Canvas editing | C3 | todo |
| C5 | Link labels | C4 | todo |
| C6 | Node picker | C4 | todo |
| C7 | Card fields, port menu, ratio locks | C4 | todo |
| C8 | Inspector, totals, warnings | C4 | todo |
| C9 | Keyboard, undo steps, clipboard | C4 | todo |
| C10 | Files and autosave | C1, A6 | todo |
| C11 | Settings window | C10 | todo |
| C12 | Workspaces | C4, C8 | todo |
| C13 | Round-up, zoom, performance, export | C4 | todo |
| C14 | Multiple views | C4, C12 | todo |
| C15 | Detached windows | C14 | todo |
| C16 | Parity review | all | todo |

Tasks that can run side by side (separate agents, separate worktrees): A3 with A4; A7 any time
after A2; C2 with C3; C5–C9 and C13 once C4 is done; C10 and C11 alongside the canvas tasks.
