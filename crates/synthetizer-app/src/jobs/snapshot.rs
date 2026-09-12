use crate::solution::Solution;
use serde::{Deserialize, Serialize};
use solver_api::{GlobalUnsatProof as UnsatProof, SolverProgress};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SolveRequest {
    #[serde(flatten)]
    pub problem: solver_api::ProblemRequest,
    #[serde(default)]
    pub solve_mode: solver_api::SolveMode,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobSnapshot<Id = String> {
    pub job_id: Id,
    pub status: JobStatus,
    pub started_at_ms: u64,
    pub progress: Option<SolverProgress>,
    pub proof: Option<solver_api::OptimalityProof>,
    pub sequence: u64,
    pub result: Option<Solution>,
    /// Populated during/after enumeration. Empty in single-solution mode.
    pub results: Vec<Solution>,
    pub enumeration_complete: bool,
    /// Progress-only emit: `result`/`results` are empty on purpose; UI must keep its copies.
    #[serde(default)]
    pub results_omitted: bool,
    /// Incremental enumeration emit: `result` is the newly found layout; append it locally.
    #[serde(default)]
    pub result_appended: bool,
    /// Server result count, also included with progress to detect missed appends.
    #[serde(default)]
    pub results_len: usize,
    pub error: Option<String>,
    /// Present for a finite global contradiction from the solver.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unsat: Option<UnsatProof>,
}

#[derive(Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Running,
    Cancelling,
    Completed,
    Cancelled,
    Incomplete,
    Unsat,
    Failed,
}

impl<Id: Clone> JobSnapshot<Id> {
    #[must_use]
    pub fn new(id: Id, started_at_ms: u64) -> Self {
        Self {
            job_id: id,
            status: JobStatus::Running,
            started_at_ms,
            progress: None,
            proof: None,
            sequence: 0,
            result: None,
            results: Vec::new(),
            enumeration_complete: false,
            results_omitted: false,
            result_appended: false,
            results_len: 0,
            error: None,
            unsat: None,
        }
    }

    /// Advance the full snapshot sequence after a host-owned update.
    pub fn finish_update(&mut self) {
        self.sequence += 1;
        self.results_omitted = false;
        self.result_appended = false;
        self.results_len = 0;
    }

    /// Update telemetry and return a packet without cloning stored solution graphs.
    #[must_use]
    pub fn update_progress(&mut self, progress: SolverProgress) -> Self {
        self.progress = Some(progress);
        self.sequence += 1;
        let mut packet = self.thin_packet();
        packet.results_omitted = true;
        packet
    }

    /// Keep the live append order. Duplicate identities do not advance the sequence.
    pub fn append_solution(&mut self, solution: Solution) -> Option<Self> {
        if !matches!(self.status, JobStatus::Running | JobStatus::Cancelling)
            || self
                .results
                .iter()
                .any(|existing| existing.has_same_layout(&solution))
        {
            return None;
        }
        if self.result.is_none() {
            self.result = Some(solution.clone());
        }
        self.results.push(solution.clone());
        self.sequence += 1;
        let mut packet = self.thin_packet();
        packet.result = Some(solution);
        packet.result_appended = true;
        Some(packet)
    }

    pub(super) fn thin_packet(&self) -> Self {
        Self {
            job_id: self.job_id.clone(),
            status: self.status,
            started_at_ms: self.started_at_ms,
            progress: self.progress.clone(),
            proof: self.proof,
            sequence: self.sequence,
            result: None,
            results: Vec::new(),
            enumeration_complete: self.enumeration_complete,
            results_omitted: false,
            result_appended: false,
            results_len: self.results.len(),
            error: self.error.clone(),
            unsat: None,
        }
    }
}
