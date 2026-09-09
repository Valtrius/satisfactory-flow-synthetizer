//! Exhaustive small-graph canonicalization for the deliberately simple reference solver.
//!
//! This module does not share algorithms, refinements, caches, or encodings with production
//! canonicalization. It enumerates every permitted relabeling and retains the lexicographically
//! smallest complete colored-incidence encoding. The factorial implementation is intentional:
//! the reference solver only uses it for exhaustively manageable graphs.

use std::collections::BTreeMap;

use solver_api::{
    CanonicalGraphKey, ConsumerPortRef, DiscardTerminalIndex, InputTerminalIndex, NodeId, NodeType,
    OutputTerminalIndex, PhysicalGraph, PhysicalLink, PhysicalNode, Problem, ProducerPortRef,
    Rational,
};

/// Canonical identity and canonical concrete labeling of one physical graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonicalizedGraph {
    /// Complete colored-incidence encoding, including exact link flows.
    pub key: CanonicalGraphKey,
    /// Graph with contiguous canonical node identifiers and deterministically sorted links.
    pub graph: PhysicalGraph,
}

#[derive(Clone, Debug, Default)]
struct PortLabeling {
    splitter_outputs: BTreeMap<(NodeId, u8), u8>,
    merger_inputs: BTreeMap<(NodeId, u8), u8>,
}

/// Canonicalizes a structurally well-formed physical graph by exhaustive permutation.
///
/// The enumerated isomorphisms are node relabelings within one [`NodeType`] color, symmetric
/// splitter-output and merger-input port permutations, equal-rate requested-terminal
/// permutations, arbitrary anonymous-discard permutations, and physical-link storage order.
///
/// Link flow participates in the final key. Callers may set every flow to exact zero while
/// canonicalizing an unresolved topology; zero has no special validity meaning here.
///
/// # Panics
///
/// Panics when a link refers to an undeclared node, an out-of-range terminal, or a port outside
/// its owner's effective arity. Reference topology enumeration constructs these invariants before
/// calling this function.
#[must_use]
pub fn canonicalize_graph(problem: &Problem, graph: &PhysicalGraph) -> CanonicalizedGraph {
    let input_labelings = terminal_labelings(&problem.inputs);
    let output_labelings = terminal_labelings(&problem.outputs);
    let discard_labelings = discard_labelings(graph);
    let node_labelings = node_labelings(&graph.nodes);
    let port_labelings = port_labelings(&graph.nodes);
    let node_types = graph
        .nodes
        .iter()
        .map(|node| (node.id, node.node_type))
        .collect::<BTreeMap<_, _>>();

    assert_eq!(
        node_types.len(),
        graph.nodes.len(),
        "physical node identifiers must be unique"
    );

    let canonical_input_rates = sorted_rates(&problem.inputs);
    let canonical_output_rates = sorted_rates(&problem.outputs);
    let mut best: Option<CanonicalizedGraph> = None;

    for input_labels in &input_labelings {
        for output_labels in &output_labelings {
            for discard_labels in &discard_labelings {
                for node_labels in &node_labelings {
                    for port_labels in &port_labelings {
                        let candidate_graph = relabel_graph(
                            graph,
                            input_labels,
                            output_labels,
                            discard_labels,
                            node_labels,
                            port_labels,
                            &node_types,
                        );
                        let key = CanonicalGraphKey::from_bytes(encode_colored_incidence(
                            &canonical_input_rates,
                            &canonical_output_rates,
                            &candidate_graph,
                        ));
                        let candidate = CanonicalizedGraph {
                            key,
                            graph: candidate_graph,
                        };
                        if best
                            .as_ref()
                            .is_none_or(|current| candidate.key < current.key)
                        {
                            best = Some(candidate);
                        }
                    }
                }
            }
        }
    }

    best.expect("every permutation family contains the empty or identity permutation")
}

/// Returns maps from arbitrary witness-local discard indices to contiguous
/// canonical anonymous indices.
fn discard_labelings(
    graph: &PhysicalGraph,
) -> Vec<BTreeMap<DiscardTerminalIndex, DiscardTerminalIndex>> {
    let mut discards = graph
        .links
        .iter()
        .filter_map(|link| match link.consumer {
            ConsumerPortRef::Discard(index) => Some(index),
            ConsumerPortRef::Output(_) | ConsumerPortRef::Node { .. } => None,
        })
        .collect::<Vec<_>>();
    discards.sort_unstable();
    discards.dedup();
    permutations(&discards)
        .into_iter()
        .map(|permutation| {
            permutation
                .into_iter()
                .enumerate()
                .map(|(canonical, source)| {
                    (
                        source,
                        DiscardTerminalIndex(
                            u32::try_from(canonical).expect("discard count must fit public index"),
                        ),
                    )
                })
                .collect()
        })
        .collect()
}

fn sorted_rates(rates: &[Rational]) -> Vec<Rational> {
    let mut sorted = rates.to_vec();
    sorted.sort();
    sorted
}

/// Returns maps from source terminal index to canonical terminal index.
fn terminal_labelings(rates: &[Rational]) -> Vec<Vec<u32>> {
    let mut rate_classes = BTreeMap::<Rational, Vec<usize>>::new();
    for (index, rate) in rates.iter().cloned().enumerate() {
        rate_classes.entry(rate).or_default().push(index);
    }

    let mut labelings = vec![vec![0; rates.len()]];
    let mut canonical_offset = 0_u32;
    for terminals in rate_classes.values() {
        let mut next = Vec::new();
        for labeling in &labelings {
            for permutation in permutations(terminals) {
                let mut labeling = labeling.clone();
                for (canonical_within_class, old_terminal) in permutation.into_iter().enumerate() {
                    labeling[old_terminal] = canonical_offset
                        + u32::try_from(canonical_within_class)
                            .expect("terminal count must fit its public u32 index");
                }
                next.push(labeling);
            }
        }
        labelings = next;
        canonical_offset +=
            u32::try_from(terminals.len()).expect("terminal count must fit its public u32 index");
    }
    labelings
}

/// Returns maps from source node identifier to contiguous canonical identifier.
fn node_labelings(nodes: &[PhysicalNode]) -> Vec<BTreeMap<NodeId, NodeId>> {
    let mut type_classes = BTreeMap::<NodeType, Vec<NodeId>>::new();
    for node in nodes {
        type_classes
            .entry(node.node_type)
            .or_default()
            .push(node.id);
    }
    for nodes in type_classes.values_mut() {
        nodes.sort();
    }

    let mut labelings = vec![BTreeMap::new()];
    let mut canonical_offset = 0_u32;
    for class in type_classes.values() {
        let mut next = Vec::new();
        for labeling in &labelings {
            for permutation in permutations(class) {
                let mut labeling = labeling.clone();
                for (canonical_within_type, old_node) in permutation.into_iter().enumerate() {
                    let canonical = canonical_offset
                        + u32::try_from(canonical_within_type)
                            .expect("node count must fit its public u32 identifier");
                    labeling.insert(old_node, NodeId(canonical));
                }
                next.push(labeling);
            }
        }
        labelings = next;
        canonical_offset +=
            u32::try_from(class.len()).expect("node count must fit its public u32 identifier");
    }
    labelings
}

/// Enumerates independent symmetric-port permutations on every physical node.
fn port_labelings(nodes: &[PhysicalNode]) -> Vec<PortLabeling> {
    let mut nodes = nodes.to_vec();
    nodes.sort_by_key(|node| node.id);
    let mut labelings = vec![PortLabeling::default()];

    for node in nodes {
        let (splitter_outputs, merger_inputs) = match node.node_type {
            NodeType::Splitter2 | NodeType::Splitter3 => {
                (Some(node.node_type.output_port_count()), None)
            }
            NodeType::Merger2 | NodeType::Merger3 => {
                (None, Some(node.node_type.input_port_count()))
            }
        };
        let arity = splitter_outputs
            .or(merger_inputs)
            .expect("every node has one symmetric side");
        let ports = (0..arity).collect::<Vec<_>>();
        let mut next = Vec::new();
        for labeling in &labelings {
            for permutation in permutations(&ports) {
                let mut labeling = labeling.clone();
                for (canonical_port, old_port) in permutation.into_iter().enumerate() {
                    let canonical_port =
                        u8::try_from(canonical_port).expect("physical arity is at most three");
                    if splitter_outputs.is_some() {
                        labeling
                            .splitter_outputs
                            .insert((node.id, old_port), canonical_port);
                    } else {
                        labeling
                            .merger_inputs
                            .insert((node.id, old_port), canonical_port);
                    }
                }
                next.push(labeling);
            }
        }
        labelings = next;
    }
    labelings
}

#[allow(clippy::too_many_arguments)]
fn relabel_graph(
    graph: &PhysicalGraph,
    input_labels: &[u32],
    output_labels: &[u32],
    discard_labels: &BTreeMap<DiscardTerminalIndex, DiscardTerminalIndex>,
    node_labels: &BTreeMap<NodeId, NodeId>,
    port_labels: &PortLabeling,
    node_types: &BTreeMap<NodeId, NodeType>,
) -> PhysicalGraph {
    let mut nodes = graph
        .nodes
        .iter()
        .map(|node| PhysicalNode {
            id: canonical_node(node.id, node_labels),
            node_type: node.node_type,
        })
        .collect::<Vec<_>>();
    nodes.sort();

    let mut links = graph
        .links
        .iter()
        .map(|link| PhysicalLink {
            producer: relabel_producer(
                link.producer,
                input_labels,
                node_labels,
                port_labels,
                node_types,
            ),
            consumer: relabel_consumer(
                link.consumer,
                output_labels,
                discard_labels,
                node_labels,
                port_labels,
                node_types,
            ),
            flow: link.flow.clone(),
        })
        .collect::<Vec<_>>();
    links.sort();

    PhysicalGraph { nodes, links }
}

fn canonical_node(node: NodeId, node_labels: &BTreeMap<NodeId, NodeId>) -> NodeId {
    *node_labels
        .get(&node)
        .expect("every referenced physical node must be declared")
}

fn relabel_producer(
    producer: ProducerPortRef,
    input_labels: &[u32],
    node_labels: &BTreeMap<NodeId, NodeId>,
    port_labels: &PortLabeling,
    node_types: &BTreeMap<NodeId, NodeType>,
) -> ProducerPortRef {
    match producer {
        ProducerPortRef::Input(input) => {
            let index = usize::try_from(input.0).expect("input index must fit usize");
            ProducerPortRef::Input(InputTerminalIndex(
                *input_labels
                    .get(index)
                    .expect("input terminal index must be in range"),
            ))
        }
        ProducerPortRef::Node { node, port } => {
            let node_type = node_types
                .get(&node)
                .copied()
                .expect("every referenced physical node must be declared");
            let port = match node_type {
                NodeType::Splitter2 | NodeType::Splitter3 => *port_labels
                    .splitter_outputs
                    .get(&(node, port))
                    .expect("splitter output port must be within effective arity"),
                NodeType::Merger2 | NodeType::Merger3 => {
                    assert_eq!(port, 0, "a merger has one producer port");
                    0
                }
            };
            ProducerPortRef::Node {
                node: canonical_node(node, node_labels),
                port,
            }
        }
    }
}

fn relabel_consumer(
    consumer: ConsumerPortRef,
    output_labels: &[u32],
    discard_labels: &BTreeMap<DiscardTerminalIndex, DiscardTerminalIndex>,
    node_labels: &BTreeMap<NodeId, NodeId>,
    port_labels: &PortLabeling,
    node_types: &BTreeMap<NodeId, NodeType>,
) -> ConsumerPortRef {
    match consumer {
        ConsumerPortRef::Output(output) => {
            let index = usize::try_from(output.0).expect("output index must fit usize");
            ConsumerPortRef::Output(OutputTerminalIndex(
                *output_labels
                    .get(index)
                    .expect("output terminal index must be in range"),
            ))
        }
        ConsumerPortRef::Discard(discard) => ConsumerPortRef::Discard(
            *discard_labels
                .get(&discard)
                .expect("every discard endpoint must receive a canonical label"),
        ),
        ConsumerPortRef::Node { node, port } => {
            let node_type = node_types
                .get(&node)
                .copied()
                .expect("every referenced physical node must be declared");
            let port = match node_type {
                NodeType::Splitter2 | NodeType::Splitter3 => {
                    assert_eq!(port, 0, "a splitter has one consumer port");
                    0
                }
                NodeType::Merger2 | NodeType::Merger3 => *port_labels
                    .merger_inputs
                    .get(&(node, port))
                    .expect("merger input port must be within effective arity"),
            };
            ConsumerPortRef::Node {
                node: canonical_node(node, node_labels),
                port,
            }
        }
    }
}

/// Encodes the full colored incidence graph using stable, platform-independent tags.
fn encode_colored_incidence(
    input_rates: &[Rational],
    output_rates: &[Rational],
    graph: &PhysicalGraph,
) -> Vec<u8> {
    // This byte-level domain tag is part of the canonical graph key protocol.
    // Production uses an independent algorithm but must emit this same format.
    let mut bytes = b"satisfactory-canonical-graph\0\x01".to_vec();

    write_len(&mut bytes, input_rates.len());
    for rate in input_rates {
        write_rational(&mut bytes, rate);
    }
    write_len(&mut bytes, output_rates.len());
    for rate in output_rates {
        write_rational(&mut bytes, rate);
    }

    write_len(&mut bytes, graph.nodes.len());
    for node in &graph.nodes {
        write_u32(&mut bytes, node.id.0);
        bytes.push(node_type_tag(node.node_type));
    }

    write_len(&mut bytes, graph.links.len());
    for link in &graph.links {
        write_producer(&mut bytes, link.producer);
        write_consumer(&mut bytes, link.consumer);
        write_rational(&mut bytes, &link.flow);
    }
    bytes
}

const fn node_type_tag(node_type: NodeType) -> u8 {
    match node_type {
        NodeType::Splitter2 => 0,
        NodeType::Splitter3 => 1,
        NodeType::Merger2 => 2,
        NodeType::Merger3 => 3,
    }
}

fn write_producer(bytes: &mut Vec<u8>, producer: ProducerPortRef) {
    match producer {
        ProducerPortRef::Input(input) => {
            bytes.push(0);
            write_u32(bytes, input.0);
        }
        ProducerPortRef::Node { node, port } => {
            bytes.push(1);
            write_u32(bytes, node.0);
            bytes.push(port);
        }
    }
}

fn write_consumer(bytes: &mut Vec<u8>, consumer: ConsumerPortRef) {
    match consumer {
        ConsumerPortRef::Output(output) => {
            bytes.push(0);
            write_u32(bytes, output.0);
        }
        ConsumerPortRef::Discard(discard) => {
            bytes.push(2);
            write_u32(bytes, discard.0);
        }
        ConsumerPortRef::Node { node, port } => {
            bytes.push(1);
            write_u32(bytes, node.0);
            bytes.push(port);
        }
    }
}

fn write_rational(bytes: &mut Vec<u8>, rate: &Rational) {
    let canonical = rate.to_string();
    write_len(bytes, canonical.len());
    bytes.extend_from_slice(canonical.as_bytes());
}

fn write_len(bytes: &mut Vec<u8>, length: usize) {
    let length = u64::try_from(length).expect("canonical encoding length must fit u64");
    bytes.extend_from_slice(&length.to_be_bytes());
}

fn write_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn permutations<T: Clone>(values: &[T]) -> Vec<Vec<T>> {
    if values.is_empty() {
        return vec![Vec::new()];
    }

    let mut all = Vec::new();
    for selected in 0..values.len() {
        let mut remaining = values.to_vec();
        let first = remaining.remove(selected);
        for mut suffix in permutations(&remaining) {
            let mut permutation = Vec::with_capacity(values.len());
            permutation.push(first.clone());
            permutation.append(&mut suffix);
            all.push(permutation);
        }
    }
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rate(value: &str) -> Rational {
        value.parse().unwrap()
    }

    fn problem(inputs: &[&str], outputs: &[&str]) -> Problem {
        Problem {
            inputs: inputs.iter().map(|value| rate(value)).collect(),
            outputs: outputs.iter().map(|value| rate(value)).collect(),
            max_link_rate: 100.into(),
        }
    }

    fn node(id: u32, node_type: NodeType) -> PhysicalNode {
        PhysicalNode {
            id: NodeId(id),
            node_type,
        }
    }

    fn link(producer: ProducerPortRef, consumer: ConsumerPortRef, flow: &str) -> PhysicalLink {
        PhysicalLink {
            producer,
            consumer,
            flow: rate(flow),
        }
    }

    fn input(index: u32) -> ProducerPortRef {
        ProducerPortRef::Input(InputTerminalIndex(index))
    }

    fn output(index: u32) -> ConsumerPortRef {
        ConsumerPortRef::Output(OutputTerminalIndex(index))
    }

    fn discard(index: u32) -> ConsumerPortRef {
        ConsumerPortRef::Discard(DiscardTerminalIndex(index))
    }

    fn producer(node: u32, port: u8) -> ProducerPortRef {
        ProducerPortRef::Node {
            node: NodeId(node),
            port,
        }
    }

    fn consumer(node: u32, port: u8) -> ConsumerPortRef {
        ConsumerPortRef::Node {
            node: NodeId(node),
            port,
        }
    }

    #[test]
    fn arbitrary_node_relabeling_has_one_key_and_contiguous_output_ids() {
        let problem = problem(&["1"], &["1/2", "1/4", "1/4"]);
        let original = PhysicalGraph {
            nodes: vec![node(91, NodeType::Splitter2), node(7, NodeType::Splitter2)],
            links: vec![
                link(input(0), consumer(91, 0), "1"),
                link(producer(91, 0), consumer(7, 0), "1/2"),
                link(producer(91, 1), output(0), "1/2"),
                link(producer(7, 0), output(1), "1/4"),
                link(producer(7, 1), output(2), "1/4"),
            ],
        };
        let relabeled = PhysicalGraph {
            nodes: vec![node(400, NodeType::Splitter2), node(3, NodeType::Splitter2)],
            links: vec![
                link(producer(3, 1), output(2), "1/4"),
                link(input(0), consumer(400, 0), "1"),
                link(producer(400, 1), output(0), "1/2"),
                link(producer(400, 0), consumer(3, 0), "1/2"),
                link(producer(3, 0), output(1), "1/4"),
            ],
        };

        let original = canonicalize_graph(&problem, &original);
        let relabeled = canonicalize_graph(&problem, &relabeled);
        assert_eq!(original.key, relabeled.key);
        assert_eq!(original.graph, relabeled.graph);
        assert_eq!(
            original
                .graph
                .nodes
                .iter()
                .map(|node| node.id)
                .collect::<Vec<_>>(),
            vec![NodeId(0), NodeId(1)]
        );
        assert!(
            original
                .graph
                .links
                .windows(2)
                .all(|pair| pair[0] <= pair[1])
        );
    }

    #[test]
    fn splitter_output_and_merger_input_swaps_are_quotiented() {
        let problem = problem(&["1"], &["1"]);
        let graph = PhysicalGraph {
            nodes: vec![node(10, NodeType::Splitter2), node(20, NodeType::Merger2)],
            links: vec![
                link(input(0), consumer(10, 0), "1"),
                link(producer(10, 0), consumer(20, 0), "1/2"),
                link(producer(10, 1), consumer(20, 1), "1/2"),
                link(producer(20, 0), output(0), "1"),
            ],
        };
        let swapped = PhysicalGraph {
            nodes: graph.nodes.clone(),
            links: vec![
                link(input(0), consumer(10, 0), "1"),
                link(producer(10, 1), consumer(20, 0), "1/2"),
                link(producer(10, 0), consumer(20, 1), "1/2"),
                link(producer(20, 0), output(0), "1"),
            ],
        };

        assert_eq!(
            canonicalize_graph(&problem, &graph),
            canonicalize_graph(&problem, &swapped)
        );
    }

    #[test]
    fn equal_rate_terminal_permutations_are_quotiented() {
        let problem = problem(&["1", "1"], &["1", "1"]);
        let straight = PhysicalGraph {
            nodes: Vec::new(),
            links: vec![
                link(input(0), output(0), "1"),
                link(input(1), output(1), "1"),
            ],
        };
        let crossed = PhysicalGraph {
            nodes: Vec::new(),
            links: vec![
                link(input(1), output(0), "1"),
                link(input(0), output(1), "1"),
            ],
        };

        assert_eq!(
            canonicalize_graph(&problem, &straight),
            canonicalize_graph(&problem, &crossed)
        );
    }

    #[test]
    fn parallel_node_pair_links_remain_distinct_physical_incidences() {
        let problem = problem(&["1"], &["1"]);
        let graph = PhysicalGraph {
            nodes: vec![node(0, NodeType::Splitter2), node(1, NodeType::Merger2)],
            links: vec![
                link(input(0), consumer(0, 0), "0"),
                link(producer(0, 0), consumer(1, 0), "0"),
                link(producer(0, 1), consumer(1, 1), "0"),
                link(producer(1, 0), output(0), "0"),
            ],
        };

        let canonical = canonicalize_graph(&problem, &graph);
        let parallel = canonical
            .graph
            .links
            .iter()
            .filter(|link| {
                matches!(link.producer, ProducerPortRef::Node { .. })
                    && matches!(link.consumer, ConsumerPortRef::Node { .. })
            })
            .count();
        assert_eq!(parallel, 2);
        assert_eq!(canonical.graph.links.len(), 4);
    }

    #[test]
    fn distinct_nonisomorphic_graphs_with_one_profile_have_distinct_keys() {
        let problem = problem(&["1"], &["1"]);
        let parallel = PhysicalGraph {
            nodes: vec![node(0, NodeType::Splitter2), node(1, NodeType::Merger2)],
            links: vec![
                link(input(0), consumer(0, 0), "0"),
                link(producer(0, 0), consumer(1, 0), "0"),
                link(producer(0, 1), consumer(1, 1), "0"),
                link(producer(1, 0), output(0), "0"),
            ],
        };
        let feedback = PhysicalGraph {
            nodes: parallel.nodes.clone(),
            links: vec![
                link(input(0), consumer(1, 0), "0"),
                link(producer(0, 0), consumer(1, 1), "0"),
                link(producer(1, 0), consumer(0, 0), "0"),
                link(producer(0, 1), output(0), "0"),
            ],
        };

        assert_ne!(
            canonicalize_graph(&problem, &parallel).key,
            canonicalize_graph(&problem, &feedback).key
        );
    }

    #[test]
    fn exact_flow_is_part_of_the_final_witness_key() {
        let problem = problem(&["1"], &["1"]);
        let one = PhysicalGraph {
            nodes: Vec::new(),
            links: vec![link(input(0), output(0), "1")],
        };
        let half = PhysicalGraph {
            nodes: Vec::new(),
            links: vec![link(input(0), output(0), "1/2")],
        };

        assert_ne!(
            canonicalize_graph(&problem, &one).key,
            canonicalize_graph(&problem, &half).key
        );
    }

    #[test]
    fn anonymous_discard_indices_and_symmetric_ports_are_quotiented_together() {
        let problem = problem(&["3"], &["1"]);
        let left = PhysicalGraph {
            nodes: vec![node(9, NodeType::Splitter3)],
            links: vec![
                link(input(0), consumer(9, 0), "3"),
                link(producer(9, 0), output(0), "1"),
                link(producer(9, 1), discard(41), "1"),
                link(producer(9, 2), discard(7), "1"),
            ],
        };
        let right = PhysicalGraph {
            nodes: vec![node(2, NodeType::Splitter3)],
            links: vec![
                link(producer(2, 0), discard(900), "1"),
                link(producer(2, 2), output(0), "1"),
                link(input(0), consumer(2, 0), "3"),
                link(producer(2, 1), discard(3), "1"),
            ],
        };

        let left = canonicalize_graph(&problem, &left);
        let right = canonicalize_graph(&problem, &right);
        assert_eq!(left, right);
        assert_eq!(
            left.graph
                .links
                .iter()
                .filter_map(|link| match link.consumer {
                    ConsumerPortRef::Discard(index) => Some(index),
                    ConsumerPortRef::Output(_) | ConsumerPortRef::Node { .. } => None,
                })
                .collect::<Vec<_>>(),
            vec![DiscardTerminalIndex(0), DiscardTerminalIndex(1)]
        );
    }
}
