mod contract;
mod history;
mod jobs;
mod shutdown;
use shutdown::{resume_jobs, shutdown_jobs};

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use uuid::Uuid;

use contract::{Solution, SolveRequest, SolverProgress, UnsatProof};
use history::{apply_history_ops, load_history};

const JOB_SNAPSHOT_EVENT: &str = "job-snapshot";

#[derive(Default)]
struct AppState {
    lifecycle: Mutex<shutdown::Lifecycle>,
    jobs: Arc<RwLock<HashMap<Uuid, Arc<Job>>>>,
}

pub(crate) struct Job {
    cancel: AtomicBool,
    accepts_cancel: AtomicBool,
    snapshot: Mutex<JobSnapshot>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JobSnapshot {
    job_id: Uuid,
    status: JobStatus,
    started_at_ms: u64,
    progress: Option<SolverProgress>,
    proof: Option<solver_api::OptimalityProof>,
    sequence: u64,
    result: Option<Solution>,
    /// Populated during/after full-N enumeration. Empty in classic single-solution mode.
    results: Vec<Solution>,
    enumeration_complete: bool,
    /// Progress-only emit: `result`/`results` are empty on purpose; UI must keep its copies.
    #[serde(default)]
    results_omitted: bool,
    /// Incremental full-N emit: `result` is the newly found layout; append it locally.
    #[serde(default)]
    result_appended: bool,
    /// Server result count, also included with progress to detect missed appends.
    #[serde(default)]
    results_len: usize,
    error: Option<String>,
    /// Present for a finite global contradiction from the solver.
    #[serde(skip_serializing_if = "Option::is_none")]
    unsat: Option<UnsatProof>,
}

#[derive(Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum JobStatus {
    Running,
    Cancelling,
    Completed,
    Cancelled,
    Incomplete,
    Unsat,
    Failed,
}

impl Job {
    fn new(id: Uuid) -> Arc<Self> {
        Arc::new(Self {
            cancel: AtomicBool::new(false),
            accepts_cancel: AtomicBool::new(true),
            snapshot: Mutex::new(JobSnapshot {
                job_id: id,
                status: JobStatus::Running,
                started_at_ms: now_ms(),
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
            }),
        })
    }

    fn request_cancel(&self) -> bool {
        let mut snapshot = self.snapshot.lock().expect("job snapshot lock poisoned");
        if snapshot.status != JobStatus::Running || !self.accepts_cancel.load(Ordering::Relaxed) {
            return false;
        }
        self.cancel.store(true, Ordering::Relaxed);
        snapshot.status = JobStatus::Cancelling;
        snapshot.sequence += 1;
        true
    }

    fn finish_search(&self) {
        let _snapshot = self.snapshot.lock().expect("job snapshot lock poisoned");
        self.accepts_cancel.store(false, Ordering::Relaxed);
    }

    fn is_active(&self) -> bool {
        matches!(
            self.snapshot
                .lock()
                .expect("job snapshot lock poisoned")
                .status,
            JobStatus::Running | JobStatus::Cancelling
        )
    }

    fn current(&self) -> JobSnapshot {
        self.snapshot
            .lock()
            .expect("job snapshot lock poisoned")
            .clone()
    }

    pub(crate) fn update(&self, app: &AppHandle, update: impl FnOnce(&mut JobSnapshot)) {
        let snapshot = {
            let mut snapshot = self.snapshot.lock().expect("job snapshot lock poisoned");
            update(&mut snapshot);
            snapshot.sequence += 1;
            snapshot.results_omitted = false;
            snapshot.result_appended = false;
            snapshot.results_len = 0;
            snapshot.clone()
        };
        let _ = app.emit(JOB_SNAPSHOT_EVENT, &snapshot);
    }

    /// Hot-path telemetry: update progress without cloning solution graphs on the wire.
    pub(crate) fn update_progress(&self, app: &AppHandle, progress: SolverProgress) {
        let payload = {
            let mut snapshot = self.snapshot.lock().expect("job snapshot lock poisoned");
            snapshot.progress = Some(progress);
            snapshot.sequence += 1;
            JobSnapshot {
                job_id: snapshot.job_id,
                status: snapshot.status,
                started_at_ms: snapshot.started_at_ms,
                progress: snapshot.progress.clone(),
                proof: snapshot.proof,
                sequence: snapshot.sequence,
                result: None,
                results: Vec::new(),
                enumeration_complete: snapshot.enumeration_complete,
                results_omitted: true,
                result_appended: false,
                results_len: snapshot.results.len(),
                error: snapshot.error.clone(),
                unsat: None,
            }
        };
        let _ = app.emit(JOB_SNAPSHOT_EVENT, &payload);
    }

    /// Stream one new layout without cloning the accumulated results list.
    pub(crate) fn append_solution(&self, app: &AppHandle, solution: Solution) {
        let payload = {
            let mut snapshot = self.snapshot.lock().expect("job snapshot lock poisoned");
            if !matches!(snapshot.status, JobStatus::Running | JobStatus::Cancelling) {
                return;
            }
            if snapshot
                .results
                .iter()
                .any(|existing| existing.has_same_layout(&solution))
            {
                return;
            }
            if snapshot.result.is_none() {
                snapshot.result = Some(solution.clone());
            }
            snapshot.results.push(solution.clone());
            snapshot.sequence += 1;
            let results_len = snapshot.results.len();
            JobSnapshot {
                job_id: snapshot.job_id,
                status: snapshot.status,
                started_at_ms: snapshot.started_at_ms,
                progress: snapshot.progress.clone(),
                proof: snapshot.proof,
                sequence: snapshot.sequence,
                result: Some(solution),
                results: Vec::new(),
                enumeration_complete: snapshot.enumeration_complete,
                results_omitted: false,
                result_appended: true,
                results_len,
                error: snapshot.error.clone(),
                unsat: None,
            }
        };
        let _ = app.emit(JOB_SNAPSHOT_EVENT, &payload);
    }

    #[cfg(test)]
    fn set_status(&self, status: JobStatus) {
        self.snapshot
            .lock()
            .expect("job snapshot lock poisoned")
            .status = status;
    }
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn create_job(
    app: AppHandle,
    state: State<'_, AppState>,
    request: SolveRequest,
) -> Result<Uuid, String> {
    let prepared = request
        .problem
        .prepare()
        .map_err(|error| error.to_string())?;
    let id = Uuid::new_v4();
    let job = Job::new(id);
    shutdown::start(app.clone(), &state, id, Arc::clone(&job), request, prepared)?;
    let _ = app.emit(JOB_SNAPSHOT_EVENT, &job.current());

    Ok(id)
}

// Retain a few terminal snapshots when a disconnected frontend cannot acknowledge them.
fn register_job(state: &AppState, id: Uuid, job: Arc<Job>) -> Result<(), String> {
    let mut jobs = state.jobs.write().map_err(|_| "jobs lock poisoned")?;
    if jobs.values().any(|job| job.is_active()) {
        return Err("a solver job is already running".into());
    }
    let mut finished: Vec<_> = jobs
        .iter()
        .map(|(&id, job)| {
            (
                job.snapshot
                    .lock()
                    .expect("job snapshot lock poisoned")
                    .started_at_ms,
                id,
            )
        })
        .collect();
    finished.sort_unstable();
    for (_, id) in finished.iter().take(finished.len().saturating_sub(3)) {
        jobs.remove(id);
    }
    jobs.insert(id, job);
    Ok(())
}

fn release_finished_job(state: &AppState, id: Uuid) -> Result<(), String> {
    let mut jobs = state.jobs.write().map_err(|_| "jobs lock poisoned")?;
    if jobs.get(&id).is_some_and(|job| job.is_active()) {
        return Err("cannot release an active job".into());
    }
    jobs.remove(&id);
    Ok(())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn release_job(state: State<'_, AppState>, job_id: Uuid) -> Result<(), String> {
    release_finished_job(&state, job_id)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn get_job(state: State<'_, AppState>, job_id: Uuid) -> Result<JobSnapshot, String> {
    Ok(find_job(&state, job_id)?.current())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn cancel_job(
    app: AppHandle,
    state: State<'_, AppState>,
    job_id: Uuid,
) -> Result<JobSnapshot, String> {
    let job = find_job(&state, job_id)?;
    if job.request_cancel() {
        let _ = app.emit(JOB_SNAPSHOT_EVENT, &job.current());
    }
    Ok(job.current())
}

fn find_job(state: &AppState, id: Uuid) -> Result<Arc<Job>, String> {
    state
        .jobs
        .read()
        .expect("jobs lock poisoned")
        .get(&id)
        .cloned()
        .ok_or_else(|| "solver job not found".to_owned())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

/// Run the desktop application.
///
/// # Panics
///
/// Panics if the Tauri runtime cannot be built.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let package = app.package_info();
            let title = format!("{} · {}", package.name, package.version);
            if let Some(window) = app.get_webview_window("main") {
                window.set_title(&title)?;
            }
            let store = history::init_history_store(app.handle())
                .map_err(|error| -> Box<dyn std::error::Error> { error.into() })?;
            app.manage(store);
            app.manage(AppState::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            create_job,
            get_job,
            release_job,
            shutdown_jobs,
            resume_jobs,
            cancel_job,
            load_history,
            apply_history_ops
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                let state = app.state::<AppState>();
                if !state
                    .lifecycle
                    .lock()
                    .expect("job lifecycle lock poisoned")
                    .tasks
                    .is_empty()
                {
                    api.prevent_exit();
                    shutdown::exit_in_background(app.clone());
                }
            }
        });
}

#[cfg(test)]
mod registry_tests {
    use super::*;

    fn state() -> AppState {
        AppState::default()
    }

    #[test]
    fn finished_snapshots_are_bounded_and_explicitly_releasable() {
        let state = state();
        for _ in 0..12 {
            let id = Uuid::new_v4();
            let job = Job::new(id);
            register_job(&state, id, Arc::clone(&job)).unwrap();
            assert!(release_finished_job(&state, id).is_err());
            job.set_status(JobStatus::Completed);
            assert!(state.jobs.read().unwrap().len() <= 4);
        }
        let ids = state
            .jobs
            .read()
            .unwrap()
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for id in ids {
            release_finished_job(&state, id).unwrap();
        }
        assert!(state.jobs.read().unwrap().is_empty());
    }

    #[test]
    fn registration_checks_and_inserts_atomically() {
        let state = state();
        std::thread::scope(|scope| {
            let left = scope.spawn(|| {
                let id = Uuid::new_v4();
                register_job(&state, id, Job::new(id))
            });
            let right = scope.spawn(|| {
                let id = Uuid::new_v4();
                register_job(&state, id, Job::new(id))
            });
            assert_ne!(left.join().unwrap().is_ok(), right.join().unwrap().is_ok());
        });
        assert_eq!(state.jobs.read().unwrap().len(), 1);
    }
}

#[cfg(test)]
mod cancellation_tests {
    use super::*;
    #[test]
    fn cancellation_is_accepted_before_search_seals_and_not_during_presentation() {
        let early = Job::new(Uuid::nil());
        assert!(early.request_cancel());
        early.finish_search();
        assert!(early.cancel.load(Ordering::Relaxed));
        assert!(early.current().status == JobStatus::Cancelling);
        let late = Job::new(Uuid::nil());
        late.finish_search();
        assert!(!late.request_cancel());
        assert!(!late.cancel.load(Ordering::Relaxed));
    }
}
