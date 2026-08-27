//! Deterministic presentation projection for exact solver outcomes.

use std::collections::{BTreeMap, BTreeSet};

use num::{Integer, Signed, ToPrimitive, Zero};
use serde::{Deserialize, Serialize};
use solver_api::{
    BestKnownSolution, ConsumerPortRef, GlobalUnsatProof, IncompleteReason, NodeId, NodeType,
    OptimalSolution, PhysicalGraph, PhysicalLink, ProducerPortRef, ProofSummary, Rational,
    SolveResult, ValidationSummary,
};
use thiserror::Error;

use solver_api::{PreparedProblem, TerminalMetadata};

/// Exact value plus a deterministic six-place decimal preview.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayRate {
    pub exact: String,
    pub decimal: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphNodeKind {
    Input,
    Splitter2,
    Splitter3,
    Merger2,
    Merger3,
    Output,
    Discard,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphNode {
    pub id: String,
    pub kind: GraphNodeKind,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub source_port: usize,
    pub target_port: usize,
    pub rate: DisplayRate,
    pub feedback: bool,
    pub discarded: bool,
}

/// Public counts separate operator belts from all physical terminal/discard links.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationStats {
    pub node_count: u32,
    pub link_count: u32,
    pub physical_link_count: u32,
    pub discard_link_count: u32,
    pub splitters: u32,
    pub mergers: u32,
    pub feedback_loops: u32,
    pub checked_through: Option<u32>,
    pub internal_max_throughput: DisplayRate,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationSolution {
    /// `proven_optimal` or the explicitly non-optimal `best_known`.
    pub status: String,
    pub model_version: u32,
    /// Present only for a completed optimality proof. A live or incomplete
    /// incumbent deliberately cannot acquire this field.
    pub proof: Option<ProofSummary>,
    /// Independent exact validation record carried by every displayed witness.
    pub validation: ValidationSummary,
    pub stats: PresentationStats,
    pub total_input: DisplayRate,
    pub total_output: DisplayRate,
    pub discard_rate: DisplayRate,
    pub belt_rate: DisplayRate,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub build_steps: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentedIncomplete {
    pub reason: IncompleteReason,
    pub best_known: Option<PresentationSolution>,
    pub proof: ProofSummary,
}

/// Application-facing mathematical outcome; internal failures remain errors.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "result", rename_all = "snake_case")]
pub enum PresentedSolveOutcome {
    Optimal(PresentationSolution),
    GloballyUnsat(GlobalUnsatProof),
    Incomplete(PresentedIncomplete),
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum PresentationError {
    #[error("solver witness count {field} is {actual}, expected {expected}")]
    CountMismatch {
        field: &'static str,
        expected: u32,
        actual: u32,
    },
    #[error("solver witness contains duplicate node id {0}")]
    DuplicateNode(u32),
    #[error("solver witness references unknown node id {0}")]
    UnknownNode(u32),
    #[error("solver witness references {side} terminal {index}, but only {count} exist")]
    TerminalOutOfRange {
        side: &'static str,
        index: u32,
        count: usize,
    },
    #[error("solver witness discard total disagrees with the exact external surplus")]
    DiscardTotalMismatch,
    #[error("solver witness count cannot fit the presentation integer type")]
    CountOverflow,
}

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

fn operator_map(graph: &PhysicalGraph) -> Result<BTreeMap<NodeId, NodeType>, PresentationError> {
    let mut operators = BTreeMap::new();
    for node in &graph.nodes {
        if operators.insert(node.id, node.node_type).is_some() {
            return Err(PresentationError::DuplicateNode(node.id.0));
        }
    }
    Ok(operators)
}

fn terminal_nodes(
    terminals: &[TerminalMetadata],
    kind: GraphNodeKind,
    prefix: &str,
) -> Vec<GraphNode> {
    terminals
        .iter()
        .enumerate()
        .map(|(index, terminal)| GraphNode {
            id: format!("{prefix}-{index}"),
            kind,
            label: format!("{} · {} /min", terminal.name, terminal.rate),
        })
        .collect()
}

fn operator_nodes(operators: &BTreeMap<NodeId, NodeType>) -> Vec<GraphNode> {
    operators
        .iter()
        .map(|(&id, &node_type)| GraphNode {
            id: operator_id(id),
            kind: node_kind(node_type),
            label: operator_label(id, node_type),
        })
        .collect()
}

fn present_link(
    index: usize,
    link: &PhysicalLink,
    request: &PreparedProblem,
    operators: &BTreeMap<NodeId, NodeType>,
    feedback: bool,
) -> Result<GraphEdge, PresentationError> {
    let (source, source_port) = match link.producer {
        ProducerPortRef::Input(input) => {
            terminal(request.inputs.len(), "input", input.0)?;
            (format!("input-{}", input.0), 0)
        }
        ProducerPortRef::Node { node, port } => {
            require_node(operators, node)?;
            (operator_id(node), usize::from(port))
        }
    };
    let (target, target_port, discarded) = match link.consumer {
        ConsumerPortRef::Output(output) => {
            terminal(request.outputs.len(), "output", output.0)?;
            (format!("output-{}", output.0), 0, false)
        }
        ConsumerPortRef::Discard(discard) => (format!("sink-{}", discard.0), 0, true),
        ConsumerPortRef::Node { node, port } => {
            require_node(operators, node)?;
            (operator_id(node), usize::from(port), false)
        }
    };
    Ok(GraphEdge {
        id: format!("edge-{index}"),
        source,
        target,
        source_port,
        target_port,
        rate: format_rate(&link.flow),
        feedback,
        discarded,
    })
}

fn terminal(count: usize, side: &'static str, index: u32) -> Result<(), PresentationError> {
    if usize::try_from(index)
        .ok()
        .is_some_and(|index| index < count)
    {
        Ok(())
    } else {
        Err(PresentationError::TerminalOutOfRange { side, index, count })
    }
}

fn require_node(
    operators: &BTreeMap<NodeId, NodeType>,
    node: NodeId,
) -> Result<(), PresentationError> {
    if operators.contains_key(&node) {
        Ok(())
    } else {
        Err(PresentationError::UnknownNode(node.0))
    }
}

fn node_kind(node_type: NodeType) -> GraphNodeKind {
    match node_type {
        NodeType::Splitter2 => GraphNodeKind::Splitter2,
        NodeType::Splitter3 => GraphNodeKind::Splitter3,
        NodeType::Merger2 => GraphNodeKind::Merger2,
        NodeType::Merger3 => GraphNodeKind::Merger3,
    }
}

fn operator_id(id: NodeId) -> String {
    format!("operator-{}", id.0)
}

fn operator_label(id: NodeId, node_type: NodeType) -> String {
    let number = id.0 + 1;
    match node_type {
        NodeType::Splitter2 => format!("Splitter {number} · 2-way"),
        NodeType::Splitter3 => format!("Splitter {number} · 3-way"),
        NodeType::Merger2 => format!("Merger {number} · 2-way"),
        NodeType::Merger3 => format!("Merger {number} · 3-way"),
    }
}

/// Marks feedback belts with the same natural-loop / irreducible-cycle rules as
/// the Z3 verifier: dominator back-edges first, then a DFS for leftover cycles.
fn feedback_annotation(
    operators: &BTreeMap<NodeId, NodeType>,
    links: &[PhysicalLink],
) -> (BTreeSet<(ProducerPortRef, ConsumerPortRef)>, u32) {
    let ids = operators.keys().copied().collect::<Vec<_>>();
    let indexes = ids
        .iter()
        .enumerate()
        .map(|(index, &id)| (id, index))
        .collect::<BTreeMap<_, _>>();
    let node_count = ids.len();

    let mut operator_links = Vec::new();
    let mut entries = Vec::new();
    for link in links {
        match (link.producer, link.consumer) {
            (
                ProducerPortRef::Node { node: source, .. },
                ConsumerPortRef::Node { node: target, .. },
            ) => {
                let (Some(&source), Some(&target)) = (indexes.get(&source), indexes.get(&target))
                else {
                    continue;
                };
                let edge = operator_links.len();
                operator_links.push((source, target, edge, link.producer, link.consumer));
            }
            (ProducerPortRef::Input(_), ConsumerPortRef::Node { node: target, .. }) => {
                if let Some(&target) = indexes.get(&target) {
                    entries.push(target);
                }
            }
            _ => {}
        }
    }
    entries.sort_unstable();
    entries.dedup();

    let mut successors = vec![Vec::new(); node_count];
    let algo_links = operator_links
        .iter()
        .map(|&(source, target, edge, _, _)| {
            successors[source].push(target);
            (source, target, edge)
        })
        .collect::<Vec<_>>();

    let idom = immediate_dominators(node_count, &successors, &entries);
    let virtual_source = node_count;
    let mut flags = vec![false; operator_links.len()];
    for &(node, target, edge) in &algo_links {
        if dominates(&idom, target, node, virtual_source) {
            flags[edge] = true;
        }
    }
    mark_irreducible_back_edges(node_count, &algo_links, &entries, &mut flags);

    let mut feedback = BTreeSet::new();
    for (edge, marked) in flags.into_iter().enumerate() {
        if marked {
            let (_, _, _, producer, consumer) = operator_links[edge];
            feedback.insert((producer, consumer));
        }
    }
    let feedback_count = u32::try_from(feedback.len()).unwrap_or(u32::MAX);
    (feedback, feedback_count)
}

fn reverse_postorder(graph: &[Vec<usize>], start: usize) -> Vec<usize> {
    let successors = graph
        .iter()
        .map(|edges| {
            let mut nodes = edges.clone();
            nodes.sort_unstable();
            nodes.dedup();
            nodes
        })
        .collect::<Vec<_>>();
    let mut seen = vec![false; graph.len()];
    let mut post = Vec::new();
    let mut stack = vec![(start, 0_usize)];
    seen[start] = true;
    while let Some((node, child)) = stack.pop() {
        if child < successors[node].len() {
            stack.push((node, child + 1));
            let next = successors[node][child];
            if !seen[next] {
                seen[next] = true;
                stack.push((next, 0));
            }
        } else {
            post.push(node);
        }
    }
    post.reverse();
    post
}

fn intersect_dominators(
    mut first: usize,
    mut second: usize,
    idom: &[Option<usize>],
    rpo_index: &[usize],
) -> usize {
    while first != second {
        while rpo_index[first] > rpo_index[second] {
            first = idom[first].expect("a processed predecessor has an immediate dominator");
        }
        while rpo_index[second] > rpo_index[first] {
            second = idom[second].expect("a processed predecessor has an immediate dominator");
        }
    }
    first
}

fn immediate_dominators(
    node_count: usize,
    successors: &[Vec<usize>],
    entries: &[usize],
) -> Vec<Option<usize>> {
    let source = node_count;
    let mut graph = successors.to_vec();
    graph.push(entries.to_vec());
    let mut predecessors = vec![Vec::new(); node_count + 1];
    for (node, targets) in graph.iter().enumerate() {
        for &target in targets {
            predecessors[target].push(node);
        }
    }
    let rpo = reverse_postorder(&graph, source);
    let mut rpo_index = vec![usize::MAX; node_count + 1];
    for (index, &node) in rpo.iter().enumerate() {
        rpo_index[node] = index;
    }
    let mut idom = vec![None; node_count + 1];
    idom[source] = Some(source);
    let mut changed = true;
    while changed {
        changed = false;
        for &node in &rpo {
            if node == source {
                continue;
            }
            let mut new_idom = None;
            for &predecessor in &predecessors[node] {
                if idom[predecessor].is_none() {
                    continue;
                }
                new_idom = Some(match new_idom {
                    None => predecessor,
                    Some(current) => intersect_dominators(predecessor, current, &idom, &rpo_index),
                });
            }
            if new_idom.is_some() && idom[node] != new_idom {
                idom[node] = new_idom;
                changed = true;
            }
        }
    }
    idom
}

fn dominates(idom: &[Option<usize>], header: usize, node: usize, source: usize) -> bool {
    if header == node {
        return true;
    }
    let mut cursor = node;
    while cursor != source {
        let Some(parent) = idom[cursor] else {
            return false;
        };
        if parent == cursor {
            break;
        }
        cursor = parent;
        if cursor == header {
            return true;
        }
    }
    header == source
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DfsColor {
    White,
    Grey,
    Black,
}

fn mark_irreducible_back_edges(
    node_count: usize,
    links: &[(usize, usize, usize)],
    entries: &[usize],
    feedback: &mut [bool],
) {
    let mut adjacency = vec![Vec::new(); node_count];
    for &(source, target, edge) in links {
        if !feedback[edge] {
            adjacency[source].push((target, edge));
        }
    }
    for edges in &mut adjacency {
        edges.sort_unstable();
    }

    let mut color = vec![DfsColor::White; node_count];
    let mut stack = Vec::new();
    for &entry in entries {
        if color[entry] == DfsColor::White {
            color[entry] = DfsColor::Grey;
            stack.push((entry, 0_usize));
        }
        while let Some((node, child)) = stack.pop() {
            if child < adjacency[node].len() {
                stack.push((node, child + 1));
                let (target, edge) = adjacency[node][child];
                match color[target] {
                    DfsColor::Grey => feedback[edge] = true,
                    DfsColor::White => {
                        color[target] = DfsColor::Grey;
                        stack.push((target, 0));
                    }
                    DfsColor::Black => {}
                }
            } else {
                color[node] = DfsColor::Black;
            }
        }
    }
}

fn format_rate(value: &Rational) -> DisplayRate {
    DisplayRate {
        exact: value.to_string(),
        decimal: decimal_string(value, 6),
    }
}

fn decimal_string(value: &Rational, precision: usize) -> String {
    let negative = value.is_negative();
    let numerator = value.numerator().abs();
    let denominator = value.denominator();
    let (whole, mut remainder) = numerator.div_rem(denominator);
    if remainder.is_zero() {
        return format!("{}{}", if negative { "-" } else { "" }, whole);
    }
    let mut digits = String::with_capacity(precision);
    for _ in 0..precision {
        remainder *= 10_u8;
        let (digit, next) = remainder.div_rem(denominator);
        digits.push(char::from(b'0' + digit.to_u8().unwrap_or(0)));
        remainder = next;
        if remainder.is_zero() {
            break;
        }
    }
    while digits.ends_with('0') {
        digits.pop();
    }
    format!("{}{}.{}", if negative { "-" } else { "" }, whole, digits)
}

fn build_steps(nodes: &[GraphNode], edges: &[GraphEdge]) -> Vec<String> {
    let labels = nodes
        .iter()
        .map(|node| (node.id.as_str(), node.label.as_str()))
        .collect::<BTreeMap<_, _>>();
    edges
        .iter()
        .map(|edge| {
            format!(
                "Connect {} port {} to {} port {} at {} /min{}",
                labels
                    .get(edge.source.as_str())
                    .copied()
                    .unwrap_or(&edge.source),
                edge.source_port + 1,
                labels
                    .get(edge.target.as_str())
                    .copied()
                    .unwrap_or(&edge.target),
                edge.target_port + 1,
                edge.rate.exact,
                if edge.discarded { " (discard)" } else { "" },
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use solver_api::{
        BestKnownSolution, CanonicalGraphKey, DiscardTerminalIndex, IncompleteResult,
        InputTerminalIndex, OptimalSolution, OutputTerminalIndex, PhysicalNode, ValidationSummary,
    };

    use super::*;
    use solver_api::{EndpointRequest, ProblemRequest};

    fn endpoint(id: &str, rate: &str) -> EndpointRequest {
        EndpointRequest {
            id: id.to_owned(),
            name: id.to_owned(),
            rate: rate.to_owned(),
        }
    }

    fn prepared(inputs: &[&str], outputs: &[&str], capacity: &str) -> PreparedProblem {
        ProblemRequest {
            inputs: inputs
                .iter()
                .enumerate()
                .map(|(index, rate)| endpoint(&format!("input {index}"), rate))
                .collect(),
            outputs: outputs
                .iter()
                .enumerate()
                .map(|(index, rate)| endpoint(&format!("output {index}"), rate))
                .collect(),
            belt_rate: capacity.to_owned(),
        }
        .prepare()
        .unwrap()
    }

    fn validation(nodes: u32, links: u32, physical: u32, discard: u32) -> ValidationSummary {
        ValidationSummary {
            validator_version: 2,
            node_count: nodes,
            link_count: links,
            physical_link_count: physical,
            discard_link_count: discard,
            cyclic_scc_count: 0,
        }
    }

    #[test]
    fn discard_is_physical_but_excluded_from_presented_l() {
        let request = prepared(&["1", "1"], &["1"], "1");
        let best = BestKnownSolution {
            node_count: 0,
            link_count: 0,
            physical_link_count: 2,
            discard_link_count: 1,
            canonical_graph_key: CanonicalGraphKey::from_bytes(vec![1]),
            graph: PhysicalGraph {
                nodes: Vec::new(),
                links: vec![
                    PhysicalLink {
                        producer: ProducerPortRef::Input(InputTerminalIndex(0)),
                        consumer: ConsumerPortRef::Output(OutputTerminalIndex(0)),
                        flow: Rational::one(),
                    },
                    PhysicalLink {
                        producer: ProducerPortRef::Input(InputTerminalIndex(1)),
                        consumer: ConsumerPortRef::Discard(DiscardTerminalIndex(0)),
                        flow: Rational::one(),
                    },
                ],
            },
            validation: validation(0, 0, 2, 1),
        };
        let outcome = SolveResult::Incomplete(IncompleteResult {
            reason: IncompleteReason::Cancelled,
            best_known: Some(best),
            proof: ProofSummary::default(),
        });
        let PresentedSolveOutcome::Incomplete(presented) =
            present_solve_result(&request, &outcome).unwrap()
        else {
            panic!("expected incomplete outcome");
        };
        let solution = presented.best_known.unwrap();
        assert_eq!(solution.status, "best_known");
        assert_eq!(solution.stats.link_count, 0);
        assert_eq!(solution.stats.physical_link_count, 2);
        assert_eq!(solution.stats.discard_link_count, 1);
        assert_eq!(solution.stats.checked_through, None);
        assert_eq!(
            solution.edges.iter().filter(|edge| edge.discarded).count(),
            1
        );
    }

    #[test]
    fn storage_order_does_not_change_presentation() {
        let request = prepared(&["2"], &["1", "1"], "2");
        let node = PhysicalNode {
            id: NodeId(7),
            node_type: NodeType::Splitter2,
        };
        let links = vec![
            PhysicalLink {
                producer: ProducerPortRef::Input(InputTerminalIndex(0)),
                consumer: ConsumerPortRef::Node {
                    node: NodeId(7),
                    port: 0,
                },
                flow: Rational::from(2_u32),
            },
            PhysicalLink {
                producer: ProducerPortRef::Node {
                    node: NodeId(7),
                    port: 0,
                },
                consumer: ConsumerPortRef::Output(OutputTerminalIndex(0)),
                flow: Rational::one(),
            },
            PhysicalLink {
                producer: ProducerPortRef::Node {
                    node: NodeId(7),
                    port: 1,
                },
                consumer: ConsumerPortRef::Output(OutputTerminalIndex(1)),
                flow: Rational::one(),
            },
        ];
        let solution = |mut nodes: Vec<PhysicalNode>, mut links: Vec<PhysicalLink>| {
            nodes.reverse();
            links.reverse();
            SolveResult::Optimal(OptimalSolution {
                node_count: 1,
                link_count: 0,
                physical_link_count: 3,
                discard_link_count: 0,
                canonical_graph_key: CanonicalGraphKey::from_bytes(vec![1]),
                graph: PhysicalGraph { nodes, links },
                proof: ProofSummary::default(),
                validation: validation(1, 0, 3, 0),
            })
        };
        let left =
            present_solve_result(&request, &solution(vec![node.clone()], links.clone())).unwrap();
        let right = present_solve_result(&request, &solution(vec![node], links)).unwrap();
        assert_eq!(left, right);
    }

    #[test]
    fn decimal_preview_is_exact_integer_long_division() {
        assert_eq!(format_rate(&"1/3".parse().unwrap()).decimal, "0.333333");
        assert_eq!(format_rate(&"5/2".parse().unwrap()).decimal, "2.5");
    }

    #[test]
    fn natural_loop_marks_the_return_into_the_merger() {
        let merger = NodeId(0);
        let splitter = NodeId(1);
        let operators =
            BTreeMap::from([(merger, NodeType::Merger2), (splitter, NodeType::Splitter2)]);
        let links = vec![
            PhysicalLink {
                producer: ProducerPortRef::Input(InputTerminalIndex(0)),
                consumer: ConsumerPortRef::Node {
                    node: merger,
                    port: 0,
                },
                flow: Rational::one(),
            },
            PhysicalLink {
                producer: ProducerPortRef::Node {
                    node: merger,
                    port: 0,
                },
                consumer: ConsumerPortRef::Node {
                    node: splitter,
                    port: 0,
                },
                flow: Rational::one(),
            },
            PhysicalLink {
                producer: ProducerPortRef::Node {
                    node: splitter,
                    port: 0,
                },
                consumer: ConsumerPortRef::Node {
                    node: merger,
                    port: 1,
                },
                flow: Rational::one(),
            },
        ];

        let (feedback, count) = feedback_annotation(&operators, &links);

        assert_eq!(
            feedback,
            BTreeSet::from([(
                ProducerPortRef::Node {
                    node: splitter,
                    port: 0,
                },
                ConsumerPortRef::Node {
                    node: merger,
                    port: 1,
                },
            )])
        );
        assert_eq!(count, 1);
    }

    #[test]
    fn feedback_count_matches_the_number_of_annotated_feedback_belts() {
        let splitter = NodeId(0);
        let left_merger = NodeId(1);
        let right_merger = NodeId(2);
        let operators = BTreeMap::from([
            (splitter, NodeType::Splitter3),
            (left_merger, NodeType::Merger2),
            (right_merger, NodeType::Merger2),
        ]);
        let links = vec![
            PhysicalLink {
                producer: ProducerPortRef::Input(InputTerminalIndex(0)),
                consumer: ConsumerPortRef::Node {
                    node: splitter,
                    port: 0,
                },
                flow: Rational::one(),
            },
            PhysicalLink {
                producer: ProducerPortRef::Node {
                    node: splitter,
                    port: 0,
                },
                consumer: ConsumerPortRef::Node {
                    node: left_merger,
                    port: 0,
                },
                flow: Rational::one(),
            },
            PhysicalLink {
                producer: ProducerPortRef::Node {
                    node: left_merger,
                    port: 0,
                },
                consumer: ConsumerPortRef::Node {
                    node: splitter,
                    port: 0,
                },
                flow: Rational::one(),
            },
            PhysicalLink {
                producer: ProducerPortRef::Node {
                    node: splitter,
                    port: 1,
                },
                consumer: ConsumerPortRef::Node {
                    node: right_merger,
                    port: 0,
                },
                flow: Rational::one(),
            },
            PhysicalLink {
                producer: ProducerPortRef::Node {
                    node: right_merger,
                    port: 0,
                },
                consumer: ConsumerPortRef::Node {
                    node: splitter,
                    port: 1,
                },
                flow: Rational::one(),
            },
        ];

        let (feedback, count) = feedback_annotation(&operators, &links);

        assert_eq!(
            feedback,
            BTreeSet::from([
                (
                    ProducerPortRef::Node {
                        node: left_merger,
                        port: 0,
                    },
                    ConsumerPortRef::Node {
                        node: splitter,
                        port: 0,
                    },
                ),
                (
                    ProducerPortRef::Node {
                        node: right_merger,
                        port: 0,
                    },
                    ConsumerPortRef::Node {
                        node: splitter,
                        port: 1,
                    },
                ),
            ])
        );
        assert_eq!(count, 2);
    }

    #[test]
    fn irreducible_cycle_marks_a_dfs_ancestor_edge() {
        let a = NodeId(0);
        let b = NodeId(1);
        let c = NodeId(2);
        let d = NodeId(3);
        let operators = BTreeMap::from([
            (a, NodeType::Merger2),
            (b, NodeType::Splitter2),
            (c, NodeType::Merger2),
            (d, NodeType::Splitter2),
        ]);
        let links = vec![
            PhysicalLink {
                producer: ProducerPortRef::Input(InputTerminalIndex(0)),
                consumer: ConsumerPortRef::Node { node: a, port: 0 },
                flow: Rational::one(),
            },
            PhysicalLink {
                producer: ProducerPortRef::Input(InputTerminalIndex(1)),
                consumer: ConsumerPortRef::Node { node: c, port: 0 },
                flow: Rational::one(),
            },
            PhysicalLink {
                producer: ProducerPortRef::Node { node: a, port: 0 },
                consumer: ConsumerPortRef::Node { node: b, port: 0 },
                flow: Rational::one(),
            },
            PhysicalLink {
                producer: ProducerPortRef::Node { node: b, port: 0 },
                consumer: ConsumerPortRef::Node { node: c, port: 1 },
                flow: Rational::one(),
            },
            PhysicalLink {
                producer: ProducerPortRef::Node { node: c, port: 0 },
                consumer: ConsumerPortRef::Node { node: d, port: 0 },
                flow: Rational::one(),
            },
            PhysicalLink {
                producer: ProducerPortRef::Node { node: d, port: 0 },
                consumer: ConsumerPortRef::Node { node: a, port: 1 },
                flow: Rational::one(),
            },
        ];

        let (feedback, count) = feedback_annotation(&operators, &links);

        assert_eq!(
            feedback,
            BTreeSet::from([(
                ProducerPortRef::Node { node: d, port: 0 },
                ConsumerPortRef::Node { node: a, port: 1 },
            )])
        );
        assert_eq!(count, 1);
    }
}
