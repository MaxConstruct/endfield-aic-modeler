# Logical flow planning only, no spatial layout

The app plans throughput: facility counts, flow per link, depot I/O and power. It does not model grid placement, facility footprints, belt or pipe routing, bridges, power poles or blueprints. Spatial planning is the part of the AIC the player is meant to do in-game, and the tool should not take that away.

The exception is logistics elements that change the flow math: splitters, mergers and storage. These exist as nodes, for both belts and pipes. Belt and pipe count appears only as a `⌈flow / capacity⌉` hint on a link.

## Consequences

- The `size` field in game data is ignored.
- Requests for layout, footprint or blueprint features are out of scope. Point to this ADR.
