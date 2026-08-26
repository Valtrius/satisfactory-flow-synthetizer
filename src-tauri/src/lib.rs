mod contract;
mod engines;
mod history;
mod layout_identity;

use std::{
    collections::HashMap,
    fs,
    sync::{
        Arc, Condvar, Mutex, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use solver_db::{
    ComponentDatabase, ComponentDatabaseConfig, DatabaseMode, PrewarmOptions, PrewarmPartialReason,
    PrewarmResult, PrewarmScheduling,
};
use tauri::{AppHandle, Emitter, Manager, State};
use uuid::Uuid;

use contract::{Solution, SolveRequest, SolverEngine, SolverProgress, UnsatProof};
use engines::{run_custom_job, run_z3_job};
use history::{load_history, save_history};

const JOB_SNAPSHOT_EVENT: &str = "job-snapshot";

struct AppState {
    jobs: Arc<RwLock<HashMap<Uuid, Arc<Job>>>>,
    database: Arc<ComponentDatabase>,
    work_priority: Arc<WorkPriority>,
}

#[derive(Default)]
struct PriorityState {
    live_jobs: usize,
    shutdown: bool,
}

/// Live solves preempt optional component prewarming. Only complete prewarm cells are committed,
/// so interruption changes performance but never solver semantics.
#[derive(Default)]
struct WorkPriority {
    state: Mutex<PriorityState>,
    idle: Condvar,
    prewarm_cancel: AtomicBool,
}

struct LiveSolvePermit {
    priority: Arc<WorkPriority>,
}

impl WorkPriority {
    fn begin_live(self: &Arc<Self>) -> LiveSolvePermit {
        let mut state = self.state.lock().expect("work-priority lock poisoned");
        state.live_jobs = state.live_jobs.saturating_add(1);
        self.prewarm_cancel.store(true, Ordering::Relaxed);
        LiveSolvePermit {
            priority: Arc::clone(self),
        }
    }

    fn wait_for_prewarm(&self) -> bool {
        let mut state = self.state.lock().expect("work-priority lock poisoned");
        while state.live_jobs != 0 && !state.shutdown {
            state = self
                .idle
                .wait(state)
                .expect("work-priority lock poisoned while waiting");
        }
        if state.shutdown {
            return false;
        }
        self.prewarm_cancel.store(false, Ordering::Relaxed);
        true
    }

    fn finish_live(&self) {
        let mut state = self.state.lock().expect("work-priority lock poisoned");
        state.live_jobs = state
            .live_jobs
            .checked_sub(1)
            .expect("live solve permit count underflow");
        if state.live_jobs == 0 {
            self.idle.notify_all();
        }
    }

    fn shutdown(&self) {
        let mut state = self.state.lock().expect("work-priority lock poisoned");
        state.shutdown = true;
        self.prewarm_cancel.store(true, Ordering::Relaxed);
        self.idle.notify_all();
    }

    fn is_shutdown(&self) -> bool {
        self.state
            .lock()
            .expect("work-priority lock poisoned")
            .shutdown
    }
}

impl Drop for LiveSolvePermit {
    fn drop(&mut self) {
        self.priority.finish_live();
    }
}

pub(crate) struct Job {
    cancel: AtomicBool,
    snapshot: Mutex<JobSnapshot>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JobSnapshot {
    job_id: Uuid,
    status: JobStatus,
    started_at_ms: u64,
    progress: Option<SolverProgress>,
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
    /// When `result_appended`, server `results.len()` after the push (1-based seq).
    #[serde(default)]
    results_len: usize,
    error: Option<String>,
    /// Present only for Custom global UNSAT terminals.
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
            snapshot: Mutex::new(JobSnapshot {
                job_id: id,
                status: JobStatus::Running,
                started_at_ms: now_ms(),
                progress: None,
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
            JobSnapshot {
                job_id: snapshot.job_id,
                status: snapshot.status,
                started_at_ms: snapshot.started_at_ms,
                progress: snapshot.progress.clone(),
                result: None,
                results: Vec::new(),
                enumeration_complete: snapshot.enumeration_complete,
                results_omitted: true,
                result_appended: false,
                results_len: 0,
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
            let results_len = snapshot.results.len();
            JobSnapshot {
                job_id: snapshot.job_id,
                status: snapshot.status,
                started_at_ms: snapshot.started_at_ms,
                progress: snapshot.progress.clone(),
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
    let running = state
        .jobs
        .read()
        .expect("jobs lock poisoned")
        .values()
        .filter(|job| {
            matches!(
                job.current().status,
                JobStatus::Running | JobStatus::Cancelling
            )
        })
        .count();
    if running >= 1 {
        return Err("a solver job is already running".to_owned());
    }

    let id = Uuid::new_v4();
    let job = Job::new(id);
    state
        .jobs
        .write()
        .expect("jobs lock poisoned")
        .insert(id, Arc::clone(&job));
    let _ = app.emit(JOB_SNAPSHOT_EVENT, &job.current());
    let engine = request.engine;
    let database = Arc::clone(&state.database);
    let priority = Arc::clone(&state.work_priority);
    tauri::async_runtime::spawn_blocking(move || {
        let _live_permit = priority.begin_live();
        match engine {
            SolverEngine::Z3 => run_z3_job(&app, &job, &request),
            SolverEngine::Custom => run_custom_job(&app, &job, &request, &database),
        }
    });

    Ok(id)
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
    if job.current().status == JobStatus::Running {
        job.cancel.store(true, Ordering::Relaxed);
        job.update(&app, |snapshot| snapshot.status = JobStatus::Cancelling);
    }
    Ok(job.current())
}

fn cancel_all_jobs(state: &AppState) {
    state.work_priority.shutdown();
    for job in state.jobs.read().expect("jobs lock poisoned").values() {
        if matches!(
            job.current().status,
            JobStatus::Running | JobStatus::Cancelling
        ) {
            job.cancel.store(true, Ordering::Relaxed);
            job.set_status(JobStatus::Cancelling);
        }
    }
}

fn component_options(scheduling: PrewarmScheduling) -> PrewarmOptions {
    PrewarmOptions {
        max_nodes: 1,
        max_boundary_ports: None,
        scheduling,
    }
}

fn open_component_database(app: &AppHandle) -> Arc<ComponentDatabase> {
    let Ok(app_data) = app.path().app_local_data_dir() else {
        return Arc::new(ComponentDatabase::disabled());
    };
    let directory = app_data.join("exact-components-v1");
    if fs::create_dir_all(&directory).is_err() {
        return Arc::new(ComponentDatabase::disabled());
    }
    let config = ComponentDatabaseConfig {
        mode: DatabaseMode::Path(directory.join("components.sqlite3")),
        ..ComponentDatabaseConfig::default()
    };
    Arc::new(ComponentDatabase::open(&config).unwrap_or_else(|_| ComponentDatabase::disabled()))
}

fn start_background_prewarm(database: Arc<ComponentDatabase>, priority: Arc<WorkPriority>) {
    if !database.is_enabled() {
        return;
    }
    drop(thread::spawn(move || {
        loop {
            if !priority.wait_for_prewarm() {
                return;
            }
            match database.prewarm_cancellable(
                component_options(PrewarmScheduling::Background),
                &priority.prewarm_cancel,
            ) {
                PrewarmResult::Partial {
                    reason: PrewarmPartialReason::Cancelled,
                    ..
                } if !priority.is_shutdown() => {}
                PrewarmResult::CompleteThrough { .. } | PrewarmResult::Partial { .. } => return,
            }
        }
    }));
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
            let database = open_component_database(app.handle());
            let work_priority = Arc::new(WorkPriority::default());
            start_background_prewarm(Arc::clone(&database), Arc::clone(&work_priority));
            app.manage(AppState {
                jobs: Arc::new(RwLock::new(HashMap::new())),
                database,
                work_priority,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            create_job,
            get_job,
            cancel_job,
            load_history,
            save_history
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if matches!(
                event,
                tauri::RunEvent::Exit | tauri::RunEvent::ExitRequested { .. }
            ) {
                cancel_all_jobs(&app.state::<AppState>());
            }
        });
}

#[cfg(test)]
mod tests {
    use std::{sync::mpsc, time::Duration};

    use super::*;

    #[test]
    fn prewarm_waits_until_every_live_solver_finishes() {
        let priority = Arc::new(WorkPriority::default());
        let first = priority.begin_live();
        let second = priority.begin_live();
        let waiter_priority = Arc::clone(&priority);
        let (sender, receiver) = mpsc::channel();
        let waiter = thread::spawn(move || {
            sender.send(waiter_priority.wait_for_prewarm()).unwrap();
        });

        drop(first);
        assert!(receiver.recv_timeout(Duration::from_millis(20)).is_err());
        drop(second);
        assert!(receiver.recv_timeout(Duration::from_secs(2)).unwrap());
        waiter.join().unwrap();
    }
}
