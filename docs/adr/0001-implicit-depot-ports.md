# Unconnected ports are implicit depot I/O

An unconnected input port means the item comes from somewhere outside the graph, and an unconnected output port means the item leaves the graph. The solver treats both as depot flow and reports them in totals; it raises no error and no warning. This matches Satisfactory Modeler, where a single node gives a useful answer without first wiring a supply for every input.

## Considered Options

- **Explicit depot node** that every port must be linked to, and an unconnected port is an error. Rejected: it makes quick what-if planning tedious.
- **Per-port "must be linked" toggle.** Rejected for now as extra UI for little gain.

## Consequences

- A forgotten link fails silently: it becomes depot I/O instead of an error. The totals panel must show depot inputs and outputs prominently so the player can spot this.
- Saved graphs rely on this meaning. Changing it later would change the result of every existing graph.
