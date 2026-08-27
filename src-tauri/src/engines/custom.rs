use std::{
    sync::{Mutex, atomic::Ordering},
    thread,
};

use custom_solver_adapter::{
    exact_adapter::PreparedAppRequest,
    presentation::{PresentedSolveOutcome, present_best_known_solution, present_solve_result},
};
use solver_api::{IncompleteReason, SolveResult, SolverEvent};
use solver_core::{
    SolveOptions, enumerate_with_component_resolver_and_observer,
    solve_with_component_resolver_and_observer,
};
use solver_db::{ComponentDatabase, PrewarmOptions, PrewarmScheduling};
use tauri::AppHandle;

use crate::{
    Job, JobStatus,
    contract::{Solution, SolveRequest, SolverProgress},
};

pub fn run_custom_job(
    app: &AppHandle,
    job: &Job,
    request: &SolveRequest,
    database: &ComponentDatabase,
) {
    let prepared = match request.to_custom_app().prepare() {
        Ok(prepared) => prepared,
        Err(error) => {
            job.update(app, |snapshot| {
                snapshot.status = JobStatus::Failed;
                snapshot.error = Some(error.to_string());
                snapshot.enumeration_complete = false;
            });
            return;
        }
    };

    let options = SolveOptions {
        max_nodes: None,
        worker_count: thread::available_parallelism().map_or(1, std::num::NonZero::get),
    };

    let (runtime, _catalog_load) = database.application_component_runtime(PrewarmOptions {
        max_nodes: 1,
        max_boundary_ports: None,
        scheduling: PrewarmScheduling::Live,
    });
    let presentation_failure = Mutex::new(None);
    let observer = |event| {
        if let Err(error) = on_custom_event(app, job, &prepared, event) {
            let mut failure = presentation_failure
                .lock()
                .expect("presentation failure lock poisoned");
            if failure.is_none() {
                *failure = Some(error);
                job.cancel.store(true, Ordering::Relaxed);
            }
        }
    };
    let result = if request.enumerate_all_at_n {
        enumerate_with_component_resolver_and_observer(
            &prepared.problem,
            &options,
            &job.cancel,
            &runtime,
            &observer,
        )
    } else {
        solve_with_component_resolver_and_observer(
            &prepared.problem,
            &options,
            &job.cancel,
            &runtime,
            &observer,
        )
    };

    if let Some(error) = presentation_failure
        .into_inner()
        .expect("presentation failure lock poisoned")
    {
        job.update(app, |snapshot| {
            snapshot.status = JobStatus::Failed;
            snapshot.error = Some(error);
            snapshot.enumeration_complete = false;
            snapshot.unsat = None;
        });
        return;
    }

    match result {
        Ok(solve_result) => apply_custom_terminal(app, job, &prepared, &solve_result),
        Err(error) => job.update(app, |snapshot| {
            snapshot.status = JobStatus::Failed;
            snapshot.error = Some(error.to_string());
            snapshot.enumeration_complete = false;
            snapshot.unsat = None;
        }),
    }
}

fn on_custom_event(
    app: &AppHandle,
    job: &Job,
    prepared: &PreparedAppRequest,
    event: SolverEvent,
) -> Result<(), String> {
    match event {
        SolverEvent::Progress(progress) => {
            job.update_progress(app, SolverProgress::from_custom(progress));
            Ok(())
        }
        SolverEvent::Incumbent(best) => {
            let presented = present_best_known_solution(prepared, &best, None)
                .map_err(|error| format!("presentation failed: {error}"))?;
            let solution = Solution::from_custom(presented);
            job.update(app, |snapshot| {
                if !matches!(snapshot.status, JobStatus::Running | JobStatus::Cancelling) {
                    return;
                }
                snapshot.result = Some(solution);
            });
            Ok(())
        }
        SolverEvent::SolutionFound(layout) => {
            let presented = present_best_known_solution(prepared, &layout, None)
                .map_err(|error| format!("presentation failed: {error}"))?;
            // Enumerated layouts are distinct witnesses, never optimality claims.
            let solution = Solution::from_custom(presented);
            job.append_solution(app, solution);
            Ok(())
        }
    }
}

fn apply_custom_terminal(
    app: &AppHandle,
    job: &Job,
    prepared: &PreparedAppRequest,
    solve_result: &SolveResult,
) {
    let presented = match present_solve_result(prepared, solve_result) {
        Ok(presented) => presented,
        Err(error) => {
            job.update(app, |snapshot| {
                snapshot.status = JobStatus::Failed;
                snapshot.error = Some(format!("presentation failed: {error}"));
                snapshot.enumeration_complete = false;
                snapshot.unsat = None;
            });
            return;
        }
    };

    match presented {
        PresentedSolveOutcome::Optimal(solution) => job.update(app, |snapshot| {
            let solution = Solution::from_custom(solution);
            promote_optimal(&mut snapshot.results, &mut snapshot.result, solution);
            snapshot.status = JobStatus::Completed;
            snapshot.enumeration_complete = true;
            snapshot.progress = None;
            snapshot.unsat = None;
            snapshot.error = None;
        }),
        PresentedSolveOutcome::GloballyUnsat(proof) => job.update(app, |snapshot| {
            snapshot.status = JobStatus::Unsat;
            snapshot.result = None;
            snapshot.enumeration_complete = true;
            snapshot.progress = None;
            snapshot.unsat = Some(proof);
            snapshot.error = Some("No exact network exists for these rates.".to_owned());
        }),
        PresentedSolveOutcome::Incomplete(incomplete) => {
            let best = incomplete.best_known.map(Solution::from_custom);
            let (status, error) = match incomplete.reason {
                IncompleteReason::Cancelled => (JobStatus::Cancelled, None),
                IncompleteReason::DeadlineReached => (
                    JobStatus::Incomplete,
                    Some("Solve deadline reached before the proof finished.".to_owned()),
                ),
                IncompleteReason::ResourceLimit { detail } => (JobStatus::Incomplete, Some(detail)),
                IncompleteReason::WorkerFailed { detail } => (JobStatus::Failed, Some(detail)),
            };
            job.update(app, |snapshot| {
                snapshot.status = status;
                // Retain streamed enumeration layouts; fill classic best-known if needed.
                if snapshot.result.is_none() {
                    snapshot.result = best;
                }
                snapshot.enumeration_complete = false;
                snapshot.unsat = None;
                snapshot.error = error;
            });
        }
    }
}

fn promote_optimal(
    results: &mut Vec<Solution>,
    selected: &mut Option<Solution>,
    solution: Solution,
) {
    if let Some(layout) = results
        .iter_mut()
        .find(|layout| layout.has_same_layout(&solution))
    {
        *layout = solution.clone();
    } else if !results.is_empty() {
        results.push(solution.clone());
    }
    *selected = Some(solution);
}

#[cfg(test)]
mod tests {
    use crate::contract::{
        DisplayRate, GraphEdge, GraphNode, GraphNodeKind, SolutionStats, SolverEngine,
    };

    use super::*;

    fn solution(status: &str, edge_id: &str) -> Solution {
        let zero = DisplayRate {
            exact: "0".to_owned(),
            decimal: "0".to_owned(),
        };
        Solution {
            engine: SolverEngine::Custom,
            status: status.to_owned(),
            model_version: 4,
            proof: None,
            validation: None,
            stats: SolutionStats {
                node_count: 0,
                splitters: 0,
                mergers: 0,
                feedback_loops: 0,
                link_count: 1,
                checked_through: None,
                belt_count: None,
                internal_max_throughput: None,
                physical_link_count: Some(1),
                discard_link_count: Some(0),
            },
            total_input: zero.clone(),
            total_output: zero.clone(),
            discard_rate: zero.clone(),
            belt_rate: zero.clone(),
            nodes: vec![GraphNode {
                id: "input-0".to_owned(),
                kind: GraphNodeKind::Input,
                label: "Input".to_owned(),
            }],
            edges: vec![GraphEdge {
                id: edge_id.to_owned(),
                source: "input-0".to_owned(),
                target: "output-0".to_owned(),
                source_port: 0,
                target_port: 0,
                rate: zero,
                feedback: false,
                discarded: false,
            }],
            build_steps: Vec::new(),
        }
    }

    #[test]
    fn terminal_proof_upgrades_the_matching_enumerated_layout() {
        let mut results = vec![
            solution("best_known", "edge-0"),
            solution("best_known", "edge-1"),
        ];
        let mut selected = results.first().cloned();
        let mut optimal = solution("proven_optimal", "edge-1");
        optimal.proof = Some(solver_api::ProofSummary {
            proof_version: 1,
            initial_node_lower_bound: 0,
            node_counts_exhausted_through: None,
            link_groups_exhausted: 1,
            profiles_exhausted: 1,
            root_partitions_exhausted: 1,
        });

        promote_optimal(&mut results, &mut selected, optimal.clone());

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].status, "best_known");
        assert_eq!(results[1], optimal);
        assert_eq!(selected, Some(optimal));
    }
}
