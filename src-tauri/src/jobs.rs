//! Native job execution and event delivery.
use crate::{
    Job, JobStatus,
    contract::{Solution, SolveRequest},
};
use solver_api::{PreparedProblem, RunOptions, SolveMode, SolveOutcome, SolverEvent};
use std::sync::{Mutex, atomic::Ordering};
use synthetizer_app::{
    jobs::{cancel_before_presentation, project_outcome},
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

fn apply_outcome(
    app: &AppHandle,
    job: &Job,
    request: &SolveRequest,
    prepared: &PreparedProblem,
    outcome: &SolveOutcome,
) {
    job.update(app, |snapshot| {
        if let Err(error) = project_outcome(snapshot, request.solve_mode, prepared, outcome) {
            snapshot.status = JobStatus::Failed;
            snapshot.error = Some(error.to_string());
            snapshot.enumeration_complete = false;
        }
    });
}

fn fail(app: &AppHandle, job: &Job, error: String) {
    job.update(app, |snapshot| {
        snapshot.status = JobStatus::Failed;
        snapshot.error = Some(error);
        snapshot.enumeration_complete = false;
    });
}
