//! The two passes that solve one section (ADR 0004).
//!
//! 1. Maximize throughput: what the section's origins put out plus what its sinks take in.
//! 2. Hold throughput at that optimum and split every fan-out and fan-in as evenly as possible.
//!
//! When throughput has no ceiling (the section has limits, but some part can grow from the depot
//! without end), pass 1 settles the section by its limits instead (ADR 0006): run every limited
//! node as close to its limit as it gets, then every other node as low as its links allow.
//!
//! Pass 1 (either way) is a mixed-integer program when a node's activation input has links: it
//! settles how many facilities that node runs, with its search capped at [`NODE_LIMIT`]. Pass 2
//! and the throttle analysis take those numbers as given and are linear programs (ADR 0013).

use std::collections::HashMap;

use good_lp::solvers::microlp::MicroLpProblem;
use good_lp::{
    Expression, ResolutionError, Solution as _, SolutionStatus, SolverModel, Variable, microlp,
    variable,
};

use crate::graph::{LinkId, NodeId, Port};
use crate::solve::model::{Ctx, Model, Program, build, runs_without_end};
use crate::solve::sections::Section;
use crate::solve::{RoundUp, UndeterminedReason};

/// How many branch-and-bound nodes pass 1 may search when a section has a node whose activation
/// input has links (ADR 0013). Beyond that many, the best answer found so far is taken: it may fall
/// short of the most the section could do (throughput, or how close the limited nodes come to
/// their limits), and only in those sections. Every other section is a linear program, solved
/// exactly. A count of nodes, unlike a time limit, gives the same answer on every machine.
const NODE_LIMIT: u64 = 200;

/// The error a capped search gives when it found no answer at all within [`NODE_LIMIT`].
const NO_ANSWER: &str = "the search found no answer within its node limit";

/// Solved values for one section.
pub(crate) struct Values {
    pub activity: HashMap<NodeId, f64>,
    pub links: HashMap<LinkId, f64>,
    pub depot: Vec<(Port, f64)>,
    pub ports: Vec<(Port, f64)>,
    /// How many facilities each node whose activation input has links runs, as pass 1 settled.
    pub running: HashMap<NodeId, u32>,
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
    let first = build(ctx, section, &Program::Pass1, None);
    if first.limits.is_empty() {
        return Outcome::Undetermined(UndeterminedReason::NoLimit);
    }
    let fed: HashMap<NodeId, u32> = (first.running.iter()).map(|&(node, _)| (node, 0)).collect();
    let pass1 = match pass1(ctx, section, &Program::Pass1, first) {
        // The capped search found nothing: every node whose activation input has links stays off,
        // which is always possible and is a linear program.
        Err(ResolutionError::Other(NO_ANSWER)) => {
            let off = Program::Fixed {
                running: fed,
                scaled: None,
            };
            pass1(ctx, section, &off, build(ctx, section, &off, None))
        }
        result => result,
    };
    let (values, settled) = match pass1 {
        Ok(settled) => settled,
        Err(ResolutionError::Unbounded) => {
            return Outcome::Undetermined(UndeterminedReason::Unbounded);
        }
        Err(error) => return Outcome::Failed(error.to_string()),
    };

    // Every later program runs the facilities pass 1 settled on, and is linear.
    let fixed = Program::Fixed {
        running: values.running.clone(),
        scaled: None,
    };
    let mut second = build(ctx, section, &fixed, None);
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
        Ok((even, _)) => Outcome::Solved(Values {
            running: values.running,
            ..even
        }),
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

/// Pass 1 on `program`, already built as `first`: the most throughput, or, if throughput has no
/// ceiling, the limits settle the section.
fn pass1(
    ctx: &Ctx<'_>,
    section: &Section,
    program: &Program,
    first: Model,
) -> Result<(Values, Settled), ResolutionError> {
    let objective = first.throughput.clone();
    match run(first, objective, Direction::Maximise, Vec::new()) {
        Ok((values, optimum)) => Ok((values, Settled::Throughput(optimum))),
        Err(ResolutionError::Unbounded) => settle_by_limits(ctx, section, program),
        Err(error) => Err(error),
    }
}

/// Pass 1 when throughput has no ceiling: run every limited node as close to its limit as it
/// gets, then every node without a limit as low as its links allow. A node nothing asks anything
/// of runs at 0 rather than without end.
fn settle_by_limits(
    ctx: &Ctx<'_>,
    section: &Section,
    program: &Program,
) -> Result<(Values, Settled), ResolutionError> {
    let first = build(ctx, section, program, None);
    let reach = drivers_reach(&first);
    let (_, drivers) = run(first, reach, Direction::Maximise, Vec::new())?;
    let second = build(ctx, section, program, None);
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
        running,
        open_running,
        depot_activation,
        whole_units,
        ..
    } = model;
    // good_lp's variables in the order it made them, which is the order microlp's are made in.
    let order: Vec<Variable> = if running.is_empty() {
        Vec::new()
    } else {
        (vars.iter_variables_with_def())
            .map(|(var, _)| var)
            .collect()
    };
    let unsolved = match direction {
        Direction::Maximise => vars.maximise(objective.clone()),
        Direction::Minimise => vars.minimise(objective.clone()),
    };
    let mut problem = unsolved.using(microlp);
    for constraint in constraints.into_iter().chain(extra) {
        problem.add_constraint(constraint);
    }
    let solution = if running.is_empty() {
        Answer::Exact(problem.solve()?)
    } else {
        Answer::Capped(solve_capped(&problem, &order)?)
    };
    if runs_without_end(&open_running, |running| solution.value(running)) {
        return Err(ResolutionError::Unbounded);
    }
    let clean = |value: f64| if value.abs() < 1e-9 { 0.0 } else { value };
    let mut values = Values {
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
        running: (running.into_iter())
            .map(|(node, running)| (node, solution.value(running).round() as u32))
            .collect(),
    };
    // A count of whole units is a whole number: solver noise around one is that number.
    for node in whole_units {
        if let Some(count) = values.activity.get_mut(&node) {
            let whole = count.round();
            if (*count - whole).abs() <= tolerance(whole) {
                *count = whole;
            }
        }
    }
    // An activation input from the depot: each running facility needs all of it.
    for (port, count, per_running) in depot_activation {
        let running = RoundUp::of(clean(solution.value(count))).facilities;
        let needed = f64::from(running) * per_running;
        for (at, flow) in values.ports.iter_mut().chain(values.depot.iter_mut()) {
            if *at == port {
                *flow += needed;
            }
        }
    }
    Ok((values, solution.eval(&objective)))
}

/// A solved program: through good_lp, or through microlp with its search capped.
enum Answer {
    Exact(good_lp::solvers::microlp::MicroLpSolution),
    Capped(HashMap<Variable, f64>),
}

impl good_lp::Solution for Answer {
    fn status(&self) -> SolutionStatus {
        match self {
            Self::Exact(solution) => solution.status(),
            Self::Capped(_) => SolutionStatus::TimeLimit,
        }
    }

    fn value(&self, variable: Variable) -> f64 {
        match self {
            Self::Exact(solution) => solution.value(variable),
            Self::Capped(values) => values[&variable],
        }
    }
}

/// Solves `problem`, which has whole-number variables, with at most [`NODE_LIMIT`] branch-and-bound
/// nodes, keeping the best answer found by then. good_lp has no node limit, so this asks microlp
/// directly and pairs its values, which it gives in the order the variables were made, with
/// good_lp's `order`.
fn solve_capped(
    problem: &MicroLpProblem,
    order: &[Variable],
) -> Result<HashMap<Variable, f64>, ResolutionError> {
    let mut options = ::microlp::SolveOptions::default();
    options.node_limit = Some(NODE_LIMIT);
    match problem.as_inner().solve_with(options)? {
        ::microlp::SolveOutcome::Solution(solution) => Ok((order.iter().copied())
            .zip(solution.iter().map(|(_, value)| value))
            .collect()),
        ::microlp::SolveOutcome::Interrupted(_) => Err(ResolutionError::Other(NO_ANSWER)),
    }
}

#[cfg(test)]
mod tests {
    use good_lp::{ProblemVariables, Solution as _, SolverModel, microlp, variable};

    use super::solve_capped;

    /// The capped search reads microlp's values in the order the variables were made; good_lp
    /// makes them in the same order. Both ways of solving give the same values for each variable.
    #[test]
    fn a_capped_solve_gives_each_variable_its_own_value() {
        let build = || {
            let mut vars = ProblemVariables::new();
            let x = vars.add(variable().min(0).max(2.5));
            let n = vars.add(variable().integer().min(0).max(10));
            let y = vars.add(variable().min(0).max(7));
            let order = vec![x, n, y];
            let mut problem = vars.maximise(3 * x + n + 2 * y).using(microlp);
            problem.add_constraint((x + y).leq(4.0));
            problem.add_constraint((n - x).leq(0.5));
            (problem, order)
        };
        let (problem, _) = build();
        let exact = problem.solve().expect("solves");
        let (problem, order) = build();
        let capped = solve_capped(&problem, &order).expect("solves");
        // x = 2.5, y = 1.5, and n the whole number at most x + 0.5: 3.
        for (var, expected) in order.iter().zip([2.5, 3.0, 1.5]) {
            assert!((exact.value(*var) - expected).abs() < 1e-9);
            assert!((capped[var] - expected).abs() < 1e-9);
        }
    }
}
