//! Shared dispatch and presentation. Reference remains a non-UI oracle.
use crate::presentation::{PresentationError, PresentationSolution, present_best_known_solution};
use serde::{Deserialize, Serialize};
use solver_api::{
    BestKnownSolution, CanonicalGraphKey, PreparedProblem, RunOptions, SolveObserver, SolveOutcome,
    SolverError,
};
use std::sync::atomic::AtomicBool;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SolverEngine {
    #[default]
    Custom,
    Z3,
    Astra,
}

/// # Errors
/// Forwards the selected solver's common error contract.
pub fn solve(
    engine: SolverEngine,
    problem: &solver_api::Problem,
    options: &RunOptions,
    cancel: &AtomicBool,
    observer: &dyn SolveObserver,
) -> Result<SolveOutcome, SolverError> {
    match engine {
        SolverEngine::Custom => solver_core::solve_problem(problem, options, cancel, observer),
        SolverEngine::Z3 => solver_z3::solve_problem(problem, options, cancel, observer),
        SolverEngine::Astra => solver_astra::solve_problem(problem, options, cancel, observer),
    }
}

/// Display metadata is added once, after exact solutions leave a solver.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Solution {
    pub engine: SolverEngine,
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
        engine: SolverEngine,
        prepared: &PreparedProblem,
        best: &BestKnownSolution,
    ) -> Result<Self, PresentationError> {
        let display = present_best_known_solution(prepared, best, None)?;
        Ok(Self {
            engine,
            display,
            layout_key: solver_validation::layout_key(&prepared.problem, &best.graph),
        })
    }

    #[must_use]
    pub fn has_same_layout(&self, other: &Self) -> bool {
        self.layout_key == other.layout_key
    }
}
