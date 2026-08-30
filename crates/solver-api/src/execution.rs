//! Execution controls and complete mathematical results, independent of transport.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{BestKnownSolution, SolveResult, SolverEvent};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SolveMode {
    #[default]
    Optimal,
    AllAtMinimumNodesAndMinimumLinks,
    AllAtMinimumNodes,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunOptions {
    pub mode: SolveMode,
    /// Exhaustion of this inclusive bound is incomplete, never global UNSAT.
    pub max_nodes: Option<u32>,
    pub worker_count: usize,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            mode: SolveMode::Optimal,
            max_nodes: None,
            worker_count: 1,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptimalityProof {
    pub minimum_node_count: Option<u32>,
    /// Minimum operator-link count at `minimum_node_count`, never a global L bound.
    pub minimum_link_count: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum EnumerationStatus {
    NotRequested,
    AllAtMinimumNodesAndMinimumLinks {
        node_count: Option<u32>,
        link_count: Option<u32>,
        complete: bool,
    },
    AllAtMinimumNodes {
        node_count: Option<u32>,
        complete: bool,
    },
}

/// The result remains available without an observer, including partial enumeration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SolveOutcome {
    pub result: SolveResult,
    pub solutions: Vec<BestKnownSolution>,
    pub proof: OptimalityProof,
    pub enumeration: EnumerationStatus,
}

impl SolveOutcome {
    #[must_use]
    pub fn new(result: SolveResult, mode: SolveMode, solutions: Vec<BestKnownSolution>) -> Self {
        let proof = match &result {
            SolveResult::Optimal(solution) => OptimalityProof {
                minimum_node_count: Some(solution.node_count),
                minimum_link_count: Some(solution.link_count),
            },
            SolveResult::Incomplete(incomplete) => OptimalityProof {
                minimum_node_count: incomplete.best_known.as_ref().and_then(|best| {
                    let lower = incomplete
                        .proof
                        .node_counts_exhausted_through
                        .and_then(|n| n.checked_add(1))
                        .unwrap_or(0)
                        .max(incomplete.proof.initial_node_lower_bound);
                    (lower == best.node_count).then_some(best.node_count)
                }),
                minimum_link_count: None,
            },
            SolveResult::GloballyUnsat(_) => OptimalityProof::default(),
        };
        let complete = !matches!(result, SolveResult::Incomplete(_));
        Self {
            result,
            solutions: if mode == SolveMode::Optimal {
                Vec::new()
            } else {
                solutions
            },
            proof,
            enumeration: match mode {
                SolveMode::Optimal => EnumerationStatus::NotRequested,
                SolveMode::AllAtMinimumNodesAndMinimumLinks => {
                    EnumerationStatus::AllAtMinimumNodesAndMinimumLinks {
                        node_count: proof.minimum_node_count,
                        link_count: proof.minimum_link_count,
                        complete,
                    }
                }
                SolveMode::AllAtMinimumNodes => EnumerationStatus::AllAtMinimumNodes {
                    node_count: proof.minimum_node_count,
                    complete,
                },
            },
        }
    }
}

/// Observers are optional. Reference deliberately does not implement progress reporting.
pub trait SolveObserver: Sync {
    fn on_event(&self, event: SolverEvent);
}

impl<F: Fn(SolverEvent) + Sync> SolveObserver for F {
    fn on_event(&self, event: SolverEvent) {
        self(event);
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum SolverError {
    #[error("invalid problem: {0}")]
    InvalidProblem(String),
    #[error("invalid solver options: {0}")]
    InvalidOptions(String),
    #[error("solver failed: {0}")]
    Internal(String),
}
