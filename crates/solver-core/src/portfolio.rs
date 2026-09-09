//! Independent exact searches share a worker budget, never a proof ledger.
use crate::encoding::Counts;
use solver_api::{
    BestKnownSolution, Diagnostic, DiagnosticValue, IncompleteReason, IncompleteResult,
    OptimalSolution, Problem, ProofSummary, RunOptions, SolveObserver, SolveResult, SolverError,
    SolverEvent, SolverProgress,
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

enum Message {
    Event(usize, SolverEvent),
    Finished(usize, Result<SolveResult, SolverError>),
}

struct Delivery<'a> {
    observer: &'a dyn SolveObserver,
    best: Option<BestKnownSolution>,
    progress: [Option<SolverProgress>; 2],
    started: Instant,
}

impl Delivery<'_> {
    fn event(&mut self, branch: usize, mut event: SolverEvent) {
        match &mut event {
            SolverEvent::Incumbent(solution) => {
                if self.best.as_ref().is_some_and(|best| {
                    (best.node_count, best.link_count) <= (solution.node_count, solution.link_count)
                }) {
                    return;
                }
                self.best = Some(solution.clone());
            }
            SolverEvent::Progress(progress) => {
                progress.elapsed_ms =
                    u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
                for diagnostic in &mut progress.custom {
                    if diagnostic.name == "solver.root"
                        && let DiagnosticValue::Text(value) = &mut diagnostic.value
                    {
                        // Root records are generated internally as JSON objects.
                        *value = format!("{{\"branch\":{branch},{}", &value[1..]);
                    }
                }
                progress.custom.push(Diagnostic::text(
                    "solver.portfolio_branch",
                    "Solver independent search",
                    branch.to_string(),
                ));
                self.progress[branch] = Some(progress.clone());
            }
            SolverEvent::SolutionFound(_) => {}
        }
        // The shared outer collector deduplicates enumeration by exact layout
        // identity. Both exhaustive searches have the same mathematical scope.
        self.observer.on_event(event);
    }

    fn result(&self, branch: usize, mut result: SolveResult) -> SolveResult {
        if let Some(mut progress) = self.progress[branch].clone() {
            progress.custom.retain(|d| d.name != "solver.root");
            progress.custom.push(Diagnostic::text(
                "solver.portfolio_proof_owner",
                "Solver returned proof owner",
                branch.to_string(),
            ));
            self.observer.on_event(SolverEvent::Progress(progress));
        }
        match &mut result {
            SolveResult::Optimal(optimal) => {
                // The first globally delivered best witness may be another
                // equal optimum. The winner's exhausted lower obligations
                // establish its objective, independently of the witness tie.
                if let Some(best) = &self.best
                    && (best.node_count, best.link_count)
                        == (optimal.node_count, optimal.link_count)
                {
                    *optimal = OptimalSolution {
                        node_count: best.node_count,
                        link_count: best.link_count,
                        physical_link_count: best.physical_link_count,
                        discard_link_count: best.discard_link_count,
                        canonical_graph_key: best.canonical_graph_key.clone(),
                        graph: best.graph.clone(),
                        validation: best.validation.clone(),
                        proof: optimal.proof.clone(),
                    };
                }
            }
            SolveResult::Incomplete(incomplete) => incomplete.best_known.clone_from(&self.best),
            SolveResult::GloballyUnsat(_) => {}
        }
        result
    }
}

#[allow(clippy::too_many_lines)]
pub(crate) fn run(
    problem: &Problem,
    options: &RunOptions,
    cancel: &AtomicBool,
    observer: &dyn SolveObserver,
) -> Result<SolveResult, SolverError> {
    if options.worker_count == 1 {
        return crate::run(problem, options, cancel, observer, Counts::Boolean);
    }
    let stops = [AtomicBool::new(false), AtomicBool::new(false)];
    let mut delivery = Delivery {
        observer,
        best: None,
        progress: [None, None],
        started: Instant::now(),
    };
    let mut completed = None;
    let mut unfinished = Vec::new();
    let (send, receive) = mpsc::channel();
    thread::scope(|scope| {
        let mut handles = Vec::new();
        for (branch, counts) in [Counts::Sparse, Counts::Boolean].into_iter().enumerate() {
            let send = send.clone();
            let stop = &stops[branch];
            let options = RunOptions {
                worker_count: if branch == 0 {
                    options.worker_count.div_ceil(2)
                } else {
                    options.worker_count / 2
                },
                ..*options
            };
            handles.push(scope.spawn(move || {
                let bridge = |event| {
                    let _ = send.send(Message::Event(branch, event));
                };
                let result = crate::run(problem, &options, stop, &bridge, counts);
                let _ = send.send(Message::Finished(branch, result));
            }));
        }
        drop(send);
        loop {
            if cancel.load(Ordering::Relaxed) || completed.is_some() {
                for stop in &stops {
                    stop.store(true, Ordering::Relaxed);
                }
            }
            match receive.recv_timeout(Duration::from_millis(25)) {
                Ok(Message::Event(branch, event)) => delivery.event(branch, event),
                Ok(Message::Finished(branch, Ok(result)))
                    if !matches!(result, SolveResult::Incomplete(_)) =>
                {
                    if completed.is_none() {
                        completed = Some((branch, result));
                    }
                }
                Ok(Message::Finished(branch, result)) => unfinished.push((branch, result)),
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
        // Each search already joins its workers and subprocess readers before
        // Finished. These joins also cover panic paths. Nothing is detached.
        for handle in handles {
            if handle.join().is_err() {
                unfinished.push((
                    0,
                    Ok(SolveResult::Incomplete(IncompleteResult {
                        reason: IncompleteReason::WorkerFailed {
                            detail: "Solver portfolio search panicked".into(),
                        },
                        best_known: None,
                        proof: ProofSummary::default(),
                    })),
                ));
            }
        }
    });
    let (branch, result) = if let Some(completed) = completed {
        completed
    } else {
        let (branch, result) = unfinished
            .into_iter()
            .next()
            .expect("joined searches report a result");
        (branch, result?)
    };
    // Explicit cancellation remains incomplete even if a proof raced the cancel.
    // solve_problem applies this after public identity normalization as well.
    let result = delivery.result(branch, result);
    if cancel.load(Ordering::Relaxed) {
        let proof = match &result {
            SolveResult::Optimal(r) => r.proof.clone(),
            SolveResult::Incomplete(r) => r.proof.clone(),
            SolveResult::GloballyUnsat(r) => r.proof.clone(),
        };
        return Ok(SolveResult::Incomplete(IncompleteResult {
            reason: IncompleteReason::Cancelled,
            best_known: delivery.best,
            proof,
        }));
    }
    Ok(result)
}
