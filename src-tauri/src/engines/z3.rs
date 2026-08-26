use std::{sync::Mutex, sync::atomic::Ordering};

use solver_z3::{Solution as Z3Solution, SolveTermination, SolverEvent, solve_exact};
use tauri::AppHandle;

use crate::{
    Job, JobStatus,
    contract::{Solution, SolveRequest, SolverProgress},
    layout_identity,
};

pub fn run_z3_job(app: &AppHandle, job: &Job, request: &SolveRequest) {
    let z3_request = request.to_z3();
    let conversion_failure = Mutex::new(None);
    let result = solve_exact(&z3_request, &job.cancel, |event| match event {
        SolverEvent::Progress(progress) => {
            job.update_progress(app, SolverProgress::from_z3(progress));
        }
        SolverEvent::SolutionFound(solution) => match z3_solution(request, *solution) {
            Ok(solution) => job.append_solution(app, solution),
            Err(error) => {
                let mut failure = conversion_failure
                    .lock()
                    .expect("Z3 conversion failure lock poisoned");
                if failure.is_none() {
                    *failure = Some(error);
                    job.cancel.store(true, Ordering::Relaxed);
                }
            }
        },
    });

    if let Some(error) = conversion_failure
        .into_inner()
        .expect("Z3 conversion failure lock poisoned")
    {
        job.update(app, |snapshot| {
            snapshot.status = JobStatus::Failed;
            snapshot.error = Some(error);
            snapshot.enumeration_complete = false;
        });
        return;
    }

    match result {
        Ok(SolveTermination::Completed(solution)) => {
            let solution = match z3_solution(request, *solution) {
                Ok(solution) => solution,
                Err(error) => return fail_conversion(app, job, error),
            };
            job.update(app, |snapshot| {
                snapshot.status = JobStatus::Completed;
                snapshot.result = Some(solution);
                snapshot.results.clear();
                snapshot.enumeration_complete = true;
                snapshot.progress = None;
                snapshot.unsat = None;
                snapshot.error = None;
            });
        }
        Ok(SolveTermination::Enumerated(solutions)) => job.update(app, |snapshot| {
            snapshot.status = JobStatus::Completed;
            if snapshot.results.is_empty() {
                snapshot.results = convert_solutions(request, solutions);
            }
            if snapshot.result.is_none() {
                snapshot.result = snapshot.results.first().cloned();
            }
            snapshot.enumeration_complete = true;
            snapshot.progress = None;
            snapshot.unsat = None;
            snapshot.error = None;
        }),
        Ok(SolveTermination::Incomplete { solutions, error }) => job.update(app, |snapshot| {
            snapshot.status = JobStatus::Incomplete;
            snapshot.error = Some(error.to_string());
            if snapshot.results.is_empty() {
                snapshot.results = convert_solutions(request, solutions);
            }
            if snapshot.result.is_none() {
                snapshot.result = snapshot.results.first().cloned();
            }
            snapshot.enumeration_complete = false;
            snapshot.unsat = None;
        }),
        Ok(SolveTermination::Cancelled { solutions }) => job.update(app, |snapshot| {
            snapshot.status = JobStatus::Cancelled;
            if !solutions.is_empty() && snapshot.results.is_empty() {
                snapshot.results = convert_solutions(request, solutions);
            }
            if snapshot.result.is_none() {
                snapshot.result = snapshot.results.first().cloned();
            }
            snapshot.enumeration_complete = false;
            snapshot.unsat = None;
        }),
        Err(error) => job.update(app, |snapshot| {
            snapshot.status = JobStatus::Failed;
            snapshot.error = Some(error.to_string());
            snapshot.enumeration_complete = false;
            snapshot.unsat = None;
        }),
    }
}

fn z3_solution(request: &SolveRequest, source: Z3Solution) -> Result<Solution, String> {
    let mut solution = Solution::from_z3(source);
    layout_identity::attach(request, &mut solution)?;
    Ok(solution)
}

fn convert_solutions(request: &SolveRequest, sources: Vec<Z3Solution>) -> Vec<Solution> {
    let mut solutions = Vec::new();
    for source in sources {
        let Ok(solution) = z3_solution(request, source) else {
            continue;
        };
        if !solutions
            .iter()
            .any(|existing: &Solution| existing.has_same_layout(&solution))
        {
            solutions.push(solution);
        }
    }
    solutions
}

fn fail_conversion(app: &AppHandle, job: &Job, error: String) {
    job.update(app, |snapshot| {
        snapshot.status = JobStatus::Failed;
        snapshot.error = Some(error);
        snapshot.enumeration_complete = false;
    });
}
