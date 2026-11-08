//! The two passes that solve one section (ADR 0004).
//!
//! 1. Maximize throughput: what the section's origins put out plus what its sinks take in.
//! 2. Hold throughput at that optimum and split every fan-out and fan-in as evenly as possible.

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
    let best = match run(first, objective, Direction::Maximise, Vec::new()) {
        Ok((values, best)) => (values, best),
        Err(ResolutionError::Unbounded) => {
            return Outcome::Undetermined(UndeterminedReason::Unbounded);
        }
        Err(error) => return Outcome::Failed(error.to_string()),
    };
    let (values, optimum) = best;

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
    // Pass 2 must not trade throughput for evenness.
    extra.push(
        second
            .throughput
            .clone()
            .geq(optimum - tolerance(optimum) * 1e-2),
    );
    match run(second, unevenness, Direction::Minimise, extra) {
        Ok((even, _)) => Outcome::Solved(even),
        // Numerical trouble in pass 2 only costs evenness; pass 1's answer is still right.
        Err(_) => Outcome::Solved(values),
    }
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
    };
    Ok((values, solution.eval(&objective)))
}
