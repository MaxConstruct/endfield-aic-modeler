# Reference: SFTools manual "Add node" flow

How Satisfactory Tools (SFTools, Angular/TypeScript) handles two manual graph edits. Use it to cross-check this project's equivalent flows.

1. **Plain add.** Open the *Add node* dialog on a blank canvas and add a Recipe node (*AI Limiter*). The result is `1× Assembler, 1 @ 100%`.
2. **Add producer.** Drag from the AI Limiter's *Copper Sheet* input port onto blank canvas. The *Add producer of Copper Sheet* dialog opens. Picking the *Copper Sheet* recipe adds `Copper Sheet, 3× Constructor (2 @ 100%, 1 @ 50%)`, wired to the AI Limiter with a `Copper Sheet 25/min` edge.

Source: SFTools commit `eee1486`. Paths are relative to `src/app/`.

> **Terminology map.** SFTools *machine* = AIC *facility*. SFTools *clock speed* (1–250 %) has no AIC equivalent; treat it as 100 % fixed if AIC has no overclocking. *Sloops* (somersloops) are an output-boost mechanic; 0 throughout this document.

---

## Key facts up front

- **No solver runs.** Manual adds never call the LP solver (`ProductionSolverService` / HiGHS). The node is built locally from closed-form arithmetic, and the graph is re-rendered and saved.
- **Manual nodes are locked** (`node.locked = true`). A later solver run builds around them and never replaces them.
- **The recipe picker is not filtered by enabled recipes.** It lists every recipe that has a machine (`Data.getRecipesForMachines()`). It ignores the plan's `enabledRecipes` and alternate-recipe settings, which only affect solver runs.
- **The machine is always `recipe.producedIn[0]`.** In this data set each recipe runs in exactly one machine.
- **Rate in = rate out.** The new producer's target is sized to exactly the consumer's *unmet* demand for that item. The connecting edge carries `min(spare output of producer, remaining demand of consumer)`.

---

## Flow 1 — Add node dialog (no connection context)

### Call chain

```
User: press A  |  right-click blank → "Add node…"  |  double-click blank
  ├─ A key:       HotkeyService.handle → HotkeyService.run('graph.addNode')
  │                 → PlannerComponent.hotkeyItems()
  │                 → new BlankContextMenu(actions, plannerGraph.canvasCenter(), …).getItems()[0].action()
  ├─ right-click: PlannerGraphService.onBlankContextMenu → BlankContextMenu item "Add node…"
  └─ dbl-click:   PlannerGraphService.onBlankDoubleClick(local)
        ▼
PlannerActionsService.requestNodeAdd(position)            // Subject<GraphPoint>
        ▼
PlannerComponent: actions.nodeAddRequests.subscribe → addNodePositionSignal.set(position)
        ▼
addNodeOpen() = true; addNodeFilter() = null (no pending connect); addNodeSuggestedAmount() = null
        ▼
<add-node-dialog [filter]=null [suggestedAmount]=null>    // Components/Planner/AddNode/AddNodeDialogComponent
  ngOnInit: filter null → nothing preselected, nodeType = 'recipe'
  types:    all 7 (Recipe, Mine, Product, Byproduct, Input, Generator, Sink)
  recipeOptions: Data.getRecipesForMachines(), sorted by label (no filter)
        ▼  user picks "AI Limiter", clicks Add node
AddNodeDialogComponent.submit()
  recipe  = data.getRecipeByClassName(recipeClassName)
  machine = recipe.producedIn[0]                          // Assembler
  target  = recipeTargetFor(recipe, machine)              // = 1 (no filter / no suggested amount)
  mode    = GroupingModeResolver.resolve(planManager.activeSettings())
  clock   = ClockSpeedResolver.forRecipe(recipe, machine)
  groups  = MachineGroupNormalizer.generateForTarget(target, clock, 0, mode)
  node    = new RecipeNode(uuid, target, groups, machine, recipe)   // setupIO() runs in ctor
  node.groupingMode = mode; node.locked = true
  emit add(node)
        ▼
PlannerComponent.onAddNode(node)
  (no active plan → startPlanWithNode: createPlan, placeAddedNode, setGraph, setActivePlan; done)
  graph = PlanSerializer.reviveGraph(plan.graph)          // or {nodes:[],edges:[]}
  history.push(snapshotOf(plan))                          // undo
  placeAddedNode(node, position, origin=null, …)          // centred on the click point
  updated = {nodes:[...graph.nodes, node], edges:[...graph.edges]}
  PlannerGraphService.restore(container, updated, fit=false)
      → GraphReconciler.computeWarnings(updated)          // red ⚠ badges
      → addNode per node (labels, ports, groups)
  PlannerGraphService.selectNodeById(node.id)
  PlanManager.setGraph(plan.id, updated, graphDirty=true) // persist + sync
```

### Result for AI Limiter

These rates come from SFTools' game data, which is fetched from the API at runtime and not stored in the repo. They match the screenshots.

| | Value |
|---|---|
| Recipe | 5 Copper Sheet + 20 Quickwire → 1 AI Limiter, 12 s, Assembler |
| `referenceCycles` | `60 / 12 × manufacturingSpeed(1)` = 5 cycles/min |
| `target` | 1 (machines at 100 %) |
| `groups` | `[{machines: 1, clockSpeed: 100, sloops: 0}]` |
| Inputs (`setupIO`) | Copper Sheet 5 × 5 = **25/min**, Quickwire 20 × 5 = **100/min** |
| Output | AI Limiter **5/min** |
| Warnings | Inputs unmet (Copper Sheet 25, Quickwire 100); output 5/min not consumed |

---

## Flow 2 — "Add producer of Copper Sheet" (connect-to-blank)

### Call chain

```
User: drag from AI Limiter's Copper Sheet INPUT port, release on empty canvas
        ▼
PlannerGraphService.beginConnectGesture(cell, magnet)
  connectGesture = {nodeId: <AI Limiter id>, side: 'in', itemClassName: 'Desc_CopperSheet_C'}
  watchConnectDrop → mouseup/touchend → settleConnectDrop(x6Graph, client)
        ▼
settleConnectDrop: temp edge has no target cell and no node under the drop point
  → PlannerActionsService.requestConnectToBlank({nodeId, itemClassName, side: 'input', position: drop})
        ▼
PlannerComponent.beginConnectToBlank(request)
  graph = PlanSerializer.reviveGraph(plan.graph)
  free  = side === 'output'
          ? GraphReconciler.spareOutput(graph, nodeId, item)        // produced − Σ outgoing edges
          : GraphReconciler.remainingDemand(graph, nodeId, item)    // required − Σ incoming edges
        = 25 − 0 = 25
  pendingConnectSignal.set(request)
  addNodeSuggestedAmountSignal.set(free > 0 ? free : null)          // 25
  addNodePositionSignal.set(request.position)
        ▼
addNodeFilter() = {itemClassName: Copper Sheet, role: side==='output' ? 'consumer' : 'producer'}  // 'producer'
        ▼
<add-node-dialog [filter]={Copper Sheet, producer} [suggestedAmount]=25>
  title: "Add producer of Copper Sheet"
  types (role = producer):
    recipe    if any recipe has Copper Sheet among its PRODUCTS
    generator if any generator fuel has Copper Sheet as byproduct   → none
    input     always for producers
    mine      only if the item is a raw resource                   → no
    ⇒ [Recipe, Input]                                               (matches screenshot)
  ngOnInit: preferMine = false → setType(types[0] = 'recipe')
  applySingleOptionDefaults: auto-select only when exactly 1 recipe option
        ▼  user picks "Copper Sheet" recipe, clicks Add node
submit()
  machine = recipe.producedIn[0]                                    // Constructor
  target  = recipeTargetFor(recipe, machine):
              io         = recipe.products.find(item == Copper Sheet)   // ingredients if role = consumer
              perMachine = io.amount × Formulas.referenceCycles(recipe, machine)
              target     = suggestedAmount / perMachine                 // 25 / 10 = 2.5
  groups  = MachineGroupNormalizer.generateForTarget(2.5, clock=100, sloops=0, 'underclock-last')
  node    = new RecipeNode(uuid, 2.5, groups, Constructor, CopperSheet); locked = true
        ▼
PlannerComponent.onAddNode(node)
  pending = pendingConnectSignal(); closeAddNode() clears position/pending/suggested
  graph   = reviveGraph(plan.graph); history.push(...)
  origin  = AI Limiter node
  placeAddedNode(node, drop, origin, vertical?):
     horizontal: node.y = drop.y − h/2
                 node.x = drop.x ≥ origin.x + origin.w/2 ? drop.x : drop.x − node.w
                 (the border facing the origin lands on the drop point)
  updated = graph + node
  edge    = connectingEdgeFor(updated, pending, node):
     side 'input' → new node must OUTPUT the item (else no edge)
     source = new node, target = AI Limiter
     amount = min(spareOutput(updated, source, item), remainingDemand(updated, target, item))
            = min(25 − 0, 25 − 0) = 25
  updated.edges.push({sourceId, targetId, itemClassName, amount: 25})
  restore → computeWarnings → render; selectNodeById; PlanManager.setGraph(…, dirty=true)
```

For the mirror case (dragging from an **output** port), the role becomes `consumer`. Recipes are then filtered on **ingredients**, the types offered become recipe / product / byproduct / sink (sink only if the item is sinkable), and the edge runs from the existing node to the new node.

### Result for Copper Sheet

| | Value |
|---|---|
| Recipe | 2 Copper Ingot → 1 Copper Sheet, 6 s, Constructor |
| `referenceCycles` | `60 / 6 × 1` = 10 cycles/min → 10 sheets/min per machine at 100 % |
| `target` | 25 / 10 = **2.5** machines at 100 % |
| `groups` | `[{machines: 2, clockSpeed: 100, sloops: 0}, {machines: 1, clockSpeed: 50, sloops: 0}]` |
| `amount` (machine count) | Σ machines = 3 → label `3× Constructor` |
| Capacity | 2 × 1.0 + 1 × 0.5 = 2.5 ≥ target → no capacity warning |
| Inputs | Copper Ingot 2 × (10 × 2.5) = **50/min** (unmet → ⚠ on the input port) |
| Output | Copper Sheet 1 × 25 = **25/min** |
| Edge | Copper Sheet **25/min**, Copper Sheet → AI Limiter |
| AI Limiter after | Copper Sheet satisfied; Quickwire still unmet; output still unconsumed |

---

## Algorithms

### 1. Target sizing (`AddNodeDialogComponent.recipeTargetFor`)

```
if no filter or suggested ≤ 0: target = 1
perMachine = amountOfFilterItemPerCycle × referenceCycles(recipe, machine)
target     = suggested / perMachine       (fallback 1 if perMachine ≤ 0)
```

`target` is measured in **machines at 100 % clock**. It is the node's core quantity: every rate is derived from it.

### 2. Rates (`Formulas`, `RecipeNode.setupIO`)

```
referenceCycles = (60 / recipe.time) × machine.manufacturingSpeed      // cycles/min per machine @100%
targetCycles    = referenceCycles × target
boost           = outputBoostRatio(machine, groups)                    // 1 with no sloops
input[i]        = ingredient[i].amount × targetCycles
output[j]       = product[j].amount × targetCycles × boost
capacity        = Σ group.machines × group.clockSpeed / 100
```

Rates are per minute. An input is not boosted; only outputs get the sloop boost.

### 3. Clock resolution (`ClockSpeedResolver.forRecipeIn`)

The first match wins, and a duplicate settings row resolves to its **last** entry:

1. the plan's `recipeClockSpeeds[recipe]`
2. the plan's `machineClockSpeeds[machine]`
3. the plan's `defaultClockSpeed`
4. 100 when the plan has no settings

The result goes through `Formulas.clampClock`: clamped to 1–250 and rounded to 4 decimals.

### 4. Grouping mode (`GroupingModeResolver.resolve`)

`plan.settings.defaultGroupingMode ?? globalPlanDefaults.groupingMode`. The shipped default is **`'underclock-last'`** (`Model/Settings/SettingsDefaults.ts`).

### 5. Machine groups (`MachineGroupNormalizer`)

```
generateForTarget(target, clockSpeed, sloops, mode):
  clock  = roundClock(clockSpeed)                 // ceil to 4 dp, clamp 1..250
  amount = target × 100 / clock                   // machines needed at that clock
  return generate(amount, clock, sloops, mode)

roundClock(v)     = min(250, max(1, ceil(v × 10000 − 1e-7) / 10000))   // always rounds UP
wholeMachines(a)  = max(1, ceil(a − 1e-9))

generate by mode:
  'no-clocking'    → [{machines: wholeMachines(amount), clock: roundClock(clock)}]
  'clock-equally'  → m = wholeMachines(amount); [{machines: m, clock: roundClock(amount × clock / m)}]
  'underclock-last'→ fromFractionalAmount(amount, clock, sloops):
       whole     = floor(amount + 1e-9)
       remainder = amount − whole
       if remainder ≤ 1e-9:            [{max(whole,1) @ roundClock(clock)}]
       remClock = remainder × clock
       if remClock ≥ 1:                [{whole @ clock}] (if whole > 0) + [{1 @ roundClock(remClock)}]
       elif whole ≥ 1 and amount×clock/whole ≤ 250:
                                       [{whole @ roundClock(amount × clock / whole)}]  // fold tiny remainder in
       else:                           [{whole @ clock}] (if whole > 0) + [{1 @ 1%}]
```

Design rule: **clocks always round up**, so built capacity is never below target. The capacity-shortage check (`RecipeNode.isCapacityShort`) is therefore near-exact: `target − capacity > max(1e-9, 1e-10 × capacity)`.

### 6. Edge amount and warnings (`GraphReconciler`)

```
requiredInputs(node)[item]  = Σ node.inputs[item].maxAmount
producedOutputs(node)[item] = Σ node.outputs[item].maxAmount
spareOutput(g, n, item)     = max(0, produced − Σ edges(source = n, item).amount)
remainingDemand(g, n, item) = max(0, required − Σ edges(target = n, item).amount)
```

`computeWarnings` checks the following. Two numbers "differ" when `|a − b| > max(0.001, 1e-4 × max(|a|, |b|))`.

- **Inputs:** only for nodes with fixed demand (Recipe, Product, Subplan, Sink). Raises a warning per item when `required` differs from the supplied total.
- **Outputs:** only for nodes with fixed supply (Recipe, Mine, Subplan). Raises a warning per item when `produced` differs from the consumed total.
- **Capacity:** for Recipe nodes, `target > capacity`.

Input, Byproduct and Generator nodes are elastic, so they never get IO warnings.

### 7. Persistence

`PlanManager.setGraph(planId, graph, graphDirty=true)` replaces the plan's graph and sets `metadata.graphDirty = true`. The plan store then syncs it: local storage, an offline mirror, and the API when signed in. `RecipeNode.toJSON` saves `{type, id, recipeClassName, machineClassName, target, groups, groupingMode, x, y, locked?, done?}`. On load, `PlanSerializer.deserializeNode` looks the recipe and machine up again by class name.

---

## Test data

SFTools has **no automated tests** (no `*.spec.ts` / `*.test.ts` in the repo). The vectors below are derived by hand from the code paths above and can serve as golden cases.

### Flow-level vectors

| # | Scenario | Input | Expected |
|---|---|---|---|
| F1 | Plain add, AI Limiter | filter null, suggested null, clock 100, mode underclock-last | target 1; groups `[1@100]`; in Copper Sheet 25, Quickwire 100; out AI Limiter 5; locked |
| F2 | Producer of Copper Sheet for F1 | remainingDemand = 25; recipe Copper Sheet (10/min/machine) | target 2.5; groups `[2@100, 1@50]`; label `3× Constructor`; in Copper Ingot 50; out 25; edge 25 Copper Sheet → AI Limiter |
| F3 | Same as F2, demand already partly supplied by a 10/min edge | remainingDemand = 15 | target 1.5; groups `[1@100, 1@50]`; edge 15 |
| F4 | Same as F2, demand fully supplied | remainingDemand = 0 → suggested null | target 1; groups `[1@100]`; edge amount `min(10, 0)` = 0. `connectingEdgeFor` still pushes it; `reconcile` only drops edges ≤ 1e-6 on later edits |
| F5 | Producer dialog for a raw resource (e.g. Copper Ore) | role producer, item is a resource | types include Mine and it is preselected (`preferMine`); a Mine / Input node gets `amount = suggested` |
| F6 | Picked node does not produce the item (impossible via the filtered picker) | — | `connectingEdgeFor` returns null → node added with no edge |

### `MachineGroupNormalizer.generateForTarget` vectors (sloops 0)

| target | clock | mode | groups |
|---|---|---|---|
| 1 | 100 | underclock-last | `[1@100]` |
| 2.5 | 100 | underclock-last | `[2@100, 1@50]` |
| 2.5 | 100 | clock-equally | `[3@83.3334]` (83.3333… rounded **up**) |
| 2.5 | 100 | no-clocking | `[3@100]` |
| 0.25 | 100 | underclock-last | `[1@25]` |
| 2.005 | 100 | underclock-last | remainder 0.005 → remClock 0.5 < 1 → fold: `[2@100.25]` |
| 0.005 | 100 | underclock-last | whole 0, remClock 0.5 < 1 → `[1@1]` |
| 3 | 100 | underclock-last | `[3@100]` |
| 2.5 | 250 | underclock-last | amount = 1.0 → `[1@250]` |
| 1.5 | 50 | underclock-last | amount = 3.0 → `[3@50]` |

### `Formulas` vectors

| Call | Result |
|---|---|
| `referenceCycles(time 6 s, speed 1)` | 10 |
| `referenceCycles(time 12 s, speed 1)` | 5 |
| `groupCapacity([2@100, 1@50])` | 2.5 |
| `clampClock(0.2)` / `clampClock(300)` / `clampClock(83.33336)` | 1 / 250 / 83.3334 (`Math.round`, unlike `roundClock`'s ceil) |

---

## Files involved

| Concern | File |
|---|---|
| Dialog UI, target sizing, node construction | `Components/Planner/AddNode/AddNodeDialogComponent.ts` (+ `.html`, `AddNodeFilter.ts`) |
| Dialog open/close, placement, connecting edge | `Components/Planner/PlannerComponent.ts` (`beginConnectToBlank`, `onAddNode`, `placeAddedNode`, `connectingEdgeFor`, `startPlanWithNode`) |
| Port drag gesture, drop detection, render | `Components/Planner/PlannerGraphService.ts` (`beginConnectGesture`, `settleConnectDrop`, `restore`, `machinesLine`) |
| Event bus | `Components/Planner/PlannerActionsService.ts` (`requestNodeAdd`, `requestConnectToBlank`) |
| Menu and hotkey entry | `Components/Planner/ContextMenu/BlankContextMenu.ts`, `Model/Hotkeys/HotkeyService.ts`, `Model/Hotkeys/HotkeyCatalog.ts` (`graph.addNode` = A) |
| Rates | `Model/Planner/Formulas.ts`, `Model/Planner/Solver/Response/RecipeNode.ts` |
| Clock / grouping | `Model/Planner/ClockSpeedResolver.ts`, `Model/Planner/GroupingModeResolver.ts`, `Model/Planner/MachineGroupNormalizer.ts` |
| Demand, supply and warnings | `Model/Planner/Graph/GraphReconciler.ts` |
| Persistence | `Model/Planner/PlanManager.ts` (`setGraph`), `Model/Planner/PlanSerializer.ts` |
| Recipe catalogue | `Model/Data/Data.ts` (`getRecipesForMachines`, `getRecipeByClassName`) |

## Points worth comparing in the AIC modeler

- **Sizing rule.** Is a new producer sized to the consumer's *remaining* demand, or to its total demand? SFTools uses remaining demand.
- **Rounding direction.** SFTools always rounds the last machine's clock **up** and never leaves capacity short. Without clocking, the equivalent is `ceil(machines)` with partial utilization.
- **Edge amount.** SFTools uses `min(spare, remaining)` and does not rebalance the rest of the graph. No solver runs, and the graph is reconciled only on later edits (`GraphReconciler.reconcile`).
- **Locked manual nodes.** They are fixed constraints for later automatic solves.
- **Unfiltered recipe picker.** It lists every recipe, alternates included, whatever the plan's enabled-recipe settings say.
