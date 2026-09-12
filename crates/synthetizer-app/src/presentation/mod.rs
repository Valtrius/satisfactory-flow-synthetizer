//! Deterministic presentation projection for exact solver outcomes.
mod feedback;
mod graph;
mod projection;
mod rates;
mod types;

pub use projection::{present_best_known_solution, present_solve_result};
pub use types::{
    DisplayRate, GraphEdge, GraphNode, GraphNodeKind, PresentationError, PresentationSolution,
    PresentationStats, PresentedIncomplete, PresentedSolveOutcome,
};

#[cfg(test)]
mod tests;
