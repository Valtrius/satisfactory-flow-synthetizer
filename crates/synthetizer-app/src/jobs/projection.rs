use super::{JobSnapshot, JobStatus};
use crate::{
    presentation::{PresentationError, PresentedSolveOutcome, present_solve_result},
    solution::Solution,
};
use solver_api::{
    EnumerationStatus, IncompleteReason, PreparedProblem, SolveMode, SolveOutcome, SolveResult,
};

/// Apply a cancellation accepted by the host before its completion seal.
#[must_use]
pub fn cancel_before_presentation(
    outcome: SolveOutcome,
    mode: SolveMode,
    cancelled: bool,
) -> SolveOutcome {
    if !cancelled || matches!(outcome.result, SolveResult::Incomplete(_)) {
        return outcome;
    }
    let (best_known, proof) = match outcome.result {
        SolveResult::Optimal(s) => (
            Some(solver_api::BestKnownSolution {
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
        SolveResult::Incomplete(_) => unreachable!(),
    };
    SolveOutcome::new(
        SolveResult::Incomplete(solver_api::IncompleteResult {
            reason: IncompleteReason::Cancelled,
            best_known,
            proof,
        }),
        mode,
        outcome.solutions,
    )
}

/// Project a sealed solver outcome without changing the host sequence or emitting events.
/// Canonical keys must be the normalized identities supplied by the solver API.
/// # Errors
/// Returns inconsistent witness metadata or malformed graph references. The snapshot is unchanged on error.
pub fn project_outcome<Id>(
    snapshot: &mut JobSnapshot<Id>,
    mode: SolveMode,
    prepared: &PreparedProblem,
    outcome: &SolveOutcome,
) -> Result<(), PresentationError> {
    let presented = present_solve_result(prepared, &outcome.result)?;
    let solutions = terminal_solutions(snapshot, prepared, outcome)?;
    apply_terminal(snapshot, mode, outcome, presented, solutions);
    Ok(())
}

/// Pre-project a hard interruption while the compute worker is still available.
/// The host retains the live graphs and applies this thin packet only after it
/// retires the worker. It must not apply it after accepting a terminal snapshot.
/// This keeps proof and status rules in Rust even if the worker traps or blocks.
#[must_use]
pub fn interruption_packet<Id: Clone>(
    snapshot: &JobSnapshot<Id>,
    mode: SolveMode,
    best: Option<&solver_api::BestKnownSolution>,
    proof: &solver_api::ProofSummary,
    reason: IncompleteReason,
) -> Option<JobSnapshot<Id>> {
    if !matches!(snapshot.status, JobStatus::Running | JobStatus::Cancelling) {
        return None;
    }
    let outcome = SolveOutcome::new(
        SolveResult::Incomplete(solver_api::IncompleteResult {
            reason: reason.clone(),
            best_known: best.cloned(),
            proof: proof.clone(),
        }),
        mode,
        Vec::new(),
    );
    let mut packet = snapshot.thin_packet();
    packet.sequence += 1;
    packet.results_omitted = true;
    packet.enumeration_complete = false;
    packet.proof = Some(outcome.proof);
    packet.unsat = None;
    apply_incomplete_status(&mut packet, reason);
    Some(packet)
}

fn apply_incomplete_status<Id>(snapshot: &mut JobSnapshot<Id>, reason: IncompleteReason) {
    snapshot.error = None;
    snapshot.status = match reason {
        IncompleteReason::Cancelled => JobStatus::Cancelled,
        IncompleteReason::WorkerFailed { detail } => {
            snapshot.error = Some(detail);
            JobStatus::Failed
        }
        IncompleteReason::DeadlineReached => {
            snapshot.error = Some("Solve deadline reached.".to_owned());
            JobStatus::Incomplete
        }
        IncompleteReason::ResourceLimit { detail } => {
            snapshot.error = Some(detail);
            JobStatus::Incomplete
        }
    };
}

pub(super) fn terminal_solutions<Id>(
    snapshot: &JobSnapshot<Id>,
    prepared: &PreparedProblem,
    outcome: &SolveOutcome,
) -> Result<Vec<Solution>, PresentationError> {
    // Search has sealed, so no further live results can change this cache.
    let published = snapshot
        .results
        .iter()
        .filter(|solution| {
            solution.display.status == "best_known" && solution.display.proof.is_none()
        })
        .map(|solution| (&solution.layout_key, solution))
        .collect::<std::collections::BTreeMap<_, _>>();
    outcome
        .solutions
        .iter()
        .map(|best| {
            if let Some(solution) = published.get(&best.canonical_graph_key) {
                return Ok((*solution).clone());
            }
            Solution::from_best(prepared, best)
        })
        .collect()
}

fn apply_terminal<Id>(
    snapshot: &mut JobSnapshot<Id>,
    mode: SolveMode,
    outcome: &SolveOutcome,
    presented: PresentedSolveOutcome,
    mut solutions: Vec<Solution>,
) {
    let cancelled = snapshot.status == JobStatus::Cancelling;
    let complete = matches!(
        outcome.enumeration,
        EnumerationStatus::AllMinN { complete: true, .. }
            | EnumerationStatus::AllMinNL { complete: true, .. }
    );
    // Result indices also identify saved graph edits. Keep the live append order
    // when the mathematical API returns its deterministically ordered collection.
    let published = snapshot
        .results
        .iter()
        .enumerate()
        .map(|(index, solution)| (&solution.layout_key, index))
        .collect::<std::collections::BTreeMap<_, _>>();
    solutions.sort_by_key(|solution| {
        published
            .get(&solution.layout_key)
            .copied()
            .unwrap_or(usize::MAX)
    });
    snapshot.results = solutions;
    snapshot.enumeration_complete = complete && !cancelled;
    snapshot.error = None;
    snapshot.unsat = None;
    snapshot.proof = Some(outcome.proof);
    match presented {
        PresentedSolveOutcome::Optimal(display) => {
            let SolveResult::Optimal(exact) = &outcome.result else {
                unreachable!()
            };
            let solution = Solution {
                display,
                layout_key: exact.canonical_graph_key.clone(),
            };
            if let Some(existing) = snapshot
                .results
                .iter_mut()
                .find(|s| s.has_same_layout(&solution))
            {
                *existing = solution.clone();
            }
            snapshot.result = Some(solution);
            snapshot.status = if cancelled {
                JobStatus::Cancelled
            } else {
                JobStatus::Completed
            };
        }
        PresentedSolveOutcome::GloballyUnsat(proof) => {
            snapshot.result = None;
            snapshot.status = JobStatus::Unsat;
            snapshot.unsat = Some(proof);
            snapshot.error = Some("No exact network exists for these rates.".to_owned());
        }
        PresentedSolveOutcome::Incomplete(incomplete) => {
            apply_incomplete_status(snapshot, incomplete.reason);
            if (mode == SolveMode::OneMinNL || snapshot.result.is_none())
                && let (Some(display), SolveResult::Incomplete(exact)) =
                    (incomplete.best_known, &outcome.result)
                && let Some(best) = &exact.best_known
            {
                snapshot.result = Some(Solution {
                    display,
                    layout_key: best.canonical_graph_key.clone(),
                });
            }
        }
    }
}
