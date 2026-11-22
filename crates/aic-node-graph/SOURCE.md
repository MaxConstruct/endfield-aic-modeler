# Where this crate came from

- `src/`: copied from [nannou-org/egui_graph](https://github.com/nannou-org/egui_graph) 0.19.1,
  commit `1a5e2b5385c8c81a49cbde836317848b06849966`, on 2026-11-17, with the tests in its files.
- `LICENSE-MIT`: egui_graph's licence, copied unchanged. This crate is MIT only, while the rest of
  the workspace is MIT OR Apache-2.0 (ADR 0011).
- Upstream fixes are merged by hand. The code keeps egui_graph's words (edge, socket), not the
  project glossary's.

## Left out

- `src/layout/`, the automatic layout, and the `layout` feature with what it gated: the
  `pub mod layout` and its re-exports in `lib.rs`.
- The `serde` feature with what it gated: the `cfg_attr` serde derives on `ResizeBehavior`,
  `AutoPan`, `Snap`, `View` and `NodeId`, and the `serialize_sorted_layout` helper for
  `View::layout`.
- `README.md`, so its examples no longer run as doc tests, and `CHANGELOG.md`.
- `examples/`, and the dev-dependencies they used (`eframe`, `env_logger`, `petgraph`).

## Changes made here

- `Cargo.toml` written here: package `aic-node-graph`, lib `aic_node_graph`, the workspace's
  version, `publish = false`, no features. Edition 2021 and the `egui` and `siphasher`
  dependencies as upstream has them. A dev-dependency on `egui` with `default_fonts`, which the
  tests need to lay out text; upstream got the fonts through its `eframe` dev-dependency.
- Crate doc in `lib.rs` of our own, in place of the README included as crate docs.
- Two `map_or(true, ...)` calls in `lib.rs` became `is_none_or(...)`, for clippy's
  `unnecessary_map_or`; rustfmt then joined the second `if` onto one line.
- `#[allow(clippy::type_complexity)]` on the `let (best, across, along)` statement in
  `lib.rs`, which names two `fn(egui::Pos2) -> f32` types.
- Otherwise rustfmt with this workspace's settings leaves the copied files as they were.

M11 step 2, the widget changed to suit the canvas (tests in `src/interaction_tests.rs` unless
said otherwise):

- A socket can have a handle, a rect inside the node's content (`SocketLayout::input_handle`,
  `output_handle`). A primary press on a handle starts an edge from its socket
  (`EdgeEvent::Started`), does not move the node and does not change the selection. A press
  elsewhere on the frame still drags the node. The socket stays on the frame's edge.
- While an edge is in progress, the pointer over a handle of the opposite kind on another node
  makes its socket the end, and a release there ends the edge (`EdgeEvent::Ended`). A release
  over a handle of the same kind, a frame away from its handles, or the background cancels it.
- A socket with a handle is pressed and ended on only through its handle, not from the
  background just outside the frame. A socket without one works as before.
- A release on a socket of the same kind as the edge's start now only cancels the edge. Before,
  the start node reported `Cancelled` and the other node `Ended`, with the same kind.
- `NodeSockets::input_handle` and `output_handle` give the handles, in graph space. A culled
  node's handles move with it, as its sockets do.
- `Node::paint_sockets(false)` paints no half-disc for the node's sockets; they still work.
- `NodeCtx::highlighted_sockets` gives the content a `SocketHighlight`: the socket an edge in
  progress started from and the one it would end on.
- A primary drag on the background pans the view, and selects nothing. A middle drag still pans.
- A click on a node (a press and release that moves the pointer no more than egui's
  `max_click_dist` and leaves the node in place) makes it the only selected node; Shift+click
  adds or removes it. Ctrl no longer toggles. A click on the background clears the selection
  unless Shift or Cmd/Ctrl is held. A press no longer changes the selection; its release does.
- A selection box is drawn by a secondary drag on the background, or a primary drag there with
  Shift; it shows once the drag passes the click distance. On release the nodes it touches
  become the selection; with Shift held at release they are added, with Cmd/Ctrl taken out. The
  box no longer selects or highlights edges, and Shift no longer keeps nodes out of it.
- `GraphResponse::background_secondary_click`: the graph position of a secondary click on the
  background, not reported at the end of a box drag.
- Dragging a selected node moves the selection; dragging an unselected node moves it alone and
  leaves the selection as it was. Before, a press on an unselected node selected it alone.
- The graph shows its content in its own copy of egui's `Scene` (`Scene` in `lib.rs`) that pans
  on a middle drag and never on the wheel, a trackpad scroll or a pinch, and never zooms. Those
  events are left to the caller, which sets `View::scene_rect`. The background area grows to
  include the visible area rather than being set to it, as egui keeps that setter private.
- Auto-pan works for a secondary-button box too, and never during a pan of the view.
- The widget no longer handles Delete and Backspace. `NodeResponse::removed`,
  `EdgeResponse::deleted` and `EdgeResponse::changed` are gone.
- `Edge::new(a, b)` takes no selection flag; `Edge::selected(bool)` sets the stroke only. The
  edge has no select or deselect logic of its own; a click is on its response.
- `Edge::distance_per_point` and `Edge::DEFAULT_DISTANCE_PER_POINT` are in screen pixels: the
  edge divides it by the view's scale, for painting and hit-testing alike, as does
  `EdgeInProgress::show_styled` (test in `edge.rs`).
- Fix: a node's sublayers, and the selection box's, follow the scene's transform when it is the
  identity too. egui stores no transform for a layer at the identity, so they kept the transform
  of a zoomed view, and frames and handles were hit-tested where they were drawn before.
- `#[allow(clippy::large_enum_variant)]` on `Content` in `node.rs`, which the larger
  `SocketLayout` made large; boxing it would allocate for every node in view.
- `NodesCtx::set_position` places a node while the nodes show, in the view the frame shows
  after any pan, and `NodesCtx::position` reads a node's place. The app pins a workspace's ports
  to the edges of its canvas with them.
- `Graph::selection_area` sets the selection box's fill and stroke. The stroke's width, given or
  not, is now in screen pixels: the box is outlined as wide at any zoom.
- Tests of what a canvas that draws its own cards leans on (`interaction_tests::cards`): a
  widget that senses clicks only, on a socket's handle, gets its clicks and leaves the drag to
  the handle.
- A tap, a press and its release within one frame (a touchpad tap, a quick click), does what a
  slower click does: on a node it selects, on the background it clears the selection, and with
  the secondary button it is reported. Before, a press was only seen while its button was still
  down at the end of the frame.
- Tests changed to the new conventions: `selection_area::paints_above_nodes` and
  `paints_below_windows` draw the box with a secondary drag; `cull::out_of_view_node_is_removed`
  became `out_of_view_node_keeps_its_selection`; `auto_pan::immutable_graph_pans_only_for_selection`
  builds the new `DragNodes` and checks that a pan never auto-pans; `edge::tests::show_edge` uses
  the new `Edge::new`.
