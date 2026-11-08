//! The graph a player builds: nodes, links, and the rules for linking them.

use aic_data::{Catalog, FacilityId, ItemId, RecipeId, RegionId};
use serde::{Deserialize, Serialize};
use slotmap::{SlotMap, new_key_type};
use thiserror::Error;

new_key_type! {
    pub struct NodeId;
    pub struct LinkId;
}

/// One player-made plan. It belongs to exactly one region.
///
/// Fields are private so links can only be made through [`Graph::connect`], which keeps each
/// logistics node's item in step with its links. Graphs loaded from a file may still break the
/// rules; [`Graph::validate`] reports that.
#[derive(Debug, Clone)]
pub struct Graph {
    pub(crate) region: RegionId,
    pub(crate) nodes: SlotMap<NodeId, Node>,
    pub(crate) links: SlotMap<LinkId, Link>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub kind: NodeKind,
    /// Position on the canvas. The solver ignores it.
    pub pos: Pos,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Pos {
    pub x: f32,
    pub y: f32,
}

/// What a node is. Every limit is an upper bound (ADR 0003).
#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind {
    /// Runs a recipe on `facility`. `limit` caps the facility count.
    Recipe {
        recipe: RecipeId,
        facility: FacilityId,
        limit: Option<f64>,
    },
    /// Supplies one item from outside the graph, up to `limit` per minute.
    Source { item: ItemId, limit: f64 },
    /// Consumes one item as a goal of the graph, up to `limit` per minute if set.
    Sink { item: ItemId, limit: Option<f64> },
    /// Passes one item from its input links to its output links. `None` until the first link.
    Splitter { item: Option<ItemId> },
    /// Joins links of one item. `None` until the first link.
    Merger { item: Option<ItemId> },
    /// Buffers one item; inflow and outflow need not match. `None` until the first link.
    Storage { item: Option<ItemId> },
    /// Caps the flow of one item passing through it, per minute.
    ControlPort { item: Option<ItemId>, limit: f64 },
    /// A recipe node loaded from a save whose recipe or facility no longer exists. Kept so the
    /// file survives a data update; the solver leaves it out.
    MissingRecipe {
        recipe: RecipeId,
        facility: FacilityId,
        limit: Option<f64>,
    },
}

impl NodeKind {
    /// The single item of a source, sink or logistics node.
    pub fn item(&self) -> Option<&ItemId> {
        match self {
            Self::Source { item, .. } | Self::Sink { item, .. } => Some(item),
            Self::Splitter { item }
            | Self::Merger { item }
            | Self::Storage { item }
            | Self::ControlPort { item, .. } => item.as_ref(),
            Self::Recipe { .. } | Self::MissingRecipe { .. } => None,
        }
    }

    /// Splitters, mergers, storage and control ports: one item, any number of links per side.
    pub fn is_logistics(&self) -> bool {
        matches!(
            self,
            Self::Splitter { .. }
                | Self::Merger { .. }
                | Self::Storage { .. }
                | Self::ControlPort { .. }
        )
    }

    fn logistics_item_mut(&mut self) -> Option<&mut Option<ItemId>> {
        match self {
            Self::Splitter { item }
            | Self::Merger { item }
            | Self::Storage { item }
            | Self::ControlPort { item, .. } => Some(item),
            _ => None,
        }
    }
}

/// A wire carrying one item from an output port of `from` to an input port of `to`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub from: NodeId,
    pub to: NodeId,
    pub item: ItemId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Side {
    Input,
    Output,
}

/// One port: an item slot on one side of a node.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Port {
    pub node: NodeId,
    pub item: ItemId,
    pub side: Side,
}

/// One end of a link the player is making. `item` names the port on a recipe, source or sink node;
/// a logistics node has a single port per side, so its `item` may be `None`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortRef {
    pub node: NodeId,
    pub item: Option<ItemId>,
}

impl PortRef {
    pub fn new(node: NodeId, item: impl Into<ItemId>) -> Self {
        Self {
            node,
            item: Some(item.into()),
        }
    }

    /// The port of a logistics node, whatever its item.
    pub fn any(node: NodeId) -> Self {
        Self { node, item: None }
    }
}

/// Why a link can't be made.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LinkError {
    #[error("node {0:?} does not exist")]
    UnknownNode(NodeId),
    #[error("node {node:?} has no {side:?} port for {item:?}")]
    NoSuchPort {
        node: NodeId,
        side: Side,
        item: Option<ItemId>,
    },
    #[error("`{from}` can't link to `{to}`")]
    ItemMismatch { from: ItemId, to: ItemId },
    #[error("`{from}` and `{to}` don't mix: one is a solid, the other a fluid")]
    SolidFluid { from: ItemId, to: ItemId },
    #[error("neither end has an item yet")]
    NoItem,
    #[error("a node can't link to itself")]
    SelfLink,
    #[error("these two ports are already linked")]
    Duplicate,
}

/// Why a node edit was refused.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum EditError {
    #[error("node {0:?} does not exist")]
    UnknownNode(NodeId),
    #[error("this kind of node has no limit")]
    NoLimit,
    #[error("this kind of node needs a limit")]
    LimitRequired,
    #[error("a limit must be a finite number of at least 0, not {0}")]
    InvalidLimit(f64),
    #[error("only recipe nodes have a facility")]
    NotARecipe,
}

impl Graph {
    pub fn new(region: RegionId) -> Self {
        Self {
            region,
            nodes: SlotMap::with_key(),
            links: SlotMap::with_key(),
        }
    }

    pub fn region(&self) -> &RegionId {
        &self.region
    }

    pub fn nodes(&self) -> &SlotMap<NodeId, Node> {
        &self.nodes
    }

    pub fn links(&self) -> &SlotMap<LinkId, Link> {
        &self.links
    }

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id)
    }

    pub fn link(&self, id: LinkId) -> Option<&Link> {
        self.links.get(id)
    }

    /// Links that end at (`Side::Input`) or start from (`Side::Output`) `node`.
    pub fn links_at(&self, node: NodeId, side: Side) -> impl Iterator<Item = (LinkId, &Link)> {
        self.links.iter().filter(move |(_, link)| match side {
            Side::Input => link.to == node,
            Side::Output => link.from == node,
        })
    }

    pub fn add_node(&mut self, kind: NodeKind, pos: Pos) -> NodeId {
        self.nodes.insert(Node { kind, pos })
    }

    /// Removes a node and every link touching it.
    pub fn remove_node(&mut self, id: NodeId) -> Option<Node> {
        let touching: Vec<LinkId> = self
            .links
            .iter()
            .filter(|(_, link)| link.from == id || link.to == id)
            .map(|(link_id, _)| link_id)
            .collect();
        for link in touching {
            self.remove_link(link);
        }
        self.nodes.remove(id)
    }

    /// Links an output port to an input port. A logistics node without an item takes the link's item.
    pub fn connect(
        &mut self,
        catalog: &Catalog,
        from: PortRef,
        to: PortRef,
    ) -> Result<LinkId, LinkError> {
        let item = self.check_link(catalog, &from, &to)?;
        if self
            .links
            .values()
            .any(|link| link.from == from.node && link.to == to.node && link.item == item)
        {
            return Err(LinkError::Duplicate);
        }
        for node in [from.node, to.node] {
            if let Some(slot) = self.nodes[node].kind.logistics_item_mut() {
                slot.get_or_insert_with(|| item.clone());
            }
        }
        Ok(self.links.insert(Link {
            from: from.node,
            to: to.node,
            item,
        }))
    }

    /// Removes a link. A logistics node left without links forgets its item.
    pub fn remove_link(&mut self, id: LinkId) -> Option<Link> {
        let link = self.links.remove(id)?;
        for node in [link.from, link.to] {
            let still_linked = self.links.values().any(|l| l.from == node || l.to == node);
            if still_linked {
                continue;
            }
            if let Some(slot) = self
                .nodes
                .get_mut(node)
                .and_then(|n| n.kind.logistics_item_mut())
            {
                *slot = None;
            }
        }
        Some(link)
    }

    pub fn set_pos(&mut self, id: NodeId, pos: Pos) -> Result<(), EditError> {
        self.nodes
            .get_mut(id)
            .ok_or(EditError::UnknownNode(id))?
            .pos = pos;
        Ok(())
    }

    /// Sets the limit of a recipe, source, sink or control port node. Sources and control ports
    /// need one; recipe and sink nodes may have none.
    pub fn set_limit(&mut self, id: NodeId, limit: Option<f64>) -> Result<(), EditError> {
        if let Some(value) = limit
            && !(value.is_finite() && value >= 0.0)
        {
            return Err(EditError::InvalidLimit(value));
        }
        let node = self.nodes.get_mut(id).ok_or(EditError::UnknownNode(id))?;
        match &mut node.kind {
            NodeKind::Recipe { limit: slot, .. }
            | NodeKind::Sink { limit: slot, .. }
            | NodeKind::MissingRecipe { limit: slot, .. } => *slot = limit,
            NodeKind::Source { limit: slot, .. } | NodeKind::ControlPort { limit: slot, .. } => {
                *slot = limit.ok_or(EditError::LimitRequired)?;
            }
            NodeKind::Splitter { .. } | NodeKind::Merger { .. } | NodeKind::Storage { .. } => {
                return Err(EditError::NoLimit);
            }
        }
        Ok(())
    }

    /// Picks which facility a recipe node runs on. [`Graph::validate`] reports one that can't run
    /// the recipe.
    pub fn set_facility(&mut self, id: NodeId, facility: FacilityId) -> Result<(), EditError> {
        match &mut self
            .nodes
            .get_mut(id)
            .ok_or(EditError::UnknownNode(id))?
            .kind
        {
            NodeKind::Recipe { facility: slot, .. } => {
                *slot = facility;
                Ok(())
            }
            _ => Err(EditError::NotARecipe),
        }
    }

    /// The item a link between these ports would carry, or why there can't be one.
    pub(crate) fn check_link(
        &self,
        catalog: &Catalog,
        from: &PortRef,
        to: &PortRef,
    ) -> Result<ItemId, LinkError> {
        if from.node == to.node {
            return Err(LinkError::SelfLink);
        }
        let from_item = self.port_item(catalog, from, Side::Output)?;
        let to_item = self.port_item(catalog, to, Side::Input)?;
        match (from_item, to_item) {
            (Some(a), Some(b)) if a == b => Ok(a),
            (Some(a), Some(b)) => {
                let fluid = |id: &ItemId| catalog.item(id.as_str()).is_some_and(|i| i.is_fluid);
                if fluid(&a) == fluid(&b) {
                    Err(LinkError::ItemMismatch { from: a, to: b })
                } else {
                    Err(LinkError::SolidFluid { from: a, to: b })
                }
            }
            (Some(item), None) | (None, Some(item)) => Ok(item),
            (None, None) => Err(LinkError::NoItem),
        }
    }

    /// The item of the port `port` names on `side`. `None` for a logistics node with no item yet.
    fn port_item(
        &self,
        catalog: &Catalog,
        port: &PortRef,
        side: Side,
    ) -> Result<Option<ItemId>, LinkError> {
        let node = self
            .nodes
            .get(port.node)
            .ok_or(LinkError::UnknownNode(port.node))?;
        let no_port = || LinkError::NoSuchPort {
            node: port.node,
            side,
            item: port.item.clone(),
        };
        match &node.kind {
            NodeKind::Recipe { recipe, .. } => {
                let recipe = catalog.recipe(recipe.as_str()).ok_or_else(no_port)?;
                let items = match side {
                    Side::Input => &recipe.inputs,
                    Side::Output => &recipe.outputs,
                };
                let wanted = port.item.as_ref().ok_or_else(no_port)?;
                items
                    .iter()
                    .find(|(id, _)| id == wanted)
                    .map(|(id, _)| Some(id.clone()))
                    .ok_or_else(no_port)
            }
            NodeKind::Source { item, .. } | NodeKind::Sink { item, .. } => {
                let right_side = matches!(
                    (&node.kind, side),
                    (NodeKind::Source { .. }, Side::Output) | (NodeKind::Sink { .. }, Side::Input)
                );
                let right_item = port.item.as_ref().is_none_or(|wanted| wanted == item);
                if right_side && right_item {
                    Ok(Some(item.clone()))
                } else {
                    Err(no_port())
                }
            }
            // A logistics node answers with its own item; a different requested item shows up as
            // a mismatch against the other end.
            NodeKind::Splitter { item }
            | NodeKind::Merger { item }
            | NodeKind::Storage { item }
            | NodeKind::ControlPort { item, .. } => Ok(item.clone().or_else(|| port.item.clone())),
            NodeKind::MissingRecipe { .. } => Err(no_port()),
        }
    }
}
