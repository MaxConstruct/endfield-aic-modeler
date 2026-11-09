//! Groups: named sets of nodes and groups that collapse into one node on the canvas. A view only:
//! the solver sees the flat graph.

use aic_data::Catalog;

use crate::graph::{EditError, Graph, GroupId, NodeId, Port, Pos, Side};

/// A named set of nodes, possibly including other groups.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub name: String,
    /// The group this one is directly in, if any.
    pub parent: Option<GroupId>,
    /// Shown as one node with the group's ports, instead of its members.
    pub collapsed: bool,
    /// Where the collapsed group sits on the canvas.
    pub pos: Pos,
    pub mode: GroupMode,
}

/// What a group means to the solver.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum GroupMode {
    /// A folder: the solver sees its nodes as if there were no group.
    #[default]
    Folder,
    /// The group stands for identical copies of what is in it. Limits inside define one copy; the
    /// solver finds how many copies run, up to `limit`.
    Copies { limit: Option<f64> },
}

/// Something that can be put in a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Member {
    Node(NodeId),
    Group(GroupId),
}

impl Graph {
    pub fn groups(&self) -> &slotmap::SlotMap<GroupId, Group> {
        &self.groups
    }

    pub fn group_info(&self, id: GroupId) -> Option<&Group> {
        self.groups.get(id)
    }

    /// The group a member is directly in.
    pub fn parent_of(&self, member: Member) -> Option<GroupId> {
        match member {
            Member::Node(id) => self.nodes.get(id)?.group,
            Member::Group(id) => self.groups.get(id)?.parent,
        }
    }

    /// Puts `members` in a new group, inside the group they are all directly in. The group starts
    /// expanded, at the members' top-left.
    pub fn group(&mut self, members: &[Member], name: &str) -> Result<GroupId, EditError> {
        let Some(&first) = members.first() else {
            return Err(EditError::EmptyGroup);
        };
        for &member in members {
            let exists = match member {
                Member::Node(id) => self.nodes.contains_key(id),
                Member::Group(id) => self.groups.contains_key(id),
            };
            if !exists {
                return Err(match member {
                    Member::Node(id) => EditError::UnknownNode(id),
                    Member::Group(id) => EditError::UnknownGroup(id),
                });
            }
        }
        let parent = self.parent_of(first);
        if members.iter().any(|m| self.parent_of(*m) != parent) {
            return Err(EditError::MixedGroups);
        }
        let pos = members
            .iter()
            .map(|m| self.member_pos(*m))
            .reduce(|a, b| Pos {
                x: a.x.min(b.x),
                y: a.y.min(b.y),
            })
            .unwrap_or_default();
        let group = self.groups.insert(Group {
            name: name.to_owned(),
            parent,
            collapsed: false,
            pos,
            mode: GroupMode::Folder,
        });
        for &member in members {
            self.set_parent(member, Some(group));
        }
        Ok(group)
    }

    /// Dissolves a group: its members move up to the group it was in.
    pub fn ungroup(&mut self, id: GroupId) -> Result<(), EditError> {
        let group = self.groups.remove(id).ok_or(EditError::UnknownGroup(id))?;
        for node in self.nodes.values_mut() {
            if node.group == Some(id) {
                node.group = group.parent;
            }
        }
        for other in self.groups.values_mut() {
            if other.parent == Some(id) {
                other.parent = group.parent;
            }
        }
        Ok(())
    }

    /// Removes a group with every node and group in it, and their links.
    pub fn remove_group(&mut self, id: GroupId) {
        for node in self.nodes_in_group(id) {
            self.remove_node(node);
        }
        let mut doomed = vec![id];
        while let Some(group) = doomed.pop() {
            doomed.extend(self.child_groups(group));
            self.groups.remove(group);
        }
    }

    pub fn set_group_collapsed(&mut self, id: GroupId, collapsed: bool) -> Result<(), EditError> {
        self.groups
            .get_mut(id)
            .ok_or(EditError::UnknownGroup(id))?
            .collapsed = collapsed;
        Ok(())
    }

    /// Makes a group a folder, or a stand-in for copies of what is in it.
    pub fn set_group_mode(&mut self, id: GroupId, mode: GroupMode) -> Result<(), EditError> {
        if let GroupMode::Copies { limit: Some(value) } = mode
            && !(value.is_finite() && value >= 0.0)
        {
            return Err(EditError::InvalidLimit(value));
        }
        self.groups
            .get_mut(id)
            .ok_or(EditError::UnknownGroup(id))?
            .mode = mode;
        Ok(())
    }

    pub fn rename_group(&mut self, id: GroupId, name: &str) -> Result<(), EditError> {
        name.clone_into(
            &mut self
                .groups
                .get_mut(id)
                .ok_or(EditError::UnknownGroup(id))?
                .name,
        );
        Ok(())
    }

    pub fn set_group_pos(&mut self, id: GroupId, pos: Pos) -> Result<(), EditError> {
        self.groups
            .get_mut(id)
            .ok_or(EditError::UnknownGroup(id))?
            .pos = pos;
        Ok(())
    }

    /// Every node in a group, nested ones too, in graph order.
    pub fn nodes_in_group(&self, id: GroupId) -> Vec<NodeId> {
        self.nodes
            .iter()
            .filter(|(_, node)| self.is_within(node.group, id))
            .map(|(node, _)| node)
            .collect()
    }

    /// Whether the group `inner` is `outer` or inside it.
    pub fn is_within(&self, inner: Option<GroupId>, outer: GroupId) -> bool {
        let mut current = inner;
        while let Some(group) = current {
            if group == outer {
                return true;
            }
            current = self.groups.get(group).and_then(|g| g.parent);
        }
        false
    }

    /// The collapsed group a node is drawn as: its outermost collapsed group, or `None` when the
    /// node shows on its own.
    pub fn shown_as(&self, node: NodeId) -> Option<GroupId> {
        let mut shown = None;
        let mut current = self.nodes.get(node)?.group;
        while let Some(group) = current {
            let info = self.groups.get(group)?;
            if info.collapsed {
                shown = Some(group);
            }
            current = info.parent;
        }
        shown
    }

    /// The collapsed group a group is drawn inside, if any: its outermost collapsed ancestor.
    pub fn group_shown_as(&self, id: GroupId) -> Option<GroupId> {
        let mut shown = None;
        let mut current = self.groups.get(id)?.parent;
        while let Some(group) = current {
            let info = self.groups.get(group)?;
            if info.collapsed {
                shown = Some(group);
            }
            current = info.parent;
        }
        shown
    }

    /// A group's ports: the ports of the nodes in it that no link joins to another node in it.
    /// A port linked to a node outside is one; so is a port with no links, which is depot I/O.
    /// In node order, inputs before outputs.
    pub fn external_ports(&self, catalog: &Catalog, id: GroupId) -> Vec<Port> {
        let inside = self.nodes_in_group(id);
        let mut ports = Vec::new();
        for &node in &inside {
            let kind = &self.nodes[node].kind;
            for side in [Side::Input, Side::Output] {
                for item in kind.port_items(catalog, side).into_iter().flatten() {
                    let linked_inside = self.links_at(node, side).any(|(_, link)| {
                        let other = match side {
                            Side::Input => link.from,
                            Side::Output => link.to,
                        };
                        link.item == item && inside.contains(&other)
                    });
                    if !linked_inside {
                        ports.push(Port { node, item, side });
                    }
                }
            }
        }
        ports.sort_by_key(|port| port.side);
        ports
    }

    fn child_groups(&self, id: GroupId) -> Vec<GroupId> {
        self.groups
            .iter()
            .filter(|(_, g)| g.parent == Some(id))
            .map(|(child, _)| child)
            .collect()
    }

    fn member_pos(&self, member: Member) -> Pos {
        match member {
            Member::Node(id) => self.nodes[id].pos,
            Member::Group(id) => self.groups[id].pos,
        }
    }

    fn set_parent(&mut self, member: Member, parent: Option<GroupId>) {
        match member {
            Member::Node(id) => self.nodes[id].group = parent,
            Member::Group(id) => self.groups[id].parent = parent,
        }
    }

    /// Removes a group that no longer has any nodes in it, and so on up.
    pub(crate) fn drop_if_empty(&mut self, id: GroupId) {
        if !self.nodes_in_group(id).is_empty() {
            return;
        }
        let parent = self.groups.get(id).and_then(|g| g.parent);
        let mut doomed = vec![id];
        while let Some(group) = doomed.pop() {
            doomed.extend(self.child_groups(group));
            self.groups.remove(group);
        }
        if let Some(parent) = parent {
            self.drop_if_empty(parent);
        }
    }
}
