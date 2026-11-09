# Workspaces with explicit ports replace groups

Groups were a view over the flat graph: a group collapsed into one card whose pins were whatever ports of its nodes weren't linked inside it, and expanded back in place. A group could also stand for N copies of what was in it. In use, this didn't match how players think about a part of a factory. Satisfactory Modeler, which players compare us with, has workspaces (it calls them outposts): a place you open, work in, and leave, whose inputs and outputs you choose.

So a group is now a **workspace**. There is no collapsing or expanding: from outside, a workspace is a card; a double-click (or Open) shows only what is in it, with a back button and a path to the top. Its inputs and outputs are **ports** the player adds, pinned to the left and right edges of its canvas. A port is a node of the workspace (`NodeKind::WorkspacePort`) with an outside end, which links in the workspace's parent reach, and an inside end, which links inside reach. A link joins two nodes of one workspace, or reaches a port's end from the side that end faces; nothing else crosses a workspace's edge (`LinkError::AcrossWorkspaces`).

A port with either end unlinked lets nothing through (it holds the flow at 0, which counts as a limit), and the solver warns about it (`Warning::PortUnlinked`). This differs from an unlinked port of a recipe node, which is on the depot (ADR 0001): a workspace's port is a promise to be linked, and pulling from the depot through it would hide a forgotten link.

Copies are gone. In Satisfactory a blueprint pays off because the same block of constructors repeats many times; in Endfield space is tight and a layout rarely repeats, so the mode cost more in code and in the player's attention than it gave.

## Considered Options

- **Keep groups and add a "view inside" mode.** Rejected: ports derived from links change as the player edits, so the card's pins would shift under them, and a workspace couldn't have an input before something inside used it.
- **Unlinked workspace ports on the depot, like any open port.** Rejected by the player: a workspace's numbers should come from what it is linked to, and a missing link should be visible.
- **Ports as movable cards inside the workspace.** Rejected for ports pinned to the edges, as in Satisfactory Modeler: they stay in view while the player pans and zooms.
- **Keep copies on workspaces.** Rejected later, for the reason above.

## Consequences

- The save format is version 2: `workspaces` instead of `groups`, a node's `workspace` instead of `group`, and `workspace_port` nodes. A version 1 file still opens: its groups become workspaces and every link crossing a group's edge is sent through new ports (one per outside producer coming in, one per inside producer going out), so it solves the same.
- Making a workspace from a selection (Cmd/Ctrl+G) routes crossing links through new ports the same way; dissolving one (Cmd/Ctrl+Shift+G) links what fed each port straight to what it fed.
- An empty workspace is allowed and stays until deleted.
- Copy and paste carry workspaces with everything in them; a port pasted at the top of the graph is left out, since only a workspace has ports.
- `Solution::copies`, `group_flow`, `node_copies`, `Warning::CopiesUndetermined` and `UndeterminedReason::CopiesUndetermined` are gone.
