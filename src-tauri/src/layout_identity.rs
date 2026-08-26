//! One authoritative, flow-independent layout identity for both engine adapters.

use std::collections::HashMap;

use solver_api::{
    ConsumerPortRef, DiscardTerminalIndex, InputTerminalIndex, NodeId, NodeType,
    OutputTerminalIndex, PhysicalGraph, PhysicalLink, PhysicalNode, Problem, ProducerPortRef,
    Rational,
};
use solver_core::canonical::canonicalize_effective_layout;

use crate::contract::{GraphNodeKind, Solution, SolveRequest};

pub(crate) fn attach(request: &SolveRequest, solution: &mut Solution) -> Result<(), String> {
    let problem = problem(request)?;
    let graph = physical_graph(&problem, solution)?;
    solution.set_layout_key(canonicalize_effective_layout(&problem, &graph));
    Ok(())
}

fn problem(request: &SolveRequest) -> Result<Problem, String> {
    let max_link_rate = parse_rate("belt", "beltRate", &request.belt_rate)?;
    if !max_link_rate.is_positive() {
        return Err("belt rate must be greater than zero".to_owned());
    }
    let outputs = request
        .outputs
        .iter()
        .map(|endpoint| parse_rate("output", &endpoint.id, &endpoint.rate))
        .collect::<Result<Vec<_>, _>>()?;
    let inputs = if request.inputs.is_empty() {
        let total_output = outputs.iter().cloned().sum::<Rational>();
        automatic_input_rates(&total_output, &max_link_rate)
    } else {
        request
            .inputs
            .iter()
            .map(|endpoint| parse_rate("input", &endpoint.id, &endpoint.rate))
            .collect::<Result<Vec<_>, _>>()?
    };
    Ok(Problem {
        inputs,
        outputs,
        max_link_rate,
    })
}

fn automatic_input_rates(total: &Rational, max_link_rate: &Rational) -> Vec<Rational> {
    let mut remaining = total.clone();
    let mut inputs = Vec::new();
    while remaining.is_positive() {
        let rate = if remaining > *max_link_rate {
            max_link_rate.clone()
        } else {
            remaining.clone()
        };
        remaining = remaining - &rate;
        inputs.push(rate);
    }
    inputs
}

fn parse_rate(kind: &str, id: &str, rate: &str) -> Result<Rational, String> {
    rate.parse()
        .map_err(|error| format!("invalid {kind} rate for {id}: {error}"))
}

fn physical_graph(problem: &Problem, solution: &Solution) -> Result<PhysicalGraph, String> {
    let input_ids = terminal_ids(
        solution,
        GraphNodeKind::Input,
        problem.inputs.len(),
        "input",
    )?;
    let output_ids = terminal_ids(
        solution,
        GraphNodeKind::Output,
        problem.outputs.len(),
        "output",
    )?;

    let mut operator_ids = HashMap::new();
    let mut nodes = Vec::new();
    let mut discard_ids = HashMap::new();
    for node in &solution.nodes {
        match node_type(node.kind) {
            Some(node_type) => {
                let id = NodeId(
                    u32::try_from(nodes.len())
                        .map_err(|_| "operator count does not fit u32".to_owned())?,
                );
                if operator_ids.insert(node.id.as_str(), id).is_some() {
                    return Err(format!("duplicate operator id {}", node.id));
                }
                nodes.push(PhysicalNode { id, node_type });
            }
            None if node.kind == GraphNodeKind::Discard => {
                let id = DiscardTerminalIndex(
                    u32::try_from(discard_ids.len())
                        .map_err(|_| "discard count does not fit u32".to_owned())?,
                );
                if discard_ids.insert(node.id.as_str(), id).is_some() {
                    return Err(format!("duplicate discard id {}", node.id));
                }
            }
            None => {}
        }
    }

    let node_kinds = solution
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node.kind))
        .collect::<HashMap<_, _>>();
    let links =
        solution
            .edges
            .iter()
            .map(|edge| {
                let source_kind = node_kinds
                    .get(edge.source.as_str())
                    .copied()
                    .ok_or_else(|| format!("unknown edge source {}", edge.source))?;
                let target_kind = node_kinds
                    .get(edge.target.as_str())
                    .copied()
                    .ok_or_else(|| format!("unknown edge target {}", edge.target))?;
                Ok(PhysicalLink {
                    producer: producer(
                        edge.source.as_str(),
                        edge.source_port,
                        source_kind,
                        &input_ids,
                        &operator_ids,
                    )?,
                    consumer: consumer(
                        edge.target.as_str(),
                        edge.target_port,
                        target_kind,
                        &output_ids,
                        &operator_ids,
                        &discard_ids,
                    )?,
                    flow: edge.rate.exact.parse().map_err(|error| {
                        format!("invalid edge rate {}: {error}", edge.rate.exact)
                    })?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
    Ok(PhysicalGraph { nodes, links })
}

fn terminal_ids<'a>(
    solution: &'a Solution,
    kind: GraphNodeKind,
    expected: usize,
    side: &str,
) -> Result<HashMap<&'a str, u32>, String> {
    let mut ids = HashMap::new();
    for node in solution.nodes.iter().filter(|node| node.kind == kind) {
        let index = terminal_index(ids.len())?;
        if ids.insert(node.id.as_str(), index).is_some() {
            return Err(format!("duplicate {side} terminal {}", node.id));
        }
    }
    if ids.len() != expected {
        return Err(format!(
            "{side} terminal count mismatch: expected {expected}, found {}",
            ids.len()
        ));
    }
    Ok(ids)
}

fn terminal_index(index: usize) -> Result<u32, String> {
    u32::try_from(index).map_err(|_| "terminal count does not fit u32".to_owned())
}

const fn node_type(kind: GraphNodeKind) -> Option<NodeType> {
    match kind {
        GraphNodeKind::Splitter2 => Some(NodeType::Splitter2),
        GraphNodeKind::Splitter3 => Some(NodeType::Splitter3),
        GraphNodeKind::Merger2 => Some(NodeType::Merger2),
        GraphNodeKind::Merger3 => Some(NodeType::Merger3),
        GraphNodeKind::Input | GraphNodeKind::Output | GraphNodeKind::Discard => None,
    }
}

fn producer(
    id: &str,
    port: usize,
    kind: GraphNodeKind,
    inputs: &HashMap<&str, u32>,
    operators: &HashMap<&str, NodeId>,
) -> Result<ProducerPortRef, String> {
    match kind {
        GraphNodeKind::Input => inputs
            .get(id)
            .copied()
            .map(InputTerminalIndex)
            .map(ProducerPortRef::Input)
            .ok_or_else(|| format!("unknown input terminal {id}")),
        GraphNodeKind::Splitter2
        | GraphNodeKind::Splitter3
        | GraphNodeKind::Merger2
        | GraphNodeKind::Merger3 => Ok(ProducerPortRef::Node {
            node: *operators
                .get(id)
                .ok_or_else(|| format!("unknown operator {id}"))?,
            port: u8::try_from(port).map_err(|_| "producer port does not fit u8".to_owned())?,
        }),
        GraphNodeKind::Output | GraphNodeKind::Discard => {
            Err(format!("{id} cannot produce a physical link"))
        }
    }
}

fn consumer(
    id: &str,
    port: usize,
    kind: GraphNodeKind,
    outputs: &HashMap<&str, u32>,
    operators: &HashMap<&str, NodeId>,
    discards: &HashMap<&str, DiscardTerminalIndex>,
) -> Result<ConsumerPortRef, String> {
    match kind {
        GraphNodeKind::Output => outputs
            .get(id)
            .copied()
            .map(OutputTerminalIndex)
            .map(ConsumerPortRef::Output)
            .ok_or_else(|| format!("unknown output terminal {id}")),
        GraphNodeKind::Discard => discards
            .get(id)
            .copied()
            .map(ConsumerPortRef::Discard)
            .ok_or_else(|| format!("unknown discard terminal {id}")),
        GraphNodeKind::Splitter2
        | GraphNodeKind::Splitter3
        | GraphNodeKind::Merger2
        | GraphNodeKind::Merger3 => Ok(ConsumerPortRef::Node {
            node: *operators
                .get(id)
                .ok_or_else(|| format!("unknown operator {id}"))?,
            port: u8::try_from(port).map_err(|_| "consumer port does not fit u8".to_owned())?,
        }),
        GraphNodeKind::Input => Err(format!("{id} cannot consume a physical link")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{
        DisplayRate, EndpointInput, GraphEdge, GraphNode, SolutionStats, SolverEngine,
    };

    fn rate(exact: &str) -> crate::contract::DisplayRate {
        DisplayRate {
            exact: exact.to_owned(),
            decimal: exact.to_owned(),
        }
    }

    fn solution(engine: SolverEngine, internal_rate: &str) -> Solution {
        Solution {
            engine,
            status: "best_known".to_owned(),
            model_version: 4,
            proof: None,
            validation: None,
            stats: SolutionStats {
                node_count: 1,
                splitters: 1,
                mergers: 0,
                feedback_loops: 0,
                link_count: 0,
                checked_through: None,
                belt_count: Some(0),
                internal_max_throughput: Some(rate("0")),
                physical_link_count: Some(2),
                discard_link_count: Some(0),
            },
            total_input: rate("10"),
            total_output: rate("10"),
            discard_rate: rate("0"),
            belt_rate: rate("1200"),
            nodes: vec![
                GraphNode {
                    id: "source".to_owned(),
                    kind: GraphNodeKind::Input,
                    label: String::new(),
                },
                GraphNode {
                    id: "operator-9".to_owned(),
                    kind: GraphNodeKind::Splitter2,
                    label: String::new(),
                },
                GraphNode {
                    id: "target".to_owned(),
                    kind: GraphNodeKind::Output,
                    label: String::new(),
                },
            ],
            edges: vec![
                GraphEdge {
                    id: "in".to_owned(),
                    source: "source".to_owned(),
                    target: "operator-9".to_owned(),
                    source_port: 0,
                    target_port: 0,
                    rate: rate("10"),
                    feedback: false,
                    discarded: false,
                },
                GraphEdge {
                    id: "out".to_owned(),
                    source: "operator-9".to_owned(),
                    target: "target".to_owned(),
                    source_port: 0,
                    target_port: 0,
                    rate: rate(internal_rate),
                    feedback: false,
                    discarded: false,
                },
            ],
            build_steps: Vec::new(),
            layout_key: None,
        }
    }

    #[test]
    fn both_engines_use_the_same_flow_independent_layout_key() {
        let request = SolveRequest {
            inputs: vec![EndpointInput {
                id: "source".to_owned(),
                name: String::new(),
                rate: "10".to_owned(),
            }],
            outputs: vec![EndpointInput {
                id: "target".to_owned(),
                name: String::new(),
                rate: "10".to_owned(),
            }],
            belt_rate: "1200".to_owned(),
            enumerate_all_at_n: true,
            engine: SolverEngine::Custom,
        };
        let mut custom = solution(SolverEngine::Custom, "5");
        let mut z3 = solution(SolverEngine::Z3, "7");

        attach(&request, &mut custom).unwrap();
        attach(&request, &mut z3).unwrap();

        assert!(custom.has_same_layout(&z3));
        assert_ne!(custom.edges, z3.edges);
    }

    #[test]
    fn empty_request_inputs_accept_the_synthesized_input_terminal() {
        let request = SolveRequest {
            inputs: Vec::new(),
            outputs: vec![EndpointInput {
                id: "requested-output".to_owned(),
                name: String::new(),
                rate: "10".to_owned(),
            }],
            belt_rate: "1200".to_owned(),
            enumerate_all_at_n: true,
            engine: SolverEngine::Custom,
        };
        let mut synthesized = solution(SolverEngine::Custom, "10");

        attach(&request, &mut synthesized).unwrap();

        assert!(synthesized.layout_key.is_some());
    }

    #[test]
    fn empty_request_inputs_are_split_across_multiple_synthesized_belts() {
        let request = SolveRequest {
            inputs: Vec::new(),
            outputs: vec![
                EndpointInput {
                    id: "requested-output-a".to_owned(),
                    name: String::new(),
                    rate: "800".to_owned(),
                },
                EndpointInput {
                    id: "requested-output-b".to_owned(),
                    name: String::new(),
                    rate: "800".to_owned(),
                },
            ],
            belt_rate: "1200".to_owned(),
            enumerate_all_at_n: true,
            engine: SolverEngine::Z3,
        };
        let normalized = problem(&request).unwrap();
        assert_eq!(
            normalized.inputs,
            vec!["1200".parse().unwrap(), "400".parse().unwrap()]
        );

        let mut synthesized = Solution {
            engine: SolverEngine::Z3,
            status: "proven_optimal".to_owned(),
            model_version: 4,
            proof: None,
            validation: None,
            stats: SolutionStats {
                node_count: 0,
                splitters: 0,
                mergers: 0,
                feedback_loops: 0,
                link_count: 0,
                checked_through: Some(0),
                belt_count: Some(0),
                internal_max_throughput: Some(rate("0")),
                physical_link_count: Some(2),
                discard_link_count: Some(0),
            },
            total_input: rate("1600"),
            total_output: rate("1600"),
            discard_rate: rate("0"),
            belt_rate: rate("1200"),
            nodes: vec![
                GraphNode {
                    id: "input-0".to_owned(),
                    kind: GraphNodeKind::Input,
                    label: String::new(),
                },
                GraphNode {
                    id: "input-1".to_owned(),
                    kind: GraphNodeKind::Input,
                    label: String::new(),
                },
                GraphNode {
                    id: "output-0".to_owned(),
                    kind: GraphNodeKind::Output,
                    label: String::new(),
                },
                GraphNode {
                    id: "output-1".to_owned(),
                    kind: GraphNodeKind::Output,
                    label: String::new(),
                },
            ],
            edges: vec![
                GraphEdge {
                    id: "edge-0".to_owned(),
                    source: "input-0".to_owned(),
                    target: "output-0".to_owned(),
                    source_port: 0,
                    target_port: 0,
                    rate: rate("800"),
                    feedback: false,
                    discarded: false,
                },
                GraphEdge {
                    id: "edge-1".to_owned(),
                    source: "input-1".to_owned(),
                    target: "output-1".to_owned(),
                    source_port: 0,
                    target_port: 0,
                    rate: rate("800"),
                    feedback: false,
                    discarded: false,
                },
            ],
            build_steps: Vec::new(),
            layout_key: None,
        };

        attach(&request, &mut synthesized).unwrap();

        assert!(synthesized.layout_key.is_some());
    }
}
