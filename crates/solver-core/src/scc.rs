//! Dynamic SCC analysis and exact frozen-subsystem contracts.
//!
//! Dynamic graph SCCs may grow as links are added. Open summaries retain their
//! current immutable equations for contradiction detection and deductions, but
//! rank deficiency alone has no pruning meaning. Separately, callers may make
//! an explicit frozen-subsystem declaration. Certification then fixes all
//! internal nodes and links, exposes only declared boundaries, and extracts an
//! immutable exact contract. External links may later place that contracted
//! subsystem inside a larger dynamic graph SCC.

use std::collections::{BTreeMap, BTreeSet};

use num::{BigInt, BigRational, Integer, One, Zero};
use petgraph::{algo::kosaraju_scc, graphmap::DiGraphMap};
use solver_api::{ConsumerPortRef, NodeId, NodeType, PhysicalNode, ProducerPortRef, Rational};
use thiserror::Error;

use crate::{
    algebra::sparse::{SparseAlgebraError, SparseAnalysis, SparseRow, SparseSystem},
    canonical::{MarkedLinkCanonicalKey, canonicalize_marked_link},
    topology::{FlowVarId, SealedMacroInstance, TopologyState},
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
    /// Frozen component instances analyzed through their immutable quotient contracts.
    pub contracted_macro_count: usize,
    /// Exact fraction-free analysis with supplied and terminal-known values substituted.
    ///
    /// `is_unique() == false` is descriptive only while this region is open.
    pub algebra: SparseAnalysis,
}

/// One consumer port exposed as a frozen subsystem input.
///
/// The port may currently be open or connected from outside the selected node
/// set. Once certified, a currently open declared boundary may be attached by
/// component composition; undeclared selected-node ports are frozen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeclaredBoundaryInput {
    /// Selected-node consumer port carrying the symbolic boundary flow.
    pub port: ConsumerPortRef,
}

/// One producer port exposed as a frozen subsystem output.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeclaredBoundaryOutput {
    /// Selected-node producer port carrying the symbolic boundary flow.
    pub port: ProducerPortRef,
}

/// Explicit request to freeze one internal subsystem.
///
/// Vector order defines the local matrix coordinates. These topology-local raw
/// identifiers are witness reconstruction data, never a persistence/cache key;
/// Milestone 6 wraps the contract in a jointly canonical boundary encoding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrozenSubsystemDeclaration {
    /// Fixed internal node identities.
    pub nodes: Vec<NodeId>,
    /// Every selected-node consumer port not connected internally, exactly once.
    pub boundary_inputs: Vec<DeclaredBoundaryInput>,
    /// Every selected-node producer port not connected internally, exactly once.
    pub boundary_outputs: Vec<DeclaredBoundaryOutput>,
}

/// Dense row-major arbitrary-precision rational matrix.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RationalMatrix {
    rows: usize,
    columns: usize,
    entries: Vec<Rational>,
}

impl RationalMatrix {
    fn from_rows(rows: Vec<Vec<Rational>>, columns: usize) -> Self {
        debug_assert!(rows.iter().all(|row| row.len() == columns));
        Self {
            rows: rows.len(),
            columns,
            entries: rows.into_iter().flatten().collect(),
        }
    }

    /// Returns the number of rows.
    #[must_use]
    pub const fn row_count(&self) -> usize {
        self.rows
    }

    /// Returns the number of columns.
    #[must_use]
    pub const fn column_count(&self) -> usize {
        self.columns
    }

    /// Borrows one row, or returns `None` when `row` is out of range.
    #[must_use]
    pub fn row(&self, row: usize) -> Option<&[Rational]> {
        let start = row.checked_mul(self.columns)?;
        let end = start.checked_add(self.columns)?;
        self.entries.get(start..end)
    }

    /// Multiplies this matrix by an exact column vector.
    ///
    /// Returns `None` when the vector dimension differs from `column_count`.
    #[must_use]
    pub fn multiply(&self, vector: &[Rational]) -> Option<Vec<Rational>> {
        if vector.len() != self.columns {
            return None;
        }
        (0..self.rows)
            .map(|row| {
                self.row(row).map(|values| {
                    values
                        .iter()
                        .zip(vector)
                        .map(|(coefficient, value)| coefficient * value)
                        .sum()
                })
            })
            .collect()
    }
}

/// One immutable physical link and its exact symbolic input-flow row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrozenInternalLink {
    /// Relabeling- and storage-order-invariant marked-link identity.
    ///
    /// This key is scoped to the certification topology. Milestone 6 replaces
    /// it with the canonical component-subgraph identity used for persistence.
    pub canonical_marked_link_key: MarkedLinkCanonicalKey,
    /// Link index in the topology at certification time, retained for flattening.
    pub link_index: usize,
    /// Frozen producer endpoint.
    pub producer: ProducerPortRef,
    /// Frozen consumer endpoint.
    pub consumer: ConsumerPortRef,
    /// Coefficients mapping declared inputs to this link's flow.
    pub coefficients: Vec<Rational>,
}

/// One selected producer port and its exact symbolic input-flow row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProducedFlowRow {
    /// Producer port whose flow is represented.
    pub producer: ProducerPortRef,
    /// Coefficients mapping declared inputs to that producer's flow.
    pub coefficients: Vec<Rational>,
}

/// Capacity-independent exact contract for a frozen internal subsystem.
///
/// With boundary input vector `x`, every selected producer flow is `P*x`,
/// every physical internal link is one corresponding row of `K*x`, declared
/// outputs are `T*x`, and the necessary and sufficient boundary equalities are
/// `R*x=0`. Full column rank of the internal producer block makes these maps
/// unique on that exact domain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrozenSubsystem {
    /// Boundary declaration in the coordinate order used by these matrices.
    pub declaration: FrozenSubsystemDeclaration,
    /// Fixed node types and identities, sorted by node identifier.
    pub fixed_nodes: Vec<PhysicalNode>,
    /// Fixed physical internal links and their symbolic flow rows.
    pub internal_links: Vec<FrozenInternalLink>,
    /// Selected producer identities aligned with `all_produced_flow_map` rows.
    pub produced_flows: Vec<ProducedFlowRow>,
    /// Exact residual boundary domain `R*x=0` in canonical RREF row basis.
    pub boundary_domain: RationalMatrix,
    /// Exact map for every selected-node producer port.
    pub all_produced_flow_map: RationalMatrix,
    /// Exact map for physical links internal to the frozen subsystem.
    pub internal_flow_map: RationalMatrix,
    /// Exact map from declared inputs to declared outputs.
    pub transfer_map: RationalMatrix,
}

/// Result of conditional symbolic analysis of a valid frozen declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrozenSubsystemAnalysis {
    /// Internal producer flows are unique functions of boundary inputs.
    Symbolic(Box<FrozenSubsystem>),
    /// The internal producer block lacks full column rank.
    ///
    /// This prevents certification only; it is never a proof that the physical
    /// search branch is impossible.
    Singular {
        /// Exact internal-producer coefficient rank.
        producer_rank: usize,
        /// Number of selected producer variables.
        producer_count: usize,
    },
}

/// Exact reason a concrete boundary vector does not satisfy a frozen contract.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum FrozenSubsystemRejection {
    /// Caller supplied the wrong boundary-input dimension.
    #[error("expected {expected} boundary inputs, got {actual}")]
    InputCount { expected: usize, actual: usize },
    /// Physical link capacity must itself be strictly positive.
    #[error("physical link capacity must be strictly positive")]
    NonPositiveCapacity,
    /// A declared input violates mandatory physical-link positivity.
    #[error("boundary input {index} is not strictly positive: {value}")]
    NonPositiveInput { index: usize, value: Rational },
    /// A declared input exceeds mandatory physical-link capacity.
    #[error("boundary input {index} exceeds capacity {capacity}: {value}")]
    InputCapacityExceeded {
        index: usize,
        value: Box<Rational>,
        capacity: Box<Rational>,
    },
    /// Boundary inputs violate one exact residual equality.
    #[error("boundary inputs violate residual domain row {row}: {value}")]
    BoundaryDomain { row: usize, value: Rational },
    /// A selected producer (hence one internal or output physical link) is nonpositive.
    #[error("selected producer row {row} is not strictly positive: {value}")]
    NonPositiveProducedFlow { row: usize, value: Rational },
    /// A selected producer exceeds mandatory physical-link capacity.
    #[error("selected producer row {row} exceeds capacity {capacity}: {value}")]
    ProducedFlowCapacityExceeded {
        row: usize,
        value: Box<Rational>,
        capacity: Box<Rational>,
    },
}

/// Exact concrete application of a frozen symbolic contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrozenSubsystemApplication {
    /// Declared inputs in contract coordinate order.
    pub inputs: Vec<Rational>,
    /// Declared outputs in contract coordinate order.
    pub outputs: Vec<Rational>,
    /// Physical internal-link flows aligned with `FrozenSubsystem::internal_links`.
    pub internal_flows: Vec<Rational>,
    /// All selected producer flows aligned with `FrozenSubsystem::produced_flows`.
    pub produced_flows: Vec<Rational>,
    /// Maximum internal physical-link flow, or `None` when there is no internal link.
    pub peak_internal_flow: Option<Rational>,
}

impl FrozenSubsystem {
    /// Applies exact boundary values and enforces every physical-link bound.
    ///
    /// Declaration validation proves that selected producer ports partition
    /// into internal-link producers and declared outputs. Consequently checking
    /// every row of `all_produced_flow_map`, together with every declared input,
    /// checks `0 < f <= B` for every physical link represented by the contract.
    ///
    /// # Errors
    ///
    /// Returns a structured exact rejection for a dimension, residual-domain,
    /// positivity, or capacity violation.
    pub fn evaluate(
        &self,
        inputs: &[Rational],
        capacity: &Rational,
    ) -> Result<FrozenSubsystemApplication, FrozenSubsystemRejection> {
        if inputs.len() != self.boundary_domain.column_count() {
            return Err(FrozenSubsystemRejection::InputCount {
                expected: self.boundary_domain.column_count(),
                actual: inputs.len(),
            });
        }
        if !capacity.is_positive() {
            return Err(FrozenSubsystemRejection::NonPositiveCapacity);
        }
        for (index, value) in inputs.iter().enumerate() {
            if !value.is_positive() {
                return Err(FrozenSubsystemRejection::NonPositiveInput {
                    index,
                    value: value.clone(),
                });
            }
            if value > capacity {
                return Err(FrozenSubsystemRejection::InputCapacityExceeded {
                    index,
                    value: Box::new(value.clone()),
                    capacity: Box::new(capacity.clone()),
                });
            }
        }

        let Some(domain_values) = self.boundary_domain.multiply(inputs) else {
            return Err(FrozenSubsystemRejection::InputCount {
                expected: self.boundary_domain.column_count(),
                actual: inputs.len(),
            });
        };
        for (row, value) in domain_values.into_iter().enumerate() {
            if !value.is_zero() {
                return Err(FrozenSubsystemRejection::BoundaryDomain { row, value });
            }
        }

        let Some(produced_flows) = self.all_produced_flow_map.multiply(inputs) else {
            return Err(FrozenSubsystemRejection::InputCount {
                expected: self.all_produced_flow_map.column_count(),
                actual: inputs.len(),
            });
        };
        for (row, value) in produced_flows.iter().enumerate() {
            if !value.is_positive() {
                return Err(FrozenSubsystemRejection::NonPositiveProducedFlow {
                    row,
                    value: value.clone(),
                });
            }
            if value > capacity {
                return Err(FrozenSubsystemRejection::ProducedFlowCapacityExceeded {
                    row,
                    value: Box::new(value.clone()),
                    capacity: Box::new(capacity.clone()),
                });
            }
        }

        let Some(internal_flows) = self.internal_flow_map.multiply(inputs) else {
            return Err(FrozenSubsystemRejection::InputCount {
                expected: self.internal_flow_map.column_count(),
                actual: inputs.len(),
            });
        };
        let Some(outputs) = self.transfer_map.multiply(inputs) else {
            return Err(FrozenSubsystemRejection::InputCount {
                expected: self.transfer_map.column_count(),
                actual: inputs.len(),
            });
        };
        let peak_internal_flow = internal_flows.iter().max().cloned();
        Ok(FrozenSubsystemApplication {
            inputs: inputs.to_vec(),
            outputs,
            internal_flows,
            produced_flows,
            peak_internal_flow,
        })
    }
}

/// Malformed input or internal exact-algebra failure during SCC analysis.
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
    /// Rollback-tracked frozen component metadata contradicted the flattened state.
    #[error("sealed component contraction is malformed: {0}")]
    MalformedSealedMacro(&'static str),
    /// Caller-supplied and topology-known values disagree exactly.
    #[error("conflicting exact known values for {variable:?}: {left} versus {right}")]
    ConflictingKnownValue {
        variable: FlowVarId,
        left: Box<Rational>,
        right: Box<Rational>,
    },
    /// A frozen subsystem must contain at least one physical node.
    #[error("a frozen subsystem declaration has no nodes")]
    EmptyFrozenSubsystem,
    /// The declaration named one selected node more than once.
    #[error("frozen subsystem declaration repeats node {node:?}")]
    DuplicateDeclaredNode { node: NodeId },
    /// A declared node is not materialized in the current topology.
    #[error("frozen subsystem declaration refers to missing node {node:?}")]
    MissingDeclaredNode { node: NodeId },
    /// A useful frozen subsystem must expose at least one input boundary.
    #[error("a frozen subsystem declaration has no boundary inputs")]
    MissingBoundaryInputs,
    /// A useful frozen subsystem must expose at least one output boundary.
    #[error("a frozen subsystem declaration has no boundary outputs")]
    MissingBoundaryOutputs,
    /// One input boundary was declared more than once.
    #[error("frozen subsystem declaration repeats input boundary {port:?}")]
    DuplicateBoundaryInput { port: ConsumerPortRef },
    /// One output boundary was declared more than once.
    #[error("frozen subsystem declaration repeats output boundary {port:?}")]
    DuplicateBoundaryOutput { port: ProducerPortRef },
    /// A declared boundary port is absent or is not owned by a selected node.
    #[error("declared boundary port is absent or outside the frozen subsystem")]
    InvalidBoundaryPort,
    /// A declared boundary is connected to another selected-node port.
    #[error("a declared boundary port is connected internally")]
    BoundaryConnectedInternally,
    /// An undeclared selected-node port is open or connected outside the subsystem.
    #[error("an undeclared selected-node port is not connected internally")]
    UndeclaredPortNotInternal,
    /// Dense symbolic Bareiss elimination violated its exact-division invariant.
    #[error("symbolic Bareiss division was non-exact at row {row}, column {column}")]
    NonExactSymbolicDivision { row: usize, column: usize },
    /// Fraction-free sparse elimination violated an internal exactness invariant.
    #[error(transparent)]
    Sparse(#[from] SparseAlgebraError),
}

/// One vertex in the dynamic graph after every frozen subsystem is contracted.
///
/// A macro's internal physical cycles are already certified and are therefore
/// not dynamic graph cycles. Only later links through declared boundaries can
/// create a quotient self-edge or place the macro in a larger SCC.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum QuotientVertex {
    Primitive(NodeId),
    Macro(usize),
}

fn sealed_macro_owners(
    topology: &TopologyState,
    declared_nodes: &BTreeSet<NodeId>,
) -> Result<BTreeMap<NodeId, usize>, SccError> {
    let mut owners = BTreeMap::new();
    for (macro_index, instance) in topology.sealed_macro_instances().iter().enumerate() {
        if instance.nodes().is_empty() {
            return Err(SccError::MalformedSealedMacro(
                "a contraction has no physical members",
            ));
        }
        for &node in instance.nodes() {
            if !declared_nodes.contains(&node) {
                return Err(SccError::MissingNode { node });
            }
            if owners.insert(node, macro_index).is_some() {
                return Err(SccError::MalformedSealedMacro(
                    "physical node belongs to more than one contraction",
                ));
            }
        }
    }
    Ok(owners)
}

fn sealed_internal_link_owners(
    topology: &TopologyState,
    macro_owners: &BTreeMap<NodeId, usize>,
) -> Result<BTreeMap<usize, usize>, SccError> {
    let mut owners = BTreeMap::new();
    for (macro_index, instance) in topology.sealed_macro_instances().iter().enumerate() {
        for &link_index in instance.internal_link_indices() {
            let link = topology
                .links()
                .get(link_index)
                .ok_or(SccError::LinkIndexOutOfBounds {
                    index: link_index,
                    link_count: topology.links().len(),
                })?;
            let Some(producer) = producer_node(link.producer) else {
                return Err(SccError::MalformedSealedMacro(
                    "an internal link starts outside its contraction",
                ));
            };
            let Some(consumer) = consumer_node(link.consumer) else {
                return Err(SccError::MalformedSealedMacro(
                    "an internal link ends outside its contraction",
                ));
            };
            if macro_owners.get(&producer) != Some(&macro_index)
                || macro_owners.get(&consumer) != Some(&macro_index)
            {
                return Err(SccError::MalformedSealedMacro(
                    "an internal link crosses a contraction boundary",
                ));
            }
            if owners.insert(link_index, macro_index).is_some() {
                return Err(SccError::MalformedSealedMacro(
                    "physical link belongs to more than one contraction",
                ));
            }
        }
    }
    Ok(owners)
}

fn quotient_vertex(node: NodeId, owners: &BTreeMap<NodeId, usize>) -> QuotientVertex {
    owners
        .get(&node)
        .map_or(QuotientVertex::Primitive(node), |&owner| {
            QuotientVertex::Macro(owner)
        })
}

fn quotient_vertex_nodes(topology: &TopologyState, vertex: QuotientVertex) -> Vec<NodeId> {
    match vertex {
        QuotientVertex::Primitive(node) => vec![node],
        QuotientVertex::Macro(index) => topology.sealed_macro_instances()[index].nodes().to_vec(),
    }
}

fn region_macro_indices(nodes: &BTreeSet<NodeId>, owners: &BTreeMap<NodeId, usize>) -> Vec<usize> {
    nodes
        .iter()
        .filter_map(|node| owners.get(node).copied())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
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

    let macro_owners = sealed_macro_owners(topology, &node_ids)?;
    let mut graph = DiGraphMap::<QuotientVertex, ()>::new();
    for &node in &node_ids {
        graph.add_node(quotient_vertex(node, &macro_owners));
    }
    let internal_macro_links = sealed_internal_link_owners(topology, &macro_owners)?;
    for (link_index, link) in topology.links().iter().enumerate() {
        let producer = producer_node(link.producer);
        let consumer = consumer_node(link.consumer);
        validate_endpoint_node(producer, &node_ids)?;
        validate_endpoint_node(consumer, &node_ids)?;
        if let (Some(producer), Some(consumer)) = (producer, consumer) {
            let producer = quotient_vertex(producer, &macro_owners);
            let consumer = quotient_vertex(consumer, &macro_owners);
            // Only links fixed inside a sealed subsystem disappear under
            // contraction. A later declared-boundary feedback link maps to a
            // quotient self-edge and must make the macro region cyclic.
            if internal_macro_links.get(&link_index).is_some_and(|owner| {
                producer == QuotientVertex::Macro(*owner)
                    && consumer == QuotientVertex::Macro(*owner)
            }) {
                continue;
            }
            graph.add_edge(producer, consumer, ());
        }
    }

    let mut regions = kosaraju_scc(&graph)
        .into_iter()
        .map(|mut vertices| {
            vertices.sort_unstable();
            let cyclic = vertices.len() > 1
                || vertices
                    .first()
                    .is_some_and(|&vertex| graph.contains_edge(vertex, vertex));
            let mut nodes = vertices
                .into_iter()
                .flat_map(|vertex| quotient_vertex_nodes(topology, vertex))
                .collect::<Vec<_>>();
            nodes.sort_unstable();
            nodes.dedup();
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
    let declared_nodes = topology
        .nodes()
        .iter()
        .map(|node| node.id)
        .collect::<BTreeSet<_>>();
    let macro_owners = sealed_macro_owners(topology, &declared_nodes)?;
    let contracted_macros = region_macro_indices(&node_set, &macro_owners);
    let mut system = SparseSystem::new();
    let mut internal_variables = Vec::new();

    for node in topology
        .nodes()
        .iter()
        .filter(|node| node_set.contains(&node.id) && !macro_owners.contains_key(&node.id))
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
    for &macro_index in &contracted_macros {
        insert_sealed_macro_rows(
            &mut system,
            topology,
            &topology.sealed_macro_instances()[macro_index],
        )?;
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

    let internal_macro_links = sealed_internal_link_owners(topology, &macro_owners)?;
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
        if !internal_macro_links.contains_key(&index) {
            let producer = producer_variable(topology, link.producer)?;
            let consumer = consumer_variable(topology, link.consumer)?;
            system.insert(equality_row(producer, consumer));
        }
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
        contracted_macro_count: contracted_macros.len(),
        algebra,
    })
}

#[derive(Clone, Debug)]
struct ValidatedInternalLink {
    index: usize,
    producer: ProducerPortRef,
    consumer: ConsumerPortRef,
}

#[derive(Clone, Debug)]
struct ValidatedFrozenDeclaration {
    declaration: FrozenSubsystemDeclaration,
    fixed_nodes: Vec<PhysicalNode>,
    internal_links: Vec<ValidatedInternalLink>,
}

#[derive(Clone, Debug)]
struct SymbolicBlock {
    producer_order: Vec<ProducerPortRef>,
    producer_variables: Vec<FlowVarId>,
    internal_coefficients: Vec<Vec<BigInt>>,
    boundary_coefficients: Vec<Vec<BigInt>>,
}

/// Certifies one immutable subsystem as an exact symbolic boundary contract.
///
/// This operation is capacity-independent. It first proves that every selected
/// node port is either connected to another selected node or appears exactly
/// once in the declared boundary. It then builds `A*z + C*x = 0`, where `z`
/// contains every selected producer flow and `x` contains declared input flows.
/// Fraction-free elimination proves whether `z` is a unique function of `x`.
/// When it is, the result stores exact `P`, `K`, `T`, and the canonical residual
/// boundary row space `R*x = 0`.
///
/// A [`FrozenSubsystemAnalysis::Singular`] result rejects only this optional
/// certification. It is not evidence that the surrounding physical search
/// branch is impossible, because later boundary feedback can make the larger
/// graph-level SCC unique.
///
/// # Errors
///
/// Returns an error for a malformed declaration, stale physical endpoints, or
/// an internal exact-elimination invariant failure.
pub fn analyze_frozen_subsystem(
    topology: &TopologyState,
    declaration: &FrozenSubsystemDeclaration,
) -> Result<FrozenSubsystemAnalysis, SccError> {
    let validated = validate_frozen_declaration(topology, declaration)?;
    let block = build_symbolic_block(topology, &validated)?;
    let producer_count = block.producer_order.len();
    let (producer_rank, basis_rows) =
        coefficient_rank_and_basis(&block.internal_coefficients, &block.producer_variables)?;
    if producer_rank != producer_count {
        return Ok(FrozenSubsystemAnalysis::Singular {
            producer_rank,
            producer_count,
        });
    }

    let basis_internal = basis_rows
        .iter()
        .map(|&row| block.internal_coefficients[row].clone())
        .collect::<Vec<_>>();
    let basis_boundary_rhs = basis_rows
        .iter()
        .map(|&row| {
            block.boundary_coefficients[row]
                .iter()
                .map(std::ops::Neg::neg)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let mut produced_map = solve_full_rank_fraction_free(&basis_internal, &basis_boundary_rhs)?;

    let residual_rows = residual_boundary_rows(
        &block.internal_coefficients,
        &block.boundary_coefficients,
        &produced_map,
    );
    let boundary_domain = canonical_rref(residual_rows, declaration.boundary_inputs.len());
    reduce_rows_modulo_domain(&mut produced_map, &boundary_domain);

    Ok(FrozenSubsystemAnalysis::Symbolic(Box::new(
        assemble_frozen_contract(
            topology,
            validated,
            &block.producer_order,
            &produced_map,
            boundary_domain,
        ),
    )))
}

fn assemble_frozen_contract(
    topology: &TopologyState,
    validated: ValidatedFrozenDeclaration,
    producer_order: &[ProducerPortRef],
    produced_map: &[Vec<BigRational>],
    boundary_domain: Vec<Vec<BigRational>>,
) -> FrozenSubsystem {
    let producer_index = producer_order
        .iter()
        .copied()
        .enumerate()
        .map(|(index, producer)| (producer, index))
        .collect::<BTreeMap<_, _>>();
    let snapshot = topology.partial_topology();
    let mut internal_links = validated
        .internal_links
        .iter()
        .map(|link| {
            let producer_row = producer_index[&link.producer];
            FrozenInternalLink {
                canonical_marked_link_key: canonicalize_marked_link(&snapshot, link.index),
                link_index: link.index,
                producer: link.producer,
                consumer: link.consumer,
                coefficients: rational_row(&produced_map[producer_row]),
            }
        })
        .collect::<Vec<_>>();
    internal_links.sort_by(|left, right| {
        left.canonical_marked_link_key
            .cmp(&right.canonical_marked_link_key)
            .then_with(|| left.coefficients.cmp(&right.coefficients))
            .then_with(|| left.producer.cmp(&right.producer))
            .then_with(|| left.consumer.cmp(&right.consumer))
    });

    let transfer_rows = validated
        .declaration
        .boundary_outputs
        .iter()
        .map(|boundary| rational_row(&produced_map[producer_index[&boundary.port]]))
        .collect::<Vec<_>>();
    let internal_rows = internal_links
        .iter()
        .map(|link| link.coefficients.clone())
        .collect::<Vec<_>>();
    let mut produced_flows = internal_links
        .iter()
        .map(|link| ProducedFlowRow {
            producer: link.producer,
            coefficients: link.coefficients.clone(),
        })
        .collect::<Vec<_>>();
    produced_flows.extend(
        validated
            .declaration
            .boundary_outputs
            .iter()
            .zip(&transfer_rows)
            .map(|(boundary, coefficients)| ProducedFlowRow {
                producer: boundary.port,
                coefficients: coefficients.clone(),
            }),
    );
    let all_produced_rows = produced_flows
        .iter()
        .map(|row| row.coefficients.clone())
        .collect::<Vec<_>>();
    let input_count = validated.declaration.boundary_inputs.len();

    FrozenSubsystem {
        declaration: validated.declaration,
        fixed_nodes: validated.fixed_nodes,
        internal_links,
        produced_flows,
        boundary_domain: RationalMatrix::from_rows(
            boundary_domain
                .into_iter()
                .map(|row| rational_row(&row))
                .collect(),
            input_count,
        ),
        all_produced_flow_map: RationalMatrix::from_rows(all_produced_rows, input_count),
        internal_flow_map: RationalMatrix::from_rows(internal_rows, input_count),
        transfer_map: RationalMatrix::from_rows(transfer_rows, input_count),
    }
}

fn validate_frozen_declaration(
    topology: &TopologyState,
    declaration: &FrozenSubsystemDeclaration,
) -> Result<ValidatedFrozenDeclaration, SccError> {
    if declaration.nodes.is_empty() {
        return Err(SccError::EmptyFrozenSubsystem);
    }
    if declaration.boundary_inputs.is_empty() {
        return Err(SccError::MissingBoundaryInputs);
    }
    if declaration.boundary_outputs.is_empty() {
        return Err(SccError::MissingBoundaryOutputs);
    }

    let mut node_set = BTreeSet::new();
    for &node in &declaration.nodes {
        if !node_set.insert(node) {
            return Err(SccError::DuplicateDeclaredNode { node });
        }
    }
    let node_by_id = topology
        .nodes()
        .iter()
        .map(|node| (node.id, node))
        .collect::<BTreeMap<_, _>>();
    let mut fixed_nodes = Vec::with_capacity(node_set.len());
    for &node in &node_set {
        let Some(physical) = node_by_id.get(&node) else {
            return Err(SccError::MissingDeclaredNode { node });
        };
        fixed_nodes.push((*physical).clone());
    }

    let (boundary_inputs, boundary_outputs) =
        validate_boundary_ports(topology, declaration, &node_set)?;
    validate_selected_port_partition(
        topology,
        &fixed_nodes,
        &node_set,
        &boundary_inputs,
        &boundary_outputs,
    )?;

    let internal_links = topology
        .links()
        .iter()
        .enumerate()
        .filter(|(_, link)| {
            producer_node(link.producer).is_some_and(|node| node_set.contains(&node))
                && consumer_node(link.consumer).is_some_and(|node| node_set.contains(&node))
        })
        .map(|(index, link)| ValidatedInternalLink {
            index,
            producer: link.producer,
            consumer: link.consumer,
        })
        .collect::<Vec<_>>();

    Ok(ValidatedFrozenDeclaration {
        declaration: FrozenSubsystemDeclaration {
            nodes: node_set.into_iter().collect(),
            boundary_inputs: declaration.boundary_inputs.clone(),
            boundary_outputs: declaration.boundary_outputs.clone(),
        },
        fixed_nodes,
        internal_links,
    })
}

fn validate_boundary_ports(
    topology: &TopologyState,
    declaration: &FrozenSubsystemDeclaration,
    node_set: &BTreeSet<NodeId>,
) -> Result<(BTreeSet<ConsumerPortRef>, BTreeSet<ProducerPortRef>), SccError> {
    let mut inputs = BTreeSet::new();
    for boundary in &declaration.boundary_inputs {
        if !inputs.insert(boundary.port) {
            return Err(SccError::DuplicateBoundaryInput {
                port: boundary.port,
            });
        }
        if !matches!(boundary.port, ConsumerPortRef::Node { node, .. } if node_set.contains(&node))
            || !topology.consumer_ports().contains_key(&boundary.port)
        {
            return Err(SccError::InvalidBoundaryPort);
        }
    }
    let mut outputs = BTreeSet::new();
    for boundary in &declaration.boundary_outputs {
        if !outputs.insert(boundary.port) {
            return Err(SccError::DuplicateBoundaryOutput {
                port: boundary.port,
            });
        }
        if !matches!(boundary.port, ProducerPortRef::Node { node, .. } if node_set.contains(&node))
            || !topology.producer_ports().contains_key(&boundary.port)
        {
            return Err(SccError::InvalidBoundaryPort);
        }
    }
    Ok((inputs, outputs))
}

fn validate_selected_port_partition(
    topology: &TopologyState,
    fixed_nodes: &[PhysicalNode],
    node_set: &BTreeSet<NodeId>,
    boundary_inputs: &BTreeSet<ConsumerPortRef>,
    boundary_outputs: &BTreeSet<ProducerPortRef>,
) -> Result<(), SccError> {
    let producer_links = topology
        .links()
        .iter()
        .map(|link| (link.producer, link.consumer))
        .collect::<BTreeMap<_, _>>();
    let consumer_links = topology
        .links()
        .iter()
        .map(|link| (link.consumer, link.producer))
        .collect::<BTreeMap<_, _>>();
    for node in fixed_nodes {
        for port in 0..node.node_type.input_port_count() {
            let reference = ConsumerPortRef::Node {
                node: node.id,
                port,
            };
            let is_internal = consumer_links.get(&reference).is_some_and(|producer| {
                producer_node(*producer).is_some_and(|owner| node_set.contains(&owner))
            });
            validate_partition_member(boundary_inputs.contains(&reference), is_internal)?;
        }
        for port in 0..node.node_type.output_port_count() {
            let reference = ProducerPortRef::Node {
                node: node.id,
                port,
            };
            let is_internal = producer_links.get(&reference).is_some_and(|consumer| {
                consumer_node(*consumer).is_some_and(|owner| node_set.contains(&owner))
            });
            validate_partition_member(boundary_outputs.contains(&reference), is_internal)?;
        }
    }
    Ok(())
}

fn validate_partition_member(is_boundary: bool, is_internal: bool) -> Result<(), SccError> {
    match (is_boundary, is_internal) {
        (true, true) => Err(SccError::BoundaryConnectedInternally),
        (false, false) => Err(SccError::UndeclaredPortNotInternal),
        _ => Ok(()),
    }
}

fn build_symbolic_block(
    topology: &TopologyState,
    validated: &ValidatedFrozenDeclaration,
) -> Result<SymbolicBlock, SccError> {
    let producer_order = validated
        .fixed_nodes
        .iter()
        .flat_map(|node| {
            (0..node.node_type.output_port_count()).map(move |port| ProducerPortRef::Node {
                node: node.id,
                port,
            })
        })
        .collect::<Vec<_>>();
    let producer_variables = producer_order
        .iter()
        .map(|&producer| producer_variable(topology, producer))
        .collect::<Result<Vec<_>, _>>()?;
    let producer_index = producer_order
        .iter()
        .copied()
        .enumerate()
        .map(|(index, producer)| (producer, index))
        .collect::<BTreeMap<_, _>>();
    let input_index = validated
        .declaration
        .boundary_inputs
        .iter()
        .enumerate()
        .map(|(index, boundary)| (boundary.port, index))
        .collect::<BTreeMap<_, _>>();
    let internal_upstream = validated
        .internal_links
        .iter()
        .map(|link| (link.consumer, link.producer))
        .collect::<BTreeMap<_, _>>();

    let producer_count = producer_order.len();
    let input_count = input_index.len();
    let mut internal_coefficients = Vec::new();
    let mut boundary_coefficients = Vec::new();
    for node in &validated.fixed_nodes {
        append_node_symbolic_rows(
            node,
            producer_count,
            input_count,
            &producer_index,
            &input_index,
            &internal_upstream,
            &mut internal_coefficients,
            &mut boundary_coefficients,
        )?;
    }

    Ok(SymbolicBlock {
        producer_order,
        producer_variables,
        internal_coefficients,
        boundary_coefficients,
    })
}

#[allow(clippy::too_many_arguments)]
fn append_node_symbolic_rows(
    node: &PhysicalNode,
    producer_count: usize,
    input_count: usize,
    producer_index: &BTreeMap<ProducerPortRef, usize>,
    input_index: &BTreeMap<ConsumerPortRef, usize>,
    internal_upstream: &BTreeMap<ConsumerPortRef, ProducerPortRef>,
    internal_coefficients: &mut Vec<Vec<BigInt>>,
    boundary_coefficients: &mut Vec<Vec<BigInt>>,
) -> Result<(), SccError> {
    let outputs = (0..node.node_type.output_port_count())
        .map(|port| {
            producer_index[&ProducerPortRef::Node {
                node: node.id,
                port,
            }]
        })
        .collect::<Vec<_>>();
    let inputs = (0..node.node_type.input_port_count())
        .map(|port| ConsumerPortRef::Node {
            node: node.id,
            port,
        })
        .collect::<Vec<_>>();
    match node.node_type {
        NodeType::Splitter2 | NodeType::Splitter3 => {
            for &output in &outputs[1..] {
                let mut internal = vec![BigInt::zero(); producer_count];
                let boundary = vec![BigInt::zero(); input_count];
                internal[output] += BigInt::one();
                internal[outputs[0]] -= BigInt::one();
                internal_coefficients.push(internal);
                boundary_coefficients.push(boundary);
            }
            let mut internal = vec![BigInt::zero(); producer_count];
            let mut boundary = vec![BigInt::zero(); input_count];
            add_consumer_expression(
                inputs[0],
                BigInt::one(),
                producer_index,
                input_index,
                internal_upstream,
                &mut internal,
                &mut boundary,
            )?;
            for output in outputs {
                internal[output] -= BigInt::one();
            }
            internal_coefficients.push(internal);
            boundary_coefficients.push(boundary);
        }
        NodeType::Merger2 | NodeType::Merger3 => {
            let mut internal = vec![BigInt::zero(); producer_count];
            let mut boundary = vec![BigInt::zero(); input_count];
            for input in inputs {
                add_consumer_expression(
                    input,
                    BigInt::one(),
                    producer_index,
                    input_index,
                    internal_upstream,
                    &mut internal,
                    &mut boundary,
                )?;
            }
            internal[outputs[0]] -= BigInt::one();
            internal_coefficients.push(internal);
            boundary_coefficients.push(boundary);
        }
    }
    Ok(())
}

fn add_consumer_expression(
    consumer: ConsumerPortRef,
    factor: BigInt,
    producer_index: &BTreeMap<ProducerPortRef, usize>,
    input_index: &BTreeMap<ConsumerPortRef, usize>,
    internal_upstream: &BTreeMap<ConsumerPortRef, ProducerPortRef>,
    internal: &mut [BigInt],
    boundary: &mut [BigInt],
) -> Result<(), SccError> {
    if let Some(&index) = input_index.get(&consumer) {
        boundary[index] += factor;
        return Ok(());
    }
    let Some(upstream) = internal_upstream.get(&consumer) else {
        return Err(SccError::UndeclaredPortNotInternal);
    };
    let Some(&index) = producer_index.get(upstream) else {
        return Err(SccError::UndeclaredPortNotInternal);
    };
    internal[index] += factor;
    Ok(())
}

fn coefficient_rank_and_basis(
    coefficients: &[Vec<BigInt>],
    variables: &[FlowVarId],
) -> Result<(usize, Vec<usize>), SccError> {
    let mut system = SparseSystem::new();
    let mut rank = 0;
    let mut basis = Vec::new();
    for (row_index, row) in coefficients.iter().enumerate() {
        let checkpoint = system.checkpoint();
        system.insert(SparseRow::new(
            variables.iter().copied().zip(row.iter().cloned()),
            BigInt::zero(),
        ));
        let analysis = system.analyze_over(
            variables.iter().copied(),
            &BTreeMap::<FlowVarId, BigRational>::new(),
        )?;
        if analysis.coefficient_rank > rank {
            rank = analysis.coefficient_rank;
            basis.push(row_index);
        } else {
            system.rollback(checkpoint);
        }
    }
    Ok((rank, basis))
}

fn solve_full_rank_fraction_free(
    coefficients: &[Vec<BigInt>],
    right_hand_sides: &[Vec<BigInt>],
) -> Result<Vec<Vec<BigRational>>, SccError> {
    let size = coefficients.len();
    debug_assert_eq!(right_hand_sides.len(), size);
    debug_assert!(coefficients.iter().all(|row| row.len() == size));
    let rhs_columns = right_hand_sides.first().map_or(0, Vec::len);
    debug_assert!(right_hand_sides.iter().all(|row| row.len() == rhs_columns));
    let mut matrix = coefficients
        .iter()
        .zip(right_hand_sides)
        .map(|(left, right)| left.iter().chain(right).cloned().collect::<Vec<_>>())
        .collect::<Vec<_>>();
    let mut previous_pivot = BigInt::one();

    for column in 0..size {
        let pivot_row = (column..size)
            .find(|&row| !matrix[row][column].is_zero())
            .expect("rank proof supplies one pivot per producer column");
        matrix.swap(column, pivot_row);
        let pivot = matrix[column][column].clone();
        let pivot_tail = matrix[column][(column + 1)..].to_vec();
        for (row, target_row) in matrix.iter_mut().enumerate().skip(column + 1) {
            let eliminated = target_row[column].clone();
            for (target, pivot_value) in target_row[(column + 1)..].iter_mut().zip(&pivot_tail) {
                let numerator = &*target * &pivot - &eliminated * pivot_value;
                let (quotient, remainder) = numerator.div_rem(&previous_pivot);
                if !remainder.is_zero() {
                    return Err(SccError::NonExactSymbolicDivision { row, column });
                }
                *target = quotient;
            }
            target_row[column] = BigInt::zero();
        }
        previous_pivot = pivot;
    }

    let mut solution = vec![vec![BigRational::zero(); rhs_columns]; size];
    for row in (0..size).rev() {
        for rhs in 0..rhs_columns {
            let mut value = BigRational::from_integer(matrix[row][size + rhs].clone());
            for column in (row + 1)..size {
                value -=
                    BigRational::from_integer(matrix[row][column].clone()) * &solution[column][rhs];
            }
            solution[row][rhs] = value / BigRational::from_integer(matrix[row][row].clone());
        }
    }
    Ok(solution)
}

fn residual_boundary_rows(
    internal: &[Vec<BigInt>],
    boundary: &[Vec<BigInt>],
    produced_map: &[Vec<BigRational>],
) -> Vec<Vec<BigRational>> {
    internal
        .iter()
        .zip(boundary)
        .map(|(internal_row, boundary_row)| {
            (0..boundary_row.len())
                .map(|column| {
                    internal_row.iter().zip(produced_map).fold(
                        BigRational::from_integer(boundary_row[column].clone()),
                        |sum, (coefficient, produced)| {
                            sum + BigRational::from_integer(coefficient.clone()) * &produced[column]
                        },
                    )
                })
                .collect::<Vec<_>>()
        })
        .filter(|row| row.iter().any(|value| !value.is_zero()))
        .collect()
}

fn canonical_rref(mut rows: Vec<Vec<BigRational>>, columns: usize) -> Vec<Vec<BigRational>> {
    rows.retain(|row| row.iter().any(|value| !value.is_zero()));
    let mut pivot_row = 0;
    for column in 0..columns {
        let Some(found) = (pivot_row..rows.len()).find(|&row| !rows[row][column].is_zero()) else {
            continue;
        };
        rows.swap(pivot_row, found);
        let pivot = rows[pivot_row][column].clone();
        for value in &mut rows[pivot_row] {
            *value /= &pivot;
        }
        let normalized = rows[pivot_row].clone();
        for (row_index, row) in rows.iter_mut().enumerate() {
            if row_index == pivot_row || row[column].is_zero() {
                continue;
            }
            let factor = row[column].clone();
            for (value, pivot_value) in row.iter_mut().zip(&normalized) {
                *value -= &factor * pivot_value;
            }
        }
        pivot_row += 1;
        if pivot_row == rows.len() {
            break;
        }
    }
    rows.truncate(pivot_row);
    rows
}

fn reduce_rows_modulo_domain(rows: &mut [Vec<BigRational>], domain: &[Vec<BigRational>]) {
    for domain_row in domain {
        let Some(pivot) = domain_row.iter().position(|value| !value.is_zero()) else {
            continue;
        };
        for row in rows.iter_mut() {
            let factor = row[pivot].clone();
            if factor.is_zero() {
                continue;
            }
            for (value, domain_value) in row.iter_mut().zip(domain_row) {
                *value -= &factor * domain_value;
            }
        }
    }
}

fn rational_row(row: &[BigRational]) -> Vec<Rational> {
    row.iter().cloned().map(Rational::from).collect()
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

fn insert_sealed_macro_rows(
    system: &mut SparseSystem,
    topology: &TopologyState,
    instance: &SealedMacroInstance,
) -> Result<(), SccError> {
    let component = instance.component();
    if !component.is_projection_certified() {
        return Err(SccError::MalformedSealedMacro(
            "component lacks an immutable projection certificate",
        ));
    }
    let input_variables = instance
        .boundary_inputs()
        .iter()
        .copied()
        .map(|port| consumer_variable(topology, port))
        .collect::<Result<Vec<_>, _>>()?;
    let output_variables = instance
        .boundary_outputs()
        .iter()
        .copied()
        .map(|port| producer_variable(topology, port))
        .collect::<Result<Vec<_>, _>>()?;
    if component.domain().input_count() != input_variables.len()
        || component.transfer().column_count() != input_variables.len()
        || component.transfer().row_count() != output_variables.len()
        || component.internal_flow_map().column_count() != input_variables.len()
        || component.internal_flow_map().row_count() != instance.internal_link_indices().len()
    {
        return Err(SccError::MalformedSealedMacro(
            "contract dimensions disagree with the physical batch",
        ));
    }

    for coefficients in component.domain().equalities().rows() {
        system.insert(rational_zero_row(
            input_variables
                .iter()
                .copied()
                .zip(coefficients.iter().cloned()),
        ));
    }
    for (output, coefficients) in output_variables
        .iter()
        .copied()
        .zip(component.transfer().rows())
    {
        system.insert(component_expression_row(
            output,
            &input_variables,
            coefficients,
        ));
    }
    for (&link_index, coefficients) in instance
        .internal_link_indices()
        .iter()
        .zip(component.internal_flow_map().rows())
    {
        let link = topology
            .links()
            .get(link_index)
            .ok_or(SccError::LinkIndexOutOfBounds {
                index: link_index,
                link_count: topology.links().len(),
            })?;
        let producer = producer_variable(topology, link.producer)?;
        let consumer = consumer_variable(topology, link.consumer)?;
        system.insert(component_expression_row(
            producer,
            &input_variables,
            coefficients,
        ));
        system.insert(component_expression_row(
            consumer,
            &input_variables,
            coefficients,
        ));
    }
    Ok(())
}

fn component_expression_row(
    target: FlowVarId,
    inputs: &[FlowVarId],
    coefficients: &[Rational],
) -> SparseRow {
    rational_zero_row(
        std::iter::once((target, Rational::one())).chain(
            inputs
                .iter()
                .copied()
                .zip(coefficients.iter().map(|coefficient| -coefficient)),
        ),
    )
}

/// Clears denominators by one positive common multiple. The resulting integer
/// row has exactly the same rational solution set as the certified contract row.
fn rational_zero_row(terms: impl IntoIterator<Item = (FlowVarId, Rational)>) -> SparseRow {
    let terms = terms.into_iter().collect::<Vec<_>>();
    let denominator_lcm = terms
        .iter()
        .map(|(_, coefficient)| coefficient.denominator())
        .fold(BigInt::one(), |lcm, denominator| lcm.lcm(denominator));
    SparseRow::new(
        terms.into_iter().map(|(variable, coefficient)| {
            let scale = &denominator_lcm / coefficient.denominator();
            (variable, coefficient.numerator() * scale)
        }),
        BigInt::zero(),
    )
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
        problem::{Preparation, prepare_problem},
        topology::{ConsumerChoice, ProducerChoice, TopologyDecision},
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

    fn splitter_declaration(node: u32) -> FrozenSubsystemDeclaration {
        FrozenSubsystemDeclaration {
            nodes: vec![NodeId(node)],
            boundary_inputs: vec![DeclaredBoundaryInput {
                port: consumer(node, 0),
            }],
            boundary_outputs: vec![
                DeclaredBoundaryOutput {
                    port: producer(node, 0),
                },
                DeclaredBoundaryOutput {
                    port: producer(node, 1),
                },
            ],
        }
    }

    fn symbolic(analysis: FrozenSubsystemAnalysis) -> FrozenSubsystem {
        match analysis {
            FrozenSubsystemAnalysis::Symbolic(contract) => *contract,
            FrozenSubsystemAnalysis::Singular { .. } => panic!("expected a symbolic contract"),
        }
    }

    #[test]
    fn acyclic_frozen_subsystem_has_exact_transfer_and_capacity_application() {
        let topology = topology(
            problem(&["2"], &["1", "1"]),
            vec![node(0, NodeType::Splitter2)],
            vec![],
        );
        let contract =
            symbolic(analyze_frozen_subsystem(&topology, &splitter_declaration(0)).unwrap());

        assert_eq!(contract.boundary_domain.row_count(), 0);
        assert_eq!(contract.internal_flow_map.row_count(), 0);
        assert_eq!(contract.transfer_map.row(0).unwrap(), &[rational("1/2")]);
        assert_eq!(contract.transfer_map.row(1).unwrap(), &[rational("1/2")]);
        let application = contract.evaluate(&[rational("2")], &rational("2")).unwrap();
        assert_eq!(application.outputs, vec![rational("1"), rational("1")]);
        assert_eq!(application.peak_internal_flow, None);
        assert!(matches!(
            contract.evaluate(&[rational("2")], &rational("3/2")),
            Err(FrozenSubsystemRejection::InputCapacityExceeded { .. })
        ));
    }

    fn feedback_component(
        reverse_links: bool,
        swap_labels: bool,
    ) -> (TopologyState, FrozenSubsystemDeclaration) {
        let (splitter, merger) = if swap_labels { (1, 0) } else { (0, 1) };
        let mut links = vec![
            (producer(merger, 0), consumer(splitter, 0)),
            (producer(splitter, 0), consumer(merger, 0)),
        ];
        if reverse_links {
            links.reverse();
        }
        let topology = topology(
            problem(&["1"], &["1"]),
            vec![
                node(
                    0,
                    if swap_labels {
                        NodeType::Merger2
                    } else {
                        NodeType::Splitter2
                    },
                ),
                node(
                    1,
                    if swap_labels {
                        NodeType::Splitter2
                    } else {
                        NodeType::Merger2
                    },
                ),
            ],
            links,
        );
        let declaration = FrozenSubsystemDeclaration {
            nodes: vec![NodeId(merger), NodeId(splitter)],
            boundary_inputs: vec![DeclaredBoundaryInput {
                port: consumer(merger, 1),
            }],
            boundary_outputs: vec![DeclaredBoundaryOutput {
                port: producer(splitter, 1),
            }],
        };
        (topology, declaration)
    }

    #[test]
    fn cyclic_frozen_subsystem_extracts_unique_k_and_t_exactly() {
        let (topology, declaration) = feedback_component(false, false);
        let contract = symbolic(analyze_frozen_subsystem(&topology, &declaration).unwrap());

        assert_eq!(contract.boundary_domain.row_count(), 0);
        assert_eq!(contract.transfer_map.row(0).unwrap(), &[rational("1")]);
        let mut internal_rows = (0..contract.internal_flow_map.row_count())
            .map(|row| contract.internal_flow_map.row(row).unwrap().to_vec())
            .collect::<Vec<_>>();
        internal_rows.sort();
        assert_eq!(
            internal_rows,
            vec![vec![rational("1")], vec![rational("2")]]
        );

        let application = contract.evaluate(&[rational("1")], &rational("2")).unwrap();
        assert_eq!(application.outputs, vec![rational("1")]);
        assert_eq!(application.peak_internal_flow, Some(rational("2")));
        assert!(matches!(
            contract.evaluate(&[rational("1")], &rational("1")),
            Err(FrozenSubsystemRejection::ProducedFlowCapacityExceeded { .. })
        ));
        assert!(matches!(
            contract.evaluate(&[Rational::zero()], &rational("2")),
            Err(FrozenSubsystemRejection::NonPositiveInput { .. })
        ));
    }

    #[test]
    fn frozen_matrices_survive_node_labels_and_link_storage_order() {
        let (left_topology, left_declaration) = feedback_component(false, false);
        let (right_topology, right_declaration) = feedback_component(true, true);
        let left = symbolic(analyze_frozen_subsystem(&left_topology, &left_declaration).unwrap());
        let right =
            symbolic(analyze_frozen_subsystem(&right_topology, &right_declaration).unwrap());

        assert_eq!(left.boundary_domain, right.boundary_domain);
        assert_eq!(left.transfer_map, right.transfer_map);
        assert_eq!(left.internal_flow_map, right.internal_flow_map);
    }

    #[test]
    fn malformed_boundary_partition_is_rejected_before_algebra() {
        let topology = topology(
            problem(&["2"], &["1", "1"]),
            vec![node(0, NodeType::Splitter2)],
            vec![],
        );
        let mut missing_output = splitter_declaration(0);
        missing_output.boundary_outputs.pop();
        assert_eq!(
            analyze_frozen_subsystem(&topology, &missing_output),
            Err(SccError::UndeclaredPortNotInternal)
        );

        let (cycle, mut declaration) = feedback_component(false, false);
        declaration.boundary_outputs[0].port = producer(0, 0);
        assert_eq!(
            analyze_frozen_subsystem(&cycle, &declaration),
            Err(SccError::BoundaryConnectedInternally)
        );
    }

    #[test]
    fn singular_frozen_candidate_is_nonfatal_to_the_physical_branch() {
        let topology = topology(
            problem(&["2"], &["1", "1"]),
            vec![
                node(0, NodeType::Splitter2),
                node(1, NodeType::Merger2),
                node(2, NodeType::Splitter2),
            ],
            vec![
                (producer(0, 0), consumer(1, 0)),
                (producer(0, 1), consumer(1, 1)),
                (producer(1, 0), consumer(0, 0)),
            ],
        );
        let declaration = splitter_declaration(2);
        let declaration = FrozenSubsystemDeclaration {
            nodes: vec![NodeId(0), NodeId(1), NodeId(2)],
            ..declaration
        };
        assert!(matches!(
            analyze_frozen_subsystem(&topology, &declaration).unwrap(),
            FrozenSubsystemAnalysis::Singular {
                producer_rank: _,
                producer_count: _
            }
        ));
    }

    #[test]
    fn conditional_block_reduction_is_row_order_and_scale_invariant() {
        fn reduce(
            internal: &[Vec<BigInt>],
            boundary: &[Vec<BigInt>],
        ) -> (Vec<Vec<BigRational>>, Vec<Vec<BigRational>>) {
            let variables = [FlowVarId(0)];
            let (_, basis) = coefficient_rank_and_basis(internal, &variables).unwrap();
            let left = basis
                .iter()
                .map(|&row| internal[row].clone())
                .collect::<Vec<_>>();
            let right = basis
                .iter()
                .map(|&row| boundary[row].iter().map(std::ops::Neg::neg).collect())
                .collect::<Vec<_>>();
            let mut produced = solve_full_rank_fraction_free(&left, &right).unwrap();
            let domain = canonical_rref(residual_boundary_rows(internal, boundary, &produced), 2);
            reduce_rows_modulo_domain(&mut produced, &domain);
            (produced, domain)
        }

        let left = reduce(
            &[vec![1.into()], vec![1.into()]],
            &[vec![(-1).into(), 0.into()], vec![0.into(), (-1).into()]],
        );
        let right = reduce(
            &[vec![3.into()], vec![(-2).into()]],
            &[vec![0.into(), (-3).into()], vec![2.into(), 0.into()]],
        );
        assert_eq!(left, right);
        assert_eq!(
            left.1,
            vec![vec![
                BigRational::one(),
                BigRational::from_integer((-1).into())
            ]]
        );
        assert_eq!(left.0, vec![vec![BigRational::zero(), BigRational::one()]]);
    }

    #[test]
    fn fraction_free_block_solver_matches_every_small_invertible_two_by_two_matrix() {
        let rhs = vec![
            vec![BigInt::from(-2), BigInt::from(3)],
            vec![BigInt::one(), BigInt::from(-1)],
        ];
        for a in -2..=2 {
            for b in -2..=2 {
                for c in -2..=2 {
                    for d in -2..=2 {
                        if a * d == b * c {
                            continue;
                        }
                        let coefficients = vec![
                            vec![BigInt::from(a), BigInt::from(b)],
                            vec![BigInt::from(c), BigInt::from(d)],
                        ];
                        let solved = solve_full_rank_fraction_free(&coefficients, &rhs).unwrap();
                        for column in 0..2 {
                            let first = BigRational::from_integer(BigInt::from(a))
                                * &solved[0][column]
                                + BigRational::from_integer(BigInt::from(b)) * &solved[1][column];
                            let second = BigRational::from_integer(BigInt::from(c))
                                * &solved[0][column]
                                + BigRational::from_integer(BigInt::from(d)) * &solved[1][column];
                            assert_eq!(first, BigRational::from_integer(rhs[0][column].clone()));
                            assert_eq!(second, BigRational::from_integer(rhs[1][column].clone()));
                        }
                    }
                }
            }
        }
    }
}
