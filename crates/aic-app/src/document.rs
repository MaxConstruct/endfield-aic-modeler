//! One open graph: the `aic-core` [`Graph`], which is the source of truth, the egui-snarl view of it,
//! and its latest [`Solution`].
//!
//! Every edit goes through [`Document`], which changes the graph and the view together. The view
//! holds each node's `NodeId` as its payload and one wire per link.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use aic_core::{
    EditError, Graph, LinkError, LinkId, NodeId, NodeKind, Port, PortRef, Pos, Side, Solution,
    solve,
};
use aic_data::{Catalog, FacilityId, ItemId, RegionId};
use eframe::egui::Pos2;
use egui_snarl::{InPinId, OutPinId, Snarl};

/// A node's id in the egui-snarl view. Not stable: snarl reuses it after a removal.
pub type ViewId = egui_snarl::NodeId;

/// Where a new node's edge should land once its size is known: its right edge for a node that
/// feeds the port it was added from, its left edge for one that the port feeds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Anchor {
    Left(f32),
    Right(f32),
}

pub struct Document {
    pub(crate) graph: Graph,
    pub(crate) view: Snarl<NodeId>,
    view_ids: HashMap<NodeId, ViewId>,
    pub(crate) anchors: HashMap<NodeId, Anchor>,
    solution: Solution,
    stale: bool,
}

impl Document {
    pub fn new(region: RegionId) -> Self {
        Self {
            graph: Graph::new(region),
            view: Snarl::new(),
            view_ids: HashMap::new(),
            anchors: HashMap::new(),
            solution: Solution::default(),
            stale: true,
        }
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
        &mut Snarl<NodeId>,
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

    pub fn view_id(&self, node: NodeId) -> Option<ViewId> {
        self.view_ids.get(&node).copied()
    }

    pub fn add_node(&mut self, kind: NodeKind, pos: Pos2) -> NodeId {
        let id = self.graph.add_node(kind, Pos { x: pos.x, y: pos.y });
        let view_id = self.view.insert_node(pos, id);
        self.view_ids.insert(id, view_id);
        self.stale = true;
        id
    }

    /// Adds a node whose edge lands on `anchor` once the canvas has measured it.
    pub fn add_node_anchored(&mut self, kind: NodeKind, pos: Pos2, anchor: Anchor) -> NodeId {
        let id = self.add_node(kind, pos);
        self.anchors.insert(id, anchor);
        id
    }

    pub fn remove_node(&mut self, id: NodeId) {
        if self.graph.remove_node(id).is_some()
            && let Some(view_id) = self.view_ids.remove(&id)
        {
            self.view.remove_node(view_id);
        }
        self.anchors.remove(&id);
        self.stale = true;
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
        self.stale = true;
        Ok(link)
    }

    pub fn remove_link(&mut self, catalog: &Catalog, id: LinkId) {
        if let Some((out_pin, in_pin)) = self.wire(catalog, id) {
            self.view.disconnect(out_pin, in_pin);
        }
        self.graph.remove_link(id);
        self.stale = true;
    }

    pub fn set_limit(&mut self, id: NodeId, limit: Option<f64>) -> Result<(), EditError> {
        self.graph.set_limit(id, limit)?;
        self.stale = true;
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
        self.stale = true;
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
        self.stale = true;
        Ok(())
    }

    /// The link drawn as this wire, if there is one.
    pub fn link_at(&self, catalog: &Catalog, out_pin: OutPinId, in_pin: InPinId) -> Option<LinkId> {
        self.graph
            .links()
            .keys()
            .find(|&id| self.wire(catalog, id) == Some((out_pin, in_pin)))
    }

    /// The wire that draws `link`: from the pin of its item on one node to the pin on the other.
    pub fn wire(&self, catalog: &Catalog, link: LinkId) -> Option<(OutPinId, InPinId)> {
        let link = self.graph.link(link)?;
        let pin_of = |node: NodeId, side: Side| -> Option<(ViewId, usize)> {
            let kind = &self.graph.node(node)?.kind;
            let index = if kind.is_logistics() {
                0
            } else {
                pins(catalog, kind, side)
                    .iter()
                    .position(|item| item.as_ref() == Some(&link.item))?
            };
            Some((self.view_id(node)?, index))
        };
        let (from, output) = pin_of(link.from, Side::Output)?;
        let (to, input) = pin_of(link.to, Side::Input)?;
        Some((OutPinId { node: from, output }, InPinId { node: to, input }))
    }

    /// Copies node moves from the view into the graph, and drops links whose wire the view lost.
    /// egui-snarl's Cmd+drag on a pin ("yank") removes wires without telling the viewer.
    pub fn sync_from_view(&mut self, catalog: &Catalog) {
        for (view_id, pos, &id) in self.view.nodes_pos_ids() {
            debug_assert_eq!(self.view_ids.get(&id), Some(&view_id));
            let new = Pos { x: pos.x, y: pos.y };
            if self.graph.node(id).is_some_and(|node| node.pos != new) {
                // Position doesn't affect the solve, so this doesn't make the solution stale.
                let _ = self.graph.set_pos(id, new);
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
            self.stale = true;
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

/// The items of a node's ports on one side, in pin order. A logistics node has one port per side,
/// whose item is `None` until its first link.
pub fn pins(catalog: &Catalog, kind: &NodeKind, side: Side) -> Vec<Option<ItemId>> {
    match (kind, side) {
        (NodeKind::Recipe { recipe, .. }, _) => catalog
            .recipe(recipe.as_str())
            .map(|recipe| {
                let items = match side {
                    Side::Input => &recipe.inputs,
                    Side::Output => &recipe.outputs,
                };
                items.iter().map(|(item, _)| Some(item.clone())).collect()
            })
            .unwrap_or_default(),
        (NodeKind::Source { item, .. }, Side::Output)
        | (NodeKind::Sink { item, .. }, Side::Input) => {
            vec![Some(item.clone())]
        }
        (NodeKind::Source { .. }, Side::Input) | (NodeKind::Sink { .. }, Side::Output) => vec![],
        (
            NodeKind::Splitter { item }
            | NodeKind::Merger { item }
            | NodeKind::Storage { item }
            | NodeKind::ControlPort { item, .. },
            _,
        ) => vec![item.clone()],
        (NodeKind::MissingRecipe { .. }, _) => vec![],
    }
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

        doc.remove_node(furnace);
        assert_eq!(doc.view.nodes().count(), 0);
        assert!(doc.solve_if_stale(&catalog).is_some(), "removal re-solves");
    }
}
