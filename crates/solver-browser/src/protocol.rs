use serde::{Deserialize, Serialize};
use solver_api::{
    BestKnownSolution, IncompleteReason, NodeProfile, Problem, ProofSummary, SolveMode,
};
use solver_core::{
    Counts,
    leaf::LeafSpec,
    profile::{AccountedProfile, ProfileLinkAccounting},
};
use synthetizer_app::{
    jobs::{JobSnapshot, interruption_packet},
    solution::Solution,
};

#[derive(Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Strategy {
    #[default]
    Portfolio,
    Boolean,
    Sparse,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct Options {
    pub worker_count: usize,
    pub max_nodes: Option<u32>,
    pub max_layouts: usize,
    pub max_identity_bytes: usize,
    pub strategy: Strategy,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            worker_count: 1,
            max_nodes: None,
            max_layouts: 50_000,
            max_identity_bytes: 64 * 1024 * 1024,
            strategy: Strategy::Portfolio,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LeafWork {
    pub problem: Problem,
    pub profile: NodeProfile,
    pub links: u32,
    pub discards: u32,
    pub physical_links: u32,
    pub source: Option<usize>,
    pub second_source: Option<usize>,
    pub mode: SolveMode,
    pub sparse: bool,
}
impl LeafWork {
    pub fn new(problem: Problem, spec: LeafSpec) -> Self {
        Self {
            problem,
            profile: spec.profile.profile,
            links: spec.profile.accounting.link_count,
            discards: spec.profile.accounting.discard_link_count,
            physical_links: spec.profile.accounting.physical_link_count,
            source: spec.source,
            second_source: spec.second_source,
            mode: spec.mode,
            sparse: spec.counts == Counts::Sparse,
        }
    }
    pub fn spec(&self) -> LeafSpec {
        LeafSpec {
            profile: AccountedProfile {
                profile: self.profile,
                accounting: ProfileLinkAccounting {
                    link_count: self.links,
                    discard_link_count: self.discards,
                    physical_link_count: self.physical_links,
                },
            },
            source: self.source,
            second_source: self.second_source,
            mode: self.mode,
            counts: if self.sparse {
                Counts::Sparse
            } else {
                Counts::Boolean
            },
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dispatch {
    pub id: String,
    pub branch: usize,
    pub source: String,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Event {
    Witness {
        id: String,
        witness: Box<BestKnownSolution>,
    },
    Retired {
        id: String,
        verdict: String,
        detail: String,
    },
    Interrupt {
        reason: String,
        detail: String,
    },
}

#[derive(Serialize)]
pub struct IndexedSolution {
    pub index: usize,
    pub solution: Solution,
}
#[derive(Serialize)]
pub struct Recovery {
    pub cancelled: JobSnapshot,
    pub failed: JobSnapshot,
}
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Scheduling {
    pub dispatched: usize,
    pub second_output_roots: usize,
    pub adaptive_groups: usize,
    pub adaptive_children: usize,
    pub peak_active: usize,
    pub active: usize,
    pub identity_bytes: usize,
    pub proof_owner: Option<usize>,
    pub budgets: Vec<usize>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Update {
    pub dispatch: Vec<Dispatch>,
    pub stop: Vec<String>,
    pub packets: Vec<JobSnapshot>,
    pub append: Vec<IndexedSolution>,
    pub count: usize,
    pub preferred_index: Option<usize>,
    pub recovery: Recovery,
    pub done: bool,
    pub scheduling: Scheduling,
}

pub fn encode(value: &impl Serialize) -> Result<String, String> {
    serde_json::to_string(value).map_err(|error| error.to_string())
}

pub fn recovery(
    snapshot: &JobSnapshot,
    mode: SolveMode,
    best: Option<&BestKnownSolution>,
    proof: &ProofSummary,
) -> Result<Recovery, String> {
    Ok(Recovery {
        cancelled: interruption_packet(snapshot, mode, best, proof, IncompleteReason::Cancelled)
            .ok_or("cannot interrupt a sealed job")?,
        failed: interruption_packet(
            snapshot,
            mode,
            best,
            proof,
            IncompleteReason::WorkerFailed {
                detail: "Browser compute stopped before completion.".into(),
            },
        )
        .ok_or("cannot interrupt a sealed job")?,
    })
}
