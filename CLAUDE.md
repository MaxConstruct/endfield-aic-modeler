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
6. **Use the glossary's terms in code.** `Facility`, not `Machine`. `Link`, not `Edge`. `Region`, not `Location`. Only `raw.rs` uses factoriolab's names.
7. **Every user-facing UI string goes through Fluent.** No string literals in widgets.
8. **Player values are limits and the solver maximizes** (ADR 0003). Never reintroduce exact targets or a "minimize facilities" objective.
9. Every milestone ends with `cargo test --workspace` and `cargo clippy --workspace -- -D warnings` passing.
