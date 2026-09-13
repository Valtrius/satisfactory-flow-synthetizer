//! Native ownership only: threads, process sessions, wall-clock telemetry and delivery.
use crate::{
    Counts, Failure,
    diagnostics::RootStats,
    leaf::{Completion, LeafContext, LeafDriver, LeafPhase, LeafTelemetry, ResponseKind},
    planner::{ExactPlanner, LeafId, LeafTask, PlannerEvent, PlannerUpdate},
    process::Session,
};
use solver_api::{
    IncompleteReason, IncompleteResult, Problem, RunOptions, SolveObserver, SolveResult,
    SolverError,
};
use std::{
    collections::BTreeMap,
    fmt::Write as _,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

struct WorkerMessage {
    event: PlannerEvent,
    trace: Option<String>,
}

struct Telemetry<'a> {
    stats: &'a mut Option<RootStats>,
    started: Option<Instant>,
}
impl LeafTelemetry for Telemetry<'_> {
    fn start(&mut self, _phase: LeafPhase) {
        self.started = self.stats.as_ref().map(|_| Instant::now());
    }
    fn stop(&mut self, phase: LeafPhase) {
        if let (Some(stats), Some(started)) = (self.stats.as_mut(), self.started.take()) {
            match phase {
                LeafPhase::Validation => stats.validation += started.elapsed(),
                LeafPhase::Identity => stats.identity += started.elapsed(),
            }
        }
    }
    fn model(&mut self) {
        if let Some(stats) = self.stats.as_mut() {
            stats.models += 1;
        }
    }
    fn duplicate(&mut self) {
        if let Some(stats) = self.stats.as_mut() {
            stats.duplicates += 1;
        }
    }
    fn valid(&mut self) {
        if let Some(stats) = self.stats.as_mut() {
            stats.valid += 1;
        }
    }
}

fn drive_leaf(
    context: &LeafContext,
    task: LeafTask,
    cancel: &AtomicBool,
    stop: &AtomicBool,
    send: &mpsc::Sender<WorkerMessage>,
    stats: &mut Option<RootStats>,
) -> Result<Completion, Failure> {
    if cancel.load(Ordering::Relaxed) || stop.load(Ordering::Relaxed) {
        return Err(Failure::Cancelled);
    }
    if task.root.impossible {
        return Ok(Completion::Exhausted);
    }
    let mut leaf = LeafDriver::new(context, task.spec);
    let mut session = Session::new()?;
    let mut reply = None;
    loop {
        let cancelled = cancel.load(Ordering::Relaxed) || stop.load(Ordering::Relaxed);
        let action = leaf.advance(
            reply.as_deref(),
            cancelled,
            &mut Telemetry {
                stats,
                started: None,
            },
        )?;
        if let Some(witness) = action.witness {
            send.send(WorkerMessage {
                event: PlannerEvent::Witness(task.id, witness),
                trace: None,
            })
            .map_err(|e| Failure::Worker(e.to_string()))?;
        }
        if let Some(completion) = action.completion {
            return Ok(completion);
        }
        if !action.commands.is_empty() {
            session.write(&action.commands)?;
        }
        let started = stats
            .as_ref()
            .filter(|_| action.response == Some(ResponseKind::CheckSat))
            .map(|_| Instant::now());
        let response = session.response(cancel, stop);
        if let (Some(stats), Some(started)) = (stats.as_mut(), started) {
            stats.check += started.elapsed();
        }
        reply = Some(response?);
    }
    // Session::drop reaps the backend and joins its reader before Retired is sent.
}

fn worker(
    context: &LeafContext,
    receive: &Mutex<mpsc::Receiver<LeafTask>>,
    send: &mpsc::Sender<WorkerMessage>,
    cancel: &AtomicBool,
    stop: &AtomicBool,
    origin: Instant,
    diagnostics: bool,
) {
    loop {
        let task = receive
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .recv();
        let Ok(task) = task else { break };
        let mut stats = diagnostics.then(RootStats::default);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            drive_leaf(context, task, cancel, stop, send, &mut stats)
        }))
        .unwrap_or_else(|_| Err(Failure::Worker("Solver worker panicked".into())));
        let trace = stats.map(|stats| {
            let verdict = match &result {
                Ok(Completion::Exhausted) => "exhausted",
                Ok(Completion::Optimum) => "optimum",
                Err(Failure::Cancelled) => "cancelled",
                Err(Failure::Worker(_)) => "failed",
            };
            stats.record(
                task.id.index,
                task.nodes,
                task.links,
                &task.root,
                verdict,
                origin,
            )
        });
        if send
            .send(WorkerMessage {
                event: PlannerEvent::Retired(task.id, result),
                trace,
            })
            .is_err()
        {
            break;
        }
    }
}

fn elapsed(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn publish(
    planner: &ExactPlanner,
    update: PlannerUpdate,
    traces: &mut BTreeMap<LeafId, String>,
    observer: &dyn SolveObserver,
    started: Instant,
) -> Vec<LeafTask> {
    for group in update.closed_groups {
        for job in group.jobs {
            let Some(mut trace) = traces.remove(&LeafId {
                group: group.group,
                index: job.index,
            }) else {
                continue;
            };
            if group.adaptive {
                let trigger = job.trigger_ms.map_or_else(
                    || "null".into(),
                    |ms| format!("{}.{:03}", ms / 1000, ms % 1000),
                );
                trace.pop();
                write!(trace,",\"parent_root\":{},\"child_count\":{},\"proof_committed\":{},\"refinement_trigger_s\":{trigger},\"refinement_grace_ms\":0}}",job.parent,job.child_count,job.committed).expect("String write");
            }
            observer.on_event(planner.root_progress(
                group.nodes,
                group.links,
                elapsed(started),
                trace,
            ));
        }
    }
    for event in update.events {
        observer.on_event(event);
    }
    update.dispatch
}

/// Each native branch keeps its original worker budget and independent ledger.
pub(crate) fn run(
    problem: &Problem,
    options: &RunOptions,
    cancel: &AtomicBool,
    observer: &dyn SolveObserver,
    counts: Counts,
) -> Result<SolveResult, SolverError> {
    let started = Instant::now();
    // The outer SolutionCollector owns graphs; each branch retains its own identity ledger.
    let mut planner = ExactPlanner::streaming(problem, *options, counts)?;
    let context = planner.context().cloned();
    let stop = AtomicBool::new(false);
    let diagnostics = std::env::var_os("SOLVER_DIAGNOSTICS").is_some_and(|v| v == "1");
    let mut traces = BTreeMap::new();
    let mut join_failed = false;
    let (tasks, task_receive) = mpsc::channel();
    let task_receive = Mutex::new(task_receive);
    thread::scope(|scope| {
        let (send, receive) = mpsc::channel();
        let mut handles = Vec::new();
        if let Some(context) = &context {
            for _ in 0..options.worker_count {
                let receive = &task_receive;
                let send = send.clone();
                let stop = &stop;
                handles.push(scope.spawn(move || {
                    worker(context, receive, &send, cancel, stop, started, diagnostics);
                }));
            }
        }
        drop(send);
        let mut last = Instant::now();
        loop {
            if cancel.load(Ordering::Relaxed) {
                planner.cancel();
            }
            let update = planner.poll(elapsed(started));
            let dispatch = publish(&planner, update, &mut traces, observer, started);
            if cancel.load(Ordering::Relaxed) {
                planner.cancel();
            }
            stop.store(planner.stopping(), Ordering::Relaxed);
            for task in dispatch {
                if tasks.send(task).is_err() {
                    let error = Failure::Worker("native worker queue disconnected".into());
                    let _ = planner
                        .accept(PlannerEvent::Retired(task.id, Err(error)), elapsed(started));
                }
            }
            if planner.result().is_some() {
                break;
            }
            match receive.recv_timeout(Duration::from_millis(25)) {
                Ok(message) => {
                    if let (PlannerEvent::Retired(id, _), Some(trace)) =
                        (&message.event, message.trace)
                    {
                        traces.insert(*id, trace);
                    }
                    // Rejections poison the planner and still drain active sessions.
                    let _ = planner.accept(message.event, elapsed(started));
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    planner.fail(Failure::Worker("native worker channel disconnected".into()));
                    break;
                }
            }
            if last.elapsed() >= Duration::from_millis(250) {
                observer.on_event(planner.progress(elapsed(started)));
                last = Instant::now();
            }
        }
        stop.store(true, Ordering::Relaxed);
        drop(tasks);
        for handle in handles {
            join_failed |= handle.join().is_err();
        }
    });
    if join_failed || planner.result().is_none() {
        return Ok(host_failure(
            &planner,
            "native workers did not retire cleanly",
        ));
    }
    Ok(planner.result().expect("sealed planner").clone())
}

fn host_failure(planner: &ExactPlanner, detail: &str) -> SolveResult {
    // Preserve accepted witnesses even if a native host fails outside a leaf.
    SolveResult::Incomplete(IncompleteResult {
        reason: IncompleteReason::WorkerFailed {
            detail: detail.into(),
        },
        best_known: planner.best().cloned(),
        proof: planner.proof().clone(),
    })
}
