//! Why a node runs below its limit.
//!
//! microlp gives no dual values, so this asks the LP directly: for each linked port of a throttled
//! node, would the node run higher if that port were on the depot? The port that frees it the most
//! is the reason. Then the limit behind it is found by following links from that port.

use std::collections::{HashMap, HashSet, VecDeque};

use good_lp::{Solution as _, SolverModel, microlp};

use crate::graph::{NodeId, NodeKind, Port, Side};
use crate::solve::ThrottleReason;
use crate::solve::model::{Ctx, build};
use crate::solve::passes::{Values, tolerance};
use crate::solve::sections::Section;

pub(crate) fn reasons(
    ctx: &Ctx<'_>,
    section: &Section,
    values: &Values,
) -> HashMap<NodeId, ThrottleReason> {
    let reference = build(ctx, section, None);
    let mut reasons = HashMap::new();
    for (&node, &limit) in &reference.limits {
        let current = values.activity[&node];
        if current >= limit - tolerance(limit) {
            continue;
        }
        let mut ports: Vec<&Port> = reference
            .linked_ports
            .keys()
            .filter(|p| p.node == node)
            .collect();
        ports.sort();
        let Some(baseline) = max_activity(ctx, section, node, None) else {
            continue;
        };
        let freeing = ports
            .iter()
            .filter_map(|port| Some((*port, max_activity(ctx, section, node, Some(port))?)))
            .filter(|(_, freed)| *freed > baseline + tolerance(baseline))
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(port, _)| port);
        // No single port holds it back; it shares a fan-out or fan-in with other nodes.
        let shared = || {
            ports.iter().copied().find(|port| {
                let links = &reference.linked_ports[*port];
                reference
                    .fan_groups
                    .iter()
                    .any(|group| links.iter().any(|l| group.contains(l)))
            })
        };
        let Some(port) = freeing.or_else(shared) else {
            continue;
        };
        let reason = ThrottleReason {
            item: port.item.clone(),
            side: port.side,
            limited_by: limited_by(ctx, section, values, &reference.limits, port),
        };
        reasons.insert(node, reason);
    }
    reasons
}

/// The most `node` could run, with `relaxed` on the depot. `None` if that can't be solved; infinite
/// if nothing bounds it.
fn max_activity(
    ctx: &Ctx<'_>,
    section: &Section,
    node: NodeId,
    relaxed: Option<&Port>,
) -> Option<f64> {
    let model = build(ctx, section, relaxed);
    let target = model.activity[&node];
    let mut problem = model.vars.maximise(target).using(microlp);
    for constraint in model.constraints {
        problem.add_constraint(constraint);
    }
    match problem.solve() {
        Ok(solution) => Some(solution.value(target)),
        Err(good_lp::ResolutionError::Unbounded) => Some(f64::INFINITY),
        Err(_) => None,
    }
}

/// The nearest node at its limit, following links away from `port`: upstream from an input,
/// downstream from an output. Storage stops the search, since it decouples its two sides.
fn limited_by(
    ctx: &Ctx<'_>,
    section: &Section,
    values: &Values,
    limits: &HashMap<NodeId, f64>,
    port: &Port,
) -> Option<NodeId> {
    let graph = ctx.graph;
    let upstream = port.side == Side::Input;
    let next = |link_id| {
        let link = &graph.links[link_id];
        if upstream { link.from } else { link.to }
    };
    let at_limit = |node: NodeId| {
        limits
            .get(&node)
            .is_some_and(|&limit| values.activity[&node] >= limit - tolerance(limit))
    };

    let mut seen = HashSet::from([port.node]);
    let mut queue: VecDeque<NodeId> = section
        .links
        .iter()
        .filter(|&&id| {
            let link = &graph.links[id];
            link.item == port.item
                && if upstream {
                    link.to == port.node
                } else {
                    link.from == port.node
                }
        })
        .map(|&id| next(id))
        .collect();
    while let Some(node) = queue.pop_front() {
        if !seen.insert(node) {
            continue;
        }
        if at_limit(node) {
            return Some(node);
        }
        if matches!(graph.nodes[node].kind, NodeKind::Storage { .. }) {
            continue;
        }
        for &id in &section.links {
            let link = &graph.links[id];
            let leads_on = if upstream {
                link.to == node
            } else {
                link.from == node
            };
            if leads_on {
                queue.push_back(next(id));
            }
        }
    }
    None
}
