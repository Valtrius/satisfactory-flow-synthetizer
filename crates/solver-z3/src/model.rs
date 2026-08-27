use num::BigRational;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointRequest {
    pub id: String,
    pub name: String,
    pub rate: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SolveRequest {
    pub inputs: Vec<EndpointRequest>,
    pub outputs: Vec<EndpointRequest>,
    pub belt_rate: String,
    /// When true, after proving minimal N, collect every distinct layout at that size.
    #[serde(default)]
    pub enumerate_all_at_n: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct Endpoint {
    pub name: String,
    pub rate: BigRational,
}

#[derive(Clone, Debug)]
pub(crate) struct Problem {
    pub inputs: Vec<Endpoint>,
    pub outputs: Vec<Endpoint>,
    pub belt_rate: BigRational,
    pub total_input: BigRational,
    pub total_output: BigRational,
    pub discard_rate: BigRational,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OperatorKind {
    Splitter2,
    Splitter3,
    Merger2,
    Merger3,
}

impl OperatorKind {
    pub const fn input_count(self) -> usize {
        match self {
            Self::Splitter2 | Self::Splitter3 => 1,
            Self::Merger2 => 2,
            Self::Merger3 => 3,
        }
    }

    pub const fn output_count(self) -> usize {
        match self {
            Self::Splitter2 => 2,
            Self::Splitter3 => 3,
            Self::Merger2 | Self::Merger3 => 1,
        }
    }

    pub const fn public_kind(self) -> NodeKind {
        match self {
            Self::Splitter2 => NodeKind::Splitter2,
            Self::Splitter3 => NodeKind::Splitter3,
            Self::Merger2 => NodeKind::Merger2,
            Self::Merger3 => NodeKind::Merger3,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Route {
    Consumer(usize),
    Discard,
}

#[derive(Clone, Debug)]
pub(crate) struct Candidate {
    pub node_types: Vec<OperatorKind>,
    pub routes: Vec<Option<Route>>,
}

#[derive(Clone, Debug)]
pub(crate) struct VerifiedCandidate {
    pub candidate: Candidate,
    pub producer_rates: Vec<Option<BigRational>>,
}

pub(crate) const PORTS: usize = 3;

pub(crate) const fn producer_count(input_count: usize, node_count: usize) -> usize {
    input_count + node_count * PORTS
}

pub(crate) const fn consumer_count(output_count: usize, node_count: usize) -> usize {
    node_count * PORTS + output_count
}

pub(crate) const fn node_producer(input_count: usize, node: usize, port: usize) -> usize {
    input_count + node * PORTS + port
}

pub(crate) const fn node_consumer(node: usize, port: usize) -> usize {
    node * PORTS + port
}

pub(crate) const fn output_consumer(node_count: usize, output: usize) -> usize {
    node_count * PORTS + output
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayRate {
    pub exact: String,
    pub decimal: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Input,
    Splitter2,
    Splitter3,
    Merger2,
    Merger3,
    Output,
    Discard,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphNode {
    pub id: String,
    pub kind: NodeKind,
    pub label: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
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

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SolutionStats {
    pub node_count: usize,
    pub splitters: usize,
    pub mergers: usize,
    pub feedback_loops: usize,
    pub checked_through: usize,
    /// Operator↔operator belts only (includes feedback; excludes I/O stubs and discard).
    pub belt_count: usize,
    /// Max rate among those same belts.
    pub internal_max_throughput: DisplayRate,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Solution {
    pub status: String,
    pub model_version: u32,
    pub stats: SolutionStats,
    pub total_input: DisplayRate,
    pub total_output: DisplayRate,
    pub discard_rate: DisplayRate,
    pub belt_rate: DisplayRate,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub build_steps: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum SolverProgress {
    /// Rates normalized; combinatorial lower bound is known.
    Preparing { lower_bound: usize },
    /// Starting (or about to start) the portfolio at `node_count`.
    Checking {
        node_count: usize,
        rejected_unstable_candidates: usize,
        lower_bound: usize,
        profile_count: usize,
        attempt_slots: usize,
        threads_per_attempt: usize,
    },
    /// Every profile at this size was unsat; some SAT models failed verification.
    CandidateRejected {
        node_count: usize,
        rejected_unstable_candidates: usize,
        reason: String,
        lower_bound: usize,
    },
    /// Mid-size portfolio heartbeat (throttled). Counters are telemetry only.
    SizeProgress {
        node_count: usize,
        lower_bound: usize,
        profiles_total: usize,
        profiles_unresolved: usize,
        profiles_unsat: usize,
        active_attempts: usize,
        launched_attempts: usize,
        abandoned_attempts: usize,
        rejected_unstable_candidates: usize,
        attempt_slots: usize,
    },
}
