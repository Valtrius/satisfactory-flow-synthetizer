use std::collections::VecDeque;

use num::{BigRational, One, Zero};

use crate::{
    format_rate,
    model::{
        Candidate, Endpoint, GraphEdge, GraphNode, NodeKind, OperatorKind, PORTS, Problem, Route,
        Solution, SolutionStats, SolveRequest, VerifiedCandidate, consumer_count, node_consumer,
        node_producer, output_consumer, producer_count,
    },
    parse_rate,
};

const MAX_ENDPOINTS_PER_SIDE: usize = 24;

#[derive(Debug)]
pub(crate) enum ProblemError {
    Invalid(String),
    Insufficient {
        available: Box<BigRational>,
        requested: Box<BigRational>,
    },
}

pub(crate) fn normalize_problem(request: &SolveRequest) -> Result<Problem, ProblemError> {
    if request.inputs.len() > MAX_ENDPOINTS_PER_SIDE
        || request.outputs.len() > MAX_ENDPOINTS_PER_SIDE
    {
        return Err(ProblemError::Invalid(format!(
            "at most {MAX_ENDPOINTS_PER_SIDE} inputs and outputs are allowed"
        )));
    }

    let belt_rate = parse_rate(&request.belt_rate)
        .map_err(|message| ProblemError::Invalid(format!("belt capacity: {message}")))?;
    if belt_rate <= BigRational::zero() {
        return Err(ProblemError::Invalid(
            "belt capacity must be greater than zero".to_owned(),
        ));
    }

    let outputs = normalize_endpoints(&request.outputs, "output", &belt_rate)?;
    let total_output = outputs.iter().fold(BigRational::zero(), |total, endpoint| {
        total + &endpoint.rate
    });
    if request.inputs.is_empty() && outputs.is_empty() {
        return Err(ProblemError::Invalid(
            "add at least one input or output".to_owned(),
        ));
    }
    let inputs = if request.inputs.is_empty() {
        automatic_inputs(&total_output, &belt_rate)
    } else {
        normalize_endpoints(&request.inputs, "input", &belt_rate)?
    };
    let total_input = inputs.iter().fold(BigRational::zero(), |total, endpoint| {
        total + &endpoint.rate
    });
    if total_output > total_input {
        return Err(ProblemError::Insufficient {
            available: Box::new(total_input),
            requested: Box::new(total_output),
        });
    }
    let discard_rate = &total_input - &total_output;

    Ok(Problem {
        inputs,
        outputs,
        belt_rate,
        total_input,
        total_output,
        discard_rate,
    })
}

fn automatic_inputs(total: &BigRational, belt_rate: &BigRational) -> Vec<Endpoint> {
    let mut remaining = total.clone();
    let mut rates = Vec::new();
    while remaining > BigRational::zero() {
        let rate = if remaining > *belt_rate {
            belt_rate.clone()
        } else {
            remaining.clone()
        };
        remaining -= &rate;
        rates.push(rate);
    }
    let input_count = rates.len();
    rates
        .into_iter()
        .enumerate()
        .map(|(index, rate)| Endpoint {
            name: if input_count == 1 {
                "Automatic supply".to_owned()
            } else {
                format!("Automatic supply {}", index + 1)
            },
            rate,
        })
        .collect()
}

fn normalize_endpoints(
    requests: &[crate::EndpointRequest],
    side: &str,
    belt_rate: &BigRational,
) -> Result<Vec<Endpoint>, ProblemError> {
    requests
        .iter()
        .enumerate()
        .map(|(index, endpoint)| {
            let rate = parse_rate(&endpoint.rate).map_err(|message| {
                ProblemError::Invalid(format!("{side} {}: {message}", index + 1))
            })?;
            if rate <= BigRational::zero() {
                return Err(ProblemError::Invalid(format!(
                    "{side} {} rate must be greater than zero",
                    index + 1
                )));
            }
            if &rate > belt_rate {
                return Err(ProblemError::Invalid(format!(
                    "{side} {} rate exceeds the belt capacity",
                    index + 1
                )));
            }
            let name = endpoint.name.trim();
            if name.chars().count() > 80 {
                return Err(ProblemError::Invalid(format!(
                    "{side} {} name is too long",
                    index + 1
                )));
            }
            Ok(Endpoint {
                name: if name.is_empty() {
                    format!("{} {}", title_case(side), index + 1)
                } else {
                    name.to_owned()
                },
                rate,
            })
        })
        .collect()
}

fn title_case(value: &str) -> String {
    let mut characters = value.chars();
    match characters.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
    }
}

#[allow(clippy::too_many_lines)]
pub(crate) fn verify_candidate(
    problem: &Problem,
    candidate: Candidate,
) -> Result<VerifiedCandidate, String> {
    let node_count = candidate.node_types.len();
    let producer_total = producer_count(problem.inputs.len(), node_count);
    let consumer_total = consumer_count(problem.outputs.len(), node_count);
    if candidate.routes.len() != producer_total {
        return Err("candidate has the wrong producer count".to_owned());
    }

    let mut predecessor = vec![None; consumer_total];
    for producer in 0..producer_total {
        let active = producer_is_active(
            problem.inputs.len(),
            candidate.node_types.as_slice(),
            producer,
        );
        match (&candidate.routes[producer], active) {
            (Some(Route::Consumer(consumer)), true) => {
                if *consumer >= consumer_total {
                    return Err("route points outside the consumer table".to_owned());
                }
                if predecessor[*consumer].replace(producer).is_some() {
                    return Err("consumer has more than one incoming belt".to_owned());
                }
            }
            (Some(Route::Discard), true) | (None, false) => {}
            (None, true) => return Err("active producer has no destination".to_owned()),
            (Some(_), false) => return Err("inactive producer has a destination".to_owned()),
        }
    }

    for node in 0..node_count {
        let kind = candidate.node_types[node];
        for port in 0..PORTS {
            let incoming = predecessor[node_consumer(node, port)].is_some();
            if incoming != (port < kind.input_count()) {
                return Err("operator input ports do not match its type".to_owned());
            }
        }
    }
    for output in 0..problem.outputs.len() {
        if predecessor[output_consumer(node_count, output)].is_none() {
            return Err("requested output has no incoming belt".to_owned());
        }
    }

    validate_reachability(problem, &candidate, predecessor.as_slice())?;

    let mut unknown_for_producer = vec![None; producer_total];
    let mut unknown_producers = Vec::new();
    for node in 0..node_count {
        for port in 0..candidate.node_types[node].output_count() {
            let producer = node_producer(problem.inputs.len(), node, port);
            unknown_for_producer[producer] = Some(unknown_producers.len());
            unknown_producers.push(producer);
        }
    }

    let unknown_count = unknown_producers.len();
    let mut matrix = vec![vec![BigRational::zero(); unknown_count + 1]; unknown_count];
    let mut row = 0;
    for node in 0..node_count {
        let kind = candidate.node_types[node];
        match kind {
            OperatorKind::Splitter2 | OperatorKind::Splitter3 => {
                let divisor = BigRational::from_integer(kind.output_count().into());
                let factor = BigRational::one() / divisor;
                let input = predecessor[node_consumer(node, 0)].expect("checked above");
                for port in 0..kind.output_count() {
                    let output = node_producer(problem.inputs.len(), node, port);
                    add_output_coefficient(
                        matrix[row].as_mut_slice(),
                        unknown_for_producer.as_slice(),
                        output,
                    );
                    subtract_input(
                        matrix[row].as_mut_slice(),
                        unknown_for_producer.as_slice(),
                        problem,
                        input,
                        &factor,
                    );
                    row += 1;
                }
            }
            OperatorKind::Merger2 | OperatorKind::Merger3 => {
                let output = node_producer(problem.inputs.len(), node, 0);
                add_output_coefficient(
                    matrix[row].as_mut_slice(),
                    unknown_for_producer.as_slice(),
                    output,
                );
                for port in 0..kind.input_count() {
                    let input = predecessor[node_consumer(node, port)].expect("checked above");
                    subtract_input(
                        matrix[row].as_mut_slice(),
                        unknown_for_producer.as_slice(),
                        problem,
                        input,
                        &BigRational::one(),
                    );
                }
                row += 1;
            }
        }
    }
    if row != unknown_count {
        return Err("operator equations are not square".to_owned());
    }

    let solution = solve_linear_system(matrix)?;
    let mut producer_rates = vec![None; producer_total];
    for (input, endpoint) in problem.inputs.iter().enumerate() {
        producer_rates[input] = Some(endpoint.rate.clone());
    }
    for (unknown, producer) in unknown_producers.into_iter().enumerate() {
        let rate = solution[unknown].clone();
        if rate <= BigRational::zero() {
            return Err("steady flow is not positive".to_owned());
        }
        if rate > problem.belt_rate {
            return Err("an internal belt exceeds the configured capacity".to_owned());
        }
        producer_rates[producer] = Some(rate);
    }

    for (output, endpoint) in problem.outputs.iter().enumerate() {
        let producer = predecessor[output_consumer(node_count, output)].expect("checked above");
        if producer_rates[producer].as_ref() != Some(&endpoint.rate) {
            return Err("resolved output rate does not match its target".to_owned());
        }
    }
    let discarded = candidate
        .routes
        .iter()
        .enumerate()
        .filter(|(_, route)| matches!(route, Some(Route::Discard)))
        .fold(BigRational::zero(), |total, (producer, _)| {
            total
                + producer_rates[producer]
                    .as_ref()
                    .expect("active route has a rate")
        });
    if discarded != problem.discard_rate {
        return Err("discarded steady flow does not match the surplus".to_owned());
    }

    Ok(VerifiedCandidate {
        candidate,
        producer_rates,
    })
}

fn producer_is_active(inputs: usize, node_types: &[OperatorKind], producer: usize) -> bool {
    if producer < inputs {
        return true;
    }
    let offset = producer - inputs;
    let node = offset / PORTS;
    let port = offset % PORTS;
    port < node_types[node].output_count()
}

fn validate_reachability(
    problem: &Problem,
    candidate: &Candidate,
    predecessor: &[Option<usize>],
) -> Result<(), String> {
    let node_count = candidate.node_types.len();
    let mut forward = vec![Vec::new(); node_count];
    let mut reverse = vec![Vec::new(); node_count];
    let mut source_reachable = vec![false; node_count];
    let mut terminal_reachable = vec![false; node_count];

    for node in 0..node_count {
        let kind = candidate.node_types[node];
        for port in 0..kind.input_count() {
            let producer = predecessor[node_consumer(node, port)].expect("checked above");
            if producer < problem.inputs.len() {
                source_reachable[node] = true;
            } else {
                let source_node = (producer - problem.inputs.len()) / PORTS;
                forward[source_node].push(node);
                reverse[node].push(source_node);
            }
        }
        for port in 0..kind.output_count() {
            let producer = node_producer(problem.inputs.len(), node, port);
            match candidate.routes[producer] {
                Some(Route::Discard) => terminal_reachable[node] = true,
                Some(Route::Consumer(consumer)) if consumer >= node_count * PORTS => {
                    terminal_reachable[node] = true;
                }
                _ => {}
            }
        }
    }

    spread_reachability(forward.as_slice(), &mut source_reachable);
    spread_reachability(reverse.as_slice(), &mut terminal_reachable);
    if source_reachable.iter().any(|reachable| !reachable) {
        return Err("an operator is not reachable from an input".to_owned());
    }
    if terminal_reachable.iter().any(|reachable| !reachable) {
        return Err("a feedback component has no path to an output or discard".to_owned());
    }
    Ok(())
}

fn spread_reachability(adjacency: &[Vec<usize>], reachable: &mut [bool]) {
    let mut queue = reachable
        .iter()
        .enumerate()
        .filter_map(|(index, value)| value.then_some(index))
        .collect::<VecDeque<_>>();
    while let Some(node) = queue.pop_front() {
        for &next in &adjacency[node] {
            if !reachable[next] {
                reachable[next] = true;
                queue.push_back(next);
            }
        }
    }
}

fn add_output_coefficient(row: &mut [BigRational], unknowns: &[Option<usize>], producer: usize) {
    let column = unknowns[producer].expect("operator output is an unknown");
    row[column] += BigRational::one();
}

fn subtract_input(
    row: &mut [BigRational],
    unknowns: &[Option<usize>],
    problem: &Problem,
    producer: usize,
    factor: &BigRational,
) {
    if producer < problem.inputs.len() {
        let right_hand_side = row.len() - 1;
        row[right_hand_side] += factor * &problem.inputs[producer].rate;
    } else {
        let column = unknowns[producer].expect("operator input producer is active");
        row[column] -= factor;
    }
}

fn solve_linear_system(mut matrix: Vec<Vec<BigRational>>) -> Result<Vec<BigRational>, String> {
    let size = matrix.len();
    if size == 0 {
        return Ok(Vec::new());
    }

    let mut pivot_row = 0;
    for column in 0..size {
        let Some(found) = (pivot_row..size).find(|&row| !matrix[row][column].is_zero()) else {
            return Err("feedback equations do not have one unique steady flow".to_owned());
        };
        matrix.swap(pivot_row, found);
        let pivot = matrix[pivot_row][column].clone();
        for value in &mut matrix[pivot_row][column..=size] {
            *value /= &pivot;
        }
        for row in 0..size {
            if row == pivot_row || matrix[row][column].is_zero() {
                continue;
            }
            let factor = matrix[row][column].clone();
            let pivot_values = matrix[pivot_row][column..=size].to_vec();
            for (value, pivot_value) in matrix[row][column..=size]
                .iter_mut()
                .zip(pivot_values.iter())
            {
                *value -= &factor * pivot_value;
            }
        }
        pivot_row += 1;
    }
    Ok((0..size).map(|row| matrix[row][size].clone()).collect())
}

#[allow(clippy::too_many_lines)]
pub(crate) fn build_solution(problem: &Problem, verified: &VerifiedCandidate) -> Solution {
    let node_count = verified.candidate.node_types.len();
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut discarded_streams = Vec::new();

    for (index, endpoint) in problem.inputs.iter().enumerate() {
        nodes.push(GraphNode {
            id: input_node_id(index),
            kind: NodeKind::Input,
            label: format!(
                "{} · {} /min",
                endpoint.name,
                format_rate(&endpoint.rate).exact
            ),
        });
    }
    for (index, kind) in verified.candidate.node_types.iter().copied().enumerate() {
        nodes.push(GraphNode {
            id: operator_node_id(index),
            kind: kind.public_kind(),
            label: operator_label(index, kind),
        });
    }
    for (index, endpoint) in problem.outputs.iter().enumerate() {
        nodes.push(GraphNode {
            id: output_node_id(index),
            kind: NodeKind::Output,
            label: format!(
                "{} · {} /min",
                endpoint.name,
                format_rate(&endpoint.rate).exact
            ),
        });
    }

    let feedback_producers = feedback_producers(problem, &verified.candidate);
    for (producer, route) in verified.candidate.routes.iter().enumerate() {
        let Some(route) = route else { continue };
        let rate = verified.producer_rates[producer]
            .as_ref()
            .expect("active producer has a verified rate")
            .clone();
        let source = producer_source(problem.inputs.len(), producer);
        match route {
            Route::Discard => discarded_streams.push(Stream {
                source: source.0,
                source_port: source.1,
                rate,
            }),
            Route::Consumer(consumer) => {
                let (target, target_port) = consumer_target(node_count, *consumer);
                let feedback = feedback_producers[producer];
                push_edge(
                    &mut edges,
                    source.0,
                    target,
                    source.1,
                    target_port,
                    &rate,
                    feedback,
                    false,
                );
            }
        }
    }

    append_discard_sinks(&mut nodes, &mut edges, discarded_streams);
    let splitters = verified
        .candidate
        .node_types
        .iter()
        .filter(|kind| matches!(kind, OperatorKind::Splitter2 | OperatorKind::Splitter3))
        .count();
    let mergers = node_count - splitters;
    let feedback_loops = feedback_producers.iter().filter(|flag| **flag).count();
    let build_steps = build_steps(nodes.as_slice(), edges.as_slice());
    let (belt_count, internal_max_throughput) = operator_belt_metrics(&nodes, &edges);

    Solution {
        status: "proven_optimal".to_owned(),
        model_version: 3,
        stats: SolutionStats {
            node_count,
            splitters,
            mergers,
            feedback_loops,
            checked_through: node_count,
            belt_count,
            internal_max_throughput,
        },
        total_input: format_rate(&problem.total_input),
        total_output: format_rate(&problem.total_output),
        discard_rate: format_rate(&problem.discard_rate),
        belt_rate: format_rate(&problem.belt_rate),
        nodes,
        edges,
        build_steps,
    }
}

struct Stream {
    source: String,
    source_port: usize,
    rate: BigRational,
}

fn append_discard_sinks(
    nodes: &mut Vec<GraphNode>,
    edges: &mut Vec<GraphEdge>,
    streams: Vec<Stream>,
) {
    let sink_count = streams.len();
    for (index, stream) in streams.into_iter().enumerate() {
        let sink_id = format!("sink-{index}");
        let sink_label = if sink_count == 1 {
            format!("Sink · {} /min", format_rate(&stream.rate).exact)
        } else {
            format!(
                "Sink {} · {} /min",
                index + 1,
                format_rate(&stream.rate).exact
            )
        };
        nodes.push(GraphNode {
            id: sink_id.clone(),
            kind: NodeKind::Discard,
            label: sink_label,
        });
        push_edge(
            edges,
            stream.source,
            sink_id,
            stream.source_port,
            0,
            &stream.rate,
            false,
            true,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn push_edge(
    edges: &mut Vec<GraphEdge>,
    source: String,
    target: String,
    source_port: usize,
    target_port: usize,
    rate: &BigRational,
    feedback: bool,
    discarded: bool,
) {
    edges.push(GraphEdge {
        id: format!("edge-{}", edges.len()),
        source,
        target,
        source_port,
        target_port,
        rate: format_rate(rate),
        feedback,
        discarded,
    });
}

fn operator_links(problem: &Problem, candidate: &Candidate) -> Vec<(usize, usize, usize)> {
    let node_count = candidate.node_types.len();
    let mut links = Vec::new();
    for node in 0..node_count {
        for port in 0..candidate.node_types[node].output_count() {
            let producer = node_producer(problem.inputs.len(), node, port);
            if let Some(Route::Consumer(consumer)) = candidate.routes[producer]
                && let Some(target) = consumer_operator(node_count, consumer)
            {
                links.push((node, target, producer));
            }
        }
    }
    links
}

fn input_entries(problem: &Problem, candidate: &Candidate) -> Vec<usize> {
    let node_count = candidate.node_types.len();
    let mut entries = Vec::new();
    for route in candidate.routes.iter().take(problem.inputs.len()) {
        if let Some(Route::Consumer(consumer)) = route
            && let Some(target) = consumer_operator(node_count, *consumer)
        {
            entries.push(target);
        }
    }
    entries.sort_unstable();
    entries.dedup();
    entries
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
    // Natural-loop back-edges are already marked. This DFS is only for leftover
    // irreducible cycles, where neither side dominates the other.
    let mut adjacency = vec![Vec::new(); node_count];
    for &(source, target, producer) in links {
        if !feedback[producer] {
            adjacency[source].push((target, producer));
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
                let (target, producer) = adjacency[node][child];
                match color[target] {
                    DfsColor::Grey => feedback[producer] = true,
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

fn feedback_producers(problem: &Problem, candidate: &Candidate) -> Vec<bool> {
    let node_count = candidate.node_types.len();
    let links = operator_links(problem, candidate);
    let entries = input_entries(problem, candidate);
    let mut successors = vec![Vec::new(); node_count];
    for &(source, target, _) in &links {
        successors[source].push(target);
    }
    let idom = immediate_dominators(node_count, &successors, &entries);
    let source = node_count;
    let mut feedback = vec![false; candidate.routes.len()];
    for &(node, target, producer) in &links {
        if dominates(&idom, target, node, source) {
            feedback[producer] = true;
        }
    }
    mark_irreducible_back_edges(node_count, &links, &entries, &mut feedback);
    feedback
}

fn producer_source(inputs: usize, producer: usize) -> (String, usize) {
    if producer < inputs {
        (input_node_id(producer), 0)
    } else {
        let offset = producer - inputs;
        (operator_node_id(offset / PORTS), offset % PORTS)
    }
}

fn consumer_target(node_count: usize, consumer: usize) -> (String, usize) {
    if consumer < node_count * PORTS {
        (operator_node_id(consumer / PORTS), consumer % PORTS)
    } else {
        (output_node_id(consumer - node_count * PORTS), 0)
    }
}

fn consumer_operator(node_count: usize, consumer: usize) -> Option<usize> {
    (consumer < node_count * PORTS).then_some(consumer / PORTS)
}

fn input_node_id(index: usize) -> String {
    format!("input-{index}")
}

fn output_node_id(index: usize) -> String {
    format!("output-{index}")
}

fn operator_node_id(index: usize) -> String {
    format!("operator-{index}")
}

const fn is_operator_kind(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::Splitter2 | NodeKind::Splitter3 | NodeKind::Merger2 | NodeKind::Merger3
    )
}

/// Belts that connect two operators (includes feedback; excludes I/O stubs and discard).
fn operator_belt_metrics(nodes: &[GraphNode], edges: &[GraphEdge]) -> (usize, crate::DisplayRate) {
    let operator_ids = nodes
        .iter()
        .filter(|node| is_operator_kind(node.kind))
        .map(|node| node.id.as_str())
        .collect::<std::collections::HashSet<_>>();
    let mut count = 0;
    let mut max_rate = BigRational::zero();
    for edge in edges {
        if edge.discarded {
            continue;
        }
        if !operator_ids.contains(edge.source.as_str())
            || !operator_ids.contains(edge.target.as_str())
        {
            continue;
        }
        count += 1;
        if let Ok(rate) = parse_rate(&edge.rate.exact)
            && rate > max_rate
        {
            max_rate = rate;
        }
    }
    (count, format_rate(&max_rate))
}

/// Stable identity for deduplicating isomorphic layouts under node relabeling.
pub(crate) fn solution_identity(solution: &Solution) -> String {
    let index_of = solution
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.id.as_str(), index))
        .collect::<std::collections::HashMap<_, _>>();
    let n = solution.nodes.len();
    let mut colors = solution
        .nodes
        .iter()
        .map(|node| kind_color(node.kind))
        .collect::<Vec<_>>();

    let mut adjacency = vec![Vec::<(usize, u64)>::new(); n];
    for edge in &solution.edges {
        let Some(&src) = index_of.get(edge.source.as_str()) else {
            continue;
        };
        let Some(&tgt) = index_of.get(edge.target.as_str()) else {
            continue;
        };
        let edge_tag = edge_color(edge);
        adjacency[src].push((tgt, edge_tag));
        adjacency[tgt].push((src, edge_tag.wrapping_mul(0x9e37_79b9).wrapping_add(1)));
    }

    for _ in 0..n.max(1) {
        let mut next = vec![0_u64; n];
        for node in 0..n {
            let mut parts = adjacency[node]
                .iter()
                .map(|(neighbor, tag)| {
                    let mut h = colors[*neighbor];
                    h ^= tag.wrapping_mul(0xbf58_476d_1ce4_e5b9);
                    h
                })
                .collect::<Vec<_>>();
            parts.sort_unstable();
            let mut h = colors[node];
            for part in parts {
                h = h
                    .wrapping_mul(0x94d0_49bb_1331_11eb)
                    .wrapping_add(part)
                    .wrapping_add(1);
            }
            next[node] = h;
        }
        if next == colors {
            break;
        }
        colors = next;
    }

    let mut edge_keys = solution
        .edges
        .iter()
        .filter_map(|edge| {
            let src = *index_of.get(edge.source.as_str())?;
            let tgt = *index_of.get(edge.target.as_str())?;
            Some(format!(
                "{}:{}->{}:{}:{}:{}:{}",
                colors[src],
                edge.source_port,
                colors[tgt],
                edge.target_port,
                edge.feedback,
                edge.discarded,
                edge.rate.exact
            ))
        })
        .collect::<Vec<_>>();
    edge_keys.sort_unstable();
    edge_keys.join("|")
}

const fn kind_color(kind: NodeKind) -> u64 {
    match kind {
        NodeKind::Input => 1,
        NodeKind::Splitter2 => 2,
        NodeKind::Splitter3 => 3,
        NodeKind::Merger2 => 4,
        NodeKind::Merger3 => 5,
        NodeKind::Output => 6,
        NodeKind::Discard => 7,
    }
}

fn edge_color(edge: &GraphEdge) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325_u64;
    h ^= u64::try_from(edge.source_port).unwrap_or(u64::MAX);
    h = h.wrapping_mul(0x100_0000_01b3);
    h ^= u64::try_from(edge.target_port).unwrap_or(u64::MAX);
    h = h.wrapping_mul(0x100_0000_01b3);
    h ^= u64::from(u8::from(edge.feedback));
    h = h.wrapping_mul(0x100_0000_01b3);
    h ^= u64::from(u8::from(edge.discarded));
    h = h.wrapping_mul(0x100_0000_01b3);
    for byte in edge.rate.exact.bytes() {
        h ^= u64::from(byte);
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

fn operator_label(index: usize, kind: OperatorKind) -> String {
    match kind {
        OperatorKind::Splitter2 => format!("Splitter {} · 2-way", index + 1),
        OperatorKind::Splitter3 => format!("Splitter {} · 3-way", index + 1),
        OperatorKind::Merger2 => format!("Merger {} · 2-way", index + 1),
        OperatorKind::Merger3 => format!("Merger {} · 3-way", index + 1),
    }
}

fn build_steps(nodes: &[GraphNode], edges: &[GraphEdge]) -> Vec<String> {
    nodes
        .iter()
        .filter(|node| {
            matches!(
                node.kind,
                NodeKind::Splitter2 | NodeKind::Splitter3 | NodeKind::Merger2 | NodeKind::Merger3
            )
        })
        .map(|node| {
            let incoming = edges
                .iter()
                .filter(|edge| edge.target == node.id)
                .map(|edge| {
                    format!(
                        "{} → input {} at {}/min",
                        edge.source,
                        edge.target_port + 1,
                        edge.rate.exact
                    )
                })
                .collect::<Vec<_>>()
                .join("; ");
            let outgoing = edges
                .iter()
                .filter(|edge| edge.source == node.id)
                .map(|edge| {
                    format!(
                        "output {} → {} at {}/min",
                        edge.source_port + 1,
                        edge.target,
                        edge.rate.exact
                    )
                })
                .collect::<Vec<_>>()
                .join("; ");
            format!("{}: {}. {}.", node.label, incoming, outgoing)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EndpointRequest;

    struct Wiring {
        inputs: usize,
        kinds: Vec<OperatorKind>,
        out_port: Vec<usize>,
        in_port: Vec<usize>,
        routes: Vec<Option<Route>>,
    }

    impl Wiring {
        fn new(inputs: usize, kinds: Vec<OperatorKind>) -> Self {
            let nodes = kinds.len();
            Self {
                inputs,
                out_port: vec![0; nodes],
                in_port: vec![0; nodes],
                routes: vec![None; producer_count(inputs, nodes)],
                kinds,
            }
        }

        fn feed(mut self, input: usize, destination: usize) -> Self {
            let port = self.take_in(destination);
            self.routes[input] = Some(Route::Consumer(node_consumer(destination, port)));
            self
        }

        fn belt(mut self, source: usize, destination: usize) -> Self {
            let out = self.take_out(source);
            let input = self.take_in(destination);
            let producer = node_producer(self.inputs, source, out);
            self.routes[producer] = Some(Route::Consumer(node_consumer(destination, input)));
            self
        }

        fn finish(self) -> Candidate {
            Candidate {
                node_types: self.kinds,
                routes: self.routes,
            }
        }

        fn take_in(&mut self, node: usize) -> usize {
            let port = self.in_port[node];
            self.in_port[node] += 1;
            port
        }

        fn take_out(&mut self, node: usize) -> usize {
            let port = self.out_port[node];
            self.out_port[node] += 1;
            port
        }
    }

    fn problem_with_inputs(count: usize) -> Problem {
        let inputs = (0..count)
            .map(|index| EndpointRequest {
                id: format!("in{index}"),
                name: format!("in{index}"),
                rate: "1".to_owned(),
            })
            .collect();
        normalize_problem(&SolveRequest {
            inputs,
            outputs: vec![EndpointRequest {
                id: "out".to_owned(),
                name: "out".to_owned(),
                rate: "1".to_owned(),
            }],
            belt_rate: "1200".to_owned(),
            enumerate_all_at_n: false,
        })
        .expect("fixture problem")
    }

    fn operator_feedback_pairs(problem: &Problem, candidate: &Candidate) -> Vec<(usize, usize)> {
        let flags = feedback_producers(problem, candidate);
        let node_count = candidate.node_types.len();
        let mut pairs = Vec::new();
        for node in 0..node_count {
            for port in 0..candidate.node_types[node].output_count() {
                let producer = node_producer(problem.inputs.len(), node, port);
                if flags[producer]
                    && let Some(Route::Consumer(consumer)) = &candidate.routes[producer]
                    && let Some(target) = consumer_operator(node_count, *consumer)
                {
                    pairs.push((node, target));
                }
            }
        }
        pairs.sort_unstable();
        pairs
    }

    const SCREENSHOT_M3: usize = 0;
    const SCREENSHOT_S4: usize = 1;
    const SCREENSHOT_S5: usize = 2;
    const SCREENSHOT_S1: usize = 3;
    const SCREENSHOT_S6: usize = 4;
    const SCREENSHOT_M4: usize = 5;
    const SCREENSHOT_S2: usize = 6;
    const SCREENSHOT_S3: usize = 7;

    fn screenshot_candidate(perm: [usize; 8]) -> Candidate {
        let logical_kinds = [
            OperatorKind::Merger3,
            OperatorKind::Splitter3,
            OperatorKind::Splitter3,
            OperatorKind::Splitter2,
            OperatorKind::Splitter3,
            OperatorKind::Merger3,
            OperatorKind::Splitter2,
            OperatorKind::Splitter2,
        ];
        let mut kinds = vec![OperatorKind::Splitter2; 8];
        for (logical, kind) in logical_kinds.into_iter().enumerate() {
            kinds[perm[logical]] = kind;
        }
        let at = |logical: usize| perm[logical];
        Wiring::new(1, kinds)
            .feed(0, at(SCREENSHOT_M3))
            .belt(at(SCREENSHOT_M3), at(SCREENSHOT_S4))
            .belt(at(SCREENSHOT_S4), at(SCREENSHOT_S5))
            .belt(at(SCREENSHOT_S4), at(SCREENSHOT_M4))
            .belt(at(SCREENSHOT_S5), at(SCREENSHOT_S1))
            .belt(at(SCREENSHOT_S5), at(SCREENSHOT_M4))
            .belt(at(SCREENSHOT_S1), at(SCREENSHOT_S6))
            .belt(at(SCREENSHOT_S1), at(SCREENSHOT_M3))
            .belt(at(SCREENSHOT_S6), at(SCREENSHOT_M4))
            .belt(at(SCREENSHOT_M4), at(SCREENSHOT_S2))
            .belt(at(SCREENSHOT_S2), at(SCREENSHOT_S3))
            .belt(at(SCREENSHOT_S3), at(SCREENSHOT_M3))
            .finish()
    }

    fn assert_screenshot_marks(perm: [usize; 8]) {
        let problem = problem_with_inputs(1);
        let candidate = screenshot_candidate(perm);
        let pairs = operator_feedback_pairs(&problem, &candidate);
        let mut expected = vec![
            (perm[SCREENSHOT_S1], perm[SCREENSHOT_M3]),
            (perm[SCREENSHOT_S3], perm[SCREENSHOT_M3]),
        ];
        expected.sort_unstable();
        assert_eq!(pairs, expected);
        assert_eq!(pairs.len(), 2);
    }

    #[test]
    fn screenshot_topology_marks_returns_into_the_header() {
        assert_screenshot_marks([0, 1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn screenshot_marks_are_stable_after_relabeling() {
        let mut perm = [0, 1, 2, 3, 4, 5, 6, 7];
        perm[SCREENSHOT_M3] = SCREENSHOT_S3;
        perm[SCREENSHOT_S3] = SCREENSHOT_M3;
        assert_screenshot_marks(perm);
    }

    #[test]
    fn natural_loop_marks_the_return_into_the_merger() {
        let problem = problem_with_inputs(1);
        let candidate = Wiring::new(1, vec![OperatorKind::Merger2, OperatorKind::Splitter2])
            .feed(0, 0)
            .belt(0, 1)
            .belt(1, 0)
            .finish();
        assert_eq!(operator_feedback_pairs(&problem, &candidate), vec![(1, 0)]);
    }

    #[test]
    fn nested_loops_mark_both_headers() {
        let problem = problem_with_inputs(1);
        let candidate = Wiring::new(
            1,
            vec![
                OperatorKind::Merger2,
                OperatorKind::Splitter2,
                OperatorKind::Merger2,
                OperatorKind::Splitter2,
                OperatorKind::Splitter2,
            ],
        )
        .feed(0, 0)
        .belt(0, 1)
        .belt(1, 2)
        .belt(1, 4)
        .belt(2, 3)
        .belt(3, 2)
        .belt(3, 4)
        .belt(4, 0)
        .finish();
        assert_eq!(
            operator_feedback_pairs(&problem, &candidate),
            vec![(3, 2), (4, 0)]
        );
    }

    #[test]
    fn irreducible_cycle_marks_a_dfs_ancestor_edge() {
        let problem = problem_with_inputs(2);
        let candidate = Wiring::new(
            2,
            vec![
                OperatorKind::Merger2,
                OperatorKind::Splitter2,
                OperatorKind::Merger2,
                OperatorKind::Splitter2,
            ],
        )
        .feed(0, 0)
        .feed(1, 2)
        .belt(0, 1)
        .belt(1, 2)
        .belt(2, 3)
        .belt(3, 0)
        .finish();
        let pairs = operator_feedback_pairs(&problem, &candidate);
        assert_eq!(pairs, vec![(3, 0)]);
        let flags = feedback_producers(&problem, &candidate);
        assert!(!flags[0]);
        assert!(!flags[1]);
    }

    #[test]
    fn acyclic_split_marks_nothing() {
        let problem = problem_with_inputs(1);
        let candidate = Wiring::new(1, vec![OperatorKind::Splitter2])
            .feed(0, 0)
            .finish();
        assert!(operator_feedback_pairs(&problem, &candidate).is_empty());
    }

    #[test]
    fn solves_a_unique_feedback_system() {
        let matrix = vec![
            vec![
                BigRational::one(),
                BigRational::new((-1).into(), 2.into()),
                BigRational::one(),
            ],
            vec![BigRational::zero(), BigRational::one(), BigRational::one()],
        ];
        let solution = solve_linear_system(matrix).unwrap();
        assert_eq!(solution.len(), 2);
    }

    #[test]
    fn rejects_a_singular_feedback_system() {
        let matrix = vec![
            vec![BigRational::one(), BigRational::one(), BigRational::one()],
            vec![BigRational::one(), BigRational::one(), BigRational::one()],
        ];
        assert!(solve_linear_system(matrix).is_err());
    }
}
