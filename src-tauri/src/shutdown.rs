//! Solver threads remain owned until shutdown has joined them.
use crate::{AppState, Job, contract::SolveRequest, jobs::run_job, register_job};
use solver_api::PreparedProblem;
use std::{
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use uuid::Uuid;

#[derive(Default)]
pub(crate) struct Lifecycle {
    pub closing: bool,
    pub tasks: Vec<JoinHandle<()>>,
    exit_waiter: bool,
}

pub(crate) fn start(
    app: AppHandle,
    state: &AppState,
    id: Uuid,
    job: Arc<Job>,
    request: SolveRequest,
    prepared: PreparedProblem,
) -> Result<(), String> {
    let mut lifecycle = state
        .lifecycle
        .lock()
        .map_err(|_| "job lifecycle lock poisoned")?;
    if lifecycle.closing {
        return Err("application is closing".into());
    }
    reap(&mut lifecycle)?;
    register_job(state, id, Arc::clone(&job))?;
    match thread::Builder::new()
        .name("solver-job".into())
        .spawn(move || run_job(&app, &job, &request, &prepared))
    {
        Ok(task) => {
            lifecycle.tasks.push(task);
            Ok(())
        }
        Err(error) => {
            state
                .jobs
                .write()
                .map_err(|_| "jobs lock poisoned")?
                .remove(&id);
            Err(format!("cannot start solver job: {error}"))
        }
    }
}

fn reap(lifecycle: &mut Lifecycle) -> Result<(), String> {
    let mut failed = false;
    for index in (0..lifecycle.tasks.len()).rev() {
        if lifecycle.tasks[index].is_finished() {
            failed |= lifecycle.tasks.swap_remove(index).join().is_err();
        }
    }
    if failed {
        Err("a solver task failed during shutdown".into())
    } else {
        Ok(())
    }
}

fn wait_for_tasks(lifecycle: &Mutex<Lifecycle>, timeout: Duration) -> Result<(), String> {
    let started = Instant::now();
    loop {
        {
            let mut state = lifecycle
                .lock()
                .map_err(|_| "job lifecycle lock poisoned")?;
            reap(&mut state)?;
            if state.tasks.is_empty() {
                return Ok(());
            }
        }
        if started.elapsed() >= timeout {
            return Err("solver cleanup has not finished; its tasks are still owned and cancellation remains requested".into());
        }
        thread::sleep(Duration::from_millis(25));
    }
}

pub(crate) fn stop_and_join(state: &AppState) -> Result<(), String> {
    {
        let mut lifecycle = state
            .lifecycle
            .lock()
            .map_err(|_| "job lifecycle lock poisoned")?;
        lifecycle.closing = true;
        for job in state
            .jobs
            .read()
            .map_err(|_| "jobs lock poisoned")?
            .values()
        {
            job.request_cancel();
        }
    }
    wait_for_tasks(&state.lifecycle, Duration::from_secs(30))
}

#[tauri::command]
pub(crate) async fn shutdown_jobs(app: AppHandle) -> Result<Vec<crate::JobSnapshot>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        stop_and_join(&state)?;
        let jobs = state.jobs.read().map_err(|_| "jobs lock poisoned")?;
        Ok(jobs.values().map(|job| job.current()).collect())
    })
    .await
    .map_err(|e| format!("cannot join shutdown task: {e}"))?
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn resume_jobs(state: tauri::State<'_, AppState>) -> Result<(), String> {
    state
        .lifecycle
        .lock()
        .map_err(|_| "job lifecycle lock poisoned")?
        .closing = false;
    Ok(())
}

/// Fallback for an exit that did not pass through the frontend close handler.
pub(crate) fn exit_in_background(app: AppHandle) {
    {
        let state = app.state::<AppState>();
        let mut lifecycle = state.lifecycle.lock().expect("job lifecycle lock poisoned");
        if lifecycle.exit_waiter {
            return;
        }
        lifecycle.exit_waiter = true;
    }
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    tauri::async_runtime::spawn_blocking(move || {
        loop {
            match stop_and_join(&app.state::<AppState>()) {
                Ok(()) => {
                    app.exit(0);
                    return;
                }
                Err(error) => {
                    let retry = app
                        .dialog()
                        .message(error)
                        .title("Solver cleanup failed")
                        .kind(MessageDialogKind::Error)
                        .buttons(MessageDialogButtons::OkCancelCustom(
                            "Retry cleanup".into(),
                            "Return to app".into(),
                        ))
                        .blocking_show();
                    if !retry && let Some(window) = app.get_webview_window("main") {
                        let state = app.state::<AppState>();
                        let mut lifecycle =
                            state.lifecycle.lock().expect("job lifecycle lock poisoned");
                        lifecycle.closing = false;
                        lifecycle.exit_waiter = false;
                        let _ = window.show();
                        return;
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_timeout_retains_the_task_for_a_later_join() {
        let (send, receive) = std::sync::mpsc::channel();
        let task = thread::spawn(move || {
            receive.recv().unwrap();
        });
        let lifecycle = Mutex::new(Lifecycle {
            closing: true,
            tasks: vec![task],
            exit_waiter: false,
        });
        assert!(wait_for_tasks(&lifecycle, Duration::ZERO).is_err());
        assert_eq!(lifecycle.lock().unwrap().tasks.len(), 1);
        send.send(()).unwrap();
        wait_for_tasks(&lifecycle, Duration::from_secs(2)).unwrap();
        assert!(lifecycle.lock().unwrap().tasks.is_empty());
    }
}
