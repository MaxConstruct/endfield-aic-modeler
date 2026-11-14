//! The egui-snarl view of an open graph. The graph, its solution and its undo history are
//! [`aic_editor::Document`], held by the [`aic_editor::Session`]; a [`DocView`] only keeps a
//! snarl graph drawing one level of it, and brings that in line after the document changed
//! ([`DocView::reconcile`]): view nodes are added for new members, removed for gone ones and moved
//! when the graph moved them, and wires are added or removed to match the links. Unchanged members
//! keep their snarl `NodeId`, so egui-snarl's state survives.
//!
//! The view shows one level of the graph: the top, or the workspace the player opened. Its payload
//! is a [`Member`] of that level (a node, the level's own ports included, or a workspace inside
//! it), with one wire per link at that level.
//!
//! The canvas can show the graph in several views (ADR 0008). The editor holds one view at a time;
//! the others wait parked and are swapped in to be drawn ([`DocView::swap`]), drawn afresh if the
//! graph changed meanwhile.

use std::collections::{HashMap, HashSet};

use aic_core::{Graph, LinkId, Member, NodeId, Pos, Side, Solution, WorkspaceId};
use aic_data::Catalog;
use aic_editor::{Document, Session};
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
pub fn to_pos(pos: Pos2) -> Pos {
    Pos { x: pos.x, y: pos.y }
}

/// One view's level of the graph and the egui-snarl graph drawing it.
pub struct DocView {
    pub(crate) view: Snarl<Member>,
    view_ids: HashMap<Member, ViewId>,
    /// The workspace the view shows; `None` for the top of the graph.
    level: Option<WorkspaceId>,
    pub(crate) anchors: HashMap<NodeId, Anchor>,
    /// Where the graph had each member the last time the view was brought in line with it. A view
    /// node moves only when the graph's position differs, so a drag the view hasn't handed to the
    /// graph yet isn't undone by an unrelated edit.
    shown_pos: HashMap<Member, Pos>,
    /// The document's [`Document::generation`] this was brought in line with; `None` before it
    /// was drawn.
    synced: Option<u64>,
}

impl DocView {
    /// A view of `level` (`None` for the top), drawn when it is first brought in line.
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

    /// The workspace the canvas shows; `None` for the top of the graph.
    pub fn level(&self) -> Option<WorkspaceId> {
        self.level
    }

    /// Shows another level of the graph: a workspace, or `None` for the top.
    pub fn set_level(&mut self, doc: &Document, catalog: &Catalog, level: Option<WorkspaceId>) {
        if level.is_some_and(|id| doc.graph().workspace_info(id).is_none()) {
            return;
        }
        self.level = level;
        self.rebuild(doc, catalog);
    }

    /// Draws the level afresh: one view node per member, and one wire per link between two of
    /// them.
    pub fn rebuild(&mut self, doc: &Document, catalog: &Catalog) {
        self.view = Snarl::new();
        self.view_ids.clear();
        self.shown_pos.clear();
        self.anchors.clear();
        self.reconcile(doc, catalog);
    }

    /// Brings the view in line if the document changed since it last was.
    pub fn sync(&mut self, doc: &Document, catalog: &Catalog) {
        if self.synced != Some(doc.generation()) {
            self.reconcile(doc, catalog);
        }
    }

    /// Brings the view in line with the graph: view nodes for members the view lacks, none for
    /// members that are gone, moved ones where the graph moved them, and one wire per link at the
    /// level shown. A level whose workspace is gone takes the view back up first.
    pub fn reconcile(&mut self, doc: &Document, catalog: &Catalog) {
        while let Some(level) = self.level
            && doc.graph().workspace_info(level).is_none()
        {
            self.level = None;
        }
        self.reconcile_nodes(doc.graph());
        let wanted: Vec<(OutPinId, InPinId)> = doc
            .graph()
            .links()
            .keys()
            .filter_map(|link| self.wire(doc, catalog, link))
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
        self.synced = Some(doc.generation());
    }

    fn reconcile_nodes(&mut self, graph: &Graph) {
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

    /// Puts `other` in `self` and `self` in `other`. The view coming in is drawn afresh if the
    /// graph changed since it was last in line, and falls back to the top if its workspace is
    /// gone.
    pub fn swap(&mut self, doc: &Document, catalog: &Catalog, other: &mut DocView) {
        // The view going out is brought in line first, so it comes back as it left.
        self.sync(doc, catalog);
        std::mem::swap(self, other);
        if self.synced != Some(doc.generation()) {
            self.rebuild(doc, catalog);
        }
    }

    /// The pieces the canvas draws from, borrowed apart so the view can change while the rest is
    /// read.
    pub(crate) fn parts<'a>(
        &'a mut self,
        doc: &'a Document,
    ) -> (
        &'a Graph,
        &'a mut Snarl<Member>,
        &'a mut HashMap<NodeId, Anchor>,
        &'a Solution,
    ) {
        (
            doc.graph(),
            &mut self.view,
            &mut self.anchors,
            doc.solution(),
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
    pub fn shown(&self, doc: &Document, node: NodeId) -> Option<Member> {
        doc.shown(self.level, node)
    }

    /// The link drawn as this wire, if there is one.
    pub fn link_at(
        &self,
        doc: &Document,
        catalog: &Catalog,
        out_pin: OutPinId,
        in_pin: InPinId,
    ) -> Option<LinkId> {
        doc.graph()
            .links()
            .keys()
            .find(|&id| self.wire(doc, catalog, id) == Some((out_pin, in_pin)))
    }

    /// The wire that draws `link`: from the pin of its port on what shows its source to the pin on
    /// what shows its target. Only a link at the level shown has one.
    pub fn wire(
        &self,
        doc: &Document,
        catalog: &Catalog,
        link: LinkId,
    ) -> Option<(OutPinId, InPinId)> {
        let graph = doc.graph();
        let link = graph.link(link)?;
        let pin_of = |node: NodeId, side: Side| -> Option<(ViewId, usize)> {
            let member = self.shown(doc, node)?;
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
    /// egui-snarl's Cmd+drag on a pin ("yank") removes wires without telling the viewer. Expects
    /// the view in line with the document, as [`Self::sync`] leaves it.
    pub fn sync_from_view(&mut self, session: &mut Session) {
        let catalog = session.catalog();
        let doc = session.document();
        let in_line = self.synced == Some(doc.generation());
        let mut moved: Vec<(Member, Pos)> = Vec::new();
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
            if matches!(member, Member::Node(id) if is_port(doc.graph(), id)) {
                continue;
            }
            moved.push((member, pos));
        }
        let wires: HashSet<(OutPinId, InPinId)> = self.view.wires().collect();
        let lost: Vec<LinkId> = doc
            .graph()
            .links()
            .keys()
            .filter(|&id| {
                self.wire(doc, catalog, id)
                    .is_some_and(|wire| !wires.contains(&wire))
            })
            .collect();
        for (member, pos) in moved {
            session.move_member(member, pos);
            self.shown_pos.insert(member, pos);
        }
        for id in lost {
            session.disconnect(id);
        }
        // What the view handed over is already drawn.
        if in_line {
            self.synced = Some(session.document().generation());
        }
    }
}

/// The open document and the view drawing it, read-only: what the tests and the canvas look at.
/// Its methods hand out references that outlive the value itself.
#[derive(Clone, Copy)]
pub struct DocumentRef<'a> {
    pub(crate) doc: &'a Document,
    pub(crate) view: &'a DocView,
}

impl<'a> DocumentRef<'a> {
    pub fn graph(&self) -> &'a Graph {
        self.doc.graph()
    }

    pub fn solution(&self) -> &'a Solution {
        self.doc.solution()
    }

    pub fn path(&self) -> Option<&'a std::path::Path> {
        self.doc.path()
    }

    pub fn is_draft(&self) -> bool {
        self.doc.is_draft()
    }

    pub fn is_modified(&self) -> bool {
        self.doc.is_modified()
    }

    /// The canvas's nodes and wires, at view positions ([`VIEW_SCALE`]).
    pub fn view(&self) -> &'a Snarl<Member> {
        self.view.view()
    }

    /// The workspace the canvas shows; `None` for the top of the graph.
    pub fn level(&self) -> Option<WorkspaceId> {
        self.view.level()
    }
}

/// The open document for edits that keep the view in line, as the editor makes them.
pub struct DocumentMut<'a> {
    pub(crate) session: &'a mut aic_editor::Session,
    pub(crate) view: &'a mut DocView,
}

impl DocumentMut<'_> {
    fn sync(&mut self) {
        self.view
            .sync(self.session.document(), self.session.catalog());
    }

    /// Adds a node to the level shown, at `pos` in graph space.
    pub fn add_node(&mut self, kind: aic_core::NodeKind, pos: Pos2) -> NodeId {
        let id = self.session.add_node(kind, to_pos(pos), self.view.level());
        self.sync();
        id
    }

    pub fn graph(&self) -> &Graph {
        self.session.document().graph()
    }

    pub fn set_limit(
        &mut self,
        node: NodeId,
        limit: Option<f64>,
    ) -> Result<(), aic_core::EditError> {
        self.session.set_limit(node, limit)
    }

    pub fn remove_node(&mut self, _catalog: &Catalog, node: NodeId) {
        self.session.remove_node(node);
        self.sync();
    }

    pub fn connect(
        &mut self,
        _catalog: &Catalog,
        from: aic_core::PortRef,
        to: aic_core::PortRef,
    ) -> Result<LinkId, aic_core::LinkError> {
        let link = self.session.try_connect(from, to);
        self.sync();
        link
    }
}

#[cfg(test)]
mod tests {
    use aic_core::{NodeKind, PortRef};

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

    /// A session with the view of its top level, edited the way the editor does: edit the
    /// session, then bring the view in line.
    struct Rig {
        session: Session,
        view: DocView,
    }

    impl Rig {
        fn new() -> Self {
            let session = Session::new(catalog(), aic_editor::settings::Availability::default());
            let mut view = DocView::new(None);
            view.sync(session.document(), session.catalog());
            Self { session, view }
        }

        fn doc(&self) -> &Document {
            self.session.document()
        }

        fn catalog(&self) -> &Catalog {
            self.session.catalog()
        }

        fn sync(&mut self) {
            self.view
                .sync(self.session.document(), self.session.catalog());
        }

        fn add_node(&mut self, kind: NodeKind, pos: Pos2) -> NodeId {
            let id = self.session.add_node(kind, to_pos(pos), self.view.level());
            self.sync();
            id
        }

        fn connect(&mut self, from: PortRef, to: PortRef) -> LinkId {
            let link = self.session.try_connect(from, to).unwrap();
            self.sync();
            link
        }

        fn wire(&self, link: LinkId) -> Option<(OutPinId, InPinId)> {
            self.view.wire(self.doc(), self.catalog(), link)
        }

        fn sync_from_view(&mut self) {
            self.view.sync_from_view(&mut self.session);
        }

        fn swap(&mut self, other: &mut DocView) {
            self.view
                .swap(self.session.document(), self.session.catalog(), other);
        }
    }

    #[test]
    fn a_view_swapped_in_draws_what_changed_while_it_waited() {
        let mut rig = Rig::new();
        rig.add_node(recipe(), Pos2::ZERO);
        let mut other = DocView::new(None);
        rig.swap(&mut other);
        assert_eq!(
            rig.view.view().nodes().count(),
            1,
            "drawn on its first swap"
        );
        rig.swap(&mut other);

        rig.add_node(recipe(), Pos2::ZERO);
        rig.swap(&mut other);
        assert_eq!(rig.view.view().nodes().count(), 2);
    }

    #[test]
    fn a_view_parked_after_an_edit_keeps_its_ids_when_it_comes_back() {
        let mut rig = Rig::new();
        let a = rig.add_node(recipe(), Pos2::ZERO);
        let b = rig.add_node(recipe(), Pos2::ZERO);
        let before = (
            rig.view.view_id(Member::Node(a)),
            rig.view.view_id(Member::Node(b)),
        );
        let mut other = DocView::new(None);
        rig.swap(&mut other);
        rig.swap(&mut other);

        // An edit no one synced the view after, then the view is parked and brought back.
        rig.session.set_limit(a, Some(2.0)).unwrap();
        rig.swap(&mut other);
        rig.swap(&mut other);
        assert_eq!(
            (
                rig.view.view_id(Member::Node(a)),
                rig.view.view_id(Member::Node(b))
            ),
            before
        );
    }

    #[test]
    fn a_view_whose_workspace_is_gone_shows_the_top() {
        let mut rig = Rig::new();
        let workspace = rig.session.add_workspace(Pos::default(), None);
        rig.sync();
        let mut inside = DocView::new(Some(workspace));
        rig.session.remove_workspace(workspace);
        rig.sync();
        rig.swap(&mut inside);
        assert_eq!(rig.view.level(), None);
    }

    #[test]
    fn links_and_wires_stay_in_step() {
        let mut rig = Rig::new();
        let furnace = rig.add_node(recipe(), Pos2::ZERO);
        let splitter = rig.add_node(NodeKind::Splitter { item: None }, Pos2::ZERO);
        let link = rig.connect(PortRef::new(furnace, "ingot"), PortRef::any(splitter));
        let wire = rig.wire(link).unwrap();
        assert_eq!(rig.view.view().wires().collect::<Vec<_>>(), [wire]);
        assert_eq!(
            rig.view.link_at(rig.doc(), rig.catalog(), wire.0, wire.1),
            Some(link)
        );

        rig.session.disconnect(link);
        rig.sync();
        assert_eq!(rig.view.view().wires().count(), 0);
        assert_eq!(rig.doc().graph().links().len(), 0);
    }

    #[test]
    fn a_wire_lost_in_the_view_drops_its_link() {
        let mut rig = Rig::new();
        let furnace = rig.add_node(recipe(), Pos2::ZERO);
        let splitter = rig.add_node(NodeKind::Splitter { item: None }, Pos2::ZERO);
        let link = rig.connect(PortRef::new(furnace, "ingot"), PortRef::any(splitter));
        let (out_pin, _) = rig.wire(link).unwrap();
        rig.view.view.drop_outputs(out_pin);

        rig.sync_from_view();
        assert_eq!(rig.doc().graph().links().len(), 0);
    }

    #[test]
    fn removing_a_node_removes_it_from_the_view() {
        let mut rig = Rig::new();
        let furnace = rig.add_node(recipe(), Pos2::new(10.0, 20.0));
        rig.session.remove_node(furnace);
        rig.sync();
        assert_eq!(rig.view.view().nodes().count(), 0);
    }

    #[test]
    fn undo_brings_back_links_and_their_wires() {
        let mut rig = Rig::new();
        let furnace = rig.add_node(recipe(), Pos2::ZERO);
        let splitter = rig.add_node(NodeKind::Splitter { item: None }, Pos2::ZERO);
        rig.connect(PortRef::new(furnace, "ingot"), PortRef::any(splitter));
        rig.session.checkpoint();
        rig.session.remove_node(splitter);
        rig.sync();
        rig.session.checkpoint();

        rig.session.undo();
        rig.sync();
        assert_eq!(rig.doc().graph().links().len(), 1);
        assert_eq!(rig.view.view().wires().count(), 1);
    }

    #[test]
    fn a_view_of_a_document_from_a_graph_draws_its_links() {
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
        let doc = Document::from_graph(graph, Some("plan.aicm".into()));
        let mut view = DocView::new(None);
        view.sync(&doc, &catalog);
        assert_eq!(view.view().wires().count(), 1);
        assert_eq!(
            view.view()[view.view_id(Member::Node(furnace)).unwrap()],
            Member::Node(furnace)
        );
        assert!(!doc.is_modified());
    }

    type Wire = (Member, usize, Member, usize);

    /// What a view draws, independent of its snarl ids: each member's position and each wire as the
    /// members and pin indexes at its two ends.
    fn drawn(view: &DocView) -> (Vec<(Member, Pos2)>, Vec<Wire>) {
        let mut nodes: Vec<(Member, Pos2)> = view
            .view()
            .nodes_pos_ids()
            .map(|(_, pos, &member)| (member, pos))
            .collect();
        nodes.sort_by_key(|(member, _)| format!("{member:?}"));
        let mut wires: Vec<_> = view
            .view()
            .wires()
            .map(|(out_pin, in_pin)| {
                (
                    view.view()[out_pin.node],
                    out_pin.output,
                    view.view()[in_pin.node],
                    in_pin.input,
                )
            })
            .collect();
        wires.sort_by_key(|wire| format!("{wire:?}"));
        (nodes, wires)
    }

    #[test]
    fn reconcile_keeps_view_ids_of_unchanged_members() {
        let mut rig = Rig::new();
        let furnace = rig.add_node(recipe(), Pos2::new(1.0, 2.0));
        let splitter = rig.add_node(NodeKind::Splitter { item: None }, Pos2::ZERO);
        let before = (
            rig.view.view_id(Member::Node(furnace)),
            rig.view.view_id(Member::Node(splitter)),
        );
        let other = rig.add_node(NodeKind::Splitter { item: None }, Pos2::ZERO);
        rig.connect(PortRef::new(furnace, "ingot"), PortRef::any(splitter));
        rig.session.remove_node(other);
        rig.sync();
        assert_eq!(
            (
                rig.view.view_id(Member::Node(furnace)),
                rig.view.view_id(Member::Node(splitter))
            ),
            before
        );
        assert_eq!(rig.view.view().wires().count(), 1);
    }

    #[test]
    fn reconcile_after_undo_matches_a_fresh_build() {
        let mut rig = Rig::new();
        let furnace = rig.add_node(recipe(), Pos2::new(10.0, 20.0));
        let splitter = rig.add_node(NodeKind::Splitter { item: None }, Pos2::new(50.0, 0.0));
        rig.connect(PortRef::new(furnace, "ingot"), PortRef::any(splitter));
        rig.session.checkpoint();
        // Edits that change members, positions and links, then undo them.
        rig.session.remove_node(splitter);
        rig.add_node(recipe(), Pos2::new(7.0, 7.0));
        let view_id = rig.view.view_id(Member::Node(furnace)).unwrap();
        rig.view.view.get_node_info_mut(view_id).unwrap().pos = Pos2::new(300.0, 300.0);
        rig.sync();
        rig.sync_from_view();
        rig.session.checkpoint();
        assert!(rig.session.undo());
        rig.sync();

        let mut fresh = DocView::new(None);
        fresh.sync(rig.doc(), rig.catalog());
        assert_eq!(drawn(&rig.view), drawn(&fresh));
    }

    #[test]
    fn a_drag_the_graph_has_not_seen_survives_an_unrelated_edit() {
        let mut rig = Rig::new();
        let furnace = rig.add_node(recipe(), Pos2::ZERO);
        let view_id = rig.view.view_id(Member::Node(furnace)).unwrap();
        rig.view.view.get_node_info_mut(view_id).unwrap().pos = Pos2::new(40.0, 0.0);
        rig.add_node(NodeKind::Splitter { item: None }, Pos2::ZERO);
        rig.sync_from_view();
        assert_eq!(
            rig.doc().graph().node(furnace).unwrap().pos,
            Pos { x: 20.0, y: 0.0 }
        );
    }
}
