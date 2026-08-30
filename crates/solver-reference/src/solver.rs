use std::{
    collections::{BTreeMap, BTreeSet},
    ops::ControlFlow,
    sync::atomic::{AtomicBool, Ordering},
};

use solver_api::{
    BestKnownSolution, ConsumerPortRef, ExternalTerminal, GlobalUnsatProof, GlobalUnsatReason,
    IncompleteReason, IncompleteResult, InputTerminalIndex, OptimalSolution, OutputTerminalIndex,
    PhysicalGraph, PhysicalLink, Problem, ProducerPortRef, ProofSummary, Rational, SolveResult,
};
use solver_validation::{ValidationError, solve_topology, validate_solution};
use thiserror::Error;

use crate::{
    ProfilePlan, ProfileTopology, canonicalize_graph, enumerate_profile_plans,
    enumerate_topologies_with_discard,
};

const REFERENCE_PROOF_VERSION: u32 = 2;

/// Finite bounds for the deliberately exhaustive reference oracle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReferenceOptions {
    /// Largest physical node count that may be enumerated.
    pub max_nodes: u32,
}

impl Default for ReferenceOptions {
    fn default() -> Self {
        Self { max_nodes: 4 }
    }
}

/// Invalid input or an internal failure that prevents the reference proof from being trusted.
#[derive(Debug, Error)]
pub enum ReferenceError {
    #[error("invalid reference problem: {0}")]
    InvalidProblem(String),
    #[error("reference profile/link accounting overflowed u32")]
    CountOverflow,
    #[error(
        "validated graph count mismatch: expected ({expected_nodes} nodes, {expected_links} modeled, {expected_physical_links} physical, {expected_discard_links} discard), got ({actual_nodes} nodes, {actual_links} modeled, {actual_physical_links} physical, {actual_discard_links} discard)"
    )]
    ValidationCountMismatch {
        expected_nodes: u32,
        expected_links: u32,
        expected_physical_links: u32,
        expected_discard_links: u32,
        actual_nodes: u32,
        actual_links: u32,
        actual_physical_links: u32,
        actual_discard_links: u32,
    },
    #[error("canonical terminal mapping is inconsistent with the concrete graph")]
    CanonicalTerminalMapping,
    #[error("reference topology enumeration produced a malformed graph: {0}")]
    InvalidEnumeratedTopology(Box<ValidationError>),
    #[error("an independently solved reference witness failed the validation firewall: {0}")]
    ValidationFirewall(Box<ValidationError>),
}

/// Exhaustively solve a small problem for the minimum physical operator count.
///
/// This oracle deliberately uses no production lower bounds, partial-state cache,
/// pruning, or propagation. At a fixed equal-link group it
/// enumerates every complete physical topology, deduplicates only completed graph
/// isomorphs, and finishes the whole group before choosing its smallest canonical
/// validated witness.
///
/// # Errors
///
/// Returns [`ReferenceError::InvalidProblem`] for malformed rates or missing terminal
/// sides. A malformed enumerated topology or a failure of the independent validation
/// firewall is an internal error rather than evidence that a candidate is impossible.
#[allow(clippy::too_many_lines)]
pub fn solve_reference(
    problem: &Problem,
    options: &ReferenceOptions,
    cancel: &AtomicBool,
) -> Result<SolveResult, ReferenceError> {
    solve_reference_internal(problem, *options, cancel, false, false, &mut Vec::new())
}

/// Shared Problem/Solution entry point. Reference intentionally has no progress API.
///
/// # Errors
/// Returns malformed input or a failure of the independent oracle.
pub fn solve_problem(
    problem: &Problem,
    options: &solver_api::RunOptions,
    cancel: &AtomicBool,
) -> Result<solver_api::SolveOutcome, solver_api::SolverError> {
    let mut solutions = Vec::new();
    let native = ReferenceOptions {
        max_nodes: options.max_nodes.unwrap_or(4),
    };
    let result = solve_reference_internal(
        problem,
        native,
        cancel,
        options.mode != solver_api::SolveMode::Optimal,
        options.mode == solver_api::SolveMode::AllAtMinimumNodes,
        &mut solutions,
    )
    .map_err(|error| match error {
        ReferenceError::InvalidProblem(_) => {
            solver_api::SolverError::InvalidProblem(error.to_string())
        }
        _ => solver_api::SolverError::Internal(error.to_string()),
    })?;
    let mut outcome = solver_api::SolveOutcome::new(result, options.mode, solutions);
    // This only encodes the public return value. Reference search and deduplication
    // above remain independent of the production canonicalizer.
    solver_validation::normalize_outcome_identity(problem, &mut outcome);
    Ok(outcome)
}

#[allow(clippy::too_many_lines)]
fn solve_reference_internal(
    problem: &Problem,
    options: ReferenceOptions,
    cancel: &AtomicBool,
    enumerate: bool,
    enumerate_all_link_counts: bool,
    solutions: &mut Vec<BestKnownSolution>,
) -> Result<SolveResult, ReferenceError> {
    validate_problem(problem)?;
    let mut proof = empty_proof();

    let total_input = problem.total_input();
    let total_output = problem.total_output();
    if total_input < total_output {
        return Ok(SolveResult::GloballyUnsat(GlobalUnsatProof {
            reason: GlobalUnsatReason::InsufficientInput {
                total_input,
                total_output,
            },
            proof,
        }));
    }
    if let Some(reason) = external_capacity_contradiction(problem) {
        return Ok(SolveResult::GloballyUnsat(GlobalUnsatProof {
            reason,
            proof,
        }));
    }
    let canonical_problem = CanonicalProblem::new(problem)?;

    let input_count = u32::try_from(canonical_problem.problem.inputs.len())
        .map_err(|_| ReferenceError::InvalidProblem("too many inputs".to_owned()))?;
    let output_count = u32::try_from(canonical_problem.problem.outputs.len())
        .map_err(|_| ReferenceError::InvalidProblem("too many outputs".to_owned()))?;
    let surplus =
        &canonical_problem.problem.total_input() - &canonical_problem.problem.total_output();
    let mut best_known = None;
    let mut preferred = None;
    let mut layouts = BTreeSet::new();

    for node_count in 0..=options.max_nodes {
        if cancel.load(Ordering::Relaxed) {
            return Ok(incomplete(IncompleteReason::Cancelled, best_known, proof));
        }

        let profiles = enumerate_profile_plans(
            node_count,
            input_count,
            output_count,
            &surplus,
            &canonical_problem.problem.max_link_rate,
        );
        let mut link_groups = BTreeMap::<u32, Vec<_>>::new();
        for plan in profiles {
            link_groups.entry(plan.link_count).or_default().push(plan);
        }

        for (_link_count, profiles) in link_groups {
            if cancel.load(Ordering::Relaxed) {
                return Ok(incomplete(IncompleteReason::Cancelled, best_known, proof));
            }

            let mut canonical_topologies = BTreeSet::new();
            let mut group_best = None;
            for plan in profiles {
                let mut interrupted = false;
                let mut profile_error = None;
                let enumeration = enumerate_topologies_with_discard(
                    plan.profile,
                    input_count,
                    output_count,
                    plan.discard_link_count,
                    |topology| {
                        if cancel.load(Ordering::Relaxed) {
                            interrupted = true;
                            return ControlFlow::Break(());
                        }
                        match evaluate_topology(
                            &canonical_problem,
                            problem,
                            topology,
                            plan,
                            &mut canonical_topologies,
                        ) {
                            Ok(Some(candidate)) => {
                                if enumerate {
                                    let mut topology = candidate.graph.clone();
                                    for link in &mut topology.links {
                                        link.flow = Rational::zero();
                                    }
                                    if layouts.insert(canonicalize_graph(problem, &topology).key) {
                                        solutions.push(candidate.clone());
                                    }
                                }
                                retain_smallest(&mut group_best, candidate.clone());
                                retain_smallest(&mut best_known, candidate);
                                ControlFlow::Continue(())
                            }
                            Ok(None) => ControlFlow::Continue(()),
                            Err(error) => {
                                profile_error = Some(error);
                                ControlFlow::Break(())
                            }
                        }
                    },
                );

                if let Some(error) = profile_error {
                    return Err(error);
                }
                if interrupted || matches!(enumeration, ControlFlow::Break(())) {
                    return Ok(incomplete(IncompleteReason::Cancelled, best_known, proof));
                }

                proof.profiles_exhausted = proof
                    .profiles_exhausted
                    .checked_add(1)
                    .ok_or(ReferenceError::CountOverflow)?;
                // The simple reference solver has one exhaustive root partition per profile.
                proof.root_partitions_exhausted = proof
                    .root_partitions_exhausted
                    .checked_add(1)
                    .ok_or(ReferenceError::CountOverflow)?;
            }

            proof.link_groups_exhausted = proof
                .link_groups_exhausted
                .checked_add(1)
                .ok_or(ReferenceError::CountOverflow)?;
            if let Some(best) = group_best {
                let solution = OptimalSolution {
                    node_count: best.node_count,
                    link_count: best.link_count,
                    physical_link_count: best.physical_link_count,
                    discard_link_count: best.discard_link_count,
                    canonical_graph_key: best.canonical_graph_key,
                    graph: best.graph,
                    proof,
                    validation: best.validation,
                };
                if !enumerate_all_link_counts {
                    return Ok(SolveResult::Optimal(solution));
                }
                proof = solution.proof.clone();
                if preferred.is_none() {
                    preferred = Some(solution);
                }
            }
        }

        if let Some(mut solution) = preferred.take() {
            solution.proof = proof;
            return Ok(SolveResult::Optimal(solution));
        }
        proof.node_counts_exhausted_through = Some(node_count);
    }

    Ok(incomplete(
        IncompleteReason::ResourceLimit {
            detail: format!(
                "reference solver exhausted every topology through {} physical nodes",
                options.max_nodes
            ),
        },
        best_known,
        proof,
    ))
}

fn validate_problem(problem: &Problem) -> Result<(), ReferenceError> {
    if problem.inputs.is_empty() {
        return Err(ReferenceError::InvalidProblem(
            "at least one input is required".to_owned(),
        ));
    }
    if problem.outputs.is_empty() {
        return Err(ReferenceError::InvalidProblem(
            "at least one output is required".to_owned(),
        ));
    }
    if !problem.max_link_rate.is_positive() {
        return Err(ReferenceError::InvalidProblem(
            "maximum link rate must be strictly positive".to_owned(),
        ));
    }
    if let Some((index, _)) = problem
        .inputs
        .iter()
        .enumerate()
        .find(|(_, rate)| !rate.is_positive())
    {
        return Err(ReferenceError::InvalidProblem(format!(
            "input {index} rate must be strictly positive"
        )));
    }
    if let Some((index, _)) = problem
        .outputs
        .iter()
        .enumerate()
        .find(|(_, rate)| !rate.is_positive())
    {
        return Err(ReferenceError::InvalidProblem(format!(
            "output {index} rate must be strictly positive"
        )));
    }
    if problem.inputs.len() > u32::MAX as usize || problem.outputs.len() > u32::MAX as usize {
        return Err(ReferenceError::InvalidProblem(
            "terminal count exceeds the public index range".to_owned(),
        ));
    }
    Ok(())
}

fn external_capacity_contradiction(problem: &Problem) -> Option<GlobalUnsatReason> {
    for (index, rate) in problem.inputs.iter().enumerate() {
        if rate > &problem.max_link_rate {
            return Some(GlobalUnsatReason::ExternalRateExceedsCapacity {
                terminal: ExternalTerminal::Input(InputTerminalIndex(
                    u32::try_from(index).expect("validated input count fits u32"),
                )),
                rate: rate.clone(),
                max_link_rate: problem.max_link_rate.clone(),
            });
        }
    }
    for (index, rate) in problem.outputs.iter().enumerate() {
        if rate > &problem.max_link_rate {
            return Some(GlobalUnsatReason::ExternalRateExceedsCapacity {
                terminal: ExternalTerminal::Output(OutputTerminalIndex(
                    u32::try_from(index).expect("validated output count fits u32"),
                )),
                rate: rate.clone(),
                max_link_rate: problem.max_link_rate.clone(),
            });
        }
    }
    None
}

fn evaluate_topology(
    canonical_problem: &CanonicalProblem,
    caller_problem: &Problem,
    topology: ProfileTopology,
    plan: ProfilePlan,
    seen: &mut BTreeSet<solver_api::CanonicalGraphKey>,
) -> Result<Option<BestKnownSolution>, ReferenceError> {
    if topology.link_count() != plan.link_count as usize {
        return Ok(None);
    }
    let placeholder = PhysicalGraph {
        nodes: topology.nodes,
        links: topology
            .links
            .into_iter()
            .map(|(producer, consumer)| PhysicalLink {
                producer,
                consumer,
                flow: Rational::zero(),
            })
            .collect(),
    };
    let topology_canonical = canonicalize_graph(&canonical_problem.problem, &placeholder);
    if !seen.insert(topology_canonical.key) {
        return Ok(None);
    }

    let solved = match solve_topology(&canonical_problem.problem, &topology_canonical.graph) {
        Ok(graph) => graph,
        Err(error) if is_candidate_rejection(&error) => return Ok(None),
        Err(error) => {
            return Err(ReferenceError::InvalidEnumeratedTopology(Box::new(error)));
        }
    };
    let solved_canonical = canonicalize_graph(&canonical_problem.problem, &solved);
    validate_solution(&canonical_problem.problem, &solved_canonical.graph)
        .map_err(|error| ReferenceError::ValidationFirewall(Box::new(error)))?;
    let caller_graph = canonical_problem.restore_caller_terminals(&solved_canonical.graph)?;
    let validation = validate_solution(caller_problem, &caller_graph)
        .map_err(|error| ReferenceError::ValidationFirewall(Box::new(error)))?;
    let node_count = plan.profile.node_count();
    if validation.node_count != node_count
        || validation.link_count != plan.link_count
        || validation.physical_link_count != plan.physical_link_count
        || validation.discard_link_count != plan.discard_link_count
    {
        return Err(ReferenceError::ValidationCountMismatch {
            expected_nodes: node_count,
            expected_links: plan.link_count,
            expected_physical_links: plan.physical_link_count,
            expected_discard_links: plan.discard_link_count,
            actual_nodes: validation.node_count,
            actual_links: validation.link_count,
            actual_physical_links: validation.physical_link_count,
            actual_discard_links: validation.discard_link_count,
        });
    }
    Ok(Some(BestKnownSolution {
        node_count,
        link_count: validation.link_count,
        physical_link_count: plan.physical_link_count,
        discard_link_count: plan.discard_link_count,
        canonical_graph_key: solved_canonical.key,
        graph: caller_graph,
        validation,
    }))
}

/// Canonical terminal order used internally by reference enumeration.
///
/// The canonicalizer colors terminals by sorted exact rate. Searching against this
/// independently prepared problem prevents a canonical terminal index from being
/// interpreted through the caller's unrelated insertion order. The final graph is
/// mapped back and validated against the original problem before it can escape.
struct CanonicalProblem {
    problem: Problem,
    original_scale: Rational,
    input_original_by_canonical: Vec<InputTerminalIndex>,
    output_original_by_canonical: Vec<OutputTerminalIndex>,
}

impl CanonicalProblem {
    fn new(problem: &Problem) -> Result<Self, ReferenceError> {
        let original_scale = common_rate_scale(problem);
        let normalized_inputs = problem
            .inputs
            .iter()
            .map(|rate| {
                rate.checked_div(&original_scale)
                    .expect("global rate scale is strictly positive")
            })
            .collect::<Vec<_>>();
        let normalized_outputs = problem
            .outputs
            .iter()
            .map(|rate| {
                rate.checked_div(&original_scale)
                    .expect("global rate scale is strictly positive")
            })
            .collect::<Vec<_>>();
        let (inputs, input_original_by_canonical) = canonical_side(&normalized_inputs)?;
        let (outputs, output_original_by_canonical) = canonical_side(&normalized_outputs)?;
        Ok(Self {
            problem: Problem {
                inputs,
                outputs,
                max_link_rate: problem
                    .max_link_rate
                    .checked_div(&original_scale)
                    .expect("global rate scale is strictly positive"),
            },
            original_scale,
            input_original_by_canonical: input_original_by_canonical
                .into_iter()
                .map(InputTerminalIndex)
                .collect(),
            output_original_by_canonical: output_original_by_canonical
                .into_iter()
                .map(OutputTerminalIndex)
                .collect(),
        })
    }

    fn restore_caller_terminals(
        &self,
        canonical: &PhysicalGraph,
    ) -> Result<PhysicalGraph, ReferenceError> {
        let mut graph = canonical.clone();
        for link in &mut graph.links {
            if let ProducerPortRef::Input(index) = link.producer {
                let index = usize::try_from(index.0)
                    .map_err(|_| ReferenceError::CanonicalTerminalMapping)?;
                link.producer = ProducerPortRef::Input(
                    *self
                        .input_original_by_canonical
                        .get(index)
                        .ok_or(ReferenceError::CanonicalTerminalMapping)?,
                );
            }
            if let ConsumerPortRef::Output(index) = link.consumer {
                let index = usize::try_from(index.0)
                    .map_err(|_| ReferenceError::CanonicalTerminalMapping)?;
                link.consumer = ConsumerPortRef::Output(
                    *self
                        .output_original_by_canonical
                        .get(index)
                        .ok_or(ReferenceError::CanonicalTerminalMapping)?,
                );
            }
            link.flow = &link.flow * &self.original_scale;
        }
        Ok(graph)
    }
}

/// Returns the largest positive rational scale dividing every external rate.
///
/// This independently reproduces the plan's one-global-unit normalization: clear
/// every input/output denominator with one LCM, take the integer GCD of every
/// cleared numerator, and form `gcd/lcm`. Capacity is deliberately not part of
/// the GCD; it is divided by the resulting rate scale alongside every flow.
fn common_rate_scale(problem: &Problem) -> Rational {
    let rates = problem.inputs.iter().chain(&problem.outputs);
    let zero = Rational::zero().numerator().clone();

    let mut denominator_lcm = Rational::one().denominator().clone();
    for rate in rates.clone() {
        let denominator = rate.denominator().clone();
        let mut left = denominator_lcm.clone();
        let mut right = denominator.clone();
        while right != zero {
            let remainder = &left % &right;
            left = right;
            right = remainder;
        }
        denominator_lcm = (denominator_lcm / left) * denominator;
    }

    let mut cleared = rates.map(|rate| rate.numerator() * (&denominator_lcm / rate.denominator()));
    let mut numerator_gcd = cleared
        .next()
        .expect("validated problems have at least one external rate");
    for numerator in cleared {
        let mut left = numerator_gcd;
        let mut right = numerator;
        while right != zero {
            let remainder = &left % &right;
            left = right;
            right = remainder;
        }
        numerator_gcd = left;
    }

    Rational::new(numerator_gcd, denominator_lcm)
        .expect("a denominator LCM of positive external rates is nonzero")
}

fn canonical_side(rates: &[Rational]) -> Result<(Vec<Rational>, Vec<u32>), ReferenceError> {
    let mut indexed = rates.iter().cloned().enumerate().collect::<Vec<_>>();
    indexed.sort_by(|(left_index, left), (right_index, right)| {
        left.cmp(right).then(left_index.cmp(right_index))
    });
    let mut canonical_rates = Vec::with_capacity(indexed.len());
    let mut originals = Vec::with_capacity(indexed.len());
    for (original, rate) in indexed {
        originals.push(u32::try_from(original).map_err(|_| ReferenceError::CountOverflow)?);
        canonical_rates.push(rate);
    }
    Ok((canonical_rates, originals))
}

fn is_candidate_rejection(error: &ValidationError) -> bool {
    matches!(
        error,
        ValidationError::NodeUnreachableFromInput { .. }
            | ValidationError::NodeCannotReachOutput { .. }
            | ValidationError::InconsistentSteadyState
            | ValidationError::NonUniqueSteadyState { .. }
            | ValidationError::InconsistentCyclicScc { .. }
            | ValidationError::NonUniqueCyclicScc { .. }
            | ValidationError::IncorrectOutputRate { .. }
            | ValidationError::NonPositiveResolvedFlow { .. }
            | ValidationError::ResolvedFlowAboveCapacity { .. }
    )
}

fn retain_smallest(target: &mut Option<BestKnownSolution>, candidate: BestKnownSolution) {
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
    }
}

fn empty_proof() -> ProofSummary {
    ProofSummary {
        proof_version: REFERENCE_PROOF_VERSION,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn problem(inputs: &[&str], outputs: &[&str], capacity: &str) -> Problem {
        Problem {
            inputs: inputs.iter().map(|value| value.parse().unwrap()).collect(),
            outputs: outputs.iter().map(|value| value.parse().unwrap()).collect(),
            max_link_rate: capacity.parse().unwrap(),
        }
    }

    fn solve(problem: &Problem, max_nodes: u32) -> SolveResult {
        solve_reference(
            problem,
            &ReferenceOptions { max_nodes },
            &AtomicBool::new(false),
        )
        .unwrap()
    }

    #[test]
    fn direct_link_is_proved_optimal_at_zero_nodes() {
        let problem = problem(&["60"], &["60"], "60");
        let result = solve(&problem, 0);
        let SolveResult::Optimal(solution) = result else {
            panic!("expected an optimal direct link");
        };
        assert_eq!((solution.node_count, solution.link_count), (0, 0));
        assert_eq!(solution.graph.links[0].flow, "60".parse().unwrap());
        assert_eq!(
            solution.validation,
            validate_solution(&problem, &solution.graph).unwrap()
        );
    }

    #[test]
    fn one_global_rate_scale_preserves_the_key_and_restores_caller_flows() {
        let SolveResult::Optimal(unit) = solve(&problem(&["1"], &["1"], "2"), 0) else {
            panic!("expected unit direct solution");
        };
        let scaled_problem = problem(&["20"], &["20"], "40");
        let SolveResult::Optimal(scaled) = solve(&scaled_problem, 0) else {
            panic!("expected scaled direct solution");
        };

        assert_eq!(unit.canonical_graph_key, scaled.canonical_graph_key);
        assert_eq!(unit.graph.links[0].flow, Rational::one());
        assert_eq!(scaled.graph.links[0].flow, "20".parse().unwrap());
        validate_solution(&scaled_problem, &scaled.graph).unwrap();
    }

    #[test]
    fn fractional_surplus_uses_the_same_normalized_key_as_integer_units() {
        let fractional_problem = problem(&["1", "1"], &["1/2"], "1");
        let SolveResult::Optimal(fractional) = solve(&fractional_problem, 1) else {
            panic!("expected fractional discard solution");
        };
        let integer_problem = problem(&["2", "2"], &["1"], "2");
        let SolveResult::Optimal(integer) = solve(&integer_problem, 1) else {
            panic!("expected integer-unit discard solution");
        };

        assert_eq!(fractional.canonical_graph_key, integer.canonical_graph_key);
        assert_eq!(fractional.node_count, integer.node_count);
        assert_eq!(fractional.link_count, integer.link_count);
        assert_eq!(fractional.discard_link_count, integer.discard_link_count);
        assert_eq!(discard_total(&fractional.graph), "3/2".parse().unwrap());
        assert_eq!(discard_total(&integer.graph), "3".parse().unwrap());
    }

    #[test]
    fn canonical_problem_clears_all_denominators_with_one_exact_gcd_unit() {
        let canonical = CanonicalProblem::new(&problem(&["1/2", "1/3"], &["1/6"], "1")).unwrap();
        assert_eq!(canonical.problem.inputs, vec![2.into(), 3.into()]);
        assert_eq!(canonical.problem.outputs, vec![1.into()]);
        assert_eq!(canonical.problem.max_link_rate, 6.into());
        assert_eq!(canonical.original_scale, "1/6".parse().unwrap());
        assert_eq!(
            canonical.input_original_by_canonical,
            vec![InputTerminalIndex(1), InputTerminalIndex(0)]
        );
    }

    #[test]
    fn exhaustively_finds_one_splitter_and_one_merger_optima() {
        let split = solve(&problem(&["120"], &["60", "60"], "120"), 1);
        let SolveResult::Optimal(split) = split else {
            panic!("expected splitter optimum");
        };
        assert_eq!((split.node_count, split.link_count), (1, 0));

        let merge = solve(&problem(&["30", "90"], &["120"], "120"), 1);
        let SolveResult::Optimal(merge) = merge else {
            panic!("expected merger optimum");
        };
        assert_eq!((merge.node_count, merge.link_count), (1, 0));
    }

    #[test]
    fn zero_node_surplus_uses_a_physical_discard_excluded_from_link_objective() {
        let problem = problem(&["2", "1"], &["2"], "2");
        let SolveResult::Optimal(solution) = solve(&problem, 0) else {
            panic!("expected direct output plus one discard");
        };
        assert_eq!(solution.node_count, 0);
        assert_eq!(solution.link_count, 0);
        assert_eq!(solution.physical_link_count, 2);
        assert_eq!(solution.discard_link_count, 1);
        assert_eq!(discard_total(&solution.graph), "1".parse().unwrap());
        assert_eq!(
            solution.validation,
            validate_solution(&problem, &solution.graph).unwrap()
        );
    }

    #[test]
    fn discard_capacity_can_force_an_extra_node_and_two_discard_lines() {
        let problem = problem(&["1", "1"], &["1/2"], "1");
        let SolveResult::Optimal(solution) = solve(&problem, 1) else {
            panic!("expected one splitter to create sufficient discard lines");
        };
        assert_eq!((solution.node_count, solution.link_count), (1, 0));
        assert_eq!(solution.physical_link_count, 4);
        assert_eq!(solution.discard_link_count, 2);
        assert_eq!(
            solution.proof.profiles_exhausted, 2,
            "both equal-L splitter profiles, including the three-discard alternative, must be exhausted"
        );
        assert_eq!(discard_total(&solution.graph), "3/2".parse().unwrap());
        assert!(solution.graph.links.iter().all(|link| {
            !matches!(link.consumer, ConsumerPortRef::Discard(_))
                || (link.flow.is_positive() && link.flow <= problem.max_link_rate)
        }));
    }

    #[test]
    fn external_capacity_failure_is_a_finite_global_proof() {
        let result = solve(&problem(&["11"], &["11"], "10"), 0);
        assert!(matches!(
            result,
            SolveResult::GloballyUnsat(GlobalUnsatProof {
                reason: GlobalUnsatReason::ExternalRateExceedsCapacity { .. },
                ..
            })
        ));
    }

    #[test]
    fn input_deficit_is_a_finite_global_proof() {
        let result = solve(&problem(&["1"], &["2"], "10"), 0);
        assert!(matches!(
            result,
            SolveResult::GloballyUnsat(GlobalUnsatProof {
                reason: GlobalUnsatReason::InsufficientInput { .. },
                ..
            })
        ));
    }

    #[test]
    fn bounded_exhaustion_is_incomplete_not_globally_unsat() {
        let result = solve(&problem(&["120"], &["40", "80"], "120"), 0);
        let SolveResult::Incomplete(incomplete) = result else {
            panic!("bounded exhaustion must be incomplete");
        };
        assert!(matches!(
            incomplete.reason,
            IncompleteReason::ResourceLimit { .. }
        ));
        assert!(incomplete.best_known.is_none());
        assert_eq!(incomplete.proof.node_counts_exhausted_through, Some(0));
    }

    #[test]
    fn pre_cancelled_search_is_incomplete_without_false_proof() {
        let cancel = AtomicBool::new(true);
        let result = solve_reference(
            &problem(&["120"], &["60", "60"], "120"),
            &ReferenceOptions { max_nodes: 1 },
            &cancel,
        )
        .unwrap();
        let SolveResult::Incomplete(incomplete) = result else {
            panic!("cancelled search must be incomplete");
        };
        assert_eq!(incomplete.reason, IncompleteReason::Cancelled);
        assert!(incomplete.best_known.is_none());
        assert_eq!(incomplete.proof.node_counts_exhausted_through, None);
    }

    #[test]
    fn equal_rate_terminal_symmetry_returns_the_same_canonical_witness() {
        let problem = problem(&["120"], &["60", "60"], "120");
        let first = solve(&problem, 1);
        let second = solve(&problem, 1);
        assert_eq!(first, second);
    }

    #[test]
    fn unsorted_unequal_inputs_are_mapped_back_to_caller_indices() {
        let problem = problem(&["90", "30"], &["120"], "120");
        let SolveResult::Optimal(solution) = solve(&problem, 1) else {
            panic!("expected one-merger optimum");
        };
        assert_eq!(
            input_flow(&solution.graph, InputTerminalIndex(0)),
            "90".parse().unwrap()
        );
        assert_eq!(
            input_flow(&solution.graph, InputTerminalIndex(1)),
            "30".parse().unwrap()
        );
        validate_solution(&problem, &solution.graph).unwrap();
    }

    #[test]
    fn unsorted_unequal_outputs_are_mapped_back_to_caller_indices() {
        let problem = problem(&["120"], &["80", "40"], "120");
        let SolveResult::Optimal(solution) = solve(&problem, 2) else {
            panic!("expected splitter-merger optimum");
        };
        assert_eq!(
            output_flow(&solution.graph, OutputTerminalIndex(0)),
            "80".parse().unwrap()
        );
        assert_eq!(
            output_flow(&solution.graph, OutputTerminalIndex(1)),
            "40".parse().unwrap()
        );
        validate_solution(&problem, &solution.graph).unwrap();
    }

    fn input_flow(graph: &PhysicalGraph, terminal: InputTerminalIndex) -> Rational {
        graph
            .links
            .iter()
            .find(|link| link.producer == ProducerPortRef::Input(terminal))
            .expect("validated input has one physical link")
            .flow
            .clone()
    }

    fn output_flow(graph: &PhysicalGraph, terminal: OutputTerminalIndex) -> Rational {
        graph
            .links
            .iter()
            .find(|link| link.consumer == ConsumerPortRef::Output(terminal))
            .expect("validated output has one physical link")
            .flow
            .clone()
    }

    fn discard_total(graph: &PhysicalGraph) -> Rational {
        graph
            .links
            .iter()
            .filter(|link| matches!(link.consumer, ConsumerPortRef::Discard(_)))
            .map(|link| link.flow.clone())
            .sum()
    }

    #[test]
    fn rejects_nonpositive_public_rates_as_invalid_input() {
        let error = solve_reference(
            &problem(&["0"], &["0"], "10"),
            &ReferenceOptions { max_nodes: 0 },
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert!(matches!(error, ReferenceError::InvalidProblem(_)));
    }
}
