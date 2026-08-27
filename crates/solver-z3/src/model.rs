use num::BigRational;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub(crate) struct Endpoint {
    pub rate: BigRational,
}

#[derive(Clone, Debug)]
pub(crate) struct Problem {
    pub worker_count: usize,
    pub inputs: Vec<Endpoint>,
    pub outputs: Vec<Endpoint>,
    pub belt_rate: BigRational,
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

pub(crate) type Solution = solver_api::BestKnownSolution;

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
        /// Present while Opt searches under a strict operator-belt cap.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_operator_belts: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        incumbent_belt_count: Option<usize>,
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_operator_belts: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        incumbent_belt_count: Option<usize>,
    },
}
