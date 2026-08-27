use solver_z3::{SolveTermination, SolverEvent, solve_exact};
use tauri::AppHandle;

use crate::{
    Job, JobStatus,
    contract::{Solution, SolveRequest, SolverProgress},
};

pub fn run_z3_job(app: &AppHandle, job: &Job, request: &SolveRequest) {
    let z3_request = request.to_z3();
    let result = solve_exact(&z3_request, &job.cancel, |event| match event {
        SolverEvent::Progress(progress) => {
            job.update_progress(app, SolverProgress::from_z3(progress));
        }
        SolverEvent::SolutionFound(solution) => {
            job.append_solution(app, Solution::from_z3(*solution));
        }
    });

    match result {
        Ok(SolveTermination::Completed(solution)) => job.update(app, |snapshot| {
            snapshot.status = JobStatus::Completed;
            snapshot.result = Some(Solution::from_z3(*solution));
            snapshot.results.clear();
            snapshot.enumeration_complete = true;
            snapshot.progress = None;
            snapshot.unsat = None;
            snapshot.error = None;
        }),
        Ok(SolveTermination::Enumerated(solutions)) => job.update(app, |snapshot| {
            snapshot.status = JobStatus::Completed;
            if snapshot.results.is_empty() {
                snapshot.results = solutions.into_iter().map(Solution::from_z3).collect();
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
                snapshot.results = solutions.into_iter().map(Solution::from_z3).collect();
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
                snapshot.results = solutions.into_iter().map(Solution::from_z3).collect();
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
