//! Production solve and presentation.
use crate::presentation::{PresentationError, PresentationSolution, present_best_known_solution};
use serde::Serialize;
use solver_api::{
    BestKnownSolution, CanonicalGraphKey, PreparedProblem, RunOptions, SolveObserver, SolveOutcome,
    SolverError,
};
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

/// Display metadata is added once, after exact solutions leave a solver.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Solution {
    #[serde(flatten)]
    pub display: PresentationSolution,
    /// In-process identity only. IPC and history use the display graph.
    #[serde(skip)]
    pub layout_key: CanonicalGraphKey,
}

impl Solution {
    /// # Errors
    /// Returns inconsistent witness metadata or malformed graph references.
    pub fn from_best(
        prepared: &PreparedProblem,
        best: &BestKnownSolution,
    ) -> Result<Self, PresentationError> {
        let display = present_best_known_solution(prepared, best, None)?;
        Ok(Self {
            display,
            layout_key: solver_validation::layout_key(&prepared.problem, &best.graph),
        })
    }

    #[must_use]
    pub fn has_same_layout(&self, other: &Self) -> bool {
        self.layout_key == other.layout_key
    }
}
