//! Outer lexicographic production proof orchestration.

use std::{
    collections::BTreeMap,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    thread,
    time::Instant,
};

#[cfg(test)]
use std::sync::Arc;

use solver_api::{
    BestKnownSolution, ConsumerPortRef, IncompleteReason, IncompleteResult, InputTerminalIndex,
    OptimalSolution, OutputTerminalIndex, PhysicalGraph, Problem, ProducerPortRef, ProofSummary,
    SolvePhase, SolveResult, SolverEvent, SolverProgress,
};
use solver_validation::{ValidationError, validate_solution};
use thiserror::Error;

use crate::{
    acyclic_incumbent::find_small_acyclic_witness,
    canonical::canonicalize_witness_cancellable,
    lower_bound::{LowerBoundError, baseline_lower_bounds},
    problem::{InvalidProblem, NormalizedProblem, Preparation, prepare_problem},
    profile::{
        AccountedProfile, ProfileArithmeticError, ProfileLinkAccounting,
        enumerate_accounted_profile_groups,
    },
    proof_ledger::{
        FoldedProfileResult, ParentProofStatus, PartitionFailure, PartitionResult, ProofLedger,
        ProofLedgerError,
    },
    search::{
        ProfileSearchError, ProfileSearchResult, ProfileSearchStats, ProfileWitness, RootPartition,
        RootPartitionPlan, plan_profile_root_partitions_with_accounting,
        search_profile_root_partition,
    },
    telemetry::{ProofObligation, SearchInstrumentation},
};

const PROOF_VERSION: u32 = 1;

/// Finite controls for production search.
///
/// `None` keeps increasing the physical node count until a solution is proved optimal or the
/// caller cancels. A finite bound is intended for tests and explicitly resource-limited runs; its
/// exhaustion is never a global unsatisfiability proof.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SolveOptions {
    /// Largest physical node count to exhaust, inclusive.
    pub max_nodes: Option<u32>,
    /// Maximum fixed-profile proof workers in the current equal-link group.
    pub worker_count: usize,
}

impl Default for SolveOptions {
    fn default() -> Self {
        Self {
            max_nodes: None,
            worker_count: 1,
        }
    }
}

pub use solver_api::SolveObserver;

/// Invalid input or an internal failure that prevents a production proof from being trusted.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum SolverError {
    /// Malformed public problem.
    #[error(transparent)]
    InvalidProblem(#[from] InvalidProblem),
    /// Exact profile/link accounting overflowed.
    #[error(transparent)]
    ProfileArithmetic(#[from] ProfileArithmeticError),
    /// Exact baseline-bound accounting overflowed.
    #[error(transparent)]
    LowerBound(#[from] LowerBoundError),
    /// A zero-worker configuration cannot discharge proof obligations.
    #[error("worker_count must be at least one")]
    InvalidWorkerCount,
    /// One fixed-profile proof failed internally.
    #[error(transparent)]
    ProfileSearch(#[from] ProfileSearchError),
    /// A proof worker unwound instead of returning an explicit leaf status.
    #[error("profile worker panicked while proving deterministic profile {profile_index}")]
    ProfileWorkerPanicked {
        /// Canonical index of the affected profile in its equal-link group.
        profile_index: usize,
    },
    /// A normalized terminal could not be restored to its original public index.
    #[error("normalized {side} terminal {normalized_index} has no valid original mapping")]
    TerminalMapping {
        /// `input` or `output`.
        side: &'static str,
        /// Canonical normalized terminal index.
        normalized_index: u32,
    },
    /// A mapped/scaled witness failed the independent exact validator.
    #[error("mapped production witness failed the independent validation firewall: {0}")]
    ValidationFirewall(Box<ValidationError>),
    /// Independent validation disagreed with the fixed lexicographic obligation.
    #[error(
        "mapped witness count mismatch: expected ({expected_nodes} nodes, {expected_links} modeled links, {expected_physical_links} physical links, {expected_discard_links} discard links), got ({actual_nodes}, {actual_links}, {actual_physical_links}, {actual_discard_links})"
    )]
    ValidationCountMismatch {
        /// Fixed node count being searched.
        expected_nodes: u32,
        /// Fixed equal-link group being searched.
        expected_links: u32,
        /// Fixed total physical-link count for this profile.
        expected_physical_links: u32,
        /// Fixed anonymous discard-link count for this profile.
        expected_discard_links: u32,
        /// Independently counted nodes.
        actual_nodes: u32,
        /// Independently counted links.
        actual_links: u32,
        /// Independently counted total physical links.
        actual_physical_links: u32,
        /// Independently counted anonymous discard links.
        actual_discard_links: u32,
    },
    /// Finite proof accounting could not be represented without wrapping.
    #[error("production proof accounting overflowed")]
    ProofAccountingOverflow,
    /// The explicit hierarchical proof ledger rejected an update.
    #[error("hierarchical proof ledger rejected an update: {detail}")]
    ProofLedger {
        /// Stable internal diagnostic.
        detail: String,
    },
    /// A parent proof status disagreed with the coordinator's deterministic fold.
    #[error("hierarchical proof ledger invariant failed: {detail}")]
    ProofLedgerInvariant {
        /// Stable diagnostic describing the mismatched parent obligation.
        detail: String,
    },
}

impl From<ProofLedgerError> for SolverError {
    fn from(error: ProofLedgerError) -> Self {
        Self::ProofLedger {
            detail: error.to_string(),
        }
    }
}

/// Solves a public exact-flow problem for the minimum physical operator count.
///
/// The solver exhausts all smaller node counts, all smaller link groups at the winning node count,
/// and every profile in the winning equal-link group. A witness is mapped out of normalized units
/// and terminal order, then independently validated against `problem` before either `Optimal` or
/// `best_known` can be returned.
///
/// # Errors
///
/// Returns [`SolverError::InvalidProblem`] for malformed input. Arithmetic, fixed-profile search,
/// terminal restoration, proof-accounting, or final validation failures are internal errors and
/// never mathematical UNSAT results.
#[allow(clippy::too_many_lines)]
pub fn solve(
    problem: &Problem,
    options: &SolveOptions,
    cancel: &AtomicBool,
) -> Result<SolveResult, SolverError> {
    solve_with_observer(problem, options, cancel, &|_| {})
}

/// Solves with live progress and validated-incumbent notifications.
///
/// # Errors
///
/// Returns the same exact input or internal failures as [`solve`].
pub fn solve_with_observer(
    problem: &Problem,
    options: &SolveOptions,
    cancel: &AtomicBool,
    observer: &dyn SolveObserver,
) -> Result<SolveResult, SolverError> {
    solve_internal(
        problem,
        options,
        cancel,
        observer,
        false,
        #[cfg(test)]
        None,
    )
}

/// Enumerates every distinct validated topology at the minimum physical node count.
///
/// The returned [`SolveResult`] retains the ordinary lexicographic preferred solution and proof.
/// Distinct layouts are delivered through [`SolverEvent::SolutionFound`]. A cancelled run returns
/// the ordinary incomplete result while already emitted layouts remain valid partial results.
///
/// # Errors
///
/// Returns the same errors as [`solve_with_observer`].
pub fn enumerate_with_observer(
    problem: &Problem,
    options: &SolveOptions,
    cancel: &AtomicBool,
    observer: &dyn SolveObserver,
) -> Result<SolveResult, SolverError> {
    solve_internal(
        problem,
        options,
        cancel,
        observer,
        true,
        #[cfg(test)]
        None,
    )
}

#[allow(clippy::too_many_lines)]
fn solve_internal(
    problem: &Problem,
    options: &SolveOptions,
    cancel: &AtomicBool,
    observer: &dyn SolveObserver,
    enumerate_all_at_n: bool,
    #[cfg(test)] panic_next_root_worker: Option<&AtomicBool>,
) -> Result<SolveResult, SolverError> {
    if options.worker_count == 0 {
        return Err(SolverError::InvalidWorkerCount);
    }
    let solve_started = Instant::now();
    let mut instrumentation = SearchInstrumentation::default();
    emit_progress(
        observer,
        SolvePhase::Normalizing,
        None,
        0,
        None,
        &instrumentation,
        solve_started,
    );
    let preparation = prepare_problem(problem)?;
    emit_progress(
        observer,
        SolvePhase::GlobalChecks,
        None,
        0,
        None,
        &instrumentation,
        solve_started,
    );
    let mut proof = empty_proof();
    let normalized = match preparation {
        Preparation::Prepared(normalized) => normalized,
        Preparation::GloballyUnsat(mut global) => {
            global.proof = proof;
            return Ok(SolveResult::GloballyUnsat(global));
        }
    };

    emit_progress(
        observer,
        SolvePhase::ComputingLowerBound,
        None,
        0,
        None,
        &instrumentation,
        solve_started,
    );
    let bounds = baseline_lower_bounds(&normalized)?;
    let mut progress = progress_snapshot(
        SolvePhase::ComputingLowerBound,
        None,
        0,
        None,
        &instrumentation,
        solve_started,
    );
    progress.node_lower_bound = Some(bounds.combined_nodes);
    emit_event(observer, SolverEvent::Progress(progress));
    proof.initial_node_lower_bound = bounds.combined_nodes;
    // The lower-bound certificate itself discharges every smaller node count;
    // no topology enumeration is required for those obligations.
    proof.node_counts_exhausted_through = bounds.combined_nodes.checked_sub(1);
    if options
        .max_nodes
        .is_some_and(|maximum| maximum < bounds.combined_nodes)
    {
        return Ok(incomplete(
            IncompleteReason::ResourceLimit {
                detail: format!(
                    "configured node limit is below the proven starting lower bound {}",
                    bounds.combined_nodes
                ),
            },
            None,
            proof,
        ));
    }
    let input_count =
        u32::try_from(normalized.inputs.len()).map_err(|_| SolverError::TerminalMapping {
            side: "input",
            normalized_index: u32::MAX,
        })?;
    let output_count =
        u32::try_from(normalized.outputs.len()).map_err(|_| SolverError::TerminalMapping {
            side: "output",
            normalized_index: u32::MAX,
        })?;
    let mut best_known = None;
    let mut enumerated = BTreeMap::new();
    let mut preferred = None;
    let mut winning_node = None;
    let mut node_count = bounds.combined_nodes;
    let mut proof_ledger = ProofLedger::default();

    loop {
        if cancel.load(Ordering::Relaxed) {
            return Ok(incomplete(IncompleteReason::Cancelled, best_known, proof));
        }

        let groups = enumerate_accounted_profile_groups(
            node_count,
            input_count,
            output_count,
            &normalized.surplus,
            &normalized.max_link_rate,
        )?;
        proof_ledger.begin_node(node_count)?;
        for group in groups {
            if cancel.load(Ordering::Relaxed) {
                return Ok(incomplete(IncompleteReason::Cancelled, best_known, proof));
            }

            // A concrete validated witness in the first satisfiable structural group
            // is already a complete minimum-node certificate: the proven
            // node lower bound discharges every smaller N, and the groups
            // visited earlier at this N were folded UNSAT. This optional small
            // acyclic constructor proves only existence; a miss leaves the
            // exhaustive search unchanged.
            if node_count >= 3 {
                for accounted in &group.profiles {
                    if let Some(graph) =
                        find_small_acyclic_witness(&normalized, accounted.profile, cancel)
                    {
                        emit_progress(
                            observer,
                            SolvePhase::ValidatingWitness,
                            Some(ProofObligation {
                                node_count,
                                link_count: Some(group.link_count),
                                profile: Some(accounted.profile),
                                root_partition: None,
                            }),
                            0,
                            Some(
                                u32::try_from(group.profiles.len())
                                    .map_err(|_| SolverError::ProofAccountingOverflow)?,
                            ),
                            &instrumentation,
                            solve_started,
                        );
                        let normalized_problem = Problem {
                            inputs: normalized.inputs.as_slice().to_vec(),
                            outputs: normalized.outputs.as_slice().to_vec(),
                            max_link_rate: normalized.max_link_rate.clone(),
                        };
                        let Some(canonical) =
                            canonicalize_witness_cancellable(&normalized_problem, &graph, cancel)
                        else {
                            return Ok(incomplete(IncompleteReason::Cancelled, best_known, proof));
                        };
                        let validation = validate_solution(&normalized_problem, &canonical.graph)
                            .map_err(|error| {
                            SolverError::ValidationFirewall(Box::new(error))
                        })?;
                        if validation.link_count != accounted.accounting.link_count {
                            continue;
                        }
                        let witness = ProfileWitness {
                            canonical_graph_key: canonical.key,
                            graph: canonical.graph,
                            validation,
                        };
                        let candidate = restore_and_validate(
                            problem,
                            &normalized,
                            witness,
                            node_count,
                            accounted.accounting,
                        )?;
                        retain_best(&mut best_known, candidate.clone());
                        emit_event(observer, SolverEvent::Incumbent(candidate.clone()));
                        if enumerate_all_at_n
                            && enumerated
                                .insert(candidate.canonical_graph_key.clone(), candidate.clone())
                                .is_none()
                        {
                            emit_event(observer, SolverEvent::SolutionFound(candidate.clone()));
                        }
                        if cancel.load(Ordering::Relaxed) {
                            return Ok(incomplete(IncompleteReason::Cancelled, best_known, proof));
                        }
                        if !enumerate_all_at_n {
                            return Ok(SolveResult::Optimal(optimal_solution(candidate, proof)));
                        }
                    }
                }
            }

            // A SAT profile does not discharge the equal-link obligation. Every profile with this
            // exact L is exhausted so the canonical witness choice is scheduling-independent.
            let total_profiles = u32::try_from(group.profiles.len())
                .map_err(|_| SolverError::ProofAccountingOverflow)?;
            emit_progress(
                observer,
                SolvePhase::Searching,
                Some(ProofObligation {
                    node_count,
                    link_count: Some(group.link_count),
                    profile: None,
                    root_partition: None,
                }),
                0,
                Some(total_profiles),
                &instrumentation,
                solve_started,
            );
            let profile_tasks = ProfileGroupRun {
                problem: &normalized,
                node_count,
                link_count: group.link_count,
                requested_workers: options.worker_count,
                cancel,
                collect_all_witnesses: enumerate_all_at_n,
                #[cfg(test)]
                panic_next_root_worker,
            }
            .run(group.profiles, &mut proof_ledger)?;
            let mut group_best = None;
            let mut completed_profiles = 0_u32;
            let mut stopped = None;
            for task in profile_tasks {
                let accounted = task.accounted;
                let profile = accounted.profile;
                for root in &task.roots {
                    merge_instrumentation(&mut instrumentation, &root.instrumentation);
                    if root.exhausted {
                        increment_proof(&mut proof.root_partitions_exhausted)?;
                    }
                    emit_progress(
                        observer,
                        SolvePhase::Searching,
                        Some(ProofObligation {
                            node_count,
                            link_count: Some(group.link_count),
                            profile: Some(profile),
                            root_partition: Some(root.partition),
                        }),
                        completed_profiles,
                        Some(total_profiles),
                        &instrumentation,
                        solve_started,
                    );
                }
                let collected_witnesses = task.witnesses;
                match task.result {
                    ProfileTaskResult::Search(result) => match *result {
                        ProfileSearchResult::Exhausted {
                            best_witness,
                            witnesses: _,
                            stats: _,
                        } => {
                            completed_profiles = completed_profiles
                                .checked_add(1)
                                .ok_or(SolverError::ProofAccountingOverflow)?;
                            let witnesses = if enumerate_all_at_n {
                                collected_witnesses
                            } else {
                                best_witness.into_iter().collect()
                            };
                            for witness in witnesses {
                                emit_progress(
                                    observer,
                                    SolvePhase::ValidatingWitness,
                                    Some(ProofObligation {
                                        node_count,
                                        link_count: Some(group.link_count),
                                        profile: Some(profile),
                                        root_partition: Some(0),
                                    }),
                                    completed_profiles,
                                    Some(total_profiles),
                                    &instrumentation,
                                    solve_started,
                                );
                                let candidate = restore_and_validate(
                                    problem,
                                    &normalized,
                                    witness,
                                    node_count,
                                    accounted.accounting,
                                )?;
                                retain_best(&mut group_best, candidate.clone());
                                if retain_best(&mut best_known, candidate.clone()) {
                                    emit_event(observer, SolverEvent::Incumbent(candidate.clone()));
                                }
                                if enumerate_all_at_n
                                    && enumerated
                                        .insert(
                                            candidate.canonical_graph_key.clone(),
                                            candidate.clone(),
                                        )
                                        .is_none()
                                {
                                    emit_event(observer, SolverEvent::SolutionFound(candidate));
                                }
                            }
                            increment_proof(&mut proof.profiles_exhausted)?;
                        }
                        ProfileSearchResult::Incomplete {
                            reason,
                            best_witness,
                            witnesses: _,
                            stats: _,
                        } => {
                            let witnesses = if enumerate_all_at_n {
                                collected_witnesses
                            } else {
                                best_witness.into_iter().collect()
                            };
                            for witness in witnesses {
                                emit_progress(
                                    observer,
                                    SolvePhase::ValidatingWitness,
                                    Some(ProofObligation {
                                        node_count,
                                        link_count: Some(group.link_count),
                                        profile: Some(profile),
                                        root_partition: Some(0),
                                    }),
                                    completed_profiles,
                                    Some(total_profiles),
                                    &instrumentation,
                                    solve_started,
                                );
                                let candidate = restore_and_validate(
                                    problem,
                                    &normalized,
                                    witness,
                                    node_count,
                                    accounted.accounting,
                                )?;
                                if retain_best(&mut best_known, candidate.clone()) {
                                    emit_event(observer, SolverEvent::Incumbent(candidate.clone()));
                                }
                                if enumerate_all_at_n
                                    && enumerated
                                        .insert(
                                            candidate.canonical_graph_key.clone(),
                                            candidate.clone(),
                                        )
                                        .is_none()
                                {
                                    emit_event(observer, SolverEvent::SolutionFound(candidate));
                                }
                            }
                            stopped.get_or_insert(reason);
                        }
                        ProfileSearchResult::Failed { error, stats: _ } => {
                            return Err(SolverError::ProfileSearch(error));
                        }
                    },
                    ProfileTaskResult::WorkerPanicked => {
                        return Err(SolverError::ProfileWorkerPanicked {
                            profile_index: task.index,
                        });
                    }
                }
                emit_progress(
                    observer,
                    SolvePhase::Searching,
                    Some(ProofObligation {
                        node_count,
                        link_count: Some(group.link_count),
                        profile: Some(profile),
                        root_partition: None,
                    }),
                    completed_profiles,
                    Some(total_profiles),
                    &instrumentation,
                    solve_started,
                );
                // Incumbent delivery is synchronous and occurs only after the
                // independent validator accepted the candidate. Polling here
                // lets an observer cancel at that exact deterministic seam.
                // The completed leaf/profile counters above remain valid, but
                // the enclosing link group is deliberately not promoted to an
                // optimality proof after cancellation was observed.
                if cancel.load(Ordering::Relaxed) {
                    return Ok(incomplete(IncompleteReason::Cancelled, best_known, proof));
                }
            }
            if let Some(reason) = stopped {
                if !matches!(
                    proof_ledger.link_group_status(node_count, group.link_count),
                    ParentProofStatus::Incomplete { .. }
                ) {
                    return Err(SolverError::ProofLedgerInvariant {
                        detail: format!(
                            "stopped link group ({node_count}, {}) was not incomplete",
                            group.link_count
                        ),
                    });
                }
                return Ok(incomplete(reason, best_known, proof));
            }

            increment_proof(&mut proof.link_groups_exhausted)?;
            if let Some(best) = group_best {
                let ParentProofStatus::Sat { best_witness } =
                    proof_ledger.link_group_status(node_count, group.link_count)
                else {
                    return Err(SolverError::ProofLedgerInvariant {
                        detail: format!(
                            "winning link group ({node_count}, {}) did not fold to SAT",
                            group.link_count
                        ),
                    });
                };
                if best_witness.canonical_graph_key != best.canonical_graph_key {
                    return Err(SolverError::ProofLedgerInvariant {
                        detail: format!(
                            "winning link group ({node_count}, {}) disagreed on canonical witness",
                            group.link_count
                        ),
                    });
                }
                if !enumerate_all_at_n {
                    return Ok(SolveResult::Optimal(optimal_solution(best, proof)));
                }
                winning_node = Some(node_count);
                retain_best(&mut preferred, best);
                continue;
            }
            if proof_ledger.link_group_status(node_count, group.link_count)
                != ParentProofStatus::Unsat
            {
                return Err(SolverError::ProofLedgerInvariant {
                    detail: format!(
                        "witness-free link group ({node_count}, {}) did not fold to UNSAT",
                        group.link_count
                    ),
                });
            }
        }

        if winning_node == Some(node_count) {
            let best = preferred.ok_or_else(|| SolverError::ProofLedgerInvariant {
                detail: format!(
                    "enumeration exhausted satisfiable node obligation {node_count} without a preferred witness"
                ),
            })?;
            return Ok(SolveResult::Optimal(optimal_solution(best, proof)));
        }

        if proof_ledger.node_status(node_count) != ParentProofStatus::Unsat {
            return Err(SolverError::ProofLedgerInvariant {
                detail: format!(
                    "fully searched node obligation {node_count} did not fold to UNSAT"
                ),
            });
        }
        proof.node_counts_exhausted_through = Some(node_count);
        if options.max_nodes == Some(node_count) {
            return Ok(incomplete(
                IncompleteReason::ResourceLimit {
                    detail: format!(
                        "production solver exhausted every topology through {node_count} physical nodes"
                    ),
                },
                best_known,
                proof,
            ));
        }
        let Some(next_node_count) = node_count.checked_add(1) else {
            return Ok(incomplete(
                IncompleteReason::ResourceLimit {
                    detail:
                        "production solver exhausted the representable physical node-count range"
                            .to_owned(),
                },
                best_known,
                proof,
            ));
        };
        node_count = next_node_count;
    }
}

#[derive(Debug)]
struct ProfileTaskOutput {
    index: usize,
    accounted: AccountedProfile,
    result: ProfileTaskResult,
    roots: Vec<RootCompletion>,
    witnesses: Vec<ProfileWitness>,
}

#[derive(Debug)]
enum ProfileTaskResult {
    Search(Box<ProfileSearchResult>),
    WorkerPanicked,
}

#[derive(Debug)]
struct RootCompletion {
    partition: u32,
    exhausted: bool,
    instrumentation: SearchInstrumentation,
    witnesses: Vec<ProfileWitness>,
}

#[derive(Clone, Debug)]
struct RootTask {
    profile_index: usize,
    accounted: AccountedProfile,
    partition: RootPartition,
}

#[derive(Debug)]
struct RootTaskOutput {
    profile_index: usize,
    partition: u32,
    result: RootTaskResult,
}

type RootWorker<'scope> = thread::ScopedJoinHandle<'scope, Vec<RootTaskOutput>>;

#[derive(Debug)]
enum RootTaskResult {
    Search(Box<ProfileSearchResult>),
    WorkerPanicked,
}

struct ProfileGroupRun<'a> {
    problem: &'a NormalizedProblem,
    node_count: u32,
    link_count: u32,
    requested_workers: usize,
    cancel: &'a AtomicBool,
    collect_all_witnesses: bool,
    #[cfg(test)]
    panic_next_root_worker: Option<&'a AtomicBool>,
}

/// Proves one equal-link group with deterministic, static profile assignment.
///
/// Each fixed profile is a complete, independent proof obligation. Running those
/// obligations concurrently therefore changes neither the searched quotient nor
/// the lexicographic proof. Results are sorted back into canonical profile order
/// before proof counters, incumbents, or progress events become observable.
impl ProfileGroupRun<'_> {
    fn run(
        &self,
        profiles: Vec<AccountedProfile>,
        ledger: &mut ProofLedger,
    ) -> Result<Vec<ProfileTaskOutput>, SolverError> {
        ledger.register_link_group(
            self.node_count,
            self.link_count,
            profiles.iter().map(|accounted| accounted.profile),
        )?;
        let (root_tasks, mut root_outputs) = self.plan_roots(&profiles, ledger)?;
        root_outputs.extend(self.execute_roots(&root_tasks));
        root_outputs.sort_unstable_by_key(|task| (task.profile_index, task.partition));
        let mut completions = self.record_roots(&profiles, root_outputs, ledger)?;
        self.fold_profiles(profiles, &mut completions, ledger)
    }

    fn plan_roots(
        &self,
        profiles: &[AccountedProfile],
        ledger: &mut ProofLedger,
    ) -> Result<(Vec<RootTask>, Vec<RootTaskOutput>), SolverError> {
        let mut tasks = Vec::new();
        let mut immediate = Vec::new();
        for (profile_index, accounted) in profiles.iter().copied().enumerate() {
            if self.cancel.load(Ordering::Relaxed) {
                ledger.register_profile_partitions(
                    self.node_count,
                    self.link_count,
                    accounted.profile,
                    [0],
                )?;
                immediate.push(cancelled_root_output(profile_index, 0));
                continue;
            }
            match plan_profile_root_partitions_with_accounting(
                self.problem,
                accounted.profile,
                self.cancel,
                Some(accounted.accounting),
            ) {
                RootPartitionPlan::Partitions(partitions) => {
                    ledger.register_profile_partitions(
                        self.node_count,
                        self.link_count,
                        accounted.profile,
                        partitions.iter().map(|partition| partition.id().ordinal()),
                    )?;
                    tasks.extend(partitions.into_iter().map(|partition| RootTask {
                        profile_index,
                        accounted,
                        partition,
                    }));
                }
                RootPartitionPlan::Immediate(result) => {
                    ledger.register_profile_partitions(
                        self.node_count,
                        self.link_count,
                        accounted.profile,
                        [0],
                    )?;
                    immediate.push(RootTaskOutput {
                        profile_index,
                        partition: 0,
                        result: RootTaskResult::Search(result),
                    });
                }
            }
        }
        Ok((tasks, immediate))
    }

    fn execute_roots(&self, tasks: &[RootTask]) -> Vec<RootTaskOutput> {
        let worker_count = self.requested_workers.min(tasks.len());
        let next_task = AtomicUsize::new(0);
        thread::scope(|scope| {
            let mut handles = Vec::with_capacity(worker_count);
            for _ in 0..worker_count {
                let next_task = &next_task;
                let handle = scope.spawn(move || {
                    let mut completed = Vec::new();
                    loop {
                        // Stop claiming new root obligations once cancel is set.
                        // In-flight searches still observe the same flag.
                        if self.cancel.load(Ordering::Relaxed) {
                            break;
                        }
                        let index = next_task.fetch_add(1, Ordering::Relaxed);
                        let Some(task) = tasks.get(index) else {
                            break;
                        };
                        completed.push(self.execute_root(task));
                    }
                    completed
                });
                handles.push(handle);
            }
            Self::join_root_workers(handles, tasks, self.cancel)
        })
    }

    fn execute_root(&self, task: &RootTask) -> RootTaskOutput {
        let partition = task.partition.id().ordinal();
        let result = catch_unwind(AssertUnwindSafe(|| {
            #[cfg(test)]
            assert!(
                !self
                    .panic_next_root_worker
                    .is_some_and(|flag| flag.swap(false, Ordering::Relaxed)),
                "injected worker failure"
            );
            search_profile_root_partition(
                self.problem,
                task.accounted.profile,
                self.cancel,
                Some(task.accounted.accounting),
                &task.partition,
                self.collect_all_witnesses,
            )
        }))
        .map_or(RootTaskResult::WorkerPanicked, |result| {
            RootTaskResult::Search(Box::new(result))
        });
        RootTaskOutput {
            profile_index: task.profile_index,
            partition,
            result,
        }
    }

    fn join_root_workers(
        handles: Vec<RootWorker<'_>>,
        tasks: &[RootTask],
        cancel: &AtomicBool,
    ) -> Vec<RootTaskOutput> {
        let mut completed = Vec::with_capacity(tasks.len());
        for handle in handles {
            if let Ok(mut outputs) = handle.join() {
                completed.append(&mut outputs);
            }
        }
        // Sample cancel only after workers stop. Sampling earlier races with the
        // dynamic queue: a late cancel can leave unclaimed roots that would then
        // be recorded as panics instead of Cancelled.
        let cancelled = cancel.load(Ordering::Relaxed)
            || completed.iter().any(|output| {
                matches!(
                    &output.result,
                    RootTaskResult::Search(result)
                        if matches!(
                            result.as_ref(),
                            ProfileSearchResult::Incomplete {
                                reason: IncompleteReason::Cancelled,
                                ..
                            }
                        )
                )
            });
        let completed_keys = completed
            .iter()
            .map(|output| (output.profile_index, output.partition))
            .collect::<std::collections::BTreeSet<_>>();
        completed.extend(tasks.iter().filter_map(|task| {
            let partition = task.partition.id().ordinal();
            let key = (task.profile_index, partition);
            if completed_keys.contains(&key) {
                return None;
            }
            Some(if cancelled {
                cancelled_root_output(task.profile_index, partition)
            } else {
                RootTaskOutput {
                    profile_index: task.profile_index,
                    partition,
                    result: RootTaskResult::WorkerPanicked,
                }
            })
        }));
        completed
    }

    fn record_roots(
        &self,
        profiles: &[AccountedProfile],
        root_outputs: Vec<RootTaskOutput>,
        ledger: &mut ProofLedger,
    ) -> Result<Vec<Vec<RootCompletion>>, SolverError> {
        let mut completions = (0..profiles.len()).map(|_| Vec::new()).collect::<Vec<_>>();
        for output in root_outputs {
            let (result, completion) = root_output_parts(output.partition, output.result);
            let profile = profiles[output.profile_index].profile;
            ledger.record_partition(
                self.node_count,
                self.link_count,
                profile,
                output.partition,
                result,
            )?;
            completions[output.profile_index].push(completion);
        }
        Ok(completions)
    }

    fn fold_profiles(
        &self,
        profiles: Vec<AccountedProfile>,
        completions: &mut [Vec<RootCompletion>],
        ledger: &ProofLedger,
    ) -> Result<Vec<ProfileTaskOutput>, SolverError> {
        profiles
            .into_iter()
            .enumerate()
            .map(|(index, accounted)| {
                let folded = ledger
                    .fold_profile(self.node_count, self.link_count, accounted.profile)?
                    .ok_or_else(|| SolverError::ProofLedgerInvariant {
                        detail: format!(
                            "profile {index} in ({}, {}) retained pending leaves",
                            self.node_count, self.link_count
                        ),
                    })?;
                let result = match folded {
                    FoldedProfileResult::Search(result) => ProfileTaskResult::Search(result),
                    FoldedProfileResult::WorkerPanicked => ProfileTaskResult::WorkerPanicked,
                };
                let roots = std::mem::take(&mut completions[index]);
                let witnesses = roots
                    .iter()
                    .flat_map(|root| root.witnesses.clone())
                    .collect();
                Ok(ProfileTaskOutput {
                    index,
                    accounted,
                    result,
                    roots,
                    witnesses,
                })
            })
            .collect()
    }
}

fn cancelled_root_output(profile_index: usize, partition: u32) -> RootTaskOutput {
    RootTaskOutput {
        profile_index,
        partition,
        result: RootTaskResult::Search(Box::new(ProfileSearchResult::Incomplete {
            reason: IncompleteReason::Cancelled,
            best_witness: None,
            witnesses: Vec::new(),
            stats: ProfileSearchStats::default(),
        })),
    }
}

fn root_output_parts(partition: u32, result: RootTaskResult) -> (PartitionResult, RootCompletion) {
    match result {
        RootTaskResult::Search(result) => {
            let (exhausted, instrumentation) = profile_result_metadata(&result);
            let witnesses = match result.as_ref() {
                ProfileSearchResult::Exhausted { witnesses, .. }
                | ProfileSearchResult::Incomplete { witnesses, .. } => witnesses.clone(),
                ProfileSearchResult::Failed { .. } => Vec::new(),
            };
            (
                PartitionResult::from(*result),
                RootCompletion {
                    partition,
                    exhausted,
                    instrumentation,
                    witnesses,
                },
            )
        }
        RootTaskResult::WorkerPanicked => (
            PartitionResult::Failed {
                error: PartitionFailure::WorkerPanicked,
                progress: ProfileSearchStats::default(),
            },
            RootCompletion {
                partition,
                exhausted: false,
                instrumentation: SearchInstrumentation::default(),
                witnesses: Vec::new(),
            },
        ),
    }
}

fn profile_result_metadata(result: &ProfileSearchResult) -> (bool, SearchInstrumentation) {
    match result {
        ProfileSearchResult::Exhausted { stats, .. } => (true, stats.instrumentation.clone()),
        ProfileSearchResult::Incomplete { stats, .. }
        | ProfileSearchResult::Failed { stats, .. } => (false, stats.instrumentation.clone()),
    }
}

/// Exhausts every profile in one equal-link group, even after the first SAT result.
///
/// `Ok(true)` means at least one fully exhausted profile produced a witness. Only an explicit
/// callback error may stop the group early, which the outer solver maps to `Incomplete` or an
/// internal error rather than an optimality claim.
#[cfg(test)]
fn exhaust_equal_link_group<P, E>(
    profiles: impl IntoIterator<Item = P>,
    mut exhaust_profile: impl FnMut(P) -> Result<bool, E>,
) -> Result<bool, E> {
    let mut group_sat = false;
    for profile in profiles {
        group_sat |= exhaust_profile(profile)?;
    }
    Ok(group_sat)
}

fn restore_and_validate(
    caller_problem: &Problem,
    normalized: &NormalizedProblem,
    witness: ProfileWitness,
    node_count: u32,
    accounting: ProfileLinkAccounting,
) -> Result<BestKnownSolution, SolverError> {
    let mut graph = witness.graph;
    restore_terminals(&mut graph, normalized)?;
    for link in &mut graph.links {
        link.flow = &link.flow * &normalized.original_scale;
    }
    let validation = validate_solution(caller_problem, &graph)
        .map_err(|error| SolverError::ValidationFirewall(Box::new(error)))?;
    if validation.node_count != node_count
        || validation.link_count != accounting.link_count
        || validation.physical_link_count != accounting.physical_link_count
        || validation.discard_link_count != accounting.discard_link_count
    {
        return Err(SolverError::ValidationCountMismatch {
            expected_nodes: node_count,
            expected_links: accounting.link_count,
            expected_physical_links: accounting.physical_link_count,
            expected_discard_links: accounting.discard_link_count,
            actual_nodes: validation.node_count,
            actual_links: validation.link_count,
            actual_physical_links: validation.physical_link_count,
            actual_discard_links: validation.discard_link_count,
        });
    }
    Ok(BestKnownSolution {
        node_count,
        link_count: validation.link_count,
        physical_link_count: validation.physical_link_count,
        discard_link_count: validation.discard_link_count,
        // Canonical identity deliberately stays in normalized units and canonical terminal order.
        canonical_graph_key: witness.canonical_graph_key,
        graph,
        validation,
    })
}

fn restore_terminals(
    graph: &mut PhysicalGraph,
    normalized: &NormalizedProblem,
) -> Result<(), SolverError> {
    for link in &mut graph.links {
        if let ProducerPortRef::Input(input) = link.producer {
            let canonical = usize::try_from(input.0).map_err(|_| SolverError::TerminalMapping {
                side: "input",
                normalized_index: input.0,
            })?;
            let original = normalized
                .terminal_mapping
                .original_input(canonical)
                .and_then(|index| u32::try_from(index).ok())
                .ok_or(SolverError::TerminalMapping {
                    side: "input",
                    normalized_index: input.0,
                })?;
            link.producer = ProducerPortRef::Input(InputTerminalIndex(original));
        }
        if let ConsumerPortRef::Output(output) = link.consumer {
            let canonical =
                usize::try_from(output.0).map_err(|_| SolverError::TerminalMapping {
                    side: "output",
                    normalized_index: output.0,
                })?;
            let original = normalized
                .terminal_mapping
                .original_output(canonical)
                .and_then(|index| u32::try_from(index).ok())
                .ok_or(SolverError::TerminalMapping {
                    side: "output",
                    normalized_index: output.0,
                })?;
            link.consumer = ConsumerPortRef::Output(OutputTerminalIndex(original));
        }
    }
    Ok(())
}

fn empty_proof() -> ProofSummary {
    ProofSummary {
        proof_version: PROOF_VERSION,
        initial_node_lower_bound: 0,
        node_counts_exhausted_through: None,
        link_groups_exhausted: 0,
        profiles_exhausted: 0,
        root_partitions_exhausted: 0,
    }
}

fn incomplete(
    reason: IncompleteReason,
    best_known: Option<BestKnownSolution>,
    proof: ProofSummary,
) -> SolveResult {
    SolveResult::Incomplete(IncompleteResult {
        reason,
        best_known,
        proof,
    })
}

fn optimal_solution(candidate: BestKnownSolution, proof: ProofSummary) -> OptimalSolution {
    OptimalSolution {
        node_count: candidate.node_count,
        link_count: candidate.link_count,
        physical_link_count: candidate.physical_link_count,
        discard_link_count: candidate.discard_link_count,
        canonical_graph_key: candidate.canonical_graph_key,
        graph: candidate.graph,
        proof,
        validation: candidate.validation,
    }
}

fn retain_best(target: &mut Option<BestKnownSolution>, candidate: BestKnownSolution) -> bool {
    if target.as_ref().is_none_or(|current| {
        (
            candidate.node_count,
            candidate.link_count,
            &candidate.canonical_graph_key,
        ) < (
            current.node_count,
            current.link_count,
            &current.canonical_graph_key,
        )
    }) {
        *target = Some(candidate);
        true
    } else {
        false
    }
}

fn merge_instrumentation(total: &mut SearchInstrumentation, profile: &SearchInstrumentation) {
    total.raw_structural_decisions = total
        .raw_structural_decisions
        .saturating_add(profile.raw_structural_decisions);
    total.canonical_states_retained = total
        .canonical_states_retained
        .saturating_add(profile.canonical_states_retained);
    total.canonical_duplicates_eliminated = total
        .canonical_duplicates_eliminated
        .saturating_add(profile.canonical_duplicates_eliminated);
    total.propagation_contradictions = total
        .propagation_contradictions
        .saturating_add(profile.propagation_contradictions);
    total.capacity_prunes = total
        .capacity_prunes
        .saturating_add(profile.capacity_prunes);
    total.lower_bound_prunes = total
        .lower_bound_prunes
        .saturating_add(profile.lower_bound_prunes);
    total.scc_solves = total.scc_solves.saturating_add(profile.scc_solves);
    total.scc_cache_hits = total.scc_cache_hits.saturating_add(profile.scc_cache_hits);
    total.canonicalization_time_ns = total
        .canonicalization_time_ns
        .saturating_add(profile.canonicalization_time_ns);
    total.algebra_time_ns = total
        .algebra_time_ns
        .saturating_add(profile.algebra_time_ns);
    total.peak_state_cache_size = total
        .peak_state_cache_size
        .max(profile.peak_state_cache_size);
    total.peak_memory_bytes = total.peak_memory_bytes.max(profile.peak_memory_bytes);
    total.wall_time_ms = total.wall_time_ms.max(profile.wall_time_ms);
}

fn emit_progress(
    observer: &dyn SolveObserver,
    phase: SolvePhase,
    obligation: Option<ProofObligation>,
    completed_profiles: u32,
    total_profiles: Option<u32>,
    instrumentation: &SearchInstrumentation,
    solve_started: Instant,
) {
    emit_event(
        observer,
        SolverEvent::Progress(progress_snapshot(
            phase,
            obligation,
            completed_profiles,
            total_profiles,
            instrumentation,
            solve_started,
        )),
    );
}

fn progress_snapshot(
    phase: SolvePhase,
    obligation: Option<ProofObligation>,
    completed_profiles: u32,
    total_profiles: Option<u32>,
    instrumentation: &SearchInstrumentation,
    solve_started: Instant,
) -> SolverProgress {
    use solver_api::{Diagnostic, LinkConstraint};
    let mut custom = vec![Diagnostic::counter(
        "custom.completed_profiles",
        "Profiles closed",
        completed_profiles,
    )];
    if let Some(total) = total_profiles {
        custom.push(Diagnostic::counter(
            "custom.total_profiles",
            "Profiles in group",
            total,
        ));
    }
    if let Some(obligation) = &obligation {
        if let Some(profile) = obligation.profile {
            custom.push(Diagnostic::text(
                "custom.profile",
                "Profile",
                format!("{profile:?}"),
            ));
        }
        if let Some(root) = obligation.root_partition {
            custom.push(Diagnostic::counter(
                "custom.root_partition",
                "Root partition",
                root,
            ));
        }
    }
    custom.extend(instrumentation.diagnostics());
    SolverProgress {
        phase,
        elapsed_ms: u64::try_from(solve_started.elapsed().as_millis()).unwrap_or(u64::MAX),
        node_count: obligation.as_ref().map(|o| o.node_count),
        link_constraint: obligation
            .as_ref()
            .and_then(|o| o.link_count)
            .map(LinkConstraint::Exact),
        node_lower_bound: obligation.as_ref().map(|o| o.node_count),
        best_node_count: None,
        best_link_count: None,
        solutions_found: 0,
        custom,
    }
}

/// Progress is advisory and must never alter the mathematical outcome.
fn emit_event(observer: &dyn SolveObserver, event: SolverEvent) {
    let _ignored_observer_panic = catch_unwind(AssertUnwindSafe(|| observer.on_event(event)));
}

fn increment_proof(counter: &mut u64) -> Result<(), SolverError> {
    *counter = counter
        .checked_add(1)
        .ok_or(SolverError::ProofAccountingOverflow)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use solver_api::{GlobalUnsatProof, GlobalUnsatReason, NodeProfile, Rational};
    use solver_reference::{ReferenceOptions, solve_reference};

    use super::*;
    use crate::{
        profile::{enumerate_profile_groups, profile_link_accounting},
        search::search_profile,
    };

    fn problem(inputs: &[u32], outputs: &[u32], capacity: u32) -> Problem {
        Problem {
            inputs: inputs.iter().copied().map(Rational::from).collect(),
            outputs: outputs.iter().copied().map(Rational::from).collect(),
            max_link_rate: Rational::from(capacity),
        }
    }

    fn bounded(problem: &Problem, max_nodes: u32) -> SolveResult {
        solve(
            problem,
            &SolveOptions {
                max_nodes: Some(max_nodes),
                ..SolveOptions::default()
            },
            &AtomicBool::new(false),
        )
        .unwrap()
    }

    #[test]
    fn direct_link_is_proved_optimal_and_independently_validated() {
        let problem = problem(&[6], &[6], 6);
        let SolveResult::Optimal(solution) = bounded(&problem, 0) else {
            panic!("expected direct-link optimum");
        };
        assert_eq!((solution.node_count, solution.link_count), (0, 0));
        assert_eq!(
            solution.validation,
            validate_solution(&problem, &solution.graph).unwrap()
        );
    }

    #[test]
    fn enumeration_emits_every_distinct_minimum_node_layout() {
        // Both the S2+M2 (L=5) and S3+M3 (L=6) profiles are feasible at the
        // proven minimum N=2. Enumeration must therefore continue after the
        // first satisfiable structural link group.
        let problem = problem(&[2, 3], &[1, 4], 5);
        let layouts = Mutex::new(BTreeMap::new());
        let result = enumerate_with_observer(
            &problem,
            &SolveOptions {
                max_nodes: Some(2),
                ..SolveOptions::default()
            },
            &AtomicBool::new(false),
            &|event| {
                if let SolverEvent::SolutionFound(solution) = event {
                    layouts
                        .lock()
                        .unwrap()
                        .insert(solution.canonical_graph_key.clone(), solution);
                }
            },
        )
        .unwrap();

        let SolveResult::Optimal(preferred) = result else {
            panic!("expected a proven two-node optimum");
        };
        let layouts = layouts.into_inner().unwrap();
        assert!(layouts.len() >= 2);
        assert!(layouts.values().all(|solution| solution.node_count == 2));
        assert!(layouts.values().any(|solution| solution.link_count == 1));
        assert!(layouts.values().any(|solution| solution.link_count == 2));
        assert!(layouts.contains_key(&preferred.canonical_graph_key));
    }

    #[test]
    fn cancelled_enumeration_keeps_already_delivered_layouts() {
        let problem = problem(&[2, 3], &[1, 4], 5);
        let cancel = AtomicBool::new(false);
        let layouts = Mutex::new(Vec::new());
        let result = enumerate_with_observer(
            &problem,
            &SolveOptions {
                max_nodes: Some(2),
                ..SolveOptions::default()
            },
            &cancel,
            &|event| {
                if let SolverEvent::SolutionFound(solution) = event {
                    layouts.lock().unwrap().push(solution);
                    cancel.store(true, Ordering::Relaxed);
                }
            },
        )
        .unwrap();

        let SolveResult::Incomplete(incomplete) = result else {
            panic!("cancelled enumeration must not claim optimality");
        };
        assert_eq!(incomplete.reason, IncompleteReason::Cancelled);
        let layouts = layouts.into_inner().unwrap();
        assert!(!layouts.is_empty());
        assert!(layouts.iter().all(|solution| solution.node_count == 2));
    }

    #[test]
    fn zero_node_surplus_is_optimal_with_discard_excluded_from_l() {
        let problem = problem(&[2, 1], &[2], 2);
        let SolveResult::Optimal(solution) = bounded(&problem, 0) else {
            panic!("expected direct output plus one anonymous discard");
        };
        assert_eq!((solution.node_count, solution.link_count), (0, 0));
        assert_eq!(solution.physical_link_count, 2);
        assert_eq!(solution.discard_link_count, 1);
        assert_eq!(
            solution.validation,
            validate_solution(&problem, &solution.graph).unwrap()
        );
    }

    #[test]
    fn discard_capacity_can_force_a_node_without_increasing_l_for_discard_belts() {
        let problem = Problem {
            inputs: vec![Rational::one(), Rational::one()],
            outputs: vec!["1/2".parse().unwrap()],
            max_link_rate: Rational::one(),
        };
        let production = bounded(&problem, 1);
        let reference = solve_reference(
            &problem,
            &ReferenceOptions { max_nodes: 1 },
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_same_mathematical_outcome(&problem, &production, &reference);
        let SolveResult::Optimal(solution) = production else {
            panic!("expected one node to provide enough capacity-safe discard belts");
        };
        assert_eq!((solution.node_count, solution.link_count), (1, 0));
        assert_eq!(solution.physical_link_count, 4);
        assert_eq!(solution.discard_link_count, 2);
    }

    #[test]
    fn production_matches_reference_over_small_ordered_rate_partitions() {
        for total in 1..=4 {
            let sides = ordered_sides(total);
            for inputs in &sides {
                for outputs in &sides {
                    let problem = problem(inputs, outputs, total);
                    let production = bounded(&problem, 1);
                    let reference = solve_reference(
                        &problem,
                        &ReferenceOptions { max_nodes: 1 },
                        &AtomicBool::new(false),
                    )
                    .unwrap();
                    assert_same_mathematical_outcome(&problem, &production, &reference);
                }
            }
        }
    }

    #[test]
    fn production_matches_reference_over_all_tiny_surplus_partitions() {
        for total_input in 1..=4 {
            for total_output in 1..=total_input {
                for inputs in ordered_sides(total_input) {
                    for outputs in ordered_sides(total_output) {
                        let problem = problem(&inputs, &outputs, total_input);
                        let production = bounded(&problem, 1);
                        let reference = solve_reference(
                            &problem,
                            &ReferenceOptions { max_nodes: 1 },
                            &AtomicBool::new(false),
                        )
                        .unwrap();
                        assert_same_mathematical_outcome(&problem, &production, &reference);
                    }
                }
            }
        }
    }

    #[test]
    fn lower_link_group_wins_when_two_node_profiles_are_both_feasible() {
        // Splitter2+Merger2 realizes 2,3 -> 1,4 with five links. Splitter3+Merger3 also
        // realizes it, but with six links; finishing the L=5 group proves the former optimal.
        let problem = problem(&[2, 3], &[1, 4], 5);
        let production = bounded(&problem, 2);
        let reference = solve_reference(
            &problem,
            &ReferenceOptions { max_nodes: 2 },
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_same_mathematical_outcome(&problem, &production, &reference);
        let SolveResult::Optimal(solution) = production else {
            panic!("expected two-node optimum");
        };
        assert_eq!((solution.node_count, solution.link_count), (2, 1));
    }

    #[test]
    fn equal_link_group_driver_continues_after_the_first_sat_profile() {
        // N=3, I=O=2 has exactly two profiles in the minimum operator-L group (L=3):
        // S3+2*M2 and 2*S2+M3. Mocking only fixed-profile exhaustion keeps this policy
        // test bounded while exercising the same driver used by solve. The first profile
        // reports SAT; the second must still run.
        let group = enumerate_profile_groups(3, 2, 2)
            .unwrap()
            .into_iter()
            .find(|group| group.link_count == 3)
            .unwrap();
        assert_eq!(group.profiles.len(), 2);
        let first = group.profiles[0];
        let mut visited = Vec::new();
        let group_sat = exhaust_equal_link_group(group.profiles.clone(), |profile| {
            visited.push(profile);
            Ok::<_, std::convert::Infallible>(profile == first)
        })
        .unwrap();
        assert!(group_sat);
        assert_eq!(visited, group.profiles);
    }

    #[test]
    fn scale_and_terminal_storage_order_are_restored_before_return() {
        let scaled = problem(&[9, 3], &[12], 12);
        let permuted = problem(&[3, 9], &[12], 12);
        let base = problem(&[3, 1], &[4], 4);
        let SolveResult::Optimal(scaled_solution) = bounded(&scaled, 1) else {
            panic!("expected one-merger scaled optimum");
        };
        let SolveResult::Optimal(permuted_solution) = bounded(&permuted, 1) else {
            panic!("expected one-merger permuted optimum");
        };
        let SolveResult::Optimal(base_solution) = bounded(&base, 1) else {
            panic!("expected one-merger base optimum");
        };
        assert_eq!(
            scaled_solution.canonical_graph_key,
            base_solution.canonical_graph_key
        );
        assert_eq!(
            scaled_solution.canonical_graph_key,
            permuted_solution.canonical_graph_key
        );
        assert_eq!(
            (scaled_solution.node_count, scaled_solution.link_count),
            (permuted_solution.node_count, permuted_solution.link_count)
        );
        assert_eq!(
            terminal_input_flow(&scaled_solution.graph, InputTerminalIndex(0)),
            Rational::from(9)
        );
        assert_eq!(
            terminal_input_flow(&scaled_solution.graph, InputTerminalIndex(1)),
            Rational::from(3)
        );
        assert_eq!(
            terminal_input_flow(&permuted_solution.graph, InputTerminalIndex(0)),
            Rational::from(3)
        );
        assert_eq!(
            terminal_input_flow(&permuted_solution.graph, InputTerminalIndex(1)),
            Rational::from(9)
        );
        validate_solution(&scaled, &scaled_solution.graph).unwrap();
        validate_solution(&permuted, &permuted_solution.graph).unwrap();
    }

    #[test]
    fn cancellation_and_finite_bounds_never_claim_global_unsat() {
        let problem = problem(&[3], &[1, 2], 3);
        let bounded = bounded(&problem, 0);
        assert!(matches!(
            bounded,
            SolveResult::Incomplete(IncompleteResult {
                reason: IncompleteReason::ResourceLimit { .. },
                ..
            })
        ));

        let cancelled = solve(&problem, &SolveOptions::default(), &AtomicBool::new(true)).unwrap();
        let SolveResult::Incomplete(cancelled) = cancelled else {
            panic!("pre-cancelled solve must be incomplete");
        };
        assert_eq!(cancelled.reason, IncompleteReason::Cancelled);
        assert!(cancelled.best_known.is_none());
        assert_eq!(cancelled.proof.initial_node_lower_bound, 2);
        assert_eq!(cancelled.proof.node_counts_exhausted_through, Some(1));
    }

    #[test]
    fn source_grain_lower_bound_skips_directly_to_seven_nodes() {
        let problem = problem(&[216], &[66, 150], 1_200);
        let cancel = AtomicBool::new(false);
        let first_search_node = Mutex::new(None);
        let result = solve_with_observer(
            &problem,
            &SolveOptions {
                max_nodes: Some(7),
                worker_count: 1,
            },
            &cancel,
            &|event| {
                if let SolverEvent::Progress(SolverProgress {
                    phase: SolvePhase::Searching,
                    node_count: Some(node_count),
                    ..
                }) = event
                {
                    let mut first = first_search_node.lock().unwrap();
                    if first.is_none() {
                        *first = Some(node_count);
                        cancel.store(true, Ordering::Relaxed);
                    }
                }
            },
        )
        .unwrap();
        let SolveResult::Incomplete(incomplete) = result else {
            panic!("observer cancellation must stop the seven-node search");
        };
        assert_eq!(incomplete.reason, IncompleteReason::Cancelled);
        assert_eq!(incomplete.proof.initial_node_lower_bound, 7);
        assert_eq!(incomplete.proof.node_counts_exhausted_through, Some(6));
        assert_eq!(first_search_node.into_inner().unwrap(), Some(7));
    }

    #[test]
    fn cancellation_returns_promptly_once_search_workers_are_busy() {
        let problem = problem(&[216], &[66, 150], 1_200);
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_started = Arc::new(Mutex::new(None));
        let timer_cancel = Arc::clone(&cancel);
        let timer_started = Arc::clone(&cancel_started);
        thread::spawn(move || {
            // Let workers enter the hard seven-node search before cancelling.
            thread::sleep(std::time::Duration::from_millis(200));
            *timer_started.lock().unwrap() = Some(Instant::now());
            timer_cancel.store(true, Ordering::Relaxed);
        });
        let result = solve_with_observer(
            &problem,
            &SolveOptions {
                max_nodes: Some(7),
                worker_count: thread::available_parallelism().map_or(4, std::num::NonZero::get),
            },
            cancel.as_ref(),
            &|_| {},
        )
        .unwrap();
        let SolveResult::Incomplete(incomplete) = result else {
            panic!("busy-worker cancellation must stop the seven-node search");
        };
        assert_eq!(incomplete.reason, IncompleteReason::Cancelled);
        let cancel_started = cancel_started
            .lock()
            .unwrap()
            .expect("timer must arm cancellation");
        assert!(
            cancel_started.elapsed() < std::time::Duration::from_secs(3),
            "cancel lingered for {:?}",
            cancel_started.elapsed()
        );
    }

    #[test]
    fn cancellation_with_backlogged_roots_never_reports_worker_panic() {
        let problem = problem(&[216], &[66, 150], 1_200);
        let cancel = Arc::new(AtomicBool::new(false));
        let timer_cancel = Arc::clone(&cancel);
        thread::spawn(move || {
            thread::sleep(std::time::Duration::from_millis(50));
            timer_cancel.store(true, Ordering::Relaxed);
        });
        let result = solve(
            &problem,
            &SolveOptions {
                max_nodes: Some(7),
                // One worker leaves a deep backlog so cancel must fill unclaimed
                // roots as Cancelled rather than inventing worker panics.
                worker_count: 1,
            },
            cancel.as_ref(),
        );
        match result {
            Ok(SolveResult::Incomplete(incomplete)) => {
                assert_eq!(incomplete.reason, IncompleteReason::Cancelled);
            }
            Ok(other) => panic!("expected Cancelled incomplete, got {other:?}"),
            Err(error) => panic!("unclaimed cancelled roots must not panic the group: {error}"),
        }
    }

    #[test]
    #[ignore = "manual hard-case production benchmark"]
    fn benchmark_216_to_66_and_150_from_proven_seven_node_bound() {
        let problem = problem(&[216], &[66, 150], 1_200);
        let cancel = Arc::new(AtomicBool::new(false));
        let timer_cancel = Arc::clone(&cancel);
        thread::spawn(move || {
            thread::sleep(std::time::Duration::from_mins(1));
            timer_cancel.store(true, Ordering::Relaxed);
        });
        let last_progress = Mutex::new(None);
        let started = Instant::now();
        let result = solve_with_observer(
            &problem,
            &SolveOptions {
                max_nodes: Some(7),
                worker_count: 5,
            },
            cancel.as_ref(),
            &|event| {
                if let SolverEvent::Progress(progress) = event {
                    *last_progress.lock().unwrap() = Some(progress);
                }
            },
        )
        .unwrap();
        let outcome = match &result {
            SolveResult::Optimal(solution) => {
                format!("optimal=({}, {})", solution.node_count, solution.link_count)
            }
            SolveResult::Incomplete(incomplete) => format!("incomplete={:?}", incomplete.reason),
            SolveResult::GloballyUnsat(global) => format!("global_unsat={:?}", global.reason),
        };
        eprintln!(
            "hard-case elapsed={:?} {outcome} last_progress={:?}",
            started.elapsed(),
            last_progress.into_inner().unwrap()
        );
        match result {
            SolveResult::Optimal(solution) => {
                assert_eq!((solution.node_count, solution.link_count), (7, 11));
                assert_eq!(solution.proof.initial_node_lower_bound, 7);
                validate_solution(&problem, &solution.graph).unwrap();
            }
            SolveResult::Incomplete(incomplete) => {
                assert_eq!(incomplete.reason, IncompleteReason::Cancelled);
                assert_eq!(incomplete.proof.initial_node_lower_bound, 7);
            }
            SolveResult::GloballyUnsat(_) => panic!("balanced hard case cannot be globally UNSAT"),
        }
    }

    #[test]
    fn only_finite_global_checks_return_global_unsat() {
        let mismatch = bounded(&problem(&[1], &[2], 2), 0);
        assert!(matches!(
            mismatch,
            SolveResult::GloballyUnsat(GlobalUnsatProof {
                reason: GlobalUnsatReason::InsufficientInput { .. },
                ..
            })
        ));
        let capacity = bounded(&problem(&[2], &[2], 1), 0);
        assert!(matches!(
            capacity,
            SolveResult::GloballyUnsat(GlobalUnsatProof {
                reason: GlobalUnsatReason::ExternalRateExceedsCapacity { .. },
                ..
            })
        ));
    }

    #[test]
    fn worker_count_does_not_change_the_exact_result() {
        // Positive surplus puts S2 and S3 in the same N=1, L=2 obligation:
        // they differ only in anonymous discard ports. S2 is feasible here.
        let problem = problem(&[2], &[1], 2);
        let mut results = Vec::new();
        for worker_count in [1, 2, 4] {
            results.push(
                solve(
                    &problem,
                    &SolveOptions {
                        max_nodes: Some(1),
                        worker_count,
                    },
                    &AtomicBool::new(false),
                )
                .unwrap(),
            );
        }
        assert_eq!(results[0], results[1]);
        assert_eq!(results[0], results[2]);
        let SolveResult::Optimal(solution) = &results[0] else {
            panic!("expected a one-node surplus optimum");
        };
        assert_eq!((solution.node_count, solution.link_count), (1, 0));
    }

    #[test]
    fn progress_and_incumbents_are_emitted_only_from_the_deterministic_coordinator() {
        let problem = problem(&[6], &[6], 6);
        let events = Mutex::new(Vec::new());
        let result = solve_with_observer(
            &problem,
            &SolveOptions {
                max_nodes: Some(0),
                worker_count: 4,
            },
            &AtomicBool::new(false),
            &|event| events.lock().unwrap().push(event),
        )
        .unwrap();
        let SolveResult::Optimal(solution) = result else {
            panic!("expected direct optimum");
        };
        assert_eq!(solution.proof.profiles_exhausted, 1);
        assert_eq!(solution.proof.root_partitions_exhausted, 1);

        let events = events.into_inner().unwrap();
        let phases = events
            .iter()
            .filter_map(|event| match event {
                SolverEvent::Progress(progress) => Some(progress.phase),
                SolverEvent::Incumbent(_) | SolverEvent::SolutionFound(_) => None,
            })
            .collect::<Vec<_>>();
        for required in [
            SolvePhase::Normalizing,
            SolvePhase::GlobalChecks,
            SolvePhase::ComputingLowerBound,
            SolvePhase::Searching,
            SolvePhase::ValidatingWitness,
        ] {
            assert!(phases.contains(&required), "missing phase {required:?}");
        }
        assert!(events.iter().any(|event| {
            match event {
                SolverEvent::Progress(progress) => progress
                    .custom
                    .iter()
                    .any(|d| d.name == "custom.root_partition"),
                _ => false,
            }
        }));
        let incumbents = events
            .iter()
            .filter_map(|event| match event {
                SolverEvent::Incumbent(incumbent) => Some(incumbent),
                SolverEvent::Progress(_) | SolverEvent::SolutionFound(_) => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(incumbents.len(), 1);
        assert_eq!(
            incumbents[0].validation,
            validate_solution(&problem, &incumbents[0].graph).unwrap()
        );
    }

    #[test]
    fn active_cancellation_never_turns_partial_parallel_work_into_an_optimal_claim() {
        let problem = problem(&[2], &[1], 2);
        let cancel = AtomicBool::new(false);
        let result = solve_with_observer(
            &problem,
            &SolveOptions {
                max_nodes: Some(1),
                worker_count: 4,
            },
            &cancel,
            &|event| {
                if matches!(
                    event,
                    SolverEvent::Progress(SolverProgress {
                        phase: SolvePhase::Searching,
                        ..
                    })
                ) {
                    cancel.store(true, Ordering::Relaxed);
                }
            },
        )
        .unwrap();
        assert!(matches!(
            result,
            SolveResult::Incomplete(IncompleteResult {
                reason: IncompleteReason::Cancelled,
                ..
            })
        ));
    }

    #[test]
    fn cancellation_after_validated_incumbent_returns_nonoptimal_best_known_and_proof() {
        let problem = problem(&[6], &[6], 6);
        let cancel = AtomicBool::new(false);
        let emitted_incumbent = Mutex::new(None);
        let result = solve_with_observer(
            &problem,
            &SolveOptions {
                max_nodes: Some(0),
                worker_count: 4,
            },
            &cancel,
            &|event| {
                if let SolverEvent::Incumbent(candidate) = event {
                    assert_eq!(
                        candidate.validation,
                        validate_solution(&problem, &candidate.graph).unwrap()
                    );
                    assert!(
                        emitted_incumbent
                            .lock()
                            .unwrap()
                            .replace(candidate)
                            .is_none()
                    );
                    cancel.store(true, Ordering::Relaxed);
                }
            },
        )
        .unwrap();

        let SolveResult::Incomplete(incomplete) = result else {
            panic!("observer cancellation after an incumbent must not return Optimal");
        };
        assert_eq!(incomplete.reason, IncompleteReason::Cancelled);
        let best_known = incomplete
            .best_known
            .expect("the validated incumbent must survive as explicitly non-optimal best_known");
        assert_eq!(
            Some(best_known.clone()),
            emitted_incumbent.into_inner().unwrap()
        );
        assert_eq!(
            best_known.validation,
            validate_solution(&problem, &best_known.graph).unwrap()
        );
        assert_eq!(incomplete.proof.profiles_exhausted, 1);
        assert_eq!(incomplete.proof.root_partitions_exhausted, 1);
        assert_eq!(incomplete.proof.link_groups_exhausted, 0);
        assert_eq!(incomplete.proof.node_counts_exhausted_through, None);
    }

    #[test]
    fn outer_validator_rejects_a_corrupted_worker_witness() {
        let caller_problem = problem(&[6], &[6], 6);
        let Preparation::Prepared(normalized) = prepare_problem(&caller_problem).unwrap() else {
            panic!("balanced positive fixture must pass global preparation");
        };
        let profile = NodeProfile::default();
        let accounting = profile_link_accounting(
            profile,
            1,
            1,
            &normalized.surplus,
            &normalized.max_link_rate,
        )
        .unwrap()
        .expect("the zero-node direct-link profile must have exact accounting");
        let ProfileSearchResult::Exhausted {
            best_witness: Some(mut worker_witness),
            ..
        } = search_profile(&normalized, profile, &AtomicBool::new(false))
        else {
            panic!("the worker must produce a validated direct-link witness");
        };

        worker_witness.graph.links[0].flow = Rational::zero();
        let error =
            restore_and_validate(&caller_problem, &normalized, worker_witness, 0, accounting)
                .unwrap_err();
        assert!(matches!(error, SolverError::ValidationFirewall(_)));
    }

    #[test]
    fn zero_workers_is_rejected_before_any_proof_claim() {
        let error = solve(
            &problem(&[1], &[1], 1),
            &SolveOptions {
                max_nodes: Some(0),
                worker_count: 0,
            },
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert_eq!(error, SolverError::InvalidWorkerCount);
    }

    #[test]
    fn panicking_profile_worker_returns_internal_error() {
        // Direct-link optima never spawn root workers; use a one-node profile so
        // execute_root is reached and the injected panic is observed.
        let panic_next_root_worker = AtomicBool::new(true);
        let error = solve_internal(
            &problem(&[2], &[1, 1], 2),
            &SolveOptions {
                max_nodes: Some(1),
                worker_count: 2,
            },
            &AtomicBool::new(false),
            &|_| {},
            false,
            Some(&panic_next_root_worker),
        )
        .unwrap_err();
        assert_eq!(
            error,
            SolverError::ProfileWorkerPanicked { profile_index: 0 }
        );
    }

    #[test]
    fn observer_panics_are_advisory_and_do_not_change_the_solution() {
        let result = solve_with_observer(
            &problem(&[1], &[1], 1),
            &SolveOptions {
                max_nodes: Some(0),
                worker_count: 1,
            },
            &AtomicBool::new(false),
            &|_| panic!("injected observer failure"),
        )
        .unwrap();
        assert!(matches!(result, SolveResult::Optimal(_)));
    }

    fn ordered_sides(total: u32) -> Vec<Vec<u32>> {
        let mut sides = vec![vec![total]];
        sides.extend((1..total).map(|left| vec![left, total - left]));
        sides
    }

    fn assert_same_mathematical_outcome(
        problem: &Problem,
        production: &SolveResult,
        reference: &SolveResult,
    ) {
        match (production, reference) {
            (SolveResult::Optimal(left), SolveResult::Optimal(right)) => {
                assert_eq!(
                    (left.node_count, left.link_count),
                    (right.node_count, right.link_count)
                );
                assert_eq!(left.physical_link_count, right.physical_link_count);
                assert_eq!(left.discard_link_count, right.discard_link_count);
                assert_eq!(left.canonical_graph_key, right.canonical_graph_key);
                assert_eq!(left.graph, right.graph);
                assert_eq!(
                    left.validation,
                    validate_solution(problem, &left.graph).unwrap()
                );
                assert_eq!(
                    right.validation,
                    validate_solution(problem, &right.graph).unwrap()
                );
            }
            (SolveResult::Incomplete(left), SolveResult::Incomplete(right)) => {
                assert!(matches!(
                    left.reason,
                    IncompleteReason::ResourceLimit { .. }
                ));
                assert!(matches!(
                    right.reason,
                    IncompleteReason::ResourceLimit { .. }
                ));
            }
            (left, right) => panic!("production/reference mismatch: {left:?} versus {right:?}"),
        }
    }

    fn terminal_input_flow(graph: &PhysicalGraph, terminal: InputTerminalIndex) -> Rational {
        graph
            .links
            .iter()
            .find(|link| link.producer == ProducerPortRef::Input(terminal))
            .expect("validated input has one physical link")
            .flow
            .clone()
    }
}
