# The canvas is drawn by our own copy of egui_graph

The node canvas moves from egui-snarl to `aic-node-graph`, a crate in this workspace. It starts as a copy of [egui_graph](https://github.com/nannou-org/egui_graph) 0.19.1 (MIT) and is changed here as the app needs. Three reasons:

- **Speed.** egui-snarl lays out every node each frame, on screen or not; on Windows that was about half of a frame (PLAN M7). egui_graph skips the content of a node out of view and keeps its size and socket positions, so its links still draw. In the spike of 2026-11-17 (300 nodes, 600 links, headless, Windows) a view with 35 nodes on screen took 4.7 ms a frame against egui-snarl's 10.8 ms. With all 300 on screen the two were level at about 15.5 ms.
- **Fit.** egui_graph keeps no graph of its own: the app shows its nodes and links each frame. egui-snarl needs a second graph kept in step with the document (`DocView`), and the canvas works around its selection, its panning, its camera fit and its pin clicks.
- **Control.** Two things the canvas needs are inside egui_graph, not behind its API. A press anywhere on a node's frame drags the node, so a link can't start from a port's icon inside the card. And its mouse conventions are fixed (middle-drag pans, a drag on the background selects, Ctrl+click toggles) where ours differ. That is why it is a copy and not a dependency. Upstream also made three breaking releases in three weeks of October 2026.

ADR 0005 was reserved for "egui + egui-snarl is the shipping UI" and waited on the Windows measurements. It was never written; this ADR takes its place. [ADR 0010](0010-one-frontend-editor-crate-stays.md) stands, with `aic-node-graph` where it names egui-snarl.

## Considered Options

- **A patched copy of egui-snarl that skips off-screen nodes.** Rejected: it gains the culling and keeps the second graph and every workaround. Its cards are also built from fixed parts (header, pins, body), which leaves less room for cheaper cards when zoomed out.
- **egui_graph as a dependency from crates.io.** Rejected: ports and the mouse would have to change to suit the library, and every upstream release would be a port.
- **Send the changes upstream and depend on the result.** Rejected for now: the mouse conventions are this app's taste, and the migration shouldn't wait on someone else's review. A fix of general use can still go upstream.
- **A canvas written from scratch on egui's `Scene`.** Rejected: egui_graph is that already, with tests.

## Consequences

- `aic-node-graph` is MIT only. The rest of the workspace was MIT OR Apache-2.0 when this was decided and has been MIT only since 2026-11-24. The crate keeps egui_graph's `LICENSE-MIT` with its copyright notice, and its `Cargo.toml` says `license = "MIT"`.
- The crate is a general widget and knows nothing of the AIC. It depends on egui and on no `aic-*` crate, and keeps egui_graph's words: edge, socket. `aic-app` turns links into edges and ports into sockets. This is the second exception to "use the glossary's terms in code", after `raw.rs`.
- `aic-data`, `aic-core` and `aic-editor` must not depend on it; `scripts/check-frontends.sh` checks that.
- Upstream fixes are merged by hand. `crates/aic-node-graph/SOURCE.md` names the commit copied and lists every change made here. egui_graph's automatic layout is left out of the copy.
- The pin on eframe 0.36 was egui-snarl's. From now on, moving to a newer egui means porting this crate ourselves.
- A node out of view runs no content, so it is missing from the accessibility tree: a test must bring a node into view before it finds it by label. A card whose field has keyboard focus must not be skipped.
- Skipping off-screen nodes does nothing for a view zoomed out to the whole graph. That view was still about 15 ms in the spike, over the 8 ms budget, and needs its own work.
