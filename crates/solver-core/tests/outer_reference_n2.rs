//! Exhaustive outer-solver differential coverage through two physical nodes.
//!
//! The reference solver deliberately shares none of the production search,
//! propagation, SCC, memoization, component, lower-bound, or pruning machinery.
//! Comparing complete public outcomes here therefore covers the outer
//! lexicographic driver as well as every fixed profile it must exhaust before
//! returning.  The matrix is finite and exhaustive over all positive ordered
//! one- and two-terminal integer partitions in the stated range.

use std::{collections::BTreeSet, sync::atomic::AtomicBool};

use solver_api::{
    GlobalUnsatReason, IncompleteReason, IncompleteResult, OptimalSolution, Problem, Rational,
    SolveResult,
};
use solver_core::{SolveOptions, solve};
use solver_reference::{ReferenceOptions, solve_reference};
use solver_validation::validate_solution;

const MAX_NODES: u32 = 2;

#[derive(Default)]
struct Coverage {
    optimal_at_zero: usize,
    optimal_at_one: usize,
    optimal_at_two: usize,
    exhausted_through_two: usize,
    global_capacity_rejections: usize,
}

#[test]
fn outer_production_matches_reference_on_exhaustive_small_n2_matrix() {
    let mut coverage = Coverage::default();
    let mut case_count = 0_usize;

    // For each positive ordered rate partition through total three, exercise a
    // capacity just below the largest external rate (the finite global
    // contradiction), the exact boundary rate, and a loose capacity. BTreeSet
    // removes coincident values.
    for total_input in 1..=3 {
        for total_output in 1..=total_input {
            for inputs in ordered_sides(total_input) {
                for outputs in ordered_sides(total_output) {
                    let maximum_external = inputs
                        .iter()
                        .chain(&outputs)
                        .copied()
                        .max()
                        .expect("both terminal sides are non-empty");
                    let capacities = [
                        maximum_external.saturating_sub(1),
                        maximum_external,
                        total_input,
                    ]
                    .into_iter()
                    .filter(|capacity| *capacity > 0)
                    .collect::<BTreeSet<_>>();

                    for capacity in capacities {
                        run_case(&inputs, &outputs, capacity, &mut coverage);
                        case_count += 1;
                    }
                }
            }
        }
    }

    // Four -> three is the smallest one-by-one surplus case in this family
    // that has no witness through N=2. It forces both solvers to exhaust every
    // feasible profile and equal-link group at N=0,1,2 instead of stopping at
    // an earlier incumbent.
    run_case(&[4], &[3], 4, &mut coverage);
    case_count += 1;

    assert_eq!(case_count, 60, "matrix case manifest changed unexpectedly");
    assert!(coverage.optimal_at_zero > 0, "matrix missed direct optima");
    assert!(coverage.optimal_at_one > 0, "matrix missed one-node optima");
    assert!(coverage.optimal_at_two > 0, "matrix missed two-node optima");
    assert!(
        coverage.exhausted_through_two > 0,
        "matrix missed bounded UNSAT-through-N=2 outcomes"
    );
    assert!(
        coverage.global_capacity_rejections > 0,
        "matrix missed exact external-capacity contradictions"
    );
}

fn run_case(inputs: &[u32], outputs: &[u32], capacity: u32, coverage: &mut Coverage) {
    let problem = integer_problem(inputs, outputs, capacity);
    let label = format!("inputs={inputs:?}, outputs={outputs:?}, capacity={capacity}");
    let production = solve(
        &problem,
        &SolveOptions {
            max_nodes: Some(MAX_NODES),
            worker_count: 1,
        },
        &AtomicBool::new(false),
    )
    .unwrap_or_else(|error| panic!("production failed for {label}: {error}"));
    let reference = solve_reference(
        &problem,
        &ReferenceOptions {
            max_nodes: MAX_NODES,
        },
        &AtomicBool::new(false),
    )
    .unwrap_or_else(|error| panic!("reference failed for {label}: {error}"));

    assert_same_public_outcome(&problem, &production, &reference, &label, coverage);
}

fn assert_same_public_outcome(
    problem: &Problem,
    production: &SolveResult,
    reference: &SolveResult,
    label: &str,
    coverage: &mut Coverage,
) {
    match (production, reference) {
        (SolveResult::Optimal(left), SolveResult::Optimal(right)) => {
            assert_same_optimal(problem, left, right, label);
            match left.node_count {
                0 => coverage.optimal_at_zero += 1,
                1 => coverage.optimal_at_one += 1,
                2 => coverage.optimal_at_two += 1,
                unexpected => panic!("out-of-bound optimum N={unexpected} for {label}"),
            }
        }
        (SolveResult::Incomplete(left), SolveResult::Incomplete(right)) => {
            assert_same_bounded_exhaustion(left, right, label);
            coverage.exhausted_through_two += 1;
        }
        (SolveResult::GloballyUnsat(left), SolveResult::GloballyUnsat(right)) => {
            assert_eq!(
                left.reason, right.reason,
                "global proof differs for {label}"
            );
            if matches!(
                left.reason,
                GlobalUnsatReason::ExternalRateExceedsCapacity { .. }
            ) {
                coverage.global_capacity_rejections += 1;
            }
        }
        (left, right) => {
            panic!("production/reference outcome differs for {label}: {left:?} versus {right:?}")
        }
    }
}

fn assert_same_optimal(
    problem: &Problem,
    production: &OptimalSolution,
    reference: &OptimalSolution,
    label: &str,
) {
    assert_eq!(
        (production.node_count, production.link_count),
        (reference.node_count, reference.link_count),
        "lexicographic optimum differs for {label}"
    );
    assert_eq!(
        production.physical_link_count, reference.physical_link_count,
        "physical-link count differs for {label}"
    );
    assert_eq!(
        production.discard_link_count, reference.discard_link_count,
        "discard-link count differs for {label}"
    );
    assert_eq!(
        production.canonical_graph_key, reference.canonical_graph_key,
        "canonical winner differs for {label}"
    );
    // PhysicalGraph equality includes every exact Rational flow and endpoint,
    // so this is stronger than comparing only the key/cost.
    assert_eq!(
        production.graph, reference.graph,
        "exact witness differs for {label}"
    );
    assert_eq!(
        production.validation,
        validate_solution(problem, &production.graph)
            .unwrap_or_else(|error| panic!("production witness invalid for {label}: {error}"))
    );
    assert_eq!(
        reference.validation,
        validate_solution(problem, &reference.graph)
            .unwrap_or_else(|error| panic!("reference witness invalid for {label}: {error}"))
    );
    for link in &production.graph.links {
        assert!(link.flow.is_positive(), "non-positive link for {label}");
        assert!(
            link.flow <= problem.max_link_rate,
            "capacity violation for {label}: {} > {}",
            link.flow,
            problem.max_link_rate
        );
    }
}

fn assert_same_bounded_exhaustion(
    production: &IncompleteResult,
    reference: &IncompleteResult,
    label: &str,
) {
    assert!(
        matches!(production.reason, IncompleteReason::ResourceLimit { .. }),
        "production did not report bounded exhaustion for {label}: {:?}",
        production.reason
    );
    assert!(
        matches!(reference.reason, IncompleteReason::ResourceLimit { .. }),
        "reference did not report bounded exhaustion for {label}: {:?}",
        reference.reason
    );
    assert!(
        production.best_known.is_none(),
        "unexpected production incumbent for {label}"
    );
    assert!(
        reference.best_known.is_none(),
        "unexpected reference incumbent for {label}"
    );
    assert_eq!(
        production.proof.node_counts_exhausted_through,
        Some(MAX_NODES),
        "production did not prove UNSAT through N=2 for {label}"
    );
    assert_eq!(
        reference.proof.node_counts_exhausted_through,
        Some(MAX_NODES),
        "reference did not prove UNSAT through N=2 for {label}"
    );
    // A proven starting lower bound may discharge the whole N<=2 range without
    // enumerating the link groups and profiles that the deliberately simple
    // reference oracle visits. Those work counters need not agree; the exact
    // exhausted-through certificate and absence of an incumbent must agree.
}

fn integer_problem(inputs: &[u32], outputs: &[u32], capacity: u32) -> Problem {
    Problem {
        inputs: inputs.iter().copied().map(Rational::from).collect(),
        outputs: outputs.iter().copied().map(Rational::from).collect(),
        max_link_rate: Rational::from(capacity),
    }
}

fn ordered_sides(total: u32) -> Vec<Vec<u32>> {
    let mut sides = vec![vec![total]];
    sides.extend((1..total).map(|left| vec![left, total - left]));
    sides
}
