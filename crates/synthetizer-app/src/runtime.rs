//! Native solver execution. Job projection is host-independent.
pub use crate::solution::Solution;
use solver_api::{RunOptions, SolveObserver, SolveOutcome, SolverError};
use std::sync::atomic::AtomicBool;

/// # Errors
/// Forwards the solver's common error contract.
pub fn solve(
    problem: &solver_api::Problem,
    options: &RunOptions,
    cancel: &AtomicBool,
    observer: &dyn SolveObserver,
) -> Result<SolveOutcome, SolverError> {
    solver_core::solve_problem(problem, options, cancel, observer)
}
