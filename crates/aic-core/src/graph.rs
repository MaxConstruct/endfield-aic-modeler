//! The graph a player builds: nodes, links, and the rules for linking them.

use aic_data::{Catalog, FacilityId, ItemId, RecipeId, RegionId};
use serde::{Deserialize, Serialize};
use slotmap::{SlotMap, new_key_type};
use thiserror::Error;

use crate::solve::RoundUp;
use crate::workspace::Workspace;

new_key_type! {
    pub struct NodeId;
    pub struct LinkId;
    pub struct WorkspaceId;
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
    pub(crate) workspaces: SlotMap<WorkspaceId, Workspace>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub kind: NodeKind,
    /// Position on the canvas. The solver ignores it.
    pub pos: Pos,
    /// The workspace the node is directly in; `None` at the top of the graph.
    pub workspace: Option<WorkspaceId>,
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
    /// A sink for a settlement trade: takes `item`, the trade recipe's input, up to `limit` per
    /// minute if set. The coupons it earns count in the totals.
    OutpostOrder {
        recipe: RecipeId,
        item: ItemId,
        limit: Option<f64>,
    },
    /// Passes one item from its input links to its output links. `None` until the first link.
    Splitter { item: Option<ItemId> },
    /// Joins links of one item. `None` until the first link.
    Merger { item: Option<ItemId> },
    /// Buffers one item; inflow and outflow need not match. `None` until the first link.
    Storage { item: Option<ItemId> },
    /// Caps the flow of one item passing through it, per minute.
    ControlPort { item: Option<ItemId>, limit: f64 },
    /// A port of the workspace the node is in: an `Input` port brings `item` in from outside the
    /// workspace, an `Output` port sends it out. `None` until its first link.
    WorkspacePort { side: Side, item: Option<ItemId> },
    /// A recipe node loaded from a save whose recipe or facility no longer exists. Kept so the
    /// file survives a data update; the solver leaves it out.
    MissingRecipe {
        recipe: RecipeId,
        facility: FacilityId,
        limit: Option<f64>,
    },
}

impl NodeKind {
    /// The items of the node's ports on one side, in port order. A logistics node has one port per
    /// side, whose item is `None` until its first link.
    pub fn port_items(&self, catalog: &Catalog, side: Side) -> Vec<Option<ItemId>> {
        match (self, side) {
            (Self::Recipe { recipe, .. }, _) => catalog
                .recipe(recipe.as_str())
                .map(|recipe| {
                    let items = match side {
                        Side::Input => &recipe.inputs,
                        Side::Output => &recipe.outputs,
                    };
                    items.iter().map(|(item, _)| Some(item.clone())).collect()
                })
                .unwrap_or_default(),
            (Self::Source { item, .. }, Side::Output)
            | (Self::Sink { item, .. } | Self::OutpostOrder { item, .. }, Side::Input) => {
                vec![Some(item.clone())]
            }
            (Self::Source { .. }, Side::Input)
            | (Self::Sink { .. } | Self::OutpostOrder { .. }, Side::Output) => vec![],
            (
                Self::Splitter { item }
                | Self::Merger { item }
                | Self::Storage { item }
                | Self::ControlPort { item, .. }
                | Self::WorkspacePort { item, .. },
                _,
            ) => vec![item.clone()],
            (Self::MissingRecipe { .. }, _) => vec![],
        }
    }

    /// An outpost order for a trade recipe, with no limit. `None` when `recipe` isn't a trade.
    pub fn outpost_order(catalog: &Catalog, recipe: &RecipeId) -> Option<Self> {
        let found = catalog.recipe(recipe.as_str()).filter(|r| r.is_trade)?;
        let (item, _) = found.inputs.first()?;
        Some(Self::OutpostOrder {
            recipe: recipe.clone(),
            item: item.clone(),
            limit: None,
        })
    }

    /// Whether the node consumes its one item as a goal: a sink or an outpost order.
    pub fn is_sink(&self) -> bool {
        matches!(self, Self::Sink { .. } | Self::OutpostOrder { .. })
    }

    /// The single item of a source, sink, outpost order or logistics node.
    pub fn item(&self) -> Option<&ItemId> {
        match self {
            Self::Source { item, .. }
            | Self::Sink { item, .. }
            | Self::OutpostOrder { item, .. } => Some(item),
            Self::Splitter { item }
            | Self::Merger { item }
            | Self::Storage { item }
            | Self::ControlPort { item, .. }
            | Self::WorkspacePort { item, .. } => item.as_ref(),
            Self::Recipe { .. } | Self::MissingRecipe { .. } => None,
        }
    }

    /// The limit of a recipe, source, sink or control port node, if it has one.
    pub fn limit(&self) -> Option<f64> {
        match self {
            Self::Recipe { limit, .. }
            | Self::Sink { limit, .. }
            | Self::OutpostOrder { limit, .. }
            | Self::MissingRecipe { limit, .. } => *limit,
            Self::Source { limit, .. } | Self::ControlPort { limit, .. } => Some(*limit),
            Self::Splitter { .. }
            | Self::Merger { .. }
            | Self::Storage { .. }
            | Self::WorkspacePort { .. } => None,
        }
    }

    /// Splitters, mergers, storage, control ports and workspace ports: one item, any number of
    /// links per side.
    pub fn is_logistics(&self) -> bool {
        matches!(
            self,
            Self::Splitter { .. }
                | Self::Merger { .. }
                | Self::Storage { .. }
                | Self::ControlPort { .. }
                | Self::WorkspacePort { .. }
        )
    }

    fn logistics_item_mut(&mut self) -> Option<&mut Option<ItemId>> {
        match self {
            Self::Splitter { item }
            | Self::Merger { item }
            | Self::Storage { item }
            | Self::ControlPort { item, .. }
            | Self::WorkspacePort { item, .. } => Some(item),
            _ => None,
        }
    }
}

/// A wire carrying one item from an output port of `from` to an input port of `to`.
#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub from: NodeId,
    pub to: NodeId,
    pub item: ItemId,
    /// Its share of a ratio lock on the fan-out at `from`. Links with a share at the same port
    /// keep their flows in proportion to their shares.
    pub out_share: Option<f64>,
    /// Its share of a ratio lock on the fan-in at `to`.
    pub in_share: Option<f64>,
}

impl Link {
    pub fn new(from: NodeId, to: NodeId, item: ItemId) -> Self {
        Self {
            from,
            to,
            item,
            out_share: None,
            in_share: None,
        }
    }

    /// Its share of the ratio lock at its `Output` end (the fan-out at `from`) or its `Input` end
    /// (the fan-in at `to`).
    pub fn share(&self, side: Side) -> Option<f64> {
        match side {
            Side::Output => self.out_share,
            Side::Input => self.in_share,
        }
    }
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
    #[error("a link can't cross a workspace's edge except through one of its ports")]
    AcrossWorkspaces,
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
    #[error("the node has no such port")]
    NoSuchPort,
    #[error("link {0:?} does not exist")]
    UnknownLink(LinkId),
    #[error("a share must be a finite number above 0, not {0}")]
    InvalidShare(f64),
    #[error("workspace {0:?} does not exist")]
    UnknownWorkspace(WorkspaceId),
    #[error("a workspace made from a selection needs at least one member")]
    EmptyWorkspace,
    #[error("only nodes and workspaces in the same workspace can go in a new workspace together")]
    MixedWorkspaces,
    #[error("a workspace's ports stay on its edge")]
    PortOutsideWorkspace,
}

impl Graph {
    pub fn new(region: RegionId) -> Self {
        Self {
            region,
            nodes: SlotMap::with_key(),
            links: SlotMap::with_key(),
            workspaces: SlotMap::with_key(),
        }
    }

    pub fn region(&self) -> &RegionId {
        &self.region
    }

    /// Moves the graph to another region. Its nodes stay; a recipe or facility the new region
    /// lacks is an issue ([`Issue::RecipeOutsideRegion`](crate::Issue)), not an error.
    pub fn set_region(&mut self, region: RegionId) {
        self.region = region;
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
        self.nodes.insert(Node {
            kind,
            pos,
            workspace: None,
        })
    }

    /// Adds a node inside `workspace` (`None`: the top of the graph).
    pub fn add_node_in(
        &mut self,
        kind: NodeKind,
        pos: Pos,
        workspace: Option<WorkspaceId>,
    ) -> Result<NodeId, EditError> {
        match workspace {
            Some(id) if !self.workspaces.contains_key(id) => Err(EditError::UnknownWorkspace(id)),
            _ => Ok(self.nodes.insert(Node {
                kind,
                pos,
                workspace,
            })),
        }
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
        Ok(self.links.insert(Link::new(from.node, to.node, item)))
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

    /// Sets a link's share of the ratio lock at its `side` end, or clears it with `None`.
    pub fn set_share(
        &mut self,
        link: LinkId,
        side: Side,
        share: Option<f64>,
    ) -> Result<(), EditError> {
        if let Some(value) = share
            && !(value.is_finite() && value > 0.0)
        {
            return Err(EditError::InvalidShare(value));
        }
        let link = self
            .links
            .get_mut(link)
            .ok_or(EditError::UnknownLink(link))?;
        match side {
            Side::Output => link.out_share = share,
            Side::Input => link.in_share = share,
        }
        Ok(())
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
            | NodeKind::OutpostOrder { limit: slot, .. }
            | NodeKind::MissingRecipe { limit: slot, .. } => *slot = limit,
            NodeKind::Source { limit: slot, .. } | NodeKind::ControlPort { limit: slot, .. } => {
                *slot = limit.ok_or(EditError::LimitRequired)?;
            }
            NodeKind::Splitter { .. }
            | NodeKind::Merger { .. }
            | NodeKind::Storage { .. }
            | NodeKind::WorkspacePort { .. } => {
                return Err(EditError::NoLimit);
            }
        }
        Ok(())
    }

    /// The node's limit as a flow through `port`, per minute. On a recipe node that is what the
    /// port carries at the facility count limit: its rate per facility times the limit, plus, on a
    /// port of an activation input, that input for every running facility (ADR 0013). `None` when
    /// the node has no limit.
    pub fn port_limit(&self, catalog: &Catalog, port: &Port) -> Option<f64> {
        let rate = self.port_rate(catalog, port).ok()?;
        Some(rate.flow(self.nodes.get(port.node)?.kind.limit()?))
    }

    /// Sets the node's limit as a flow through `port`, per minute: the same limit as
    /// [`Graph::set_limit`], typed another way. A recipe node stores it as a facility count, so it
    /// keeps that count if its facility changes later.
    pub fn set_port_limit(
        &mut self,
        catalog: &Catalog,
        port: &Port,
        rate: Option<f64>,
    ) -> Result<(), EditError> {
        if let Some(value) = rate
            && !(value.is_finite() && value >= 0.0)
        {
            return Err(EditError::InvalidLimit(value));
        }
        let per = self.port_rate(catalog, port)?;
        self.set_limit(port.node, rate.map(|rate| per.most_for(rate)))
    }

    /// What flows through `port` for each unit of the node's limit: the port's rates per facility
    /// on a recipe node, 1 on a node whose limit is already a rate.
    fn port_rate(&self, catalog: &Catalog, port: &Port) -> Result<PortRate, EditError> {
        let node = self
            .nodes
            .get(port.node)
            .ok_or(EditError::UnknownNode(port.node))?;
        match &node.kind {
            NodeKind::Recipe {
                recipe, facility, ..
            } => {
                let recipe = catalog
                    .recipe(recipe.as_str())
                    .ok_or(EditError::NoSuchPort)?;
                let facility = catalog
                    .facility(facility.as_str())
                    .ok_or(EditError::NoSuchPort)?;
                let rate = match port.side {
                    Side::Input => recipe
                        .input_rates(facility)
                        .find(|(item, _)| **item == port.item)
                        .map(|(_, rate)| PortRate {
                            scaled: rate.scaled,
                            activation: rate.activation,
                        }),
                    Side::Output => recipe
                        .outputs
                        .iter()
                        .find(|(item, _)| *item == port.item)
                        .map(|(_, qty)| {
                            PortRate::scaled(recipe.rate_per_min(*qty, facility.speed))
                        }),
                };
                match rate {
                    Some(rate) if rate.scaled > 0.0 || rate.activation > 0.0 => Ok(rate),
                    _ => Err(EditError::NoSuchPort),
                }
            }
            NodeKind::Source { item, .. } if port.side == Side::Output && *item == port.item => {
                Ok(PortRate::scaled(1.0))
            }
            NodeKind::Sink { item, .. } | NodeKind::OutpostOrder { item, .. }
                if port.side == Side::Input && *item == port.item =>
            {
                Ok(PortRate::scaled(1.0))
            }
            NodeKind::ControlPort { item, .. }
                if item.as_ref().is_none_or(|item| *item == port.item) =>
            {
                Ok(PortRate::scaled(1.0))
            }
            NodeKind::Splitter { .. }
            | NodeKind::Merger { .. }
            | NodeKind::Storage { .. }
            | NodeKind::WorkspacePort { .. } => Err(EditError::NoLimit),
            _ => Err(EditError::NoSuchPort),
        }
    }

    /// Matches the graph against another data version. An ID the data doesn't have but names an
    /// alias for (an older version's ID for one of its recipes or items) becomes the ID it
    /// stands for, wherever the graph holds it: recipe nodes, outpost orders, the items of
    /// sources, sinks, logistics nodes and workspace ports, and links. Then a recipe or facility
    /// the data lacks makes a [`NodeKind::MissingRecipe`], and a missing recipe it has becomes a
    /// recipe node again. Node IDs stay the same.
    pub fn rebind(&mut self, catalog: &Catalog) {
        let follow_item = |id: &mut ItemId| {
            if catalog.item(id.as_str()).is_none()
                && let Some(new) = catalog.item_alias(id.as_str())
            {
                *id = new.clone();
            }
        };
        let follow_recipe = |id: &mut RecipeId| {
            if catalog.recipe(id.as_str()).is_none()
                && let Some(new) = catalog.recipe_alias(id.as_str())
            {
                *id = new.clone();
            }
        };
        for link in self.links.values_mut() {
            follow_item(&mut link.item);
        }
        for node in self.nodes.values_mut() {
            match &mut node.kind {
                NodeKind::Recipe { recipe, .. } | NodeKind::MissingRecipe { recipe, .. } => {
                    follow_recipe(recipe);
                }
                NodeKind::OutpostOrder { recipe, item, .. } => {
                    follow_recipe(recipe);
                    follow_item(item);
                }
                NodeKind::Source { item, .. } | NodeKind::Sink { item, .. } => follow_item(item),
                NodeKind::Splitter { item }
                | NodeKind::Merger { item }
                | NodeKind::Storage { item }
                | NodeKind::ControlPort { item, .. }
                | NodeKind::WorkspacePort { item, .. } => {
                    if let Some(item) = item {
                        follow_item(item);
                    }
                }
            }
            let (recipe, facility, limit) = match &node.kind {
                NodeKind::Recipe {
                    recipe,
                    facility,
                    limit,
                }
                | NodeKind::MissingRecipe {
                    recipe,
                    facility,
                    limit,
                } => (recipe.clone(), facility.clone(), *limit),
                _ => continue,
            };
            let known = catalog.recipe(recipe.as_str()).is_some()
                && catalog.facility(facility.as_str()).is_some();
            node.kind = if known {
                NodeKind::Recipe {
                    recipe,
                    facility,
                    limit,
                }
            } else {
                NodeKind::MissingRecipe {
                    recipe,
                    facility,
                    limit,
                }
            };
        }
    }

    /// A logistics node or port without links forgets its item, as it would after unlinking.
    pub(crate) fn forget_unlinked_items(&mut self) {
        let unlinked: Vec<NodeId> = self
            .nodes
            .keys()
            .filter(|&id| !self.links.values().any(|l| l.from == id || l.to == id))
            .collect();
        for id in unlinked {
            if let Some(slot) = self.nodes[id].kind.logistics_item_mut() {
                *slot = None;
            }
        }
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
        let from_level = self
            .end_level(from.node, Side::Output)
            .ok_or(LinkError::UnknownNode(from.node))?;
        let to_level = self
            .end_level(to.node, Side::Input)
            .ok_or(LinkError::UnknownNode(to.node))?;
        if from_level != to_level {
            return Err(LinkError::AcrossWorkspaces);
        }
        let from_item = self.port_item(catalog, from, Side::Output)?;
        let to_item = self.port_item(catalog, to, Side::Input)?;
        match (from_item, to_item) {
            (Some(a), Some(b)) if a == b => Ok(a),
            (Some(a), Some(b)) => {
                let fluid = |id: &ItemId| catalog.item(id.as_str()).is_some_and(|i| i.is_fluid());
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
            NodeKind::Source { item, .. }
            | NodeKind::Sink { item, .. }
            | NodeKind::OutpostOrder { item, .. } => {
                let right_side = match side {
                    Side::Output => matches!(node.kind, NodeKind::Source { .. }),
                    Side::Input => node.kind.is_sink(),
                };
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
            | NodeKind::ControlPort { item, .. }
            | NodeKind::WorkspacePort { item, .. } => {
                Ok(item.clone().or_else(|| port.item.clone()))
            }
            NodeKind::MissingRecipe { .. } => Err(no_port()),
        }
    }
}

/// What flows through one port for each unit of its node's limit (see [`Graph::port_limit`]).
#[derive(Debug, Clone, Copy, PartialEq)]
struct PortRate {
    /// Per unit of the limit, times the limit.
    scaled: f64,
    /// Per running facility: the facility count rounded up. Zero but on a port of an activation
    /// input.
    activation: f64,
}

impl PortRate {
    fn scaled(rate: f64) -> Self {
        Self {
            scaled: rate,
            activation: 0.0,
        }
    }

    /// What flows through the port at `limit`.
    fn flow(self, limit: f64) -> f64 {
        if self.activation == 0.0 {
            return limit * self.scaled;
        }
        let running = f64::from(RoundUp::of(limit).facilities);
        limit * self.scaled + running * self.activation
    }

    /// The largest limit whose flow through the port is at most `rate`.
    fn most_for(self, rate: f64) -> f64 {
        if self.activation == 0.0 {
            return rate / self.scaled;
        }
        // Facilities that `rate` runs flat out, allowing for rounding in `rate` itself.
        let flat_out = (rate / (self.scaled + self.activation) * (1.0 + 1e-9)).floor();
        if self.scaled > 0.0 {
            // One more facility, busy with what its activation input leaves over.
            let partly = (rate - self.activation * (flat_out + 1.0)) / self.scaled;
            if partly > flat_out {
                return partly;
            }
        }
        flat_out
    }
}
