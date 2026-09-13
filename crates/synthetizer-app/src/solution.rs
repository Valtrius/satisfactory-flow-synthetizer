//! Display payload with an in-process canonical identity.
use crate::presentation::{PresentationError, PresentationSolution, present_best_known_solution};
use serde::Serialize;
use solver_api::{BestKnownSolution, CanonicalGraphKey, PreparedProblem};

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
    /// Present a witness from the public solve API for this prepared problem.
    /// Its identity has already been normalized after restoring caller rates and terminals.
    /// # Errors
    /// Returns inconsistent witness metadata or malformed graph references.
    pub fn from_best(
        prepared: &PreparedProblem,
        best: &BestKnownSolution,
    ) -> Result<Self, PresentationError> {
        let display = present_best_known_solution(prepared, best, None)?;
        Ok(Self {
            display,
            layout_key: best.canonical_graph_key.clone(),
        })
    }

    #[must_use]
    pub fn has_same_layout(&self, other: &Self) -> bool {
        self.layout_key == other.layout_key
    }
}
