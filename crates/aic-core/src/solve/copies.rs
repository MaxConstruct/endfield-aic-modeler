//! Copies groups (groups, step 2). Each outermost copies group is solved once on its own, with its
//! ports on the depot, which gives one copy's rate through every port. The outer solve then sees
//! one stand-in node per group, whose activity is the number of copies.

use std::collections::HashMap;

use aic_data::{Catalog, ItemId};

use crate::graph::{Graph, GroupId, LinkId, NodeId, NodeKind, Port, Side};
use crate::group::GroupMode;
use crate::solve::{SectionStatus, Solution, Warning, solve};

/// One copies group as the outer solve sees it.
pub(crate) struct Module {
    pub group: GroupId,
    /// One copy's flow through the group's ports, added up per item and side.
    pub ports: Vec<(ItemId, Side, f64)>,
    pub limit: Option<f64>,
    /// One copy solved on its own; `None` when it couldn't be, which leaves the count undetermined.
    inner: Option<Solution>,
    /// The group's ports in the real graph.
    external: Vec<Port>,
    members: Vec<NodeId>,
}

impl Module {
    pub fn is_determined(&self) -> bool {
        self.inner.is_some()
    }
}

/// The graph with every outermost copies group folded into a stand-in node.
pub(crate) struct Folded {
    pub graph: Graph,
    /// Stand-in node → its group.
    pub modules: HashMap<NodeId, Module>,
}

/// Folds every copies group that isn't inside another one. Node and link IDs outside the groups
/// stay as they are.
pub(crate) fn fold(catalog: &Catalog, graph: &Graph) -> Folded {
    let mut folded = graph.clone();
    let mut modules = HashMap::new();
    let outermost: Vec<GroupId> = graph
        .groups()
        .iter()
        .filter(|(id, group)| {
            matches!(group.mode, GroupMode::Copies { .. }) && !inside_copies(graph, *id)
        })
        .map(|(id, _)| id)
        .collect();
    for group in outermost {
        let GroupMode::Copies { limit } = graph.groups()[group].mode else {
            continue;
        };
        let members = graph.nodes_in_group(group);
        if members.is_empty() {
            continue;
        }
        let inner = solve_one_copy(catalog, graph, group, &members);
        let external = graph.external_ports(catalog, group);
        let mut ports: Vec<(ItemId, Side, f64)> = Vec::new();
        if let Some(inner) = &inner {
            for port in &external {
                let rate = one_copy(inner, port).unwrap_or(0.0);
                match ports
                    .iter_mut()
                    .find(|(item, side, _)| *item == port.item && *side == port.side)
                {
                    Some((_, _, total)) => *total += rate,
                    None => ports.push((port.item.clone(), port.side, rate)),
                }
            }
        }
        // The model builds the stand-in from its `Module`. Its kind only has to be one that
        // stands as a section of its own when nothing links to it, as a recipe node does.
        let stand_in = folded.nodes.insert(crate::graph::Node {
            kind: NodeKind::Sink {
                item: ItemId::from(""),
                limit: None,
            },
            pos: graph.groups()[group].pos,
            group: None,
        });
        let crossing: Vec<LinkId> = folded
            .links
            .iter()
            .filter(|(_, l)| members.contains(&l.from) != members.contains(&l.to))
            .map(|(id, _)| id)
            .collect();
        for id in crossing {
            let link = &mut folded.links[id];
            if members.contains(&link.from) {
                link.from = stand_in;
            } else {
                link.to = stand_in;
            }
        }
        folded
            .links
            .retain(|_, l| !members.contains(&l.from) && !members.contains(&l.to));
        for &member in &members {
            folded.nodes.remove(member);
        }
        modules.insert(
            stand_in,
            Module {
                group,
                ports,
                limit,
                inner,
                external,
                members,
            },
        );
    }
    Folded {
        graph: folded,
        modules,
    }
}

fn inside_copies(graph: &Graph, group: GroupId) -> bool {
    let mut current = graph.groups()[group].parent;
    while let Some(id) = current {
        let info = &graph.groups()[id];
        if matches!(info.mode, GroupMode::Copies { .. }) {
            return true;
        }
        current = info.parent;
    }
    false
}

/// Solves what is in `group` alone, its ports on the depot. Copies groups nested inside are folded
/// by that solve in turn.
fn solve_one_copy(
    catalog: &Catalog,
    graph: &Graph,
    group: GroupId,
    members: &[NodeId],
) -> Option<Solution> {
    let mut alone = graph.clone();
    alone.nodes.retain(|id, _| members.contains(&id));
    alone
        .links
        .retain(|_, l| members.contains(&l.from) && members.contains(&l.to));
    alone.groups[group].mode = GroupMode::Folder;
    let solution = solve(catalog, &alone);
    solution
        .sections
        .iter()
        .all(|s| s.status == SectionStatus::Solved)
        .then_some(solution)
}

/// Puts the real graph's nodes back into a solution of the folded graph: each group's count of
/// copies, one copy's values inside it, and its ports' flows for all copies. Returns how much each
/// node inside a copies group counts for in the totals.
pub(crate) fn unfold(folded: &Folded, solution: &mut Solution) -> HashMap<NodeId, f64> {
    let mut scale = HashMap::new();
    for (stand_in, module) in &folded.modules {
        let count = solution.rate.remove(stand_in).flatten();
        solution.copies.insert(module.group, count);
        solution.throttle.remove(stand_in);
        solution.port_flow.retain(|port, _| port.node != *stand_in);
        solution.depot.retain(|port, _| port.node != *stand_in);
        for section in &mut solution.sections {
            if let Some(at) = section.nodes.iter().position(|n| n == stand_in) {
                section
                    .nodes
                    .splice(at..=at, module.members.iter().copied());
            }
        }
        if !module.is_determined() {
            solution.warnings.push(Warning::CopiesUndetermined {
                group: module.group,
            });
        }
        let Some(inner) = &module.inner else {
            for &node in &module.members {
                scale.insert(node, 0.0);
            }
            continue;
        };
        for &node in &module.members {
            scale.insert(node, count.unwrap_or(0.0));
        }
        solution.facility_count.extend(inner.facility_count.clone());
        solution.rate.extend(inner.rate.clone());
        solution.port_flow.extend(inner.port_flow.clone());
        solution.link_flow.extend(inner.link_flow.clone());
        solution.link_belts.extend(inner.link_belts.clone());
        solution.storage.extend(inner.storage.clone());
        solution.throttle.extend(inner.throttle.clone());
        solution.coupons.extend(inner.coupons.clone());
        solution.copies.extend(inner.copies.clone());
        solution.group_flow.extend(inner.group_flow.clone());
        solution.warnings.extend(
            inner
                .warnings
                .iter()
                .filter(|w| {
                    matches!(
                        w,
                        Warning::BeltOverCapacity { .. } | Warning::CopiesUndetermined { .. }
                    )
                })
                .cloned(),
        );
        // Nested copies groups count once per copy of this one.
        for (node, inner_scale) in inner_scale(inner, &module.members) {
            scale.insert(node, inner_scale * count.unwrap_or(0.0));
        }
        for port in &module.external {
            let per_copy = one_copy(inner, port);
            let all = per_copy.zip(count).map(|(rate, n)| rate * n);
            solution.group_flow.insert(port.clone(), all);
            // Ports the group shares by item and side are one port of the stand-in: if any of
            // them is linked outside, all of them are.
            if !outside_linked(folded, *stand_in, port) {
                solution.depot.insert(port.clone(), all);
            }
        }
    }
    scale
}

/// One copy's flow through a port of the group. A port of a copies group nested inside counts
/// all of that group's copies.
fn one_copy(inner: &Solution, port: &Port) -> Option<f64> {
    inner
        .group_flow
        .get(port)
        .or_else(|| inner.port_flow.get(port))
        .copied()
        .flatten()
}

/// Whether the stand-in has a link for this port's item and side, so the port isn't on the depot.
fn outside_linked(folded: &Folded, stand_in: NodeId, port: &Port) -> bool {
    folded.graph.links().values().any(|link| {
        link.item == port.item
            && match port.side {
                Side::Input => link.to == stand_in,
                Side::Output => link.from == stand_in,
            }
    })
}

/// How much each node inside counts for, per copy of the outer group: 1, or the count of a nested
/// copies group it is in.
fn inner_scale(inner: &Solution, members: &[NodeId]) -> Vec<(NodeId, f64)> {
    members
        .iter()
        .map(|&node| (node, inner.node_copies.get(&node).copied().unwrap_or(1.0)))
        .collect()
}
