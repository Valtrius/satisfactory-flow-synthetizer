//! Dynamic SCC analysis for ordinary physical topologies.
//!
//! Dynamic graph SCCs may grow as links are added. Open summaries retain their
//! current immutable equations for contradiction detection and deductions, but
//! rank deficiency alone has no pruning meaning.

use std::collections::{BTreeMap, BTreeSet};

use num::{BigInt, BigRational, One};
use petgraph::{algo::kosaraju_scc, graphmap::DiGraphMap};
use solver_api::{ConsumerPortRef, NodeId, NodeType, ProducerPortRef, Rational};
use thiserror::Error;

use crate::{
    algebra::sparse::{SparseAlgebraError, SparseAnalysis, SparseRow, SparseSystem},
    topology::{FlowVarId, TopologyState},
};

/// One current node-level strongly connected region.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SccRegion {
    /// Materialized node identifiers in deterministic ascending order.
    pub nodes: Vec<NodeId>,
    /// Whether the region contains a directed cycle.
    pub cyclic: bool,
}

/// Current SCC partition and the regions whose incidence may have changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AffectedSccs {
    /// Complete deterministic SCC partition, ordered lexicographically by node list.
    pub regions: Vec<SccRegion>,
    /// Indices into `regions`, in ascending order.
    pub affected_region_indices: Vec<usize>,
}

impl AffectedSccs {
    /// Iterates only the affected regions in deterministic order.
    pub fn affected_regions(&self) -> impl Iterator<Item = &SccRegion> {
        self.affected_region_indices
            .iter()
            .map(|&index| &self.regions[index])
    }
}

/// Exact current algebra and incidence of a region that remains open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenSccSummary {
    /// Region being summarized.
    pub region: SccRegion,
    /// Physical links whose producer and consumer nodes both belong to the region.
    pub internal_link_indices: Vec<usize>,
    /// Physical links entering the region from a terminal or another current SCC.
    pub incoming_link_indices: Vec<usize>,
    /// Physical links leaving the region for a terminal or another current SCC.
    pub outgoing_link_indices: Vec<usize>,
    /// Currently unoccupied producer ports owned by region nodes.
    pub open_producer_ports: Vec<ProducerPortRef>,
    /// Currently unoccupied consumer ports owned by region nodes.
    pub open_consumer_ports: Vec<ConsumerPortRef>,
    /// Exact flow variables owned by the region's node ports.
    pub internal_variables: Vec<FlowVarId>,
    /// Number of retained authoritative current equations.
    pub equation_count: usize,
    /// Exact fraction-free analysis with supplied and terminal-known values substituted.
    ///
    /// `is_unique() == false` is descriptive only while this region is open.
    pub algebra: SparseAnalysis,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum SccError {
    /// A materialized node identifier appeared more than once.
    #[error("duplicate materialized node {node:?}")]
    DuplicateNode { node: NodeId },
    /// A link endpoint referred to a node absent from the topology.
    #[error("physical link refers to missing node {node:?}")]
    MissingNode { node: NodeId },
    /// The appended-link marker is outside the current physical-link prefix.
    #[error("affected-link index {index} is outside {link_count} current links")]
    LinkIndexOutOfBounds { index: usize, link_count: usize },
    /// A supplied region no longer equals one SCC in the current topology.
    #[error("SCC region is stale or does not match the current SCC partition")]
    StaleRegion,
    /// A required explicit node port was absent.
    #[error("SCC equation refers to a missing physical port")]
    MissingPort,
    /// Caller-supplied and topology-known values disagree exactly.
    #[error("conflicting exact known values for {variable:?}: {left} versus {right}")]
    ConflictingKnownValue {
        variable: FlowVarId,
        left: Box<Rational>,
        right: Box<Rational>,
    },
    #[error(transparent)]
    Sparse(#[from] SparseAlgebraError),
}

/// Rebuilds the current SCC partition and selects regions affected by one link.
///
/// `added_link_index == None` selects every region. For an appended link, a
/// current SCC can change only if it contains one of that link's node endpoints.
/// If the edge merged several old condensation vertices, all merged vertices
/// now lie in the single current SCC containing both endpoints. If it did not,
/// only the two endpoint regions gained boundary incidence. Thus the returned
/// set is a sound exact affected-region set without retaining previous labels.
///
/// # Errors
///
/// Returns an error for a missing node endpoint, duplicate node identifier, or
/// out-of-range link marker.
pub fn detect_affected_sccs(
    topology: &TopologyState,
    added_link_index: Option<usize>,
) -> Result<AffectedSccs, SccError> {
    let mut node_ids = BTreeSet::new();
    for node in topology.nodes() {
        if !node_ids.insert(node.id) {
            return Err(SccError::DuplicateNode { node: node.id });
        }
    }

    let mut graph = DiGraphMap::<NodeId, ()>::new();
    for &node in &node_ids {
        graph.add_node(node);
    }
    for link in topology.links() {
        let producer = producer_node(link.producer);
        let consumer = consumer_node(link.consumer);
        validate_endpoint_node(producer, &node_ids)?;
        validate_endpoint_node(consumer, &node_ids)?;
        if let (Some(producer), Some(consumer)) = (producer, consumer) {
            graph.add_edge(producer, consumer, ());
        }
    }

    let mut regions = kosaraju_scc(&graph)
        .into_iter()
        .map(|mut nodes| {
            nodes.sort_unstable();
            let cyclic = nodes.len() > 1
                || nodes
                    .first()
                    .is_some_and(|&node| graph.contains_edge(node, node));
            SccRegion { nodes, cyclic }
        })
        .collect::<Vec<_>>();
    regions.sort_by(|left, right| left.nodes.cmp(&right.nodes));

    let affected_region_indices = if let Some(link_index) = added_link_index {
        let link = topology
            .links()
            .get(link_index)
            .ok_or(SccError::LinkIndexOutOfBounds {
                index: link_index,
                link_count: topology.links().len(),
            })?;
        let endpoint_nodes = [producer_node(link.producer), consumer_node(link.consumer)]
            .into_iter()
            .flatten()
            .collect::<BTreeSet<_>>();
        regions
            .iter()
            .enumerate()
            .filter_map(|(index, region)| {
                region
                    .nodes
                    .iter()
                    .any(|node| endpoint_nodes.contains(node))
                    .then_some(index)
            })
            .collect()
    } else {
        (0..regions.len()).collect()
    };

    Ok(AffectedSccs {
        regions,
        affected_region_indices,
    })
}

/// Rebuilds one open SCC's current exact equations from scratch.
///
/// Current crossing links and unoccupied node ports are reported as incidence,
/// not declared permanent boundaries. Splitter equality/conservation, merger
/// conservation, and every incident physical-link equality are retained as
/// authoritative sparse rows. Terminal `known_flow` facts are merged with
/// `known`; any exact inconsistency is reported by `summary.algebra.consistency`.
/// A consistent singular result never invalidates an open SCC.
///
/// # Errors
///
/// Returns an error for a stale region, missing port, conflicting known value,
/// or internal fraction-free elimination failure.
#[allow(clippy::too_many_lines)]
pub fn summarize_open_scc(
    topology: &TopologyState,
    region: &SccRegion,
    known: &BTreeMap<FlowVarId, Rational>,
) -> Result<OpenSccSummary, SccError> {
    let partition = detect_affected_sccs(topology, None)?;
    if !partition.regions.iter().any(|current| current == region) {
        return Err(SccError::StaleRegion);
    }
    let node_set = region.nodes.iter().copied().collect::<BTreeSet<_>>();
    let mut system = SparseSystem::new();
    let mut internal_variables = Vec::new();

    for node in topology
        .nodes()
        .iter()
        .filter(|node| node_set.contains(&node.id))
    {
        let outputs = (0..node.node_type.output_port_count())
            .map(|port| {
                producer_variable(
                    topology,
                    ProducerPortRef::Node {
                        node: node.id,
                        port,
                    },
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let inputs = (0..node.node_type.input_port_count())
            .map(|port| {
                consumer_variable(
                    topology,
                    ConsumerPortRef::Node {
                        node: node.id,
                        port,
                    },
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        internal_variables.extend(outputs.iter().copied());
        internal_variables.extend(inputs.iter().copied());
        insert_node_rows(&mut system, node.node_type, &inputs, &outputs);
    }
    for (&reference, port) in topology.producer_ports() {
        if producer_node(reference).is_some_and(|node| node_set.contains(&node)) {
            internal_variables.push(port.flow_var);
        }
    }
    for (&reference, port) in topology.consumer_ports() {
        if consumer_node(reference).is_some_and(|node| node_set.contains(&node)) {
            internal_variables.push(port.flow_var);
        }
    }
    internal_variables.sort_unstable();
    internal_variables.dedup();

    let mut internal_link_indices = Vec::new();
    let mut incoming_link_indices = Vec::new();
    let mut outgoing_link_indices = Vec::new();
    for (index, link) in topology.links().iter().enumerate() {
        let producer_inside =
            producer_node(link.producer).is_some_and(|node| node_set.contains(&node));
        let consumer_inside =
            consumer_node(link.consumer).is_some_and(|node| node_set.contains(&node));
        match (producer_inside, consumer_inside) {
            (true, true) => internal_link_indices.push(index),
            (false, true) => incoming_link_indices.push(index),
            (true, false) => outgoing_link_indices.push(index),
            (false, false) => continue,
        }
        let producer = producer_variable(topology, link.producer)?;
        let consumer = consumer_variable(topology, link.consumer)?;
        system.insert(equality_row(producer, consumer));
    }

    let open_producer_ports = topology
        .producer_ports()
        .iter()
        .filter_map(|(&reference, port)| {
            matches!(producer_node(reference), Some(node) if node_set.contains(&node))
                .then_some((reference, port))
        })
        .filter_map(|(reference, port)| port.connection.is_none().then_some(reference))
        .collect::<Vec<_>>();
    let open_consumer_ports = topology
        .consumer_ports()
        .iter()
        .filter_map(|(&reference, port)| {
            matches!(consumer_node(reference), Some(node) if node_set.contains(&node))
                .then_some((reference, port))
        })
        .filter_map(|(reference, port)| port.connection.is_none().then_some(reference))
        .collect::<Vec<_>>();

    let known = merged_known_values(topology, known)?;
    let algebra = system.analyze_over(internal_variables.iter().copied(), &known)?;
    Ok(OpenSccSummary {
        region: region.clone(),
        internal_link_indices,
        incoming_link_indices,
        outgoing_link_indices,
        open_producer_ports,
        open_consumer_ports,
        internal_variables,
        equation_count: system.rows().len(),
        algebra,
    })
}

fn producer_node(reference: ProducerPortRef) -> Option<NodeId> {
    match reference {
        ProducerPortRef::Input(_) => None,
        ProducerPortRef::Node { node, .. } => Some(node),
    }
}

fn consumer_node(reference: ConsumerPortRef) -> Option<NodeId> {
    match reference {
        ConsumerPortRef::Output(_) | ConsumerPortRef::Discard(_) => None,
        ConsumerPortRef::Node { node, .. } => Some(node),
    }
}

fn validate_endpoint_node(
    node: Option<NodeId>,
    declared: &BTreeSet<NodeId>,
) -> Result<(), SccError> {
    if let Some(node) = node
        && !declared.contains(&node)
    {
        return Err(SccError::MissingNode { node });
    }
    Ok(())
}

fn producer_variable(
    topology: &TopologyState,
    reference: ProducerPortRef,
) -> Result<FlowVarId, SccError> {
    topology
        .producer_ports()
        .get(&reference)
        .map(|port| port.flow_var)
        .ok_or(SccError::MissingPort)
}

fn consumer_variable(
    topology: &TopologyState,
    reference: ConsumerPortRef,
) -> Result<FlowVarId, SccError> {
    topology
        .consumer_ports()
        .get(&reference)
        .map(|port| port.flow_var)
        .ok_or(SccError::MissingPort)
}

fn insert_node_rows(
    system: &mut SparseSystem,
    node_type: NodeType,
    inputs: &[FlowVarId],
    outputs: &[FlowVarId],
) {
    match node_type {
        NodeType::Splitter2 | NodeType::Splitter3 => {
            for &output in &outputs[1..] {
                system.insert(equality_row(output, outputs[0]));
            }
            system.insert(SparseRow::new(
                std::iter::once((inputs[0], BigInt::one())).chain(
                    outputs
                        .iter()
                        .copied()
                        .map(|output| (output, BigInt::from(-1))),
                ),
                BigInt::from(0),
            ));
        }
        NodeType::Merger2 | NodeType::Merger3 => {
            system.insert(SparseRow::new(
                inputs
                    .iter()
                    .copied()
                    .map(|input| (input, BigInt::one()))
                    .chain(std::iter::once((outputs[0], BigInt::from(-1)))),
                BigInt::from(0),
            ));
        }
    }
}

fn equality_row(left: FlowVarId, right: FlowVarId) -> SparseRow {
    SparseRow::new(
        [(left, BigInt::one()), (right, BigInt::from(-1))],
        BigInt::from(0),
    )
}

fn merged_known_values(
    topology: &TopologyState,
    supplied: &BTreeMap<FlowVarId, Rational>,
) -> Result<BTreeMap<FlowVarId, BigRational>, SccError> {
    let mut merged = supplied.clone();
    for port in topology
        .producer_ports()
        .values()
        .chain(topology.consumer_ports().values())
    {
        let Some(value) = &port.known_flow else {
            continue;
        };
        if let Some(existing) = merged.get(&port.flow_var)
            && existing != value
        {
            return Err(SccError::ConflictingKnownValue {
                variable: port.flow_var,
                left: Box::new(existing.clone()),
                right: Box::new(value.clone()),
            });
        }
        merged.insert(port.flow_var, value.clone());
    }
    Ok(merged
        .into_iter()
        .map(|(variable, value)| (variable, value.into_big_rational()))
        .collect())
}

#[cfg(test)]
mod tests {
    use solver_api::{
        DiscardTerminalIndex, InputTerminalIndex, NodeProfile, OutputTerminalIndex, PhysicalNode,
        Problem,
    };

    use super::*;
    use crate::{
        algebra::sparse::Consistency,
        canonical::{PartialLink, PartialTopology},
    };

    fn rational(value: &str) -> Rational {
        value.parse().unwrap()
    }

    fn problem(inputs: &[&str], outputs: &[&str]) -> Problem {
        Problem {
            inputs: inputs.iter().map(|value| rational(value)).collect(),
            outputs: outputs.iter().map(|value| rational(value)).collect(),
            max_link_rate: rational("100"),
        }
    }

    fn node(id: u32, node_type: NodeType) -> PhysicalNode {
        PhysicalNode {
            id: NodeId(id),
            node_type,
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

    fn topology(
        specification: Problem,
        nodes: Vec<PhysicalNode>,
        links: Vec<(ProducerPortRef, ConsumerPortRef)>,
    ) -> TopologyState {
        TopologyState::from_partial_topology(&PartialTopology {
            discard_count: 0,
            problem: specification,
            nodes,
            links: links
                .into_iter()
                .map(|(producer, consumer)| PartialLink {
                    producer,
                    consumer,
                    flow: None,
                })
                .collect(),
            remaining_profile: NodeProfile::default(),
        })
        .unwrap()
    }

    #[test]
    fn discard_terminal_has_no_scc_node_owner() {
        assert_eq!(
            consumer_node(ConsumerPortRef::Discard(DiscardTerminalIndex(7))),
            None
        );
    }

    #[test]
    fn one_added_edge_selects_the_merged_cycle_not_unrelated_regions() {
        let topology = topology(
            problem(&["1"], &["1"]),
            vec![
                node(0, NodeType::Merger2),
                node(1, NodeType::Merger2),
                node(2, NodeType::Merger2),
            ],
            vec![
                (producer(0, 0), consumer(1, 0)),
                (producer(1, 0), consumer(0, 0)),
            ],
        );
        let analysis = detect_affected_sccs(&topology, Some(1)).unwrap();
        assert_eq!(analysis.regions.len(), 2);
        assert_eq!(analysis.affected_region_indices.len(), 1);
        assert_eq!(
            analysis.affected_regions().next().unwrap(),
            &SccRegion {
                nodes: vec![NodeId(0), NodeId(1)],
                cyclic: true,
            }
        );
        assert_eq!(analysis.regions[1].nodes, vec![NodeId(2)]);
    }

    #[test]
    fn crossing_edge_marks_both_distinct_endpoint_regions() {
        let topology = topology(
            problem(&["1"], &["1"]),
            vec![node(0, NodeType::Merger2), node(1, NodeType::Merger2)],
            vec![(producer(0, 0), consumer(1, 0))],
        );
        let analysis = detect_affected_sccs(&topology, Some(0)).unwrap();
        assert_eq!(analysis.affected_region_indices, vec![0, 1]);
        assert!(analysis.regions.iter().all(|region| !region.cyclic));
    }

    #[test]
    fn open_singular_region_is_descriptive_and_not_rejected() {
        let topology = topology(
            problem(&["1"], &["1"]),
            vec![node(0, NodeType::Merger2)],
            vec![],
        );
        let region = &detect_affected_sccs(&topology, None).unwrap().regions[0];
        let summary = summarize_open_scc(&topology, region, &BTreeMap::new()).unwrap();
        assert_eq!(summary.algebra.consistency, Consistency::Consistent);
        assert!(!summary.algebra.is_unique());
        assert_eq!(summary.open_producer_ports.len(), 1);
        assert_eq!(summary.open_consumer_ports.len(), 2);
        assert_eq!(summary.equation_count, 1);
    }

    #[test]
    fn immutable_current_equations_can_prove_open_inconsistency() {
        let topology = topology(
            problem(&["1", "1"], &["3"]),
            vec![node(0, NodeType::Merger2)],
            vec![
                (
                    ProducerPortRef::Input(InputTerminalIndex(0)),
                    consumer(0, 0),
                ),
                (
                    ProducerPortRef::Input(InputTerminalIndex(1)),
                    consumer(0, 1),
                ),
                (
                    producer(0, 0),
                    ConsumerPortRef::Output(OutputTerminalIndex(0)),
                ),
            ],
        );
        let region = &detect_affected_sccs(&topology, None).unwrap().regions[0];
        let summary = summarize_open_scc(&topology, region, &BTreeMap::new()).unwrap();
        assert_eq!(summary.algebra.consistency, Consistency::Inconsistent);
        assert!(summary.open_producer_ports.is_empty());
        assert!(summary.open_consumer_ports.is_empty());
        assert_eq!(summary.incoming_link_indices, vec![0, 1]);
        assert_eq!(summary.outgoing_link_indices, vec![2]);
    }

    fn cycle_with_labels(swapped: bool) -> TopologyState {
        let labels = if swapped { [1, 0] } else { [0, 1] };
        topology(
            problem(&["1"], &["1"]),
            vec![node(0, NodeType::Merger2), node(1, NodeType::Merger2)],
            vec![
                (producer(labels[0], 0), consumer(labels[1], 0)),
                (producer(labels[1], 0), consumer(labels[0], 0)),
            ],
        )
    }

    #[test]
    fn node_relabeling_preserves_structural_and_algebraic_open_summary() {
        let left = cycle_with_labels(false);
        let right = cycle_with_labels(true);
        let left_region = &detect_affected_sccs(&left, None).unwrap().regions[0];
        let right_region = &detect_affected_sccs(&right, None).unwrap().regions[0];
        let left_summary = summarize_open_scc(&left, left_region, &BTreeMap::new()).unwrap();
        let right_summary = summarize_open_scc(&right, right_region, &BTreeMap::new()).unwrap();

        assert_eq!(left_region.nodes.len(), right_region.nodes.len());
        assert_eq!(left_region.cyclic, right_region.cyclic);
        assert_eq!(left_summary.internal_link_indices.len(), 2);
        assert_eq!(
            left_summary.internal_link_indices.len(),
            right_summary.internal_link_indices.len()
        );
        assert_eq!(
            left_summary.algebra.consistency,
            right_summary.algebra.consistency
        );
        assert_eq!(
            left_summary.algebra.coefficient_rank,
            right_summary.algebra.coefficient_rank
        );
        assert_eq!(
            left_summary.algebra.variable_count,
            right_summary.algebra.variable_count
        );
    }

    #[test]
    fn stateless_rebuild_exactly_tracks_topology_checkpoint_rollback() {
        use crate::topology::{ConsumerChoice, ProducerChoice, TopologyDecision};
        use crate::{Preparation, prepare_problem};

        let specification = problem(&["2"], &["1", "1"]);
        let normalized = match prepare_problem(&specification).unwrap() {
            Preparation::Prepared(problem) => problem,
            Preparation::GloballyUnsat(proof) => panic!("unexpected proof: {proof:?}"),
        };
        let mut topology = TopologyState::new(
            &normalized,
            NodeProfile {
                splitter2: 1,
                ..NodeProfile::default()
            },
        )
        .unwrap();
        let before = detect_affected_sccs(&topology, None).unwrap();
        let checkpoint = topology.checkpoint();
        let decision = topology
            .legal_decisions()
            .into_iter()
            .find(|decision| {
                matches!(
                    decision,
                    TopologyDecision {
                        producer: ProducerChoice::Existing(ProducerPortRef::Input(_)),
                        consumer: ConsumerChoice::NewNode {
                            node_type: NodeType::Splitter2,
                            ..
                        }
                    }
                ) || matches!(
                    decision,
                    TopologyDecision {
                        producer: ProducerChoice::NewNode {
                            node_type: NodeType::Splitter2,
                            ..
                        },
                        consumer: ConsumerChoice::Existing(_)
                    }
                )
            })
            .unwrap();
        topology.apply(decision).unwrap();
        let during = detect_affected_sccs(&topology, Some(0)).unwrap();
        assert_eq!(during.regions.len(), 1);
        topology.rollback(checkpoint);
        assert_eq!(detect_affected_sccs(&topology, None).unwrap(), before);
    }
}
