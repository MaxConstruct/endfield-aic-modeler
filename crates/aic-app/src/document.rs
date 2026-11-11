//! One open graph: the `aic-core` [`Graph`], which is the source of truth, the egui-snarl view of it,
//! and its latest [`Solution`].
//!
//! Every edit goes through [`Document`], which changes the graph and the view together. The view
//! shows one level of the graph: the top, or the workspace the player opened. Its payload is a
//! [`Member`] of that level (a node, the level's own ports included, or a workspace inside it),
//! with one wire per link at that level.
//!
//! Undo and redo keep whole `Graph` snapshots: graphs are small, so a clone per step is cheap.
//! Edits since the last [`Document::checkpoint`] make one step, so a drag or a typed number undoes
//! in one go.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use web_time::Instant;

use aic_core::{
    EditError, Graph, LinkError, LinkId, Member, NodeId, NodeKind, Port, PortRef, Pos, Side,
    Solution, WorkspaceId, solve,
};
use aic_data::{Catalog, FacilityId, ItemId, RegionId};
use eframe::egui::Pos2;
use egui_snarl::{InPinId, OutPinId, Snarl};

/// A node's id in the egui-snarl view. Not stable: snarl reuses it after a removal.
pub type ViewId = egui_snarl::NodeId;

/// How much bigger the view's space is than the graph's. The canvas lays its cards out at its
/// largest zoom and shrinks the picture to the zoom shown, so text stays sharp up to that zoom (see
/// `canvas::MAX_ZOOM`); positions in the view are graph positions times this. Files, the graph and
/// everything outside the canvas keep graph positions.
pub const VIEW_SCALE: f32 = 2.0;

/// A graph position as a view position.
fn to_view(pos: Pos) -> Pos2 {
    Pos2::new(pos.x, pos.y) * VIEW_SCALE
}

/// Where a new node's edge should land once its size is known: its right edge for a node that
/// feeds the port it was added from, its left edge for one that the port feeds. In graph space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Anchor {
    Left(f32),
    Right(f32),
}

/// How many undo steps are kept.
const HISTORY_LIMIT: usize = 200;

pub struct Document {
    pub(crate) graph: Graph,
    pub(crate) view: Snarl<Member>,
    view_ids: HashMap<Member, ViewId>,
    /// The workspace the view shows; `None` for the top of the graph.
    level: Option<WorkspaceId>,
    pub(crate) anchors: HashMap<NodeId, Anchor>,
    solution: Solution,
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
    saved_edits: u64,
    path: Option<PathBuf>,
    /// The file is a draft in the app's own folder, not one the player chose.
    draft: bool,
}

impl Document {
    pub fn new(region: RegionId) -> Self {
        Self::bare(Graph::new(region), None)
    }

    /// A document showing `graph`, opened from `path` if it came from a file.
    pub fn from_graph(catalog: &Catalog, graph: Graph, path: Option<PathBuf>) -> Self {
        let mut doc = Self::bare(graph, path);
        doc.rebuild_view(catalog);
        doc
    }

    /// A document whose view isn't built yet.
    fn bare(graph: Graph, path: Option<PathBuf>) -> Self {
        Self {
            committed: graph.clone(),
            graph,
            view: Snarl::new(),
            view_ids: HashMap::new(),
            level: None,
            anchors: HashMap::new(),
            solution: Solution::default(),
            stale: true,
            revision: 0,
            pending: false,
            undo: Vec::new(),
            redo: Vec::new(),
            next_revision: 1,
            edits: 0,
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
    pub fn undo(&mut self, catalog: &Catalog) -> bool {
        self.checkpoint();
        let Some((graph, revision)) = self.undo.pop() else {
            return false;
        };
        let current = std::mem::replace(&mut self.committed, graph);
        self.redo.push((current, self.revision));
        self.revision = revision;
        self.restore(catalog);
        true
    }

    /// Goes forward one undone step. Returns false when there is nothing to redo.
    pub fn redo(&mut self, catalog: &Catalog) -> bool {
        if self.pending {
            return false;
        }
        let Some((graph, revision)) = self.redo.pop() else {
            return false;
        };
        let current = std::mem::replace(&mut self.committed, graph);
        self.undo.push((current, self.revision));
        self.revision = revision;
        self.restore(catalog);
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
        self.rebuild_view(catalog);
        self.stale = true;
    }

    /// Shows the committed graph again. A workspace that is gone takes the view back up.
    fn restore(&mut self, catalog: &Catalog) {
        self.edits += 1;
        self.graph = self.committed.clone();
        while let Some(level) = self.level
            && self.graph.workspace_info(level).is_none()
        {
            self.level = None;
        }
        self.rebuild_view(catalog);
        self.stale = true;
    }

    /// The workspace the canvas shows; `None` for the top of the graph.
    pub fn level(&self) -> Option<WorkspaceId> {
        self.level
    }

    /// Shows another level of the graph: a workspace, or `None` for the top.
    pub fn set_level(&mut self, catalog: &Catalog, level: Option<WorkspaceId>) {
        if level.is_some_and(|id| self.graph.workspace_info(id).is_none()) {
            return;
        }
        self.level = level;
        self.rebuild_view(catalog);
    }

    /// The workspaces from the top down to the one shown, outermost first.
    pub fn level_path(&self) -> Vec<WorkspaceId> {
        let mut path = Vec::new();
        let mut current = self.level;
        while let Some(id) = current {
            path.push(id);
            current = self.graph.workspace_info(id).and_then(|w| w.parent);
        }
        path.reverse();
        path
    }

    /// Draws the level afresh: one view node per member, and one wire per link between two of
    /// them.
    fn rebuild_view(&mut self, catalog: &Catalog) {
        self.view = Snarl::new();
        self.view_ids.clear();
        self.anchors.clear();
        for member in self.graph.members_of(self.level) {
            let pos = match member {
                Member::Node(id) => self.graph.nodes()[id].pos,
                Member::Workspace(id) => self.graph.workspaces()[id].pos,
            };
            let view_id = self.view.insert_node(to_view(pos), member);
            self.view_ids.insert(member, view_id);
        }
        let links: Vec<LinkId> = self.graph.links().keys().collect();
        for link in links {
            if let Some((out_pin, in_pin)) = self.wire(catalog, link) {
                self.view.connect(out_pin, in_pin);
            }
        }
    }

    /// Marks the graph as changed: it needs a solve and makes an undo step.
    fn changed(&mut self) {
        self.stale = true;
        self.pending = true;
        self.edits += 1;
    }

    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    pub fn solution(&self) -> &Solution {
        &self.solution
    }

    /// The pieces the canvas draws from, borrowed apart so the view can change while the rest is
    /// read.
    pub(crate) fn parts(
        &mut self,
    ) -> (
        &Graph,
        &mut Snarl<Member>,
        &mut HashMap<NodeId, Anchor>,
        &Solution,
    ) {
        (
            &self.graph,
            &mut self.view,
            &mut self.anchors,
            &self.solution,
        )
    }

    /// The canvas's nodes and wires, at view positions ([`VIEW_SCALE`]).
    pub fn view(&self) -> &Snarl<Member> {
        &self.view
    }

    /// The view node drawing a node or workspace, if it is drawn.
    pub fn view_id(&self, member: Member) -> Option<ViewId> {
        self.view_ids.get(&member).copied()
    }

    /// What draws `node` at the level shown: the node itself, or the workspace it is in. `None`
    /// when it is outside the level.
    pub fn shown(&self, node: NodeId) -> Option<Member> {
        shown(&self.graph, self.level, node)
    }

    /// Moves an empty graph to another region (see [`Graph::set_region`]).
    pub fn set_region(&mut self, region: RegionId) -> Result<(), EditError> {
        self.graph.set_region(region)?;
        self.changed();
        Ok(())
    }

    /// Adds a node to the level shown, at `pos` in graph space.
    pub fn add_node(&mut self, kind: NodeKind, pos: Pos2) -> NodeId {
        let pos = Pos { x: pos.x, y: pos.y };
        let id = self
            .graph
            .add_node_in(kind, pos, self.level)
            .expect("the level shown exists");
        let view_id = self.view.insert_node(to_view(pos), Member::Node(id));
        self.view_ids.insert(Member::Node(id), view_id);
        self.changed();
        id
    }

    /// Adds a node whose edge lands on `anchor` once the canvas has measured it.
    pub fn add_node_anchored(&mut self, kind: NodeKind, pos: Pos2, anchor: Anchor) -> NodeId {
        let id = self.add_node(kind, pos);
        self.anchors.insert(id, anchor);
        id
    }

    /// Pastes `other` into `into` (the level shown, or a workspace in it), moved by `offset`
    /// (see [`Graph::paste_into`]).
    pub fn paste(
        &mut self,
        catalog: &Catalog,
        other: &Graph,
        offset: Pos,
        into: Option<WorkspaceId>,
    ) -> Vec<Member> {
        let pasted = self.graph.paste_into(catalog, other, offset, into);
        self.rebuild_view(catalog);
        self.changed();
        pasted
    }

    /// Adds a port to the workspace shown. Ports belong to a workspace, so at the top there is
    /// none to add.
    pub fn add_port(&mut self, catalog: &Catalog, side: Side) -> Option<NodeId> {
        let port = self.graph.add_port(self.level?, side).ok()?;
        self.rebuild_view(catalog);
        self.changed();
        Some(port)
    }

    /// Adds an empty workspace to the level shown.
    pub fn add_workspace(&mut self, catalog: &Catalog, name: &str, pos: Pos2) -> WorkspaceId {
        let id = self
            .graph
            .add_workspace(name, self.level, Pos { x: pos.x, y: pos.y })
            .expect("the level shown exists");
        self.rebuild_view(catalog);
        self.changed();
        id
    }

    pub fn remove_node(&mut self, catalog: &Catalog, id: NodeId) {
        let is_port = self
            .graph
            .node(id)
            .is_some_and(|n| matches!(n.kind, NodeKind::WorkspacePort { .. }));
        if self.graph.remove_node(id).is_some() {
            if is_port {
                // A workspace card shown here loses a pin.
                self.rebuild_view(catalog);
            } else if let Some(view_id) = self.view_ids.remove(&Member::Node(id)) {
                self.view.remove_node(view_id);
            }
        }
        self.anchors.remove(&id);
        self.changed();
    }

    /// Removes a workspace with everything in it.
    pub fn remove_workspace(&mut self, catalog: &Catalog, id: WorkspaceId) {
        self.graph.remove_workspace(id);
        self.rebuild_view(catalog);
        self.changed();
    }

    /// Puts `members` in a new workspace (see [`Graph::workspace_from`]).
    pub fn workspace_from(
        &mut self,
        catalog: &Catalog,
        members: &[Member],
        name: &str,
    ) -> Result<WorkspaceId, EditError> {
        let workspace = self.graph.workspace_from(members, name)?;
        self.rebuild_view(catalog);
        self.changed();
        Ok(workspace)
    }

    /// Takes a workspace apart (see [`Graph::dissolve`]).
    pub fn dissolve(&mut self, catalog: &Catalog, id: WorkspaceId) -> Result<(), EditError> {
        self.graph.dissolve(id)?;
        self.rebuild_view(catalog);
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
        if let Some((out_pin, in_pin)) = self.wire(catalog, link) {
            self.view.connect(out_pin, in_pin);
        }
        self.changed();
        Ok(link)
    }

    pub fn remove_link(&mut self, catalog: &Catalog, id: LinkId) {
        let wire = self.wire(catalog, id);
        self.graph.remove_link(id);
        if let Some((out_pin, in_pin)) = wire {
            // Another link between the same two ports may still need the wire.
            let still_drawn = self
                .graph
                .links()
                .keys()
                .any(|other| self.wire(catalog, other) == Some((out_pin, in_pin)));
            if !still_drawn {
                self.view.disconnect(out_pin, in_pin);
            }
        }
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
    pub fn clear_port(&mut self, catalog: &Catalog, port: &Port) {
        let links: Vec<LinkId> = self
            .graph
            .links_at(port.node, port.side)
            .filter(|(_, link)| link.item == port.item)
            .map(|(id, _)| id)
            .collect();
        for link in links {
            self.remove_link(catalog, link);
        }
    }

    pub fn set_facility(&mut self, id: NodeId, facility: FacilityId) -> Result<(), EditError> {
        self.graph.set_facility(id, facility)?;
        self.changed();
        Ok(())
    }

    /// The link drawn as this wire, if there is one.
    pub fn link_at(&self, catalog: &Catalog, out_pin: OutPinId, in_pin: InPinId) -> Option<LinkId> {
        self.graph
            .links()
            .keys()
            .find(|&id| self.wire(catalog, id) == Some((out_pin, in_pin)))
    }

    /// The wire that draws `link`: from the pin of its port on what shows its source to the pin on
    /// what shows its target. Only a link at the level shown has one.
    pub fn wire(&self, catalog: &Catalog, link: LinkId) -> Option<(OutPinId, InPinId)> {
        let link = self.graph.link(link)?;
        let pin_of = |node: NodeId, side: Side| -> Option<(ViewId, usize)> {
            let member = self.shown(node)?;
            let index = pin_targets(&self.graph, catalog, member, side)
                .iter()
                .position(|target| target.is(node, &link.item))?;
            Some((self.view_id(member)?, index))
        };
        let (from, output) = pin_of(link.from, Side::Output)?;
        let (to, input) = pin_of(link.to, Side::Input)?;
        Some((OutPinId { node: from, output }, InPinId { node: to, input }))
    }

    /// Copies moves from the view into the graph, and drops links whose wire the view lost.
    /// egui-snarl's Cmd+drag on a pin ("yank") removes wires without telling the viewer.
    pub fn sync_from_view(&mut self, catalog: &Catalog) {
        for (view_id, pos, &member) in self.view.nodes_pos_ids() {
            debug_assert_eq!(self.view_ids.get(&member), Some(&view_id));
            let pos = pos / VIEW_SCALE;
            let new = Pos { x: pos.x, y: pos.y };
            // Position doesn't affect the solve, so a move doesn't make the solution stale.
            match member {
                // The level's own ports stay on the canvas's edges; their order is their own.
                Member::Node(id) if is_port(&self.graph, id) => {}
                Member::Node(id) => {
                    if self.graph.node(id).is_some_and(|node| node.pos != new) {
                        let _ = self.graph.set_pos(id, new);
                        self.pending = true;
                        self.edits += 1;
                    }
                }
                Member::Workspace(id) => {
                    if self.graph.workspace_info(id).is_some_and(|w| w.pos != new) {
                        let _ = self.graph.set_workspace_pos(id, new);
                        self.pending = true;
                        self.edits += 1;
                    }
                }
            }
        }
        let wires: HashSet<(OutPinId, InPinId)> = self.view.wires().collect();
        let lost: Vec<LinkId> = self
            .graph
            .links()
            .keys()
            .filter(|&id| {
                self.wire(catalog, id)
                    .is_some_and(|wire| !wires.contains(&wire))
            })
            .collect();
        for id in lost {
            self.graph.remove_link(id);
            self.changed();
        }
    }

    /// Re-solves if anything changed since the last solve, and says how long it took.
    pub fn solve_if_stale(&mut self, catalog: &Catalog) -> Option<Duration> {
        if !self.stale {
            return None;
        }
        let start = Instant::now();
        self.solution = solve(catalog, &self.graph);
        self.stale = false;
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

    fn catalog() -> Catalog {
        let data = r#"{
            "version": { "arknights-endfield": "test" },
            "locations": [{ "id": "tundra", "name": "tundra" }],
            "items": [
                { "id": "belt", "name": "belt", "category": "belt-and-pipe", "belt": { "speed": 0.5 } },
                { "id": "pipe", "name": "pipe", "category": "belt-and-pipe", "pipe": { "speed": 2 } },
                { "id": "ore", "name": "ore", "category": "material" },
                { "id": "ingot", "name": "ingot", "category": "material" },
                { "id": "furnace", "name": "furnace", "category": "machine",
                  "machine": { "speed": 1, "type": "electric", "usage": 5 } }
            ],
            "recipes": [
                { "id": "ingot", "name": "ingot", "category": "material", "time": 2,
                  "in": { "ore": 1 }, "out": { "ingot": 1 }, "producers": ["furnace"] }
            ]
        }"#;
        Catalog::from_bytes(data.as_bytes(), b"{}", None).expect("test catalog loads")
    }

    fn recipe() -> NodeKind {
        NodeKind::Recipe {
            recipe: "ingot".into(),
            facility: "furnace".into(),
            limit: Some(1.0),
        }
    }

    #[test]
    fn links_and_wires_stay_in_step() {
        let catalog = catalog();
        let mut doc = Document::new("tundra".into());
        let furnace = doc.add_node(recipe(), Pos2::ZERO);
        let splitter = doc.add_node(NodeKind::Splitter { item: None }, Pos2::ZERO);
        let link = doc
            .connect(
                &catalog,
                PortRef::new(furnace, "ingot"),
                PortRef::any(splitter),
            )
            .unwrap();
        let wire = doc.wire(&catalog, link).unwrap();
        assert_eq!(doc.view.wires().collect::<Vec<_>>(), [wire]);
        assert_eq!(doc.link_at(&catalog, wire.0, wire.1), Some(link));

        doc.remove_link(&catalog, link);
        assert_eq!(doc.view.wires().count(), 0);
        assert_eq!(doc.graph().links().len(), 0);
    }

    #[test]
    fn a_wire_lost_in_the_view_drops_its_link() {
        let catalog = catalog();
        let mut doc = Document::new("tundra".into());
        let furnace = doc.add_node(recipe(), Pos2::ZERO);
        let splitter = doc.add_node(NodeKind::Splitter { item: None }, Pos2::ZERO);
        let link = doc
            .connect(
                &catalog,
                PortRef::new(furnace, "ingot"),
                PortRef::any(splitter),
            )
            .unwrap();
        let (out_pin, _) = doc.wire(&catalog, link).unwrap();
        doc.view.drop_outputs(out_pin);

        doc.sync_from_view(&catalog);
        assert_eq!(doc.graph().links().len(), 0);
    }

    #[test]
    fn removing_a_node_removes_it_from_the_view() {
        let catalog = catalog();
        let mut doc = Document::new("tundra".into());
        let furnace = doc.add_node(recipe(), Pos2::new(10.0, 20.0));
        doc.solve_if_stale(&catalog);
        assert!(doc.solve_if_stale(&catalog).is_none(), "nothing changed");

        doc.remove_node(&catalog, furnace);
        assert_eq!(doc.view.nodes().count(), 0);
        assert!(doc.solve_if_stale(&catalog).is_some(), "removal re-solves");
    }

    #[test]
    fn edits_between_checkpoints_undo_as_one_step() {
        let catalog = catalog();
        let mut doc = Document::new("tundra".into());
        let furnace = doc.add_node(recipe(), Pos2::ZERO);
        doc.checkpoint();
        doc.set_limit(furnace, Some(2.0)).unwrap();
        doc.set_limit(furnace, Some(3.0)).unwrap();
        doc.checkpoint();

        assert!(doc.undo(&catalog));
        assert_eq!(doc.graph().node(furnace).unwrap().kind.limit(), Some(1.0));
        assert!(doc.undo(&catalog));
        assert!(doc.graph().nodes().is_empty());
        assert!(!doc.undo(&catalog), "nothing left to undo");

        assert!(doc.redo(&catalog));
        assert!(doc.redo(&catalog));
        assert_eq!(doc.graph().node(furnace).unwrap().kind.limit(), Some(3.0));
        assert_eq!(doc.view.nodes().count(), 1, "the view follows");
    }

    #[test]
    fn undo_brings_back_links_and_their_wires() {
        let catalog = catalog();
        let mut doc = Document::new("tundra".into());
        let furnace = doc.add_node(recipe(), Pos2::ZERO);
        let splitter = doc.add_node(NodeKind::Splitter { item: None }, Pos2::ZERO);
        doc.connect(
            &catalog,
            PortRef::new(furnace, "ingot"),
            PortRef::any(splitter),
        )
        .unwrap();
        doc.checkpoint();
        doc.remove_node(&catalog, splitter);
        doc.checkpoint();

        doc.undo(&catalog);
        assert_eq!(doc.graph().links().len(), 1);
        assert_eq!(doc.view.wires().count(), 1);
    }

    #[test]
    fn a_new_edit_drops_the_redo_steps() {
        let catalog = catalog();
        let mut doc = Document::new("tundra".into());
        doc.add_node(recipe(), Pos2::ZERO);
        doc.checkpoint();
        doc.undo(&catalog);
        doc.add_node(NodeKind::Splitter { item: None }, Pos2::ZERO);
        doc.checkpoint();
        assert!(!doc.redo(&catalog));
    }

    #[test]
    fn every_change_after_a_save_needs_saving_undo_included() {
        let catalog = catalog();
        let mut doc = Document::new("tundra".into());
        assert!(!doc.is_modified());
        doc.add_node(recipe(), Pos2::ZERO);
        assert!(doc.is_modified());
        doc.mark_saved("plan.aicm".into(), false);
        assert!(!doc.is_modified());
        doc.checkpoint();
        doc.undo(&catalog);
        assert!(doc.is_modified(), "the file still has the node");
    }

    #[test]
    fn saving_mid_edit_keeps_the_edit_one_undo_step() {
        let catalog = catalog();
        let mut doc = Document::new("tundra".into());
        let furnace = doc.add_node(recipe(), Pos2::ZERO);
        doc.checkpoint();
        doc.set_limit(furnace, Some(2.0)).unwrap();
        doc.mark_saved("plan.aicm".into(), false);
        doc.set_limit(furnace, Some(3.0)).unwrap();
        doc.checkpoint();
        doc.undo(&catalog);
        assert_eq!(doc.graph().node(furnace).unwrap().kind.limit(), Some(1.0));
    }

    #[test]
    fn a_document_from_a_graph_draws_its_links() {
        let catalog = catalog();
        let mut graph = Graph::new("tundra".into());
        let furnace = graph.add_node(recipe(), Pos { x: 5.0, y: 6.0 });
        let splitter = graph.add_node(NodeKind::Splitter { item: None }, Pos::default());
        graph
            .connect(
                &catalog,
                PortRef::new(furnace, "ingot"),
                PortRef::any(splitter),
            )
            .unwrap();
        let doc = Document::from_graph(&catalog, graph, Some("plan.aicm".into()));
        assert_eq!(doc.view.wires().count(), 1);
        assert_eq!(
            doc.view[doc.view_id(Member::Node(furnace)).unwrap()],
            Member::Node(furnace)
        );
        assert!(!doc.is_modified());
    }
}
