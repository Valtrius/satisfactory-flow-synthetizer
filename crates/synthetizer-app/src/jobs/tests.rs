use super::projection::terminal_solutions;
use super::*;
use crate::solution::Solution;
use solver_api::{
    BestKnownSolution, IncompleteResult, OptimalSolution, OptimalityProof, PhysicalGraph,
    ProofSummary,
};
use solver_api::{
    EnumerationStatus, IncompleteReason, SolveMode, SolveOutcome, SolveResult, SolverProgress,
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
        "../../../solver-core/tests/fixtures/preferred-order.json"
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
    assert!(project_outcome(&mut snapshot, request.solve_mode, &wrong_problem, &outcome).is_err());
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

#[test]
fn hard_interruption_uses_the_same_proof_and_status_rules_as_normal_projection() {
    let prepared = request().problem.prepare().unwrap();
    for mode in [SolveMode::OneMinNL, SolveMode::AllMinNL, SolveMode::AllMinN] {
        let stopped = cancel_before_presentation(fixture_outcome(mode), mode, true);
        let SolveResult::Incomplete(exact) = &stopped.result else {
            panic!()
        };
        let best = exact.best_known.as_ref().unwrap();
        for reason in [
            IncompleteReason::Cancelled,
            IncompleteReason::WorkerFailed {
                detail: "worker trapped".into(),
            },
            IncompleteReason::DeadlineReached,
            IncompleteReason::ResourceLimit {
                detail: "host budget".into(),
            },
        ] {
            let mut snapshot = JobSnapshot::new("job".to_owned(), 42);
            if mode == SolveMode::OneMinNL {
                snapshot.result = Some(Solution::from_best(&prepared, best).unwrap());
                snapshot.finish_update();
            } else {
                for solution in &stopped.solutions {
                    let _ =
                        snapshot.append_solution(Solution::from_best(&prepared, solution).unwrap());
                }
            }
            let before = serde_json::to_value(&snapshot).unwrap();
            let packet =
                interruption_packet(&snapshot, mode, Some(best), &exact.proof, reason.clone())
                    .unwrap();
            let outcome = SolveOutcome::new(
                SolveResult::Incomplete(IncompleteResult {
                    reason,
                    best_known: Some(best.clone()),
                    proof: exact.proof.clone(),
                }),
                mode,
                stopped.solutions.clone(),
            );
            let mut projected = snapshot.clone();
            project_outcome(&mut projected, mode, &prepared, &outcome).unwrap();
            projected.finish_update();
            assert!(packet.status == projected.status);
            assert_eq!(packet.error, projected.error);
            assert_eq!(packet.proof, projected.proof);
            assert_eq!(packet.sequence, projected.sequence);
            assert_eq!(packet.results_len, snapshot.results.len());
            assert!(packet.results_omitted && !packet.result_appended);
            assert!(packet.result.is_none() && packet.results.is_empty());
            assert!(packet.unsat.is_none() && !packet.enumeration_complete);
            assert_eq!(packet.proof.unwrap().minimum_link_count, None);
            assert_eq!(before, serde_json::to_value(&snapshot).unwrap());
        }
    }
}

#[test]
fn hard_interruption_rejects_every_sealed_status() {
    let mut snapshot = JobSnapshot::new("job".to_owned(), 42);
    for status in [
        JobStatus::Completed,
        JobStatus::Cancelled,
        JobStatus::Failed,
        JobStatus::Incomplete,
        JobStatus::Unsat,
    ] {
        snapshot.status = status;
        assert!(
            interruption_packet(
                &snapshot,
                SolveMode::AllMinN,
                None,
                &ProofSummary::default(),
                IncompleteReason::Cancelled
            )
            .is_none()
        );
    }
}
