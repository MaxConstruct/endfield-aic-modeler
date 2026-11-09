# English UI strings. Every user-facing string in aic-app comes from here.
# Numbers arrive preformatted, so messages never format them.

app-title = AIC Modeler

## Startup

load-failed-title = Could not load game data
load-failed-hint = Put a data folder next to the app, or start it with --data <path>.

## Window and menus

window-title = { $file } — AIC Modeler
window-title-modified = { $file } • — AIC Modeler
menu-file = File
menu-new = New…
menu-open = Open…
menu-open-recent = Open Recent
menu-save = Save
menu-save-as = Save As…
menu-export-png = Export Canvas as PNG…
export-done = Saved the canvas to { $file }.
export-failed = Could not export { $file }: { $error }
menu-edit = Edit
menu-undo = Undo
menu-redo = Redo
menu-copy = Copy
menu-paste = Paste
menu-duplicate = Duplicate
menu-delete = Delete
menu-select-all = Select All
menu-group = Group
menu-ungroup = Ungroup
menu-view = View
menu-round-up = Round up facilities
toggle-round-up = Round up

## Files

file-untitled = Untitled
file-open-failed = Could not open { $file }: { $error }
file-save-failed = Could not save { $file }: { $error }
file-other-data-version = This file was made with data { $file }; the app uses { $active }. Recipes are matched by ID.
discard-title = Discard unsaved changes?
discard-body = The graph has changes that aren't saved. They will be lost.
discard-confirm = Discard
dialog-cancel = Cancel
new-title = New graph
new-region = Region
new-create = Create

## Top bar

region-label = Region
region-locked = A graph belongs to one region. File > New starts a graph in another.
data-version = Data { $version }
settings-button = Settings…
toggle-recipes = Recipes
toggle-inspector = Inspector
status-slow-solve = Solving took { $ms } ms. The canvas may stutter.

## Node kinds

kind-recipe = Recipe
kind-source = Source
kind-sink = Sink
kind-outpost-order = Outpost order
kind-splitter = Splitter
kind-merger = Merger
kind-storage = Storage
kind-control-port = Control port
kind-missing-recipe = Missing recipe
kind-group = Group
group-default-name = Group { $number }
group-node-count = { $count } nodes
group-node-count-one = 1 node
group-expand = Expand group
group-collapse = Collapse group
group-ungroup = Ungroup
group-remove = Remove group and its nodes
group-copies-mode = Stands for copies
group-copies-hint = Limits inside define one copy. The solver finds how many copies the graph can run; totals count every copy.
group-copies = copies
group-copies-label = Copies
group-copies-limit = Copies limit
group-refused = Only nodes and groups inside the same group can be grouped together.
item-unset = ?
node-title = { $kind } · { $subject }

## Node cards

node-no-limit = no limit
node-limit-label = Node limit
node-per-min = { $rate } /min
storage-balance = { $balance } /min
storage-pass-through = through { $rate } /min
node-remove = Remove node
round-up-utilization = { $percent }% busy
outpost-earns = earns { $rate } /min
throttle-marker-hint = This port holds the node below its limit.

## Port menu (double-click a port)

port-menu-limit = Limit
port-menu-limit-label = Port limit
port-menu-clear = Clear connections
port-menu-ratio = Ratio lock
port-menu-ratio-hint = Fix how the links share this port's flow, e.g. 70 and 30. Blank links share the rest evenly.
port-menu-share-free = even
port-menu-share-label = Share for { $node }
port-menu-unlock = Unlock: split evenly

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
inspector-round-up = Rounded up
inspector-round-up-value = { $facilities } at { $percent }%
inspector-many = { $count } nodes selected
inspector-group-name = Group name
inspector-in-group = In { $group }
inspector-one-copy = Shows one copy of { $copies }.
inspector-remove-nodes = Remove nodes
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
inspector-share-out = Share at its source
inspector-share-in = Share at its target
inspector-balance = Balance
inspector-earns = Earns
inspector-earns-value = { $rate } /min { $coupon }
inspector-pass-through = Pass-through
inspector-remove-link = Remove link

## Totals

totals-title = Totals
totals-incomplete = Part of the graph is undetermined, so these totals leave it out.
totals-depot-in = From the depot
totals-depot-out = To the depot
totals-storage = Storage balance
totals-coupons = Coupons from outpost orders
totals-power = Power
totals-power-draw = Draw
totals-power-generation = Generation
totals-power-net = Net
totals-power-kw = { $kw } kW
totals-facilities = Facilities
totals-facilities-rounded = → { $count }
totals-facilities-rounded-hint = Whole facilities to build: each node rounded up.
totals-raw = Raw resources
totals-raw-use = { $used } / { $available } /min
totals-raw-use-unlimited = { $used } /min
totals-none = none
totals-per-min = { $rate } /min

## Warnings

warnings-title = Warnings
warning-undetermined-no-limit = A part of the graph has no limit. Add a source, a facility count limit, a sink limit or a control port.
warning-copies-undetermined = A part of the graph depends on a copies group whose count can't be settled.
warning-copies-undetermined-group = One copy of { $group } has no limit inside, so its count can't be settled. Add a limit inside it.
warning-undetermined-unbounded = A part of the graph can grow without end. Add a limit to it.
warning-failed = The solver failed on a part of the graph: { $message }
warning-belt-over = A link of { $item } needs { $lanes } belts.
warning-pipe-over = A link of { $item } needs { $lanes } pipes.
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

settings-title = Settings
settings-raw-title = Raw resources
settings-data-version = Data version
settings-language = Game names
settings-newest = { $version } (newest)
data-switch-failed = Could not load that data: { $error }
settings-hint = How much of each raw resource the region can supply. Blank is unlimited. Going over shows a warning; it never limits the solver.
settings-per-min = /min
settings-no-raw = This region has no raw resources in the data.
