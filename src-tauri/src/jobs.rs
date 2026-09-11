//! Production job runner and snapshot projection.

use crate::{
    Job, JobSnapshot, JobStatus,
    contract::{Solution, SolveRequest},
};
use solver_api::{
    EnumerationStatus, IncompleteReason, PreparedProblem, RunOptions, SolveMode, SolveOutcome,
    SolveResult, SolverEvent,
};
use std::sync::{Mutex, atomic::Ordering};
use synthetizer_app::{
    presentation::{PresentedSolveOutcome, present_solve_result},
    runtime,
};
use tauri::AppHandle;

pub fn run_job(app: &AppHandle, job: &Job, request: &SolveRequest, prepared: &PreparedProblem) {
    let options = RunOptions {
        mode: request.solve_mode,
        max_nodes: None,
        worker_count: std::thread::available_parallelism().map_or(1, std::num::NonZero::get),
    };
    let failure = Mutex::new(None);
    let result = runtime::solve(&prepared.problem, &options, &job.cancel, &|event| {
        let converted = match event {
            SolverEvent::Progress(progress) => {
                job.update_progress(app, progress);
                return;
            }
            SolverEvent::Incumbent(best) => {
                if request.solve_mode != SolveMode::OneMinNL {
                    return;
                }
                Solution::from_best(prepared, &best).map(|solution| {
                    job.update(app, |snapshot| {
                        snapshot.result = Some(solution);
                    });
                })
            }
            SolverEvent::SolutionFound(best) => Solution::from_best(prepared, &best)
                .map(|solution| job.append_solution(app, solution)),
        };
        if let Err(error) = converted {
            failure
                .lock()
                .expect("presentation failure")
                .get_or_insert(error.to_string());
            job.cancel.store(true, Ordering::Relaxed);
        }
    });
    job.finish_search();
    let error = failure.into_inner().expect("presentation failure");
    if let Some(error) = error {
        fail(app, job, error);
        return;
    }
    let outcome = match result {
        Ok(outcome) => outcome,
        Err(error) => {
            fail(app, job, error.to_string());
            return;
        }
    };
    let outcome = cancel_before_presentation(
        outcome,
        request.solve_mode,
        job.cancel.load(Ordering::Relaxed),
    );
    apply_outcome(app, job, request, prepared, &outcome);
}

fn cancel_before_presentation(
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

fn apply_outcome(
    app: &AppHandle,
    job: &Job,
    request: &SolveRequest,
    prepared: &PreparedProblem,
    outcome: &SolveOutcome,
) {
    let presented = match present_solve_result(prepared, &outcome.result) {
        Ok(presented) => presented,
        Err(error) => {
            fail(app, job, error.to_string());
            return;
        }
    };
    let solutions = outcome
        .solutions
        .iter()
        .map(|s| Solution::from_best(prepared, s))
        .collect::<Result<Vec<_>, _>>();
    let solutions = match solutions {
        Ok(s) => s,
        Err(e) => {
            fail(app, job, e.to_string());
            return;
        }
    };
    job.update(app, |snapshot| {
        apply_terminal(snapshot, request, prepared, outcome, presented, solutions);
    });
}

fn apply_terminal(
    snapshot: &mut JobSnapshot,
    request: &SolveRequest,
    prepared: &PreparedProblem,
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
                layout_key: solver_validation::layout_key(&prepared.problem, &exact.graph),
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
            if (request.solve_mode == SolveMode::OneMinNL || snapshot.result.is_none())
                && let (Some(display), SolveResult::Incomplete(exact)) =
                    (incomplete.best_known, &outcome.result)
                && let Some(best) = &exact.best_known
            {
                snapshot.result = Some(Solution {
                    display,
                    layout_key: solver_validation::layout_key(&prepared.problem, &best.graph),
                });
            }
        }
    }
}

fn fail(app: &AppHandle, job: &Job, error: String) {
    job.update(app, |snapshot| {
        snapshot.status = JobStatus::Failed;
        snapshot.error = Some(error);
        snapshot.enumeration_complete = false;
    });
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
            "../../crates/solver-core/tests/fixtures/preferred-order.json"
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
        let solutions = outcome
            .solutions
            .iter()
            .map(|s| Solution::from_best(&prepared, s).unwrap())
            .collect();
        let presented = present_solve_result(&prepared, &outcome.result).unwrap();
        apply_terminal(snapshot, request, &prepared, outcome, presented, solutions);
    }

    #[test]
    fn final_projection_preserves_published_indices_and_promotes_only_the_preferred_layout() {
        let request = request();
        let prepared = request.problem.prepare().unwrap();
        let outcome = fixture_outcome(request.solve_mode);
        assert!(outcome.solutions.len() > 1);
        let mut snapshot = Job::new(uuid::Uuid::nil()).current();
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
        let mut snapshot = Job::new(uuid::Uuid::nil()).current();
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
