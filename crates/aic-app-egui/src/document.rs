//! The egui side of an open graph: [`aic_editor::Document`] (the `Graph`, its solution, undo and
//! save state) plus the egui-snarl view of it.
//!
//! Every edit goes through [`Document`], which changes the graph in the inner document and then
//! brings the view in line ([`Document::reconcile`]): view nodes are added for new members,
//! removed for gone ones and moved when the graph moved them, and wires are added or removed to
//! match the links. Unchanged members keep their snarl `NodeId`, so egui-snarl's state survives.
//! The view shows one level of the graph: the top, or the workspace the player opened. Its payload
//! is a [`Member`] of that level (a node, the level's own ports included, or a workspace inside
//! it), with one wire per link at that level.
//!
//! The canvas can show the graph in several views (ADR 0008). The document holds one view's level
//! at a time; the others wait in a [`LevelView`] and are swapped in to be drawn
//! ([`Document::swap_level`]), drawn afresh if the graph changed meanwhile.

use std::collections::{HashMap, HashSet};
use std::ops::Deref;
use std::path::PathBuf;
use std::time::Duration;

use aic_core::{
    EditError, Graph, LinkError, LinkId, Member, NodeId, NodeKind, Port, PortRef, Pos, Side,
    Solution, WorkspaceId,
};
use aic_data::{Catalog, FacilityId, RegionId};
use eframe::egui::Pos2;
use egui_snarl::{InPinId, OutPinId, Snarl};

pub use aic_editor::document::{Anchor, PinTarget, is_port, pin_targets, pins, port_ref, shown};

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

/// A graph-space point as a graph position.
fn to_pos(pos: Pos2) -> Pos {
    Pos { x: pos.x, y: pos.y }
}

/// One view's level of the graph and the egui-snarl graph drawing it, while another view's is in
/// the [`Document`].
pub struct LevelView {
    view: Snarl<Member>,
    view_ids: HashMap<Member, ViewId>,
    level: Option<WorkspaceId>,
    anchors: HashMap<NodeId, Anchor>,
    shown_pos: HashMap<Member, Pos>,
    /// The document's [`Document::generation`] this was drawn from; `None` before it was drawn.
    synced: Option<u64>,
}

impl LevelView {
    /// A view of `level` (`None` for the top), drawn when it is first swapped in.
    pub fn new(level: Option<WorkspaceId>) -> Self {
        Self {
            view: Snarl::new(),
            view_ids: HashMap::new(),
            level,
            anchors: HashMap::new(),
            shown_pos: HashMap::new(),
            synced: None,
        }
    }

    pub fn level(&self) -> Option<WorkspaceId> {
        self.level
    }
}

pub struct Document {
    inner: aic_editor::Document,
    pub(crate) view: Snarl<Member>,
    view_ids: HashMap<Member, ViewId>,
    /// The workspace the view shows; `None` for the top of the graph.
    level: Option<WorkspaceId>,
    pub(crate) anchors: HashMap<NodeId, Anchor>,
    /// Where the graph had each member the last time the view was brought in line with it. A view
    /// node moves only when the graph's position differs, so a drag the view hasn't handed to the
    /// graph yet isn't undone by an unrelated edit.
    shown_pos: HashMap<Member, Pos>,
}

impl Deref for Document {
    type Target = aic_editor::Document;

    fn deref(&self) -> &aic_editor::Document {
        &self.inner
    }
}

impl Document {
    pub fn new(region: RegionId) -> Self {
        Self::around(aic_editor::Document::new(region))
    }

    /// A document showing `graph`, opened from `path` if it came from a file.
    pub fn from_graph(catalog: &Catalog, graph: Graph, path: Option<PathBuf>) -> Self {
        let mut doc = Self::around(aic_editor::Document::from_graph(graph, path));
        doc.rebuild_view(catalog);
        doc
    }

    /// A document whose view isn't built yet.
    fn around(inner: aic_editor::Document) -> Self {
        Self {
            inner,
            view: Snarl::new(),
            view_ids: HashMap::new(),
            level: None,
            anchors: HashMap::new(),
            shown_pos: HashMap::new(),
        }
    }

    // What changes the save state or the solve, not the view.

    pub fn mark_saved(&mut self, path: PathBuf, draft: bool) {
        self.inner.mark_saved(path, draft);
    }

    pub fn set_draft(&mut self, draft: bool) {
        self.inner.set_draft(draft);
    }

    pub fn checkpoint(&mut self) {
        self.inner.checkpoint();
    }

    /// Re-solves if anything changed since the last solve, and says how long it took.
    pub fn solve_if_stale(&mut self, catalog: &Catalog) -> Option<Duration> {
        self.inner.solve_if_stale(catalog)
    }

    /// Goes back one step. Returns false when there is nothing to undo.
    pub fn undo(&mut self, catalog: &Catalog) -> bool {
        let undone = self.inner.undo();
        if undone {
            self.after_restore(catalog);
        }
        undone
    }

    /// Goes forward one undone step. Returns false when there is nothing to redo.
    pub fn redo(&mut self, catalog: &Catalog) -> bool {
        let redone = self.inner.redo();
        if redone {
            self.after_restore(catalog);
        }
        redone
    }

    /// Switches to another data version or language: recipe nodes are matched against `catalog`
    /// again. Not an edit, so it isn't an undo step itself.
    pub fn rebind(&mut self, catalog: &Catalog) {
        self.inner.rebind(catalog);
        self.rebuild_view(catalog);
    }

    /// A workspace that is gone takes the view back up.
    fn after_restore(&mut self, catalog: &Catalog) {
        while let Some(level) = self.level
            && self.inner.graph().workspace_info(level).is_none()
        {
            self.level = None;
        }
        self.reconcile(catalog);
    }

    /// The workspace the canvas shows; `None` for the top of the graph.
    pub fn level(&self) -> Option<WorkspaceId> {
        self.level
    }

    /// Shows another level of the graph: a workspace, or `None` for the top.
    pub fn set_level(&mut self, catalog: &Catalog, level: Option<WorkspaceId>) {
        if level.is_some_and(|id| self.inner.graph().workspace_info(id).is_none()) {
            return;
        }
        self.level = level;
        self.rebuild_view(catalog);
    }

    /// The workspaces from the top down to the one shown, outermost first.
    pub fn level_path(&self) -> Vec<WorkspaceId> {
        self.inner.level_path(self.level)
    }

    /// Draws the level afresh: one view node per member, and one wire per link between two of
    /// them.
    fn rebuild_view(&mut self, catalog: &Catalog) {
        self.view = Snarl::new();
        self.view_ids.clear();
        self.shown_pos.clear();
        self.anchors.clear();
        self.reconcile(catalog);
    }

    /// Brings the view in line with the graph: view nodes for members the view lacks, none for
    /// members that are gone, moved ones where the graph moved them, and one wire per link at the
    /// level shown.
    pub(crate) fn reconcile(&mut self, catalog: &Catalog) {
        self.reconcile_nodes();
        let wanted: Vec<(OutPinId, InPinId)> = self
            .inner
            .graph()
            .links()
            .keys()
            .filter_map(|link| self.wire(catalog, link))
            .collect();
        let wanted_set: HashSet<_> = wanted.iter().copied().collect();
        let drawn: Vec<(OutPinId, InPinId)> = self.view.wires().collect();
        for (out_pin, in_pin) in &drawn {
            if !wanted_set.contains(&(*out_pin, *in_pin)) {
                self.view.disconnect(*out_pin, *in_pin);
            }
        }
        for (out_pin, in_pin) in wanted {
            self.view.connect(out_pin, in_pin);
        }
    }

    /// The nodes half of [`Self::reconcile`], for edits that draw no wire.
    fn reconcile_nodes(&mut self) {
        let graph = self.inner.graph();
        let members = graph.members_of(self.level);
        let present: HashSet<Member> = members.iter().copied().collect();
        let gone: Vec<Member> = self
            .view_ids
            .keys()
            .filter(|member| !present.contains(member))
            .copied()
            .collect();
        for member in gone {
            if let Some(view_id) = self.view_ids.remove(&member) {
                self.view.remove_node(view_id);
            }
            self.shown_pos.remove(&member);
            if let Member::Node(id) = member {
                self.anchors.remove(&id);
            }
        }
        for member in members {
            let pos = match member {
                Member::Node(id) => graph.nodes()[id].pos,
                Member::Workspace(id) => graph.workspaces()[id].pos,
            };
            match self.view_ids.get(&member) {
                None => {
                    let view_id = self.view.insert_node(to_view(pos), member);
                    self.view_ids.insert(member, view_id);
                    self.shown_pos.insert(member, pos);
                }
                Some(&view_id) => {
                    if self.shown_pos.insert(member, pos) != Some(pos)
                        && let Some(info) = self.view.get_node_info_mut(view_id)
                    {
                        info.pos = to_view(pos);
                    }
                }
            }
        }
    }

    /// Puts `other`'s level in the document and the document's in `other`. The level coming in is
    /// drawn afresh if the graph changed since it was last in, and falls back to the top if its
    /// workspace is gone.
    pub fn swap_level(&mut self, catalog: &Catalog, other: &mut LevelView) {
        std::mem::swap(&mut self.view, &mut other.view);
        std::mem::swap(&mut self.view_ids, &mut other.view_ids);
        std::mem::swap(&mut self.level, &mut other.level);
        std::mem::swap(&mut self.anchors, &mut other.anchors);
        std::mem::swap(&mut self.shown_pos, &mut other.shown_pos);
        let generation = self.inner.generation();
        let synced = other.synced.replace(generation);
        if synced != Some(generation) {
            while let Some(level) = self.level
                && self.inner.graph().workspace_info(level).is_none()
            {
                self.level = None;
            }
            self.rebuild_view(catalog);
        }
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
            self.inner.graph(),
            &mut self.view,
            &mut self.anchors,
            self.inner.solution(),
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
        self.inner.shown(self.level, node)
    }

    /// Moves the graph to another region (see [`Graph::set_region`]).
    pub fn set_region(&mut self, region: RegionId) {
        self.inner.set_region(region);
    }

    /// Adds a node to the level shown, at `pos` in graph space.
    pub fn add_node(&mut self, kind: NodeKind, pos: Pos2) -> NodeId {
        let id = self.inner.add_node(kind, to_pos(pos), self.level);
        self.reconcile_nodes();
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
        let pasted = self.inner.paste(catalog, other, offset, into);
        self.reconcile(catalog);
        pasted
    }

    /// Adds a port to the workspace shown. Ports belong to a workspace, so at the top there is
    /// none to add.
    pub fn add_port(&mut self, catalog: &Catalog, side: Side) -> Option<NodeId> {
        let port = self.inner.add_port(self.level, side)?;
        self.reconcile(catalog);
        Some(port)
    }

    /// Adds an empty workspace to the level shown.
    pub fn add_workspace(&mut self, catalog: &Catalog, name: &str, pos: Pos2) -> WorkspaceId {
        let id = self.inner.add_workspace(name, self.level, to_pos(pos));
        self.reconcile(catalog);
        id
    }

    pub fn remove_node(&mut self, catalog: &Catalog, id: NodeId) {
        self.inner.remove_node(id);
        self.anchors.remove(&id);
        // A removed workspace port rebuilds its workspace card's pins.
        self.reconcile(catalog);
    }

    /// Removes a workspace with everything in it.
    pub fn remove_workspace(&mut self, catalog: &Catalog, id: WorkspaceId) {
        self.inner.remove_workspace(id);
        self.reconcile(catalog);
    }

    /// Puts `members` in a new workspace (see [`Graph::workspace_from`]).
    pub fn workspace_from(
        &mut self,
        catalog: &Catalog,
        members: &[Member],
        name: &str,
    ) -> Result<WorkspaceId, EditError> {
        let workspace = self.inner.workspace_from(members, name)?;
        self.reconcile(catalog);
        Ok(workspace)
    }

    /// Takes a workspace apart (see [`Graph::dissolve`]).
    pub fn dissolve(&mut self, catalog: &Catalog, id: WorkspaceId) -> Result<(), EditError> {
        self.inner.dissolve(id)?;
        self.reconcile(catalog);
        Ok(())
    }

    pub fn rename_workspace(&mut self, id: WorkspaceId, name: &str) -> Result<(), EditError> {
        self.inner.rename_workspace(id, name)
    }

    pub fn connect(
        &mut self,
        catalog: &Catalog,
        from: PortRef,
        to: PortRef,
    ) -> Result<LinkId, LinkError> {
        let link = self.inner.connect(catalog, from, to)?;
        self.reconcile(catalog);
        Ok(link)
    }

    pub fn remove_link(&mut self, catalog: &Catalog, id: LinkId) {
        self.inner.remove_link(id);
        // Another link between the same two ports may still need the wire; reconcile keeps it.
        self.reconcile(catalog);
    }

    pub fn set_limit(&mut self, id: NodeId, limit: Option<f64>) -> Result<(), EditError> {
        self.inner.set_limit(id, limit)
    }

    /// Sets the node's limit as a flow through `port` (see [`Graph::set_port_limit`]).
    pub fn set_port_limit(
        &mut self,
        catalog: &Catalog,
        port: &Port,
        rate: Option<f64>,
    ) -> Result<(), EditError> {
        self.inner.set_port_limit(catalog, port, rate)?;
        self.reconcile(catalog);
        Ok(())
    }

    /// Sets a link's share of the ratio lock at its `side` end (see [`Graph::set_share`]).
    pub fn set_share(
        &mut self,
        link: LinkId,
        side: Side,
        share: Option<f64>,
    ) -> Result<(), EditError> {
        self.inner.set_share(link, side, share)
    }

    /// Removes every link at `port`.
    pub fn clear_port(&mut self, catalog: &Catalog, port: &Port) {
        self.inner.clear_port(port);
        self.reconcile(catalog);
    }

    pub fn set_facility(&mut self, id: NodeId, facility: FacilityId) -> Result<(), EditError> {
        self.inner.set_facility(id, facility)
    }

    /// The link drawn as this wire, if there is one.
    pub fn link_at(&self, catalog: &Catalog, out_pin: OutPinId, in_pin: InPinId) -> Option<LinkId> {
        self.inner
            .graph()
            .links()
            .keys()
            .find(|&id| self.wire(catalog, id) == Some((out_pin, in_pin)))
    }

    /// The wire that draws `link`: from the pin of its port on what shows its source to the pin on
    /// what shows its target. Only a link at the level shown has one.
    pub fn wire(&self, catalog: &Catalog, link: LinkId) -> Option<(OutPinId, InPinId)> {
        let graph = self.inner.graph();
        let link = graph.link(link)?;
        let pin_of = |node: NodeId, side: Side| -> Option<(ViewId, usize)> {
            let member = self.shown(node)?;
            let index = pin_targets(graph, catalog, member, side)
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
        let moves: Vec<(Member, Pos)> = self
            .view
            .nodes_pos_ids()
            .map(|(view_id, pos, &member)| {
                debug_assert_eq!(self.view_ids.get(&member), Some(&view_id));
                (member, to_pos(pos / VIEW_SCALE))
            })
            .collect();
        for (member, pos) in moves {
            // The level's own ports stay on the canvas's edges; their order is their own.
            if matches!(member, Member::Node(id) if is_port(self.inner.graph(), id)) {
                continue;
            }
            self.inner.move_member(member, pos);
            self.shown_pos.insert(member, pos);
        }
        let wires: HashSet<(OutPinId, InPinId)> = self.view.wires().collect();
        let lost: Vec<LinkId> = self
            .inner
            .graph()
            .links()
            .keys()
            .filter(|&id| {
                self.wire(catalog, id)
                    .is_some_and(|wire| !wires.contains(&wire))
            })
            .collect();
        for id in lost {
            self.inner.remove_link(id);
        }
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
    fn a_view_swapped_in_draws_what_changed_while_it_waited() {
        let catalog = catalog();
        let mut doc = Document::new("tundra".into());
        doc.add_node(recipe(), Pos2::ZERO);
        let mut other = LevelView::new(None);
        doc.swap_level(&catalog, &mut other);
        assert_eq!(doc.view().nodes().count(), 1, "drawn on its first swap");
        doc.swap_level(&catalog, &mut other);

        doc.add_node(recipe(), Pos2::ZERO);
        doc.swap_level(&catalog, &mut other);
        assert_eq!(doc.view().nodes().count(), 2);
    }

    #[test]
    fn a_view_whose_workspace_is_gone_shows_the_top() {
        let catalog = catalog();
        let mut doc = Document::new("tundra".into());
        let workspace = doc.add_workspace(&catalog, "W", Pos2::ZERO);
        let mut inside = LevelView::new(Some(workspace));
        doc.remove_workspace(&catalog, workspace);
        doc.swap_level(&catalog, &mut inside);
        assert_eq!(doc.level(), None);
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

    type Wire = (Member, usize, Member, usize);

    /// What a view draws, independent of its snarl ids: each member's position and each wire as the
    /// members and pin indexes at its two ends.
    fn drawn(doc: &Document) -> (Vec<(Member, Pos2)>, Vec<Wire>) {
        let mut nodes: Vec<(Member, Pos2)> = doc
            .view()
            .nodes_pos_ids()
            .map(|(_, pos, &member)| (member, pos))
            .collect();
        nodes.sort_by_key(|(member, _)| format!("{member:?}"));
        let mut wires: Vec<_> = doc
            .view()
            .wires()
            .map(|(out_pin, in_pin)| {
                (
                    doc.view()[out_pin.node],
                    out_pin.output,
                    doc.view()[in_pin.node],
                    in_pin.input,
                )
            })
            .collect();
        wires.sort_by_key(|wire| format!("{wire:?}"));
        (nodes, wires)
    }

    #[test]
    fn reconcile_keeps_view_ids_of_unchanged_members() {
        let catalog = catalog();
        let mut doc = Document::new("tundra".into());
        let furnace = doc.add_node(recipe(), Pos2::new(1.0, 2.0));
        let splitter = doc.add_node(NodeKind::Splitter { item: None }, Pos2::ZERO);
        let before = (
            doc.view_id(Member::Node(furnace)),
            doc.view_id(Member::Node(splitter)),
        );
        let other = doc.add_node(NodeKind::Splitter { item: None }, Pos2::ZERO);
        doc.connect(
            &catalog,
            PortRef::new(furnace, "ingot"),
            PortRef::any(splitter),
        )
        .unwrap();
        doc.remove_node(&catalog, other);
        assert_eq!(
            (
                doc.view_id(Member::Node(furnace)),
                doc.view_id(Member::Node(splitter))
            ),
            before
        );
        assert_eq!(doc.view().wires().count(), 1);
    }

    #[test]
    fn reconcile_after_undo_matches_a_fresh_build() {
        let catalog = catalog();
        let mut doc = Document::new("tundra".into());
        let furnace = doc.add_node(recipe(), Pos2::new(10.0, 20.0));
        let splitter = doc.add_node(NodeKind::Splitter { item: None }, Pos2::new(50.0, 0.0));
        doc.connect(
            &catalog,
            PortRef::new(furnace, "ingot"),
            PortRef::any(splitter),
        )
        .unwrap();
        doc.checkpoint();
        // Edits that change members, positions and links, then undo them.
        doc.remove_node(&catalog, splitter);
        doc.add_node(recipe(), Pos2::new(7.0, 7.0));
        doc.view
            .get_node_info_mut(doc.view_id(Member::Node(furnace)).unwrap())
            .unwrap()
            .pos = Pos2::new(300.0, 300.0);
        doc.sync_from_view(&catalog);
        doc.checkpoint();
        assert!(doc.undo(&catalog));

        let fresh = Document::from_graph(&catalog, doc.graph().clone(), None);
        assert_eq!(drawn(&doc), drawn(&fresh));
    }

    #[test]
    fn a_drag_the_graph_has_not_seen_survives_an_unrelated_edit() {
        let catalog = catalog();
        let mut doc = Document::new("tundra".into());
        let furnace = doc.add_node(recipe(), Pos2::ZERO);
        let view_id = doc.view_id(Member::Node(furnace)).unwrap();
        doc.view.get_node_info_mut(view_id).unwrap().pos = Pos2::new(40.0, 0.0);
        doc.add_node(NodeKind::Splitter { item: None }, Pos2::ZERO);
        doc.sync_from_view(&catalog);
        assert_eq!(
            doc.graph().node(furnace).unwrap().pos,
            Pos { x: 20.0, y: 0.0 }
        );
    }
}
