//! One open graph: the `aic-core` [`Graph`], which is the source of truth, its latest [`Solution`],
//! and the undo history and save state around them. No view, no level: a UI keeps those (ADR 0008)
//! and asks for what depends on a level by passing it in.
//!
//! [`Document::generation`] counts every change to what a canvas draws, so a view that was parked
//! can tell it must redraw, and a UI that lays a level out once per change (its cards and wires)
//! knows when to do it again.
//!
//! Undo and redo keep whole `Graph` snapshots: graphs are small, so a clone per step is cheap.
//! Edits since the last [`Document::checkpoint`] make one step, so a drag or a typed number undoes
//! in one go.

use std::path::{Path, PathBuf};
use std::time::Duration;

use web_time::Instant;

use aic_core::{
    EditError, Graph, KeptSections, LinkError, LinkId, Member, NodeId, NodeKind, Port, PortRef,
    Pos, Side, Solution, WorkspaceId, solve_keeping,
};
use aic_data::{Catalog, FacilityId, ItemId, RegionId};

/// Where a canvas looks: which level of the graph. `None` is the top. Views are the UI's layout
/// (ADR 0008); the document never stores the level a canvas shows.
pub type Level = Option<WorkspaceId>;

/// Where a new node's edge should land once its size is known: its right edge for a node that
/// feeds the port it was added from, its left edge for one that the port feeds. In graph space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Anchor {
    Left(f32),
    Right(f32),
}

/// How many undo steps are kept.
const HISTORY_LIMIT: usize = 200;

#[derive(Debug)]
pub struct Document {
    graph: Graph,
    solution: Solution,
    /// Each section's result at the last solve, kept for the next one while it holds.
    kept: KeptSections,
    stale: bool,
    /// The graph as of the last checkpoint, and that state's revision.
    committed: Graph,
    revision: u64,
    /// Whether the graph changed since the last checkpoint.
    pending: bool,
    undo: Vec<(Graph, u64)>,
    redo: Vec<(Graph, u64)>,
    next_revision: u64,
    /// Counts every change, moves and undos included; saving records the count it saved.
    edits: u64,
    /// Counts every change to what the views draw, so a view swapped in knows to draw afresh.
    generation: u64,
    /// Counts the solves, so what shows the solution knows when it changed.
    solves: u64,
    saved_edits: u64,
    path: Option<PathBuf>,
    /// The file is a draft in the app's own folder, not one the player chose.
    draft: bool,
}

impl Document {
    pub fn new(region: RegionId) -> Self {
        Self::from_graph(Graph::new(region), None)
    }

    /// A document showing `graph`, opened from `path` if it came from a file.
    pub fn from_graph(graph: Graph, path: Option<PathBuf>) -> Self {
        Self {
            committed: graph.clone(),
            graph,
            solution: Solution::default(),
            kept: KeptSections::default(),
            stale: true,
            revision: 0,
            pending: false,
            undo: Vec::new(),
            redo: Vec::new(),
            next_revision: 1,
            edits: 0,
            generation: 0,
            solves: 0,
            saved_edits: 0,
            path,
            draft: false,
        }
    }

    /// The file this document was opened from or last saved to, a draft included.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Whether the document lives in a draft file the app made, rather than one the player chose.
    pub fn is_draft(&self) -> bool {
        self.draft
    }

    /// Whether anything changed since the last save (or, never saved, since it was made).
    pub fn is_modified(&self) -> bool {
        self.edits != self.saved_edits
    }

    /// Records that the current state was saved to `path`, a draft or the player's file. Doesn't
    /// end the undo step, so an autosave in the middle of a drag leaves the drag one step.
    pub fn mark_saved(&mut self, path: PathBuf, draft: bool) {
        self.saved_edits = self.edits;
        self.path = Some(path);
        self.draft = draft;
    }

    /// Opened from a draft file: keep saving there.
    pub fn set_draft(&mut self, draft: bool) {
        self.draft = draft;
    }

    /// Counts every change to what a canvas draws, moves and undos included. A view redraws when
    /// it moves on.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Counts the solves. [`Self::generation`] counts edits, not solves: what shows the numbers
    /// follows both.
    pub fn solves(&self) -> u64 {
        self.solves
    }

    /// Ends the current undo step: everything changed since the last checkpoint undoes together.
    pub fn checkpoint(&mut self) {
        if !self.pending {
            return;
        }
        let previous = std::mem::replace(&mut self.committed, self.graph.clone());
        self.undo.push((previous, self.revision));
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.revision = self.next_revision;
        self.next_revision += 1;
        self.pending = false;
    }

    pub fn can_undo(&self) -> bool {
        self.pending || !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.pending && !self.redo.is_empty()
    }

    /// Goes back one step. Returns false when there is nothing to undo.
    pub fn undo(&mut self) -> bool {
        self.checkpoint();
        let Some((graph, revision)) = self.undo.pop() else {
            return false;
        };
        let current = std::mem::replace(&mut self.committed, graph);
        self.redo.push((current, self.revision));
        self.revision = revision;
        self.restore();
        true
    }

    /// Goes forward one undone step. Returns false when there is nothing to redo.
    pub fn redo(&mut self) -> bool {
        if self.pending {
            return false;
        }
        let Some((graph, revision)) = self.redo.pop() else {
            return false;
        };
        let current = std::mem::replace(&mut self.committed, graph);
        self.undo.push((current, self.revision));
        self.revision = revision;
        self.restore();
        true
    }

    /// Switches to another data version or language: recipe nodes are matched against `catalog`
    /// again, in the graph and in every undo step. Not an edit, so it isn't an undo step itself.
    pub fn rebind(&mut self, catalog: &Catalog) {
        self.graph.rebind(catalog);
        self.committed.rebind(catalog);
        for (graph, _) in self.undo.iter_mut().chain(self.redo.iter_mut()) {
            graph.rebind(catalog);
        }
        // What a section came to holds for the catalog it was solved with.
        self.kept.clear();
        self.stale = true;
        self.generation += 1;
    }

    /// Shows the committed graph again. A level that is gone is the UI's to notice.
    fn restore(&mut self) {
        self.edits += 1;
        self.generation += 1;
        self.graph = self.committed.clone();
        self.stale = true;
    }

    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    pub fn solution(&self) -> &Solution {
        &self.solution
    }

    /// `level` if it is still in the graph, else the top. A view may name a workspace an undo
    /// took away; every call that takes a level falls back this way, so none panics and no node
    /// ends up in a workspace that doesn't exist.
    pub fn resolve_level(&self, level: Level) -> Level {
        level.filter(|&id| self.graph.workspace_info(id).is_some())
    }

    /// The workspaces from the top down to `level`, outermost first.
    pub fn level_path(&self, level: Level) -> Vec<WorkspaceId> {
        let mut path = Vec::new();
        let mut current = self.resolve_level(level);
        while let Some(id) = current {
            path.push(id);
            current = self.graph.workspace_info(id).and_then(|w| w.parent);
        }
        path.reverse();
        path
    }

    /// What draws `node` at `level`: the node itself, or the workspace it is in. `None` when it
    /// is outside the level.
    pub fn shown(&self, level: Level, node: NodeId) -> Option<Member> {
        shown(&self.graph, self.resolve_level(level), node)
    }

    /// Marks the graph as changed: it needs a solve and makes an undo step.
    fn changed(&mut self) {
        self.stale = true;
        self.pending = true;
        self.edits += 1;
        self.generation += 1;
    }

    /// Moves the graph to another region (see [`Graph::set_region`]).
    pub fn set_region(&mut self, region: RegionId) {
        self.graph.set_region(region);
        self.changed();
    }

    /// Moves a node or workspace to `pos`: an undo step, but not a re-solve, since a position
    /// doesn't affect the solution. Does nothing if it is already there. A workspace's own ports
    /// stay on the canvas's edges, so they don't move.
    pub fn move_member(&mut self, member: Member, pos: Pos) {
        let moved = match member {
            Member::Node(id) if is_port(&self.graph, id) => false,
            Member::Node(id) => {
                self.graph.node(id).is_some_and(|node| node.pos != pos)
                    && self.graph.set_pos(id, pos).is_ok()
            }
            Member::Workspace(id) => {
                self.graph.workspace_info(id).is_some_and(|w| w.pos != pos)
                    && self.graph.set_workspace_pos(id, pos).is_ok()
            }
        };
        if moved {
            self.pending = true;
            self.edits += 1;
            self.generation += 1;
        }
    }

    /// Adds a node to `level`, at `pos`.
    pub fn add_node(&mut self, kind: NodeKind, pos: Pos, level: Level) -> NodeId {
        let level = self.resolve_level(level);
        let id = self
            .graph
            .add_node_in(kind, pos, level)
            .expect("the level exists");
        self.changed();
        id
    }

    /// Pastes `other` into `into` (a level), moved by `offset` (see [`Graph::paste_into`]).
    pub fn paste(
        &mut self,
        catalog: &Catalog,
        other: &Graph,
        offset: Pos,
        into: Level,
    ) -> Vec<Member> {
        let into = self.resolve_level(into);
        let pasted = self.graph.paste_into(catalog, other, offset, into);
        self.changed();
        pasted
    }

    /// Adds a port to the workspace `level`. Ports belong to a workspace, so at the top there is
    /// none to add.
    pub fn add_port(&mut self, level: Level, side: Side) -> Option<NodeId> {
        let port = self.graph.add_port(self.resolve_level(level)?, side).ok()?;
        self.changed();
        Some(port)
    }

    /// Adds an empty workspace to `level`.
    pub fn add_workspace(&mut self, name: &str, level: Level, pos: Pos) -> WorkspaceId {
        let level = self.resolve_level(level);
        let id = self
            .graph
            .add_workspace(name, level, pos)
            .expect("the level exists");
        self.changed();
        id
    }

    pub fn remove_node(&mut self, id: NodeId) {
        self.graph.remove_node(id);
        self.changed();
    }

    /// Removes a workspace with everything in it.
    pub fn remove_workspace(&mut self, id: WorkspaceId) {
        self.graph.remove_workspace(id);
        self.changed();
    }

    /// Puts `members` in a new workspace (see [`Graph::workspace_from`]).
    pub fn workspace_from(
        &mut self,
        members: &[Member],
        name: &str,
    ) -> Result<WorkspaceId, EditError> {
        let workspace = self.graph.workspace_from(members, name)?;
        self.changed();
        Ok(workspace)
    }

    /// Takes a workspace apart (see [`Graph::dissolve`]).
    pub fn dissolve(&mut self, id: WorkspaceId) -> Result<(), EditError> {
        self.graph.dissolve(id)?;
        self.changed();
        Ok(())
    }

    pub fn rename_workspace(&mut self, id: WorkspaceId, name: &str) -> Result<(), EditError> {
        self.graph.rename_workspace(id, name)?;
        self.changed();
        Ok(())
    }

    pub fn connect(
        &mut self,
        catalog: &Catalog,
        from: PortRef,
        to: PortRef,
    ) -> Result<LinkId, LinkError> {
        let link = self.graph.connect(catalog, from, to)?;
        self.changed();
        Ok(link)
    }

    pub fn remove_link(&mut self, id: LinkId) {
        self.graph.remove_link(id);
        self.changed();
    }

    pub fn set_limit(&mut self, id: NodeId, limit: Option<f64>) -> Result<(), EditError> {
        self.graph.set_limit(id, limit)?;
        self.changed();
        Ok(())
    }

    /// Sets the node's limit as a flow through `port` (see [`Graph::set_port_limit`]).
    pub fn set_port_limit(
        &mut self,
        catalog: &Catalog,
        port: &Port,
        rate: Option<f64>,
    ) -> Result<(), EditError> {
        self.graph.set_port_limit(catalog, port, rate)?;
        self.changed();
        Ok(())
    }

    /// Sets a link's share of the ratio lock at its `side` end (see [`Graph::set_share`]).
    pub fn set_share(
        &mut self,
        link: LinkId,
        side: Side,
        share: Option<f64>,
    ) -> Result<(), EditError> {
        self.graph.set_share(link, side, share)?;
        self.changed();
        Ok(())
    }

    /// Removes every link at `port`.
    pub fn clear_port(&mut self, port: &Port) {
        let links: Vec<LinkId> = self
            .graph
            .links_at(port.node, port.side)
            .filter(|(_, link)| link.item == port.item)
            .map(|(id, _)| id)
            .collect();
        for link in links {
            self.remove_link(link);
        }
    }

    pub fn set_facility(&mut self, id: NodeId, facility: FacilityId) -> Result<(), EditError> {
        self.graph.set_facility(id, facility)?;
        self.changed();
        Ok(())
    }

    /// Re-solves if anything changed since the last solve, and says how long it took. A section
    /// whose nodes, links and limits are as they were keeps its result ([`KeptSections`]).
    pub fn solve_if_stale(&mut self, catalog: &Catalog) -> Option<Duration> {
        if !self.stale {
            return None;
        }
        let start = Instant::now();
        self.solution = solve_keeping(catalog, &self.graph, &mut self.kept);
        self.stale = false;
        self.solves += 1;
        Some(start.elapsed())
    }
}

/// What one pin on the canvas stands for: a port of `node`. On a workspace's card it is the
/// outside end of one of the workspace's ports. `item` is `None` on a logistics node or port with
/// no item yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinTarget {
    pub node: NodeId,
    pub item: Option<ItemId>,
}

impl PinTarget {
    /// Whether a link of `item` at `node` uses this pin.
    pub fn is(&self, node: NodeId, item: &ItemId) -> bool {
        self.node == node && self.item.as_ref().is_none_or(|own| own == item)
    }
}

/// Whether `node` is a workspace port.
pub fn is_port(graph: &Graph, node: NodeId) -> bool {
    graph
        .node(node)
        .is_some_and(|n| matches!(n.kind, NodeKind::WorkspacePort { .. }))
}

/// What draws `node` at `level`: the node itself, or the workspace in `level` that holds it.
/// `None` when the node is outside `level`.
pub fn shown(graph: &Graph, level: Option<WorkspaceId>, node: NodeId) -> Option<Member> {
    let mut current = graph.node(node)?.workspace;
    if current == level {
        return Some(Member::Node(node));
    }
    while let Some(id) = current {
        let parent = graph.workspace_info(id)?.parent;
        if parent == level {
            return Some(Member::Workspace(id));
        }
        current = parent;
    }
    None
}

/// The pins of a node or workspace card on one side, in order. A workspace's pins are its ports'
/// outside ends; a port shown on its own (on the edge of the workspace it belongs to) has only its
/// inside end.
pub fn pin_targets(graph: &Graph, catalog: &Catalog, member: Member, side: Side) -> Vec<PinTarget> {
    match member {
        Member::Node(node) => {
            let Some(found) = graph.node(node) else {
                return Vec::new();
            };
            if let NodeKind::WorkspacePort {
                side: port_side,
                item,
            } = &found.kind
            {
                // An input port feeds in from its output side, and the other way round.
                return if *port_side == side {
                    Vec::new()
                } else {
                    vec![PinTarget {
                        node,
                        item: item.clone(),
                    }]
                };
            }
            found
                .kind
                .port_items(catalog, side)
                .into_iter()
                .map(|item| PinTarget { node, item })
                .collect()
        }
        Member::Workspace(workspace) => graph
            .ports_of(workspace, side)
            .into_iter()
            .map(|port| PinTarget {
                node: port,
                item: graph.node(port).and_then(|n| n.kind.item().cloned()),
            })
            .collect(),
    }
}

/// The items of a node's ports on one side, in pin order. A logistics node has one port per side,
/// whose item is `None` until its first link.
pub fn pins(catalog: &Catalog, kind: &NodeKind, side: Side) -> Vec<Option<ItemId>> {
    kind.port_items(catalog, side)
}

/// The port a link end names: by item on a recipe, source or sink node; the single port of a
/// logistics node.
pub fn port_ref(kind: &NodeKind, node: NodeId, item: Option<&ItemId>) -> PortRef {
    match item {
        Some(item) if !kind.is_logistics() => PortRef::new(node, item.clone()),
        _ => PortRef::any(node),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::fixture_catalog;

    fn catalog() -> Catalog {
        fixture_catalog()
    }

    fn recipe() -> NodeKind {
        NodeKind::Recipe {
            recipe: "iron_ingot".into(),
            facility: "smelter".into(),
            limit: Some(1.0),
        }
    }

    fn origin() -> Pos {
        Pos::default()
    }

    fn doc() -> Document {
        Document::new("tundra".into())
    }

    #[test]
    fn a_data_switch_solves_every_section_again() {
        let mut doc = doc();
        let node = doc.add_node(recipe(), origin(), None);
        doc.solve_if_stale(&catalog());
        let before = doc.solution().port_flow.clone();

        // The same data with the smelter twice as fast: the kept section no longer holds.
        let overlay = br#"{ "facility_speeds": { "smelter": 2 } }"#;
        let faster = Catalog::from_bytes_with_overlay(
            include_bytes!("../fixtures/satisfactory/vtest/data.json"),
            Some(overlay),
            include_bytes!("../fixtures/satisfactory/vtest/i18n/en.json"),
            None,
        )
        .unwrap();
        doc.rebind(&faster);
        doc.solve_if_stale(&faster);
        let port = |side| Port {
            node,
            item: if side == Side::Input {
                "iron_ore"
            } else {
                "iron_ingot"
            }
            .into(),
            side,
        };
        let flow = |flows: &std::collections::HashMap<Port, Option<f64>>, side| flows[&port(side)];
        assert_eq!(
            flow(&doc.solution().port_flow, Side::Output),
            flow(&before, Side::Output).map(|f| f * 2.0)
        );
    }

    #[test]
    fn a_removed_node_needs_a_new_solve() {
        let catalog = catalog();
        let mut doc = doc();
        let node = doc.add_node(recipe(), Pos { x: 10.0, y: 20.0 }, None);
        doc.solve_if_stale(&catalog);
        assert!(doc.solve_if_stale(&catalog).is_none(), "nothing changed");

        let solves = doc.solves();
        doc.remove_node(node);
        assert!(doc.solve_if_stale(&catalog).is_some(), "removal re-solves");
        assert_eq!(doc.solves(), solves + 1);
    }

    #[test]
    fn a_move_is_an_undo_step_but_not_a_solve_and_counts_as_a_change() {
        let catalog = catalog();
        let mut doc = doc();
        let node = doc.add_node(recipe(), origin(), None);
        doc.checkpoint();
        doc.solve_if_stale(&catalog);
        let generation = doc.generation();
        doc.mark_saved("plan.aicm".into(), false);

        doc.move_member(Member::Node(node), Pos { x: 5.0, y: 6.0 });
        assert!(doc.solve_if_stale(&catalog).is_none());
        assert_eq!(doc.graph().node(node).unwrap().pos, Pos { x: 5.0, y: 6.0 });
        assert!(doc.generation() > generation);
        assert!(doc.is_modified());
        doc.checkpoint();
        assert!(doc.undo());
        assert_eq!(doc.graph().node(node).unwrap().pos, origin());
    }

    #[test]
    fn moving_to_where_it_already_is_changes_nothing() {
        let mut doc = doc();
        let node = doc.add_node(recipe(), origin(), None);
        doc.checkpoint();
        let generation = doc.generation();
        doc.move_member(Member::Node(node), origin());
        assert_eq!(doc.generation(), generation);
        assert!(!doc.can_redo());
    }

    #[test]
    fn edits_between_checkpoints_undo_as_one_step() {
        let mut doc = doc();
        let node = doc.add_node(recipe(), origin(), None);
        doc.checkpoint();
        doc.set_limit(node, Some(2.0)).unwrap();
        doc.set_limit(node, Some(3.0)).unwrap();
        doc.checkpoint();

        assert!(doc.undo());
        assert_eq!(doc.graph().node(node).unwrap().kind.limit(), Some(1.0));
        assert!(doc.undo());
        assert!(doc.graph().nodes().is_empty());
        assert!(!doc.undo(), "nothing left to undo");

        assert!(doc.redo());
        assert!(doc.redo());
        assert_eq!(doc.graph().node(node).unwrap().kind.limit(), Some(3.0));
    }

    #[test]
    fn undo_brings_back_links() {
        let catalog = catalog();
        let mut doc = doc();
        let smelter = doc.add_node(recipe(), origin(), None);
        let splitter = doc.add_node(NodeKind::Splitter { item: None }, origin(), None);
        doc.connect(
            &catalog,
            PortRef::new(smelter, "iron_ingot"),
            PortRef::any(splitter),
        )
        .unwrap();
        doc.checkpoint();
        doc.remove_node(splitter);
        doc.checkpoint();

        doc.undo();
        assert_eq!(doc.graph().links().len(), 1);
    }

    #[test]
    fn a_new_edit_drops_the_redo_steps() {
        let mut doc = doc();
        doc.add_node(recipe(), origin(), None);
        doc.checkpoint();
        doc.undo();
        doc.add_node(NodeKind::Splitter { item: None }, origin(), None);
        doc.checkpoint();
        assert!(!doc.redo());
    }

    #[test]
    fn every_change_after_a_save_needs_saving_undo_included() {
        let mut doc = doc();
        assert!(!doc.is_modified());
        doc.add_node(recipe(), origin(), None);
        assert!(doc.is_modified());
        doc.mark_saved("plan.aicm".into(), false);
        assert!(!doc.is_modified());
        doc.checkpoint();
        doc.undo();
        assert!(doc.is_modified(), "the file still has the node");
    }

    #[test]
    fn saving_mid_edit_keeps_the_edit_one_undo_step() {
        let mut doc = doc();
        let node = doc.add_node(recipe(), origin(), None);
        doc.checkpoint();
        doc.set_limit(node, Some(2.0)).unwrap();
        doc.mark_saved("plan.aicm".into(), false);
        doc.set_limit(node, Some(3.0)).unwrap();
        doc.checkpoint();
        doc.undo();
        assert_eq!(doc.graph().node(node).unwrap().kind.limit(), Some(1.0));
    }

    #[test]
    fn a_document_from_a_graph_is_not_modified() {
        let graph = Graph::new("tundra".into());
        let doc = Document::from_graph(graph, Some("plan.aicm".into()));
        assert!(!doc.is_modified());
        assert_eq!(doc.path(), Some(Path::new("plan.aicm")));
    }

    #[test]
    fn a_level_is_passed_in_never_stored() {
        let mut doc = doc();
        let workspace = doc.add_workspace("W", None, origin());
        let inside = doc.add_node(recipe(), origin(), Some(workspace));
        assert_eq!(doc.level_path(Some(workspace)), [workspace]);
        assert_eq!(
            doc.shown(None, inside),
            Some(Member::Workspace(workspace)),
            "seen from the top, the workspace draws it"
        );
        assert_eq!(
            doc.shown(Some(workspace), inside),
            Some(Member::Node(inside))
        );
    }

    /// A workspace that was made and then undone away: its id is a level no longer in the graph.
    fn gone_level(doc: &mut Document) -> Level {
        doc.checkpoint();
        let ws = doc.add_workspace("W", None, origin());
        doc.checkpoint();
        doc.undo();
        assert!(doc.graph().workspace_info(ws).is_none());
        Some(ws)
    }

    fn assert_no_orphans(doc: &Document) {
        for node in doc.graph().nodes().values() {
            if let Some(ws) = node.workspace {
                assert!(doc.graph().workspace_info(ws).is_some(), "orphan in {ws:?}");
            }
        }
    }

    #[test]
    fn a_level_that_is_gone_falls_back_to_the_top() {
        let catalog = catalog();
        let mut doc = doc();
        let gone = gone_level(&mut doc);
        let node = doc.add_node(recipe(), origin(), gone);
        assert_eq!(doc.graph().node(node).unwrap().workspace, None);

        let mut copy = Graph::new("tundra".into());
        copy.add_node(recipe(), origin());
        let pasted = doc.paste(&catalog, &copy, origin(), gone);
        assert_eq!(pasted.len(), 1);
        assert_eq!(
            doc.add_port(gone, Side::Input),
            None,
            "the top has no ports to add"
        );
        assert_eq!(doc.level_path(gone), []);
        assert_eq!(doc.resolve_level(gone), None);
        // (Slot ids are reused once a graph is restored, so this goes last.)
        let workspace = doc.add_workspace("V", gone, origin());
        assert_eq!(doc.graph().workspace_info(workspace).unwrap().parent, None);
        assert_no_orphans(&doc);
    }
}
