//! Conservative exact reachability proofs for partial physical topologies.
//!
//! Reachability is a pruning oracle, not a structural validator. Malformed snapshots are therefore
//! left [`ReachabilityVerdict::Open`] so this optimization can never turn an internal representation
//! defect into a mathematical impossibility claim.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use solver_api::{
    ConsumerPortRef, DiscardTerminalIndex, InputTerminalIndex, NodeId, NodeType,
    OutputTerminalIndex, ProducerPortRef,
};

use crate::{canonical::PartialTopology, topology::TopologyState};

/// Conservative connectivity result for one partial topology.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReachabilityAnalysis {
    /// Proof result used by production pruning.
    pub verdict: ReachabilityVerdict,
    /// Whether all profile nodes are materialized and every mandatory port is occupied exactly once.
    pub structurally_complete: bool,
    /// Materialized nodes reached from an external input using existing directed links.
    pub source_reachable_nodes: Vec<NodeId>,
    /// Materialized nodes that reach a requested output or surplus discard terminal.
    pub output_reachable_nodes: Vec<NodeId>,
    /// Nodes that are already source-reachable or may gain ingress through an open consumer port.
    pub potentially_source_reachable_nodes: Vec<NodeId>,
    /// Nodes that already reach a terminal sink or may gain egress through an open producer port.
    pub potentially_output_reachable_nodes: Vec<NodeId>,
    /// Number of existing physical links lying on a directed input-to-terminal-sink walk.
    pub input_output_path_link_count: usize,
}

/// Reachability-only status. `CompleteCoverage` does not assert algebraic feasibility.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReachabilityVerdict {
    /// Future attachments may still establish all required paths.
    Open,
    /// The topology is complete and every materialized node and physical link is on an
    /// input-to-requested-output-or-discard walk.
    CompleteCoverage,
    /// A closed materialized region has a finite structural impossibility proof.
    ProvenDead(ReachabilityProof),
}

/// Finite proof that at least one materialized region can never satisfy witness reachability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReachabilityProof {
    /// A complete topology contains an ingress-closed region unreachable from every input.
    CompletedUnreachableFromInput { region: Vec<NodeId> },
    /// A complete topology contains an egress-closed region unable to reach any terminal sink.
    CompletedCannotReachOutput { region: Vec<NodeId> },
    /// Every consumer boundary of this partial materialized region is already occupied from inside
    /// the region, so no future link can introduce external-input reachability.
    ForwardClosedNoIngress { region: Vec<NodeId> },
    /// Every producer boundary of this partial materialized region is already occupied into the
    /// region, so no future link can introduce an external-output path.
    BackwardClosedNoEgress { region: Vec<NodeId> },
}

impl ReachabilityProof {
    /// Returns the deterministically node-ID-sorted closed proof region.
    #[must_use]
    pub fn region(&self) -> &[NodeId] {
        match self {
            Self::CompletedUnreachableFromInput { region }
            | Self::CompletedCannotReachOutput { region }
            | Self::ForwardClosedNoIngress { region }
            | Self::BackwardClosedNoEgress { region } => region,
        }
    }
}

/// Rebuilds conservative reachability facts from a mutable topology state.
///
/// The analysis owns no incremental cache, so checkpoint rollback automatically restores its exact
/// semantic result.
#[must_use]
pub fn analyze_reachability(state: &TopologyState) -> ReachabilityAnalysis {
    analyze_partial_reachability(&state.partial_topology())
}

/// Rebuilds conservative reachability facts from an immutable partial-topology snapshot.
///
/// # Soundness
///
/// Existing directed links are permanent. A materialized node can gain future ingress only through
/// one of its currently open consumer ports, and can gain future egress only through one of its
/// currently open producer ports. Starting potential reachability at every such open boundary and
/// closing it over existing links is therefore an over-approximation of every completion. A node
/// outside that over-approximation belongs to a closed region that no completion can repair.
///
/// Unmaterialized inventory is handled even more conservatively: while any profile node remains,
/// every materialized node is treated as potentially reachable in both directions. This deliberately
/// gives up valid prunes rather than assuming how a future node will attach.
#[must_use]
pub fn analyze_partial_reachability(topology: &PartialTopology) -> ReachabilityAnalysis {
    let graph = DerivedGraph::build(topology);
    let source_reachable = spread(&graph.adjacency, &graph.source_roots);
    let output_reachable = spread(&graph.reverse_adjacency, &graph.output_roots);

    let suppress_partial_proofs = !graph.well_formed || !remaining_is_empty(topology);
    let (potential_source_reachable, potential_output_reachable) = if suppress_partial_proofs {
        (vec![true; graph.nodes.len()], vec![true; graph.nodes.len()])
    } else {
        let mut ingress_roots = graph.source_roots.clone();
        let mut egress_roots = graph.output_roots.clone();
        for index in 0..graph.nodes.len() {
            ingress_roots[index] |= graph.open_consumers[index];
            egress_roots[index] |= graph.open_producers[index];
        }
        (
            spread(&graph.adjacency, &ingress_roots),
            spread(&graph.reverse_adjacency, &egress_roots),
        )
    };

    let path_link_count = topology
        .links
        .iter()
        .filter(|link| {
            producer_is_source_reachable(link.producer, &graph.indexes, &source_reachable)
                && consumer_can_reach_output(link.consumer, &graph.indexes, &output_reachable)
        })
        .count();

    let source_nodes = selected_nodes(&graph.nodes, &source_reachable);
    let output_nodes = selected_nodes(&graph.nodes, &output_reachable);
    let potential_source_nodes = selected_nodes(&graph.nodes, &potential_source_reachable);
    let potential_output_nodes = selected_nodes(&graph.nodes, &potential_output_reachable);

    let verdict = if !graph.well_formed {
        ReachabilityVerdict::Open
    } else if graph.structurally_complete {
        let unreachable = rejected_nodes(&graph.nodes, &source_reachable);
        if unreachable.is_empty() {
            let trapped = rejected_nodes(&graph.nodes, &output_reachable);
            if trapped.is_empty() {
                if path_link_count == topology.links.len() {
                    ReachabilityVerdict::CompleteCoverage
                } else {
                    // For a well-formed complete topology, node coverage implies link coverage.
                    // Keep the optimizer conservative if a future endpoint kind invalidates that
                    // theorem.
                    ReachabilityVerdict::Open
                }
            } else {
                ReachabilityVerdict::ProvenDead(ReachabilityProof::CompletedCannotReachOutput {
                    region: trapped,
                })
            }
        } else {
            ReachabilityVerdict::ProvenDead(ReachabilityProof::CompletedUnreachableFromInput {
                region: unreachable,
            })
        }
    } else if !remaining_is_empty(topology) {
        ReachabilityVerdict::Open
    } else {
        let no_ingress = rejected_nodes(&graph.nodes, &potential_source_reachable);
        if no_ingress.is_empty() {
            let no_egress = rejected_nodes(&graph.nodes, &potential_output_reachable);
            if no_egress.is_empty() {
                ReachabilityVerdict::Open
            } else {
                ReachabilityVerdict::ProvenDead(ReachabilityProof::BackwardClosedNoEgress {
                    region: no_egress,
                })
            }
        } else {
            ReachabilityVerdict::ProvenDead(ReachabilityProof::ForwardClosedNoIngress {
                region: no_ingress,
            })
        }
    };

    ReachabilityAnalysis {
        verdict,
        structurally_complete: graph.structurally_complete,
        source_reachable_nodes: source_nodes,
        output_reachable_nodes: output_nodes,
        potentially_source_reachable_nodes: potential_source_nodes,
        potentially_output_reachable_nodes: potential_output_nodes,
        input_output_path_link_count: path_link_count,
    }
}

#[derive(Clone, Debug)]
struct DerivedGraph {
    nodes: Vec<NodeId>,
    indexes: BTreeMap<NodeId, usize>,
    adjacency: Vec<Vec<usize>>,
    reverse_adjacency: Vec<Vec<usize>>,
    source_roots: Vec<bool>,
    output_roots: Vec<bool>,
    open_consumers: Vec<bool>,
    open_producers: Vec<bool>,
    well_formed: bool,
    structurally_complete: bool,
}

impl DerivedGraph {
    #[allow(clippy::too_many_lines)]
    fn build(topology: &PartialTopology) -> Self {
        let mut well_formed =
            !topology.problem.inputs.is_empty() && !topology.problem.outputs.is_empty();
        let mut node_types = BTreeMap::<NodeId, NodeType>::new();
        for node in &topology.nodes {
            if node_types.insert(node.id, node.node_type).is_some() {
                well_formed = false;
            }
        }
        let nodes = node_types.keys().copied().collect::<Vec<_>>();
        let indexes = nodes
            .iter()
            .enumerate()
            .map(|(index, &node)| (node, index))
            .collect::<BTreeMap<_, _>>();

        let mut declared_producers = BTreeSet::new();
        let mut declared_consumers = BTreeSet::new();
        for index in 0..topology.problem.inputs.len() {
            let Ok(index) = u32::try_from(index) else {
                well_formed = false;
                break;
            };
            declared_producers.insert(ProducerPortRef::Input(InputTerminalIndex(index)));
        }
        for index in 0..topology.problem.outputs.len() {
            let Ok(index) = u32::try_from(index) else {
                well_formed = false;
                break;
            };
            declared_consumers.insert(ConsumerPortRef::Output(OutputTerminalIndex(index)));
        }
        for index in 0..topology.discard_count {
            declared_consumers.insert(ConsumerPortRef::Discard(DiscardTerminalIndex(index)));
        }
        for (&node, &node_type) in &node_types {
            for port in 0..node_type.output_port_count() {
                declared_producers.insert(ProducerPortRef::Node { node, port });
            }
            for port in 0..node_type.input_port_count() {
                declared_consumers.insert(ConsumerPortRef::Node { node, port });
            }
        }

        let mut adjacency = vec![Vec::new(); nodes.len()];
        let mut reverse_adjacency = vec![Vec::new(); nodes.len()];
        let mut source_roots = vec![false; nodes.len()];
        let mut output_roots = vec![false; nodes.len()];
        let mut occupied_producers = BTreeSet::new();
        let mut occupied_consumers = BTreeSet::new();

        for link in &topology.links {
            if !declared_producers.contains(&link.producer)
                || !declared_consumers.contains(&link.consumer)
            {
                well_formed = false;
                continue;
            }
            if !occupied_producers.insert(link.producer)
                || !occupied_consumers.insert(link.consumer)
            {
                well_formed = false;
                continue;
            }
            if matches!(
                (link.producer, link.consumer),
                (
                    ProducerPortRef::Node { node: producer, .. },
                    ConsumerPortRef::Node { node: consumer, .. }
                ) if producer == consumer
            ) {
                well_formed = false;
                continue;
            }

            match (link.producer, link.consumer) {
                (ProducerPortRef::Input(_), ConsumerPortRef::Node { node: consumer, .. }) => {
                    source_roots[indexes[&consumer]] = true;
                }
                (
                    ProducerPortRef::Node { node: producer, .. },
                    ConsumerPortRef::Output(_) | ConsumerPortRef::Discard(_),
                ) => {
                    output_roots[indexes[&producer]] = true;
                }
                (
                    ProducerPortRef::Node { node: producer, .. },
                    ConsumerPortRef::Node { node: consumer, .. },
                ) => {
                    let producer = indexes[&producer];
                    let consumer = indexes[&consumer];
                    adjacency[producer].push(consumer);
                    reverse_adjacency[consumer].push(producer);
                }
                (
                    ProducerPortRef::Input(_),
                    ConsumerPortRef::Output(_) | ConsumerPortRef::Discard(_),
                ) => {}
            }
        }

        for neighbors in &mut adjacency {
            neighbors.sort_unstable();
            neighbors.dedup();
        }
        for neighbors in &mut reverse_adjacency {
            neighbors.sort_unstable();
            neighbors.dedup();
        }

        let mut open_consumers = vec![false; nodes.len()];
        let mut open_producers = vec![false; nodes.len()];
        for (&node, &node_type) in &node_types {
            let index = indexes[&node];
            open_producers[index] = (0..node_type.output_port_count())
                .any(|port| !occupied_producers.contains(&ProducerPortRef::Node { node, port }));
            open_consumers[index] = (0..node_type.input_port_count())
                .any(|port| !occupied_consumers.contains(&ConsumerPortRef::Node { node, port }));
        }

        let structurally_complete = well_formed
            && remaining_is_empty(topology)
            && declared_producers
                .iter()
                .all(|port| occupied_producers.contains(port))
            && declared_consumers
                .iter()
                .all(|port| occupied_consumers.contains(port));

        Self {
            nodes,
            indexes,
            adjacency,
            reverse_adjacency,
            source_roots,
            output_roots,
            open_consumers,
            open_producers,
            well_formed,
            structurally_complete,
        }
    }
}

fn remaining_is_empty(topology: &PartialTopology) -> bool {
    topology.remaining_profile.splitter2 == 0
        && topology.remaining_profile.splitter3 == 0
        && topology.remaining_profile.merger2 == 0
        && topology.remaining_profile.merger3 == 0
}

fn spread(adjacency: &[Vec<usize>], roots: &[bool]) -> Vec<bool> {
    let mut reached = roots.to_vec();
    let mut pending = reached
        .iter()
        .enumerate()
        .filter_map(|(index, &is_root)| is_root.then_some(index))
        .collect::<VecDeque<_>>();
    while let Some(node) = pending.pop_front() {
        for &next in &adjacency[node] {
            if !reached[next] {
                reached[next] = true;
                pending.push_back(next);
            }
        }
    }
    reached
}

fn selected_nodes(nodes: &[NodeId], selected: &[bool]) -> Vec<NodeId> {
    nodes
        .iter()
        .zip(selected)
        .filter_map(|(&node, &include)| include.then_some(node))
        .collect()
}

fn rejected_nodes(nodes: &[NodeId], selected: &[bool]) -> Vec<NodeId> {
    nodes
        .iter()
        .zip(selected)
        .filter_map(|(&node, &include)| (!include).then_some(node))
        .collect()
}

fn producer_is_source_reachable(
    producer: ProducerPortRef,
    indexes: &BTreeMap<NodeId, usize>,
    source_reachable: &[bool],
) -> bool {
    match producer {
        ProducerPortRef::Input(_) => true,
        ProducerPortRef::Node { node, .. } => indexes
            .get(&node)
            .is_some_and(|&index| source_reachable[index]),
    }
}

fn consumer_can_reach_output(
    consumer: ConsumerPortRef,
    indexes: &BTreeMap<NodeId, usize>,
    output_reachable: &[bool],
) -> bool {
    match consumer {
        ConsumerPortRef::Output(_) | ConsumerPortRef::Discard(_) => true,
        ConsumerPortRef::Node { node, .. } => indexes
            .get(&node)
            .is_some_and(|&index| output_reachable[index]),
    }
}

#[cfg(test)]
mod tests {
    use solver_api::{NodeProfile, PhysicalNode, Problem, Rational};

    use super::*;
    use crate::{
        canonical::PartialLink,
        problem::{Preparation, prepare_problem},
        topology::{ConsumerChoice, ProducerChoice},
    };

    fn problem(inputs: &[u32], outputs: &[u32], capacity: u32) -> Problem {
        Problem {
            inputs: inputs.iter().copied().map(Rational::from).collect(),
            outputs: outputs.iter().copied().map(Rational::from).collect(),
            max_link_rate: Rational::from(capacity),
        }
    }

    fn partial(
        problem: Problem,
        nodes: Vec<(u32, NodeType)>,
        links: Vec<(ProducerPortRef, ConsumerPortRef)>,
    ) -> PartialTopology {
        PartialTopology {
            discard_count: 0,
            problem,
            nodes: nodes
                .into_iter()
                .map(|(id, node_type)| PhysicalNode {
                    id: NodeId(id),
                    node_type,
                })
                .collect(),
            links: links
                .into_iter()
                .map(|(producer, consumer)| PartialLink {
                    producer,
                    consumer,
                    flow: None,
                })
                .collect(),
            remaining_profile: NodeProfile::default(),
        }
    }

    const fn producer(node: u32, port: u8) -> ProducerPortRef {
        ProducerPortRef::Node {
            node: NodeId(node),
            port,
        }
    }

    const fn consumer(node: u32, port: u8) -> ConsumerPortRef {
        ConsumerPortRef::Node {
            node: NodeId(node),
            port,
        }
    }

    const fn input(index: u32) -> ProducerPortRef {
        ProducerPortRef::Input(InputTerminalIndex(index))
    }

    const fn output(index: u32) -> ConsumerPortRef {
        ConsumerPortRef::Output(OutputTerminalIndex(index))
    }

    const fn discard(index: u32) -> ConsumerPortRef {
        ConsumerPortRef::Discard(DiscardTerminalIndex(index))
    }

    fn covered_parallel_topology() -> PartialTopology {
        partial(
            problem(&[2], &[2], 2),
            vec![(0, NodeType::Splitter2), (1, NodeType::Merger2)],
            vec![
                (input(0), consumer(0, 0)),
                (producer(0, 0), consumer(1, 0)),
                (producer(0, 1), consumer(1, 1)),
                (producer(1, 0), output(0)),
            ],
        )
    }

    fn disconnected_cycle_with_bypass() -> PartialTopology {
        partial(
            problem(&[2], &[2], 2),
            vec![(0, NodeType::Splitter2), (1, NodeType::Merger2)],
            vec![
                (input(0), output(0)),
                (producer(1, 0), consumer(0, 0)),
                (producer(0, 0), consumer(1, 0)),
                (producer(0, 1), consumer(1, 1)),
            ],
        )
    }

    #[test]
    fn complete_parallel_links_are_all_on_input_output_paths() {
        let analysis = analyze_partial_reachability(&covered_parallel_topology());
        assert_eq!(analysis.verdict, ReachabilityVerdict::CompleteCoverage);
        assert!(analysis.structurally_complete);
        assert_eq!(analysis.source_reachable_nodes, vec![NodeId(0), NodeId(1)]);
        assert_eq!(analysis.output_reachable_nodes, vec![NodeId(0), NodeId(1)]);
        assert_eq!(analysis.input_output_path_link_count, 4);
    }

    #[test]
    fn direct_discard_bypass_is_a_terminal_path() {
        let mut topology = partial(
            problem(&[1, 1], &[1], 1),
            Vec::new(),
            vec![(input(0), output(0)), (input(1), discard(0))],
        );
        topology.discard_count = 1;
        let analysis = analyze_partial_reachability(&topology);
        assert_eq!(analysis.verdict, ReachabilityVerdict::CompleteCoverage);
        assert!(analysis.structurally_complete);
        assert_eq!(analysis.input_output_path_link_count, 2);
    }

    #[test]
    fn direct_bypass_does_not_hide_a_completed_dead_cycle() {
        let analysis = analyze_partial_reachability(&disconnected_cycle_with_bypass());
        assert_eq!(analysis.input_output_path_link_count, 1);
        assert_eq!(
            analysis.verdict,
            ReachabilityVerdict::ProvenDead(ReachabilityProof::CompletedUnreachableFromInput {
                region: vec![NodeId(0), NodeId(1)],
            },)
        );
    }

    #[test]
    fn live_feedback_cycle_has_complete_coverage() {
        let topology = partial(
            problem(&[2], &[2], 2),
            vec![(0, NodeType::Merger2), (1, NodeType::Splitter2)],
            vec![
                (input(0), consumer(0, 0)),
                (producer(0, 0), consumer(1, 0)),
                (producer(1, 0), consumer(0, 1)),
                (producer(1, 1), output(0)),
            ],
        );
        let analysis = analyze_partial_reachability(&topology);
        assert_eq!(analysis.verdict, ReachabilityVerdict::CompleteCoverage);
        assert_eq!(analysis.input_output_path_link_count, topology.links.len());
    }

    #[test]
    fn open_cycle_boundary_prevents_speculative_pruning_then_closure_proves_death() {
        let mut topology = partial(
            problem(&[2], &[2], 2),
            vec![(0, NodeType::Splitter2), (1, NodeType::Merger2)],
            vec![
                (producer(1, 0), consumer(0, 0)),
                (producer(0, 0), consumer(1, 0)),
            ],
        );
        assert_eq!(
            analyze_partial_reachability(&topology).verdict,
            ReachabilityVerdict::Open
        );

        topology.links.push(PartialLink {
            producer: producer(0, 1),
            consumer: consumer(1, 1),
            flow: None,
        });
        assert_eq!(
            analyze_partial_reachability(&topology).verdict,
            ReachabilityVerdict::ProvenDead(ReachabilityProof::ForwardClosedNoIngress {
                region: vec![NodeId(0), NodeId(1)],
            })
        );
    }

    fn backward_closed_fixture(ids: [u32; 2], reverse_links: bool) -> PartialTopology {
        let left = ids[0];
        let right = ids[1];
        let mut topology = partial(
            problem(&[1, 1, 1, 1], &[1, 3], 3),
            vec![(left, NodeType::Merger2), (right, NodeType::Merger2)],
            vec![
                (input(0), output(0)),
                (input(1), consumer(left, 0)),
                (input(2), consumer(right, 0)),
                (producer(left, 0), consumer(right, 1)),
                (producer(right, 0), consumer(left, 1)),
            ],
        );
        if reverse_links {
            topology.links.reverse();
        }
        topology
    }

    #[test]
    fn source_reachable_region_with_no_possible_egress_is_proven_dead() {
        let analysis = analyze_partial_reachability(&backward_closed_fixture([0, 1], false));
        assert_eq!(analysis.source_reachable_nodes.len(), 2);
        assert!(analysis.output_reachable_nodes.is_empty());
        assert_eq!(
            analysis.verdict,
            ReachabilityVerdict::ProvenDead(ReachabilityProof::BackwardClosedNoEgress {
                region: vec![NodeId(0), NodeId(1)],
            })
        );
    }

    #[test]
    fn verdict_survives_node_relabeling_and_link_storage_reversal() {
        let left = analyze_partial_reachability(&backward_closed_fixture([0, 1], false));
        let right = analyze_partial_reachability(&backward_closed_fixture([8, 3], true));
        let ReachabilityVerdict::ProvenDead(left_proof) = left.verdict else {
            panic!("left fixture must be proven dead");
        };
        let ReachabilityVerdict::ProvenDead(right_proof) = right.verdict else {
            panic!("right fixture must be proven dead");
        };
        assert!(matches!(
            left_proof,
            ReachabilityProof::BackwardClosedNoEgress { .. }
        ));
        assert!(matches!(
            right_proof,
            ReachabilityProof::BackwardClosedNoEgress { .. }
        ));
        assert_eq!(left_proof.region().len(), right_proof.region().len());
        assert_eq!(
            left.input_output_path_link_count,
            right.input_output_path_link_count
        );
    }

    #[test]
    fn remaining_inventory_suppresses_even_a_closed_region_proof() {
        let mut topology = disconnected_cycle_with_bypass();
        topology.remaining_profile.splitter2 = 1;
        let analysis = analyze_partial_reachability(&topology);
        assert_eq!(analysis.verdict, ReachabilityVerdict::Open);
        assert_eq!(
            analysis.potentially_source_reachable_nodes.len(),
            topology.nodes.len()
        );
        assert_eq!(
            analysis.potentially_output_reachable_nodes.len(),
            topology.nodes.len()
        );
    }

    #[test]
    fn rebuilt_analysis_is_exactly_restored_by_topology_rollback() {
        let caller = problem(&[2], &[1, 1], 2);
        let Preparation::Prepared(normalized) = prepare_problem(&caller).unwrap() else {
            panic!("valid conserved problem must normalize");
        };
        let mut state = TopologyState::new(
            &normalized,
            NodeProfile {
                splitter2: 1,
                ..NodeProfile::default()
            },
        )
        .unwrap();
        let baseline = analyze_reachability(&state);
        let checkpoint = state.checkpoint();
        let attach = state
            .legal_decisions()
            .into_iter()
            .find(|decision| {
                matches!(decision.producer, ProducerChoice::NewNode { .. })
                    || matches!(decision.consumer, ConsumerChoice::NewNode { .. })
            })
            .unwrap();
        state.apply(attach).unwrap();
        assert_ne!(analyze_reachability(&state), baseline);
        state.rollback(checkpoint);
        assert_eq!(analyze_reachability(&state), baseline);
    }

    fn has_covered_completion(state: &mut TopologyState) -> bool {
        if state.is_complete() {
            return analyze_reachability(state).verdict == ReachabilityVerdict::CompleteCoverage;
        }
        let decisions = state.legal_decisions();
        for decision in decisions {
            let checkpoint = state.checkpoint();
            state.apply(decision).unwrap();
            let found = has_covered_completion(state);
            state.rollback(checkpoint);
            if found {
                return true;
            }
        }
        false
    }

    fn audit_prefixes(state: &mut TopologyState, prefixes: &mut usize, dead: &mut usize) {
        *prefixes += 1;
        if matches!(
            analyze_reachability(state).verdict,
            ReachabilityVerdict::ProvenDead(_)
        ) {
            *dead += 1;
            assert!(!has_covered_completion(state));
            return;
        }
        if state.is_complete() {
            return;
        }
        let decisions = state.legal_decisions();
        for decision in decisions {
            let checkpoint = state.checkpoint();
            state.apply(decision).unwrap();
            audit_prefixes(state, prefixes, dead);
            state.rollback(checkpoint);
        }
    }

    #[test]
    fn every_small_proven_dead_prefix_has_no_covered_completion() {
        let cases = [
            (problem(&[1], &[1], 1), NodeProfile::default()),
            (
                problem(&[2], &[1, 1], 2),
                NodeProfile {
                    splitter2: 1,
                    ..NodeProfile::default()
                },
            ),
            (
                problem(&[1, 1], &[2], 2),
                NodeProfile {
                    merger2: 1,
                    ..NodeProfile::default()
                },
            ),
        ];
        let mut prefixes = 0;
        let mut dead = 0;
        for (caller, profile) in cases {
            let Preparation::Prepared(normalized) = prepare_problem(&caller).unwrap() else {
                panic!("valid conserved problem must normalize");
            };
            let mut state = TopologyState::new(&normalized, profile).unwrap();
            audit_prefixes(&mut state, &mut prefixes, &mut dead);
        }

        let mut closed =
            TopologyState::from_partial_topology(&disconnected_cycle_with_bypass()).unwrap();
        audit_prefixes(&mut closed, &mut prefixes, &mut dead);
        assert!(prefixes > 3);
        assert!(dead > 0);
    }
}
