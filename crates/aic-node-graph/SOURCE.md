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
  dependencies as upstream has them.
- Crate doc in `lib.rs` of our own, in place of the README included as crate docs.
- Two `map_or(true, ...)` calls in `lib.rs` became `is_none_or(...)`, for clippy's
  `unnecessary_map_or`; rustfmt then joined the second `if` onto one line.
- `#[allow(clippy::type_complexity)]` on the `let (best, across, along)` statement in
  `lib.rs`, which names two `fn(egui::Pos2) -> f32` types.
- Otherwise rustfmt with this workspace's settings leaves the copied files as they were.
