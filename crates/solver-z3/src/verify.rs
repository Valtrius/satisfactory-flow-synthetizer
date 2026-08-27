use std::collections::VecDeque;

use num::{BigRational, One, Zero};

use crate::model::{
    Candidate, Endpoint, OperatorKind, PORTS, Problem, Route, VerifiedCandidate, consumer_count,
    node_consumer, node_producer, output_consumer, producer_count,
};

pub(crate) fn from_problem(problem: &solver_api::Problem) -> Problem {
    let endpoints = |rates: &[solver_api::Rational]| {
        rates
            .iter()
            .map(|rate| Endpoint {
                rate: rate.as_big_rational().clone(),
            })
            .collect()
    };
    Problem {
        worker_count: std::thread::available_parallelism().map_or(1, std::num::NonZero::get),
        inputs: endpoints(&problem.inputs),
        outputs: endpoints(&problem.outputs),
        belt_rate: problem.max_link_rate.as_big_rational().clone(),
        discard_rate: (problem.total_input() - problem.total_output()).into_big_rational(),
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

pub(crate) fn build_solution(
    problem: &Problem,
    verified: &VerifiedCandidate,
) -> Result<solver_api::BestKnownSolution, crate::solver::SolveError> {
    use solver_api::{
        BestKnownSolution, ConsumerPortRef, DiscardTerminalIndex, InputTerminalIndex, NodeId,
        NodeType, OutputTerminalIndex, PhysicalGraph, PhysicalLink, PhysicalNode, ProducerPortRef,
        Rational,
    };
    let exact = solver_api::Problem {
        inputs: problem
            .inputs
            .iter()
            .map(|e| Rational::from(e.rate.clone()))
            .collect(),
        outputs: problem
            .outputs
            .iter()
            .map(|e| Rational::from(e.rate.clone()))
            .collect(),
        max_link_rate: Rational::from(problem.belt_rate.clone()),
    };
    let index = |v: usize| {
        u32::try_from(v)
            .map_err(|_| crate::solver::SolveError::Solver("graph index overflow".to_owned()))
    };
    let nodes = verified
        .candidate
        .node_types
        .iter()
        .enumerate()
        .map(|(i, kind)| {
            Ok(PhysicalNode {
                id: NodeId(index(i)?),
                node_type: match kind {
                    OperatorKind::Splitter2 => NodeType::Splitter2,
                    OperatorKind::Splitter3 => NodeType::Splitter3,
                    OperatorKind::Merger2 => NodeType::Merger2,
                    OperatorKind::Merger3 => NodeType::Merger3,
                },
            })
        })
        .collect::<Result<Vec<_>, crate::solver::SolveError>>()?;
    let mut links = Vec::new();
    let mut discard = 0;
    for (producer, route) in verified.candidate.routes.iter().enumerate() {
        let Some(route) = route else { continue };
        let source = if producer < problem.inputs.len() {
            ProducerPortRef::Input(InputTerminalIndex(index(producer)?))
        } else {
            let p = producer - problem.inputs.len();
            ProducerPortRef::Node {
                node: NodeId(index(p / PORTS)?),
                port: u8::try_from(p % PORTS).unwrap(),
            }
        };
        let target = match route {
            Route::Discard => {
                let d = discard;
                discard += 1;
                ConsumerPortRef::Discard(DiscardTerminalIndex(d))
            }
            Route::Consumer(c) if *c < nodes.len() * PORTS => ConsumerPortRef::Node {
                node: NodeId(index(c / PORTS)?),
                port: u8::try_from(c % PORTS).unwrap(),
            },
            Route::Consumer(c) => {
                ConsumerPortRef::Output(OutputTerminalIndex(index(c - nodes.len() * PORTS)?))
            }
        };
        let flow = verified.producer_rates[producer]
            .as_ref()
            .ok_or_else(|| crate::solver::SolveError::Solver("missing verified flow".to_owned()))?;
        links.push(PhysicalLink {
            producer: source,
            consumer: target,
            flow: Rational::from(flow.clone()),
        });
    }
    let graph = PhysicalGraph { nodes, links };
    let validation = solver_validation::validate_solution(&exact, &graph)
        .map_err(|e| crate::solver::SolveError::Solver(format!("validation failed: {e}")))?;
    Ok(BestKnownSolution {
        node_count: validation.node_count,
        link_count: validation.link_count,
        physical_link_count: validation.physical_link_count,
        discard_link_count: validation.discard_link_count,
        canonical_graph_key: solver_validation::layout_key(&exact, &graph),
        graph,
        validation,
    })
}

pub(crate) fn solution_identity(
    solution: &solver_api::BestKnownSolution,
) -> solver_api::CanonicalGraphKey {
    solution.canonical_graph_key.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
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
