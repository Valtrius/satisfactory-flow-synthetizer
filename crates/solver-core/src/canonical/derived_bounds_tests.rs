//! Version-1 oracles for the omission of deterministic bounds from internal keys.

use super::*;

fn rational(value: &str) -> Rational {
    value.parse().unwrap()
}

// Preserve the previous encoder independently of encode_semantic_system. In
// particular, do not recover the old key by appending to the candidate's bytes.
fn legacy_state(source: &PartialTopology, topology: &CanonicalPartialTopology) -> Vec<u8> {
    let mut bytes = encode_partial_state(source, topology, PartialMark::None);
    let producers = canonical_producer_ports(source.problem.inputs.len(), &topology.nodes);
    let consumers = canonical_consumer_ports(
        source.problem.outputs.len(),
        source.discard_count,
        &topology.nodes,
    );
    let producer_indices = producers
        .iter()
        .copied()
        .enumerate()
        .map(|(i, p)| (p, i))
        .collect();
    let consumer_indices = consumers
        .iter()
        .copied()
        .enumerate()
        .map(|(i, p)| (p, producers.len() + i))
        .collect();
    let variable_count = producers.len() + consumers.len();
    let equalities = primitive_equality_basis(
        source,
        topology,
        variable_count,
        &producer_indices,
        &consumer_indices,
        false,
    );
    bytes.extend_from_slice(b"sparse-exact-rows\0\x01");
    write_varint(&mut bytes, variable_count);
    write_varint(&mut bytes, equalities.len());
    for row in &equalities {
        write_sparse_row(&mut bytes, row);
    }
    let inequalities =
        primitive_inequality_basis(&source.problem.max_link_rate, variable_count, &equalities);
    write_varint(&mut bytes, inequalities.len());
    for (relation, row) in inequalities {
        bytes.push(match relation {
            SemanticInequalityRelation::LessThan => 0,
            SemanticInequalityRelation::LessThanOrEqual => 1,
        });
        write_sparse_row(&mut bytes, &row);
    }
    bytes
}

fn legacy_scc(source: &PartialTopology, topology: &CanonicalPartialTopology) -> Vec<u8> {
    let state = legacy_state(source, topology);
    let mut bytes = b"satisfactory-canonical-scc-summary-input\0\x01".to_vec();
    write_len(&mut bytes, state.len());
    bytes.extend(state);
    write_len(&mut bytes, topology.scc_nodes.len());
    for node in &topology.scc_nodes {
        write_u32(&mut bytes, node.0);
    }
    write_len(&mut bytes, topology.known_producers.len());
    for (producer, value) in &topology.known_producers {
        write_producer(&mut bytes, *producer);
        write_rational(&mut bytes, value);
    }
    write_len(&mut bytes, topology.known_consumers.len());
    for (consumer, value) in &topology.known_consumers {
        write_consumer(&mut bytes, *consumer);
        write_rational(&mut bytes, value);
    }
    bytes
}

#[derive(Default)]
struct KeyBijection {
    forward: BTreeMap<Vec<u8>, Vec<u8>>,
    backward: BTreeMap<Vec<u8>, Vec<u8>>,
    repeats: usize,
}

impl KeyBijection {
    fn record(&mut self, current: &[u8], legacy: Vec<u8>) {
        assert!(current.len() < legacy.len());
        if let Some(previous) = self.forward.insert(current.to_vec(), legacy.clone()) {
            assert_eq!(previous, legacy, "the new key must not merge old classes");
            self.repeats += 1;
        }
        if let Some(previous) = self.backward.insert(legacy, current.to_vec()) {
            assert_eq!(previous, current, "the new key must not split old classes");
        }
    }
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

fn sample(capacity: &str, links: usize, flow: Option<&str>, surplus: bool) -> PartialTopology {
    let pairs = [
        (
            ProducerPortRef::Input(InputTerminalIndex(0)),
            consumer(42, 0),
        ),
        (producer(17, 0), consumer(42, 1)),
        (producer(42, 0), consumer(17, 0)),
        (
            producer(17, 1),
            ConsumerPortRef::Output(OutputTerminalIndex(0)),
        ),
    ];
    PartialTopology {
        problem: Problem {
            inputs: vec![rational("2/3")],
            outputs: if surplus {
                vec![rational("1/3")]
            } else {
                vec![rational("1/3"); 2]
            },
            max_link_rate: rational(capacity),
        },
        nodes: vec![
            PhysicalNode {
                id: NodeId(17),
                node_type: NodeType::Splitter2,
            },
            PhysicalNode {
                id: NodeId(42),
                node_type: NodeType::Merger2,
            },
        ],
        links: pairs[..links]
            .iter()
            .map(|&(producer, consumer)| PartialLink {
                producer,
                consumer,
                flow: flow.map(rational),
            })
            .collect(),
        discard_count: u32::from(surplus),
        remaining_profile: NodeProfile::default(),
    }
}

fn relabel(source: &PartialTopology) -> PartialTopology {
    let mut result = source.clone();
    for node in &mut result.nodes {
        node.id = NodeId(if node.id.0 == 17 { 91 } else { 6 });
    }
    for link in &mut result.links {
        if let ProducerPortRef::Node { node, port } = &mut link.producer {
            if node.0 == 17 {
                *node = NodeId(91);
                *port = 1 - *port;
            } else {
                *node = NodeId(6);
            }
        }
        if let ConsumerPortRef::Node { node, port } = &mut link.consumer {
            if node.0 == 42 {
                *node = NodeId(6);
                *port = 1 - *port;
            } else {
                *node = NodeId(91);
            }
        }
    }
    result.nodes.reverse();
    result.links.reverse();
    result
}

fn record_state(source: &PartialTopology, classes: &mut KeyBijection) {
    let selected = select_canonical(source, None, EncodingKind::State);
    let legacy = legacy_state(source, &selected.topology);
    assert_eq!(canonicalize_state(source).as_bytes(), selected.bytes);
    classes.record(&selected.bytes, legacy);
}

fn record_scc(source: &PartialTopology, classes: &mut KeyBijection) {
    for with_known in [false, true] {
        for full_region in [false, true] {
            let region = source
                .nodes
                .iter()
                .take(if full_region { 2 } else { 1 })
                .map(|n| n.id)
                .collect();
            let node = source.nodes[0].id.0;
            let known_producers = if with_known {
                [(producer(node, 0), rational("7/11"))]
                    .into_iter()
                    .collect()
            } else {
                BTreeMap::new()
            };
            let known_consumers = if with_known {
                [(consumer(node, 0), rational("14/11"))]
                    .into_iter()
                    .collect()
            } else {
                BTreeMap::new()
            };
            let annotations = SccAnnotations {
                region_nodes: &region,
                known_producers: &known_producers,
                known_consumers: &known_consumers,
            };
            let incidence =
                IncidenceGraph::build_annotated(source, None, false, Some(&annotations));
            let selected = select_canonical_with_incidence(
                source,
                &incidence,
                EncodingKind::SccSummary,
                None,
                None,
            )
            .unwrap();
            let legacy = legacy_scc(source, &selected.topology);
            let public = canonicalize_scc_summary_input_with_relabeling(
                source,
                &region,
                &known_producers,
                &known_consumers,
            );
            assert_eq!(public.key.as_bytes(), selected.bytes);
            classes.record(&selected.bytes, legacy);
        }
    }
}

#[test]
fn omitting_derived_bounds_preserves_state_and_scc_equivalence_classes() {
    let mut states = KeyBijection::default();
    let mut sccs = KeyBijection::default();
    for capacity in ["1/7", "1200", "100000000000000000000000000000000000003/11"] {
        for links in 0..=4 {
            for flow in [None, Some("0"), Some("1/3"), Some("2/3"), Some("7/11")] {
                for surplus in [false, true] {
                    let source = sample(capacity, links, flow, surplus);
                    let permuted = relabel(&source);
                    assert_eq!(canonicalize_state(&source), canonicalize_state(&permuted));
                    for topology in [&source, &permuted] {
                        record_state(topology, &mut states);
                        record_scc(topology, &mut sccs);
                    }
                }
            }
        }
    }
    assert!(states.forward.len() > 20 && sccs.forward.len() > states.forward.len());
    assert!(states.repeats > 100 && sccs.repeats > 100);
}
