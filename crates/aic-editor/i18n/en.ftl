# English UI strings. Every user-facing string in aic-app comes from here.
# Numbers arrive preformatted, so messages never format them.

app-title = AIC Modeler

## Startup

load-failed-title = Could not load game data
load-failed-hint = Put a data folder next to the app, or start it with --data <path>.
load-failed-hint-web = This build has no game data in it. Build it again with the data folder in place.

## Window and menus

window-title = { $file } — AIC Modeler
window-title-modified = { $file } • — AIC Modeler
menu-file = File
menu-new = New
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
menu-cut = Cut
menu-copy = Copy
menu-paste = Paste
menu-duplicate = Duplicate
menu-delete = Delete
menu-select-all = Select All
menu-make-workspace = Make workspace
menu-dissolve-workspace = Dissolve workspace
menu-view = View
menu-round-up = Round up facilities
menu-performance = Frame rate and solve time

## Views

view-split-right = Split right
view-split-down = Split down
view-duplicate = Duplicate view
view-move-to-window = Move to new window
view-close = Close view
view-numbered = { $title } · { $number }
toolbar-hint = { $name } ({ $keys })

## Save indicator (status bar)

save-state-saved = ✔ Saved
save-state-draft = ✔ Saved as a draft
save-state-saving = ⟳ Saving…
save-state-unsaved = ⚠ Not saved
save-state-failed = ⚠ Not saved
save-state-empty = Nothing to save yet
save-state-hint = { $file }, saved { $when }
save-state-just-now = just now
save-state-minutes = { $minutes } min ago

## Files

file-untitled = Untitled
file-open-failed = Could not open { $file }: { $error }
file-save-failed = Could not save { $file }: { $error }
file-last-missing = The last graph, { $file }, is gone. Started a new one.
file-save-as-title = Save As
file-save-as-name = Name
file-save-as-replaces = This replaces the saved file of that name.
file-save-as-hint = The file is kept in this browser and downloaded.
file-save-as-confirm = Save
file-save-as-cancel = Cancel
export-downloaded = Downloaded the canvas as { $file }.
file-other-data-version = This file was made with data { $file }; the app uses { $active }. Recipes are matched by ID.

## Top bar

toggle-recipes = Recipes
toggle-inspector = Inspector
toggle-tutorial = Tutorial

## Status bar

# The app's own version, before the author's name.
status-version = v{ $version }
# Between the version, the author's name and the GitHub link.
status-separator = ·
# A link to the app's code on GitHub.
status-github = GitHub
# Hover text on the code's license, shown after the GitHub link.
status-license-hint = The app's code is under this license; the game's data, names and icons belong to Hypergryph / Gryphline and are not.
status-slow-solve = Solving this slowly may make the canvas stutter.
status-solve-time = Solve { $ms } ms
status-frame-rate = { $fps } fps
zoom-label = Zoom
zoom-in = Zoom in
zoom-out = Zoom out
zoom-percent = { $percent }%

## Tutorial

tutorial-title = Tutorial
tutorial-add-title = Add a node
tutorial-add-body = Right-click the canvas and pick a recipe. Type a limit on its card, or double-click a port to limit it by rate.
tutorial-grow-title = Grow the chain
tutorial-grow-body = Drag a port onto empty canvas to add a node that makes or uses its item. It comes already linked.
tutorial-keys-title = Keys
tutorial-save-title = Save and export
tutorial-save-desktop = Edits save on their own, to a draft until you save the graph to a file.
tutorial-save-browser = Edits save on their own in this browser. Save also downloads the file.
tutorial-export = { $menu } › { $item } saves the canvas as a picture.

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
kind-workspace = Workspace
kind-workspace-input = Input
kind-workspace-output = Output
workspace-default-name = Workspace { $number }
workspace-node-count = { $count } nodes
workspace-node-count-one = 1 node
workspace-open = Open
workspace-title = Title
workspace-paste-into = Paste into
workspace-dissolve = Dissolve
workspace-remove = Delete workspace and its nodes
workspace-refused = Only nodes and workspaces inside the same workspace can go in a new workspace together.
workspace-back = ‹ Back
workspace-back-hint = Go back out of this workspace
workspace-top = Top
workspace-add-input = + Input
workspace-add-input-hint = Add a port that brings an item into this workspace
workspace-add-output = + Output
workspace-add-output-hint = Add a port that sends an item out of this workspace
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
inspector-workspace-name = Workspace name
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
warning-port-in-unlinked-outside = The { $item } input of { $workspace } isn't linked outside, so nothing comes in through it.
warning-port-in-unlinked-inside = The { $item } input of { $workspace } isn't linked to anything inside.
warning-port-out-unlinked-outside = The { $item } output of { $workspace } isn't linked outside, so nothing goes out through it.
warning-port-out-unlinked-inside = Nothing inside { $workspace } is linked to its { $item } output.
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
link-error-across-workspaces = a link can't cross into or out of a workspace except through one of its ports.
link-error-other = these ports don't fit together.

## Settings

settings-title = Settings
region-label = Region
settings-raw-title = Raw resources
settings-data-version = Data version
settings-language = Game names
settings-newest = { $version } (newest)
data-switch-failed = Could not load that data: { $error }
settings-hint = How much of each raw resource the region can supply. Blank is unlimited. Going over shows a warning; it never limits the solver.
settings-per-min = /min
settings-no-raw = This region has no raw resources in the data.
