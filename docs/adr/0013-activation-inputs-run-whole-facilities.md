# Activation inputs run whole facilities, settled by a capped search in pass 1

Two facilities need an input at a fixed rate just to run, whatever their throughput: the Solid-Gas Transmuting Unit (`phase_trans_2`) 6 Xiragen a minute, the Fluid-Gas Transmuting Unit (`phase_trans_1`) 6 Liquid Xiranite. A unit at half load still needs all 6, and 1.5 units are two running, so 12: a node at facility count `x` needs `6 × ceil(x)` on top of what scales with `x`. This is an **activation input**. The data folds it into every recipe of those facilities as an input that scales, which under-counts at partial load, so the overlay (`data/v1.5.3_overlay.json`, `facility_activation`) says which input it is. A recipe's inputs stay the totals for one facility flat out.

- **On the depot**, an activation port needs no solver support: the depot never runs out and the port is in no objective. The program has only the part that scales; `activation × ceil(count)` is added to the port's flow after the solve.
- **Fed by links**, it needs a whole number. In pass 1 only (the throughput solve, or the settle-by-limits solves of ADR 0006), the node gets `running`, tied by `running ≥ count` and `running ≤ count + 1 − 1e-5`, and the port carries `scaled × count + activation × running`. microlp solves that by branch and bound, stopped after **200 nodes** (`NODE_LIMIT`), keeping the best answer found by then.
- **Every later program** (pass 2, the throttle analysis) takes `running` as given, keeps each count within `(running − 1, running]`, and is linear.

## Considered Options

All timings are one section of 20 units, release build; the pure linear program of the same graph took 0.6–6 ms.

- **Leave it a scaling input.** Rejected: half a unit shows 3/min where the game needs 6, and 5/min of supply seems to run 0.83 units when it runs none.
- **An unbounded branch-and-bound search, in every pass.** Rejected: 1 s for 20 units sharing a short supply, over 60 s without limits; pass 2 and the throttle analysis each took as long as pass 1.
- **Stay linear and round the running facilities outside the program** (solve with the input scaling, round up, switch off what falls short, repeat). Rejected: on a chain of ten pairs it delivered 30/min where 48 is possible, lost a fan-out's even split (15 of 20 units at 0.667 instead of 20 at 0.5), and took up to 9 times the linear program, one round per node.
- **Stop the search at a relative gap.** Rejected: the program's relaxation sits about 15% above the best whole-number answer when units share a supply, so gaps from 0.5% to 5% did not help: 275 ms at every gap, and over 120 s below 5% for units without limits. At 5% a limited sink got 96 of its 100.

## Consequences

- A section without a link-fed activation input is the same linear program as before, solved exactly. Sections with only depot-fed activation inputs are linear too.
- In a section with one, beyond 200 nodes the answer may fall short of the most the section could do. Measured: a chain of 20 is proved exact in 51 nodes (3.5 ms); 20 units on a short supply stop at the limit with the right answer (16 running, 8.5 ms; 4.8 ms without limits; 9.4 ms when the limits settle the section). 40 units: 33 running, 18 ms. A node count gives the same answer on every machine and every solve.
- If the search finds no answer within its nodes (seen at 50 and 100 nodes, not at 200), every link-fed activation node is switched off and pass 1 is solved again as a linear program. The section shows as solved.
- Pass 2 does not move `running`. When units without limits share a short supply, the facilities may land unevenly across the nodes: one node running 16 and the rest off.
- A node fed by links only into a port of which nothing scales is still an origin (ADR 0004), measured by what it puts out. Otherwise nothing would tie its count within one running facility.
- microlp's whole numbers need bounds that fit an `i32`: a node without a limit runs at most 1,000,000 facilities, and a solve that reaches that counts as unbounded, so ADR 0006 still applies.
- The throttle analysis builds the node it asks about with its activation input scaling, so a unit switched off for want of it names that port. The source behind it sends nothing and so isn't at its limit; no limiting node is named.
- A limit typed as a rate through an activation port is the largest count whose flow fits it (6/min is one unit, 3/min none). `RoundUp` and the totals count the same running facilities. Power still scales with the count.
- `aic-core` names `microlp` as a direct dependency, the version good_lp already uses, for its node limit.
