# Limits settle a section that would otherwise grow without end

The solver runs everything as high as limits and supply allow (ADR 0003). But the depot never runs out, so a node without a limit that pulls from it, or one fed by such a node, can push a section's throughput up without end. Until now such a section was undetermined and showed nothing at all, even the nodes a limit clearly settles. Satisfactory Modeler settles the same graphs: a limit in the middle of a chain sizes everything linked to it.

So when pass 1 finds that throughput has no ceiling, the section is settled by its limits instead, in two steps: first every limited node runs as close to its limit as it can (the sum of activity over limit, maximized), then, keeping that, every node without a limit runs as low as its links allow (the sum of their activity, minimized). Pass 2 then evens out splits as before, keeping both. A section whose throughput does have a ceiling solves exactly as before.

## Considered Options

- **Keep the whole section undetermined.** Rejected: one free branch blanked the numbers a limit fully decides, which is what the player saw and reported.
- **Leave only the free nodes undetermined.** Rejected: the free branch shares a consumer with the limited one, so the consumer can grow too, and the numbers the player wanted stay blank.
- **Always size nodes without limits by what their neighbours need (pull everywhere).** Rejected for now: it would change every solved graph, including the golden tests' throttling and fan-out answers, for cases ADR 0003 already handles.

## Consequences

- A node without a limit that nothing asks for more runs at 0, rather than without end. In the reported graph, a second planter fed from the depot runs at 0 while the limited planter's carbon furnace runs at 3.
- An unlimited sink fed by an endless supply (for example a storage's output) takes nothing, rather than everything.
- `UndeterminedReason::Unbounded` is now only reachable if settling by limits itself fails, which it shouldn't: every limited node has a ceiling.
- Changing this changes the numbers of saved graphs that were blank before; graphs that solved before are unchanged.
