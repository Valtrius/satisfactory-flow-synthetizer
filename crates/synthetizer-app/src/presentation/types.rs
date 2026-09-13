use serde::{Deserialize, Serialize};
use solver_api::{GlobalUnsatProof, IncompleteReason, ProofSummary, ValidationSummary};
use thiserror::Error;

/// Exact value plus a deterministic six-place decimal preview.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayRate {
    pub exact: String,
    pub decimal: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphNodeKind {
    Input,
    Splitter2,
    Splitter3,
    Merger2,
    Merger3,
    Output,
    Discard,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphNode {
    pub id: String,
    pub kind: GraphNodeKind,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub source_port: usize,
    pub target_port: usize,
    pub rate: DisplayRate,
    pub feedback: bool,
    pub discarded: bool,
}

/// Public counts separate operator belts from all physical terminal/discard links.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationStats {
    pub node_count: u32,
    pub link_count: u32,
    pub physical_link_count: u32,
    pub discard_link_count: u32,
    pub splitters: u32,
    pub mergers: u32,
    pub feedback_loops: u32,
    pub checked_through: Option<u32>,
    pub internal_max_throughput: DisplayRate,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationSolution {
    /// `proven_optimal` or the explicitly non-optimal `best_known`.
    pub status: String,
    pub model_version: u32,
    /// Present only for a completed optimality proof. A live or incomplete
    /// incumbent deliberately cannot acquire this field.
    pub proof: Option<ProofSummary>,
    /// Independent exact validation record carried by every displayed witness.
    pub validation: ValidationSummary,
    pub stats: PresentationStats,
    pub total_input: DisplayRate,
    pub total_output: DisplayRate,
    pub discard_rate: DisplayRate,
    pub belt_rate: DisplayRate,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub build_steps: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentedIncomplete {
    pub reason: IncompleteReason,
    pub best_known: Option<PresentationSolution>,
    pub proof: ProofSummary,
}

/// Application-facing mathematical outcome; internal failures remain errors.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "result", rename_all = "snake_case")]
pub enum PresentedSolveOutcome {
    Optimal(PresentationSolution),
    GloballyUnsat(GlobalUnsatProof),
    Incomplete(PresentedIncomplete),
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum PresentationError {
    #[error("solver witness count {field} is {actual}, expected {expected}")]
    CountMismatch {
        field: &'static str,
        expected: u32,
        actual: u32,
    },
    #[error("solver witness contains duplicate node id {0}")]
    DuplicateNode(u32),
    #[error("solver witness references unknown node id {0}")]
    UnknownNode(u32),
    #[error("solver witness references {side} terminal {index}, but only {count} exist")]
    TerminalOutOfRange {
        side: &'static str,
        index: u32,
        count: usize,
    },
    #[error("solver witness discard total disagrees with the exact external surplus")]
    DiscardTotalMismatch,
    #[error("solver witness count cannot fit the presentation integer type")]
    CountOverflow,
}
