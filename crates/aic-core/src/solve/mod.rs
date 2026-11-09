//! The solver: runs every node as high as its limits and supply allow (ADR 0003, ADR 0004).

mod copies;
mod model;
mod passes;
mod sections;
mod throttle;

use std::collections::{HashMap, HashSet};

use aic_data::{Catalog, FacilityId, ItemId};
use indexmap::IndexMap;

use crate::graph::{Graph, GroupId, LinkId, NodeId, NodeKind, Port, Side};
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
    /// How many belts (or pipes, for a fluid) each link's flow fills: 0.4 is 40% of one belt.
    pub link_belts: HashMap<LinkId, Option<f64>>,
    /// Flow through every port of every solved node, linked or not, per minute.
    pub port_flow: HashMap<Port, Option<f64>>,
    /// Flow through each unconnected port: pulled from the depot on an input, sent to it on an output.
    pub depot: HashMap<Port, Option<f64>>,
    pub storage: HashMap<NodeId, StorageFlow>,
    /// Coupons each solved outpost order earns, per minute.
    pub coupons: HashMap<NodeId, Vec<(ItemId, f64)>>,
    /// Nodes running below their limit, and why.
    pub throttle: HashMap<NodeId, ThrottleReason>,
    /// How many copies each copies group runs; `None` when one copy can't be solved on its own.
    pub copies: HashMap<GroupId, Option<f64>>,
    /// Flow through each port of a copies group for all its copies, per minute. `port_flow` has
    /// one copy's flow, as do the other values of the nodes inside.
    pub group_flow: HashMap<Port, Option<f64>>,
    /// For each node inside a copies group, how many copies of it run in all: the product of the
    /// counts of the copies groups around it. The totals count it that many times.
    pub node_copies: HashMap<NodeId, f64>,
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
    /// A copies group in it can't solve one copy on its own, so its count can't be settled.
    CopiesUndetermined,
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
    /// Whole facilities to build per facility type: each node's count rounded up, then added.
    pub facilities_rounded: IndexMap<FacilityId, u32>,
    /// Coupons earned by outpost orders, per minute.
    pub coupons: IndexMap<ItemId, f64>,
    /// Use of each raw resource of the graph's region: the output of mining recipe nodes, the
    /// supply of source nodes and pulls from the depot. Compare it with what the region can supply
    /// using [`check_resources`].
    pub raw_resource_use: IndexMap<ItemId, f64>,
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
    /// More of a raw resource is used than the region supplies: a hint, not an error. Comes from
    /// [`check_resources`], not from [`solve`].
    ResourceOverUse {
        item: ItemId,
        used: f64,
        available: f64,
    },
    /// One copy of a copies group can't be solved on its own: add a limit inside it.
    CopiesUndetermined {
        group: GroupId,
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
    let folded = copies::fold(catalog, graph);
    let flat = &folded.graph;
    let nodes: Vec<NodeId> = flat
        .nodes()
        .iter()
        .filter(|(id, node)| {
            !left_out_nodes.contains(id) && !matches!(node.kind, NodeKind::MissingRecipe { .. })
        })
        .map(|(id, _)| id)
        .collect();
    let usable: HashSet<NodeId> = nodes.iter().copied().collect();
    let links: Vec<LinkId> = flat
        .links()
        .iter()
        .filter(|(id, link)| {
            !left_out_links.contains(id) && usable.contains(&link.from) && usable.contains(&link.to)
        })
        .map(|(id, _)| id)
        .collect();

    let ctx = Ctx {
        catalog,
        graph: flat,
        modules: &folded.modules,
    };
    let mut solution = Solution {
        warnings: issues.into_iter().map(Warning::Issue).collect(),
        ..Solution::default()
    };
    for (index, section) in sections::split(flat, &nodes, &links)
        .into_iter()
        .enumerate()
    {
        let blocked = section.nodes.iter().any(|node| {
            folded
                .modules
                .get(node)
                .is_some_and(|module| !module.is_determined())
        });
        let outcome = if blocked {
            Outcome::Undetermined(UndeterminedReason::CopiesUndetermined)
        } else {
            passes::solve_section(&ctx, &section)
        };
        let status = match &outcome {
            Outcome::Solved(values) => {
                solution
                    .throttle
                    .extend(throttle::reasons(&ctx, &section, values));
                SectionStatus::Solved
            }
            Outcome::Undetermined(reason) => {
                // A copies group says why itself.
                if *reason != UndeterminedReason::CopiesUndetermined {
                    solution.warnings.push(Warning::Undetermined {
                        section: index,
                        reason: *reason,
                    });
                }
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
    record_storage(flat, &nodes, &mut solution);
    record_coupons(catalog, flat, &mut solution);
    count_belts(catalog, flat, &mut solution);
    solution.node_copies = copies::unfold(&folded, &mut solution);
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
    let (depot, ports) = match values {
        Some(values) => {
            let known = |list: &[(Port, f64)]| -> Vec<(Port, Option<f64>)> {
                list.iter()
                    .map(|(port, amount)| (port.clone(), Some(*amount)))
                    .collect()
            };
            (known(&values.depot), known(&values.ports))
        }
        None => {
            let model = model::build(ctx, section, None);
            let blank = |list: Vec<(Port, good_lp::Expression)>| -> Vec<(Port, Option<f64>)> {
                list.into_iter().map(|(port, _)| (port, None)).collect()
            };
            (blank(model.depot), blank(model.ports))
        }
    };
    solution.depot.extend(depot);
    solution.port_flow.extend(ports);
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

/// What each outpost order earns: its rate times the trade's coupons per item.
fn record_coupons(catalog: &Catalog, graph: &Graph, solution: &mut Solution) {
    for (id, node) in graph.nodes() {
        let NodeKind::OutpostOrder { recipe, .. } = &node.kind else {
            continue;
        };
        let (Some(Some(rate)), Some(trade)) =
            (solution.rate.get(&id), catalog.recipe(recipe.as_str()))
        else {
            continue;
        };
        let Some(&(_, per_trade)) = trade.inputs.first() else {
            continue;
        };
        let earned = trade
            .outputs
            .iter()
            .map(|(coupon, qty)| (coupon.clone(), rate * qty / per_trade))
            .collect();
        solution.coupons.insert(id, earned);
    }
}

/// Fills in `link_belts`, and warns about links that need more than one belt or pipe.
fn count_belts(catalog: &Catalog, graph: &Graph, solution: &mut Solution) {
    for (id, link) in graph.links() {
        let Some(&flow) = solution.link_flow.get(&id) else {
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
        let belts = flow.map(|flow| flow / capacity);
        solution.link_belts.insert(id, belts);
        let Some(belts) = belts else {
            continue;
        };
        let lanes = (belts - 1e-9).ceil();
        if lanes > 1.0 {
            solution.warnings.push(Warning::BeltOverCapacity {
                link: id,
                lanes: lanes as u32,
            });
        }
    }
}

fn totals(catalog: &Catalog, graph: &Graph, solution: &Solution) -> Totals {
    // A node inside a copies group counts once per copy.
    let times = |node: NodeId| solution.node_copies.get(&node).copied().unwrap_or(1.0);
    let mut totals = Totals {
        complete: solution
            .sections
            .iter()
            .all(|s| s.status == SectionStatus::Solved)
            && solution.copies.values().all(Option::is_some),
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
                    *totals.storage_balance.entry(item.clone()).or_default() += balance * times(id);
                }
            }
            NodeKind::Recipe {
                recipe, facility, ..
            } => {
                let Some(&Some(per_copy)) = solution.facility_count.get(&id) else {
                    continue;
                };
                let count = &(per_copy * times(id));
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
                // Each copy builds its own facilities: round one copy up, then count the copies.
                let whole = f64::from(RoundUp::of(per_copy).facilities) * times(id);
                *totals
                    .facilities_rounded
                    .entry(facility.clone())
                    .or_default() += whole.ceil() as u32;
            }
            NodeKind::OutpostOrder { .. } => {
                for (coupon, earned) in solution.coupons.get(&id).into_iter().flatten() {
                    *totals.coupons.entry(coupon.clone()).or_default() += earned * times(id);
                }
            }
            _ => {}
        }
    }
    totals.power_net_kw = totals.power_generation_kw - totals.power_draw_kw;
    totals.raw_resource_use = raw_resource_use(catalog, graph, solution);
    totals
}

/// Use of each raw resource of the graph's region, in data order. A resource nothing touches is
/// left out.
fn raw_resource_use(
    catalog: &Catalog,
    graph: &Graph,
    solution: &Solution,
) -> IndexMap<ItemId, f64> {
    let raw = catalog.raw_resources(graph.region());
    let mut used: HashMap<&ItemId, f64> = HashMap::new();
    let mut add = |item: &ItemId, amount: Option<f64>| {
        if let (Some(&raw_item), Some(amount)) = (raw.iter().find(|r| **r == item), amount) {
            *used.entry(raw_item).or_default() += amount;
        }
    };
    let times = |node: NodeId| solution.node_copies.get(&node).copied().unwrap_or(1.0);
    for (port, flow) in &solution.port_flow {
        let flow = &flow.map(|f| f * times(port.node));
        let mined = port.side == Side::Output
            && matches!(
                &graph.nodes()[port.node].kind,
                NodeKind::Recipe { recipe, .. }
                    if catalog.recipe(recipe.as_str()).is_some_and(|r| r.is_mining)
            );
        if mined {
            add(&port.item, *flow);
        }
    }
    for (port, amount) in &solution.depot {
        if port.side == Side::Input {
            add(&port.item, *amount);
        }
    }
    for (id, node) in graph.nodes() {
        if let NodeKind::Source { item, .. } = &node.kind {
            let rate = solution.rate.get(&id).copied().flatten();
            add(item, rate.map(|r| r * times(id)));
        }
    }
    raw.into_iter()
        .filter_map(|item| Some((item.clone(), used.get(item).copied()?)))
        .collect()
}

/// A facility count as whole facilities to build, and how busy each of them is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoundUp {
    pub facilities: u32,
    /// The count over the whole facilities, from 0 to 1: 1.75 facilities run 2 at 0.875.
    pub utilization: f64,
}

impl RoundUp {
    pub fn of(count: f64) -> Self {
        // Solver noise just above a whole number doesn't call for another facility.
        let facilities = (count - passes::tolerance(count)).ceil().max(0.0);
        let utilization = if facilities > 0.0 {
            (count / facilities).min(1.0)
        } else {
            0.0
        };
        Self {
            facilities: facilities as u32,
            utilization,
        }
    }
}

/// Compares raw resource use with what the region can supply per minute. A resource missing from
/// `available` is unlimited.
pub fn check_resources(totals: &Totals, available: &HashMap<ItemId, f64>) -> Vec<Warning> {
    totals
        .raw_resource_use
        .iter()
        .filter_map(|(item, &used)| {
            let &available = available.get(item)?;
            (used > available + passes::tolerance(available)).then(|| Warning::ResourceOverUse {
                item: item.clone(),
                used,
                available,
            })
        })
        .collect()
}
