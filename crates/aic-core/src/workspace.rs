//! Workspaces: named places that hold nodes and other workspaces, shown as one card from outside.
//!
//! What crosses a workspace's edge goes through its ports. A port is a node of the workspace
//! ([`NodeKind::WorkspacePort`]) with an outside end and an inside end; links outside the
//! workspace reach its outside end, links inside reach its inside end. A port links nothing
//! through on its own: with either end unlinked, nothing flows through it.

use std::collections::{HashMap, HashSet};

use aic_data::{Catalog, ItemId};

use crate::graph::{
    EditError, Graph, Link, LinkId, Node, NodeId, NodeKind, Pos, Side, WorkspaceId,
};

/// A named place holding nodes and other workspaces.
#[derive(Debug, Clone, PartialEq)]
pub struct Workspace {
    pub name: String,
    /// The workspace this one is in; `None` at the top of the graph.
    pub parent: Option<WorkspaceId>,
    /// Where its card sits in its parent.
    pub pos: Pos,
}

/// What a canvas shows as one card: a node, or a workspace inside the one being looked at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Member {
    Node(NodeId),
    Workspace(WorkspaceId),
}

impl Graph {
    pub fn workspaces(&self) -> &slotmap::SlotMap<WorkspaceId, Workspace> {
        &self.workspaces
    }

    pub fn workspace_info(&self, id: WorkspaceId) -> Option<&Workspace> {
        self.workspaces.get(id)
    }

    /// The workspace a member is directly in.
    pub fn parent_of(&self, member: Member) -> Option<WorkspaceId> {
        match member {
            Member::Node(id) => self.nodes.get(id)?.workspace,
            Member::Workspace(id) => self.workspaces.get(id)?.parent,
        }
    }

    /// What is directly in `level` (`None`: the top of the graph): its nodes, ports included, and
    /// the workspaces in it. Nodes first, in graph order.
    pub fn members_of(&self, level: Option<WorkspaceId>) -> Vec<Member> {
        let nodes = self
            .nodes
            .iter()
            .filter(|(_, node)| node.workspace == level)
            .map(|(id, _)| Member::Node(id));
        let workspaces = self
            .workspaces
            .iter()
            .filter(|(_, ws)| ws.parent == level)
            .map(|(id, _)| Member::Workspace(id));
        nodes.chain(workspaces).collect()
    }

    /// A new, empty workspace in `parent`.
    pub fn add_workspace(
        &mut self,
        name: &str,
        parent: Option<WorkspaceId>,
        pos: Pos,
    ) -> Result<WorkspaceId, EditError> {
        if let Some(parent) = parent
            && !self.workspaces.contains_key(parent)
        {
            return Err(EditError::UnknownWorkspace(parent));
        }
        Ok(self.workspaces.insert(Workspace {
            name: name.to_owned(),
            parent,
            pos,
        }))
    }

    /// Adds a port to a workspace, below its others on that side. Its item is set by its first
    /// link.
    pub fn add_port(&mut self, workspace: WorkspaceId, side: Side) -> Result<NodeId, EditError> {
        if !self.workspaces.contains_key(workspace) {
            return Err(EditError::UnknownWorkspace(workspace));
        }
        let below = self
            .ports_of(workspace, side)
            .iter()
            .map(|&id| self.nodes[id].pos.y + 1.0)
            .fold(0.0, f32::max);
        Ok(self.nodes.insert(Node {
            kind: NodeKind::WorkspacePort { side, item: None },
            pos: Pos { x: 0.0, y: below },
            workspace: Some(workspace),
        }))
    }

    /// A workspace's ports on one side, in order (top to bottom).
    pub fn ports_of(&self, workspace: WorkspaceId, side: Side) -> Vec<NodeId> {
        let mut ports: Vec<(NodeId, f32)> = self
            .nodes
            .iter()
            .filter(|(_, node)| {
                node.workspace == Some(workspace)
                    && matches!(node.kind, NodeKind::WorkspacePort { side: s, .. } if s == side)
            })
            .map(|(id, node)| (id, node.pos.y))
            .collect();
        ports.sort_by(|a, b| a.1.total_cmp(&b.1));
        ports.into_iter().map(|(id, _)| id).collect()
    }

    /// Puts `members` in a new workspace, inside the one they are all directly in. Links that
    /// cross the new workspace's edge go through new ports: one per outside port feeding in, and
    /// one per inside port sending out.
    pub fn workspace_from(
        &mut self,
        members: &[Member],
        name: &str,
    ) -> Result<WorkspaceId, EditError> {
        let Some(&first) = members.first() else {
            return Err(EditError::EmptyWorkspace);
        };
        for &member in members {
            let exists = match member {
                Member::Node(id) => self.nodes.contains_key(id),
                Member::Workspace(id) => self.workspaces.contains_key(id),
            };
            if !exists {
                return Err(match member {
                    Member::Node(id) => EditError::UnknownNode(id),
                    Member::Workspace(id) => EditError::UnknownWorkspace(id),
                });
            }
        }
        let parent = self.parent_of(first);
        if members.iter().any(|m| self.parent_of(*m) != parent) {
            return Err(EditError::MixedWorkspaces);
        }
        // A workspace's own ports stay on its edge.
        if members.iter().any(|m| {
            matches!(m, Member::Node(id) if matches!(self.nodes[*id].kind, NodeKind::WorkspacePort { .. }))
        }) {
            return Err(EditError::PortOutsideWorkspace);
        }
        let pos = members
            .iter()
            .map(|m| self.member_pos(*m))
            .reduce(|a, b| Pos {
                x: a.x.min(b.x),
                y: a.y.min(b.y),
            })
            .unwrap_or_default();
        let workspace = self.workspaces.insert(Workspace {
            name: name.to_owned(),
            parent,
            pos,
        });
        for &member in members {
            self.set_parent(member, Some(workspace));
        }
        self.route_crossing_links();
        Ok(workspace)
    }

    /// Takes a workspace apart: what is in it moves up to its parent, and each port gives way to
    /// links straight from what fed it to what it fed.
    pub fn dissolve(&mut self, id: WorkspaceId) -> Result<(), EditError> {
        let workspace = self
            .workspaces
            .get(id)
            .cloned()
            .ok_or(EditError::UnknownWorkspace(id))?;
        let ports: Vec<NodeId> = [Side::Input, Side::Output]
            .into_iter()
            .flat_map(|side| self.ports_of(id, side))
            .collect();
        for port in ports {
            let incoming: Vec<Link> = self
                .links_at(port, Side::Input)
                .map(|(_, l)| l.clone())
                .collect();
            let outgoing: Vec<Link> = self
                .links_at(port, Side::Output)
                .map(|(_, l)| l.clone())
                .collect();
            self.remove_node(port);
            for before in &incoming {
                for after in &outgoing {
                    let mut link = Link::new(before.from, after.to, before.item.clone());
                    link.out_share = before.out_share;
                    link.in_share = after.in_share;
                    let duplicate = self
                        .links
                        .values()
                        .any(|l| l.from == link.from && l.to == link.to && l.item == link.item);
                    if !duplicate && link.from != link.to {
                        self.links.insert(link);
                    }
                }
            }
        }
        self.workspaces.remove(id);
        for node in self.nodes.values_mut() {
            if node.workspace == Some(id) {
                node.workspace = workspace.parent;
            }
        }
        for other in self.workspaces.values_mut() {
            if other.parent == Some(id) {
                other.parent = workspace.parent;
            }
        }
        Ok(())
    }

    /// Removes a workspace with every node and workspace in it, and their links.
    pub fn remove_workspace(&mut self, id: WorkspaceId) {
        for node in self.nodes_in_workspace(id) {
            self.remove_node(node);
        }
        let mut doomed = vec![id];
        while let Some(workspace) = doomed.pop() {
            doomed.extend(self.child_workspaces(workspace));
            self.workspaces.remove(workspace);
        }
    }

    pub fn rename_workspace(&mut self, id: WorkspaceId, name: &str) -> Result<(), EditError> {
        name.clone_into(
            &mut self
                .workspaces
                .get_mut(id)
                .ok_or(EditError::UnknownWorkspace(id))?
                .name,
        );
        Ok(())
    }

    pub fn set_workspace_pos(&mut self, id: WorkspaceId, pos: Pos) -> Result<(), EditError> {
        self.workspaces
            .get_mut(id)
            .ok_or(EditError::UnknownWorkspace(id))?
            .pos = pos;
        Ok(())
    }

    /// Every node in a workspace, nested ones and ports too, in graph order.
    pub fn nodes_in_workspace(&self, id: WorkspaceId) -> Vec<NodeId> {
        self.nodes
            .iter()
            .filter(|(_, node)| self.is_within(node.workspace, id))
            .map(|(node, _)| node)
            .collect()
    }

    /// Whether the workspace `inner` is `outer` or inside it.
    pub fn is_within(&self, inner: Option<WorkspaceId>, outer: WorkspaceId) -> bool {
        let mut current = inner;
        while let Some(workspace) = current {
            if workspace == outer {
                return true;
            }
            current = self.workspaces.get(workspace).and_then(|w| w.parent);
        }
        false
    }

    /// The workspace a link's end works in: a node's own, except at a port's outside end, which
    /// is in the port's parent. `None` at the top of the graph; the outer `None` for a node that
    /// doesn't exist.
    pub(crate) fn end_level(&self, node: NodeId, side: Side) -> Option<Option<WorkspaceId>> {
        let found = self.nodes.get(node)?;
        let outside = match found.kind {
            // Links come into a port in from outside, and leave a port out to outside.
            NodeKind::WorkspacePort {
                side: port_side, ..
            } => port_side == side,
            _ => false,
        };
        if outside {
            let workspace = found.workspace?;
            Some(self.workspaces.get(workspace)?.parent)
        } else {
            Some(found.workspace)
        }
    }

    /// The workspace a link lies in, when both its ends agree; `None` when they don't.
    pub(crate) fn link_level(&self, link: &Link) -> Option<Option<WorkspaceId>> {
        let from = self.end_level(link.from, Side::Output)?;
        let to = self.end_level(link.to, Side::Input)?;
        (from == to).then_some(from)
    }

    /// Copies of `members`, all in one workspace, with everything in the workspaces among them
    /// and the links between what is copied. What is copied sits at the top of the new graph.
    pub fn copy_members(&self, members: &[Member]) -> Graph {
        let mut nodes: Vec<NodeId> = Vec::new();
        let mut workspaces: Vec<WorkspaceId> = Vec::new();
        for &member in members {
            match member {
                Member::Node(id) if self.nodes.contains_key(id) => nodes.push(id),
                Member::Workspace(id) if self.workspaces.contains_key(id) => {
                    workspaces.extend(
                        self.workspaces
                            .keys()
                            .filter(|&w| self.is_within(Some(w), id)),
                    );
                    nodes.extend(self.nodes_in_workspace(id));
                }
                _ => {}
            }
        }
        let top: HashSet<Member> = members.iter().copied().collect();
        let mut copy = Graph::new(self.region.clone());
        let mut workspace_ids = HashMap::new();
        for &id in &workspaces {
            let mut workspace = self.workspaces[id].clone();
            workspace.parent = None;
            workspace_ids.insert(id, copy.workspaces.insert(workspace));
        }
        for &id in &workspaces {
            if !top.contains(&Member::Workspace(id)) {
                let parent = self.workspaces[id]
                    .parent
                    .and_then(|p| workspace_ids.get(&p).copied());
                copy.workspaces[workspace_ids[&id]].parent = parent;
            }
        }
        let mut ids = HashMap::new();
        for &id in &nodes {
            if ids.contains_key(&id) {
                continue;
            }
            let mut node = self.nodes[id].clone();
            node.workspace = if top.contains(&Member::Node(id)) {
                None
            } else {
                node.workspace.and_then(|w| workspace_ids.get(&w).copied())
            };
            ids.insert(id, copy.nodes.insert(node));
        }
        for link in self.links.values() {
            if let (Some(&from), Some(&to)) = (ids.get(&link.from), ids.get(&link.to)) {
                let copied = Link {
                    from,
                    to,
                    ..link.clone()
                };
                // A link to a port's outside end leaves with the port's workspace.
                if copy.link_level(&copied).is_some() {
                    copy.links.insert(copied);
                }
            }
        }
        copy.forget_unlinked_items();
        copy
    }

    /// Adds everything in `other` into the workspace `into` (`None`: the top of the graph), what
    /// sits at `other`'s top moved by `offset`. Ports can only go in a workspace, so at the top
    /// they are left out. Returns what was added at `into`'s level.
    pub fn paste_into(
        &mut self,
        catalog: &Catalog,
        other: &Graph,
        offset: Pos,
        into: Option<WorkspaceId>,
    ) -> Vec<Member> {
        let moved = |pos: Pos| Pos {
            x: pos.x + offset.x,
            y: pos.y + offset.y,
        };
        let mut pasted = Vec::new();
        let mut workspace_ids = HashMap::new();
        for (id, workspace) in &other.workspaces {
            let mut workspace = workspace.clone();
            if workspace.parent.is_none() {
                workspace.pos = moved(workspace.pos);
            }
            let new = self.workspaces.insert(workspace);
            workspace_ids.insert(id, new);
        }
        for (id, workspace) in &other.workspaces {
            let new = workspace_ids[&id];
            match workspace.parent {
                Some(parent) => self.workspaces[new].parent = Some(workspace_ids[&parent]),
                None => {
                    self.workspaces[new].parent = into;
                    pasted.push(Member::Workspace(new));
                }
            }
        }
        let mut ids = HashMap::new();
        for (id, node) in &other.nodes {
            let top = node.workspace.is_none();
            let is_port = matches!(node.kind, NodeKind::WorkspacePort { .. });
            if top && is_port && into.is_none() {
                continue;
            }
            let pos = if top { moved(node.pos) } else { node.pos };
            let mut kind = node.kind.clone();
            if top && let NodeKind::WorkspacePort { item, .. } = &mut kind {
                // A port pasted into a workspace starts unlinked, as a new one does.
                *item = None;
            }
            let new = self.nodes.insert(Node {
                kind,
                pos,
                workspace: match node.workspace {
                    Some(w) => Some(workspace_ids[&w]),
                    None => into,
                },
            });
            ids.insert(id, new);
            if top {
                pasted.push(Member::Node(new));
            }
        }
        for link in other.links.values() {
            let (Some(&from), Some(&to)) = (ids.get(&link.from), ids.get(&link.to)) else {
                continue;
            };
            let linked = self.connect(
                catalog,
                crate::graph::PortRef::new(from, link.item.clone()),
                crate::graph::PortRef::new(to, link.item.clone()),
            );
            if let Ok(id) = linked {
                self.links[id].out_share = link.out_share;
                self.links[id].in_share = link.in_share;
            }
        }
        pasted
    }

    /// Sends every link whose ends are in different workspaces through ports instead: out
    /// through a port of each workspace it leaves, in through a port of each it enters. A port
    /// is shared by the links of one item from one producer, so a fan-out stays one port.
    pub(crate) fn route_crossing_links(&mut self) {
        let crossing: Vec<LinkId> = self
            .links
            .iter()
            .filter(|(_, link)| self.link_level(link).is_none())
            .map(|(id, _)| id)
            .collect();
        let mut ports: HashMap<(WorkspaceId, Side, ItemId, NodeId), NodeId> = HashMap::new();
        for id in crossing {
            let Some(link) = self.links.remove(id) else {
                continue;
            };
            let (Some(from_level), Some(to_level)) = (
                self.end_level(link.from, Side::Output),
                self.end_level(link.to, Side::Input),
            ) else {
                continue;
            };
            let up = self.ancestors(from_level);
            let down = self.ancestors(to_level);
            // Workspaces the link leaves, innermost first, and enters, outermost first.
            let leaves: Vec<WorkspaceId> = up
                .iter()
                .take_while(|w| !down.contains(w))
                .copied()
                .collect();
            let mut enters: Vec<WorkspaceId> = down
                .iter()
                .take_while(|w| !up.contains(w))
                .copied()
                .collect();
            enters.reverse();
            let mut stops = vec![link.from];
            for (side, workspace) in leaves
                .iter()
                .map(|&w| (Side::Output, w))
                .chain(enters.iter().map(|&w| (Side::Input, w)))
            {
                let key = (workspace, side, link.item.clone(), link.from);
                let port = match ports.get(&key) {
                    Some(&port) => port,
                    None => {
                        let Ok(port) = self.add_port(workspace, side) else {
                            continue;
                        };
                        self.set_port_item(port, link.item.clone());
                        ports.insert(key, port);
                        port
                    }
                };
                stops.push(port);
            }
            stops.push(link.to);
            let last = stops.len() - 2;
            for (index, pair) in stops.windows(2).enumerate() {
                let mut part = Link::new(pair[0], pair[1], link.item.clone());
                if index == 0 {
                    part.out_share = link.out_share;
                }
                if index == last {
                    part.in_share = link.in_share;
                }
                let duplicate = self
                    .links
                    .values()
                    .any(|l| l.from == part.from && l.to == part.to && l.item == part.item);
                if !duplicate {
                    self.links.insert(part);
                }
            }
        }
    }

    /// `level` and every workspace around it, innermost first.
    fn ancestors(&self, level: Option<WorkspaceId>) -> Vec<WorkspaceId> {
        let mut chain = Vec::new();
        let mut current = level;
        while let Some(id) = current {
            chain.push(id);
            current = self.workspaces.get(id).and_then(|w| w.parent);
        }
        chain
    }

    fn child_workspaces(&self, id: WorkspaceId) -> Vec<WorkspaceId> {
        self.workspaces
            .iter()
            .filter(|(_, w)| w.parent == Some(id))
            .map(|(child, _)| child)
            .collect()
    }

    fn member_pos(&self, member: Member) -> Pos {
        match member {
            Member::Node(id) => self.nodes[id].pos,
            Member::Workspace(id) => self.workspaces[id].pos,
        }
    }

    fn set_parent(&mut self, member: Member, parent: Option<WorkspaceId>) {
        match member {
            Member::Node(id) => self.nodes[id].workspace = parent,
            Member::Workspace(id) => self.workspaces[id].parent = parent,
        }
    }

    fn set_port_item(&mut self, port: NodeId, item: ItemId) {
        if let Some(NodeKind::WorkspacePort { item: slot, .. }) =
            self.nodes.get_mut(port).map(|n| &mut n.kind)
        {
            *slot = Some(item);
        }
    }
}
