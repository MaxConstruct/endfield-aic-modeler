//! The solver: runs every node as high as its limits and supply allow (ADR 0003, ADR 0004).

mod model;
mod passes;
mod sections;
mod throttle;

use std::collections::{HashMap, HashSet};

use aic_data::{Catalog, FacilityId, ItemId};
use indexmap::IndexMap;

use crate::graph::{Graph, Link, LinkId, NodeId, NodeKind, Port, Side};
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
    /// A workspace port has no link at one end, so nothing flows through it. `outside`: the end
    /// facing the workspace's parent; otherwise the end facing inside.
    PortUnlinked {
        port: NodeId,
        outside: bool,
    },
    /// A recipe node needs the environment of `gas`, and no node of the graph that creates it runs
    /// a unit (ADR 0015). The recipe runs anyway; its numbers are as without the warning.
    EnvironmentMissing {
        node: NodeId,
        gas: ItemId,
    },
    /// The graph's array nodes need more of one projection, a T1 array by facility ID, than its
    /// forge nodes project (ADR 0015): each array node's count rounded up, times what its array is
    /// stacked from, against each forge node's units times what its recipe projects.
    ProjectionsShort {
        projection: FacilityId,
        needed: u32,
        provided: u32,
    },
    /// A problem with the graph itself. The nodes and links involved are left out of the solve.
    Issue(Issue),
}

/// Solves `graph`. Never fails: a section that can't be solved shows blank with a warning, and the
/// other sections still get numbers.
pub fn solve(catalog: &Catalog, graph: &Graph) -> Solution {
    solve_keeping(catalog, graph, &mut KeptSections::default())
}

/// What each section of the last solve came to, so that the next solve keeps it if the section's
/// nodes, links and limits are as they were, and solves only the others. Its result is exactly
/// what [`solve`] gives.
///
/// The results hold for one catalog: [`KeptSections::clear`] it when the catalog changes.
#[derive(Default)]
pub struct KeptSections {
    /// Each section of the last solve, by its nodes.
    sections: HashMap<Vec<NodeId>, Kept>,
    /// How many sections the last solve kept.
    kept: usize,
}

impl KeptSections {
    /// How many sections the last solve kept rather than solved again.
    pub fn kept(&self) -> usize {
        self.kept
    }

    /// Forgets every section, so the next solve solves them all.
    pub fn clear(&mut self) {
        self.sections.clear();
        self.kept = 0;
    }
}

impl std::fmt::Debug for KeptSections {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeptSections")
            .field("sections", &self.sections.len())
            .field("kept", &self.kept)
            .finish()
    }
}

/// One section as it was solved: what the solve read of the graph, and what it came to.
struct Kept {
    /// The kind of each of the section's nodes, in section order: its limits are in it.
    kinds: Vec<NodeKind>,
    /// The section's links, in section order.
    links: Vec<(LinkId, Link)>,
    outcome: Outcome,
    throttle: HashMap<NodeId, ThrottleReason>,
}

impl Kept {
    fn solve(ctx: &Ctx<'_>, section: &sections::Section) -> Self {
        let outcome = passes::solve_section(ctx, section);
        let throttle = match &outcome {
            Outcome::Solved(values) => throttle::reasons(ctx, section, values),
            _ => HashMap::new(),
        };
        Self {
            kinds: (section.nodes.iter())
                .map(|&node| ctx.graph.nodes[node].kind.clone())
                .collect(),
            links: (section.links.iter())
                .map(|&link| (link, ctx.graph.links[link].clone()))
                .collect(),
            outcome,
            throttle,
        }
    }

    /// Whether `section` of `graph` is what this was solved from. Its nodes are the key it was
    /// found by.
    fn holds_for(&self, graph: &Graph, section: &sections::Section) -> bool {
        let nodes = (section.nodes.iter()).map(|&node| &graph.nodes[node].kind);
        let links = (section.links.iter()).map(|&link| (link, &graph.links[link]));
        self.kinds.iter().eq(nodes) && (self.links.iter()).map(|(id, link)| (*id, link)).eq(links)
    }
}

/// Solves `graph` as [`solve`] does, keeping the result of every section that is as it was at the
/// last solve with `kept`.
pub fn solve_keeping(catalog: &Catalog, graph: &Graph, kept: &mut KeptSections) -> Solution {
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
    solution.warnings.extend(unlinked_ports(graph));
    let mut previous = std::mem::take(&mut kept.sections);
    kept.kept = 0;
    for (index, section) in sections::split(graph, &nodes, &links)
        .into_iter()
        .enumerate()
    {
        let result = match previous.remove(&section.nodes) {
            Some(result) if result.holds_for(graph, &section) => {
                kept.kept += 1;
                result
            }
            _ => Kept::solve(&ctx, &section),
        };
        let status = match &result.outcome {
            Outcome::Solved(_) => {
                solution
                    .throttle
                    .extend((result.throttle.iter()).map(|(node, reason)| (*node, reason.clone())));
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
        let values = match &result.outcome {
            Outcome::Solved(values) => Some(values),
            _ => None,
        };
        record(&ctx, &section, values, &mut solution);
        kept.sections.insert(section.nodes.clone(), result);
        solution.sections.push(SectionResult {
            nodes: section.nodes,
            links: section.links,
            status,
        });
    }
    record_storage(graph, &nodes, &mut solution);
    record_coupons(catalog, graph, &mut solution);
    count_belts(catalog, graph, &mut solution);
    let area_warnings: Vec<Warning> = environments_missing(catalog, graph, &solution)
        .chain(projections_short(catalog, graph, &solution))
        .collect();
    solution.warnings.extend(area_warnings);
    solution.totals = totals(catalog, graph, &solution);
    solution
}

/// A warning for every recipe node, in graph order, that needs a gas environment no node of the
/// graph creates: none whose recipe spreads that gas runs a unit, or may (its count undetermined).
fn environments_missing<'a>(
    catalog: &'a Catalog,
    graph: &'a Graph,
    solution: &'a Solution,
) -> impl Iterator<Item = Warning> + 'a {
    let recipes = || {
        (graph.nodes().iter()).filter_map(|(id, node)| match &node.kind {
            NodeKind::Recipe { recipe, .. } => Some((id, catalog.recipe(recipe.as_str())?)),
            _ => None,
        })
    };
    let created: HashSet<&ItemId> = recipes()
        .filter_map(|(id, recipe)| {
            let gas = recipe.creates_environment.as_ref()?;
            let runs = match solution.facility_count.get(&id) {
                Some(Some(count)) => *count >= 1.0 - passes::tolerance(1.0),
                // Undetermined: it may run.
                Some(None) => true,
                // Left out of the solve.
                None => false,
            };
            runs.then_some(gas)
        })
        .collect();
    recipes().filter_map(move |(node, recipe)| {
        let gas = recipe.environment.as_ref()?;
        (!created.contains(gas)).then(|| Warning::EnvironmentMissing {
            node,
            gas: gas.clone(),
        })
    })
}

/// What the graph's array and forge nodes come to for one projection.
#[derive(Default)]
struct Projection {
    needed: u32,
    provided: u32,
    /// Some array or forge node of it is undetermined, so it isn't checked.
    unknown: bool,
}

/// A warning for every projection the arrays need more of than the forges project, in the
/// catalog's order of facilities. A projection with an undetermined node isn't checked.
fn projections_short<'a>(
    catalog: &'a Catalog,
    graph: &Graph,
    solution: &Solution,
) -> impl Iterator<Item = Warning> + 'a {
    let mut projections: HashMap<&FacilityId, Projection> = HashMap::new();
    for (id, node) in graph.nodes() {
        let NodeKind::Recipe {
            recipe, facility, ..
        } = &node.kind
        else {
            continue;
        };
        let (Some(recipe), Some(facility), Some(&count)) = (
            catalog.recipe(recipe.as_str()),
            catalog.facility(facility.as_str()),
            solution.facility_count.get(&id),
        ) else {
            // Left out of the solve: it neither needs nor projects anything.
            continue;
        };
        // An array needs its stack for each facility its count rounds up to; a forge projects for
        // each whole unit it runs.
        let arrays = count.map(|count| RoundUp::of(count).facilities);
        for (projection, n) in &facility.stack {
            let tally = projections.entry(projection).or_default();
            match arrays {
                Some(arrays) => tally.needed += arrays * n,
                None => tally.unknown = true,
            }
        }
        let units = count.map(|count| whole_within(count) as u32);
        for (projection, n) in &recipe.projects {
            let tally = projections.entry(projection).or_default();
            match units {
                Some(units) => tally.provided += units * n,
                None => tally.unknown = true,
            }
        }
    }
    let mut short: Vec<(&FacilityId, Projection)> = (projections.into_iter())
        .filter(|(_, tally)| !tally.unknown && tally.needed > tally.provided)
        .collect();
    short.sort_by_key(|(projection, _)| catalog.facilities().get_index_of(*projection));
    short
        .into_iter()
        .map(|(projection, tally)| Warning::ProjectionsShort {
            projection: projection.clone(),
            needed: tally.needed,
            provided: tally.provided,
        })
}

/// A warning for each workspace port with an end that has no links: its outside end first.
fn unlinked_ports(graph: &Graph) -> Vec<Warning> {
    graph
        .nodes()
        .iter()
        .filter_map(|(id, node)| {
            let NodeKind::WorkspacePort { side, .. } = node.kind else {
                return None;
            };
            let inside = match side {
                Side::Input => Side::Output,
                Side::Output => Side::Input,
            };
            let unlinked = |end: Side| graph.links_at(id, end).next().is_none();
            if unlinked(side) {
                Some(Warning::PortUnlinked {
                    port: id,
                    outside: true,
                })
            } else if unlinked(inside) {
                Some(Warning::PortUnlinked {
                    port: id,
                    outside: false,
                })
            } else {
                None
            }
        })
        .collect()
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
            let model = model::build(ctx, section, &model::Program::Pass1, None);
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
            .is_some_and(|item| item.is_fluid());
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
        // A port that moves nothing, such as an idle branch's, isn't worth listing.
        .filter(|(_, amount)| amount.abs() > passes::tolerance(*amount))
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
                let Some(&Some(count)) = solution.facility_count.get(&id) else {
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
                *totals
                    .facilities_rounded
                    .entry(facility.clone())
                    .or_default() += RoundUp::of(count).facilities;
            }
            NodeKind::OutpostOrder { .. } => {
                for (coupon, earned) in solution.coupons.get(&id).into_iter().flatten() {
                    *totals.coupons.entry(coupon.clone()).or_default() += earned;
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
    for (port, flow) in &solution.port_flow {
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
            add(item, rate);
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

/// The whole facilities `count` holds, rounded down: a limit of 1.5 runs one unit of a facility of
/// whole units (ADR 0015). Solver noise just below a whole number counts as that number.
pub(crate) fn whole_within(count: f64) -> f64 {
    (count + passes::tolerance(count)).floor().max(0.0)
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
