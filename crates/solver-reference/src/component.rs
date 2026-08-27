//! Independent exhaustive topology oracle for small sealed-component cells.
//!
//! This module enumerates complete physical port partitions directly. It does
//! not call production search, propagation, SCC analysis, components, pruning,
//! memoization, or lower bounds. Synthetic external terminals are used only to
//! name declared component boundaries while canonicalizing a complete topology.

use std::{
    collections::{BTreeMap, btree_map::Entry},
    sync::atomic::{AtomicBool, Ordering},
};

use solver_api::{
    CanonicalGraphKey, ConsumerPortRef, InputTerminalIndex, NodeId, NodeProfile, NodeType,
    OutputTerminalIndex, PhysicalGraph, PhysicalLink, PhysicalNode, Problem, ProducerPortRef,
    Rational,
};
use thiserror::Error;

use crate::canonicalize_graph;

/// Version of [`ComponentCellManifest::root_digest_input`].
pub const COMPONENT_MANIFEST_VERSION: u32 = 1;

/// One finite component-enumeration cell.
///
/// Let `P` and `C` be the selected nodes' producer- and consumer-port counts,
/// `p` the boundary-input count, and `q` the boundary-output count. A valid cell
/// has `p,q >= 1` and the exact internal-link balance
/// `k = P-q = C-p >= 0`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComponentCell {
    pub profile: NodeProfile,
    pub boundary_input_count: u32,
    pub boundary_output_count: u32,
    pub internal_link_count: u32,
}

impl ComponentCell {
    /// Constructs one exactly port-balanced nonempty component cell.
    ///
    /// # Errors
    ///
    /// Returns [`ComponentOracleError`] when the profile or a boundary side is
    /// empty, a physical count overflows, or the requested boundary counts do
    /// not induce one common nonnegative internal-link count.
    pub fn new(
        profile: NodeProfile,
        boundary_input_count: u32,
        boundary_output_count: u32,
    ) -> Result<Self, ComponentOracleError> {
        let node_count = checked_node_count(profile)?;
        if node_count == 0 {
            return Err(ComponentOracleError::EmptyProfile);
        }
        if boundary_input_count == 0 {
            return Err(ComponentOracleError::MissingBoundaryInputs);
        }
        if boundary_output_count == 0 {
            return Err(ComponentOracleError::MissingBoundaryOutputs);
        }

        let producer_ports = checked_producer_port_count(profile)?;
        let consumer_ports = checked_consumer_port_count(profile)?;
        let producer_internal = producer_ports.checked_sub(boundary_output_count);
        let consumer_internal = consumer_ports.checked_sub(boundary_input_count);
        let Some(internal_link_count) = producer_internal
            .zip(consumer_internal)
            .and_then(|(producer, consumer)| (producer == consumer).then_some(producer))
        else {
            return Err(ComponentOracleError::UnbalancedCell {
                producer_ports,
                consumer_ports,
                boundary_input_count,
                boundary_output_count,
            });
        };
        internal_link_count
            .checked_add(boundary_input_count)
            .and_then(|count| count.checked_add(boundary_output_count))
            .ok_or(ComponentOracleError::PhysicalLinkCountOverflow)?;

        Ok(Self {
            profile,
            boundary_input_count,
            boundary_output_count,
            internal_link_count,
        })
    }

    /// Returns the number of physical links after synthetic boundary links are
    /// included.
    #[must_use]
    pub const fn physical_link_count(self) -> u32 {
        self.internal_link_count + self.boundary_input_count + self.boundary_output_count
    }
}

/// Finite bounds for component-cell generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComponentEnumerationBounds {
    /// Inclusive physical-node bound. Cells themselves always contain at least
    /// one node.
    pub max_nodes: u32,
    /// Optional inclusive bound on `boundary_input_count + boundary_output_count`.
    /// `None` still yields a finite universe because node arities bound both sides.
    pub max_boundary_ports: Option<u32>,
}

/// One internal node-to-node physical link in a canonical component topology.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComponentInternalLink {
    pub producer: ProducerPortRef,
    pub consumer: ConsumerPortRef,
}

/// One topology-ID-free canonical component topology.
///
/// `boundary_inputs[i]` is the selected-node consumer port reached by canonical
/// synthetic input `i`; `boundary_outputs[j]` is the selected-node producer
/// port feeding canonical synthetic output `j`. These arrays therefore convert
/// directly to a frozen-subsystem declaration after a consumer maps the local
/// contiguous [`NodeId`] values into its own topology.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CanonicalComponentTopology {
    pub cell: ComponentCell,
    pub key: CanonicalGraphKey,
    pub nodes: Vec<PhysicalNode>,
    pub internal_links: Vec<ComponentInternalLink>,
    pub boundary_inputs: Vec<ConsumerPortRef>,
    pub boundary_outputs: Vec<ProducerPortRef>,
}

impl CanonicalComponentTopology {
    /// Reconstructs the canonical complete synthetic-boundary graph.
    ///
    /// Every flow is the exact zero placeholder used only for structural
    /// canonicalization. The graph is not a concrete flow witness.
    ///
    /// # Panics
    ///
    /// Panics only if a caller manually constructs this DTO with more boundary
    /// entries than its public `u32` cell counts can represent. Values returned
    /// by [`enumerate_component_cell`] preserve the invariant.
    #[must_use]
    pub fn synthetic_graph(&self) -> PhysicalGraph {
        let mut links = Vec::new();
        for (index, &consumer) in self.boundary_inputs.iter().enumerate() {
            links.push(PhysicalLink {
                producer: ProducerPortRef::Input(InputTerminalIndex(
                    u32::try_from(index).expect("validated boundary count is u32"),
                )),
                consumer,
                flow: Rational::zero(),
            });
        }
        links.extend(self.internal_links.iter().map(|link| PhysicalLink {
            producer: link.producer,
            consumer: link.consumer,
            flow: Rational::zero(),
        }));
        for (index, &producer) in self.boundary_outputs.iter().enumerate() {
            links.push(PhysicalLink {
                producer,
                consumer: ConsumerPortRef::Output(OutputTerminalIndex(
                    u32::try_from(index).expect("validated boundary count is u32"),
                )),
                flow: Rational::zero(),
            });
        }
        links.sort_unstable();
        PhysicalGraph {
            nodes: self.nodes.clone(),
            links,
        }
    }
}

/// Deterministic proof input for one completely enumerated cell.
///
/// This is not a cryptographic digest. A persistence layer may hash the stable
/// versioned bytes returned by [`Self::root_digest_input`] with its selected
/// cryptographic algorithm.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentCellManifest {
    pub cell: ComponentCell,
    /// Sorted, duplicate-free canonical topology identities.
    pub canonical_topology_keys: Vec<CanonicalGraphKey>,
}

impl ComponentCellManifest {
    /// Returns versioned deterministic bytes suitable as one Merkle/root-digest
    /// leaf input.
    #[must_use]
    pub fn root_digest_input(&self) -> Vec<u8> {
        let mut bytes = b"satisfactory-reference-component-cell-manifest\0\x01".to_vec();
        write_u32(&mut bytes, self.cell.profile.splitter2);
        write_u32(&mut bytes, self.cell.profile.splitter3);
        write_u32(&mut bytes, self.cell.profile.merger2);
        write_u32(&mut bytes, self.cell.profile.merger3);
        write_u32(&mut bytes, self.cell.boundary_input_count);
        write_u32(&mut bytes, self.cell.boundary_output_count);
        write_u32(&mut bytes, self.cell.internal_link_count);
        write_length(&mut bytes, self.canonical_topology_keys.len());
        for key in &self.canonical_topology_keys {
            write_length(&mut bytes, key.as_bytes().len());
            bytes.extend_from_slice(key.as_bytes());
        }
        bytes
    }
}

/// Exact progress captured when cancellation interrupts a cell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ComponentEnumerationProgress {
    pub search_states_visited: u64,
    pub labeled_topologies_completed: u64,
    pub canonical_topologies_found: u64,
}

/// Outcome of one bounded component-cell enumeration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComponentCellEnumeration {
    /// Every legal labeled topology was exhausted and canonically deduplicated.
    Complete {
        topologies: Vec<CanonicalComponentTopology>,
        manifest: ComponentCellManifest,
    },
    /// Cancellation interrupted raw exhaustive enumeration. No complete
    /// manifest or partial topology set is exposed as proof-complete.
    Cancelled {
        progress: ComponentEnumerationProgress,
    },
}

/// Invalid cell or exact public-count failure.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ComponentOracleError {
    #[error("a component cell must contain at least one physical node")]
    EmptyProfile,
    #[error("a component cell must expose at least one boundary input")]
    MissingBoundaryInputs,
    #[error("a component cell must expose at least one boundary output")]
    MissingBoundaryOutputs,
    #[error("component profile node count overflowed u32")]
    NodeCountOverflow,
    #[error("component profile producer-port count overflowed u32")]
    ProducerPortCountOverflow,
    #[error("component profile consumer-port count overflowed u32")]
    ConsumerPortCountOverflow,
    #[error("component physical-link count overflowed u32")]
    PhysicalLinkCountOverflow,
    #[error(
        "component cell is not port-balanced: P={producer_ports}, C={consumer_ports}, p={boundary_input_count}, q={boundary_output_count}"
    )]
    UnbalancedCell {
        producer_ports: u32,
        consumer_ports: u32,
        boundary_input_count: u32,
        boundary_output_count: u32,
    },
    #[error("component enumeration progress overflowed u64")]
    ProgressOverflow,
    #[error("canonical component topology violated the synthetic-boundary invariant")]
    CanonicalBoundaryInvariant,
}

/// Enumerates every finite cell admitted by the supplied node and boundary bounds.
///
/// Cells are sorted by `(node_count, profile, p, q, k)`. No structural topology
/// or algebraic feasibility pruning occurs here.
///
/// # Errors
///
/// Returns [`ComponentOracleError`] if a derived profile or physical count does
/// not fit its public `u32` representation.
pub fn component_cells(
    bounds: ComponentEnumerationBounds,
) -> Result<Vec<ComponentCell>, ComponentOracleError> {
    let mut cells = Vec::new();
    for node_count in 1..=bounds.max_nodes {
        for profile in profiles_at_node_count(node_count) {
            let producer_ports = checked_producer_port_count(profile)?;
            let consumer_ports = checked_consumer_port_count(profile)?;
            let Some(maximum_internal) = producer_ports.min(consumer_ports).checked_sub(1) else {
                continue;
            };
            for internal_link_count in 0..=maximum_internal {
                let boundary_input_count = consumer_ports - internal_link_count;
                let boundary_output_count = producer_ports - internal_link_count;
                if bounds.max_boundary_ports.is_some_and(|maximum| {
                    u64::from(boundary_input_count) + u64::from(boundary_output_count)
                        > u64::from(maximum)
                }) {
                    continue;
                }
                cells.push(ComponentCell::new(
                    profile,
                    boundary_input_count,
                    boundary_output_count,
                )?);
            }
        }
    }
    cells.sort_unstable_by_key(|cell| {
        (
            cell.profile.node_count(),
            cell.profile,
            cell.boundary_input_count,
            cell.boundary_output_count,
            cell.internal_link_count,
        )
    });
    Ok(cells)
}

/// Exhaustively enumerates and canonically deduplicates one component cell.
///
/// The raw DFS checks cancellation at every recursive state. A `Complete`
/// outcome is returned only after all complete labeled port bijections have
/// been exhausted. Canonical topologies and manifest keys are sorted by the
/// authoritative reference graph key.
///
/// # Errors
///
/// Returns [`ComponentOracleError`] for an invalid cell, a public-count
/// overflow, progress overflow, or a violated internal canonicalization
/// invariant.
pub fn enumerate_component_cell(
    cell: ComponentCell,
    cancel: &AtomicBool,
) -> Result<ComponentCellEnumeration, ComponentOracleError> {
    let validated = ComponentCell::new(
        cell.profile,
        cell.boundary_input_count,
        cell.boundary_output_count,
    )?;
    if validated != cell {
        return Err(ComponentOracleError::UnbalancedCell {
            producer_ports: checked_producer_port_count(cell.profile)?,
            consumer_ports: checked_consumer_port_count(cell.profile)?,
            boundary_input_count: cell.boundary_input_count,
            boundary_output_count: cell.boundary_output_count,
        });
    }

    let nodes = materialize_nodes(cell.profile)?;
    let producers = producer_ports(cell.boundary_input_count, &nodes);
    let consumers = consumer_ports(cell.boundary_output_count, &nodes);
    debug_assert_eq!(producers.len(), consumers.len());
    let physical_link_count = usize::try_from(cell.physical_link_count())
        .map_err(|_| ComponentOracleError::PhysicalLinkCountOverflow)?;
    debug_assert_eq!(producers.len(), physical_link_count);
    let problem = synthetic_problem(cell);
    let mut enumerator = CellEnumerator {
        cell,
        cancel,
        problem,
        nodes,
        producers,
        consumers,
        used_consumers: vec![false; physical_link_count],
        links: Vec::new(),
        canonical: BTreeMap::new(),
        progress: ComponentEnumerationProgress::default(),
    };
    if enumerator.enumerate_from(0)? || cancel.load(Ordering::Relaxed) {
        return Ok(ComponentCellEnumeration::Cancelled {
            progress: enumerator.progress,
        });
    }

    let topologies = enumerator.canonical.into_values().collect::<Vec<_>>();
    let manifest = ComponentCellManifest {
        cell,
        canonical_topology_keys: topologies
            .iter()
            .map(|topology| topology.key.clone())
            .collect(),
    };
    Ok(ComponentCellEnumeration::Complete {
        topologies,
        manifest,
    })
}

struct CellEnumerator<'a> {
    cell: ComponentCell,
    cancel: &'a AtomicBool,
    problem: Problem,
    nodes: Vec<PhysicalNode>,
    producers: Vec<ProducerPortRef>,
    consumers: Vec<ConsumerPortRef>,
    used_consumers: Vec<bool>,
    links: Vec<(ProducerPortRef, ConsumerPortRef)>,
    canonical: BTreeMap<CanonicalGraphKey, CanonicalComponentTopology>,
    progress: ComponentEnumerationProgress,
}

impl CellEnumerator<'_> {
    /// Returns `true` when cancellation interrupted this subtree.
    fn enumerate_from(&mut self, producer_index: usize) -> Result<bool, ComponentOracleError> {
        if self.cancel.load(Ordering::Relaxed) {
            return Ok(true);
        }
        self.progress.search_states_visited = self
            .progress
            .search_states_visited
            .checked_add(1)
            .ok_or(ComponentOracleError::ProgressOverflow)?;
        if producer_index == self.producers.len() {
            self.record_complete_topology()?;
            return Ok(false);
        }

        let producer = self.producers[producer_index];
        for consumer_index in 0..self.consumers.len() {
            let consumer = self.consumers[consumer_index];
            if self.used_consumers[consumer_index] || !legal_component_link(producer, consumer) {
                continue;
            }
            self.used_consumers[consumer_index] = true;
            self.links.push((producer, consumer));
            let cancelled = self.enumerate_from(producer_index + 1)?;
            self.links.pop();
            self.used_consumers[consumer_index] = false;
            if cancelled {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn record_complete_topology(&mut self) -> Result<(), ComponentOracleError> {
        self.progress.labeled_topologies_completed = self
            .progress
            .labeled_topologies_completed
            .checked_add(1)
            .ok_or(ComponentOracleError::ProgressOverflow)?;
        let graph = PhysicalGraph {
            nodes: self.nodes.clone(),
            links: self
                .links
                .iter()
                .map(|&(producer, consumer)| PhysicalLink {
                    producer,
                    consumer,
                    flow: Rational::zero(),
                })
                .collect(),
        };
        let canonical = canonicalize_graph(&self.problem, &graph);
        let topology =
            extract_component_topology(self.cell, canonical.key.clone(), canonical.graph)?;
        if let Entry::Vacant(entry) = self.canonical.entry(canonical.key) {
            entry.insert(topology);
            self.progress.canonical_topologies_found = self
                .progress
                .canonical_topologies_found
                .checked_add(1)
                .ok_or(ComponentOracleError::ProgressOverflow)?;
        }
        Ok(())
    }
}

fn extract_component_topology(
    cell: ComponentCell,
    key: CanonicalGraphKey,
    graph: PhysicalGraph,
) -> Result<CanonicalComponentTopology, ComponentOracleError> {
    let mut boundary_inputs = vec![
        None;
        usize::try_from(cell.boundary_input_count).map_err(|_| {
            ComponentOracleError::PhysicalLinkCountOverflow
        })?
    ];
    let mut boundary_outputs = vec![
        None;
        usize::try_from(cell.boundary_output_count).map_err(|_| {
            ComponentOracleError::PhysicalLinkCountOverflow
        })?
    ];
    let mut internal_links = Vec::new();
    for link in graph.links {
        match (link.producer, link.consumer) {
            (ProducerPortRef::Input(input), consumer @ ConsumerPortRef::Node { .. }) => {
                let slot = usize::try_from(input.0)
                    .ok()
                    .and_then(|index| boundary_inputs.get_mut(index))
                    .ok_or(ComponentOracleError::CanonicalBoundaryInvariant)?;
                if slot.replace(consumer).is_some() {
                    return Err(ComponentOracleError::CanonicalBoundaryInvariant);
                }
            }
            (producer @ ProducerPortRef::Node { .. }, ConsumerPortRef::Output(output)) => {
                let slot = usize::try_from(output.0)
                    .ok()
                    .and_then(|index| boundary_outputs.get_mut(index))
                    .ok_or(ComponentOracleError::CanonicalBoundaryInvariant)?;
                if slot.replace(producer).is_some() {
                    return Err(ComponentOracleError::CanonicalBoundaryInvariant);
                }
            }
            (producer @ ProducerPortRef::Node { .. }, consumer @ ConsumerPortRef::Node { .. }) => {
                internal_links.push(ComponentInternalLink { producer, consumer });
            }
            _ => return Err(ComponentOracleError::CanonicalBoundaryInvariant),
        }
    }
    let boundary_inputs = boundary_inputs
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or(ComponentOracleError::CanonicalBoundaryInvariant)?;
    let boundary_outputs = boundary_outputs
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or(ComponentOracleError::CanonicalBoundaryInvariant)?;
    if internal_links.len()
        != usize::try_from(cell.internal_link_count)
            .map_err(|_| ComponentOracleError::PhysicalLinkCountOverflow)?
    {
        return Err(ComponentOracleError::CanonicalBoundaryInvariant);
    }
    Ok(CanonicalComponentTopology {
        cell,
        key,
        nodes: graph.nodes,
        internal_links,
        boundary_inputs,
        boundary_outputs,
    })
}

fn synthetic_problem(cell: ComponentCell) -> Problem {
    Problem {
        inputs: (0..cell.boundary_input_count)
            .map(|_| Rational::one())
            .collect(),
        outputs: (0..cell.boundary_output_count)
            .map(|_| Rational::one())
            .collect(),
        max_link_rate: Rational::one(),
    }
}

fn legal_component_link(producer: ProducerPortRef, consumer: ConsumerPortRef) -> bool {
    match (producer, consumer) {
        (ProducerPortRef::Input(_), ConsumerPortRef::Node { .. })
        | (ProducerPortRef::Node { .. }, ConsumerPortRef::Output(_)) => true,
        (
            ProducerPortRef::Node { node: producer, .. },
            ConsumerPortRef::Node { node: consumer, .. },
        ) => producer != consumer,
        (ProducerPortRef::Input(_), ConsumerPortRef::Output(_) | ConsumerPortRef::Discard(_))
        | (ProducerPortRef::Node { .. }, ConsumerPortRef::Discard(_)) => false,
    }
}

fn materialize_nodes(profile: NodeProfile) -> Result<Vec<PhysicalNode>, ComponentOracleError> {
    checked_node_count(profile)?;
    let mut nodes = Vec::new();
    let mut next_id = 0_u32;
    for (node_type, count) in [
        (NodeType::Splitter2, profile.splitter2),
        (NodeType::Splitter3, profile.splitter3),
        (NodeType::Merger2, profile.merger2),
        (NodeType::Merger3, profile.merger3),
    ] {
        for _ in 0..count {
            nodes.push(PhysicalNode {
                id: NodeId(next_id),
                node_type,
            });
            next_id = next_id
                .checked_add(1)
                .ok_or(ComponentOracleError::NodeCountOverflow)?;
        }
    }
    Ok(nodes)
}

fn producer_ports(boundary_input_count: u32, nodes: &[PhysicalNode]) -> Vec<ProducerPortRef> {
    let mut ports = (0..boundary_input_count)
        .map(|index| ProducerPortRef::Input(InputTerminalIndex(index)))
        .collect::<Vec<_>>();
    for node in nodes {
        ports.extend(
            (0..node.node_type.output_port_count()).map(|port| ProducerPortRef::Node {
                node: node.id,
                port,
            }),
        );
    }
    ports
}

fn consumer_ports(boundary_output_count: u32, nodes: &[PhysicalNode]) -> Vec<ConsumerPortRef> {
    let mut ports = Vec::new();
    for node in nodes {
        ports.extend(
            (0..node.node_type.input_port_count()).map(|port| ConsumerPortRef::Node {
                node: node.id,
                port,
            }),
        );
    }
    ports.extend(
        (0..boundary_output_count).map(|index| ConsumerPortRef::Output(OutputTerminalIndex(index))),
    );
    ports
}

fn profiles_at_node_count(node_count: u32) -> Vec<NodeProfile> {
    let mut profiles = Vec::new();
    for splitter2 in 0..=node_count {
        let non_splitter2 = node_count - splitter2;
        for splitter3 in 0..=non_splitter2 {
            let merger_total = non_splitter2 - splitter3;
            for merger2 in 0..=merger_total {
                profiles.push(NodeProfile {
                    splitter2,
                    splitter3,
                    merger2,
                    merger3: merger_total - merger2,
                });
            }
        }
    }
    profiles.sort_unstable();
    profiles
}

fn checked_node_count(profile: NodeProfile) -> Result<u32, ComponentOracleError> {
    profile
        .splitter2
        .checked_add(profile.splitter3)
        .and_then(|count| count.checked_add(profile.merger2))
        .and_then(|count| count.checked_add(profile.merger3))
        .ok_or(ComponentOracleError::NodeCountOverflow)
}

fn checked_producer_port_count(profile: NodeProfile) -> Result<u32, ComponentOracleError> {
    profile
        .splitter2
        .checked_mul(2)
        .and_then(|count| {
            profile
                .splitter3
                .checked_mul(3)
                .and_then(|ports| count.checked_add(ports))
        })
        .and_then(|count| count.checked_add(profile.merger2))
        .and_then(|count| count.checked_add(profile.merger3))
        .ok_or(ComponentOracleError::ProducerPortCountOverflow)
}

fn checked_consumer_port_count(profile: NodeProfile) -> Result<u32, ComponentOracleError> {
    profile
        .splitter2
        .checked_add(profile.splitter3)
        .and_then(|count| {
            profile
                .merger2
                .checked_mul(2)
                .and_then(|ports| count.checked_add(ports))
        })
        .and_then(|count| {
            profile
                .merger3
                .checked_mul(3)
                .and_then(|ports| count.checked_add(ports))
        })
        .ok_or(ComponentOracleError::ConsumerPortCountOverflow)
}

fn write_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn write_length(bytes: &mut Vec<u8>, value: usize) {
    bytes.extend_from_slice(value.to_string().as_bytes());
    bytes.push(0);
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    fn profile(splitter2: u32, splitter3: u32, merger2: u32, merger3: u32) -> NodeProfile {
        NodeProfile {
            splitter2,
            splitter3,
            merger2,
            merger3,
        }
    }

    fn complete(cell: ComponentCell) -> (Vec<CanonicalComponentTopology>, ComponentCellManifest) {
        match enumerate_component_cell(cell, &AtomicBool::new(false)).unwrap() {
            ComponentCellEnumeration::Complete {
                topologies,
                manifest,
            } => (topologies, manifest),
            ComponentCellEnumeration::Cancelled { .. } => panic!("unexpected cancellation"),
        }
    }

    #[test]
    fn bounded_cells_obey_exact_balance_and_optional_total_boundary_limit() {
        let all = component_cells(ComponentEnumerationBounds {
            max_nodes: 1,
            max_boundary_ports: None,
        })
        .unwrap();
        assert_eq!(all.len(), 4);
        assert!(all.iter().all(|cell| {
            cell.internal_link_count
                == checked_producer_port_count(cell.profile).unwrap() - cell.boundary_output_count
                && cell.internal_link_count
                    == checked_consumer_port_count(cell.profile).unwrap()
                        - cell.boundary_input_count
        }));

        let limited = component_cells(ComponentEnumerationBounds {
            max_nodes: 1,
            max_boundary_ports: Some(3),
        })
        .unwrap();
        assert_eq!(limited.len(), 2);
        assert!(
            limited
                .iter()
                .all(|cell| { cell.boundary_input_count + cell.boundary_output_count <= 3 })
        );
    }

    #[test]
    fn rejects_empty_boundary_and_unbalanced_cells() {
        assert_eq!(
            ComponentCell::new(NodeProfile::default(), 1, 1),
            Err(ComponentOracleError::EmptyProfile)
        );
        assert_eq!(
            ComponentCell::new(profile(1, 0, 0, 0), 0, 2),
            Err(ComponentOracleError::MissingBoundaryInputs)
        );
        assert!(matches!(
            ComponentCell::new(profile(1, 0, 0, 0), 1, 1),
            Err(ComponentOracleError::UnbalancedCell { .. })
        ));
    }

    #[test]
    fn one_splitter_or_merger_has_one_boundary_only_topology() {
        for cell in [
            ComponentCell::new(profile(1, 0, 0, 0), 1, 2).unwrap(),
            ComponentCell::new(profile(0, 1, 0, 0), 1, 3).unwrap(),
            ComponentCell::new(profile(0, 0, 1, 0), 2, 1).unwrap(),
            ComponentCell::new(profile(0, 0, 0, 1), 3, 1).unwrap(),
        ] {
            let (topologies, manifest) = complete(cell);
            assert_eq!(topologies.len(), 1);
            assert_eq!(manifest.canonical_topology_keys.len(), 1);
            assert!(topologies[0].internal_links.is_empty());
        }
    }

    #[test]
    fn splitter_merger_cells_have_exhaustive_known_canonical_counts() {
        let pair = profile(1, 0, 1, 0);
        for (p, q, expected) in [(3, 3, 1), (2, 2, 2), (1, 1, 2)] {
            let cell = ComponentCell::new(pair, p, q).unwrap();
            let (topologies, _) = complete(cell);
            assert_eq!(topologies.len(), expected, "unexpected cell {cell:?}");
            assert!(
                topologies
                    .iter()
                    .all(|topology| topology.internal_links.len()
                        == usize::try_from(cell.internal_link_count).unwrap())
            );
        }
    }

    #[test]
    fn every_boundary_attaches_to_a_node_and_internal_links_never_self_connect() {
        let cells = component_cells(ComponentEnumerationBounds {
            max_nodes: 2,
            max_boundary_ports: Some(4),
        })
        .unwrap();
        for cell in cells {
            let (topologies, _) = complete(cell);
            for topology in topologies {
                let graph = topology.synthetic_graph();
                assert!(
                    graph
                        .links
                        .iter()
                        .all(|link| match (link.producer, link.consumer) {
                            (ProducerPortRef::Input(_), ConsumerPortRef::Node { .. })
                            | (ProducerPortRef::Node { .. }, ConsumerPortRef::Output(_)) => true,
                            (
                                ProducerPortRef::Node { node: producer, .. },
                                ConsumerPortRef::Node { node: consumer, .. },
                            ) => producer != consumer,
                            _ => false,
                        })
                );
            }
        }
    }

    #[test]
    fn all_small_relabel_port_boundary_and_storage_permutations_have_one_key() {
        let cell = ComponentCell::new(profile(1, 0, 1, 0), 2, 2).unwrap();
        let (topologies, _) = complete(cell);
        let original = topologies[0].synthetic_graph();
        let problem = synthetic_problem(cell);
        let expected = canonicalize_graph(&problem, &original).key;
        let splitter = original
            .nodes
            .iter()
            .find(|node| node.node_type == NodeType::Splitter2)
            .unwrap()
            .id;
        let merger = original
            .nodes
            .iter()
            .find(|node| node.node_type == NodeType::Merger2)
            .unwrap()
            .id;

        // Exhaust both permutations of every two-element symmetry together
        // with arbitrary local IDs and both storage orders.
        for mask in 0_u8..128 {
            let input_swap = mask & 1 != 0;
            let output_swap = mask & 2 != 0;
            let splitter_port_swap = mask & 4 != 0;
            let merger_port_swap = mask & 8 != 0;
            let arbitrary_ids = mask & 16 != 0;
            let reverse_nodes = mask & 32 != 0;
            let reverse_links = mask & 64 != 0;
            let new_splitter = if arbitrary_ids { NodeId(91) } else { NodeId(0) };
            let new_merger = if arbitrary_ids { NodeId(7) } else { NodeId(1) };

            let mut variant = original.clone();
            for node in &mut variant.nodes {
                node.id = if node.id == splitter {
                    new_splitter
                } else {
                    new_merger
                };
            }
            for link in &mut variant.links {
                link.producer = match link.producer {
                    ProducerPortRef::Input(InputTerminalIndex(index)) => {
                        ProducerPortRef::Input(InputTerminalIndex(if input_swap {
                            1 - index
                        } else {
                            index
                        }))
                    }
                    ProducerPortRef::Node { node, port } if node == splitter => {
                        ProducerPortRef::Node {
                            node: new_splitter,
                            port: if splitter_port_swap { 1 - port } else { port },
                        }
                    }
                    ProducerPortRef::Node { .. } => ProducerPortRef::Node {
                        node: new_merger,
                        port: 0,
                    },
                };
                link.consumer = match link.consumer {
                    ConsumerPortRef::Output(OutputTerminalIndex(index)) => {
                        ConsumerPortRef::Output(OutputTerminalIndex(if output_swap {
                            1 - index
                        } else {
                            index
                        }))
                    }
                    ConsumerPortRef::Node { node, port } if node == merger => {
                        ConsumerPortRef::Node {
                            node: new_merger,
                            port: if merger_port_swap { 1 - port } else { port },
                        }
                    }
                    ConsumerPortRef::Node { .. } => ConsumerPortRef::Node {
                        node: new_splitter,
                        port: 0,
                    },
                    ConsumerPortRef::Discard(_) => unreachable!(),
                };
            }
            if reverse_nodes {
                variant.nodes.reverse();
            }
            if reverse_links {
                variant.links.reverse();
            }

            assert_eq!(canonicalize_graph(&problem, &variant).key, expected);
        }
    }

    #[test]
    fn direct_dfs_topology_set_matches_independent_full_permutation_oracle() {
        let pair = profile(1, 0, 1, 0);
        for (p, q) in [(3, 3), (2, 2), (1, 1)] {
            let cell = ComponentCell::new(pair, p, q).unwrap();
            let (topologies, _) = complete(cell);
            let direct = topologies
                .iter()
                .map(|topology| topology.key.clone())
                .collect::<BTreeSet<_>>();
            assert_eq!(direct, brute_force_keys(cell));
        }
    }

    #[test]
    fn manifest_is_sorted_versioned_and_repeatable() {
        let cell = ComponentCell::new(profile(1, 0, 1, 0), 1, 1).unwrap();
        let (_, first) = complete(cell);
        let (_, second) = complete(cell);
        assert_eq!(first, second);
        assert!(
            first
                .canonical_topology_keys
                .windows(2)
                .all(|keys| keys[0] < keys[1])
        );
        assert!(
            first
                .root_digest_input()
                .starts_with(b"satisfactory-reference-component-cell-manifest\0\x01")
        );
        assert_eq!(COMPONENT_MANIFEST_VERSION, 1);
    }

    #[test]
    fn pre_cancelled_cell_never_exposes_a_complete_manifest() {
        let cancel = AtomicBool::new(true);
        let cell = ComponentCell::new(profile(1, 0, 1, 0), 1, 1).unwrap();
        assert_eq!(
            enumerate_component_cell(cell, &cancel).unwrap(),
            ComponentCellEnumeration::Cancelled {
                progress: ComponentEnumerationProgress::default(),
            }
        );
    }

    fn brute_force_keys(cell: ComponentCell) -> BTreeSet<CanonicalGraphKey> {
        let nodes = materialize_nodes(cell.profile).unwrap();
        let producers = producer_ports(cell.boundary_input_count, &nodes);
        let mut permutations = Vec::new();
        permute_consumers(
            &mut consumer_ports(cell.boundary_output_count, &nodes),
            0,
            &mut permutations,
        );
        let problem = synthetic_problem(cell);
        permutations
            .into_iter()
            .filter_map(|consumers| {
                let links = producers.iter().copied().zip(consumers).collect::<Vec<_>>();
                let legal = links
                    .iter()
                    .all(|&(producer, consumer)| match (producer, consumer) {
                        (ProducerPortRef::Input(_), ConsumerPortRef::Node { .. })
                        | (ProducerPortRef::Node { .. }, ConsumerPortRef::Output(_)) => true,
                        (
                            ProducerPortRef::Node { node: producer, .. },
                            ConsumerPortRef::Node { node: consumer, .. },
                        ) => producer != consumer,
                        _ => false,
                    });
                legal.then(|| {
                    canonicalize_graph(
                        &problem,
                        &PhysicalGraph {
                            nodes: nodes.clone(),
                            links: links
                                .into_iter()
                                .map(|(producer, consumer)| PhysicalLink {
                                    producer,
                                    consumer,
                                    flow: Rational::zero(),
                                })
                                .collect(),
                        },
                    )
                    .key
                })
            })
            .collect()
    }

    fn permute_consumers(
        values: &mut [ConsumerPortRef],
        start: usize,
        permutations: &mut Vec<Vec<ConsumerPortRef>>,
    ) {
        if start == values.len() {
            permutations.push(values.to_vec());
            return;
        }
        for selected in start..values.len() {
            values.swap(start, selected);
            permute_consumers(values, start + 1, permutations);
            values.swap(start, selected);
        }
    }
}
