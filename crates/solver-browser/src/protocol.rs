use serde::Serialize;
use solver_api::{BestKnownSolution, IncompleteReason, ProofSummary, SolveMode};
use synthetizer_app::jobs::{JobSnapshot, interruption_packet};

#[derive(Serialize)]
pub struct Dispatch {
    pub id: (u64, usize),
    pub impossible: bool,
}

#[derive(Serialize)]
pub struct Action {
    pub commands: String,
    pub witnessed: bool,
    pub completion: Option<&'static str>,
}

#[derive(Serialize)]
pub struct Recovery {
    pub cancelled: JobSnapshot,
    pub failed: JobSnapshot,
}

#[derive(Serialize)]
pub struct Update {
    pub dispatch: Vec<Dispatch>,
    pub packets: Vec<JobSnapshot>,
    pub recovery: Recovery,
    pub done: bool,
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
    let cancelled = interruption_packet(snapshot, mode, best, proof, IncompleteReason::Cancelled)
        .ok_or("cannot interrupt a sealed job")?;
    let failed = interruption_packet(
        snapshot,
        mode,
        best,
        proof,
        IncompleteReason::WorkerFailed {
            detail: "Browser compute worker stopped before completion.".into(),
        },
    )
    .ok_or("cannot interrupt a sealed job")?;
    Ok(Recovery { cancelled, failed })
}
