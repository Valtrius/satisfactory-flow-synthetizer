use crate::{portfolio, process};
use solver_api::{
    BestKnownSolution, IncompleteReason, IncompleteResult, Problem, RunOptions, SolveMode,
    SolveObserver, SolveOutcome, SolveResult, SolverError,
};
use solver_validation::SolutionCollector;
use std::sync::atomic::{AtomicBool, Ordering};

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
