# AIC Modeler

Graph-first factory planner for the AIC in *Arknights: Endfield*. Rust, egui + egui-snarl, LP solver via `good_lp`.

Read before working:

- `docs/PLAN.md`: scope, solver semantics, milestones and golden tests.
- `GLOSSARY.md`: the domain vocabulary. Use these terms in code, UI and docs.
- `docs/adr/`: decisions that are hard to reverse. Don't work against them without asking.

## Ground rules

1. **The UI never does math.** All solving lives in `aic-core` and all non-drawing app logic in `aic-editor`; neither has a GUI dependency. A UI builds a `Graph`, calls `solve(&catalog, &graph)`, and renders the `Solution`.
2. **Internal IDs are game IDs** (`copper_nugget`, `furnance_1`), never display names. Display names come from the i18n table at render time.
3. **All rates are items per minute** inside the app. Convert once, in the data layer.
4. **Data loading goes through our own schema.** Only `crates/aic-data/src/raw.rs` knows the factoriolab format. Everything else uses `Catalog`.
5. **Data is copied in by hand, not fetched.** No submodules, no clone scripts, no third-party network access at build or run time. Never download game data.
6. **Use the glossary's terms in code.** `Facility`, not `Machine`. `Link`, not `Edge`. `Region`, not `Location`. Two exceptions: `raw.rs` uses factoriolab's names, and `crates/aic-node-graph`, a general widget, keeps egui_graph's (edge, socket). `aic-app` turns links into edges and ports into sockets (ADR 0011).
7. **Every user-facing UI string goes through Fluent.** No string literals in widgets.
8. **Player values are limits and the solver maximizes** (ADR 0003). Never reintroduce exact targets or a "minimize facilities" objective.
9. Every milestone ends with `cargo test --workspace`, `cargo clippy --workspace -- -D warnings` and `scripts/check-frontends.sh` (no crate gets a dependency ADR 0010 forbids) passing.

## Working here

- **Keep the app light.** Players run it beside the game, most of them on Windows, so it competes with the game for CPU and GPU. Idle must cost nothing: no repaint loops, and no work redone every frame that could be done once per solve or edit. A frame while interacting stays under 8 ms. Measure with `cargo run --release -p aic-app --example perf_app`; `PERF_HEADLESS=1` times the UI's CPU work alone, and `PERF_IDLE=1` and `PERF_MINIMIZE=1` check that idle and minimized windows stop drawing.
- **Look at the UI through kittest snapshots**, not a launched window. Run `UPDATE_SNAPSHOTS=1 cargo test -p aic-app`, then read `crates/aic-app/tests/snapshots/<name>.png`. Add a snapshot test for each new screen or state you need to see, and review changed PNGs before calling a UI change done. `UPDATE_SNAPSHOTS=1` only rewrites images whose diff passes kittest's threshold, so after a theme or color change delete `crates/aic-app/tests/snapshots/*.png` first, then regenerate. Keep `egui_kittest` on the same minor version as eframe.
- **Load the Rust skills before a milestone.** From m01–m15 (ownership, type-driven design, error handling, domain, ecosystem, domain errors, anti-patterns), plus `tdd`. Skip the ones that don't apply, such as concurrency when nothing is async, and say which you skipped.
- **Commit trailers:** end commit messages with the `Co-Authored-By: Claude …` line only. Leave out the `Claude-Session:` link, in pull request descriptions too.
