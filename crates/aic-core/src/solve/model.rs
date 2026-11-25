//! The linear program for one section.
//!
//! Variables: a facility count per recipe node, a rate per source, sink and logistics node, and a
//! flow per link. Depot flows are expressions of those. Every limit is an upper bound on a variable.
//!
//! A facility's activation input (ADR 0013) is needed in full by each running facility. On a port
//! with no links it needs no variable: the depot supplies it, and it is added to the port's flow
//! after the solve. On a port fed by links, pass 1 ([`Program::Pass1`]) gives the node a whole
//! number of running facilities, which makes the section a mixed-integer program; every later
//! program takes that number as given ([`Program::Fixed`]) and is linear. A section without such a
//! port builds the same linear program as before activation inputs.
//!
//! A facility of whole units (ADR 0015) has nothing but activation inputs, and its count is the
//! facilities it runs: a whole number. Its limit is rounded down to one. Fed by links, pass 1 ties
//! its count to its whole number of running facilities; every later program fixes the count to
//! that number. With no links at all nothing ties it, and its count, bounded by a whole limit and
//! in the objective only, comes out whole.

use std::collections::HashMap;

use aic_data::{Catalog, ItemId};
use good_lp::{Constraint, Expression, ProblemVariables, Variable, variable};

use crate::graph::{Graph, LinkId, NodeId, NodeKind, Port, Side};
use crate::solve::sections::Section;
use crate::solve::{RoundUp, whole_within};

/// How far below the next whole number the running facilities must stay above the count, so that
/// a count of exactly 1 runs one facility, not two, and a count of 0 runs none. It must be well
/// above microlp's integrality tolerance (1e-6): a running count of 0.999999 would be taken for 1.
const GAP: f64 = 1e-5;

/// The most facilities one node may run in pass 1: microlp's whole numbers need finite bounds that
/// fit an `i32`, so a node without a limit gets this one. A solve that reaches it means the node
/// could run without end: the section is unbounded.
const MOST_RUNNING: f64 = 1e6;

/// How the nodes whose activation input has links are built (ADR 0013).
#[derive(Debug, Clone)]
pub(crate) enum Program {
    /// Pass 1: each runs a whole number of facilities, tied to its count from both sides.
    Pass1,
    /// Every later program: each runs the facilities pass 1 settled on, `running`, with its count
    /// kept inside them. The `scaled` node, if any, has its activation input scale with its count
    /// instead, as the data has it: the throttle analysis asks what holds that node back.
    Fixed {
        running: HashMap<NodeId, u32>,
        scaled: Option<NodeId>,
    },
}

/// How one activation port is built.
#[derive(Debug, Clone, Copy)]
enum Activation {
    /// No links (or relaxed): only the part that scales is in the program; each running facility's
    /// whole activation input is added to the port's flow after the solve.
    Depot,
    /// Pass 1: this whole number of facilities runs.
    Running(Variable),
    /// This many facilities run.
    Fixed(u32),
    /// The activation input scales with the count.
    Scaled,
}

pub(crate) struct Ctx<'a> {
    pub catalog: &'a Catalog,
    pub graph: &'a Graph,
}

pub(crate) struct Model {
    pub vars: ProblemVariables,
    pub constraints: Vec<Constraint>,
    /// Facility count of a recipe node; supply of a source; consumption of a sink; flow through a
    /// splitter, merger or control port.
    pub activity: HashMap<NodeId, Variable>,
    /// The limit on each node's activity, where it has one.
    pub limits: HashMap<NodeId, f64>,
    pub links: HashMap<LinkId, Variable>,
    /// Unconnected ports, with what flows through them (ADR 0001).
    pub depot: Vec<(Port, Expression)>,
    /// Every port, linked or not, with what flows through it.
    pub ports: Vec<(Port, Expression)>,
    /// The pass-1 objective (ADR 0004): what origins put out plus what sinks take in.
    pub throughput: Expression,
    /// Links that share one port: each fan-out and each fan-in.
    pub fan_groups: Vec<Vec<LinkId>>,
    /// Every port with links, and those links.
    pub linked_ports: HashMap<Port, Vec<LinkId>>,
    /// Pass 1: the whole number of facilities each node whose activation input has links runs.
    pub running: Vec<(NodeId, Variable)>,
    /// Those of `running` with no limit, bounded only by [`MOST_RUNNING`]. If a solve takes one
    /// there, the section is unbounded.
    pub open_running: Vec<Variable>,
    /// Activation ports with no links, the node's count and the activation input per running
    /// facility: what each running facility needs is added to the port's flow after the solve.
    pub depot_activation: Vec<(Port, Variable, f64)>,
    /// Recipe nodes on a facility of whole units (ADR 0015): their counts are whole numbers.
    pub whole_units: Vec<NodeId>,
}

/// Whether a solution runs one of `open_running` up to [`MOST_RUNNING`]: that node would run
/// without end, so the program is unbounded.
pub(crate) fn runs_without_end(open_running: &[Variable], value: impl Fn(Variable) -> f64) -> bool {
    (open_running.iter()).any(|&running| value(running) > MOST_RUNNING - 0.5)
}

/// Builds the LP for `section`, its activation inputs as `program` says. A `relaxed` port stops
/// constraining its node, as if it were on the depot; its links stay bound by their other ends.
/// Throttle reasons use this to ask whether a port holds its node back.
pub(crate) fn build(
    ctx: &Ctx<'_>,
    section: &Section,
    program: &Program,
    relaxed: Option<&Port>,
) -> Model {
    let mut linked_ports: HashMap<Port, Vec<LinkId>> = HashMap::new();
    for &id in &section.links {
        let link = &ctx.graph.links[id];
        for (node, side) in [(link.from, Side::Output), (link.to, Side::Input)] {
            let port = Port {
                node,
                item: link.item.clone(),
                side,
            };
            linked_ports.entry(port).or_default().push(id);
        }
    }

    let mut vars = ProblemVariables::new();
    let links: HashMap<LinkId, Variable> = section
        .links
        .iter()
        .map(|&id| (id, vars.add(variable().min(0))))
        .collect();
    // Sorted so the LP is built the same way every time: when several splits are equally good,
    // the same one wins on every solve.
    let mut fan_groups: Vec<Vec<LinkId>> = Vec::new();
    let mut constraints = Vec::new();
    for (port, ids) in &linked_ports {
        let (locked, free): (Vec<LinkId>, Vec<LinkId>) = ids
            .iter()
            .partition(|id| ctx.graph.links[**id].share(port.side).is_some());
        // A ratio lock: every locked link carries flow in proportion to its share.
        if let [first, rest @ ..] = locked.as_slice() {
            let first_share = ctx.graph.links[*first]
                .share(port.side)
                .expect("partitioned by share");
            for id in rest {
                let share = ctx.graph.links[*id]
                    .share(port.side)
                    .expect("partitioned by share");
                constraints.push((links[id] * first_share).eq(links[first] * share));
            }
        }
        // The even split is for the links the player left free.
        if free.len() > 1 {
            fan_groups.push(free);
        }
    }
    fan_groups.sort();

    let mut builder = Builder {
        ctx,
        relaxed,
        model: Model {
            vars,
            constraints,
            activity: HashMap::new(),
            limits: HashMap::new(),
            links,
            depot: Vec::new(),
            ports: Vec::new(),
            throughput: Expression::from(0.0),
            fan_groups,
            linked_ports,
            running: Vec::new(),
            open_running: Vec::new(),
            depot_activation: Vec::new(),
            whole_units: Vec::new(),
        },
        program,
        has_throughput: false,
    };
    for &node in &section.nodes {
        builder.node(node);
    }
    let mut model = builder.model;
    if !builder.has_throughput {
        // No origin and no sink: a closed loop. Run it as high as its limits allow.
        model.throughput = model.activity.values().map(|&v| Expression::from(v)).sum();
    }
    model
}

struct Builder<'a, 'b> {
    ctx: &'a Ctx<'b>,
    relaxed: Option<&'a Port>,
    model: Model,
    program: &'a Program,
    has_throughput: bool,
}

impl Builder<'_, '_> {
    fn node(&mut self, node: NodeId) {
        match &self.ctx.graph.nodes[node].kind {
            NodeKind::Recipe {
                recipe,
                facility,
                limit,
            } => self.recipe(node, recipe.as_str(), facility.as_str(), *limit),
            NodeKind::Source { item, limit } => {
                let supply = self.activity(node, Some(*limit));
                self.side(node, item, Side::Output, supply.into());
                self.add_throughput(supply.into());
            }
            NodeKind::Sink { item, limit } | NodeKind::OutpostOrder { item, limit, .. } => {
                let consumption = self.activity(node, *limit);
                self.side(node, item, Side::Input, consumption.into());
                self.add_throughput(consumption.into());
            }
            NodeKind::Splitter { .. } | NodeKind::Merger { .. } | NodeKind::ControlPort { .. } => {
                self.pass_through(node);
            }
            NodeKind::Storage { .. } => self.storage(node),
            NodeKind::WorkspacePort { .. } => self.workspace_port(node),
            NodeKind::MissingRecipe { .. } => {}
        }
    }

    fn activity(&mut self, node: NodeId, limit: Option<f64>) -> Variable {
        let mut definition = variable().min(0);
        if let Some(limit) = limit {
            definition = definition.max(limit);
            self.model.limits.insert(node, limit);
        }
        let var = self.model.vars.add(definition);
        self.model.activity.insert(node, var);
        var
    }

    fn recipe(&mut self, node: NodeId, recipe: &str, facility: &str, limit: Option<f64>) {
        let catalog = self.ctx.catalog;
        let recipe = catalog.recipe(recipe).expect("validated: recipe exists");
        let facility = catalog
            .facility(facility)
            .expect("validated: facility exists");
        let speed = facility.speed;
        let whole = facility.whole_units();
        // A facility of whole units runs no more than the whole units its limit holds (ADR 0015).
        let limit = if whole {
            limit.map(whole_within)
        } else {
            limit
        };
        let count = self.activity(node, limit);
        if whole {
            self.model.whole_units.push(node);
        }
        // One way of running for the node, shared by every activation input it has: a facility of
        // whole units may have several. Its ports are fed when any of them has links.
        let (mut has_activation, mut fed) = (false, false);
        for (item, rate) in recipe.input_rates(facility) {
            if rate.activation > 0.0 {
                has_activation = true;
                let port = Port {
                    node,
                    item: item.clone(),
                    side: Side::Input,
                };
                fed |= self.ties(&port);
            }
        }
        if whole {
            // No facility of the game's has one, but an output with links would tie the count of
            // whole units to something else too.
            fed |= (recipe.outputs.iter()).any(|(item, _)| {
                self.ties(&Port {
                    node,
                    item: item.clone(),
                    side: Side::Output,
                })
            });
        }
        let activation =
            (has_activation || whole).then(|| self.activation(node, count, limit, fed, whole));
        let mut fed_by_links = false;
        for (item, rate) in recipe.input_rates(facility) {
            let port = Port {
                node,
                item: item.clone(),
                side: Side::Input,
            };
            let linked = self.model.linked_ports.contains_key(&port);
            let (scaled, amount) = if rate.activation > 0.0 {
                let activation = activation.expect("made for a node with an activation input");
                match activation {
                    Activation::Depot => {
                        self.model
                            .depot_activation
                            .push((port, count, rate.activation));
                        (rate.scaled, count * rate.scaled)
                    }
                    Activation::Running(running) => {
                        (rate.scaled, count * rate.scaled + running * rate.activation)
                    }
                    Activation::Fixed(running) => (
                        rate.scaled,
                        count * rate.scaled + f64::from(running) * rate.activation,
                    ),
                    Activation::Scaled => {
                        let all = rate.scaled + rate.activation;
                        (all, count * all)
                    }
                }
            } else {
                // Not an activation input: as it always was.
                (rate.scaled, count * rate.scaled)
            };
            self.side(node, item, Side::Input, amount);
            // Links into a port of which nothing scales don't tie the count to them: a node fed
            // only so is still an origin, measured by what it puts out (ADR 0013).
            fed_by_links |= linked && scaled > 0.0;
        }
        for (item, qty) in &recipe.outputs {
            let rate = recipe.rate_per_min(*qty, speed);
            self.side(node, item, Side::Output, count * rate);
        }
        if !fed_by_links {
            // An origin: nothing in the graph feeds it. Measure it by what it puts out, or, for a
            // recipe with no outputs (a generator), by what it takes in.
            let measured = if recipe.outputs.is_empty() {
                &recipe.inputs
            } else {
                &recipe.outputs
            };
            let total: f64 = measured
                .iter()
                .map(|(_, qty)| recipe.rate_per_min(*qty, speed))
                .sum();
            self.add_throughput(count * total);
        }
    }

    /// How the activation ports of `node` are built, adding what they need: the whole number of
    /// running facilities in pass 1, or the bounds that keep the count inside the facilities pass
    /// 1 settled on. `fed` says whether some activation port has links (and isn't relaxed).
    /// `whole`: the facility runs in whole units, so its count is the running facilities.
    fn activation(
        &mut self,
        node: NodeId,
        count: Variable,
        limit: Option<f64>,
        fed: bool,
        whole: bool,
    ) -> Activation {
        match self.program {
            Program::Fixed { scaled, .. } if *scaled == Some(node) => Activation::Scaled,
            _ if !fed => Activation::Depot,
            Program::Pass1 if whole => {
                // `limit` is already whole.
                let most = limit.map_or(MOST_RUNNING, |limit| limit.min(MOST_RUNNING));
                let running = (self.model.vars).add(variable().integer().min(0).max(most));
                if most >= MOST_RUNNING {
                    self.model.open_running.push(running);
                }
                self.model.running.push((node, running));
                (self.model.constraints).push(Expression::from(running).eq(count));
                Activation::Running(running)
            }
            Program::Fixed { running, .. } if whole => {
                let running = running.get(&node).copied().unwrap_or(0);
                (self.model.constraints).push(Expression::from(count).eq(f64::from(running)));
                Activation::Fixed(running)
            }
            Program::Pass1 => {
                // Tied from both sides, so it can neither fall short of the count nor start a
                // facility the count has no work for. `running ≥ count` has no allowance for
                // solver noise of its own: every pass maximizes something and would use it up;
                // microlp already takes a whole number within its feasibility tolerance (1e-7) of
                // a constraint as meeting it.
                let most = limit.map_or(MOST_RUNNING, |limit| {
                    f64::from(RoundUp::of(limit).facilities).min(MOST_RUNNING)
                });
                let running = (self.model.vars).add(variable().integer().min(0).max(most));
                if most >= MOST_RUNNING {
                    self.model.open_running.push(running);
                }
                self.model.running.push((node, running));
                let constraints = &mut self.model.constraints;
                constraints.push(Expression::from(running).geq(count));
                constraints.push(Expression::from(running).leq(count + (1.0 - GAP)));
                Activation::Running(running)
            }
            Program::Fixed { running, .. } => {
                let running = running.get(&node).copied().unwrap_or(0);
                // Inside the facilities that run, so its count rounded up is still `running`.
                let constraints = &mut self.model.constraints;
                constraints.push(Expression::from(count).leq(f64::from(running)));
                if running >= 1 {
                    constraints.push(Expression::from(count).geq(f64::from(running) - 1.0 + GAP));
                }
                Activation::Fixed(running)
            }
        }
    }

    /// Whether `port` has links that constrain its node: it has some, and isn't relaxed.
    fn ties(&self, port: &Port) -> bool {
        self.model.linked_ports.contains_key(port) && self.relaxed != Some(port)
    }

    /// Splitters, mergers and control ports: what comes in goes out. An unconnected side is depot.
    fn pass_through(&mut self, node: NodeId) {
        let kind = &self.ctx.graph.nodes[node].kind;
        let limit = match kind {
            NodeKind::ControlPort { limit, .. } => Some(*limit),
            _ => None,
        };
        let Some(item) = self.logistics_item(node) else {
            return;
        };
        let flow = self.activity(node, limit);
        let fed_by_links = self.side(node, &item, Side::Input, flow.into());
        self.side(node, &item, Side::Output, flow.into());
        if !fed_by_links {
            self.add_throughput(flow.into());
        }
    }

    /// A workspace port: what comes in goes out, and an end with no links lets nothing through.
    fn workspace_port(&mut self, node: NodeId) {
        let Some(item) = self.logistics_item(node) else {
            return;
        };
        let linked = |side| {
            self.model.linked_ports.contains_key(&Port {
                node,
                item: item.clone(),
                side,
            })
        };
        // A closed end holds the port at 0: a limit like any other, so its section is settled.
        let closed = !(linked(Side::Input) && linked(Side::Output));
        let flow = self.activity(node, closed.then_some(0.0));
        self.closed_side(node, &item, Side::Input, flow.into());
        self.closed_side(node, &item, Side::Output, flow.into());
    }

    /// Ties one port to `amount`, as [`Builder::side`] does, except that with no links the port is
    /// closed rather than on the depot: `amount` must be 0. Returns whether the port has links.
    fn closed_side(&mut self, node: NodeId, item: &ItemId, side: Side, amount: Expression) -> bool {
        let port = Port {
            node,
            item: item.clone(),
            side,
        };
        if self.model.linked_ports.contains_key(&port) {
            return self.side(node, item, side, amount);
        }
        self.model.ports.push((port, amount.clone()));
        self.model.constraints.push(amount.eq(0.0));
        false
    }

    /// Storage takes whatever its input links bring and gives whatever its output links take; the
    /// difference is its balance. Its output side is an origin of its section.
    fn storage(&mut self, node: NodeId) {
        let Some(item) = self.logistics_item(node) else {
            return;
        };
        let out = Port {
            node,
            item,
            side: Side::Output,
        };
        if let Some(ids) = self.model.linked_ports.get(&out) {
            let total: Expression = ids
                .iter()
                .map(|id| Expression::from(self.model.links[id]))
                .sum();
            self.add_throughput(total);
        }
    }

    /// The item of a logistics node: its own, or else that of its links.
    fn logistics_item(&self, node: NodeId) -> Option<ItemId> {
        if let Some(item) = self.ctx.graph.nodes[node].kind.item() {
            return Some(item.clone());
        }
        self.model
            .linked_ports
            .keys()
            .find(|port| port.node == node)
            .map(|port| port.item.clone())
    }

    /// Ties one port to `amount`: its links carry exactly that much, or else the depot does.
    /// Returns whether the port has links.
    fn side(&mut self, node: NodeId, item: &ItemId, side: Side, amount: Expression) -> bool {
        let port = Port {
            node,
            item: item.clone(),
            side,
        };
        self.model.ports.push((port.clone(), amount.clone()));
        let Some(ids) = self.model.linked_ports.get(&port) else {
            self.model.depot.push((port, amount));
            return false;
        };
        if self.relaxed != Some(&port) {
            let carried: Expression = ids
                .iter()
                .map(|id| Expression::from(self.model.links[id]))
                .sum();
            self.model.constraints.push(carried.eq(amount));
        }
        true
    }

    fn add_throughput(&mut self, amount: Expression) {
        self.model.throughput += amount;
        self.has_throughput = true;
    }
}
