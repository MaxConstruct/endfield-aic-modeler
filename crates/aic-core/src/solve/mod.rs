//! The solver: runs every node as high as its limits and supply allow (ADR 0003, ADR 0004).

mod model;
mod passes;
mod sections;
mod throttle;

use std::collections::{HashMap, HashSet};

use aic_data::{Catalog, FacilityId, ItemId};
use indexmap::IndexMap;

use crate::graph::{Graph, LinkId, NodeId, NodeKind, Port, Side};
use crate::validate::Issue;
use model::Ctx;
use passes::{Outcome, Values};

/// Everything the UI shows for a graph. A value of `None` is undetermined and shows blank, never
/// as 0 or ∞. A node or link missing from a map has nothing to show (for example, a logistics
/// node without links).
#[derive(Debug, Clone, Default)]
pub struct Solution {
    pub sections: Vec<SectionResult>,
    /// Facility count of each recipe node, possibly fractional.
    pub facility_count: HashMap<NodeId, Option<f64>>,
    /// Supply of a source, consumption of a sink, or flow through a splitter, merger or control
    /// port, per minute.
    pub rate: HashMap<NodeId, Option<f64>>,
    pub link_flow: HashMap<LinkId, Option<f64>>,
    /// Flow through each unconnected port: pulled from the depot on an input, sent to it on an output.
    pub depot: HashMap<Port, Option<f64>>,
    pub storage: HashMap<NodeId, StorageFlow>,
    /// Nodes running below their limit, and why.
    pub throttle: HashMap<NodeId, ThrottleReason>,
    pub totals: Totals,
    pub warnings: Vec<Warning>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SectionResult {
    pub nodes: Vec<NodeId>,
    pub links: Vec<LinkId>,
    pub status: SectionStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SectionStatus {
    Solved,
    Undetermined(UndeterminedReason),
    /// The LP failed numerically. This is a bug; the section shows blank.
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UndeterminedReason {
    /// The section has no limit at all: add a source, a facility count limit, a sink limit or a
    /// control port.
    NoLimit,
    /// The section has limits, but some part of it can still grow without end.
    Unbounded,
}

/// Flows of a storage node, per minute.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct StorageFlow {
    pub inflow: Option<f64>,
    pub outflow: Option<f64>,
    /// Inflow minus outflow. Positive fills, negative drains.
    pub balance: Option<f64>,
    /// The part that goes straight through: the smaller of inflow and outflow.
    pub pass_through: Option<f64>,
}

/// Why a node runs below its limit: the port that holds it back, and the node at its limit
/// behind that port, if one is found.
#[derive(Debug, Clone, PartialEq)]
pub struct ThrottleReason {
    pub item: ItemId,
    /// `Input`: a shortage upstream. `Output`: a backup downstream.
    pub side: Side,
    pub limited_by: Option<NodeId>,
}

/// Sums over every solved section, per minute and in kW.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Totals {
    pub depot_in: IndexMap<ItemId, f64>,
    pub depot_out: IndexMap<ItemId, f64>,
    pub storage_balance: IndexMap<ItemId, f64>,
    pub power_draw_kw: f64,
    pub power_generation_kw: f64,
    /// Generation minus draw. Positive is a surplus.
    pub power_net_kw: f64,
    /// Facility count per facility type.
    pub facilities: IndexMap<FacilityId, f64>,
    /// False when some section is undetermined or failed, so these sums leave part of the graph out.
    pub complete: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Warning {
    Undetermined {
        section: usize,
        reason: UndeterminedReason,
    },
    Failed {
        section: usize,
        message: String,
    },
    /// A link carries more than one belt or pipe can: a hint, not an error.
    BeltOverCapacity {
        link: LinkId,
        lanes: u32,
    },
    /// A problem with the graph itself. The nodes and links involved are left out of the solve.
    Issue(Issue),
}

/// Solves `graph`. Never fails: a section that can't be solved shows blank with a warning, and the
/// other sections still get numbers.
pub fn solve(catalog: &Catalog, graph: &Graph) -> Solution {
    let issues = graph.validate(catalog);
    let mut left_out_nodes = HashSet::new();
    let mut left_out_links = HashSet::new();
    for issue in issues.iter().filter(|issue| issue.excludes()) {
        match issue {
            Issue::BadLink { link, .. } => {
                left_out_links.insert(*link);
            }
            Issue::MissingRecipe { node, .. }
            | Issue::UnknownItem { node, .. }
            | Issue::InvalidLimit { node } => {
                left_out_nodes.insert(*node);
            }
            _ => {}
        }
    }
    let nodes: Vec<NodeId> = graph
        .nodes()
        .iter()
        .filter(|(id, node)| {
            !left_out_nodes.contains(id) && !matches!(node.kind, NodeKind::MissingRecipe { .. })
        })
        .map(|(id, _)| id)
        .collect();
    let usable: HashSet<NodeId> = nodes.iter().copied().collect();
    let links: Vec<LinkId> = graph
        .links()
        .iter()
        .filter(|(id, link)| {
            !left_out_links.contains(id) && usable.contains(&link.from) && usable.contains(&link.to)
        })
        .map(|(id, _)| id)
        .collect();

    let ctx = Ctx { catalog, graph };
    let mut solution = Solution {
        warnings: issues.into_iter().map(Warning::Issue).collect(),
        ..Solution::default()
    };
    for (index, section) in sections::split(graph, &nodes, &links)
        .into_iter()
        .enumerate()
    {
        let outcome = passes::solve_section(&ctx, &section);
        let status = match &outcome {
            Outcome::Solved(values) => {
                solution
                    .throttle
                    .extend(throttle::reasons(&ctx, &section, values));
                SectionStatus::Solved
            }
            Outcome::Undetermined(reason) => {
                solution.warnings.push(Warning::Undetermined {
                    section: index,
                    reason: *reason,
                });
                SectionStatus::Undetermined(*reason)
            }
            Outcome::Failed(message) => {
                solution.warnings.push(Warning::Failed {
                    section: index,
                    message: message.clone(),
                });
                SectionStatus::Failed
            }
        };
        let values = match outcome {
            Outcome::Solved(values) => Some(values),
            _ => None,
        };
        record(&ctx, &section, values.as_ref(), &mut solution);
        solution.sections.push(SectionResult {
            nodes: section.nodes,
            links: section.links,
            status,
        });
    }
    record_storage(graph, &nodes, &mut solution);
    warn_over_capacity(catalog, graph, &mut solution);
    solution.totals = totals(catalog, graph, &solution);
    solution
}

/// Copies one section's values into the solution, or `None` everywhere if it has none.
fn record(
    ctx: &Ctx<'_>,
    section: &sections::Section,
    values: Option<&Values>,
    solution: &mut Solution,
) {
    let graph = ctx.graph;
    for &node in &section.nodes {
        let value = values.map(|v| v.activity.get(&node).copied());
        let target = match graph.nodes()[node].kind {
            NodeKind::Recipe { .. } => &mut solution.facility_count,
            NodeKind::Storage { .. } | NodeKind::MissingRecipe { .. } => continue,
            _ => &mut solution.rate,
        };
        // A logistics node with no item and no links has no variable; leave it out.
        if let Some(None) = value {
            continue;
        }
        target.insert(node, value.flatten());
    }
    for &link in &section.links {
        solution
            .link_flow
            .insert(link, values.map(|v| v.links[&link]));
    }
    let ports = match values {
        Some(values) => values
            .depot
            .iter()
            .map(|(port, amount)| (port.clone(), Some(*amount)))
            .collect(),
        None => model::build(ctx, section, None)
            .depot
            .into_iter()
            .map(|(port, _)| (port, None))
            .collect::<Vec<_>>(),
    };
    solution.depot.extend(ports);
}

fn record_storage(graph: &Graph, nodes: &[NodeId], solution: &mut Solution) {
    for &node in nodes {
        if !matches!(graph.nodes()[node].kind, NodeKind::Storage { .. }) {
            continue;
        }
        let sum = |side: Side| -> Option<f64> {
            graph
                .links_at(node, side)
                .map(|(id, _)| solution.link_flow.get(&id).copied().flatten())
                .sum()
        };
        let (inflow, outflow) = (sum(Side::Input), sum(Side::Output));
        let both = inflow.zip(outflow);
        solution.storage.insert(
            node,
            StorageFlow {
                inflow,
                outflow,
                balance: both.map(|(i, o)| i - o),
                pass_through: both.map(|(i, o)| i.min(o)),
            },
        );
    }
}

fn warn_over_capacity(catalog: &Catalog, graph: &Graph, solution: &mut Solution) {
    for (id, link) in graph.links() {
        let Some(Some(flow)) = solution.link_flow.get(&id) else {
            continue;
        };
        let fluid = catalog
            .item(link.item.as_str())
            .is_some_and(|item| item.is_fluid);
        let capacity = if fluid {
            catalog.pipe_per_min()
        } else {
            catalog.belt_per_min()
        };
        let lanes = (flow / capacity - 1e-9).ceil();
        if lanes > 1.0 {
            solution.warnings.push(Warning::BeltOverCapacity {
                link: id,
                lanes: lanes as u32,
            });
        }
    }
}

fn totals(catalog: &Catalog, graph: &Graph, solution: &Solution) -> Totals {
    let mut totals = Totals {
        complete: solution
            .sections
            .iter()
            .all(|s| s.status == SectionStatus::Solved),
        ..Totals::default()
    };
    let mut depot: Vec<(&Port, f64)> = solution
        .depot
        .iter()
        .filter_map(|(port, amount)| Some((port, (*amount)?)))
        .collect();
    // Graph order, so the totals panel doesn't reshuffle between solves.
    depot.sort_by_key(|(port, _)| *port);
    for (port, amount) in depot {
        let map = match port.side {
            Side::Input => &mut totals.depot_in,
            Side::Output => &mut totals.depot_out,
        };
        *map.entry(port.item.clone()).or_default() += amount;
    }
    for (id, node) in graph.nodes() {
        match &node.kind {
            NodeKind::Storage { item: Some(item) } => {
                if let Some(balance) = solution.storage.get(&id).and_then(|s| s.balance) {
                    *totals.storage_balance.entry(item.clone()).or_default() += balance;
                }
            }
            NodeKind::Recipe {
                recipe, facility, ..
            } => {
                let Some(Some(count)) = solution.facility_count.get(&id) else {
                    continue;
                };
                let (Some(recipe), Some(found)) = (
                    catalog.recipe(recipe.as_str()),
                    catalog.facility(facility.as_str()),
                ) else {
                    continue;
                };
                let power = count * recipe.power_override_kw.unwrap_or(found.power_kw);
                if power >= 0.0 {
                    totals.power_draw_kw += power;
                } else {
                    totals.power_generation_kw -= power;
                }
                *totals.facilities.entry(facility.clone()).or_default() += count;
            }
            _ => {}
        }
    }
    totals.power_net_kw = totals.power_generation_kw - totals.power_draw_kw;
    totals
}
