use super::{
    GraphNode, GraphNodeKind, PresentationError, PresentationSolution, PresentationStats,
    PresentedIncomplete, PresentedSolveOutcome,
};
use super::{
    feedback::feedback_annotation,
    graph::{build_steps, operator_map, operator_nodes, present_link, terminal_nodes},
    rates::format_rate,
};
use solver_api::{
    BestKnownSolution, ConsumerPortRef, NodeType, OptimalSolution, PhysicalGraph, PreparedProblem,
    ProducerPortRef, ProofSummary, Rational, SolveResult, ValidationSummary,
};

/// Converts one exact mathematical result without affecting solver decisions.
///
/// # Errors
///
/// Returns [`PresentationError`] if a supposedly validated witness has malformed
/// references or accounting. Such a failure must be treated as an internal error.
pub fn present_solve_result(
    request: &PreparedProblem,
    result: &SolveResult,
) -> Result<PresentedSolveOutcome, PresentationError> {
    match result {
        SolveResult::Optimal(solution) => Ok(PresentedSolveOutcome::Optimal(present_optimal(
            request, solution,
        )?)),
        SolveResult::GloballyUnsat(proof) => {
            Ok(PresentedSolveOutcome::GloballyUnsat(proof.clone()))
        }
        SolveResult::Incomplete(incomplete) => {
            let best_known = incomplete
                .best_known
                .as_ref()
                .map(|solution| {
                    present_best_known_solution(
                        request,
                        solution,
                        incomplete.proof.node_counts_exhausted_through,
                    )
                })
                .transpose()?;
            Ok(PresentedSolveOutcome::Incomplete(PresentedIncomplete {
                reason: incomplete.reason.clone(),
                best_known,
                proof: incomplete.proof.clone(),
            }))
        }
    }
}

fn present_optimal(
    request: &PreparedProblem,
    solution: &OptimalSolution,
) -> Result<PresentationSolution, PresentationError> {
    present_witness(
        request,
        "proven_optimal",
        WitnessCounts {
            nodes: solution.node_count,
            links: solution.link_count,
            physical_links: solution.physical_link_count,
            discard_links: solution.discard_link_count,
            checked_through: Some(solution.node_count),
        },
        &solution.graph,
        Some(solution.proof.clone()),
        solution.validation.clone(),
    )
}

/// Projects one independently validated incumbent without implying optimality.
///
/// # Errors
///
/// Returns [`PresentationError`] when witness references or exact accounting
/// disagree with the prepared application request.
pub fn present_best_known_solution(
    request: &PreparedProblem,
    solution: &BestKnownSolution,
    checked_through: Option<u32>,
) -> Result<PresentationSolution, PresentationError> {
    present_witness(
        request,
        "best_known",
        WitnessCounts {
            nodes: solution.node_count,
            links: solution.link_count,
            physical_links: solution.physical_link_count,
            discard_links: solution.discard_link_count,
            checked_through,
        },
        &solution.graph,
        None,
        solution.validation.clone(),
    )
}

#[derive(Clone, Copy)]
struct WitnessCounts {
    nodes: u32,
    links: u32,
    physical_links: u32,
    discard_links: u32,
    checked_through: Option<u32>,
}

fn present_witness(
    request: &PreparedProblem,
    status: &str,
    counts: WitnessCounts,
    graph: &PhysicalGraph,
    proof: Option<ProofSummary>,
    validation: ValidationSummary,
) -> Result<PresentationSolution, PresentationError> {
    verify_counts(counts, graph)?;
    let operators = operator_map(graph)?;
    let (feedback_links, feedback_loops) = feedback_annotation(&operators, &graph.links);
    let mut nodes = terminal_nodes(&request.inputs, GraphNodeKind::Input, "input");
    nodes.extend(operator_nodes(&operators));
    nodes.extend(terminal_nodes(
        &request.outputs,
        GraphNodeKind::Output,
        "output",
    ));

    let mut links = graph.links.clone();
    links.sort();
    let mut edges = Vec::with_capacity(links.len());
    let mut discard_total = Rational::zero();
    for (index, link) in links.iter().enumerate() {
        if matches!(link.consumer, ConsumerPortRef::Discard(_)) {
            discard_total = &discard_total + &link.flow;
        }
        edges.push(present_link(
            index,
            link,
            request,
            &operators,
            feedback_links.contains(&(link.producer, link.consumer)),
        )?);
    }
    let discards = edges
        .iter()
        .filter(|edge| edge.discarded)
        .map(|edge| (edge.target.clone(), edge.rate.clone()))
        .collect::<Vec<_>>();
    for (offset, (id, rate)) in discards.iter().enumerate() {
        let label = if discards.len() == 1 {
            format!("Sink · {} /min", rate.exact)
        } else {
            format!("Sink {} · {} /min", offset + 1, rate.exact)
        };
        nodes.push(GraphNode {
            id: id.clone(),
            kind: GraphNodeKind::Discard,
            label,
        });
    }

    let total_input = request.problem.inputs.iter().sum::<Rational>();
    let total_output = request.problem.outputs.iter().sum::<Rational>();
    let expected_discard = &total_input - &total_output;
    if discard_total != expected_discard {
        return Err(PresentationError::DiscardTotalMismatch);
    }
    let splitters = u32::try_from(
        operators
            .values()
            .filter(|kind| matches!(kind, NodeType::Splitter2 | NodeType::Splitter3))
            .count(),
    )
    .map_err(|_| PresentationError::CountOverflow)?;
    let mergers = counts
        .nodes
        .checked_sub(splitters)
        .ok_or(PresentationError::CountOverflow)?;
    let build_steps = build_steps(&nodes, &edges);
    Ok(PresentationSolution {
        status: status.to_owned(),
        model_version: 4,
        proof,
        validation,
        stats: PresentationStats {
            node_count: counts.nodes,
            link_count: counts.links,
            physical_link_count: counts.physical_links,
            discard_link_count: counts.discard_links,
            splitters,
            mergers,
            feedback_loops,
            checked_through: counts.checked_through,
            internal_max_throughput: format_rate(
                &graph
                    .links
                    .iter()
                    .filter(|link| {
                        matches!(link.producer, ProducerPortRef::Node { .. })
                            && matches!(link.consumer, ConsumerPortRef::Node { .. })
                    })
                    .map(|link| link.flow.clone())
                    .max()
                    .unwrap_or_else(Rational::zero),
            ),
        },
        total_input: format_rate(&total_input),
        total_output: format_rate(&total_output),
        discard_rate: format_rate(&expected_discard),
        belt_rate: format_rate(&request.problem.max_link_rate),
        nodes,
        edges,
        build_steps,
    })
}

fn verify_counts(counts: WitnessCounts, graph: &PhysicalGraph) -> Result<(), PresentationError> {
    let actual_nodes =
        u32::try_from(graph.nodes.len()).map_err(|_| PresentationError::CountOverflow)?;
    let actual_physical =
        u32::try_from(graph.links.len()).map_err(|_| PresentationError::CountOverflow)?;
    let actual_discard = u32::try_from(
        graph
            .links
            .iter()
            .filter(|link| matches!(link.consumer, ConsumerPortRef::Discard(_)))
            .count(),
    )
    .map_err(|_| PresentationError::CountOverflow)?;
    for (field, expected, actual) in [
        ("node_count", counts.nodes, actual_nodes),
        (
            "physical_link_count",
            counts.physical_links,
            actual_physical,
        ),
        ("discard_link_count", counts.discard_links, actual_discard),
        (
            "link_count",
            counts.links,
            u32::try_from(
                graph
                    .links
                    .iter()
                    .filter(|link| {
                        matches!(link.producer, ProducerPortRef::Node { .. })
                            && matches!(link.consumer, ConsumerPortRef::Node { .. })
                    })
                    .count(),
            )
            .map_err(|_| PresentationError::CountOverflow)?,
        ),
    ] {
        if expected != actual {
            return Err(PresentationError::CountMismatch {
                field,
                expected,
                actual,
            });
        }
    }
    Ok(())
}
