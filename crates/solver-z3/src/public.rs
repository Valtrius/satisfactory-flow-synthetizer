use crate::{
    model::SolverProgress as NativeProgress,
    solver::{SolveTermination, SolverEvent as NativeEvent},
    verify::from_problem,
};
use solver_api::{
    BestKnownSolution, Diagnostic, IncompleteReason, IncompleteResult, LinkConstraint,
    OptimalSolution, Problem, ProofSummary, RunOptions, SolveMode, SolveObserver, SolveOutcome,
    SolvePhase, SolveResult, SolverError, SolverEvent, SolverProgress,
};
use solver_validation::SolutionCollector;
use std::{sync::atomic::AtomicBool, time::Instant};

/// Runs Z3 against the same exact problem and result contract as the other solvers.
///
/// # Errors
/// Returns malformed input, invalid options, or an internal search/validation failure.
pub fn solve_problem(
    problem: &Problem,
    options: &RunOptions,
    cancel: &AtomicBool,
    observer: &dyn SolveObserver,
) -> Result<SolveOutcome, SolverError> {
    problem.validate()?;
    if options.worker_count == 0 {
        return Err(SolverError::InvalidOptions(
            "worker count must be positive".to_owned(),
        ));
    }
    if let Some(proof) = problem.global_contradiction() {
        return Ok(SolveOutcome::new(
            SolveResult::GloballyUnsat(proof),
            options.mode,
            Vec::new(),
        ));
    }
    let mut native = from_problem(problem);
    native.worker_count = options.worker_count;
    let collector = SolutionCollector::new(problem, observer);
    let started = Instant::now();
    let mut proof = ProofSummary::default();
    let terminal = crate::solver::search(
        &native,
        options.mode == SolveMode::AllAtMinimumNodes,
        options.max_nodes,
        cancel,
        |event| match event {
            NativeEvent::Progress(progress) => {
                let progress = common_progress(progress, options.mode, started);
                if let Some(lower) = progress.node_lower_bound {
                    proof.initial_node_lower_bound = proof.initial_node_lower_bound.max(lower);
                    proof.node_counts_exhausted_through = lower.checked_sub(1);
                }
                collector.on_event(SolverEvent::Progress(progress));
            }
            NativeEvent::Incumbent(solution) => {
                collector.on_event(SolverEvent::Incumbent(*solution));
            }
            NativeEvent::SolutionFound(solution) => {
                collector.on_event(SolverEvent::SolutionFound(*solution));
            }
        },
    )
    .map_err(|error| SolverError::Internal(error.to_string()))?;
    let result = match terminal {
        SolveTermination::Completed(solution) => SolveResult::Optimal(optimal(*solution, proof)),
        SolveTermination::Enumerated(solutions) => {
            let best = best(solutions).ok_or_else(|| {
                SolverError::Internal("completed enumeration has no witness".to_owned())
            })?;
            SolveResult::Optimal(optimal(best, proof))
        }
        SolveTermination::Cancelled { solutions } => SolveResult::Incomplete(IncompleteResult {
            reason: IncompleteReason::Cancelled,
            best_known: best(solutions),
            proof,
        }),
        SolveTermination::Incomplete { solutions, error } => {
            SolveResult::Incomplete(IncompleteResult {
                reason: IncompleteReason::WorkerFailed {
                    detail: error.to_string(),
                },
                best_known: best(solutions),
                proof,
            })
        }
        SolveTermination::Limited { exhausted_through } => {
            proof.node_counts_exhausted_through = exhausted_through;
            SolveResult::Incomplete(IncompleteResult {
                reason: IncompleteReason::ResourceLimit {
                    detail: "configured node limit reached".to_owned(),
                },
                best_known: None,
                proof,
            })
        }
    };
    Ok(collector.finish(result, options.mode))
}

fn best(solutions: Vec<crate::model::Solution>) -> Option<BestKnownSolution> {
    solutions.into_iter().min_by(|a, b| {
        (a.node_count, a.link_count, &a.canonical_graph_key).cmp(&(
            b.node_count,
            b.link_count,
            &b.canonical_graph_key,
        ))
    })
}

fn optimal(best: BestKnownSolution, proof: ProofSummary) -> OptimalSolution {
    OptimalSolution {
        node_count: best.node_count,
        link_count: best.link_count,
        physical_link_count: best.physical_link_count,
        discard_link_count: best.discard_link_count,
        canonical_graph_key: best.canonical_graph_key,
        graph: best.graph,
        validation: best.validation,
        proof,
    }
}

fn common_progress(native: NativeProgress, mode: SolveMode, started: Instant) -> SolverProgress {
    let mut result = SolverProgress {
        phase: SolvePhase::Searching,
        elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        node_count: None,
        link_constraint: None,
        node_lower_bound: None,
        best_node_count: None,
        best_link_count: None,
        solutions_found: 0,
        custom: Vec::new(),
    };
    let count = |n| u32::try_from(n).unwrap_or(u32::MAX);
    match native {
        NativeProgress::Preparing { lower_bound } => {
            result.phase = SolvePhase::ComputingLowerBound;
            result.node_lower_bound = Some(count(lower_bound));
        }
        NativeProgress::Checking {
            node_count,
            rejected_unstable_candidates,
            profile_count,
            attempt_slots,
            threads_per_attempt,
            max_operator_belts,
            incumbent_belt_count,
            ..
        } => {
            result.node_count = Some(count(node_count));
            result.link_constraint = max_operator_belts.map(|l| LinkConstraint::AtMost(count(l)));
            result.best_link_count = incumbent_belt_count.map(count);
            result.custom = vec![
                Diagnostic::counter("z3.profiles_total", "Profiles at size", profile_count),
                Diagnostic::counter("z3.attempt_slots", "Attempt slots", attempt_slots),
                Diagnostic::counter(
                    "z3.threads_per_attempt",
                    "Threads per attempt",
                    threads_per_attempt,
                ),
                Diagnostic::counter(
                    "z3.rejected",
                    "Unstable rejects",
                    rejected_unstable_candidates,
                ),
            ];
        }
        NativeProgress::SizeProgress {
            node_count,
            max_operator_belts,
            incumbent_belt_count,
            ..
        } => {
            result.node_count = Some(count(node_count));
            result.link_constraint = max_operator_belts.map(|l| LinkConstraint::AtMost(count(l)));
            result.best_link_count = incumbent_belt_count.map(count);
            result.custom = portfolio_diagnostics(&native);
        }
        NativeProgress::CandidateRejected {
            node_count,
            rejected_unstable_candidates,
            reason,
            ..
        } => {
            result.node_count = Some(count(node_count));
            result.custom = vec![
                Diagnostic::counter(
                    "z3.rejected",
                    "Unstable rejects",
                    rejected_unstable_candidates,
                ),
                Diagnostic::text("z3.rejection_reason", "Rejection reason", reason),
            ];
        }
    }
    if let Some(n) = result.node_count {
        result.node_lower_bound = Some(n);
    }
    if result.link_constraint.is_some() {
        result.phase = SolvePhase::OptimizingLinks;
    } else if mode == SolveMode::AllAtMinimumNodes && result.node_count.is_some() {
        result.phase = SolvePhase::Enumerating;
    }
    result
}

fn portfolio_diagnostics(native: &NativeProgress) -> Vec<Diagnostic> {
    let NativeProgress::SizeProgress {
        profiles_total,
        profiles_unresolved,
        profiles_unsat,
        active_attempts,
        launched_attempts,
        abandoned_attempts,
        rejected_unstable_candidates,
        attempt_slots,
        ..
    } = native
    else {
        return Vec::new();
    };
    vec![
        Diagnostic::counter("z3.profiles_total", "Profiles at size", profiles_total),
        Diagnostic::counter(
            "z3.profiles_unresolved",
            "Unresolved profiles",
            profiles_unresolved,
        ),
        Diagnostic::counter("z3.profiles_unsat", "Profiles proved UNSAT", profiles_unsat),
        Diagnostic::counter("z3.active_attempts", "Active attempts", active_attempts),
        Diagnostic::counter(
            "z3.launched_attempts",
            "Attempts launched",
            launched_attempts,
        ),
        Diagnostic::counter(
            "z3.abandoned_attempts",
            "Abandoned unknowns",
            abandoned_attempts,
        ),
        Diagnostic::counter(
            "z3.rejected",
            "Unstable rejects",
            rejected_unstable_candidates,
        ),
        Diagnostic::counter("z3.attempt_slots", "Attempt slots", attempt_slots),
    ]
}
