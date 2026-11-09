//! The two passes that solve one section (ADR 0004).
//!
//! 1. Maximize throughput: what the section's origins put out plus what its sinks take in.
//! 2. Hold throughput at that optimum and split every fan-out and fan-in as evenly as possible.
//!
//! When throughput has no ceiling (the section has limits, but some part can grow from the depot
//! without end), pass 1 settles the section by its limits instead (ADR 0006): run every limited
//! node as close to its limit as it gets, then every other node as low as its links allow.

use std::collections::HashMap;

use good_lp::{Expression, ResolutionError, Solution as _, SolverModel, microlp, variable};

use crate::graph::{LinkId, NodeId, Port};
use crate::solve::UndeterminedReason;
use crate::solve::model::{Ctx, Model, build};
use crate::solve::sections::Section;

/// Solved values for one section.
pub(crate) struct Values {
    pub activity: HashMap<NodeId, f64>,
    pub links: HashMap<LinkId, f64>,
    pub depot: Vec<(Port, f64)>,
    pub ports: Vec<(Port, f64)>,
}

pub(crate) enum Outcome {
    Solved(Values),
    Undetermined(UndeterminedReason),
    /// The LP failed for a reason that should not happen with upper-bound limits. A bug.
    Failed(String),
}

/// How close to a bound or an optimum counts as reaching it.
pub(crate) fn tolerance(value: f64) -> f64 {
    1e-7 * value.abs().max(1.0)
}

pub(crate) fn solve_section(ctx: &Ctx<'_>, section: &Section) -> Outcome {
    let first = build(ctx, section, None);
    if first.limits.is_empty() {
        return Outcome::Undetermined(UndeterminedReason::NoLimit);
    }
    let objective = first.throughput.clone();
    let (values, settled) = match run(first, objective, Direction::Maximise, Vec::new()) {
        Ok((values, optimum)) => (values, Settled::Throughput(optimum)),
        Err(ResolutionError::Unbounded) => match settle_by_limits(ctx, section) {
            Ok(settled) => settled,
            Err(ResolutionError::Unbounded) => {
                return Outcome::Undetermined(UndeterminedReason::Unbounded);
            }
            Err(error) => return Outcome::Failed(error.to_string()),
        },
        Err(error) => return Outcome::Failed(error.to_string()),
    };

    let mut second = build(ctx, section, None);
    if second.fan_groups.is_empty() {
        return Outcome::Solved(values);
    }
    // Unevenness of a group: its largest link minus the group's mean. Minimizing it fills the links
    // like water: a branch that can't take an equal share takes what it can, the rest share evenly.
    let mut unevenness = Expression::from(0.0);
    let mut extra = Vec::new();
    for group in &second.fan_groups {
        let largest = second.vars.add(variable().min(0));
        let share = 1.0 / group.len() as f64;
        for link in group {
            let flow = second.links[link];
            extra.push(Expression::from(largest).geq(flow));
            unevenness -= flow * share;
        }
        unevenness += largest;
    }
    // Pass 2 must not trade what pass 1 settled for evenness.
    extra.extend(settled.keep(&second));
    match run(second, unevenness, Direction::Minimise, extra) {
        Ok((even, _)) => Outcome::Solved(even),
        // Numerical trouble in pass 2 only costs evenness; pass 1's answer is still right.
        Err(_) => Outcome::Solved(values),
    }
}

/// What pass 1 settled, which pass 2 keeps while it evens out the splits.
enum Settled {
    /// Throughput reached this optimum.
    Throughput(f64),
    /// Throughput had no ceiling, so the limits settled it (ADR 0006): limited nodes got as close
    /// to their limits as `drivers` (their activity over their limit, added up), and the other
    /// nodes came down to `least` (their activity, added up).
    Limits { drivers: f64, least: f64 },
}

impl Settled {
    fn keep(&self, model: &Model) -> Vec<good_lp::Constraint> {
        match *self {
            Self::Throughput(optimum) => {
                vec![model.throughput.clone().geq(optimum - slack(optimum))]
            }
            Self::Limits { drivers, least } => vec![
                drivers_reach(model).geq(drivers - slack(drivers)),
                others_run(model).leq(least + slack(least)),
            ],
        }
    }
}

/// How far below an optimum a later pass may go: a hundredth of the tolerance.
fn slack(optimum: f64) -> f64 {
    tolerance(optimum) * 1e-2
}

/// How close the limited nodes are to their limits, added up: activity over limit.
fn drivers_reach(model: &Model) -> Expression {
    model
        .limits
        .iter()
        .filter(|(_, limit)| **limit > 0.0)
        .map(|(node, limit)| model.activity[node] * (1.0 / limit))
        .sum()
}

/// How much the nodes without a limit run, added up.
fn others_run(model: &Model) -> Expression {
    model
        .activity
        .iter()
        .filter(|(node, _)| !model.limits.contains_key(node))
        .map(|(_, var)| Expression::from(*var))
        .sum()
}

/// Pass 1 when throughput has no ceiling: run every limited node as close to its limit as it
/// gets, then every node without a limit as low as its links allow. A node nothing asks anything
/// of runs at 0 rather than without end.
fn settle_by_limits(
    ctx: &Ctx<'_>,
    section: &Section,
) -> Result<(Values, Settled), ResolutionError> {
    let first = build(ctx, section, None);
    let reach = drivers_reach(&first);
    let (_, drivers) = run(first, reach, Direction::Maximise, Vec::new())?;
    let second = build(ctx, section, None);
    let hold = vec![drivers_reach(&second).geq(drivers - slack(drivers))];
    let others = others_run(&second);
    let (values, least) = run(second, others, Direction::Minimise, hold)?;
    Ok((values, Settled::Limits { drivers, least }))
}

enum Direction {
    Maximise,
    Minimise,
}

fn run(
    model: Model,
    objective: Expression,
    direction: Direction,
    extra: Vec<good_lp::Constraint>,
) -> Result<(Values, f64), ResolutionError> {
    let Model {
        vars,
        constraints,
        activity,
        links,
        depot,
        ports,
        ..
    } = model;
    let unsolved = match direction {
        Direction::Maximise => vars.maximise(objective.clone()),
        Direction::Minimise => vars.minimise(objective.clone()),
    };
    let mut problem = unsolved.using(microlp);
    for constraint in constraints.into_iter().chain(extra) {
        problem.add_constraint(constraint);
    }
    let solution = problem.solve()?;
    let clean = |value: f64| if value.abs() < 1e-9 { 0.0 } else { value };
    let values = Values {
        activity: activity
            .into_iter()
            .map(|(node, var)| (node, clean(solution.value(var))))
            .collect(),
        links: links
            .into_iter()
            .map(|(link, var)| (link, clean(solution.value(var))))
            .collect(),
        depot: depot
            .into_iter()
            .map(|(port, amount)| (port, clean(solution.eval(&amount))))
            .collect(),
        ports: ports
            .into_iter()
            .map(|(port, amount)| (port, clean(solution.eval(&amount))))
            .collect(),
    };
    Ok((values, solution.eval(&objective)))
}
