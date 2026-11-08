# English UI strings. Every user-facing string in aic-app comes from here.
# Numbers arrive preformatted, so messages never format them.

app-title = AIC Modeler

## Startup

load-failed-title = Could not load game data
load-failed-hint = Put a data folder next to the app, or start it with --data <path>.

## Top bar

region-label = Region
region-locked = Clear the graph to switch regions.
data-version = Data { $version }
settings-button = Raw resources…
status-slow-solve = Solving took { $ms } ms. The canvas may stutter.

## Node kinds

kind-recipe = Recipe
kind-source = Source
kind-sink = Sink
kind-splitter = Splitter
kind-merger = Merger
kind-storage = Storage
kind-control-port = Control port
kind-missing-recipe = Missing recipe
item-unset = ?
node-title = { $kind } · { $subject }

## Node cards

node-limit = limit { $limit }
node-no-limit = no limit
node-per-min = { $rate } /min
storage-balance = { $balance } /min
storage-pass-through = through { $rate } /min
node-remove = Remove node
throttle-marker-hint = This port holds the node below its limit.

## Links

link-label = { $flow } /min · { $belts } belts
link-label-fluid = { $flow } /min · { $belts } pipes

## Node picker

picker-search-hint = Search recipes
picker-match-name = Name
picker-match-inputs = Inputs
picker-match-outputs = Outputs
picker-source-of = Source of { $item }
picker-sink-of = Sink of { $item }
picker-no-recipes = No recipe matches.

## Recipe browser

browser-title = Recipes
browser-search-hint = Search by name
browser-all-categories = All categories
browser-add-hint = Click to add to the canvas.

## Inspector

inspector-title = Inspector
inspector-nothing = Select a node or link to see it here.
inspector-facility = Facility
inspector-count = Facility count
inspector-rate = Rate
inspector-limit = Limit
inspector-limit-hint = blank: no limit
inspector-limit-required = This node needs a limit.
inspector-limit-invalid = Enter a number of at least 0.
inspector-ports = Ports
inspector-port-in = in · { $item }
inspector-port-out = out · { $item }
inspector-depot = depot
inspector-throttled = Throttled
inspector-throttle-input = Short of { $item } on its input.
inspector-throttle-output = { $item } backs up on its output.
inspector-limited-by = Held back by { $node }, which is at its limit.
inspector-link = Link
inspector-flow = Flow
inspector-belts = Belts
inspector-pipes = Pipes
inspector-balance = Balance
inspector-pass-through = Pass-through
inspector-remove-link = Remove link

## Totals

totals-title = Totals
totals-incomplete = Part of the graph is undetermined, so these totals leave it out.
totals-depot-in = From the depot
totals-depot-out = To the depot
totals-storage = Storage balance
totals-power = Power
totals-power-draw = Draw
totals-power-generation = Generation
totals-power-net = Net
totals-power-kw = { $kw } kW
totals-facilities = Facilities
totals-raw = Raw resources
totals-raw-use = { $used } / { $available } /min
totals-raw-use-unlimited = { $used } /min
totals-none = none
totals-per-min = { $rate } /min

## Warnings

warnings-title = Warnings
warning-undetermined-no-limit = A part of the graph has no limit. Add a source, a facility count limit, a sink limit or a control port.
warning-undetermined-unbounded = A part of the graph can grow without end. Add a limit to it.
warning-failed = The solver failed on a part of the graph: { $message }
warning-belt-over = A { $item } link needs { $lanes } belts.
warning-pipe-over = A { $item } link needs { $lanes } pipes.
warning-resource-over = { $item }: using { $used } /min, but the region supplies { $available } /min.
warning-issue-bad-link = A link breaks a linking rule: { $error }
warning-issue-missing-recipe = Recipe { $recipe } is not in this data version.
warning-issue-unknown-item = Item { $item } is not in this data version.
warning-issue-recipe-region = { $recipe } is not available in this region.
warning-issue-facility-region = { $facility } is not available in this region.
warning-issue-facility-cannot-run = { $facility } can't run this recipe.
warning-issue-invalid-limit = A node has a limit that is not a number of at least 0.
link-refused = Can't link: { $error }
link-error-mismatch = { $from } doesn't go into { $to }.
link-error-solid-fluid = { $from } and { $to } don't mix: one is a solid, the other a fluid.
link-error-no-item = neither end has an item yet.
link-error-self = a node can't link to itself.
link-error-duplicate = these two ports are already linked.
link-error-other = these ports don't fit together.

## Settings

settings-title = Raw resources
settings-hint = How much of each raw resource the region can supply. Blank is unlimited. Going over shows a warning; it never limits the solver.
settings-per-min = /min
settings-no-raw = This region has no raw resources in the data.
