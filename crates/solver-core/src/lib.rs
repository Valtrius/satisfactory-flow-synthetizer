//! Astra: exact compact topology synthesis with a local incremental cvc5 backend.
pub mod lower_bound;
pub mod problem;
pub mod profile;
pub use problem::{NormalizedProblem, Preparation, prepare_problem};
mod cardinality;
mod diagnostics;
mod encoding;
use diagnostics::RootStats;
mod portfolio;
mod process;

use crate::{
    lower_bound::{baseline_lower_bounds, profile_impossibility},
    profile::{AccountedProfile, enumerate_accounted_profile_groups},
};
use encoding::{Counts, Encoding};
use process::{Failure, Session};
use solver_api::{
    BestKnownSolution, CanonicalGraphKey, ConsumerPortRef, Diagnostic, IncompleteReason,
    IncompleteResult, LinkConstraint, OptimalSolution, PhysicalGraph, Problem, ProducerPortRef,
    ProofSummary, RunOptions, SolveMode, SolveObserver, SolveOutcome, SolvePhase, SolveResult,
    SolverError, SolverEvent, SolverProgress,
};
use solver_validation::{
    SolutionCollector, ValidationError, layout_key, solve_topology, validate_solution,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

/// Path selected for the offline backend, also exposed for benchmark provenance.
#[must_use]
pub fn cvc5_executable() -> std::path::PathBuf {
    process::executable()
}

/// Solve any of the three shared scopes. Incumbents and enumeration survive interruption.
/// # Errors
/// Returns malformed problems/options. Backend failures produce an incomplete outcome.
pub fn solve_problem(
    problem: &Problem,
    options: &RunOptions,
    cancel: &AtomicBool,
    observer: &dyn SolveObserver,
) -> Result<SolveOutcome, SolverError> {
    problem.validate()?;
    if options.worker_count == 0 {
        return Err(SolverError::InvalidOptions(
            "worker count must be positive".into(),
        ));
    }
    let collector = SolutionCollector::new(problem, observer);
    let result = portfolio::run(problem, options, cancel, &collector)?;
    let outcome = collector.finish(result, options.mode);
    // Public identity normalization is part of the run, too. A cancellation during
    // that final work must not escape as a completed result.
    if cancel.load(Ordering::Relaxed) {
        let (best_known, proof) = match outcome.result {
            SolveResult::Optimal(s) => (
                Some(BestKnownSolution {
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
            SolveResult::Incomplete(_) => return Ok(outcome),
        };
        return Ok(SolveOutcome::new(
            SolveResult::Incomplete(IncompleteResult {
                reason: IncompleteReason::Cancelled,
                best_known,
                proof,
            }),
            options.mode,
            outcome.solutions,
        ));
    }
    Ok(outcome)
}

/// Return the first validated witness at the proved minimum node and link counts.
/// # Errors
/// Returns invalid problems or options.
pub fn solve_with_observer(
    problem: &Problem,
    options: &RunOptions,
    cancel: &AtomicBool,
    observer: &dyn SolveObserver,
) -> Result<SolveOutcome, SolverError> {
    solve_problem(
        problem,
        &RunOptions {
            mode: SolveMode::OneMinNL,
            ..*options
        },
        cancel,
        observer,
    )
}
/// Enumerate every layout at minimum N and minimum L.
/// # Errors
/// Returns invalid problems or options.
pub fn enumerate_minimum_links_with_observer(
    problem: &Problem,
    options: &RunOptions,
    cancel: &AtomicBool,
    observer: &dyn SolveObserver,
) -> Result<SolveOutcome, SolverError> {
    solve_problem(
        problem,
        &RunOptions {
            mode: SolveMode::AllMinNL,
            ..*options
        },
        cancel,
        observer,
    )
}
/// Enumerate every feasible L group at minimum N.
/// # Errors
/// Returns invalid problems or options.
pub fn enumerate_with_observer(
    problem: &Problem,
    options: &RunOptions,
    cancel: &AtomicBool,
    observer: &dyn SolveObserver,
) -> Result<SolveOutcome, SolverError> {
    solve_problem(
        problem,
        &RunOptions {
            mode: SolveMode::AllMinN,
            ..*options
        },
        cancel,
        observer,
    )
}

enum Message {
    Valid(BestKnownSolution),
    Done(usize, Result<Completion, Failure>, Option<String>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Completion {
    Exhausted,
    Optimum,
}

#[derive(Clone, Copy)]
struct Root {
    profile: usize,
    source: Option<usize>,
    impossible: bool,
}

struct RootLedger {
    owners: Vec<usize>,
    completed: Vec<bool>,
    remaining: Vec<usize>,
}

impl RootLedger {
    fn new(roots: &[Root], profiles: usize) -> Self {
        let mut remaining = vec![0; profiles];
        for root in roots {
            remaining[root.profile] += 1;
        }
        Self {
            owners: roots.iter().map(|root| root.profile).collect(),
            completed: vec![false; roots.len()],
            remaining,
        }
    }

    fn finish(&mut self, root: usize, proof: &mut ProofSummary) -> Result<(), Failure> {
        let completed = self
            .completed
            .get_mut(root)
            .ok_or_else(|| Failure::Worker("unknown Astra root completion".into()))?;
        if *completed {
            return Err(Failure::Worker("duplicate Astra root completion".into()));
        }
        *completed = true;
        proof.root_partitions_exhausted += 1;
        let remaining = &mut self.remaining[self.owners[root]];
        *remaining -= 1;
        if *remaining == 0 {
            proof.profiles_exhausted += 1;
        }
        Ok(())
    }

    fn complete(&self) -> bool {
        self.remaining.iter().all(|&count| count == 0)
    }
}

struct Search<'a> {
    counts: Counts,
    original: &'a Problem,
    normalized: NormalizedProblem,
    problem: Problem,
    options: &'a RunOptions,
    cancel: &'a AtomicBool,
    observer: &'a dyn SolveObserver,
    started: Instant,
    proof: ProofSummary,
    best: Option<BestKnownSolution>,
    layouts: BTreeMap<CanonicalGraphKey, BestKnownSolution>,
    minimum_links_ms: Option<u64>,
}

#[allow(clippy::too_many_lines)]
fn run(
    problem: &Problem,
    options: &RunOptions,
    cancel: &AtomicBool,
    observer: &dyn SolveObserver,
    counts: Counts,
) -> Result<SolveResult, SolverError> {
    let started = Instant::now();
    let preparation =
        prepare_problem(problem).map_err(|e| SolverError::InvalidProblem(e.to_string()))?;
    if cancel.load(Ordering::Relaxed) {
        return Ok(SolveResult::Incomplete(IncompleteResult {
            reason: IncompleteReason::Cancelled,
            best_known: None,
            proof: ProofSummary::default(),
        }));
    }
    let normalized = match preparation {
        Preparation::GloballyUnsat(proof) => return Ok(SolveResult::GloballyUnsat(proof)),
        Preparation::Prepared(value) => value,
    };
    let mut search = Search {
        counts,
        original: problem,
        problem: Problem {
            inputs: normalized.inputs.as_slice().to_vec(),
            outputs: normalized.outputs.as_slice().to_vec(),
            max_link_rate: normalized.max_link_rate.clone(),
        },
        normalized,
        options,
        cancel,
        observer,
        started,
        proof: ProofSummary {
            proof_version: 1,
            ..ProofSummary::default()
        },
        best: None,
        layouts: BTreeMap::new(),
        minimum_links_ms: None,
    };
    search.progress(None, None, SolvePhase::ComputingLowerBound);
    let lower = match baseline_lower_bounds(&search.normalized) {
        Ok(value) => value.combined_nodes,
        Err(error) => {
            return Ok(search.incomplete(IncompleteReason::ResourceLimit {
                detail: error.to_string(),
            }));
        }
    };
    search.proof.initial_node_lower_bound = lower;
    search.progress(Some(lower), None, SolvePhase::ComputingLowerBound);
    for nodes in lower..=u32::MAX {
        if cancel.load(Ordering::Relaxed) {
            return Ok(search.incomplete(IncompleteReason::Cancelled));
        }
        if options.max_nodes.is_some_and(|cap| nodes > cap) {
            return Ok(search.incomplete(IncompleteReason::ResourceLimit {
                detail: "maximum node count exhausted".into(),
            }));
        }
        let groups = match enumerate_accounted_profile_groups(
            nodes,
            u32::try_from(problem.inputs.len()).unwrap(),
            u32::try_from(problem.outputs.len()).unwrap(),
            &search.normalized.surplus,
            &search.normalized.max_link_rate,
        ) {
            Ok(groups) => groups,
            Err(error) => {
                return Ok(search.incomplete(IncompleteReason::ResourceLimit {
                    detail: error.to_string(),
                }));
            }
        };
        for group in groups {
            search.progress(Some(nodes), Some(group.link_count), SolvePhase::Searching);
            match search.group(nodes, group.link_count, &group.profiles) {
                Ok(Completion::Optimum) => return Ok(search.complete()),
                Ok(Completion::Exhausted) => {}
                Err(failure) => {
                    let reason = match failure {
                        Failure::Cancelled => IncompleteReason::Cancelled,
                        Failure::Worker(detail) => IncompleteReason::WorkerFailed { detail },
                    };
                    return Ok(search.incomplete(reason));
                }
            }
            if cancel.load(Ordering::Relaxed) {
                return Ok(search.incomplete(IncompleteReason::Cancelled));
            }
            search.proof.link_groups_exhausted += 1;
            if search.best.is_some() {
                if search.minimum_links_ms.is_none() {
                    search.minimum_links_ms = Some(search.elapsed());
                }
                search.progress(Some(nodes), Some(group.link_count), SolvePhase::Enumerating);
                if options.mode != SolveMode::AllMinN {
                    return Ok(search.complete());
                }
            }
        }
        if search.best.is_some() {
            return Ok(search.complete());
        }
        search.proof.node_counts_exhausted_through = Some(nodes);
    }
    Ok(search.incomplete(IncompleteReason::ResourceLimit {
        detail: "public node index exhausted".into(),
    }))
}

impl Search<'_> {
    fn elapsed(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    fn progress(&self, nodes: Option<u32>, links: Option<u32>, phase: SolvePhase) {
        let mut custom = vec![Diagnostic::counter(
            "astra.profiles_exhausted",
            "Completed Astra profiles",
            self.proof.profiles_exhausted,
        )];
        if let Some(ms) = self.minimum_links_ms {
            custom.push(Diagnostic::counter(
                "astra.minimum_links_complete_ms",
                "Minimum-link enumeration completed",
                ms,
            ));
        }
        self.observer
            .on_event(SolverEvent::Progress(SolverProgress {
                phase,
                elapsed_ms: self.elapsed(),
                node_count: (phase != SolvePhase::ComputingLowerBound)
                    .then_some(nodes)
                    .flatten(),
                link_constraint: links.map(LinkConstraint::Exact),
                node_lower_bound: nodes,
                best_node_count: self.best.as_ref().map(|b| b.node_count),
                best_link_count: self.best.as_ref().map(|b| b.link_count),
                solutions_found: self.layouts.len() as u64,
                custom,
            }));
    }

    #[allow(clippy::too_many_lines)]
    fn group(
        &mut self,
        nodes: u32,
        links: u32,
        tasks: &[AccountedProfile],
    ) -> Result<Completion, Failure> {
        let completed_before = self.proof.profiles_exhausted;
        let mut roots = Vec::new();
        for (profile, task) in tasks.iter().enumerate() {
            let impossible =
                profile_impossibility(&self.normalized, task.profile, task.accounting).is_some();
            // Every output has exactly one producer. Keep all source owners,
            // including impossible choices, for cvc5 to discharge explicitly.
            if !impossible && self.options.worker_count > 1 && !self.problem.outputs.is_empty() {
                for source in 0..self.problem.inputs.len() + task.profile.node_count() as usize {
                    roots.push(Root {
                        profile,
                        source: Some(source),
                        impossible: false,
                    });
                }
            } else {
                roots.push(Root {
                    profile,
                    source: None,
                    impossible,
                });
            }
        }
        let mut ledger = RootLedger::new(&roots, tasks.len());
        let roots = &roots;
        let next = AtomicUsize::new(0);
        let stop = AtomicBool::new(false);
        let (send, receive) = mpsc::channel();
        let normalized = &self.normalized;
        let problem = &self.problem;
        let original = self.original;
        let cancel = self.cancel;
        let mode = self.options.mode;
        let counts = self.counts;
        let diagnostics = std::env::var_os("ASTRA_DIAGNOSTICS").is_some_and(|v| v == "1");
        let origin = self.started;
        let workers = self.options.worker_count.min(roots.len());
        let mut messages = Vec::new();
        let mut optimum = false;
        // Borrow only immutable inputs into workers, allowing coordinator-owned event delivery.
        thread::scope(|scope| {
            let mut handles = Vec::new();
            for _ in 0..workers {
                let send = send.clone();
                let next = &next;
                let stop = &stop;
                handles.push(scope.spawn(move || {
                    loop {
                        if stop.load(Ordering::Relaxed) || cancel.load(Ordering::Relaxed) {
                            break;
                        }
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(&root) = roots.get(index) else {
                            break;
                        };
                        let mut stats = diagnostics.then(RootStats::default);
                        let result = if root.impossible {
                            Ok(Completion::Exhausted)
                        } else {
                            profile(
                                problem,
                                original,
                                normalized,
                                tasks[root.profile],
                                root.source,
                                mode,
                                counts,
                                cancel,
                                stop,
                                &send,
                                &mut stats,
                            )
                        };
                        let stopped = result.is_err() || matches!(result, Ok(Completion::Optimum));
                        let trace = stats.map(|stats| {
                            let verdict = match &result {
                                Ok(Completion::Exhausted) => "exhausted",
                                Ok(Completion::Optimum) => "optimum",
                                Err(Failure::Cancelled) => "cancelled",
                                Err(Failure::Worker(_)) => "failed",
                            };
                            stats.record(index, nodes, links, &root, verdict, origin)
                        });
                        let _ = send.send(Message::Done(index, result, trace));
                        if stopped {
                            stop.store(true, Ordering::Relaxed);
                            break;
                        }
                    }
                }));
            }
            drop(send);
            // Events must stream while scoped workers hold immutable problem references.
            // Collecting the references above prevents borrowing all of self here; coordinator
            // state is handled in a separate mutable view below.
            let mut state = GroupState {
                best: &mut self.best,
                layouts: &mut self.layouts,
                proof: &mut self.proof,
                observer: self.observer,
                mode: self.options.mode,
                ledger: &mut ledger,
                optimum: &mut optimum,
            };
            let mut last = Instant::now();
            loop {
                match receive.recv_timeout(Duration::from_millis(50)) {
                    Ok(message) => {
                        if let Message::Done(_, _, Some(trace)) = &message {
                            state.progress(nodes, links, self.started, Some(trace.clone()));
                        }
                        if let Err(error) = state.accept(message) {
                            messages.push(error);
                            stop.store(true, Ordering::Relaxed);
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
                if last.elapsed() >= Duration::from_millis(250) {
                    state.progress(nodes, links, self.started, None);
                    last = Instant::now();
                }
            }
            for handle in handles {
                if handle.join().is_err() {
                    messages.push(Failure::Worker("Astra worker panicked".into()));
                }
            }
        });
        if let Some(index) = messages
            .iter()
            .position(|e| matches!(e, Failure::Worker(_)))
        {
            return Err(messages.swap_remove(index));
        }
        if self.cancel.load(Ordering::Relaxed) {
            return Err(Failure::Cancelled);
        }
        // All lower N/L obligations finished before this group started. A
        // validated current-group witness establishes the optimum; equal-L
        // exhaustion is required only for enumeration. Internal sibling stops
        // do not become user cancellation or fabricated completion records.
        if optimum && self.best.is_some() {
            return Ok(Completion::Optimum);
        }
        if !messages.is_empty() {
            return Err(Failure::Cancelled);
        }
        if !ledger.complete()
            || self.proof.profiles_exhausted - completed_before != tasks.len() as u64
        {
            return Err(Failure::Worker("unfinished Astra profile group".into()));
        }
        Ok(Completion::Exhausted)
    }

    fn incomplete(&self, reason: IncompleteReason) -> SolveResult {
        SolveResult::Incomplete(IncompleteResult {
            reason,
            best_known: self.best.clone(),
            proof: self.proof.clone(),
        })
    }
    fn complete(&self) -> SolveResult {
        if self.cancel.load(Ordering::Relaxed) {
            return self.incomplete(IncompleteReason::Cancelled);
        }
        let best = self
            .best
            .as_ref()
            .expect("completed SAT result has a validated witness");
        SolveResult::Optimal(OptimalSolution {
            node_count: best.node_count,
            link_count: best.link_count,
            physical_link_count: best.physical_link_count,
            discard_link_count: best.discard_link_count,
            canonical_graph_key: best.canonical_graph_key.clone(),
            graph: best.graph.clone(),
            validation: best.validation.clone(),
            proof: self.proof.clone(),
        })
    }
}

struct GroupState<'a> {
    best: &'a mut Option<BestKnownSolution>,
    layouts: &'a mut BTreeMap<CanonicalGraphKey, BestKnownSolution>,
    proof: &'a mut ProofSummary,
    observer: &'a dyn SolveObserver,
    mode: SolveMode,
    ledger: &'a mut RootLedger,
    optimum: &'a mut bool,
}
impl GroupState<'_> {
    fn progress(&self, nodes: u32, links: u32, started: Instant, trace: Option<String>) {
        let mut custom = vec![Diagnostic::counter(
            "astra.profiles_exhausted",
            "Completed Astra profiles",
            self.proof.profiles_exhausted,
        )];
        if let Some(trace) = trace {
            custom.push(Diagnostic::text("astra.root", "Astra root evidence", trace));
        }
        self.observer
            .on_event(SolverEvent::Progress(SolverProgress {
                phase: SolvePhase::Searching,
                elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
                node_count: Some(nodes),
                link_constraint: Some(LinkConstraint::Exact(links)),
                node_lower_bound: Some(nodes),
                best_node_count: self.best.as_ref().map(|b| b.node_count),
                best_link_count: self.best.as_ref().map(|b| b.link_count),
                solutions_found: self.layouts.len() as u64,
                custom,
            }));
    }

    fn accept(&mut self, message: Message) -> Result<(), Failure> {
        match message {
            Message::Valid(solution) => {
                if self.best.as_ref().is_none_or(|b| {
                    (solution.node_count, solution.link_count) < (b.node_count, b.link_count)
                }) {
                    *self.best = Some(solution.clone());
                    self.observer
                        .on_event(SolverEvent::Incumbent(solution.clone()));
                }
                if self.mode != SolveMode::OneMinNL
                    && !self.layouts.contains_key(&solution.canonical_graph_key)
                {
                    self.layouts
                        .insert(solution.canonical_graph_key.clone(), solution.clone());
                    self.observer.on_event(SolverEvent::SolutionFound(solution));
                }
            }
            Message::Done(root, result, _) => match result? {
                Completion::Exhausted => self.ledger.finish(root, self.proof)?,
                Completion::Optimum if self.mode == SolveMode::OneMinNL => *self.optimum = true,
                Completion::Optimum => {
                    return Err(Failure::Worker("early stop in enumeration".into()));
                }
            },
        }
        Ok(())
    }
}

fn restore(
    original: &Problem,
    normalized: &NormalizedProblem,
    mut graph: PhysicalGraph,
    key: CanonicalGraphKey,
) -> Result<BestKnownSolution, Failure> {
    for link in &mut graph.links {
        if let ProducerPortRef::Input(ref mut id) = link.producer {
            id.0 = u32::try_from(
                normalized
                    .terminal_mapping
                    .original_input(id.0 as usize)
                    .unwrap(),
            )
            .unwrap();
        }
        if let ConsumerPortRef::Output(ref mut id) = link.consumer {
            id.0 = u32::try_from(
                normalized
                    .terminal_mapping
                    .original_output(id.0 as usize)
                    .unwrap(),
            )
            .unwrap();
        }
        link.flow = &link.flow * &normalized.original_scale;
    }
    let validation = validate_solution(original, &graph)
        .map_err(|e| Failure::Worker(format!("Astra witness restoration failed: {e}")))?;
    Ok(BestKnownSolution {
        node_count: validation.node_count,
        link_count: validation.link_count,
        physical_link_count: validation.physical_link_count,
        discard_link_count: validation.discard_link_count,
        canonical_graph_key: key,
        graph,
        validation,
    })
}

#[allow(clippy::too_many_arguments)]
fn profile(
    problem: &Problem,
    original: &Problem,
    normalized: &NormalizedProblem,
    task: AccountedProfile,
    source: Option<usize>,
    mode: SolveMode,
    counts: Counts,
    cancel: &AtomicBool,
    stop: &AtomicBool,
    send: &mpsc::Sender<Message>,
    stats: &mut Option<RootStats>,
) -> Result<Completion, Failure> {
    let encoding = Encoding::new(problem, task, counts);
    let mut session = Session::new()?;
    session.write(&encoding.script)?;
    if let Some(source) = source {
        session.write(&encoding.output_source_assertion(source)?)?;
    }
    let mut seen = BTreeSet::new();
    loop {
        if cancel.load(Ordering::Relaxed) || stop.load(Ordering::Relaxed) {
            return Err(Failure::Cancelled);
        }
        session.write("(check-sat)\n")?;
        let check = stats.as_ref().map(|_| Instant::now());
        let response = session.response(cancel, stop);
        if let (Some(stats), Some(start)) = (stats.as_mut(), check) {
            stats.check += start.elapsed();
        }
        match response?.as_str() {
            "unsat" => return Ok(Completion::Exhausted),
            "sat" => {}
            response => {
                return Err(Failure::Worker(format!(
                    "cvc5 did not finish its proof: {response}"
                )));
            }
        }
        session.write(&encoding.query())?;
        let (graph, block) = encoding.model(&session.response(cancel, stop)?)?;
        session.write(&block)?;
        let validation = stats.as_ref().map(|_| Instant::now());
        let solved = solve_topology(problem, &graph);
        if let (Some(stats), Some(start)) = (stats.as_mut(), validation) {
            stats.models += 1;
            stats.validation += start.elapsed();
        }
        let solved = match solved {
            Ok(graph) => graph,
            Err(
                ValidationError::NonUniqueSteadyState { .. }
                | ValidationError::NonUniqueCyclicScc { .. }
                | ValidationError::InconsistentCyclicScc { .. },
            ) => continue,
            Err(error) => {
                return Err(Failure::Worker(format!(
                    "Astra model failed independent reconstruction: {error}"
                )));
            }
        };
        let start = stats.as_ref().map(|_| Instant::now());
        let validation = validate_solution(problem, &solved);
        if let (Some(stats), Some(start)) = (stats.as_mut(), start) {
            stats.validation += start.elapsed();
        }
        let validation = validation.map_err(|e| Failure::Worker(e.to_string()))?;
        if validation.node_count != task.profile.node_count()
            || validation.link_count != task.accounting.link_count
        {
            return Err(Failure::Worker("Astra model objective mismatch".into()));
        }
        let start = stats.as_ref().map(|_| Instant::now());
        let identity = layout_key(problem, &solved);
        if let (Some(stats), Some(start)) = (stats.as_mut(), start) {
            stats.identity += start.elapsed();
        }
        if !seen.insert(identity.clone()) {
            if let Some(stats) = stats {
                stats.duplicates += 1;
            }
            continue;
        }
        // Exact layout identity preserves deduplication without imposing a
        // lexicographic preferred representative on independently valid graphs.
        let start = stats.as_ref().map(|_| Instant::now());
        let raw = restore(original, normalized, solved, identity);
        if let (Some(stats), Some(start)) = (stats.as_mut(), start) {
            stats.validation += start.elapsed();
            stats.valid += u64::from(raw.is_ok());
        }
        let raw = raw?;
        send.send(Message::Valid(raw))
            .map_err(|e| Failure::Worker(e.to_string()))?;
        if mode == SolveMode::OneMinNL {
            return Ok(Completion::Optimum);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_roots_and_duplicate_completions_cannot_discharge_a_profile() {
        let roots = [
            Root {
                profile: 0,
                source: Some(0),
                impossible: false,
            },
            Root {
                profile: 0,
                source: Some(1),
                impossible: false,
            },
            Root {
                profile: 1,
                source: None,
                impossible: true,
            },
        ];
        let mut ledger = RootLedger::new(&roots, 2);
        let mut proof = ProofSummary::default();
        ledger.finish(0, &mut proof).unwrap();
        assert_eq!(proof.profiles_exhausted, 0);
        assert!(!ledger.complete());
        assert!(ledger.finish(0, &mut proof).is_err());
        assert!(ledger.finish(3, &mut proof).is_err());
        assert_eq!(proof.root_partitions_exhausted, 1);
        ledger.finish(2, &mut proof).unwrap();
        assert_eq!(proof.profiles_exhausted, 1);
        assert!(!ledger.complete());
        ledger.finish(1, &mut proof).unwrap();
        assert_eq!(proof.profiles_exhausted, 2);
        assert_eq!(proof.root_partitions_exhausted, 3);
        assert!(ledger.complete());
    }
}
