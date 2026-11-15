# One frontend; the editor crate stays

ADR 0009 put a second desktop UI, in iced 0.14 with iced_nodegraph 0.5, beside the egui one, over a shared GUI-free crate, `aic-editor`. The iced UI is dropped, and egui with egui-snarl stays the only UI. `aic-editor` stays: the egui app keeps its document, undo, selection, editing commands, files, autosave, settings, Fluent strings, style tokens and built-in data there, and draws only.

```
aic-data  <-  aic-core  <-  aic-editor  <-  aic-app  (eframe, egui-snarl, egui_dock)
```

The iced_nodegraph spike (2026-11-15, recorded in `docs/archive/TASK_GUI_ICED.md`) failed on size. 300 cards and 600 links took 43 to 48 ms a frame on the M1 Pro against a budget of 8 ms, almost all of it GPU time in iced_nodegraph's own shader, which no setting on our side brought near the budget. iced 0.14 itself also showed a frame floor that already breaks the budget on this Mac with two cards. egui measured 5.6 to 8.6 ms for the same graph on the same day.

## Considered Options

- **Re-plan the iced canvas on a custom iced widget.** Rejected: it is a large piece of work for a second UI the app doesn't need, and iced's own frame floor might fail the budget anyway.
- **Discard the whole experiment and go back to `main` as it was.** Rejected: separating the editor logic from the drawing is useful with one UI too. The logic is tested without a GUI, the review of the split fixed a flaky snapshot test, and undoing the split is work that gains nothing.

## Consequences

- `aic-editor`, `aic-core` and `aic-data` have no GUI dependency, and `aic-editor` builds for wasm32. `scripts/check-frontends.sh` enforces this.
- `aic-app-egui` is renamed back to `aic-app`.
- The iced code stays on the git tag `exp/iced-spike`, for whoever wants to measure a later iced_nodegraph.
- Two things in `aic-editor` exist only because of the second UI: the toolkit-neutral `Command` and `Chord`, and the egui app's `DocView`, which keeps snarl in step with the document by its generation. They stay; they are small, tested, and a toolkit-neutral command list is still the natural seam for key bindings.
