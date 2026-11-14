# Two frontends share one GUI-free editor crate

The app gets a second desktop UI, built in iced 0.14 with iced_nodegraph 0.5, next to the egui one. PLAN M4 said that a failed performance spike would reopen the framework question and that iced was the comparison; the egui spike passed, and the iced UI is now an experiment to see how it compares, not a replacement. Both UIs sit on one crate, `aic-editor`, that holds everything an editor does that isn't drawing: the document and its undo, the selection, editing commands, files and autosave, the Fluent strings, number formatting, settings models, the design tokens and the built-in data. It has no GUI dependency and stays wasm-clean.

```
aic-data  <-  aic-core  <-  aic-editor  <-  aic-app-egui  (eframe, egui-snarl, egui_dock)
                                        <-  aic-app-iced  (iced, iced_nodegraph)
```

The frontends never depend on each other. `aic-editor`, `aic-core` and `aic-data` depend on no GUI toolkit. `scripts/check-frontends.sh` enforces the table in `docs/TASK_GUI_ICED.md` section 3.

## Considered Options

- **iced replaces egui.** Rejected: the egui UI ships and works, including the web build; iced_nodegraph is a young crate with one maintainer, and nobody knows yet whether it carries this canvas.
- **Separate crates with copied logic.** Rejected: every rule (undo steps, refusals, autosave, clipboard) would exist twice and drift, and the two UIs could stop opening each other's files.
- **A shared crate, egui untouched.** Rejected: the logic that would be shared is tangled into egui's `Editor` and `Document` (snarl state, levels), so the iced UI would have to copy it anyway. Moving the egui app onto `aic-editor` first is what makes the sharing real, and it keeps the two honest against each other.

## Consequences

- Two wgpu versions end up in `Cargo.lock` (eframe's and iced's). They are never linked into one binary.
- `aic-editor` must stay GUI-free and wasm-clean, so a web build of either UI stays possible. Dialogs, the system clipboard, screenshots and view layout are the frontend's; the session returns what to do next.
- Both UIs read one string table, `crates/aic-editor/i18n/en.ftl`, and one set of design tokens, `aic_editor::style`.
- A `.aicm` saved by one UI opens in the other; the save format does not change.
- Each UI keeps its own app data (settings, drafts), so running both at once never has them autosave over each other.
- `aic-app` is renamed `aic-app-egui`; the new crate is `aic-app-iced`.
- The egui UI stays the shipping UI until the player says otherwise.
