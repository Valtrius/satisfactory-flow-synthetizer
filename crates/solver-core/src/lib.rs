//! Exact compact topology synthesis with a local incremental cvc5 backend.
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
    second_source: Option<usize>,
    impossible: bool,
}

/// Children partition the parent by the second output's unique producer.
/// Only children enter the ledger; all must finish before the profile is exhausted.
fn refine_roots(
    roots: Vec<Root>,
    tasks: &[AccountedProfile],
    inputs: usize,
    outputs: usize,
    workers: usize,
) -> Vec<Root> {
    let live = roots.iter().filter(|root| !root.impossible).count();
    if workers <= 1 || outputs < 2 || live >= workers {
        return roots;
    }
    let mut refined: Vec<_> = roots
        .into_iter()
        .flat_map(|root| {
            let sources = if root.impossible || root.source.is_none() {
                0
            } else {
                inputs + tasks[root.profile].profile.node_count() as usize
            };
            if sources == 0 {
                vec![root]
            } else {
                (0..sources)
                    .map(|source| Root {
                        second_source: Some(source),
                        ..root
                    })
                    .collect()
            }
        })
        .collect();
    refined.sort_by_key(|root| root.second_source);
    refined
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AdaptiveChoice {
    Parent,
    Children,
}

struct AdaptiveLedger {
    base: RootLedger,
    choice: Vec<Option<AdaptiveChoice>>,
    children: Vec<Option<Vec<bool>>>,
}

impl AdaptiveLedger {
    fn new(roots: &[Root], profiles: usize) -> Self {
        Self {
            base: RootLedger::new(roots, profiles),
            choice: vec![None; roots.len()],
            children: vec![None; roots.len()],
        }
    }

    fn finish(
        &mut self,
        parent: usize,
        child: Option<usize>,
        proof: &mut ProofSummary,
    ) -> Result<(), Failure> {
        let choice = self
            .choice
            .get_mut(parent)
            .ok_or_else(|| Failure::Worker("unknown adaptive parent".into()))?;
        // A competing parent or child may finish after the other proof won.
        if choice.is_some() {
            return Ok(());
        }
        let (selected, units) = if let Some(child) = child {
            let children = self.children[parent]
                .as_mut()
                .ok_or_else(|| Failure::Worker("unregistered adaptive children".into()))?;
            let done = children
                .get_mut(child)
                .ok_or_else(|| Failure::Worker("unknown adaptive child".into()))?;
            if *done {
                return Err(Failure::Worker("duplicate adaptive child".into()));
            }
            *done = true;
            if !children.iter().all(|done| *done) {
                return Ok(());
            }
            (AdaptiveChoice::Children, children.len())
        } else {
            (AdaptiveChoice::Parent, 1)
        };
        self.base.finish(parent, proof)?;
        proof.root_partitions_exhausted += u64::try_from(units - 1).expect("root count fits u64");
        *choice = Some(selected);
        Ok(())
    }

    fn committed(&self, parent: usize, child: Option<usize>) -> bool {
        self.choice[parent]
            == Some(if child.is_some() {
                AdaptiveChoice::Children
            } else {
                AdaptiveChoice::Parent
            })
    }
}

#[derive(Clone, Copy)]
struct AdaptiveJob {
    root: Root,
    parent: usize,
    trigger_s: Option<f64>,
}

impl Search<'_> {
    /// Parents retain their running backend. Only a validated current-group witness
    /// enables children, which occupy spare worker slots. Either the original parent
    /// or every disjoint child discharges its obligation, never both.
    #[allow(clippy::too_many_lines)]
    fn adaptive_group(
        &mut self,
        nodes: u32,
        links: u32,
        tasks: &[AccountedProfile],
        parents: &[Root],
    ) -> Result<Completion, Failure> {
        use std::fmt::Write as _;
        let completed_before = self.proof.profiles_exhausted;
        let mut ledger = AdaptiveLedger::new(parents, tasks.len());
        let mut jobs: Vec<_> = parents
            .iter()
            .enumerate()
            .map(|(parent, &root)| AdaptiveJob {
                root,
                parent,
                trigger_s: None,
            })
            .collect();
        let mut queue: std::collections::VecDeque<_> = (0..jobs.len()).collect();
        let mut finished = BTreeSet::new();
        let mut traces = Vec::new();
        let mut errors = Vec::new();
        let mut active = 0;
        let mut gate = None;
        let stop = AtomicBool::new(false);
        let (send, receive) = mpsc::channel();
        let normalized = &self.normalized;
        let problem = &self.problem;
        let original = self.original;
        let cancel = self.cancel;
        let mode = self.options.mode;
        let counts = self.counts;
        let workers = self.options.worker_count;
        let origin = self.started;
        let diagnostics = std::env::var_os("SOLVER_DIAGNOSTICS").is_some_and(|v| v == "1");
        let mut optimum = false;
        thread::scope(|scope| {
            let mut handles = Vec::new();
            let mut last = Instant::now();
            loop {
                if cancel.load(Ordering::Relaxed) || ledger.base.complete() || !errors.is_empty() {
                    stop.store(true, Ordering::Relaxed);
                }
                while !stop.load(Ordering::Relaxed) && active < workers {
                    let Some(index) = queue.pop_front() else {
                        break;
                    };
                    let job = jobs[index];
                    if ledger.choice[job.parent].is_some() {
                        continue;
                    }
                    let stop = &stop;
                    let send = send.clone();
                    active += 1;
                    handles.push(scope.spawn(move || {
                        let mut stats = diagnostics.then(RootStats::default);
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            if job.root.impossible {
                                Ok(Completion::Exhausted)
                            } else {
                                profile(
                                    problem,
                                    original,
                                    normalized,
                                    tasks[job.root.profile],
                                    job.root.source,
                                    job.root.second_source,
                                    mode,
                                    counts,
                                    cancel,
                                    stop,
                                    &send,
                                    &mut stats,
                                )
                            }
                        }))
                        .unwrap_or_else(|_| {
                            Err(Failure::Worker("adaptive worker panicked".into()))
                        });
                        let trace = stats.map(|stats| {
                            let verdict = match &result {
                                Ok(Completion::Exhausted) => "exhausted",
                                Ok(Completion::Optimum) => "optimum",
                                Err(Failure::Cancelled) => "cancelled",
                                Err(Failure::Worker(_)) => "failed",
                            };
                            stats.record(index, nodes, links, &job.root, verdict, origin)
                        });
                        let _ = send.send(Message::Done(index, result, trace));
                    }));
                }
                if active == 0 {
                    break;
                }
                match receive.recv_timeout(Duration::from_millis(25)) {
                    Ok(message @ Message::Valid(_)) => {
                        let mut state = GroupState {
                            best: &mut self.best,
                            layouts: &mut self.layouts,
                            proof: &mut self.proof,
                            observer: self.observer,
                            mode,
                            ledger: &mut ledger.base,
                            optimum: &mut optimum,
                        };
                        if let Err(error) = state.accept(message) {
                            errors.push(error);
                        }
                        // All lower N/L obligations were exhausted before this group.
                        if self
                            .best
                            .as_ref()
                            .is_some_and(|b| b.node_count == nodes && b.link_count == links)
                        {
                            gate.get_or_insert_with(|| origin.elapsed().as_secs_f64());
                        }
                    }
                    Ok(Message::Done(index, result, trace)) => {
                        if !finished.insert(index) {
                            errors
                                .push(Failure::Worker("duplicate adaptive job completion".into()));
                            continue;
                        }
                        active -= 1;
                        let job = jobs[index];
                        if let Some(trace) = trace {
                            traces.push((index, trace));
                        }
                        match result {
                            Ok(Completion::Exhausted) => {
                                if let Err(error) = ledger.finish(
                                    job.parent,
                                    job.root.second_source,
                                    &mut self.proof,
                                ) {
                                    errors.push(error);
                                }
                            }
                            Ok(Completion::Optimum) => errors
                                .push(Failure::Worker("early stop in adaptive enumeration".into())),
                            Err(Failure::Cancelled)
                                if stop.load(Ordering::Relaxed)
                                    || cancel.load(Ordering::Relaxed) => {}
                            Err(error) => errors.push(error),
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        errors.push(Failure::Worker("adaptive channel disconnected".into()));
                        break;
                    }
                }
                // Never queue children ahead of unstarted parents. A spare slot is
                // required, and a completed parent immediately makes its queued children unnecessary.
                if let Some(trigger_s) = gate.filter(|_| active < workers && queue.is_empty()) {
                    for (parent, &root) in parents.iter().enumerate() {
                        if root.impossible
                            || root.source.is_none()
                            || ledger.choice[parent].is_some()
                            || ledger.children[parent].is_some()
                        {
                            continue;
                        }
                        let children = problem.inputs.len()
                            + tasks[root.profile].profile.node_count() as usize;
                        ledger.children[parent] = Some(vec![false; children]);
                        for source in 0..children {
                            jobs.push(AdaptiveJob {
                                root: Root {
                                    second_source: Some(source),
                                    ..root
                                },
                                parent,
                                trigger_s: Some(trigger_s),
                            });
                            queue.push_back(jobs.len() - 1);
                        }
                    }
                    // Give each parent one child per round rather than filling all
                    // spare slots with the children of the first parent.
                    queue
                        .make_contiguous()
                        .sort_by_key(|&index| jobs[index].root.second_source);
                }
                if last.elapsed() >= Duration::from_millis(250) {
                    let state = GroupState {
                        best: &mut self.best,
                        layouts: &mut self.layouts,
                        proof: &mut self.proof,
                        observer: self.observer,
                        mode,
                        ledger: &mut ledger.base,
                        optimum: &mut optimum,
                    };
                    state.progress(nodes, links, origin, None);
                    last = Instant::now();
                }
            }
            for handle in handles {
                if handle.join().is_err() {
                    errors.push(Failure::Worker("adaptive worker join failed".into()));
                }
            }
        });
        // Evidence is published after choosing the disjoint cover. Finished searches
        // outside that cover stay visible with proof_committed=false.
        for (index, mut trace) in traces {
            let job = jobs[index];
            let committed = ledger.committed(job.parent, job.root.second_source);
            let children = ledger.children[job.parent].as_ref().map_or(0, Vec::len);
            let trigger = job
                .trigger_s
                .map_or_else(|| "null".into(), |s| s.to_string());
            trace.pop();
            write!(trace, ",\"parent_root\":{},\"child_count\":{children},\"proof_committed\":{committed},\"refinement_trigger_s\":{trigger},\"refinement_grace_ms\":0}}", job.parent).expect("writing to a String cannot fail");
            let state = GroupState {
                best: &mut self.best,
                layouts: &mut self.layouts,
                proof: &mut self.proof,
                observer: self.observer,
                mode,
                ledger: &mut ledger.base,
                optimum: &mut optimum,
            };
            state.progress(nodes, links, origin, Some(trace));
        }
        if let Some(index) = errors.iter().position(|e| matches!(e, Failure::Worker(_))) {
            return Err(errors.swap_remove(index));
        }
        if cancel.load(Ordering::Relaxed) || !errors.is_empty() {
            return Err(Failure::Cancelled);
        }
        if !ledger.base.complete()
            || self.proof.profiles_exhausted - completed_before != tasks.len() as u64
        {
            return Err(Failure::Worker("unfinished adaptive profile group".into()));
        }
        Ok(Completion::Exhausted)
    }
}

#[cfg(test)]
mod adaptive_tests {
    use super::*;
    fn ledger() -> AdaptiveLedger {
        let roots = vec![Root {
            profile: 0,
            source: Some(0),
            second_source: None,
            impossible: false,
        }];
        let mut ledger = AdaptiveLedger::new(&roots, 1);
        ledger.children[0] = Some(vec![false; 3]);
        ledger
    }
    #[test]
    fn partial_children_never_exhaust_and_duplicate_children_fail() {
        let mut ledger = ledger();
        let mut proof = ProofSummary::default();
        ledger.finish(0, Some(0), &mut proof).unwrap();
        ledger.finish(0, Some(1), &mut proof).unwrap();
        assert!(!ledger.base.complete());
        assert_eq!(proof.root_partitions_exhausted, 0);
        assert!(ledger.finish(0, Some(1), &mut proof).is_err());
        assert!(ledger.finish(0, Some(9), &mut proof).is_err());
    }
    #[test]
    fn parent_and_complete_children_are_alternative_proofs() {
        for parent_first in [true, false] {
            let mut ledger = ledger();
            let mut proof = ProofSummary::default();
            ledger.finish(0, Some(0), &mut proof).unwrap();
            if parent_first {
                ledger.finish(0, None, &mut proof).unwrap();
            }
            ledger.finish(0, Some(1), &mut proof).unwrap();
            ledger.finish(0, Some(2), &mut proof).unwrap();
            ledger.finish(0, None, &mut proof).unwrap();
            assert!(ledger.base.complete());
            assert_eq!(proof.profiles_exhausted, 1);
            assert_eq!(
                proof.root_partitions_exhausted,
                if parent_first { 1 } else { 3 }
            );
            assert_eq!(ledger.committed(0, None), parent_first);
            assert_eq!(ledger.committed(0, Some(0)), !parent_first);
        }
    }
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
            .ok_or_else(|| Failure::Worker("unknown Solver root completion".into()))?;
        if *completed {
            return Err(Failure::Worker("duplicate Solver root completion".into()));
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
            "solver.profiles_exhausted",
            "Completed Solver profiles",
            self.proof.profiles_exhausted,
        )];
        if let Some(ms) = self.minimum_links_ms {
            custom.push(Diagnostic::counter(
                "solver.minimum_links_complete_ms",
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
                // Descending owners start the opposite end of the complete cover
                // first, avoiding the measured ascending-order completion cliff.
                for source in
                    (0..self.problem.inputs.len() + task.profile.node_count() as usize).rev()
                {
                    roots.push(Root {
                        profile,
                        source: Some(source),
                        second_source: None,
                        impossible: false,
                    });
                }
            } else {
                roots.push(Root {
                    profile,
                    source: None,
                    second_source: None,
                    impossible,
                });
            }
        }
        if self.options.mode == SolveMode::AllMinNL
            && matches!(self.counts, Counts::Boolean)
            && self.options.worker_count > 1
            && self.problem.outputs.len() >= 2
            && roots.iter().filter(|root| !root.impossible).count() >= self.options.worker_count
        {
            return self.adaptive_group(nodes, links, tasks, &roots);
        }
        let roots =
            if self.options.mode == SolveMode::AllMinNL && matches!(self.counts, Counts::Boolean) {
                refine_roots(
                    roots,
                    tasks,
                    self.problem.inputs.len(),
                    self.problem.outputs.len(),
                    self.options.worker_count,
                )
            } else {
                roots
            };
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
        let diagnostics = std::env::var_os("SOLVER_DIAGNOSTICS").is_some_and(|v| v == "1");
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
                                root.second_source,
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
                    messages.push(Failure::Worker("Solver worker panicked".into()));
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
            return Err(Failure::Worker("unfinished Solver profile group".into()));
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
            "solver.profiles_exhausted",
            "Completed Solver profiles",
            self.proof.profiles_exhausted,
        )];
        if let Some(trace) = trace {
            custom.push(Diagnostic::text(
                "solver.root",
                "Solver root evidence",
                trace,
            ));
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
        .map_err(|e| Failure::Worker(format!("Solver witness restoration failed: {e}")))?;
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
    second_source: Option<usize>,
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
        session.write(&encoding.output_source_assertion(0, source)?)?;
    }
    if let Some(source) = second_source {
        session.write(&encoding.output_source_assertion(1, source)?)?;
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
                    "Solver model failed independent reconstruction: {error}"
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
            return Err(Failure::Worker("Solver model objective mismatch".into()));
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
                second_source: None,
                impossible: false,
            },
            Root {
                profile: 0,
                source: Some(1),
                second_source: None,
                impossible: false,
            },
            Root {
                profile: 1,
                source: None,
                second_source: None,
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
