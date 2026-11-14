//! What the player has selected.

use aic_core::{LinkId, Member, NodeId, WorkspaceId};

/// What the player has selected: nodes and workspaces as drawn on the canvas, or one link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    /// Never empty.
    Items(Vec<Member>),
    Link(LinkId),
}

impl Selection {
    pub fn node(id: NodeId) -> Self {
        Self::Items(vec![Member::Node(id)])
    }

    /// Some items, or `None` for none.
    pub fn of_items(items: Vec<Member>) -> Option<Self> {
        (!items.is_empty()).then_some(Self::Items(items))
    }

    /// Some nodes, or `None` for none.
    pub fn of_nodes(nodes: Vec<NodeId>) -> Option<Self> {
        Self::of_items(nodes.into_iter().map(Member::Node).collect())
    }

    pub fn items(&self) -> &[Member] {
        match self {
            Self::Items(items) => items,
            Self::Link(_) => &[],
        }
    }

    /// The selected nodes, leaving out workspaces.
    pub fn nodes(&self) -> Vec<NodeId> {
        self.items()
            .iter()
            .filter_map(|m| match m {
                Member::Node(id) => Some(*id),
                Member::Workspace(_) => None,
            })
            .collect()
    }

    pub fn contains(&self, member: Member) -> bool {
        self.items().contains(&member)
    }

    pub fn contains_node(&self, id: NodeId) -> bool {
        self.contains(Member::Node(id))
    }

    /// The node, when exactly one item is selected and it is a node.
    pub fn single_node(&self) -> Option<NodeId> {
        match self.items() {
            [Member::Node(one)] => Some(*one),
            _ => None,
        }
    }

    /// The workspace, when exactly one item is selected and it is a workspace.
    pub fn single_workspace(&self) -> Option<WorkspaceId> {
        match self.items() {
            [Member::Workspace(one)] => Some(*one),
            _ => None,
        }
    }
}
