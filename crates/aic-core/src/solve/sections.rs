//! Cuts a graph into sections at every storage node.
//!
//! A storage node's input side and output side belong to different sections (or the same one, if
//! another path joins them). Each section is solved as its own LP, so one that can't be solved
//! doesn't blank the others.

use std::collections::HashMap;

use crate::graph::{Graph, LinkId, NodeId, NodeKind};

pub(crate) struct Section {
    /// Nodes in graph order. A storage node appears in every section one of its sides is in.
    pub nodes: Vec<NodeId>,
    pub links: Vec<LinkId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Vertex {
    Whole(NodeId),
    StorageIn(NodeId),
    StorageOut(NodeId),
}

/// Splits the usable part of `graph` into sections. `nodes` and `links` must already leave out
/// anything the solver can't use, and every link must join two of `nodes`.
pub(crate) fn split(graph: &Graph, nodes: &[NodeId], links: &[LinkId]) -> Vec<Section> {
    let is_storage = |node: NodeId| matches!(graph.nodes[node].kind, NodeKind::Storage { .. });
    let source_end = |node| {
        if is_storage(node) {
            Vertex::StorageOut(node)
        } else {
            Vertex::Whole(node)
        }
    };
    let target_end = |node| {
        if is_storage(node) {
            Vertex::StorageIn(node)
        } else {
            Vertex::Whole(node)
        }
    };

    let mut sets = DisjointSets::default();
    for &link in links {
        let link = &graph.links[link];
        sets.union(source_end(link.from), target_end(link.to));
    }

    let mut sections: Vec<Section> = Vec::new();
    let mut by_root: HashMap<Vertex, usize> = HashMap::new();
    let mut section_of = |sets: &mut DisjointSets, vertex: Vertex, sections: &mut Vec<Section>| {
        let root = sets.find(vertex);
        *by_root.entry(root).or_insert_with(|| {
            sections.push(Section {
                nodes: Vec::new(),
                links: Vec::new(),
            });
            sections.len() - 1
        })
    };

    for &node in nodes {
        let kind = &graph.nodes[node].kind;
        let vertices: Vec<Vertex> = if is_storage(node) {
            vec![Vertex::StorageIn(node), Vertex::StorageOut(node)]
        } else {
            vec![Vertex::Whole(node)]
        };
        for vertex in vertices {
            let linked = sets.contains(vertex);
            // A recipe, source or sink stands alone as its own section; a logistics node or a
            // storage side without links has nothing to solve.
            if !linked && (kind.is_logistics() || is_storage(node)) {
                continue;
            }
            let index = section_of(&mut sets, vertex, &mut sections);
            if sections[index].nodes.last() != Some(&node) {
                sections[index].nodes.push(node);
            }
        }
    }
    for &link_id in links {
        let link = &graph.links[link_id];
        let index = section_of(&mut sets, source_end(link.from), &mut sections);
        sections[index].links.push(link_id);
    }
    sections
}

#[derive(Default)]
struct DisjointSets {
    parent: HashMap<Vertex, Vertex>,
}

impl DisjointSets {
    fn contains(&self, vertex: Vertex) -> bool {
        self.parent.contains_key(&vertex)
    }

    fn find(&mut self, vertex: Vertex) -> Vertex {
        let parent = *self.parent.entry(vertex).or_insert(vertex);
        if parent == vertex {
            return vertex;
        }
        let root = self.find(parent);
        self.parent.insert(vertex, root);
        root
    }

    fn union(&mut self, a: Vertex, b: Vertex) {
        let (root_a, root_b) = (self.find(a), self.find(b));
        if root_a != root_b {
            self.parent.insert(root_a, root_b);
        }
    }
}
