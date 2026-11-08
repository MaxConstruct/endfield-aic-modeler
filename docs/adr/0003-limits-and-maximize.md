# Player values are limits, and the solver maximizes

Every value the player types is an upper limit: a facility count, a sink rate, a source cap or a link maximum. The solver runs every node as high as its limits and supply allow. This is how Satisfactory Modeler works and how the game behaves: a facility runs as fast as its inputs and output space allow. It replaces the original plan of minimizing facility counts to meet exact targets.

## Considered Options

- **Exact targets, minimize facilities** (the original `PLAN.md` model, with `CountMode::Fixed` and `SinkMode::Fixed`). Rejected: a target that can't be met becomes an infeasible error rather than a visible shortfall, and the objective needs hand-tuned penalty weights.

## Consequences

- A node shows two numbers: actual and limit. Falling short of a limit is normal output, not an error, so infeasible graphs mostly disappear.
- A linked output port can't produce more than its links carry away. A backed-up byproduct throttles its facility, just like the in-game stall. Surplus is a reason a node is throttled, not a warning about leftover output.
- A section with no limit anywhere is unbounded, and its values show as undetermined (blank).
