//! Transport-neutral job snapshots and terminal projection.
//! Hosts own scheduling, time, cancellation acceptance and the completion seal.
use crate::{
    presentation::{PresentationError, PresentedSolveOutcome, present_solve_result},
    solution::Solution,
};
use serde::{Deserialize, Serialize};
use solver_api::{
    EnumerationStatus, GlobalUnsatProof as UnsatProof, IncompleteReason, PreparedProblem,
    SolveMode, SolveOutcome, SolveResult, SolverProgress,
};

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

    fn thin_packet(&self) -> Self {
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

/// Apply a cancellation accepted by the host before its completion seal.
#[must_use]
pub fn cancel_before_presentation(
    outcome: SolveOutcome,
    mode: SolveMode,
    cancelled: bool,
) -> SolveOutcome {
    if !cancelled || matches!(outcome.result, SolveResult::Incomplete(_)) {
        return outcome;
    }
    let (best_known, proof) = match outcome.result {
        SolveResult::Optimal(s) => (
            Some(solver_api::BestKnownSolution {
                node_count: s.node_count,
                link_count: s.link_count,
                physical_link_count: s.physical_link_count,
                discard_link_count: s.discard_link_count,
                canonical_graph_key: s.canonical_graph_key,
                graph: s.graph,
                validation: s.validation,
            }),
            s.proof,
        ),
        SolveResult::GloballyUnsat(s) => (None, s.proof),
        SolveResult::Incomplete(_) => unreachable!(),
    };
    SolveOutcome::new(
        SolveResult::Incomplete(solver_api::IncompleteResult {
            reason: IncompleteReason::Cancelled,
            best_known,
            proof,
        }),
        mode,
        outcome.solutions,
    )
}

/// Project a sealed solver outcome without changing the host sequence or emitting events.
/// Canonical keys must be the normalized identities supplied by the solver API.
/// # Errors
/// Returns inconsistent witness metadata or malformed graph references. The snapshot is unchanged on error.
pub fn project_outcome<Id>(
    snapshot: &mut JobSnapshot<Id>,
    mode: SolveMode,
    prepared: &PreparedProblem,
    outcome: &SolveOutcome,
) -> Result<(), PresentationError> {
    let presented = present_solve_result(prepared, &outcome.result)?;
    let solutions = terminal_solutions(snapshot, prepared, outcome)?;
    apply_terminal(snapshot, mode, outcome, presented, solutions);
    Ok(())
}

fn terminal_solutions<Id>(
    snapshot: &JobSnapshot<Id>,
    prepared: &PreparedProblem,
    outcome: &SolveOutcome,
) -> Result<Vec<Solution>, PresentationError> {
    // Search has sealed, so no further live results can change this cache.
    let published = snapshot
        .results
        .iter()
        .filter(|solution| {
            solution.display.status == "best_known" && solution.display.proof.is_none()
        })
        .map(|solution| (&solution.layout_key, solution))
        .collect::<std::collections::BTreeMap<_, _>>();
    outcome
        .solutions
        .iter()
        .map(|best| {
            if let Some(solution) = published.get(&best.canonical_graph_key) {
                return Ok((*solution).clone());
            }
            Solution::from_best(prepared, best)
        })
        .collect()
}

fn apply_terminal<Id>(
    snapshot: &mut JobSnapshot<Id>,
    mode: SolveMode,
    outcome: &SolveOutcome,
    presented: PresentedSolveOutcome,
    mut solutions: Vec<Solution>,
) {
    let cancelled = snapshot.status == JobStatus::Cancelling;
    let complete = matches!(
        outcome.enumeration,
        EnumerationStatus::AllMinN { complete: true, .. }
            | EnumerationStatus::AllMinNL { complete: true, .. }
    );
    // Result indices also identify saved graph edits. Keep the live append order
    // when the mathematical API returns its deterministically ordered collection.
    let published = snapshot
        .results
        .iter()
        .enumerate()
        .map(|(index, solution)| (&solution.layout_key, index))
        .collect::<std::collections::BTreeMap<_, _>>();
    solutions.sort_by_key(|solution| {
        published
            .get(&solution.layout_key)
            .copied()
            .unwrap_or(usize::MAX)
    });
    snapshot.results = solutions;
    snapshot.enumeration_complete = complete && !cancelled;
    snapshot.error = None;
    snapshot.unsat = None;
    snapshot.proof = Some(outcome.proof);
    match presented {
        PresentedSolveOutcome::Optimal(display) => {
            let SolveResult::Optimal(exact) = &outcome.result else {
                unreachable!()
            };
            let solution = Solution {
                display,
                layout_key: exact.canonical_graph_key.clone(),
            };
            if let Some(existing) = snapshot
                .results
                .iter_mut()
                .find(|s| s.has_same_layout(&solution))
            {
                *existing = solution.clone();
            }
            snapshot.result = Some(solution);
            snapshot.status = if cancelled {
                JobStatus::Cancelled
            } else {
                JobStatus::Completed
            };
        }
        PresentedSolveOutcome::GloballyUnsat(proof) => {
            snapshot.result = None;
            snapshot.status = JobStatus::Unsat;
            snapshot.unsat = Some(proof);
            snapshot.error = Some("No exact network exists for these rates.".to_owned());
        }
        PresentedSolveOutcome::Incomplete(incomplete) => {
            snapshot.status = match incomplete.reason {
                IncompleteReason::Cancelled => JobStatus::Cancelled,
                IncompleteReason::WorkerFailed { detail } => {
                    snapshot.error = Some(detail);
                    JobStatus::Failed
                }
                IncompleteReason::DeadlineReached => {
                    snapshot.error = Some("Solve deadline reached.".to_owned());
                    JobStatus::Incomplete
                }
                IncompleteReason::ResourceLimit { detail } => {
                    snapshot.error = Some(detail);
                    JobStatus::Incomplete
                }
            };
            if (mode == SolveMode::OneMinNL || snapshot.result.is_none())
                && let (Some(display), SolveResult::Incomplete(exact)) =
                    (incomplete.best_known, &outcome.result)
                && let Some(best) = &exact.best_known
            {
                snapshot.result = Some(Solution {
                    display,
                    layout_key: best.canonical_graph_key.clone(),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solver_api::{
        BestKnownSolution, IncompleteResult, OptimalSolution, OptimalityProof, PhysicalGraph,
        ProofSummary,
    };

    fn request() -> SolveRequest {
        serde_json::from_value(serde_json::json!({
            "solveMode": "all_min_n",
            "inputs": [{"id": "a", "name": "A", "rate": "24"}],
            "outputs": [{"id":"a","name":"","rate":"7"},{"id":"b","name":"","rate":"6"},{"id":"c","name":"","rate":"5"},{"id":"d","name":"","rate":"4"},{"id":"e","name":"","rate":"2"}],
            "beltRate": "1200"
        })).unwrap()
    }

    // Fixed independently validated graphs test projection without rerunning search.
    // Proof counters below are projection fixtures, not a new mathematical proof.
    fn fixture_outcome(mode: SolveMode) -> SolveOutcome {
        let data: serde_json::Value = serde_json::from_str(include_str!(
            "../../solver-core/tests/fixtures/preferred-order.json"
        ))
        .unwrap();
        let problem: solver_api::Problem = serde_json::from_value(data["problem"].clone()).unwrap();
        let graphs: Vec<PhysicalGraph> = serde_json::from_value(data["graphs"].clone()).unwrap();
        let solutions = graphs
            .into_iter()
            .map(|graph| {
                let validation = solver_validation::validate_solution(&problem, &graph).unwrap();
                BestKnownSolution {
                    node_count: validation.node_count,
                    link_count: validation.link_count,
                    physical_link_count: validation.physical_link_count,
                    discard_link_count: validation.discard_link_count,
                    canonical_graph_key: solver_validation::layout_key(&problem, &graph),
                    graph,
                    validation,
                }
            })
            .collect::<Vec<_>>();
        let best = solutions
            .iter()
            .min_by_key(|s| (s.node_count, s.link_count, &s.canonical_graph_key))
            .unwrap()
            .clone();
        let result = SolveResult::Optimal(OptimalSolution {
            node_count: best.node_count,
            link_count: best.link_count,
            physical_link_count: best.physical_link_count,
            discard_link_count: best.discard_link_count,
            canonical_graph_key: best.canonical_graph_key,
            graph: best.graph,
            validation: best.validation,
            proof: ProofSummary {
                initial_node_lower_bound: best.node_count,
                ..ProofSummary::default()
            },
        });
        SolveOutcome::new(result, mode, solutions)
    }

    fn project(snapshot: &mut JobSnapshot, request: &SolveRequest, outcome: &SolveOutcome) {
        let prepared = request.problem.prepare().unwrap();
        project_outcome(snapshot, request.solve_mode, &prepared, outcome).unwrap();
    }

    #[test]
    fn incremental_packets_preserve_wire_fields_and_full_snapshot_state() {
        let request = request();
        let prepared = request.problem.prepare().unwrap();
        let outcome = fixture_outcome(request.solve_mode);
        let solution = Solution::from_best(&prepared, &outcome.solutions[0]).unwrap();
        let mut snapshot = JobSnapshot::new("job".to_owned(), 42);
        let packet = snapshot.append_solution(solution.clone()).unwrap();
        assert!(packet.result_appended);
        assert!(!packet.results_omitted);
        assert!(packet.results.is_empty());
        assert_eq!(packet.results_len, 1);
        assert_eq!(packet.sequence, 1);
        assert_eq!(packet.result, Some(solution.clone()));
        assert!(snapshot.append_solution(solution.clone()).is_none());
        assert_eq!(snapshot.sequence, 1);
        let progress = SolverProgress {
            phase: solver_api::SolvePhase::Enumerating,
            elapsed_ms: 10,
            node_count: Some(7),
            link_constraint: None,
            node_lower_bound: Some(7),
            best_node_count: Some(7),
            best_link_count: None,
            solutions_found: 1,
            custom: Vec::new(),
        };
        let packet = snapshot.update_progress(progress.clone());
        let json = serde_json::to_value(packet).unwrap();
        assert_eq!(json["jobId"], "job");
        assert_eq!(json["startedAtMs"], 42);
        assert_eq!(json["status"], "running");
        assert_eq!(json["sequence"], 2);
        assert_eq!(json["resultsOmitted"], true);
        assert_eq!(json["resultAppended"], false);
        assert_eq!(json["resultsLen"], 1);
        assert!(json["result"].is_null());
        assert_eq!(json["results"], serde_json::json!([]));
        assert!(json.get("unsat").is_none());
        assert_eq!(snapshot.progress, Some(progress));
        assert_eq!(snapshot.results, vec![solution]);
        snapshot.finish_update();
        assert_eq!(snapshot.sequence, 3);
        assert!(!snapshot.results_omitted && !snapshot.result_appended);
        assert_eq!(snapshot.results_len, 0);
        snapshot.status = JobStatus::Completed;
        assert!(
            snapshot
                .append_solution(Solution::from_best(&prepared, &outcome.solutions[1]).unwrap())
                .is_none()
        );
        assert_eq!(snapshot.sequence, 3);
    }

    #[test]
    fn terminal_projection_errors_do_not_partially_replace_the_snapshot() {
        let request = request();
        let prepared = request.problem.prepare().unwrap();
        let outcome = fixture_outcome(request.solve_mode);
        let mut snapshot = JobSnapshot::new("job".to_owned(), 42);
        snapshot
            .append_solution(Solution::from_best(&prepared, &outcome.solutions[0]).unwrap())
            .unwrap();
        let before = serde_json::to_value(&snapshot).unwrap();
        let mut wrong_request = request.clone();
        wrong_request.problem.outputs.truncate(1);
        let wrong_problem = wrong_request.problem.prepare().unwrap();
        assert!(
            project_outcome(&mut snapshot, request.solve_mode, &wrong_problem, &outcome).is_err()
        );
        assert_eq!(serde_json::to_value(snapshot).unwrap(), before);
    }

    #[test]
    fn an_optimum_does_not_complete_partial_enumeration_and_reuses_normalized_identity() {
        for mode in [SolveMode::AllMinNL, SolveMode::AllMinN] {
            let request = request();
            let prepared = request.problem.prepare().unwrap();
            let mut outcome = fixture_outcome(mode);
            match &mut outcome.enumeration {
                EnumerationStatus::AllMinNL { complete, .. }
                | EnumerationStatus::AllMinN { complete, .. } => *complete = false,
                EnumerationStatus::NotRequested => unreachable!(),
            }
            let SolveResult::Optimal(best) = &outcome.result else {
                unreachable!()
            };
            let expected_key = solver_validation::layout_key(&prepared.problem, &best.graph);
            assert_eq!(best.canonical_graph_key, expected_key);
            let mut snapshot = JobSnapshot::new("job".to_owned(), 42);
            project_outcome(&mut snapshot, mode, &prepared, &outcome).unwrap();
            assert!(snapshot.status == JobStatus::Completed);
            assert!(!snapshot.enumeration_complete);
            assert_eq!(snapshot.result.unwrap().layout_key, expected_key);
            assert!(snapshot.proof.unwrap().minimum_link_count.is_some());
        }
    }

    #[test]
    fn terminal_conversion_matches_fresh_projection_with_partial_live_delivery() {
        let request = request();
        let prepared = request.problem.prepare().unwrap();
        let outcome = fixture_outcome(SolveMode::AllMinN);
        let expected = outcome
            .solutions
            .iter()
            .map(|best| Solution::from_best(&prepared, best).unwrap())
            .collect::<Vec<_>>();
        for count in [0, 1, expected.len()] {
            let mut snapshot = JobSnapshot::new(String::new(), 0);
            snapshot.results = expected.iter().take(count).cloned().rev().collect();
            assert_eq!(
                terminal_solutions(&snapshot, &prepared, &outcome).unwrap(),
                expected
            );
            // Cache entries with terminal proof metadata must be reconstructed.
            for solution in &mut snapshot.results {
                solution.display.status = "proven_optimal".into();
                solution.display.proof = Some(ProofSummary::default());
            }
            assert_eq!(
                terminal_solutions(&snapshot, &prepared, &outcome).unwrap(),
                expected
            );
        }
    }

    #[test]
    fn final_projection_preserves_published_indices_and_promotes_only_the_preferred_layout() {
        let request = request();
        let prepared = request.problem.prepare().unwrap();
        let outcome = fixture_outcome(request.solve_mode);
        assert!(outcome.solutions.len() > 1);
        let mut snapshot = JobSnapshot::new(String::new(), 0);
        snapshot.results = outcome
            .solutions
            .iter()
            .rev()
            .map(|s| Solution::from_best(&prepared, s).unwrap())
            .collect();
        let keys = snapshot
            .results
            .iter()
            .map(|s| s.layout_key.clone())
            .collect::<Vec<_>>();
        project(&mut snapshot, &request, &outcome);
        assert!(snapshot.status == JobStatus::Completed);
        assert!(snapshot.enumeration_complete);
        assert_eq!(snapshot.proof, Some(outcome.proof));
        let payload = serde_json::to_value(&snapshot).unwrap();
        assert!(payload["result"].get("layoutKey").is_none());
        assert!(
            payload["results"]
                .as_array()
                .unwrap()
                .iter()
                .all(|solution| solution.get("layoutKey").is_none())
        );
        assert_eq!(
            keys,
            snapshot
                .results
                .iter()
                .map(|s| s.layout_key.clone())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            snapshot
                .results
                .iter()
                .filter(|s| s.display.status == "proven_optimal")
                .count(),
            1
        );
        let stopped = cancel_before_presentation(outcome, request.solve_mode, true);
        project(&mut snapshot, &request, &stopped);
        assert!(snapshot.status == JobStatus::Cancelled);
        assert!(!snapshot.enumeration_complete);
        assert_eq!(snapshot.proof.unwrap().minimum_link_count, None);
    }

    #[test]
    fn cancelled_opt_keeps_a_best_known_result_but_no_enumeration_or_link_proof() {
        let mut request = request();
        request.solve_mode = SolveMode::OneMinNL;
        let outcome = fixture_outcome(request.solve_mode);
        let SolveResult::Optimal(optimal) = outcome.result else {
            panic!()
        };
        let best = solver_api::BestKnownSolution {
            node_count: optimal.node_count,
            link_count: optimal.link_count,
            physical_link_count: optimal.physical_link_count,
            discard_link_count: optimal.discard_link_count,
            canonical_graph_key: optimal.canonical_graph_key,
            graph: optimal.graph,
            validation: optimal.validation,
        };
        let stopped = SolveOutcome::new(
            SolveResult::Incomplete(IncompleteResult {
                reason: IncompleteReason::Cancelled,
                best_known: Some(best),
                proof: optimal.proof,
            }),
            SolveMode::OneMinNL,
            Vec::new(),
        );
        let mut snapshot = JobSnapshot::new(String::new(), 0);
        project(&mut snapshot, &request, &stopped);
        assert!(snapshot.status == JobStatus::Cancelled);
        assert!(!snapshot.enumeration_complete);
        assert!(snapshot.results.is_empty());
        assert_eq!(snapshot.result.unwrap().display.status, "best_known");
        assert_eq!(
            snapshot.proof,
            Some(OptimalityProof {
                minimum_node_count: Some(7),
                minimum_link_count: None
            })
        );
    }
}
