# Throughput is measured at origins and sinks

Pass 1 of the solver maximizes throughput, and pass 2 then splits fan-outs and fan-ins evenly among the answers that keep that throughput. So the pass-1 objective decides which splits count as "costing throughput". We measure throughput only where flow enters the graph or reaches a goal: the output of every **origin** (a node nothing in the graph feeds: a source, a recipe node with no linked input, a storage's output side, a splitter, merger or control port pulling from the depot) plus the consumption of every sink. A recipe node fed by links adds nothing on its own, so how a fan-out divides its item among consumers never changes the objective, and pass 2 is free to split it evenly, as the in-game splitter does.

## Considered Options

- **Maximize the sum of facility counts.** Rejected: in golden test 4 it sends everything to the consumer that needs the least input per facility, starving the other.
- **Lexicographic max-min over node utilization (count / limit).** Rejected: it treats every limit as a target to fill evenly. In test 4 it equalizes the two consumers' counts (20/10 split) instead of their input links (15/15).
- **Weight each node by the items it handles.** Rejected: the weights favor recipes with more items per craft, so a split depends on recipe ratios downstream rather than on what each branch can take.

## Consequences

- An origin with no outputs (a generator) is measured by its inputs instead.
- A sink's limit is a goal: when a fan-out feeds a sink on one branch and the depot on the other, the sink's branch is filled first.
- When competing branches start at different origins, the one whose origin puts out more items per unit of shared flow is favored. This is rare and accepted.
- A section with neither origins nor sinks is a closed loop; it maximizes the sum of node activity instead.
- Evenness (pass 2) minimizes, for every fan group, its largest link minus its mean link. This fills links like water: a branch that can't take an equal share takes what it can, and the rest split evenly.
- Changing this objective changes the result of existing saved graphs.
