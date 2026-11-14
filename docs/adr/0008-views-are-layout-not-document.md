# Views are layout state, not part of the document

The canvas can show the open graph in several views at once: tabs in a dock that splits without limit, and on the desktop separate OS windows. Each view has its own level and camera. Everything else is shared: there is one document, one solve, one undo history, one autosave and one selection. Like a file open in two panes of a code editor, an edit in one view shows in all of them on the same frame.

The views and their arrangement are the app's layout, kept in eframe storage with the settings. They are not saved in the `.aicm` file, which still holds one camera (the focused view's). A graph means the same thing however it was being looked at, and opening a file on another machine, or in the web build where windows can't be detached, must not depend on a layout that may not fit there.

## Considered Options

- **Several documents open at once, one per tab.** Rejected: the editor is built around one document with its region, undo and autosave; making it multi-document is a much larger change for a use the player doesn't have.
- **A selection per view, like a cursor per editor pane.** Rejected by the player: selecting a node should highlight it wherever it is shown, and the one inspector should not depend on which view was clicked last.
- **Save the layout in the file.** Rejected for the reasons above, and because it would change the save format for state that is not part of the plan.
- **Detached windows with their own menus and status bar.** Rejected: a detached window shows only the canvas; commands stay in the main window.

## Consequences

- Per-view state (camera, level, pin centres, limit fields being typed, egui-snarl's state) moves out of the editor into a view; egui-snarl's id includes the view as well as the level.
- Commands that act on the view (zoom, fit) act on the focused view; commands that act on the graph act on the shared selection from any view.
- A view whose workspace no longer exists falls back to the top level.
- The web build has tabs and splits but no detached windows.
