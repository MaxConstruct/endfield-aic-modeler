//! What the canvas draws of an open graph. The graph, its solution and its undo history are
//! [`aic_editor::Document`], held by the [`aic_editor::Session`]; a [`DocView`] is one level of it
//! laid out for drawing: a [`Card`] per member with its pins, a [`Wire`] per pair of pins that
//! links join, and where each card sits. It is worked out again when the document changes
//! ([`DocView::sync`]), not every frame.
//!
//! The view shows one level of the graph: the top, or the workspace the player opened. A card is
//! a [`Member`] of that level: a node, the level's own ports included, or a workspace inside it.
//!
//! The canvas can show the graph in several views (ADR 0008). The editor holds one view at a time;
//! the others wait parked and are swapped in to be drawn ([`DocView::swap`]).

use std::collections::HashMap;

use aic_core::{Graph, LinkId, Member, NodeId, Pos, Side, Solution, WorkspaceId};
use aic_data::{Catalog, ItemId};
use aic_editor::{Document, Session};
use eframe::egui::Pos2;

pub use aic_editor::document::{Anchor, PinTarget, is_port, pin_targets, pins, port_ref, shown};

/// A card's id in the canvas widget: an `aic_node_graph` node. Made from the card's [`Member`],
/// so it stays the same for as long as the member exists.
pub type CardId = aic_node_graph::NodeId;

/// How much bigger the view's space is than the graph's. The canvas lays its cards out at its
/// largest zoom and shrinks the picture to the zoom shown, so text stays sharp up to that zoom (see
/// `canvas::MAX_ZOOM`); positions in the view are graph positions times this. Files, the graph and
/// everything outside the canvas keep graph positions.
pub const VIEW_SCALE: f32 = 2.0;

/// A graph position as a view position: where the card's top-left corner goes.
fn to_view(pos: Pos) -> Pos2 {
    Pos2::new(pos.x, pos.y) * VIEW_SCALE
}

/// A graph-space point as a graph position.
pub fn to_pos(pos: Pos2) -> Pos {
    Pos { x: pos.x, y: pos.y }
}

/// One card on the canvas: a node or a workspace of the level shown, with the ports its pins
/// stand for on each side, in pin order.
#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    pub member: Member,
    pub id: CardId,
    pub inputs: Vec<PinTarget>,
    pub outputs: Vec<PinTarget>,
    /// Which pins a wire ends on, by side (inputs, then outputs) and place.
    linked: [Vec<bool>; 2],
}

impl Card {
    fn new(graph: &Graph, catalog: &Catalog, member: Member) -> Self {
        let inputs = pin_targets(graph, catalog, member, Side::Input);
        let outputs = pin_targets(graph, catalog, member, Side::Output);
        Self {
            member,
            id: CardId::new(member),
            linked: [vec![false; inputs.len()], vec![false; outputs.len()]],
            inputs,
            outputs,
        }
    }

    /// Whether a wire ends on the pin at `index` of `side`. A pin without one stands for a port
    /// on the depot.
    pub fn is_linked(&self, side: Side, index: usize) -> bool {
        let pins = match side {
            Side::Input => &self.linked[0],
            Side::Output => &self.linked[1],
        };
        pins.get(index).copied().unwrap_or(false)
    }

    /// The ports its pins on `side` stand for.
    pub fn pins(&self, side: Side) -> &[PinTarget] {
        match side {
            Side::Input => &self.inputs,
            Side::Output => &self.outputs,
        }
    }

    /// Which of its pins on `side` a link of `item` at `node` uses.
    fn pin_of(&self, side: Side, node: NodeId, item: &ItemId) -> Option<usize> {
        self.pins(side).iter().position(|pin| pin.is(node, item))
    }
}

/// One pin of a card: the card, and the pin's place on its side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Pin {
    pub card: CardId,
    pub member: Member,
    pub index: usize,
}

/// The line drawn from an output pin to an input pin: every link the two pins' ports share.
/// Nearly always one.
#[derive(Debug, Clone, PartialEq)]
pub struct Wire {
    pub from: Pin,
    pub to: Pin,
    pub links: Vec<LinkId>,
}

/// One view's level of the graph, laid out for the canvas.
pub struct DocView {
    /// The workspace the view shows; `None` for the top of the graph.
    level: Option<WorkspaceId>,
    cards: Vec<Card>,
    by_member: HashMap<Member, usize>,
    members: HashMap<CardId, Member>,
    wires: Vec<Wire>,
    /// Where each card is drawn, in view space. The canvas widget moves cards here as the player
    /// drags them; [`Self::sync_from_view`] hands the moves to the graph.
    pub(crate) layout: aic_node_graph::Layout,
    pub(crate) anchors: HashMap<NodeId, Anchor>,
    /// Where the graph had each member the last time the view was brought in line with it. A card
    /// moves only when the graph's position differs, so a drag the view hasn't handed to the
    /// graph yet isn't undone by an unrelated edit.
    shown_pos: HashMap<Member, Pos>,
    /// The document's [`Document::generation`] this was brought in line with; `None` before it
    /// was drawn.
    synced: Option<u64>,
    /// What the cards and wires show, worked out by the canvas once per change, in their order.
    pub(crate) faces: crate::canvas::Faces,
}

impl DocView {
    /// A view of `level` (`None` for the top), laid out when it is first brought in line.
    pub fn new(level: Option<WorkspaceId>) -> Self {
        Self {
            level,
            cards: Vec::new(),
            by_member: HashMap::new(),
            members: HashMap::new(),
            wires: Vec::new(),
            layout: aic_node_graph::Layout::default(),
            anchors: HashMap::new(),
            shown_pos: HashMap::new(),
            synced: None,
            faces: crate::canvas::Faces::default(),
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

    /// Lays the level out afresh, every card where the graph has it.
    pub fn rebuild(&mut self, doc: &Document, catalog: &Catalog) {
        self.layout.clear();
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

    /// Brings the view in line with the graph: a card for each member of the level, moved where
    /// the graph moved it, and a wire for each pair of pins that links join. A level whose
    /// workspace is gone takes the view back up first.
    pub fn reconcile(&mut self, doc: &Document, catalog: &Catalog) {
        let graph = doc.graph();
        while let Some(level) = self.level
            && graph.workspace_info(level).is_none()
        {
            self.level = None;
        }
        self.cards = graph
            .members_of(self.level)
            .into_iter()
            .map(|member| Card::new(graph, catalog, member))
            .collect();
        self.by_member = (self.cards.iter().enumerate())
            .map(|(index, card)| (card.member, index))
            .collect();
        self.members = (self.cards.iter())
            .map(|card| (card.id, card.member))
            .collect();
        self.place_cards(graph);
        self.wires = self.wires_of(graph);
        for wire in &self.wires {
            for (pin, side) in [(wire.from, 1), (wire.to, 0)] {
                self.cards[self.by_member[&pin.member]].linked[side][pin.index] = true;
            }
        }
        self.synced = Some(doc.generation());
        // The cards are new: what they show is worked out again before they are drawn.
        self.faces = crate::canvas::Faces::default();
    }

    /// Forgets the places of cards that are gone, and moves the cards the graph moved.
    fn place_cards(&mut self, graph: &Graph) {
        let (members, by_member) = (&self.members, &self.by_member);
        self.layout.retain(|id, _| members.contains_key(id));
        self.shown_pos
            .retain(|member, _| by_member.contains_key(member));
        self.anchors
            .retain(|node, _| by_member.contains_key(&Member::Node(*node)));
        for card in &self.cards {
            let pos = match card.member {
                Member::Node(id) => graph.nodes()[id].pos,
                Member::Workspace(id) => graph.workspaces()[id].pos,
            };
            let moved = self.shown_pos.insert(card.member, pos) != Some(pos);
            if moved || !self.layout.contains_key(&card.id) {
                self.layout.insert(card.id, to_view(pos));
            }
        }
    }

    /// The wires of the level, in the order of the graph's links.
    fn wires_of(&self, graph: &Graph) -> Vec<Wire> {
        let mut wires: Vec<Wire> = Vec::new();
        let mut at: HashMap<(Pin, Pin), usize> = HashMap::new();
        for (id, link) in graph.links() {
            let (Some(from), Some(to)) = (
                self.pin(graph, link.from, Side::Output, &link.item),
                self.pin(graph, link.to, Side::Input, &link.item),
            ) else {
                continue;
            };
            match at.get(&(from, to)) {
                Some(&index) => wires[index].links.push(id),
                None => {
                    at.insert((from, to), wires.len());
                    wires.push(Wire {
                        from,
                        to,
                        links: vec![id],
                    });
                }
            }
        }
        wires
    }

    /// The pin that stands for the port of `item` on `node`'s `side`, on whatever card shows the
    /// node at this level. `None` when the node is outside the level.
    fn pin(&self, graph: &Graph, node: NodeId, side: Side, item: &ItemId) -> Option<Pin> {
        let member = shown(graph, self.level, node)?;
        let card = self.card(member)?;
        Some(Pin {
            card: card.id,
            member,
            index: card.pin_of(side, node, item)?,
        })
    }

    /// Where the port of `item` on `node`'s `side` is drawn: the card, and which of its pins.
    pub fn pin_of(&self, doc: &Document, node: NodeId, side: Side, item: &ItemId) -> Option<Pin> {
        self.pin(doc.graph(), node, side, item)
    }

    /// Puts `other` in `self` and `self` in `other`, each in line with the graph. A view coming in
    /// whose workspace is gone falls back to the top.
    pub fn swap(&mut self, doc: &Document, catalog: &Catalog, other: &mut DocView) {
        // The view going out is brought in line first, so it comes back as it left.
        self.sync(doc, catalog);
        std::mem::swap(self, other);
        self.sync(doc, catalog);
    }

    /// The level's cards, in the order the graph lists its members.
    pub fn cards(&self) -> &[Card] {
        &self.cards
    }

    /// The level's wires.
    pub fn wires(&self) -> &[Wire] {
        &self.wires
    }

    /// The card drawing a node or workspace, if the level shows it.
    pub fn card(&self, member: Member) -> Option<&Card> {
        self.by_member.get(&member).map(|&index| &self.cards[index])
    }

    /// The node or workspace a card of the canvas widget draws.
    pub fn member(&self, id: CardId) -> Option<Member> {
        self.members.get(&id).copied()
    }

    /// What draws `node` at the level shown: the node itself, or the workspace it is in. `None`
    /// when it is outside the level.
    pub fn shown(&self, doc: &Document, node: NodeId) -> Option<Member> {
        doc.shown(self.level, node)
    }

    /// Where a card is drawn, in view space ([`VIEW_SCALE`]).
    pub fn position(&self, member: Member) -> Option<Pos2> {
        self.layout.get(&self.card(member)?.id).copied()
    }

    /// Hands the places the player dragged cards to over to the graph. Expects the view in line
    /// with the document, as [`Self::sync`] leaves it.
    pub fn sync_from_view(&mut self, session: &mut Session) {
        let in_line = self.synced == Some(session.document().generation());
        let graph = session.document().graph();
        let moved: Vec<(Member, Pos)> = (self.cards.iter())
            // The level's own ports stay on the canvas's edges; their order is their own.
            .filter(|card| !matches!(card.member, Member::Node(id) if is_port(graph, id)))
            .filter_map(|card| {
                let pos = to_pos(*self.layout.get(&card.id)? / VIEW_SCALE);
                (self.shown_pos.get(&card.member) != Some(&pos)).then_some((card.member, pos))
            })
            .collect();
        if moved.is_empty() {
            return;
        }
        for (member, pos) in moved {
            session.move_member(member, pos);
            self.shown_pos.insert(member, pos);
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

    /// The cards the canvas draws at the level shown.
    pub fn cards(&self) -> &'a [Card] {
        self.view.cards()
    }

    /// The wires the canvas draws at the level shown.
    pub fn wires(&self) -> &'a [Wire] {
        self.view.wires()
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

    pub fn remove_node(&mut self, node: NodeId) {
        self.session.remove_node(node);
        self.sync();
    }

    pub fn connect(
        &mut self,
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
                { "id": "slag", "name": "slag", "category": "material" },
                { "id": "furnace", "name": "furnace", "category": "machine",
                  "machine": { "speed": 1, "type": "electric", "usage": 5 } }
            ],
            "recipes": [
                { "id": "ingot", "name": "ingot", "category": "material", "time": 2,
                  "in": { "ore": 1 }, "out": { "ingot": 1, "slag": 1 }, "producers": ["furnace"] },
                { "id": "ore", "name": "ore", "category": "material", "time": 2,
                  "in": { "ingot": 1, "slag": 1 }, "out": { "ore": 1 }, "producers": ["furnace"] }
            ]
        }"#;
        Catalog::from_bytes(data.as_bytes(), b"{}", None).expect("test catalog loads")
    }

    /// ore → ingot + slag.
    fn recipe() -> NodeKind {
        NodeKind::Recipe {
            recipe: "ingot".into(),
            facility: "furnace".into(),
            limit: Some(1.0),
        }
    }

    /// ingot + slag → ore.
    fn back() -> NodeKind {
        NodeKind::Recipe {
            recipe: "ore".into(),
            facility: "furnace".into(),
            limit: None,
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

        /// The wire that draws `link`, as the members and pin places at its two ends.
        fn wire(&self, link: LinkId) -> Option<End> {
            self.view
                .wires()
                .iter()
                .find(|wire| wire.links.contains(&link))
                .map(end)
        }

        fn sync_from_view(&mut self) {
            self.view.sync_from_view(&mut self.session);
        }

        fn swap(&mut self, other: &mut DocView) {
            self.view
                .swap(self.session.document(), self.session.catalog(), other);
        }

        /// Drags `member`'s card to `pos` in view space, as the canvas widget does.
        fn drag(&mut self, member: Member, pos: Pos2) {
            let id = self.view.card(member).expect("the card is shown").id;
            self.view.layout.insert(id, pos);
        }
    }

    type End = (Member, usize, Member, usize);

    fn end(wire: &Wire) -> End {
        (
            wire.from.member,
            wire.from.index,
            wire.to.member,
            wire.to.index,
        )
    }

    /// What a view draws: each member's position, and each wire as the members and pin places at
    /// its two ends.
    fn drawn(view: &DocView) -> (Vec<(Member, Pos2)>, Vec<End>) {
        let mut cards: Vec<(Member, Pos2)> = view
            .cards()
            .iter()
            .map(|card| (card.member, view.position(card.member).unwrap()))
            .collect();
        cards.sort_by_key(|(member, _)| format!("{member:?}"));
        let mut wires: Vec<End> = view.wires().iter().map(end).collect();
        wires.sort_by_key(|wire| format!("{wire:?}"));
        (cards, wires)
    }

    #[test]
    fn a_view_swapped_in_draws_what_changed_while_it_waited() {
        let mut rig = Rig::new();
        rig.add_node(recipe(), Pos2::ZERO);
        let mut other = DocView::new(None);
        rig.swap(&mut other);
        assert_eq!(rig.view.cards().len(), 1, "laid out on its first swap");
        rig.swap(&mut other);

        rig.add_node(recipe(), Pos2::ZERO);
        rig.swap(&mut other);
        assert_eq!(rig.view.cards().len(), 2);
    }

    #[test]
    fn a_card_keeps_its_id_while_its_member_lives() {
        let mut rig = Rig::new();
        let a = rig.add_node(recipe(), Pos2::ZERO);
        let b = rig.add_node(recipe(), Pos2::ZERO);
        let before = rig.view.card(Member::Node(b)).unwrap().id;
        // Removing an earlier member must not hand its id to a later one: the canvas widget keeps
        // a card's size and selection by id.
        rig.session.remove_node(a);
        rig.sync();
        assert_eq!(rig.view.card(Member::Node(b)).unwrap().id, before);
        assert_eq!(rig.view.member(before), Some(Member::Node(b)));
        assert!(rig.view.card(Member::Node(a)).is_none());
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
        assert_eq!(rig.view.wires().len(), 1);
        assert_eq!(
            rig.wire(link),
            Some((Member::Node(furnace), 0, Member::Node(splitter), 0))
        );

        rig.session.disconnect(link);
        rig.sync();
        assert_eq!(rig.view.wires().len(), 0);
        assert_eq!(rig.doc().graph().links().len(), 0);
    }

    #[test]
    fn two_items_between_the_same_cards_are_two_wires_on_their_own_pins() {
        let mut rig = Rig::new();
        let furnace = rig.add_node(recipe(), Pos2::ZERO);
        let back = rig.add_node(back(), Pos2::ZERO);
        let ingot = rig.connect(PortRef::new(furnace, "ingot"), PortRef::new(back, "ingot"));
        let slag = rig.connect(PortRef::new(furnace, "slag"), PortRef::new(back, "slag"));
        let (ingot, slag) = (rig.wire(ingot).unwrap(), rig.wire(slag).unwrap());
        assert_eq!(rig.view.wires().len(), 2);
        assert_ne!(ingot.1, slag.1, "each from its own output pin");
        assert_ne!(ingot.3, slag.3, "each to its own input pin");
        let card = rig.view.card(Member::Node(furnace)).unwrap();
        assert_eq!(card.outputs[ingot.1].item, Some("ingot".into()));
        assert_eq!(card.outputs[slag.1].item, Some("slag".into()));
        // Its outputs are linked and its input, on the depot, is not.
        assert!(card.is_linked(Side::Output, ingot.1) && card.is_linked(Side::Output, slag.1));
        assert!(!card.is_linked(Side::Input, 0));
    }

    #[test]
    fn a_link_into_a_workspace_is_a_wire_to_the_workspaces_card() {
        let mut rig = Rig::new();
        let furnace = rig.add_node(recipe(), Pos2::ZERO);
        let back = rig.add_node(back(), Pos2::ZERO);
        let link = rig.connect(PortRef::new(furnace, "ingot"), PortRef::new(back, "ingot"));
        rig.session.select_nodes(vec![back]);
        rig.session.make_workspace();
        rig.sync();
        let workspace = rig.doc().graph().workspaces().keys().next().unwrap();

        // From the top, the link that now ends on the workspace's port ends on its card.
        let wires: Vec<End> = rig.view.wires().iter().map(end).collect();
        assert_eq!(
            wires,
            [(Member::Node(furnace), 0, Member::Workspace(workspace), 0)]
        );
        assert!(rig.wire(link).is_none(), "the old link went through a port");

        // Inside, the port is a card of its own, wired to the node.
        let mut inside = DocView::new(Some(workspace));
        inside.sync(rig.doc(), rig.catalog());
        let port = rig.doc().graph().ports_of(workspace, Side::Input)[0];
        let wires: Vec<End> = inside.wires().iter().map(end).collect();
        assert_eq!(wires, [(Member::Node(port), 0, Member::Node(back), 0)]);
    }

    #[test]
    fn removing_a_node_removes_its_card() {
        let mut rig = Rig::new();
        let furnace = rig.add_node(recipe(), Pos2::new(10.0, 20.0));
        rig.session.remove_node(furnace);
        rig.sync();
        assert_eq!(rig.view.cards().len(), 0);
        assert!(rig.view.layout.is_empty(), "and its place is forgotten");
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
        assert_eq!(rig.view.wires().len(), 1);
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
        assert_eq!(view.wires().len(), 1);
        // A card sits at its graph position, in view space.
        assert_eq!(
            view.position(Member::Node(furnace)),
            Some(Pos2::new(5.0, 6.0) * VIEW_SCALE)
        );
        assert!(!doc.is_modified());
    }

    #[test]
    fn a_dragged_card_moves_its_member_in_the_graph() {
        let mut rig = Rig::new();
        let furnace = rig.add_node(recipe(), Pos2::ZERO);
        rig.drag(Member::Node(furnace), Pos2::new(40.0, 10.0));
        rig.sync_from_view();
        assert_eq!(
            rig.doc().graph().node(furnace).unwrap().pos,
            Pos { x: 20.0, y: 5.0 }
        );
        // Nothing moved since: nothing is handed over, and the document stays as it is.
        let generation = rig.doc().generation();
        rig.sync_from_view();
        assert_eq!(rig.doc().generation(), generation);
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
        rig.drag(Member::Node(furnace), Pos2::new(300.0, 300.0));
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
        rig.drag(Member::Node(furnace), Pos2::new(40.0, 0.0));
        rig.add_node(NodeKind::Splitter { item: None }, Pos2::ZERO);
        rig.sync_from_view();
        assert_eq!(
            rig.doc().graph().node(furnace).unwrap().pos,
            Pos { x: 20.0, y: 0.0 }
        );
    }
}
