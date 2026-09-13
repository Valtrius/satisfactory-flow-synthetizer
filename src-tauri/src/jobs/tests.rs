use super::*;
use solver_api::{Diagnostic, LinkConstraint, SolvePhase};

fn progress(branch: Option<usize>, nodes: u32, links: u32) -> SolverProgress {
    SolverProgress {
        phase: SolvePhase::Searching,
        elapsed_ms: 0,
        node_count: Some(nodes),
        link_constraint: Some(LinkConstraint::Exact(links)),
        node_lower_bound: Some(nodes),
        best_node_count: None,
        best_link_count: None,
        solutions_found: 0,
        custom: branch
            .map(|branch| {
                Diagnostic::text(
                    "solver.portfolio_branch",
                    "Solver independent search",
                    branch.to_string(),
                )
            })
            .into_iter()
            .collect(),
    }
}

#[test]
fn interleaved_branches_do_not_alternate_displayed_nodes_or_links() {
    // Branch 1 reports first and keeps moving ahead of branch 0.
    let events = [
        progress(Some(1), 7, 3),
        progress(Some(0), 6, 2),
        progress(Some(1), 7, 3),
        progress(Some(0), 6, 3),
        progress(Some(1), 8, 2),
        progress(Some(0), 7, 2),
        progress(Some(1), 8, 3),
    ];
    let displayed: Vec<_> = events
        .into_iter()
        .filter(should_display_progress)
        .map(|progress| {
            (
                progress.node_count,
                progress.link_constraint,
                progress.node_lower_bound,
            )
        })
        .collect();
    assert_eq!(
        displayed,
        [
            (Some(6), Some(LinkConstraint::Exact(2)), Some(6)),
            (Some(6), Some(LinkConstraint::Exact(3)), Some(6)),
            (Some(7), Some(LinkConstraint::Exact(2)), Some(7)),
        ]
    );
}

#[test]
fn final_progress_switches_to_either_returned_proof_owner() {
    for owner in [0, 1] {
        let mut final_progress = progress(Some(owner), 7, 3);
        final_progress.custom.push(Diagnostic::text(
            "solver.portfolio_proof_owner",
            "Solver returned proof owner",
            owner.to_string(),
        ));
        let events = [
            progress(Some(0), 6, 2),
            progress(Some(1), 7, 3),
            final_progress.clone(),
        ];
        let displayed: Vec<_> = events.into_iter().filter(should_display_progress).collect();
        assert_eq!(displayed, [progress(Some(0), 6, 2), final_progress]);
    }
}

#[test]
fn single_worker_progress_does_not_need_a_portfolio_branch() {
    assert!(should_display_progress(&progress(None, 6, 2)));
}
