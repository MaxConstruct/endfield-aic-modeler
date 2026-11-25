# AIC Modeler

A graph-first planner for the AIC (Automated Industry Complex) in *Arknights: Endfield*. The player places nodes on a canvas, wires them together, and the app balances the flows. It plans logical throughput only. Physical layout stays in-game.

## Language

### Game data

**Facility**:
A building type that performs recipes, identified by its game ID (e.g. `furnance_1`, `mix_pool_1`).
_Avoid_: Machine, producer, building

**Recipe**:
A transformation of input items into output items that one facility performs over a fixed time.
_Avoid_: Formula, craft

**Activation input**:
An input a facility needs at a fixed rate for each facility that is running, whatever its throughput (e.g. 6 Xiragen per minute for each Solid-Gas Transmuting Unit, even one at half load). A recipe's inputs include it (the data file lists it apart, on the facility, and loading adds it); only the rest of that item's rate scales with the facility count.
_Avoid_: Fuel, upkeep, idle cost, base input

**Gas environment**:
A gas that a Gas Dispersing Unit (`vaporizer_1`) spreads around itself, which some recipes need in place to run (e.g. Inergen for `xiranite_oven_xiranite_powder_2`). It is not a flow: the recipe doesn't consume it, and one unit serves every facility in its range. Named by the gas's item ID; shown under the game's name for it (Inergen's is the Stable Environment).
_Avoid_: Atmosphere, gas mode, aura, field

**Array**:
An area that a Forge of Luminance projects, in which items on belts turn into another form (e.g. Cuprium into L-Cuprium in an L-Array). In the data it is a facility, one for each kind and tier (`magic_ming_1` is the L-Array T1), whose rate is that of a fully belted area.
_Avoid_: Formation, field, aura, zone

**Projection**:
One T1 array as a forge projects it. Stacking projections makes an array of a higher tier, or of another kind (an L and a T make an E-Array). A graph counts what its arrays need against what its forges project.
_Avoid_: Beam, stake, pole

**Whole units**:
How a facility runs whose inputs are all activation inputs: the Gas Dispersing Unit and the Forges of Luminance. Its facility count is a whole number.
_Avoid_: Integer facility, on/off facility

**Byproduct**:
Any output of a recipe other than the one the player is producing it for (e.g. `liquid_sewage` from `copper_nugget`).
_Avoid_: Secondary output, waste

**Region**:
A separate in-game area with its own recipes and facilities: Valley IV (`tundra`) or Wuling (`jinlong`).
_Avoid_: Location, zone, map

**Data version**:
A named snapshot of game data, matching a game version (e.g. `v1.5.3`).
_Avoid_: Dataset, patch

**Alias**:
An ID an older data version used for a recipe or item, which a newer one maps to its own ID for the same thing (e.g. recipe `copper_nugget` → `furnance_copper_nugget_1`), so graphs saved on the older version still open.
_Avoid_: Redirect, legacy ID, rename, migration

**Raw resource**:
An item that a mining recipe extracts in a region (e.g. Cuprium Ore). Each region can supply only so much of each per minute.
_Avoid_: Ore, raw material, natural resource

### Planning

**Graph**:
One player-made plan. It belongs to exactly one region.
_Avoid_: Factory, blueprint, layout, project

**Node**:
One placed element on a graph: a recipe, source, sink, splitter, merger, storage or control port node, or a workspace's port.
_Avoid_: Block, building

**Source node**:
A node that supplies one item from outside the graph, up to a required limit (e.g. "only 60/min of ore").
_Avoid_: Resource, input node, supply

**Sink node**:
A node that consumes one item as a goal of the graph, up to an optional limit.
_Avoid_: Output node, target, consumer

**Outpost order**:
A sink node for a settlement trade, where items go in and coupons come out.
_Avoid_: Settlement recipe, trade recipe

**Splitter**:
A node that takes one item from one link and passes it on through several links.
_Avoid_: Splurger, divider

**Merger**:
A node that joins several links of one item into one.
_Avoid_: Splurger, converger, combiner

**Control port**:
A node that caps the rate of one item passing through it on a belt or pipe. Any number can sit in a row.
_Avoid_: Valve, throttle, link limit

**Storage**:
A node that buffers one item, solid or fluid, so that its inflow and outflow need not match. Demand downstream of a storage is not passed on upstream.
_Avoid_: Container, stash, tank, buffer

**Balance**:
A storage's inflow minus its outflow. Positive means it fills up, negative means it drains.
_Avoid_: Net, delta

**Pass-through**:
The part of a storage's flow that goes straight through: the smaller of inflow and outflow.
_Avoid_: Throughput

**Origin**:
A node that nothing in the graph feeds: a source, a recipe node with no linked input, the output side of a storage, or a logistics node drawing from the depot. Throughput is counted where flow leaves origins and where it reaches sinks.
_Avoid_: Root, start, producer

**Section**:
A part of a graph connected without crossing a storage. Each section needs its own driver.
_Avoid_: Subgraph, island, component

**Driver**:
A limit somewhere in a section that bounds it. A section with no driver is undetermined.
_Avoid_: Constraint, anchor

**Undetermined**:
A value the solver cannot settle because nothing limits it. It is shown blank, never as 0 or ∞.
_Avoid_: Unknown, unsolved, empty

**Draft**:
A graph that hasn't been saved to a file of the player's choosing yet. The app keeps it in its own folder and saves it as it changes.
_Avoid_: Temp file, scratch, unsaved graph

**Workspace**:
A named place in a graph that holds nodes and other workspaces. From outside it is one card; opened, it shows only what is in it. What crosses its edge goes through its ports.
_Avoid_: Group, outpost, blueprint, folder, module

**Port** (of a workspace):
An input or output the player adds to a workspace, carrying one item across its edge. It has an outside end and an inside end; with either end unlinked, nothing flows through it.
_Avoid_: Pin, connector, gate

**Level**:
What a canvas shows: the top of the graph, or the inside of one workspace.
_Avoid_: Layer, depth, scope

**View**:
One look at the open graph, in a tab: a level and a camera. Every view shows the same graph, numbers and selection; views are not saved in the graph's file.
_Avoid_: Window, pane, editor, tab

**Limit**:
An upper bound the player sets on a facility count, sink, source or control port. The solver runs everything as high as limits and supply allow. A recipe node's limit can also be typed as a rate through one of its ports (e.g. 225/min of plates); it is the same limit, kept as a facility count.
_Avoid_: Target, fixed value, constraint

**Throttled**:
Running below its limit because of a shortage upstream or a backup downstream.
_Avoid_: Starved, blocked, bottlenecked

**Fan-out**:
One output port, or one splitter, sending an item through several links. It splits evenly whenever that costs no throughput; otherwise the split follows what each branch can take.
_Avoid_: Branch, split

**Fan-in**:
Several links feeding one input port or merger. It draws evenly from the feeders whenever that costs no throughput.
_Avoid_: Merge, join

**Ratio lock**:
A fixed split the player sets on a fan-out or fan-in (e.g. 70/30) that replaces the even split. Each locked link has a share; links without one share the rest evenly.
_Avoid_: Weight, priority

**Facility count**:
How many facilities a recipe node runs, possibly fractional (1.75 means two facilities, one at 75% utilization).
_Avoid_: Machine count, multiplier

**Port**:
One item slot on a node, input or output. A node has one port per item it consumes or produces.
_Avoid_: Pin, socket, slot

**Link**:
A wire carrying one item from an output port to an input port.
_Avoid_: Edge, connection, belt, wire

**Depot**:
The Protocol Core's storage. Every unconnected port implicitly draws from or delivers to the depot.
_Avoid_: Core, storage, warehouse, bank

**Surplus**:
Output that a linked port's links can't carry away. It backs up and throttles the facility, as it stalls the facility in-game.
_Avoid_: Overflow, excess, leftover
