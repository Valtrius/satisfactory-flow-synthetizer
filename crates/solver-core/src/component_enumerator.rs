//! Capacity-independent exhaustive enumeration of bounded physical components.
//!
//! A prewarm cell fixes a nonempty physical [`NodeProfile`] and the two
//! boundary arities.  Port balance then fixes the internal-link count
//! `k = P - q = C - p >= 0`.  Enumeration visits every legal complete physical
//! port bijection in deterministic root partitions and only deduplicates after
//! complete-witness canonicalization.  In particular, algebraic singularity is
//! never used to prune structural enumeration: it merely means that the
//! topology cannot be frozen into a reusable component record.
//!
//! This module is deliberately independent of concrete rates and capacity.
//! Exact `R`, `T`, `K`, positivity domain, and intrinsic capacity behavior are
//! derived only after a complete topology is proven uniquely solvable.

use std::{
    collections::{BTreeMap, BTreeSet, btree_map::Entry},
    sync::atomic::{AtomicBool, Ordering},
};

use solver_api::{
    CanonicalGraphKey, ConsumerPortRef, InputTerminalIndex, NodeId, NodeProfile, NodeType,
    OutputTerminalIndex, PhysicalGraph, PhysicalLink, PhysicalNode, Problem, ProducerPortRef,
    Rational,
};
use thiserror::Error;

use crate::{
    canonical::{PartialLink, PartialTopology, canonicalize_witness},
    components::{Component, ComponentError},
    scc::{
        DeclaredBoundaryInput, DeclaredBoundaryOutput, FrozenSubsystemAnalysis,
        FrozenSubsystemDeclaration, SccError, analyze_frozen_subsystem,
    },
    topology::{TopologyError, TopologyState},
};

/// Stable component-cell manifest protocol shared with the independent oracle.
pub const COMPONENT_CELL_MANIFEST_VERSION: u32 = 1;

/// Stable production root-partition manifest protocol.
pub const COMPONENT_PARTITION_MANIFEST_VERSION: u32 = 1;

/// One finite, exactly port-balanced component enumeration cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComponentCell {
    /// Exact physical-node inventory.
    pub profile: NodeProfile,
    /// Number of declared boundary inputs `p`.
    pub boundary_input_count: u32,
    /// Number of declared boundary outputs `q`.
    pub boundary_output_count: u32,
    /// Forced number of node-to-node physical links `k`.
    pub internal_link_count: u32,
}

impl ComponentCell {
    /// Constructs one valid cell with `N,p,q >= 1` and
    /// `P-q = C-p = k >= 0`.
    ///
    /// # Errors
    ///
    /// Returns an exact count or balance error for an invalid cell.
    pub fn new(
        profile: NodeProfile,
        boundary_input_count: u32,
        boundary_output_count: u32,
    ) -> Result<Self, ComponentEnumerationError> {
        if checked_node_count(profile)? == 0 {
            return Err(ComponentEnumerationError::EmptyProfile);
        }
        if boundary_input_count == 0 {
            return Err(ComponentEnumerationError::MissingBoundaryInputs);
        }
        if boundary_output_count == 0 {
            return Err(ComponentEnumerationError::MissingBoundaryOutputs);
        }
        let producer_ports = checked_producer_port_count(profile)?;
        let consumer_ports = checked_consumer_port_count(profile)?;
        let producer_internal = producer_ports.checked_sub(boundary_output_count);
        let consumer_internal = consumer_ports.checked_sub(boundary_input_count);
        let Some(internal_link_count) = producer_internal
            .zip(consumer_internal)
            .and_then(|(producer, consumer)| (producer == consumer).then_some(producer))
        else {
            return Err(ComponentEnumerationError::UnbalancedCell {
                producer_ports,
                consumer_ports,
                boundary_input_count,
                boundary_output_count,
            });
        };
        internal_link_count
            .checked_add(boundary_input_count)
            .and_then(|count| count.checked_add(boundary_output_count))
            .ok_or(ComponentEnumerationError::PhysicalLinkCountOverflow)?;
        Ok(Self {
            profile,
            boundary_input_count,
            boundary_output_count,
            internal_link_count,
        })
    }

    /// Returns the number of physical links including synthetic boundary links.
    #[must_use]
    pub const fn physical_link_count(self) -> u32 {
        self.internal_link_count + self.boundary_input_count + self.boundary_output_count
    }
}

/// Finite bounds for deterministic cell generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComponentEnumerationBounds {
    /// Inclusive physical-node bound. Cells always have at least one node.
    pub max_nodes: u32,
    /// Optional inclusive bound on `p + q`.
    ///
    /// `None` is still finite because the selected node ports bound both sides.
    pub max_boundary_ports: Option<u32>,
}

/// A deterministic root partition, fixing the consumer attached to boundary input zero.
///
/// Boundary input zero is always the first producer in the labeled DFS. Every
/// complete labeled topology therefore belongs to exactly one such partition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComponentRootPartition {
    /// Index in the deterministic node-consumer-port ordering.
    pub root_consumer_index: u32,
}

/// Exact enumeration progress, suitable for cancellation diagnostics only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ComponentEnumerationProgress {
    /// Recursive DFS states visited.
    pub search_states_visited: u64,
    /// Complete labeled port bijections reached.
    pub labeled_topologies_completed: u64,
    /// New canonical physical topology identities retained.
    pub canonical_topologies_found: u64,
    /// Canonical topologies proven uniquely solvable and frozen.
    pub components_frozen: u64,
}

impl ComponentEnumerationProgress {
    fn checked_add(self, other: Self) -> Result<Self, ComponentEnumerationError> {
        Ok(Self {
            search_states_visited: self
                .search_states_visited
                .checked_add(other.search_states_visited)
                .ok_or(ComponentEnumerationError::ProgressOverflow)?,
            labeled_topologies_completed: self
                .labeled_topologies_completed
                .checked_add(other.labeled_topologies_completed)
                .ok_or(ComponentEnumerationError::ProgressOverflow)?,
            canonical_topologies_found: self
                .canonical_topologies_found
                .checked_add(other.canonical_topologies_found)
                .ok_or(ComponentEnumerationError::ProgressOverflow)?,
            components_frozen: self
                .components_frozen
                .checked_add(other.components_frozen)
                .ok_or(ComponentEnumerationError::ProgressOverflow)?,
        })
    }
}

/// Complete canonical topology manifest for one root partition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentPartitionManifest {
    /// Cell owning this root partition.
    pub cell: ComponentCell,
    /// Exhausted deterministic partition.
    pub partition: ComponentRootPartition,
    /// Sorted, duplicate-free canonical topology identities.
    pub canonical_topology_keys: Vec<CanonicalGraphKey>,
}

impl ComponentPartitionManifest {
    /// Returns stable versioned bytes suitable for cryptographic hashing.
    #[must_use]
    pub fn digest_input(&self) -> Vec<u8> {
        let mut bytes = b"satisfactory-production-component-partition-manifest\0\x01".to_vec();
        encode_cell(&mut bytes, self.cell);
        write_u32(&mut bytes, self.partition.root_consumer_index);
        write_keys(&mut bytes, &self.canonical_topology_keys);
        bytes
    }
}

/// Complete canonical topology manifest for one entire cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentCellManifest {
    /// Exhausted cell.
    pub cell: ComponentCell,
    /// Sorted, duplicate-free union of all partition topology identities.
    pub canonical_topology_keys: Vec<CanonicalGraphKey>,
}

impl ComponentCellManifest {
    /// Returns the oracle-compatible stable versioned root input.
    #[must_use]
    pub fn root_digest_input(&self) -> Vec<u8> {
        let mut bytes = b"satisfactory-reference-component-cell-manifest\0\x01".to_vec();
        encode_cell(&mut bytes, self.cell);
        write_keys(&mut bytes, &self.canonical_topology_keys);
        bytes
    }
}

/// Complete output of one root partition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompleteComponentPartition {
    /// Exact topology coverage proof input.
    pub manifest: ComponentPartitionManifest,
    /// Every uniquely-solvable component found, ordered by canonical key.
    pub components: Vec<Component>,
    /// Exact work totals for diagnostics.
    pub progress: ComponentEnumerationProgress,
}

/// Outcome of cancellable root-partition enumeration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComponentPartitionEnumeration {
    /// The complete labeled search space for the partition was exhausted.
    Complete(CompleteComponentPartition),
    /// Enumeration stopped before proof completion; no manifest or components are exposed.
    Cancelled {
        /// Work completed before cancellation was observed.
        progress: ComponentEnumerationProgress,
    },
}

/// Complete output of one cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompleteComponentCell {
    /// Exact cell topology coverage proof input.
    pub manifest: ComponentCellManifest,
    /// Complete per-root proof inputs in partition order.
    pub partitions: Vec<ComponentPartitionManifest>,
    /// Every unique solvable component, ordered by canonical key.
    pub components: Vec<Component>,
    /// Aggregate exact enumeration work.
    pub progress: ComponentEnumerationProgress,
}

/// Outcome of cancellable complete-cell enumeration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComponentCellEnumeration {
    /// Every root partition completed.
    Complete(CompleteComponentCell),
    /// At least one root partition was interrupted. No cell manifest is exposed.
    Cancelled {
        /// Number of whole root partitions completed before interruption.
        completed_partitions: u32,
        /// Aggregate work, including the interrupted partition.
        progress: ComponentEnumerationProgress,
    },
}

/// Invalid bounds, counts, topology reconstruction, or exact component analysis.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ComponentEnumerationError {
    #[error("a component cell must contain at least one physical node")]
    EmptyProfile,
    #[error("a component cell must expose at least one boundary input")]
    MissingBoundaryInputs,
    #[error("a component cell must expose at least one boundary output")]
    MissingBoundaryOutputs,
    #[error("component node count overflowed u32")]
    NodeCountOverflow,
    #[error("component producer-port count overflowed u32")]
    ProducerPortCountOverflow,
    #[error("component consumer-port count overflowed u32")]
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
    #[error("component root partition {index} is outside {partition_count} partitions")]
    InvalidPartition { index: u32, partition_count: u32 },
    #[error("component enumeration progress overflowed u64")]
    ProgressOverflow,
    #[error("canonical component topology violated its boundary invariant")]
    CanonicalBoundaryInvariant,
    #[error(transparent)]
    Topology(#[from] TopologyError),
    #[error(transparent)]
    Scc(#[from] SccError),
    #[error(transparent)]
    Component(#[from] ComponentError),
}

/// Generates every finite cell admitted by the supplied bounds.
///
/// Cells are ordered by `(N, profile, p, q, k)`. No feasibility or topology
/// pruning occurs here.
///
/// # Errors
///
/// Returns a checked-count error if a generated profile cannot be represented.
pub fn component_cells(
    bounds: ComponentEnumerationBounds,
) -> Result<Vec<ComponentCell>, ComponentEnumerationError> {
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

/// Returns all deterministic root partitions for a valid cell.
///
/// # Errors
///
/// Returns an error if the supplied public cell fields are inconsistent.
pub fn component_partitions(
    cell: ComponentCell,
) -> Result<Vec<ComponentRootPartition>, ComponentEnumerationError> {
    validate_cell(cell)?;
    Ok((0..checked_consumer_port_count(cell.profile)?)
        .map(|root_consumer_index| ComponentRootPartition {
            root_consumer_index,
        })
        .collect())
}

/// Exhausts one deterministic root partition independently of rates and capacity.
///
/// The cancellation flag is checked at every DFS state. A cancelled result does
/// not expose partial components or a manifest, so it cannot be mistaken for a
/// proof-complete partition.
///
/// # Errors
///
/// Returns a checked-count, topology, or exact-analysis error.
pub fn enumerate_component_partition(
    cell: ComponentCell,
    partition: ComponentRootPartition,
    cancel: &AtomicBool,
) -> Result<ComponentPartitionEnumeration, ComponentEnumerationError> {
    validate_cell(cell)?;
    let nodes = materialize_nodes(cell.profile)?;
    let producers = producer_ports(cell.boundary_input_count, &nodes);
    let consumers = consumer_ports(cell.boundary_output_count, &nodes);
    let partition_count = checked_consumer_port_count(cell.profile)?;
    if partition.root_consumer_index >= partition_count {
        return Err(ComponentEnumerationError::InvalidPartition {
            index: partition.root_consumer_index,
            partition_count,
        });
    }
    if cancel.load(Ordering::Relaxed) {
        return Ok(ComponentPartitionEnumeration::Cancelled {
            progress: ComponentEnumerationProgress::default(),
        });
    }

    let physical_link_count = usize::try_from(cell.physical_link_count())
        .map_err(|_| ComponentEnumerationError::PhysicalLinkCountOverflow)?;
    debug_assert_eq!(producers.len(), consumers.len());
    debug_assert_eq!(producers.len(), physical_link_count);
    let root_consumer_index = usize::try_from(partition.root_consumer_index)
        .map_err(|_| ComponentEnumerationError::ConsumerPortCountOverflow)?;
    let root_consumer = consumers[root_consumer_index];
    debug_assert!(legal_component_link(producers[0], root_consumer));
    let mut enumerator = PartitionEnumerator {
        cell,
        cancel,
        problem: synthetic_problem(cell),
        nodes,
        producers,
        consumers,
        used_consumers: vec![false; physical_link_count],
        links: vec![(ProducerPortRef::Input(InputTerminalIndex(0)), root_consumer)],
        canonical_topologies: BTreeSet::new(),
        components: BTreeMap::new(),
        progress: ComponentEnumerationProgress::default(),
    };
    enumerator.used_consumers[root_consumer_index] = true;
    if enumerator.enumerate_from(1)? || cancel.load(Ordering::Relaxed) {
        return Ok(ComponentPartitionEnumeration::Cancelled {
            progress: enumerator.progress,
        });
    }
    let manifest = ComponentPartitionManifest {
        cell,
        partition,
        canonical_topology_keys: enumerator.canonical_topologies.into_iter().collect(),
    };
    Ok(ComponentPartitionEnumeration::Complete(
        CompleteComponentPartition {
            manifest,
            components: enumerator.components.into_values().collect(),
            progress: enumerator.progress,
        },
    ))
}

/// Exhausts every root partition and returns their exact union.
///
/// # Errors
///
/// Returns a checked-count, topology, or exact-analysis error.
pub fn enumerate_component_cell(
    cell: ComponentCell,
    cancel: &AtomicBool,
) -> Result<ComponentCellEnumeration, ComponentEnumerationError> {
    let partitions = component_partitions(cell)?;
    let mut manifests = Vec::with_capacity(partitions.len());
    let mut topology_keys = BTreeSet::new();
    let mut components = BTreeMap::new();
    let mut progress = ComponentEnumerationProgress::default();
    for (index, partition) in partitions.into_iter().enumerate() {
        match enumerate_component_partition(cell, partition, cancel)? {
            ComponentPartitionEnumeration::Complete(complete) => {
                progress = progress.checked_add(complete.progress)?;
                topology_keys.extend(complete.manifest.canonical_topology_keys.iter().cloned());
                for component in complete.components {
                    components
                        .entry(component.canonical_key().clone())
                        .or_insert(component);
                }
                manifests.push(complete.manifest);
            }
            ComponentPartitionEnumeration::Cancelled {
                progress: interrupted,
            } => {
                progress = progress.checked_add(interrupted)?;
                return Ok(ComponentCellEnumeration::Cancelled {
                    completed_partitions: u32::try_from(index)
                        .map_err(|_| ComponentEnumerationError::ProgressOverflow)?,
                    progress,
                });
            }
        }
    }
    Ok(ComponentCellEnumeration::Complete(CompleteComponentCell {
        manifest: ComponentCellManifest {
            cell,
            canonical_topology_keys: topology_keys.into_iter().collect(),
        },
        partitions: manifests,
        components: components.into_values().collect(),
        progress,
    }))
}

struct PartitionEnumerator<'a> {
    cell: ComponentCell,
    cancel: &'a AtomicBool,
    problem: Problem,
    nodes: Vec<PhysicalNode>,
    producers: Vec<ProducerPortRef>,
    consumers: Vec<ConsumerPortRef>,
    used_consumers: Vec<bool>,
    links: Vec<(ProducerPortRef, ConsumerPortRef)>,
    canonical_topologies: BTreeSet<CanonicalGraphKey>,
    components: BTreeMap<crate::components::ComponentCanonicalKey, Component>,
    progress: ComponentEnumerationProgress,
}

impl PartitionEnumerator<'_> {
    /// Returns `true` when cancellation interrupts this subtree.
    fn enumerate_from(&mut self, producer_index: usize) -> Result<bool, ComponentEnumerationError> {
        if self.cancel.load(Ordering::Relaxed) {
            return Ok(true);
        }
        self.progress.search_states_visited = self
            .progress
            .search_states_visited
            .checked_add(1)
            .ok_or(ComponentEnumerationError::ProgressOverflow)?;
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

    fn record_complete_topology(&mut self) -> Result<(), ComponentEnumerationError> {
        self.progress.labeled_topologies_completed = self
            .progress
            .labeled_topologies_completed
            .checked_add(1)
            .ok_or(ComponentEnumerationError::ProgressOverflow)?;
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
        let canonical = canonicalize_witness(&self.problem, &graph);
        if !self.canonical_topologies.insert(canonical.key) {
            return Ok(());
        }
        self.progress.canonical_topologies_found = self
            .progress
            .canonical_topologies_found
            .checked_add(1)
            .ok_or(ComponentEnumerationError::ProgressOverflow)?;
        if let Some(component) = freeze_canonical_topology(self.cell, &canonical.graph)? {
            let Entry::Vacant(entry) = self.components.entry(component.canonical_key().clone())
            else {
                return Ok(());
            };
            entry.insert(component);
            self.progress.components_frozen = self
                .progress
                .components_frozen
                .checked_add(1)
                .ok_or(ComponentEnumerationError::ProgressOverflow)?;
        }
        Ok(())
    }
}

fn freeze_canonical_topology(
    cell: ComponentCell,
    graph: &PhysicalGraph,
) -> Result<Option<Component>, ComponentEnumerationError> {
    let ports = extract_component_ports(cell, graph)?;
    let topology = PartialTopology {
        problem: synthetic_problem(cell),
        nodes: graph.nodes.clone(),
        links: graph
            .links
            .iter()
            .map(|link| PartialLink {
                producer: link.producer,
                consumer: link.consumer,
                flow: None,
            })
            .collect(),
        discard_count: 0,
        remaining_profile: NodeProfile::default(),
    };
    let topology = TopologyState::from_partial_topology(&topology)?;
    let declaration = FrozenSubsystemDeclaration {
        nodes: graph.nodes.iter().map(|node| node.id).collect(),
        boundary_inputs: ports
            .boundary_inputs
            .iter()
            .copied()
            .map(|port| DeclaredBoundaryInput { port })
            .collect(),
        boundary_outputs: ports
            .boundary_outputs
            .iter()
            .copied()
            .map(|port| DeclaredBoundaryOutput { port })
            .collect(),
    };
    match analyze_frozen_subsystem(&topology, &declaration)? {
        FrozenSubsystemAnalysis::Symbolic(frozen) => {
            let component = Component::from_frozen(*frozen)?;
            debug_assert_eq!(
                usize::try_from(component.internal_link_count()).ok(),
                Some(ports.internal_link_count)
            );
            Ok(Some(component))
        }
        FrozenSubsystemAnalysis::Singular { .. } => Ok(None),
    }
}

struct ExtractedComponentPorts {
    internal_link_count: usize,
    boundary_inputs: Vec<ConsumerPortRef>,
    boundary_outputs: Vec<ProducerPortRef>,
}

fn extract_component_ports(
    cell: ComponentCell,
    graph: &PhysicalGraph,
) -> Result<ExtractedComponentPorts, ComponentEnumerationError> {
    let input_count = usize::try_from(cell.boundary_input_count)
        .map_err(|_| ComponentEnumerationError::ConsumerPortCountOverflow)?;
    let output_count = usize::try_from(cell.boundary_output_count)
        .map_err(|_| ComponentEnumerationError::ProducerPortCountOverflow)?;
    let mut boundary_inputs = vec![None; input_count];
    let mut boundary_outputs = vec![None; output_count];
    let mut internal_link_count = 0_usize;
    for link in &graph.links {
        match (link.producer, link.consumer) {
            (ProducerPortRef::Input(input), consumer @ ConsumerPortRef::Node { .. }) => {
                let slot = usize::try_from(input.0)
                    .ok()
                    .and_then(|index| boundary_inputs.get_mut(index))
                    .ok_or(ComponentEnumerationError::CanonicalBoundaryInvariant)?;
                if slot.replace(consumer).is_some() {
                    return Err(ComponentEnumerationError::CanonicalBoundaryInvariant);
                }
            }
            (producer @ ProducerPortRef::Node { .. }, ConsumerPortRef::Output(output)) => {
                let slot = usize::try_from(output.0)
                    .ok()
                    .and_then(|index| boundary_outputs.get_mut(index))
                    .ok_or(ComponentEnumerationError::CanonicalBoundaryInvariant)?;
                if slot.replace(producer).is_some() {
                    return Err(ComponentEnumerationError::CanonicalBoundaryInvariant);
                }
            }
            (ProducerPortRef::Node { .. }, ConsumerPortRef::Node { .. }) => {
                internal_link_count = internal_link_count
                    .checked_add(1)
                    .ok_or(ComponentEnumerationError::PhysicalLinkCountOverflow)?;
            }
            _ => return Err(ComponentEnumerationError::CanonicalBoundaryInvariant),
        }
    }
    if internal_link_count
        != usize::try_from(cell.internal_link_count)
            .map_err(|_| ComponentEnumerationError::PhysicalLinkCountOverflow)?
    {
        return Err(ComponentEnumerationError::CanonicalBoundaryInvariant);
    }
    let boundary_inputs = boundary_inputs
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or(ComponentEnumerationError::CanonicalBoundaryInvariant)?;
    let boundary_outputs = boundary_outputs
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or(ComponentEnumerationError::CanonicalBoundaryInvariant)?;
    Ok(ExtractedComponentPorts {
        internal_link_count,
        boundary_inputs,
        boundary_outputs,
    })
}

fn validate_cell(cell: ComponentCell) -> Result<(), ComponentEnumerationError> {
    let rebuilt = ComponentCell::new(
        cell.profile,
        cell.boundary_input_count,
        cell.boundary_output_count,
    )?;
    if rebuilt == cell {
        Ok(())
    } else {
        Err(ComponentEnumerationError::UnbalancedCell {
            producer_ports: checked_producer_port_count(cell.profile)?,
            consumer_ports: checked_consumer_port_count(cell.profile)?,
            boundary_input_count: cell.boundary_input_count,
            boundary_output_count: cell.boundary_output_count,
        })
    }
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

fn materialize_nodes(profile: NodeProfile) -> Result<Vec<PhysicalNode>, ComponentEnumerationError> {
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
                .ok_or(ComponentEnumerationError::NodeCountOverflow)?;
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

fn checked_node_count(profile: NodeProfile) -> Result<u32, ComponentEnumerationError> {
    profile
        .splitter2
        .checked_add(profile.splitter3)
        .and_then(|count| count.checked_add(profile.merger2))
        .and_then(|count| count.checked_add(profile.merger3))
        .ok_or(ComponentEnumerationError::NodeCountOverflow)
}

fn checked_producer_port_count(profile: NodeProfile) -> Result<u32, ComponentEnumerationError> {
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
        .ok_or(ComponentEnumerationError::ProducerPortCountOverflow)
}

fn checked_consumer_port_count(profile: NodeProfile) -> Result<u32, ComponentEnumerationError> {
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
        .ok_or(ComponentEnumerationError::ConsumerPortCountOverflow)
}

fn encode_cell(bytes: &mut Vec<u8>, cell: ComponentCell) {
    write_u32(bytes, cell.profile.splitter2);
    write_u32(bytes, cell.profile.splitter3);
    write_u32(bytes, cell.profile.merger2);
    write_u32(bytes, cell.profile.merger3);
    write_u32(bytes, cell.boundary_input_count);
    write_u32(bytes, cell.boundary_output_count);
    write_u32(bytes, cell.internal_link_count);
}

fn write_keys(bytes: &mut Vec<u8>, keys: &[CanonicalGraphKey]) {
    write_length(bytes, keys.len());
    for key in keys {
        write_length(bytes, key.as_bytes().len());
        bytes.extend_from_slice(key.as_bytes());
    }
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
    use super::*;
    use solver_reference::{
        ComponentCell as ReferenceCell, ComponentCellEnumeration as ReferenceEnumeration,
        ComponentEnumerationBounds as ReferenceBounds, component_cells as reference_cells,
        enumerate_component_cell as reference_enumerate,
    };

    fn profile(splitter2: u32, splitter3: u32, merger2: u32, merger3: u32) -> NodeProfile {
        NodeProfile {
            splitter2,
            splitter3,
            merger2,
            merger3,
        }
    }

    fn complete(cell: ComponentCell) -> CompleteComponentCell {
        match enumerate_component_cell(cell, &AtomicBool::new(false)).unwrap() {
            ComponentCellEnumeration::Complete(complete) => complete,
            ComponentCellEnumeration::Cancelled { .. } => panic!("unexpected cancellation"),
        }
    }

    #[test]
    fn bounded_cells_exactly_match_the_independent_reference() {
        for bounds in [
            ComponentEnumerationBounds {
                max_nodes: 1,
                max_boundary_ports: None,
            },
            ComponentEnumerationBounds {
                max_nodes: 2,
                max_boundary_ports: Some(3),
            },
        ] {
            let production = component_cells(bounds).unwrap();
            let reference = reference_cells(ReferenceBounds {
                max_nodes: bounds.max_nodes,
                max_boundary_ports: bounds.max_boundary_ports,
            })
            .unwrap();
            assert_eq!(production.len(), reference.len());
            assert!(production.iter().zip(reference).all(|(left, right)| {
                left.profile == right.profile
                    && left.boundary_input_count == right.boundary_input_count
                    && left.boundary_output_count == right.boundary_output_count
                    && left.internal_link_count == right.internal_link_count
            }));
        }
    }

    #[test]
    fn production_cell_manifests_match_the_independent_oracle_exactly() {
        let cells = component_cells(ComponentEnumerationBounds {
            max_nodes: 2,
            max_boundary_ports: Some(3),
        })
        .unwrap();
        for cell in cells {
            let production = complete(cell);
            let reference_cell = ReferenceCell::new(
                cell.profile,
                cell.boundary_input_count,
                cell.boundary_output_count,
            )
            .unwrap();
            let ReferenceEnumeration::Complete {
                topologies: _,
                manifest,
            } = reference_enumerate(reference_cell, &AtomicBool::new(false)).unwrap()
            else {
                panic!("unexpected reference cancellation");
            };
            assert_eq!(
                production.manifest.canonical_topology_keys, manifest.canonical_topology_keys,
                "manifest mismatch for {cell:?}",
            );
            assert_eq!(
                production.manifest.root_digest_input(),
                manifest.root_digest_input(),
                "root input mismatch for {cell:?}",
            );
        }
    }

    #[test]
    fn partitions_form_an_exact_union_and_freeze_only_symbolic_components() {
        let cell = ComponentCell::new(profile(1, 0, 1, 0), 1, 1).unwrap();
        let complete = complete(cell);
        let partition_union = complete
            .partitions
            .iter()
            .flat_map(|partition| partition.canonical_topology_keys.iter().cloned())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            partition_union.into_iter().collect::<Vec<_>>(),
            complete.manifest.canonical_topology_keys,
        );
        assert!(
            complete
                .components
                .iter()
                .all(|component| component.profile() == cell.profile
                    && component.internal_link_count() == cell.internal_link_count)
        );
        assert!(
            complete
                .partitions
                .windows(2)
                .all(|partitions| partitions[0].partition < partitions[1].partition)
        );
    }

    #[test]
    fn cancellation_never_exposes_a_partition_or_cell_manifest() {
        let cell = ComponentCell::new(profile(1, 0, 1, 0), 1, 1).unwrap();
        let cancel = AtomicBool::new(true);
        assert!(matches!(
            enumerate_component_partition(
                cell,
                ComponentRootPartition {
                    root_consumer_index: 0,
                },
                &cancel,
            )
            .unwrap(),
            ComponentPartitionEnumeration::Cancelled { .. },
        ));
        assert!(matches!(
            enumerate_component_cell(cell, &cancel).unwrap(),
            ComponentCellEnumeration::Cancelled { .. },
        ));
    }
}
