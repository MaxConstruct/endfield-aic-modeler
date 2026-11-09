//! The linear program for one section.
//!
//! Variables: a facility count per recipe node, a rate per source, sink and logistics node, and a
//! flow per link. Depot flows are expressions of those. Every limit is an upper bound on a variable.

use std::collections::HashMap;

use aic_data::{Catalog, ItemId};
use good_lp::{Constraint, Expression, ProblemVariables, Variable, variable};

use crate::graph::{Graph, LinkId, NodeId, NodeKind, Port, Side};
use crate::solve::copies::Module;
use crate::solve::sections::Section;

pub(crate) struct Ctx<'a> {
    pub catalog: &'a Catalog,
    pub graph: &'a Graph,
    /// Stand-in nodes for copies groups (see `copies`).
    pub modules: &'a HashMap<NodeId, Module>,
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
}

/// Builds the LP for `section`. A `relaxed` port stops constraining its node, as if it were on the
/// depot; its links stay bound by their other ends. Throttle reasons use this to ask whether a port
/// holds its node back.
pub(crate) fn build(ctx: &Ctx<'_>, section: &Section, relaxed: Option<&Port>) -> Model {
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
        },
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
    has_throughput: bool,
}

impl Builder<'_, '_> {
    fn node(&mut self, node: NodeId) {
        if let Some(module) = self.ctx.modules.get(&node) {
            self.module(node, module);
            return;
        }
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
        let speed = catalog
            .facility(facility)
            .expect("validated: facility exists")
            .speed;
        let count = self.activity(node, limit);
        let mut fed_by_links = false;
        for (item, qty) in &recipe.inputs {
            let rate = recipe.rate_per_min(*qty, speed);
            fed_by_links |= self.side(node, item, Side::Input, count * rate);
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

    /// A copies group: its activity is the number of copies, each running one copy's port rates.
    fn module(&mut self, node: NodeId, module: &Module) {
        let copies = self.activity(node, module.limit);
        let mut fed_by_links = false;
        for (item, side, rate) in &module.ports {
            let linked = self.side(node, item, *side, copies * *rate);
            fed_by_links |= linked && *side == Side::Input;
        }
        if !fed_by_links {
            let outputs: f64 = module
                .ports
                .iter()
                .filter(|(_, side, _)| *side == Side::Output)
                .map(|(_, _, rate)| rate)
                .sum();
            self.add_throughput(copies * outputs);
        }
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
