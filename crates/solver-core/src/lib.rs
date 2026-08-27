mod acyclic_incumbent;
pub mod algebra;
pub mod canonical;
pub mod hotspot_profile;
pub mod lower_bound;
pub mod problem;
pub mod profile;
mod proof_ledger;
pub mod propagation;
pub mod reachability;
pub mod scc;
pub mod search;
pub mod solver;
pub mod telemetry;
pub mod topology;

pub use problem::{
    InvalidProblem, NormalizedProblem, Preparation, RateMultiset, TerminalMapping, prepare_problem,
};
pub use solver::{
    SolveObserver, SolveOptions, SolverError, enumerate_with_observer, solve, solve_with_observer,
};

/// Solve through the shared contract, retaining enumeration without an external observer.
///
/// # Errors
/// Returns invalid input/options or an internal solver failure.
pub fn solve_problem(
    problem: &solver_api::Problem,
    options: &solver_api::RunOptions,
    cancel: &std::sync::atomic::AtomicBool,
    observer: &dyn solver_api::SolveObserver,
) -> Result<solver_api::SolveOutcome, solver_api::SolverError> {
    let collector = solver_validation::SolutionCollector::new(problem, observer);
    let native = SolveOptions {
        max_nodes: options.max_nodes,
        worker_count: options.worker_count,
    };
    let result = match options.mode {
        solver_api::SolveMode::Optimal => solve_with_observer(problem, &native, cancel, &collector),
        solver_api::SolveMode::AllAtMinimumNodes => {
            enumerate_with_observer(problem, &native, cancel, &collector)
        }
    }
    .map_err(|error| match error {
        SolverError::InvalidProblem(_) => {
            solver_api::SolverError::InvalidProblem(error.to_string())
        }
        SolverError::InvalidWorkerCount => {
            solver_api::SolverError::InvalidOptions(error.to_string())
        }
        _ => solver_api::SolverError::Internal(error.to_string()),
    })?;
    Ok(collector.finish(result, options.mode))
}
