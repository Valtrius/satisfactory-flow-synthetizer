//! Production colored-incidence canonicalization and canonical augmentation support.
//!
//! Internal search keys use `canonaut` on a vertex-colored subdivision of the
//! edge-colored incidence graph. Full witness keys retain the exhaustive
//! individualization path so the public reference protocol remains byte-stable.
//! Neither path calls or reproduces the reference solver's topology search.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

use canonaut::structs::{CanonautManager, DenseGraph};
use num::{BigInt, Integer, One, Signed, Zero};
use solver_api::{
    CanonicalGraphKey, ConsumerPortRef, DiscardTerminalIndex, InputTerminalIndex, NodeId,
    NodeProfile, NodeType, OutputTerminalIndex, PhysicalGraph, PhysicalLink, PhysicalNode, Problem,
    ProducerPortRef, Rational,
};

use crate::{
    hotspot_profile::{self, CanonicalPhase, CanonicalTimer, GraphCanonPurpose},
    topology::OpenPortRef,
};

/// One structural connection in a partial topology.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PartialLink {
    /// Occupied producer port.
    pub producer: ProducerPortRef,
    /// Occupied consumer port.
    pub consumer: ConsumerPortRef,
    /// Exact flow when propagation has determined it, or `None` while unresolved.
    pub flow: Option<Rational>,
}

/// Immutable canonicalization snapshot of a mutable production search state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartialTopology {
    /// Exact external terminal rates and link capacity for this solve.
    pub problem: Problem,
    /// Materialized physical nodes. Unmaterialized nodes remain in `remaining_profile`.
    pub nodes: Vec<PhysicalNode>,
    /// Connected physical ports with optional exact propagated flow.
    pub links: Vec<PartialLink>,
    /// Number of anonymous surplus-discard consumer terminals.
    pub discard_count: u32,
    /// Node-type inventory not yet materialized.
    pub remaining_profile: NodeProfile,
}

/// Opaque exact identity of a partial production state.
///
/// Each fully individualized topology labeling fixes a canonical variable order
/// because every flow belongs to one explicit canonical producer or consumer
/// port. The candidate bytes append an exact normalized semantic payload:
///
/// - external assignments follow from the encoded terminal associations and rates;
/// - operator equations follow from the encoded node type and its canonical ports;
/// - link equalities follow from the encoded occupied endpoint pairs.
/// - strict positivity and capacity rows follow from the encoded physical ports
///   and exact problem capacity.
///
/// Equality rows are reduced to exact RREF and inequalities are reduced modulo
/// that basis, positively normalized, sorted, and deduplicated. Candidate
/// selection minimizes topology plus algebra together, so topology automorphisms
/// cannot leak arbitrary labels into the semantic order. A propagated link value
/// contributes only an equality row; raw `None`/`Some` coloring and algebra row
/// insertion order never enter the key.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StateKey(Box<[u8]>);

impl StateKey {
    /// Borrows the stable canonical byte representation.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Consumes the key and returns the stable canonical bytes.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.0.into_vec()
    }
}

/// Canonical identity of every semantic input to one open-SCC summary.
///
/// Besides the complete partial physical state, the key colors exactly the
/// nodes in the current region and every physical port with its supplied exact
/// known value. Raw node, port, flow-variable, and link identifiers never enter
/// the encoding. Equal keys therefore describe the same open subsystem under a
/// physical relabeling and may reuse label-free exact algebra facts.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SccSummaryKey(Box<[u8]>);

impl SccSummaryKey {
    /// Borrows the stable canonical byte representation.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// One physical flow variable expressed in the canonical labeling selected for
/// an open-SCC summary input.
///
/// Keeping the direction in the identity is necessary because producer and
/// consumer ports are separate physical variables until a link equation proves
/// them equal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CanonicalFlowEndpoint {
    /// Canonically relabeled producer port.
    Producer(ProducerPortRef),
    /// Canonically relabeled consumer port.
    Consumer(ConsumerPortRef),
}

/// Canonical open-SCC cache key together with the isomorphism used to label
/// every physical port in that key.
///
/// The relabeling lets callers store exact deductions in canonical endpoint
/// coordinates and map them back after a cache hit on an isomorphic topology.
/// It is deliberately produced by the same winning individualization as the
/// key; a separately canonicalized marked port would lose pair correlations in
/// ratio deductions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonicalSccSummaryInput {
    /// Complete canonical identity of the summarized equations and known facts.
    pub key: SccSummaryKey,
    /// Raw producer endpoint to canonical endpoint selected for `key`.
    pub producer_relabeling: BTreeMap<ProducerPortRef, ProducerPortRef>,
    /// Raw consumer endpoint to canonical endpoint selected for `key`.
    pub consumer_relabeling: BTreeMap<ConsumerPortRef, ConsumerPortRef>,
}

/// Canonical full physical witness and its authoritative cross-solver key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonicalWitness {
    /// Complete canonical graph byte protocol.
    pub key: CanonicalGraphKey,
    /// Canonically labeled, deterministically link-sorted physical graph.
    pub graph: PhysicalGraph,
}

/// Opaque canonical identity of a partial topology with one physical link individualized.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MarkedLinkCanonicalKey(Vec<u8>);

impl MarkedLinkCanonicalKey {
    /// Borrows the stable canonical marked-graph bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Opaque canonical identity of one individualized open producer or consumer port.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CanonicalOpenPortKey(Vec<u8>);

impl CanonicalOpenPortKey {
    /// Borrows the stable canonical individualized-port bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Computes the exact canonical state key used by per-profile memoization.
#[must_use]
pub fn canonicalize_state(topology: &PartialTopology) -> StateKey {
    let selected = select_canonical(topology, None, EncodingKind::State);
    StateKey(selected.bytes.into_boxed_slice())
}

/// Computes the production state key, or stops during labeling when cancelled.
pub(crate) fn canonicalize_state_cancellable(
    topology: &PartialTopology,
    cancel: &AtomicBool,
) -> Option<StateKey> {
    select_canonical_cancellable(topology, None, EncodingKind::State, cancel)
        .map(|selected| StateKey(selected.bytes.into_boxed_slice()))
}

/// Canonicalizes the complete input of [`crate::scc::summarize_open_scc`].
///
/// `known_producers` and `known_consumers` must be the endpoint projection of
/// the supplied flow-variable map. Terminal-known values need not be repeated:
/// they are already part of `topology.problem` and the primitive semantic row
/// system appended to this key.
///
/// # Panics
///
/// Panics if a region node, known-value port, physical link endpoint, or
/// topology identifier is not declared by `topology`.
#[must_use]
pub fn canonicalize_scc_summary_input(
    topology: &PartialTopology,
    region_nodes: &BTreeSet<NodeId>,
    known_producers: &BTreeMap<ProducerPortRef, Rational>,
    known_consumers: &BTreeMap<ConsumerPortRef, Rational>,
) -> SccSummaryKey {
    canonicalize_scc_summary_input_with_relabeling(
        topology,
        region_nodes,
        known_producers,
        known_consumers,
    )
    .key
}

/// Canonicalizes an open-SCC summary input and returns the exact winning port
/// relabeling alongside its key.
///
/// This has the same input contract and panic conditions as
/// [`canonicalize_scc_summary_input`]. The returned maps are bijections over all
/// declared producer and consumer ports. They may therefore translate cached
/// algebra deductions without retaining topology-local identifiers.
///
/// # Panics
///
/// Panics if a region node, known-value port, physical link endpoint, or
/// topology identifier is not declared by `topology`.
#[must_use]
pub fn canonicalize_scc_summary_input_with_relabeling(
    topology: &PartialTopology,
    region_nodes: &BTreeSet<NodeId>,
    known_producers: &BTreeMap<ProducerPortRef, Rational>,
    known_consumers: &BTreeMap<ConsumerPortRef, Rational>,
) -> CanonicalSccSummaryInput {
    canonicalize_scc_summary_input_with_relabeling_inner(
        topology,
        region_nodes,
        known_producers,
        known_consumers,
        None,
    )
    .expect("uncancelled canonicalization must complete")
}

pub(crate) fn canonicalize_scc_summary_input_with_relabeling_cancellable(
    topology: &PartialTopology,
    region_nodes: &BTreeSet<NodeId>,
    known_producers: &BTreeMap<ProducerPortRef, Rational>,
    known_consumers: &BTreeMap<ConsumerPortRef, Rational>,
    cancel: &AtomicBool,
) -> Option<CanonicalSccSummaryInput> {
    canonicalize_scc_summary_input_with_relabeling_inner(
        topology,
        region_nodes,
        known_producers,
        known_consumers,
        Some(cancel),
    )
}

fn canonicalize_scc_summary_input_with_relabeling_inner(
    topology: &PartialTopology,
    region_nodes: &BTreeSet<NodeId>,
    known_producers: &BTreeMap<ProducerPortRef, Rational>,
    known_consumers: &BTreeMap<ConsumerPortRef, Rational>,
    cancel: Option<&AtomicBool>,
) -> Option<CanonicalSccSummaryInput> {
    assert!(
        region_nodes
            .iter()
            .all(|node| topology.nodes.iter().any(|candidate| candidate.id == *node)),
        "SCC key region nodes must belong to the partial topology"
    );
    let annotations = SccAnnotations {
        region_nodes,
        known_producers,
        known_consumers,
    };
    let incidence = IncidenceGraph::build_annotated(topology, None, false, Some(&annotations));
    let selected = select_canonical_with_incidence(
        topology,
        &incidence,
        EncodingKind::SccSummary,
        None,
        cancel,
    )?;
    let (producer_relabeling, consumer_relabeling) =
        incidence.port_relabeling(topology, &selected.ranks);
    Some(CanonicalSccSummaryInput {
        key: SccSummaryKey(selected.bytes.into_boxed_slice()),
        producer_relabeling,
        consumer_relabeling,
    })
}

/// Canonicalizes a complete physical witness using the shared authoritative byte protocol.
///
/// All link flows participate exactly. No floating-point conversion occurs.
///
/// # Panics
///
/// Panics when the graph has duplicate node identifiers, repeated physical ports, or a link that
/// refers to an undeclared terminal, node, or effective port. The independent validator checks the
/// same structural requirements before an optimal result can be returned.
#[must_use]
pub fn canonicalize_witness(problem: &Problem, graph: &PhysicalGraph) -> CanonicalWitness {
    canonicalize_witness_inner(problem, graph, None)
        .expect("uncancelled canonicalization must complete")
}

/// Canonicalizes the effective physical layout while deliberately ignoring link-flow choices.
///
/// This is the shared result-deduplication identity. It preserves typed operators, terminal
/// associations, physical port incidence, and discard sinks, but treats two valid witnesses that
/// differ only by a feasible circulation inside a cycle as the same displayed layout.
#[must_use]
pub fn canonicalize_effective_layout(
    problem: &Problem,
    graph: &PhysicalGraph,
) -> CanonicalGraphKey {
    let topology = PartialTopology {
        problem: problem.clone(),
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
        discard_count: discard_count_from_links(&graph.links),
        remaining_profile: NodeProfile::default(),
    };
    CanonicalGraphKey::from_bytes(select_canonical(&topology, None, EncodingKind::Layout).bytes)
}

/// Canonicalizes a complete witness, returning no key when cancellation interrupts the exact search.
#[must_use]
pub fn canonicalize_witness_cancellable(
    problem: &Problem,
    graph: &PhysicalGraph,
    cancel: &AtomicBool,
) -> Option<CanonicalWitness> {
    canonicalize_witness_inner(problem, graph, Some(cancel))
}

fn canonicalize_witness_inner(
    problem: &Problem,
    graph: &PhysicalGraph,
    cancel: Option<&AtomicBool>,
) -> Option<CanonicalWitness> {
    let _activity = crate::diagnostics::ActivitySpan::start(
        "witness_canonicalize",
        u32::try_from(graph.nodes.len()).unwrap_or(u32::MAX),
        None,
        None,
        None,
        1,
    );
    let discard_count = discard_count_from_links(&graph.links);
    let topology = PartialTopology {
        problem: problem.clone(),
        nodes: graph.nodes.clone(),
        links: graph
            .links
            .iter()
            .map(|link| PartialLink {
                producer: link.producer,
                consumer: link.consumer,
                flow: Some(link.flow.clone()),
            })
            .collect(),
        discard_count,
        remaining_profile: NodeProfile::default(),
    };
    let selected =
        select_canonical_cancellable_inner(&topology, None, EncodingKind::Witness, cancel)?;
    Some(CanonicalWitness {
        key: CanonicalGraphKey::from_bytes(selected.bytes),
        graph: selected
            .topology
            .into_physical_graph()
            .expect("a full witness has an exact flow on every link"),
    })
}

/// Canonicalizes a partial topology after individualizing one physical link.
///
/// Equal results mean that an automorphism of the complete colored partial state can map one
/// marked link to the other.
///
/// # Panics
///
/// Panics when `marked_link` is outside `topology.links`.
#[must_use]
pub fn canonicalize_marked_link(
    topology: &PartialTopology,
    marked_link: usize,
) -> MarkedLinkCanonicalKey {
    assert!(
        marked_link < topology.links.len(),
        "marked link index must be in range"
    );
    MarkedLinkCanonicalKey(
        select_canonical(topology, Some(marked_link), EncodingKind::Marked).bytes,
    )
}

/// Computes a marked-link key, or stops during labeling when cancelled.
pub(crate) fn canonicalize_marked_link_cancellable(
    topology: &PartialTopology,
    marked_link: usize,
    cancel: &AtomicBool,
) -> Option<MarkedLinkCanonicalKey> {
    assert!(
        marked_link < topology.links.len(),
        "marked link index must be in range"
    );
    select_canonical_cancellable(topology, Some(marked_link), EncodingKind::Marked, cancel)
        .map(|selected| MarkedLinkCanonicalKey(selected.bytes))
}

/// Canonicalizes a partial topology after individualizing one open port.
///
/// Equal keys identify the same exact open-port automorphism orbit. Ordering
/// these keys therefore gives an isomorphism-invariant MRV tie-break.
///
/// # Panics
///
/// Panics if `port` is undeclared or already occupied in `topology`.
#[must_use]
pub fn canonicalize_open_port(
    topology: &PartialTopology,
    port: OpenPortRef,
) -> CanonicalOpenPortKey {
    canonicalize_open_port_inner(topology, port, None)
        .expect("uncancelled open-port canonicalization must complete")
}

pub(crate) fn canonicalize_open_port_cancellable(
    topology: &PartialTopology,
    port: OpenPortRef,
    cancel: &AtomicBool,
) -> Option<CanonicalOpenPortKey> {
    canonicalize_open_port_inner(topology, port, Some(cancel))
}

fn canonicalize_open_port_inner(
    topology: &PartialTopology,
    port: OpenPortRef,
    cancel: Option<&AtomicBool>,
) -> Option<CanonicalOpenPortKey> {
    assert!(
        open_port_is_declared(topology, port),
        "marked port must be declared"
    );
    assert!(
        open_port_is_unused(topology, port),
        "marked port must be open"
    );
    select_canonical_open_port(topology, port, cancel)
        .map(|selected| CanonicalOpenPortKey(selected.bytes))
}

fn open_port_is_declared(topology: &PartialTopology, port: OpenPortRef) -> bool {
    match port {
        OpenPortRef::Producer(ProducerPortRef::Input(index)) => {
            usize::try_from(index.0).is_ok_and(|index| index < topology.problem.inputs.len())
        }
        OpenPortRef::Consumer(ConsumerPortRef::Output(index)) => {
            usize::try_from(index.0).is_ok_and(|index| index < topology.problem.outputs.len())
        }
        OpenPortRef::Consumer(ConsumerPortRef::Discard(index)) => index.0 < topology.discard_count,
        OpenPortRef::Producer(ProducerPortRef::Node { node, port }) => topology
            .nodes
            .iter()
            .find(|candidate| candidate.id == node)
            .is_some_and(|candidate| port < candidate.node_type.output_port_count()),
        OpenPortRef::Consumer(ConsumerPortRef::Node { node, port }) => topology
            .nodes
            .iter()
            .find(|candidate| candidate.id == node)
            .is_some_and(|candidate| port < candidate.node_type.input_port_count()),
    }
}

fn open_port_is_unused(topology: &PartialTopology, port: OpenPortRef) -> bool {
    match port {
        OpenPortRef::Producer(reference) => {
            topology.links.iter().all(|link| link.producer != reference)
        }
        OpenPortRef::Consumer(reference) => {
            topology.links.iter().all(|link| link.consumer != reference)
        }
    }
}

fn discard_count_from_links(links: &[PhysicalLink]) -> u32 {
    let indices = links
        .iter()
        .filter_map(|link| match link.consumer {
            ConsumerPortRef::Discard(index) => Some(index.0),
            ConsumerPortRef::Output(_) | ConsumerPortRef::Node { .. } => None,
        })
        .collect::<BTreeSet<_>>();
    let count = u32::try_from(indices.len()).expect("discard count must fit public index type");
    assert!(
        indices.iter().copied().eq(0..count),
        "discard terminal identifiers must be contiguous"
    );
    count
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EncodingKind {
    State,
    Layout,
    SccSummary,
    Marked,
    MarkedPort,
    Witness,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MarkedPort {
    Producer(ProducerPortRef),
    Consumer(ConsumerPortRef),
}

#[derive(Clone, Debug)]
struct SelectedCanonical {
    bytes: Vec<u8>,
    topology: CanonicalPartialTopology,
    ranks: Vec<Option<u32>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CanonicalPartialTopology {
    nodes: Vec<PhysicalNode>,
    links: Vec<CanonicalPartialLink>,
    marked_producer: Option<ProducerPortRef>,
    marked_consumer: Option<ConsumerPortRef>,
    scc_nodes: Vec<NodeId>,
    known_producers: Vec<(ProducerPortRef, Rational)>,
    known_consumers: Vec<(ConsumerPortRef, Rational)>,
}

impl CanonicalPartialTopology {
    fn into_physical_graph(self) -> Option<PhysicalGraph> {
        let links = self
            .links
            .into_iter()
            .map(|link| {
                Some(PhysicalLink {
                    producer: link.producer,
                    consumer: link.consumer,
                    flow: link.flow?,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(PhysicalGraph {
            nodes: self.nodes,
            links,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct CanonicalPartialLink {
    producer: ProducerPortRef,
    consumer: ConsumerPortRef,
    flow: Option<Rational>,
    marked: bool,
}

fn select_canonical(
    topology: &PartialTopology,
    marked_link: Option<usize>,
    encoding: EncodingKind,
) -> SelectedCanonical {
    select_canonical_cancellable_inner(topology, marked_link, encoding, None)
        .expect("uncancelled canonicalization must complete")
}

fn select_canonical_cancellable(
    topology: &PartialTopology,
    marked_link: Option<usize>,
    encoding: EncodingKind,
    cancel: &AtomicBool,
) -> Option<SelectedCanonical> {
    select_canonical_cancellable_inner(topology, marked_link, encoding, Some(cancel))
}

fn select_canonical_cancellable_inner(
    topology: &PartialTopology,
    marked_link: Option<usize>,
    encoding: EncodingKind,
    cancel: Option<&AtomicBool>,
) -> Option<SelectedCanonical> {
    let started = Instant::now();
    let build_timer = CanonicalTimer::start(CanonicalPhase::IncidenceBuild);
    let incidence = IncidenceGraph::build(
        topology,
        marked_link,
        matches!(encoding, EncodingKind::Witness),
    );
    drop(build_timer);
    let selected = select_canonical_with_incidence(topology, &incidence, encoding, None, cancel);
    let purpose = match encoding {
        EncodingKind::State => GraphCanonPurpose::State,
        EncodingKind::Marked => GraphCanonPurpose::MarkedLink,
        EncodingKind::Witness
        | EncodingKind::Layout
        | EncodingKind::SccSummary
        | EncodingKind::MarkedPort => GraphCanonPurpose::Other,
    };
    hotspot_profile::record_graph_canon(started.elapsed(), purpose);
    selected
}

struct SccAnnotations<'a> {
    region_nodes: &'a BTreeSet<NodeId>,
    known_producers: &'a BTreeMap<ProducerPortRef, Rational>,
    known_consumers: &'a BTreeMap<ConsumerPortRef, Rational>,
}

fn select_canonical_open_port(
    topology: &PartialTopology,
    port: OpenPortRef,
    cancel: Option<&AtomicBool>,
) -> Option<SelectedCanonical> {
    let started = Instant::now();
    let build_timer = CanonicalTimer::start(CanonicalPhase::IncidenceBuild);
    let mut incidence = IncidenceGraph::build(topology, None, false);
    drop(build_timer);
    let marked = match port {
        OpenPortRef::Producer(reference) => MarkedPort::Producer(reference),
        OpenPortRef::Consumer(reference) => MarkedPort::Consumer(reference),
    };
    let marked_vertex = incidence.mark_port(marked);
    let selected = select_canonical_with_incidence(
        topology,
        &incidence,
        EncodingKind::MarkedPort,
        Some(marked_vertex),
        cancel,
    );
    hotspot_profile::record_graph_canon(started.elapsed(), GraphCanonPurpose::OpenPort);
    selected
}

fn select_canonical_with_incidence(
    topology: &PartialTopology,
    incidence: &IncidenceGraph,
    encoding: EncodingKind,
    individualized_vertex: Option<VertexId>,
    cancel: Option<&AtomicBool>,
) -> Option<SelectedCanonical> {
    if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
        return None;
    }
    if encoding != EncodingKind::Witness {
        return select_canonical_with_canonaut(
            topology,
            incidence,
            encoding,
            individualized_vertex,
            cancel,
        );
    }
    let _witness_timer = CanonicalTimer::start(CanonicalPhase::WitnessSearch);
    // Symmetric physical ports have an exact lexicographic minimum once the
    // terminal and node labels are fixed. Enumerating their independent
    // permutations only repeats that deterministic choice factorially.
    let groups = incidence.witness_labeling_groups(topology);
    let ranks = vec![None; incidence.base_colors.len()];
    let mut best = None;
    let mut encoder = WitnessEncoder::new(topology, incidence, &groups);
    if !search_individualizations(
        topology,
        incidence,
        &groups,
        0,
        0,
        ranks,
        &mut encoder,
        cancel,
        &mut best,
    ) {
        return None;
    }
    Some(best.expect("individualization always reaches at least one discrete coloring"))
}

fn select_canonical_with_canonaut(
    topology: &PartialTopology,
    incidence: &IncidenceGraph,
    encoding: EncodingKind,
    individualized_vertex: Option<VertexId>,
    cancel: Option<&AtomicBool>,
) -> Option<SelectedCanonical> {
    let original_vertex_count = incidence.base_colors.len();
    let dense_timer = CanonicalTimer::start(CanonicalPhase::DenseGraph);
    let edge_count = incidence
        .adjacency
        .iter()
        .enumerate()
        .map(|(left, neighbors)| neighbors.iter().filter(|(_, right)| left < *right).count())
        .sum::<usize>();
    let mut graph = DenseGraph::new(original_vertex_count.saturating_add(edge_count));
    let mut colors = incidence
        .base_colors
        .iter()
        .cloned()
        .map(CanonicalVertexColor::Original)
        .collect::<Vec<_>>();
    let mut edge_vertex = original_vertex_count;
    for (left, neighbors) in incidence.adjacency.iter().enumerate() {
        for &(edge_color, right) in neighbors {
            if left >= right {
                continue;
            }
            graph.add_edge(left, edge_vertex);
            graph.add_edge(edge_vertex, right);
            colors.push(CanonicalVertexColor::Incidence(edge_color));
            edge_vertex += 1;
        }
    }
    debug_assert_eq!(edge_vertex, graph.number_of_vertices());
    let mut color_ids = ordered_color_ids(&colors);
    if let Some(vertex) = individualized_vertex {
        color_ids[vertex] = color_ids
            .iter()
            .copied()
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .expect("canonical color count must fit u32");
    }
    graph.set_colors(color_ids);
    drop(dense_timer);
    if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
        return None;
    }
    let labeling_timer = CanonicalTimer::start(CanonicalPhase::Labeling);
    let labeling_activity = crate::diagnostics::SlowCall::start(
        u32::try_from(topology.nodes.len()).unwrap_or(u32::MAX),
    );
    let mut manager = CanonautManager::new(graph.number_of_vertices()).with_canonization();
    manager.canonize_graph(&graph);
    drop(labeling_timer);
    labeling_activity.finish(cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)));
    if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
        return None;
    }

    let relabel_timer = CanonicalTimer::start(CanonicalPhase::Relabel);
    let mut positions = vec![usize::MAX; original_vertex_count];
    for (position, &raw_vertex) in manager.labeling().iter().enumerate() {
        let vertex = usize::try_from(raw_vertex).expect("canonical vertex index must fit usize");
        if vertex < original_vertex_count {
            positions[vertex] = position;
        }
    }
    let groups = incidence.labeling_groups(topology);
    let mut ranks = vec![None; original_vertex_count];
    for group in groups {
        let mut ordered = group;
        ordered.sort_by_key(|&vertex| positions[vertex]);
        for (rank, vertex) in ordered.into_iter().enumerate() {
            ranks[vertex] = Some(u32::try_from(rank).expect("canonical rank must fit u32"));
        }
    }
    let candidate = incidence.relabel(topology, &ranks);
    drop(relabel_timer);
    let bytes = match encoding {
        EncodingKind::State => {
            let mut bytes = encode_partial_state(topology, &candidate, PartialMark::None);
            encode_semantic_system(topology, &candidate, &mut bytes);
            bytes
        }
        EncodingKind::Layout => encode_partial_state(topology, &candidate, PartialMark::None),
        EncodingKind::SccSummary => encode_scc_summary_input(topology, &candidate),
        EncodingKind::Marked => encode_partial_state(topology, &candidate, PartialMark::Link),
        EncodingKind::MarkedPort => encode_partial_state(topology, &candidate, PartialMark::Port),
        EncodingKind::Witness => unreachable!("witness protocol uses exhaustive minimization"),
    };
    Some(SelectedCanonical {
        bytes,
        topology: candidate,
        ranks,
    })
}

#[allow(clippy::too_many_arguments)]
fn search_individualizations(
    topology: &PartialTopology,
    incidence: &IncidenceGraph,
    groups: &[Vec<VertexId>],
    group_index: usize,
    rank_in_group: usize,
    ranks: Vec<Option<u32>>,
    encoder: &mut WitnessEncoder,
    cancel: Option<&AtomicBool>,
    best: &mut Option<SelectedCanonical>,
) -> bool {
    hotspot_profile::record_witness_branch();
    if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
        return false;
    }
    if group_index < groups.len() {
        let group = &groups[group_index];
        if rank_in_group == group.len() {
            return search_individualizations(
                topology,
                incidence,
                groups,
                group_index + 1,
                0,
                ranks,
                encoder,
                cancel,
                best,
            );
        }

        let choices = group
            .iter()
            .copied()
            .filter(|vertex| ranks[*vertex].is_none())
            .collect::<Vec<_>>();
        // Visit exactly the same full set of labelings without refining colors
        // just to order them. Only ranks determine the leaf encoding, and the
        // minimum over all leaves is independent of their traversal order.
        for vertex in choices {
            if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
                return false;
            }
            let mut next_ranks = ranks.clone();
            next_ranks[vertex] =
                Some(u32::try_from(rank_in_group).expect("one label group must fit a u32 rank"));
            if !search_individualizations(
                topology,
                incidence,
                groups,
                group_index,
                rank_in_group + 1,
                next_ranks,
                encoder,
                cancel,
                best,
            ) {
                return false;
            }
        }
        return true;
    }

    hotspot_profile::record_witness_leaf();
    let _leaf_timer = CanonicalTimer::start(CanonicalPhase::WitnessLeaf);
    let mut ranks = ranks;
    encoder.complete_port_ranks(&mut ranks);
    let bytes = encoder.encode(&ranks);
    if best
        .as_ref()
        .is_none_or(|current| bytes < current.bytes.as_slice())
    {
        *best = Some(SelectedCanonical {
            bytes: bytes.to_vec(),
            topology: incidence.relabel(topology, &ranks),
            ranks,
        });
    }
    true
}

/// Ranks are consecutive within the exact groups enumerated by the witness
/// search. Offsets turn those ranks into the same labels as `relabel`.
#[derive(Clone, Copy)]
struct WitnessLabel {
    vertex: VertexId,
    offset: u32,
}

impl WitnessLabel {
    fn get(self, ranks: &[Option<u32>]) -> u32 {
        self.offset + ranks[self.vertex].expect("witness label must be ranked")
    }
}

struct WitnessLinkPlan {
    producer: ProducerPortRef,
    consumer: ConsumerPortRef,
    producer_label: WitnessLabel,
    consumer_label: WitnessLabel,
    producer_port: Option<VertexId>,
    consumer_port: Option<VertexId>,
    flow_bytes: Vec<u8>,
}

struct WitnessEncoder {
    prefix_len: usize,
    bytes: Vec<u8>,
    links: Vec<WitnessLinkPlan>,
    ordered_links: Vec<(ProducerPortRef, ConsumerPortRef, usize)>,
    port_groups: Vec<Vec<VertexId>>,
}

fn canonical_witness_nodes(
    topology: &PartialTopology,
    labels: &BTreeMap<NodeId, NodeId>,
) -> Vec<PhysicalNode> {
    let mut nodes = topology
        .nodes
        .iter()
        .map(|node| PhysicalNode {
            id: labels[&node.id],
            node_type: node.node_type,
        })
        .collect::<Vec<_>>();
    nodes.sort();
    nodes
}

impl WitnessEncoder {
    fn new(
        topology: &PartialTopology,
        incidence: &IncidenceGraph,
        groups: &[Vec<VertexId>],
    ) -> Self {
        // One ordinary labeling supplies invariant node/type and terminal-rate
        // ordering. Every later leaf uses only rank lookups and cached flow bytes.
        let mut ranks = vec![None; incidence.base_colors.len()];
        for group in groups {
            for (rank, &vertex) in group.iter().enumerate() {
                ranks[vertex] = Some(u32::try_from(rank).unwrap());
            }
        }
        let inputs =
            canonical_terminal_labels(&topology.problem.inputs, &incidence.input_terminals, &ranks);
        let outputs = canonical_terminal_labels(
            &topology.problem.outputs,
            &incidence.output_terminals,
            &ranks,
        );
        let nodes = canonical_node_labels(topology, &incidence.node_vertices, &ranks);
        let initial_nodes = canonical_witness_nodes(topology, &nodes);
        let types = topology
            .nodes
            .iter()
            .map(|n| (n.id, n.node_type))
            .collect::<BTreeMap<_, _>>();
        let label = |vertex, initial_label| WitnessLabel {
            vertex,
            offset: initial_label - ranks[vertex].unwrap(),
        };
        let links = topology
            .links
            .iter()
            .map(|link| {
                let (producer_label, producer_port) = match link.producer {
                    ProducerPortRef::Input(index) => (
                        label(
                            incidence.input_terminals[index.0 as usize],
                            inputs[index.0 as usize],
                        ),
                        None,
                    ),
                    ProducerPortRef::Node { node, .. } => (
                        label(incidence.node_vertices[&node], nodes[&node].0),
                        matches!(types[&node], NodeType::Splitter2 | NodeType::Splitter3)
                            .then(|| incidence.producer_ports[&link.producer]),
                    ),
                };
                let (consumer_label, consumer_port) = match link.consumer {
                    ConsumerPortRef::Output(index) => (
                        label(
                            incidence.output_terminals[index.0 as usize],
                            outputs[index.0 as usize],
                        ),
                        None,
                    ),
                    ConsumerPortRef::Discard(index) => (
                        WitnessLabel {
                            vertex: incidence.discard_terminals[index.0 as usize],
                            offset: 0,
                        },
                        None,
                    ),
                    ConsumerPortRef::Node { node, .. } => (
                        label(incidence.node_vertices[&node], nodes[&node].0),
                        matches!(types[&node], NodeType::Merger2 | NodeType::Merger3)
                            .then(|| incidence.consumer_ports[&link.consumer]),
                    ),
                };
                let mut flow_bytes = Vec::new();
                write_rational(
                    &mut flow_bytes,
                    link.flow.as_ref().expect("full witness flow"),
                );
                WitnessLinkPlan {
                    producer: link.producer,
                    consumer: link.consumer,
                    producer_label,
                    consumer_label,
                    producer_port,
                    consumer_port,
                    flow_bytes,
                }
            })
            .collect::<Vec<_>>();
        let mut bytes = b"satisfactory-canonical-graph\0\x01".to_vec();
        let mut inputs = topology.problem.inputs.clone();
        let mut outputs = topology.problem.outputs.clone();
        inputs.sort();
        outputs.sort();
        write_rates(&mut bytes, &inputs);
        write_rates(&mut bytes, &outputs);
        write_len(&mut bytes, initial_nodes.len());
        for node in &initial_nodes {
            write_u32(&mut bytes, node.id.0);
            bytes.push(node_type_tag(node.node_type));
        }
        write_len(&mut bytes, links.len());
        Self {
            prefix_len: bytes.len(),
            bytes,
            ordered_links: Vec::with_capacity(links.len()),
            links,
            port_groups: incidence.symmetric_port_groups(topology),
        }
    }

    /// Completes the unique lexicographically minimal symmetric-port labeling
    /// for fixed terminal, discard and node labels.
    ///
    /// A splitter's links form one contiguous producer block. Assigning its
    /// output ranks by `(consumer-without-symmetric-port, flow)` therefore
    /// minimizes that block. Once those blocks are fixed, global producer order
    /// is fixed, and assigning each merger's input ranks in first-occurrence
    /// order minimizes every consumer endpoint. Parallel splitter-to-merger
    /// links are covered: their consumer base is equal, so exact flow orders
    /// them, and the merger ranks then follow that same producer order.
    fn complete_port_ranks(&self, ranks: &mut [Option<u32>]) {
        // Use protocol tag order, which deliberately differs from the public
        // enum's declaration order for discard and node consumers.
        let consumer_bases = self
            .links
            .iter()
            .map(|link| match link.consumer {
                ConsumerPortRef::Output(_) => (0_u8, link.consumer_label.get(ranks)),
                ConsumerPortRef::Node { .. } => (1, link.consumer_label.get(ranks)),
                ConsumerPortRef::Discard(_) => (2, link.consumer_label.get(ranks)),
            })
            .collect::<Vec<_>>();
        let mut splitter_links = BTreeMap::<NodeId, Vec<usize>>::new();
        for (index, link) in self.links.iter().enumerate() {
            if link.producer_port.is_some() {
                let ProducerPortRef::Node { node, .. } = link.producer else {
                    unreachable!("only splitter node producers have symmetric ports")
                };
                splitter_links.entry(node).or_default().push(index);
            }
        }
        for links in splitter_links.values_mut() {
            links.sort_unstable_by(|&left, &right| {
                consumer_bases[left]
                    .cmp(&consumer_bases[right])
                    .then_with(|| {
                        self.links[left]
                            .flow_bytes
                            .cmp(&self.links[right].flow_bytes)
                    })
                    .then_with(|| left.cmp(&right))
            });
            for (rank, &index) in links.iter().enumerate() {
                ranks[self.links[index].producer_port.unwrap()] =
                    Some(u32::try_from(rank).expect("physical arity must fit u32"));
            }
        }

        let producer = |link: &WitnessLinkPlan| match link.producer {
            ProducerPortRef::Input(_) => {
                ProducerPortRef::Input(InputTerminalIndex(link.producer_label.get(ranks)))
            }
            ProducerPortRef::Node { .. } => ProducerPortRef::Node {
                node: NodeId(link.producer_label.get(ranks)),
                port: link
                    .producer_port
                    .map_or(0, |vertex| u8::try_from(ranks[vertex].unwrap()).unwrap()),
            },
        };
        let mut producer_order = (0..self.links.len()).collect::<Vec<_>>();
        producer_order.sort_unstable_by_key(|&index| producer(&self.links[index]));
        let mut merger_ranks = BTreeMap::<NodeId, u32>::new();
        for index in producer_order {
            let link = &self.links[index];
            let Some(vertex) = link.consumer_port else {
                continue;
            };
            let ConsumerPortRef::Node { node, .. } = link.consumer else {
                unreachable!("only merger node consumers have symmetric ports")
            };
            let rank = merger_ranks.entry(node).or_default();
            ranks[vertex] = Some(*rank);
            *rank += 1;
        }

        // A malformed but structurally accepted graph may leave a physical port
        // unused. Such a rank cannot affect the witness bytes; fill it stably so
        // the canonical concrete topology can still be produced.
        for group in &self.port_groups {
            let free = (0..u32::try_from(group.len()).unwrap())
                .filter(|rank| !group.iter().any(|&vertex| ranks[vertex] == Some(*rank)))
                .collect::<Vec<_>>();
            let mut free = free.into_iter();
            for &vertex in group {
                if ranks[vertex].is_none() {
                    ranks[vertex] = Some(free.next().expect("every port group has enough ranks"));
                }
            }
        }
    }

    fn encode(&mut self, ranks: &[Option<u32>]) -> &[u8] {
        self.ordered_links.clear();
        let port = |vertex: Option<VertexId>| {
            vertex.map_or(0, |v| u8::try_from(ranks[v].unwrap()).unwrap())
        };
        for (index, link) in self.links.iter().enumerate() {
            let producer_label = link.producer_label.get(ranks);
            let consumer_label = link.consumer_label.get(ranks);
            let producer = match link.producer {
                ProducerPortRef::Input(_) => {
                    ProducerPortRef::Input(InputTerminalIndex(producer_label))
                }
                ProducerPortRef::Node { .. } => ProducerPortRef::Node {
                    node: NodeId(producer_label),
                    port: port(link.producer_port),
                },
            };
            let consumer = match link.consumer {
                ConsumerPortRef::Output(_) => {
                    ConsumerPortRef::Output(OutputTerminalIndex(consumer_label))
                }
                ConsumerPortRef::Discard(_) => {
                    ConsumerPortRef::Discard(DiscardTerminalIndex(consumer_label))
                }
                ConsumerPortRef::Node { .. } => ConsumerPortRef::Node {
                    node: NodeId(consumer_label),
                    port: port(link.consumer_port),
                },
            };
            self.ordered_links.push((producer, consumer, index));
        }
        // IncidenceGraph::build rejects repeated physical ports, so endpoint
        // pairs are unique. Flows never break a sorting tie for valid witnesses.
        self.ordered_links.sort_unstable_by_key(|&(p, c, _)| (p, c));
        self.bytes.truncate(self.prefix_len);
        for &(producer, consumer, index) in &self.ordered_links {
            write_producer(&mut self.bytes, producer);
            write_consumer(&mut self.bytes, consumer);
            self.bytes.extend_from_slice(&self.links[index].flow_bytes);
        }
        &self.bytes
    }
}

type VertexId = usize;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum BaseColor {
    ProblemCapacity(Rational),
    RemainingProfile(NodeProfile),
    InputTerminal(Rational),
    OutputTerminal(Rational),
    DiscardTerminal,
    Operator {
        node_type: NodeType,
        scc_member: bool,
    },
    ProducerPort {
        open: bool,
        known: Option<Rational>,
    },
    ConsumerPort {
        open: bool,
        known: Option<Rational>,
    },
    Link {
        flow: Option<Rational>,
        marked: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum EdgeColor {
    OwnsProducerPort,
    OwnsConsumerPort,
    LinkProducerIncidence,
    LinkConsumerIncidence,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum CanonicalVertexColor {
    Original(BaseColor),
    Incidence(EdgeColor),
}

#[derive(Clone, Debug)]
struct IncidenceGraph {
    base_colors: Vec<BaseColor>,
    adjacency: Vec<Vec<(EdgeColor, VertexId)>>,
    input_terminals: Vec<VertexId>,
    output_terminals: Vec<VertexId>,
    discard_terminals: Vec<VertexId>,
    node_vertices: BTreeMap<NodeId, VertexId>,
    producer_ports: BTreeMap<ProducerPortRef, VertexId>,
    consumer_ports: BTreeMap<ConsumerPortRef, VertexId>,
    marked_port: Option<MarkedPort>,
    scc_nodes: BTreeSet<NodeId>,
    known_producers: BTreeMap<ProducerPortRef, Rational>,
    known_consumers: BTreeMap<ConsumerPortRef, Rational>,
}

impl IncidenceGraph {
    #[allow(clippy::too_many_lines)]
    fn build(
        topology: &PartialTopology,
        marked_link: Option<usize>,
        include_link_flows: bool,
    ) -> Self {
        Self::build_annotated(topology, marked_link, include_link_flows, None)
    }

    #[allow(clippy::too_many_lines)]
    fn build_annotated(
        topology: &PartialTopology,
        marked_link: Option<usize>,
        include_link_flows: bool,
        annotations: Option<&SccAnnotations<'_>>,
    ) -> Self {
        let mut graph = Self {
            base_colors: Vec::new(),
            adjacency: Vec::new(),
            input_terminals: Vec::new(),
            output_terminals: Vec::new(),
            discard_terminals: Vec::new(),
            node_vertices: BTreeMap::new(),
            producer_ports: BTreeMap::new(),
            consumer_ports: BTreeMap::new(),
            marked_port: None,
            scc_nodes: annotations.map_or_else(BTreeSet::new, |value| value.region_nodes.clone()),
            known_producers: annotations
                .map_or_else(BTreeMap::new, |value| value.known_producers.clone()),
            known_consumers: annotations
                .map_or_else(BTreeMap::new, |value| value.known_consumers.clone()),
        };
        graph.add_vertex(BaseColor::ProblemCapacity(
            topology.problem.max_link_rate.clone(),
        ));
        graph.add_vertex(BaseColor::RemainingProfile(topology.remaining_profile));

        let used_producers = topology
            .links
            .iter()
            .map(|link| link.producer)
            .collect::<BTreeSet<_>>();
        let used_consumers = topology
            .links
            .iter()
            .map(|link| link.consumer)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            used_producers.len(),
            topology.links.len(),
            "a producer port may have at most one physical link"
        );
        assert_eq!(
            used_consumers.len(),
            topology.links.len(),
            "a consumer port may have at most one physical link"
        );

        for (index, rate) in topology.problem.inputs.iter().cloned().enumerate() {
            let terminal = graph.add_vertex(BaseColor::InputTerminal(rate));
            graph.input_terminals.push(terminal);
            let reference = ProducerPortRef::Input(InputTerminalIndex(
                u32::try_from(index).expect("input terminal count must fit u32"),
            ));
            let port = graph.add_vertex(BaseColor::ProducerPort {
                open: !used_producers.contains(&reference),
                known: graph.known_producers.get(&reference).cloned(),
            });
            graph.producer_ports.insert(reference, port);
            graph.add_edge(terminal, port, EdgeColor::OwnsProducerPort);
        }
        for (index, rate) in topology.problem.outputs.iter().cloned().enumerate() {
            let terminal = graph.add_vertex(BaseColor::OutputTerminal(rate));
            graph.output_terminals.push(terminal);
            let reference = ConsumerPortRef::Output(OutputTerminalIndex(
                u32::try_from(index).expect("output terminal count must fit u32"),
            ));
            let port = graph.add_vertex(BaseColor::ConsumerPort {
                open: !used_consumers.contains(&reference),
                known: graph.known_consumers.get(&reference).cloned(),
            });
            graph.consumer_ports.insert(reference, port);
            graph.add_edge(terminal, port, EdgeColor::OwnsConsumerPort);
        }
        for index in 0..topology.discard_count {
            let terminal = graph.add_vertex(BaseColor::DiscardTerminal);
            graph.discard_terminals.push(terminal);
            let reference = ConsumerPortRef::Discard(DiscardTerminalIndex(index));
            let port = graph.add_vertex(BaseColor::ConsumerPort {
                open: !used_consumers.contains(&reference),
                known: graph.known_consumers.get(&reference).cloned(),
            });
            graph.consumer_ports.insert(reference, port);
            graph.add_edge(terminal, port, EdgeColor::OwnsConsumerPort);
        }

        for node in &topology.nodes {
            let operator = graph.add_vertex(BaseColor::Operator {
                node_type: node.node_type,
                scc_member: graph.scc_nodes.contains(&node.id),
            });
            assert!(
                graph.node_vertices.insert(node.id, operator).is_none(),
                "physical node identifiers must be unique"
            );
            for port in 0..node.node_type.output_port_count() {
                let reference = ProducerPortRef::Node {
                    node: node.id,
                    port,
                };
                let vertex = graph.add_vertex(BaseColor::ProducerPort {
                    open: !used_producers.contains(&reference),
                    known: graph.known_producers.get(&reference).cloned(),
                });
                graph.producer_ports.insert(reference, vertex);
                graph.add_edge(operator, vertex, EdgeColor::OwnsProducerPort);
            }
            for port in 0..node.node_type.input_port_count() {
                let reference = ConsumerPortRef::Node {
                    node: node.id,
                    port,
                };
                let vertex = graph.add_vertex(BaseColor::ConsumerPort {
                    open: !used_consumers.contains(&reference),
                    known: graph.known_consumers.get(&reference).cloned(),
                });
                graph.consumer_ports.insert(reference, vertex);
                graph.add_edge(operator, vertex, EdgeColor::OwnsConsumerPort);
            }
        }

        for (index, link) in topology.links.iter().enumerate() {
            let link_vertex = graph.add_vertex(BaseColor::Link {
                flow: if include_link_flows {
                    link.flow.clone()
                } else {
                    None
                },
                marked: marked_link == Some(index),
            });
            let producer = *graph
                .producer_ports
                .get(&link.producer)
                .expect("link producer must be a declared effective port");
            let consumer = *graph
                .consumer_ports
                .get(&link.consumer)
                .expect("link consumer must be a declared effective port");
            graph.add_edge(link_vertex, producer, EdgeColor::LinkProducerIncidence);
            graph.add_edge(link_vertex, consumer, EdgeColor::LinkConsumerIncidence);
        }
        graph
    }

    fn mark_port(&mut self, port: MarkedPort) -> VertexId {
        let vertex = match port {
            MarkedPort::Producer(reference) => *self
                .producer_ports
                .get(&reference)
                .expect("marked producer port must be declared"),
            MarkedPort::Consumer(reference) => *self
                .consumer_ports
                .get(&reference)
                .expect("marked consumer port must be declared"),
        };
        self.marked_port = Some(port);
        vertex
    }

    fn add_vertex(&mut self, color: BaseColor) -> VertexId {
        let vertex = self.base_colors.len();
        self.base_colors.push(color);
        self.adjacency.push(Vec::new());
        vertex
    }

    fn add_edge(&mut self, left: VertexId, right: VertexId, color: EdgeColor) {
        self.adjacency[left].push((color, right));
        self.adjacency[right].push((color, left));
    }

    fn labeling_groups(&self, topology: &PartialTopology) -> Vec<Vec<VertexId>> {
        let mut groups = self.witness_labeling_groups(topology);
        groups.extend(self.symmetric_port_groups(topology));
        groups
    }

    fn witness_labeling_groups(&self, topology: &PartialTopology) -> Vec<Vec<VertexId>> {
        let mut groups = Vec::new();

        let mut input_classes = BTreeMap::<Rational, Vec<VertexId>>::new();
        for (index, rate) in topology.problem.inputs.iter().cloned().enumerate() {
            input_classes
                .entry(rate)
                .or_default()
                .push(self.input_terminals[index]);
        }
        groups.extend(input_classes.into_values());

        let mut output_classes = BTreeMap::<Rational, Vec<VertexId>>::new();
        for (index, rate) in topology.problem.outputs.iter().cloned().enumerate() {
            output_classes
                .entry(rate)
                .or_default()
                .push(self.output_terminals[index]);
        }
        groups.extend(output_classes.into_values());
        if !self.discard_terminals.is_empty() {
            groups.push(self.discard_terminals.clone());
        }

        let mut node_classes = BTreeMap::<NodeType, Vec<VertexId>>::new();
        for node in &topology.nodes {
            node_classes
                .entry(node.node_type)
                .or_default()
                .push(self.node_vertices[&node.id]);
        }
        groups.extend(node_classes.into_values());

        groups
    }

    fn symmetric_port_groups(&self, topology: &PartialTopology) -> Vec<Vec<VertexId>> {
        let mut groups = Vec::new();
        let mut nodes = topology.nodes.iter().collect::<Vec<_>>();
        nodes.sort_by_key(|node| node.id);
        for node in nodes {
            match node.node_type {
                NodeType::Splitter2 | NodeType::Splitter3 => groups.push(
                    (0..node.node_type.output_port_count())
                        .map(|port| {
                            self.producer_ports[&ProducerPortRef::Node {
                                node: node.id,
                                port,
                            }]
                        })
                        .collect(),
                ),
                NodeType::Merger2 | NodeType::Merger3 => groups.push(
                    (0..node.node_type.input_port_count())
                        .map(|port| {
                            self.consumer_ports[&ConsumerPortRef::Node {
                                node: node.id,
                                port,
                            }]
                        })
                        .collect(),
                ),
            }
        }
        groups
    }

    #[allow(clippy::too_many_lines)]
    fn relabel(
        &self,
        topology: &PartialTopology,
        ranks: &[Option<u32>],
    ) -> CanonicalPartialTopology {
        let input_labels =
            canonical_terminal_labels(&topology.problem.inputs, &self.input_terminals, ranks);
        let output_labels =
            canonical_terminal_labels(&topology.problem.outputs, &self.output_terminals, ranks);
        let discard_labels = canonical_anonymous_labels(&self.discard_terminals, ranks);
        let node_labels = canonical_node_labels(topology, &self.node_vertices, ranks);
        let (splitter_outputs, merger_inputs) = canonical_port_labels(topology, self, ranks);

        let node_types = topology
            .nodes
            .iter()
            .map(|node| (node.id, node.node_type))
            .collect::<BTreeMap<_, _>>();
        let mut nodes = topology
            .nodes
            .iter()
            .map(|node| PhysicalNode {
                id: *node_labels
                    .get(&node.id)
                    .expect("every node has a canonical label"),
                node_type: node.node_type,
            })
            .collect::<Vec<_>>();
        nodes.sort();

        let mut links = topology
            .links
            .iter()
            .enumerate()
            .map(|(index, link)| CanonicalPartialLink {
                producer: relabel_producer(
                    link.producer,
                    &input_labels,
                    &node_labels,
                    &splitter_outputs,
                    &node_types,
                ),
                consumer: relabel_consumer(
                    link.consumer,
                    &output_labels,
                    &discard_labels,
                    &node_labels,
                    &merger_inputs,
                    &node_types,
                ),
                flow: link.flow.clone(),
                marked: matches!(
                    self.base_colors_for_link(index),
                    Some(BaseColor::Link { marked: true, .. })
                ),
            })
            .collect::<Vec<_>>();
        links.sort();
        let marked_producer = match self.marked_port {
            Some(MarkedPort::Producer(reference)) => Some(relabel_producer(
                reference,
                &input_labels,
                &node_labels,
                &splitter_outputs,
                &node_types,
            )),
            _ => None,
        };
        let marked_consumer = match self.marked_port {
            Some(MarkedPort::Consumer(reference)) => Some(relabel_consumer(
                reference,
                &output_labels,
                &discard_labels,
                &node_labels,
                &merger_inputs,
                &node_types,
            )),
            _ => None,
        };
        let mut scc_nodes = self
            .scc_nodes
            .iter()
            .map(|node| node_labels[node])
            .collect::<Vec<_>>();
        scc_nodes.sort_unstable();
        let mut known_producers = self
            .known_producers
            .iter()
            .map(|(producer, value)| {
                (
                    relabel_producer(
                        *producer,
                        &input_labels,
                        &node_labels,
                        &splitter_outputs,
                        &node_types,
                    ),
                    value.clone(),
                )
            })
            .collect::<Vec<_>>();
        known_producers.sort();
        let mut known_consumers = self
            .known_consumers
            .iter()
            .map(|(consumer, value)| {
                (
                    relabel_consumer(
                        *consumer,
                        &output_labels,
                        &discard_labels,
                        &node_labels,
                        &merger_inputs,
                        &node_types,
                    ),
                    value.clone(),
                )
            })
            .collect::<Vec<_>>();
        known_consumers.sort();
        CanonicalPartialTopology {
            nodes,
            links,
            marked_producer,
            marked_consumer,
            scc_nodes,
            known_producers,
            known_consumers,
        }
    }

    fn port_relabeling(
        &self,
        topology: &PartialTopology,
        ranks: &[Option<u32>],
    ) -> (
        BTreeMap<ProducerPortRef, ProducerPortRef>,
        BTreeMap<ConsumerPortRef, ConsumerPortRef>,
    ) {
        let input_labels =
            canonical_terminal_labels(&topology.problem.inputs, &self.input_terminals, ranks);
        let output_labels =
            canonical_terminal_labels(&topology.problem.outputs, &self.output_terminals, ranks);
        let discard_labels = canonical_anonymous_labels(&self.discard_terminals, ranks);
        let node_labels = canonical_node_labels(topology, &self.node_vertices, ranks);
        let (splitter_outputs, merger_inputs) = canonical_port_labels(topology, self, ranks);
        let node_types = topology
            .nodes
            .iter()
            .map(|node| (node.id, node.node_type))
            .collect::<BTreeMap<_, _>>();
        let producers = self
            .producer_ports
            .keys()
            .copied()
            .map(|producer| {
                (
                    producer,
                    relabel_producer(
                        producer,
                        &input_labels,
                        &node_labels,
                        &splitter_outputs,
                        &node_types,
                    ),
                )
            })
            .collect();
        let consumers = self
            .consumer_ports
            .keys()
            .copied()
            .map(|consumer| {
                (
                    consumer,
                    relabel_consumer(
                        consumer,
                        &output_labels,
                        &discard_labels,
                        &node_labels,
                        &merger_inputs,
                        &node_types,
                    ),
                )
            })
            .collect();
        (producers, consumers)
    }

    fn base_colors_for_link(&self, target_index: usize) -> Option<&BaseColor> {
        self.base_colors
            .iter()
            .filter(|color| matches!(color, BaseColor::Link { .. }))
            .nth(target_index)
    }
}

fn ordered_color_ids<T: Ord>(values: &[T]) -> Vec<u32> {
    let mut ordered = values.iter().collect::<Vec<_>>();
    ordered.sort();
    ordered.dedup();
    values
        .iter()
        .map(|value| {
            u32::try_from(
                ordered
                    .binary_search(&value)
                    .expect("color must be present"),
            )
            .expect("incidence graph must fit u32 colors")
        })
        .collect()
}

fn canonical_terminal_labels(
    rates: &[Rational],
    vertices: &[VertexId],
    ranks: &[Option<u32>],
) -> Vec<u32> {
    let mut classes = BTreeMap::<Rational, Vec<usize>>::new();
    for (index, rate) in rates.iter().cloned().enumerate() {
        classes.entry(rate).or_default().push(index);
    }
    let mut labels = vec![0; rates.len()];
    let mut offset = 0_u32;
    for terminals in classes.values_mut() {
        terminals.sort_by_key(|index| {
            ranks[vertices[*index]].expect("every terminal receives a canonical rank")
        });
        for (within_class, &terminal) in terminals.iter().enumerate() {
            labels[terminal] =
                offset + u32::try_from(within_class).expect("terminal count must fit public index");
        }
        offset += u32::try_from(terminals.len()).expect("terminal count must fit public index");
    }
    labels
}

fn canonical_anonymous_labels(vertices: &[VertexId], ranks: &[Option<u32>]) -> Vec<u32> {
    let mut terminals = (0..vertices.len()).collect::<Vec<_>>();
    terminals.sort_by_key(|index| {
        ranks[vertices[*index]].expect("every discard terminal receives a canonical rank")
    });
    let mut labels = vec![0; vertices.len()];
    for (canonical, terminal) in terminals.into_iter().enumerate() {
        labels[terminal] =
            u32::try_from(canonical).expect("discard terminal count must fit public index");
    }
    labels
}

fn canonical_node_labels(
    topology: &PartialTopology,
    vertices: &BTreeMap<NodeId, VertexId>,
    ranks: &[Option<u32>],
) -> BTreeMap<NodeId, NodeId> {
    let mut classes = BTreeMap::<NodeType, Vec<NodeId>>::new();
    for node in &topology.nodes {
        classes.entry(node.node_type).or_default().push(node.id);
    }
    let mut labels = BTreeMap::new();
    let mut offset = 0_u32;
    for nodes in classes.values_mut() {
        nodes.sort_by_key(|node| {
            ranks[vertices[node]].expect("every physical node receives a canonical rank")
        });
        for (within_type, node) in nodes.iter().copied().enumerate() {
            labels.insert(
                node,
                NodeId(offset + u32::try_from(within_type).expect("node count must fit public ID")),
            );
        }
        offset += u32::try_from(nodes.len()).expect("node count must fit public ID");
    }
    labels
}

type PortLabels = BTreeMap<(NodeId, u8), u8>;

fn canonical_port_labels(
    topology: &PartialTopology,
    incidence: &IncidenceGraph,
    ranks: &[Option<u32>],
) -> (PortLabels, PortLabels) {
    let mut splitter_outputs = BTreeMap::new();
    let mut merger_inputs = BTreeMap::new();
    for node in &topology.nodes {
        match node.node_type {
            NodeType::Splitter2 | NodeType::Splitter3 => {
                let mut ports = (0..node.node_type.output_port_count()).collect::<Vec<_>>();
                ports.sort_by_key(|port| {
                    ranks[incidence.producer_ports[&ProducerPortRef::Node {
                        node: node.id,
                        port: *port,
                    }]]
                        .expect("every symmetric splitter port receives a canonical rank")
                });
                for (canonical, old) in ports.into_iter().enumerate() {
                    splitter_outputs.insert(
                        (node.id, old),
                        u8::try_from(canonical).expect("physical arity is at most three"),
                    );
                }
            }
            NodeType::Merger2 | NodeType::Merger3 => {
                let mut ports = (0..node.node_type.input_port_count()).collect::<Vec<_>>();
                ports.sort_by_key(|port| {
                    ranks[incidence.consumer_ports[&ConsumerPortRef::Node {
                        node: node.id,
                        port: *port,
                    }]]
                        .expect("every symmetric merger port receives a canonical rank")
                });
                for (canonical, old) in ports.into_iter().enumerate() {
                    merger_inputs.insert(
                        (node.id, old),
                        u8::try_from(canonical).expect("physical arity is at most three"),
                    );
                }
            }
        }
    }
    (splitter_outputs, merger_inputs)
}

fn relabel_producer(
    producer: ProducerPortRef,
    input_labels: &[u32],
    node_labels: &BTreeMap<NodeId, NodeId>,
    splitter_outputs: &PortLabels,
    node_types: &BTreeMap<NodeId, NodeType>,
) -> ProducerPortRef {
    match producer {
        ProducerPortRef::Input(input) => ProducerPortRef::Input(InputTerminalIndex(
            input_labels[usize::try_from(input.0).expect("input index must fit usize")],
        )),
        ProducerPortRef::Node { node, port } => {
            let port = match node_types[&node] {
                NodeType::Splitter2 | NodeType::Splitter3 => splitter_outputs[&(node, port)],
                NodeType::Merger2 | NodeType::Merger3 => {
                    assert_eq!(port, 0, "a merger has one output");
                    0
                }
            };
            ProducerPortRef::Node {
                node: node_labels[&node],
                port,
            }
        }
    }
}

fn relabel_consumer(
    consumer: ConsumerPortRef,
    output_labels: &[u32],
    discard_labels: &[u32],
    node_labels: &BTreeMap<NodeId, NodeId>,
    merger_inputs: &PortLabels,
    node_types: &BTreeMap<NodeId, NodeType>,
) -> ConsumerPortRef {
    match consumer {
        ConsumerPortRef::Output(output) => ConsumerPortRef::Output(OutputTerminalIndex(
            output_labels[usize::try_from(output.0).expect("output index must fit usize")],
        )),
        ConsumerPortRef::Discard(discard) => ConsumerPortRef::Discard(DiscardTerminalIndex(
            discard_labels[usize::try_from(discard.0).expect("discard index must fit usize")],
        )),
        ConsumerPortRef::Node { node, port } => {
            let port = match node_types[&node] {
                NodeType::Splitter2 | NodeType::Splitter3 => {
                    assert_eq!(port, 0, "a splitter has one input");
                    0
                }
                NodeType::Merger2 | NodeType::Merger3 => merger_inputs[&(node, port)],
            };
            ConsumerPortRef::Node {
                node: node_labels[&node],
                port,
            }
        }
    }
}

#[cfg(test)]
fn encode_witness(problem: &Problem, topology: &CanonicalPartialTopology) -> Vec<u8> {
    let mut bytes = b"satisfactory-canonical-graph\0\x01".to_vec();
    let mut inputs = problem.inputs.clone();
    inputs.sort();
    let mut outputs = problem.outputs.clone();
    outputs.sort();
    write_rates(&mut bytes, &inputs);
    write_rates(&mut bytes, &outputs);
    write_len(&mut bytes, topology.nodes.len());
    for node in &topology.nodes {
        write_u32(&mut bytes, node.id.0);
        bytes.push(node_type_tag(node.node_type));
    }
    write_len(&mut bytes, topology.links.len());
    for link in &topology.links {
        write_producer(&mut bytes, link.producer);
        write_consumer(&mut bytes, link.consumer);
        write_rational(
            &mut bytes,
            link.flow
                .as_ref()
                .expect("full witness links have exact flows"),
        );
    }
    bytes
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PartialMark {
    None,
    Link,
    Port,
}

fn encode_partial_state(
    source: &PartialTopology,
    topology: &CanonicalPartialTopology,
    mark: PartialMark,
) -> Vec<u8> {
    let mut bytes = match mark {
        PartialMark::None => b"satisfactory-canonical-state\0\x03".to_vec(),
        PartialMark::Link => b"satisfactory-canonical-marked-link\0\x02".to_vec(),
        PartialMark::Port => b"satisfactory-canonical-marked-port\0\x02".to_vec(),
    };
    let mut inputs = source.problem.inputs.clone();
    inputs.sort();
    let mut outputs = source.problem.outputs.clone();
    outputs.sort();
    write_rates(&mut bytes, &inputs);
    write_rates(&mut bytes, &outputs);
    write_u32(&mut bytes, source.discard_count);
    write_rational(&mut bytes, &source.problem.max_link_rate);
    write_profile(&mut bytes, source.remaining_profile);
    write_len(&mut bytes, topology.nodes.len());
    for node in &topology.nodes {
        write_u32(&mut bytes, node.id.0);
        bytes.push(node_type_tag(node.node_type));
    }

    let occupied_producers = topology
        .links
        .iter()
        .map(|link| link.producer)
        .collect::<BTreeSet<_>>();
    let occupied_consumers = topology
        .links
        .iter()
        .map(|link| link.consumer)
        .collect::<BTreeSet<_>>();
    let producers = canonical_producer_ports(inputs.len(), &topology.nodes);
    write_len(&mut bytes, producers.len());
    for producer in producers {
        write_producer(&mut bytes, producer);
        bytes.push(u8::from(occupied_producers.contains(&producer)));
        if mark == PartialMark::Port {
            bytes.push(u8::from(topology.marked_producer == Some(producer)));
        }
    }
    let consumers = canonical_consumer_ports(outputs.len(), source.discard_count, &topology.nodes);
    write_len(&mut bytes, consumers.len());
    for consumer in consumers {
        write_consumer(&mut bytes, consumer);
        bytes.push(u8::from(occupied_consumers.contains(&consumer)));
        if mark == PartialMark::Port {
            bytes.push(u8::from(topology.marked_consumer == Some(consumer)));
        }
    }

    write_len(&mut bytes, topology.links.len());
    for link in &topology.links {
        write_producer(&mut bytes, link.producer);
        write_consumer(&mut bytes, link.consumer);
        if mark == PartialMark::Link {
            bytes.push(u8::from(link.marked));
        }
    }
    bytes
}

fn encode_scc_summary_input(
    source: &PartialTopology,
    topology: &CanonicalPartialTopology,
) -> Vec<u8> {
    let mut state = encode_partial_state(source, topology, PartialMark::None);
    encode_semantic_system(source, topology, &mut state);

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum SemanticInequalityRelation {
    LessThan,
    LessThanOrEqual,
}

fn encode_semantic_system(
    source: &PartialTopology,
    topology: &CanonicalPartialTopology,
    bytes: &mut Vec<u8>,
) {
    let equality_timer = CanonicalTimer::start(CanonicalPhase::Equality);
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
        .map(|(index, port)| (port, index))
        .collect::<BTreeMap<_, _>>();
    let consumer_indices = consumers
        .iter()
        .copied()
        .enumerate()
        .map(|(index, port)| (port, producers.len() + index))
        .collect::<BTreeMap<_, _>>();
    let variable_count = producers.len() + consumers.len();
    let equalities = primitive_equality_basis(
        source,
        topology,
        variable_count,
        &producer_indices,
        &consumer_indices,
    );
    drop(equality_timer);
    let encoding_timer = CanonicalTimer::start(CanonicalPhase::SemanticEncoding);
    // Internal state/SCC protocol only. Public witness and marked-port/link
    // encodings keep their existing byte format and ordering.
    bytes.extend_from_slice(b"sparse-exact-rows\0\x01");
    write_varint(bytes, variable_count);
    write_varint(bytes, equalities.len());
    for row in &equalities {
        write_sparse_row(bytes, row);
    }
    drop(encoding_timer);

    let inequality_timer = CanonicalTimer::start(CanonicalPhase::Inequality);
    let inequalities =
        primitive_inequality_basis(&source.problem.max_link_rate, variable_count, &equalities);
    drop(inequality_timer);
    let _encoding_timer = CanonicalTimer::start(CanonicalPhase::SemanticEncoding);
    write_varint(bytes, inequalities.len());
    for (relation, row) in inequalities {
        bytes.push(match relation {
            SemanticInequalityRelation::LessThan => 0,
            SemanticInequalityRelation::LessThanOrEqual => 1,
        });
        write_sparse_row(bytes, &row);
    }
}

fn write_varint(bytes: &mut Vec<u8>, mut value: usize) {
    while value >= 128 {
        bytes.push(u8::try_from(value & 127).unwrap() | 128);
        value >>= 7;
    }
    bytes.push(u8::try_from(value).unwrap());
}

/// The row width is encoded by the surrounding system. Ordered indices retain
/// every nonzero coefficient, including the RHS, without storing dense zeros.
fn write_sparse_row(bytes: &mut Vec<u8>, row: &[Rational]) {
    write_varint(bytes, row.iter().filter(|value| !value.is_zero()).count());
    for (index, value) in row.iter().enumerate().filter(|(_, value)| !value.is_zero()) {
        write_varint(bytes, index);
        write_compact_rational(bytes, value);
    }
}

fn write_compact_rational(bytes: &mut Vec<u8>, value: &Rational) {
    if value.is_zero() {
        bytes.push(0);
    } else if value.denominator().is_one() && value.numerator().is_one() {
        bytes.push(1);
    } else if value.denominator().is_one()
        && value.numerator().is_negative()
        && value.numerator().magnitude().is_one()
    {
        bytes.push(2);
    } else {
        let integer = value.denominator().is_one();
        bytes.push(if integer { 3 } else { 4 });
        let numerator = value.numerator().to_signed_bytes_le();
        write_varint(bytes, numerator.len());
        bytes.extend_from_slice(&numerator);
        if !integer {
            let denominator = value.denominator().to_signed_bytes_le();
            write_varint(bytes, denominator.len());
            bytes.extend_from_slice(&denominator);
        }
    }
}

fn primitive_equality_basis(
    source: &PartialTopology,
    topology: &CanonicalPartialTopology,
    variable_count: usize,
    producer_indices: &BTreeMap<ProducerPortRef, usize>,
    consumer_indices: &BTreeMap<ConsumerPortRef, usize>,
) -> Vec<Vec<Rational>> {
    let mut rows = Vec::new();
    let mut inputs = source.problem.inputs.clone();
    inputs.sort();
    for (index, rate) in inputs.into_iter().enumerate() {
        let port = ProducerPortRef::Input(InputTerminalIndex(
            u32::try_from(index).expect("input count must fit public index"),
        ));
        rows.push(assignment_row(
            variable_count,
            producer_indices[&port],
            rate,
        ));
    }
    let mut outputs = source.problem.outputs.clone();
    outputs.sort();
    for (index, rate) in outputs.into_iter().enumerate() {
        let port = ConsumerPortRef::Output(OutputTerminalIndex(
            u32::try_from(index).expect("output count must fit public index"),
        ));
        rows.push(assignment_row(
            variable_count,
            consumer_indices[&port],
            rate,
        ));
    }
    let discard_variables = (0..source.discard_count)
        .map(|index| consumer_indices[&ConsumerPortRef::Discard(DiscardTerminalIndex(index))])
        .collect::<Vec<_>>();
    if let Some(surplus) = source.problem.surplus() {
        rows.push(sum_assignment_row(
            variable_count,
            &discard_variables,
            surplus,
        ));
    } else {
        // Production rejects input deficits before search. Represent an invalid
        // balance passed directly to canonicalization by a contradiction row.
        rows.push(sum_assignment_row(variable_count, &[], Rational::one()));
    }

    for node in &topology.nodes {
        let producer_ports = (0..node.node_type.output_port_count())
            .map(|port| {
                producer_indices[&ProducerPortRef::Node {
                    node: node.id,
                    port,
                }]
            })
            .collect::<Vec<_>>();
        let consumer_ports = (0..node.node_type.input_port_count())
            .map(|port| {
                consumer_indices[&ConsumerPortRef::Node {
                    node: node.id,
                    port,
                }]
            })
            .collect::<Vec<_>>();
        match node.node_type {
            NodeType::Splitter2 | NodeType::Splitter3 => {
                for &other in &producer_ports[1..] {
                    rows.push(difference_row(variable_count, producer_ports[0], other));
                }
                rows.push(conservation_row(
                    variable_count,
                    &consumer_ports,
                    &producer_ports,
                ));
            }
            NodeType::Merger2 | NodeType::Merger3 => rows.push(conservation_row(
                variable_count,
                &consumer_ports,
                &producer_ports,
            )),
        }
    }
    for link in &topology.links {
        let producer = producer_indices[&link.producer];
        let consumer = consumer_indices[&link.consumer];
        rows.push(difference_row(variable_count, producer, consumer));
        if let Some(flow) = &link.flow {
            rows.push(assignment_row(variable_count, producer, flow.clone()));
        }
    }
    rational_rref(rows, variable_count)
}

fn primitive_inequality_basis(
    capacity: &Rational,
    variable_count: usize,
    equalities: &[Vec<Rational>],
) -> Vec<(SemanticInequalityRelation, Vec<Rational>)> {
    // RREF pivot columns are unit columns. Reducing +/- one variable therefore
    // uses at most its own pivot row; all other pivot coefficients stay zero.
    let mut pivots = vec![None; variable_count];
    for equality in equalities {
        if let Some(pivot) = equality[..variable_count].iter().position(|v| !v.is_zero()) {
            debug_assert_eq!(equality[pivot].numerator(), equality[pivot].denominator());
            pivots[pivot] = Some(equality);
        }
    }
    let mut rows = Vec::with_capacity(variable_count.saturating_mul(2));
    for (variable, pivot) in pivots.into_iter().enumerate() {
        let (mut positive, mut bounded) = if let Some(equality) = pivot {
            let mut positive = equality.clone();
            positive[variable] = Rational::zero();
            let mut bounded = positive.iter().map(|v| -v).collect::<Vec<_>>();
            bounded[variable_count] = capacity - &positive[variable_count];
            (positive, bounded)
        } else {
            let mut positive = vec![Rational::zero(); variable_count + 1];
            positive[variable] = Rational::from(-1);
            let mut bounded = vec![Rational::zero(); variable_count + 1];
            bounded[variable] = Rational::one();
            bounded[variable_count] = capacity.clone();
            (positive, bounded)
        };
        normalize_positive_scale(&mut positive);
        normalize_positive_scale(&mut bounded);
        rows.push((SemanticInequalityRelation::LessThan, positive));
        rows.push((SemanticInequalityRelation::LessThanOrEqual, bounded));
    }
    rows.sort();
    rows.dedup();
    rows
}

fn assignment_row(variable_count: usize, variable: usize, value: Rational) -> Vec<Rational> {
    let mut row = vec![Rational::zero(); variable_count + 1];
    row[variable] = Rational::one();
    row[variable_count] = value;
    row
}

fn sum_assignment_row(
    variable_count: usize,
    variables: &[usize],
    value: Rational,
) -> Vec<Rational> {
    let mut row = vec![Rational::zero(); variable_count + 1];
    for &variable in variables {
        row[variable] = &row[variable] + Rational::one();
    }
    row[variable_count] = value;
    row
}

fn difference_row(variable_count: usize, left: usize, right: usize) -> Vec<Rational> {
    let mut row = vec![Rational::zero(); variable_count + 1];
    row[left] = Rational::one();
    row[right] = Rational::from(-1);
    row
}

fn conservation_row(
    variable_count: usize,
    consumers: &[usize],
    producers: &[usize],
) -> Vec<Rational> {
    let mut row = vec![Rational::zero(); variable_count + 1];
    for &consumer in consumers {
        row[consumer] = &row[consumer] + Rational::one();
    }
    for &producer in producers {
        row[producer] = &row[producer] - Rational::one();
    }
    row
}

fn rational_rref(mut rows: Vec<Vec<Rational>>, variable_count: usize) -> Vec<Vec<Rational>> {
    rows.retain(|row| row.iter().any(|value| !value.is_zero()));
    let mut pivot_row = 0;
    for column in 0..variable_count {
        let Some(selected) = (pivot_row..rows.len()).find(|&row| !rows[row][column].is_zero())
        else {
            continue;
        };
        rows.swap(pivot_row, selected);
        let pivot = rows[pivot_row][column].clone();
        if pivot.numerator() != pivot.denominator() {
            for value in &mut rows[pivot_row][column..] {
                if !value.is_zero() {
                    *value = &*value / &pivot;
                }
            }
        }
        // Earlier columns in this pivot row are zero. Borrow its nonzero suffix
        // once, avoiding a full rational-row clone and zero multiplications for
        // every elimination. Pivot order and exact normalized rows are unchanged.
        let (before, pivot_and_after) = rows.split_at_mut(pivot_row);
        let (normalized_pivot, after) = pivot_and_after.split_first_mut().unwrap();
        let nonzero = normalized_pivot
            .iter()
            .enumerate()
            .skip(column + 1)
            .filter(|(_, value)| !value.is_zero())
            .collect::<Vec<_>>();
        for row in before.iter_mut().chain(after) {
            if row[column].is_zero() {
                continue;
            }
            let factor = std::mem::replace(&mut row[column], Rational::zero());
            let unit_factor = factor.numerator() == factor.denominator();
            for &(index, pivot_value) in &nonzero {
                row[index] = if unit_factor {
                    &row[index] - pivot_value
                } else {
                    &row[index] - &factor * pivot_value
                };
            }
        }
        pivot_row += 1;
        if pivot_row == rows.len() {
            break;
        }
    }
    let inconsistent = rows.iter().any(|row| {
        row[..variable_count].iter().all(Rational::is_zero) && !row[variable_count].is_zero()
    });
    if inconsistent {
        let mut contradiction = vec![Rational::zero(); variable_count + 1];
        contradiction[variable_count] = Rational::one();
        // Every inconsistent affine system has the empty solution set. Once
        // `0 = 1` belongs to the augmented row space, retaining variable-pivot
        // rows would make equivalent contradictions depend on their generating
        // rows because pivots intentionally stop before the RHS column.
        rows.clear();
        rows.push(contradiction);
    } else {
        rows.retain(|row| row[..variable_count].iter().any(|value| !value.is_zero()));
    }
    rows.sort();
    rows.dedup();
    rows
}

#[cfg(test)]
fn reduce_modulo_equalities(
    row: &mut [Rational],
    equalities: &[Vec<Rational>],
    variable_count: usize,
) {
    for equality in equalities {
        let Some(pivot) = equality[..variable_count]
            .iter()
            .position(|value| !value.is_zero())
        else {
            continue;
        };
        if row[pivot].is_zero() {
            continue;
        }
        let factor = row[pivot].clone();
        let unit_factor = factor.numerator() == factor.denominator();
        for (value, equality_value) in row.iter_mut().zip(equality) {
            if !equality_value.is_zero() {
                *value = if unit_factor {
                    &*value - equality_value
                } else {
                    &*value - &factor * equality_value
                };
            }
        }
    }
}

fn normalize_positive_scale(row: &mut [Rational]) {
    let denominator_lcm = row
        .iter()
        .filter(|value| !value.is_zero() && !value.denominator().is_one())
        .fold(BigInt::one(), |accumulator, value| {
            accumulator.lcm(value.denominator())
        });
    let integers = row
        .iter()
        .enumerate()
        .filter(|(_, value)| !value.is_zero())
        .map(|(index, value)| {
            (
                index,
                value.numerator() * (&denominator_lcm / value.denominator()),
            )
        })
        .collect::<Vec<_>>();
    let gcd = integers
        .iter()
        .fold(BigInt::zero(), |accumulator, (_, value)| {
            if accumulator.is_one() {
                accumulator
            } else {
                accumulator.gcd(&value.abs())
            }
        });
    if gcd.is_zero() {
        return;
    }
    for (index, integer) in integers {
        row[index] = Rational::from(if gcd.is_one() {
            integer
        } else {
            integer / &gcd
        });
    }
}

fn canonical_producer_ports(input_count: usize, nodes: &[PhysicalNode]) -> Vec<ProducerPortRef> {
    let mut ports = (0..input_count)
        .map(|index| {
            ProducerPortRef::Input(InputTerminalIndex(
                u32::try_from(index).expect("input count must fit public index"),
            ))
        })
        .collect::<Vec<_>>();
    for node in nodes {
        ports.extend(
            (0..node.node_type.output_port_count()).map(|port| ProducerPortRef::Node {
                node: node.id,
                port,
            }),
        );
    }
    ports.sort();
    ports
}

fn canonical_consumer_ports(
    output_count: usize,
    discard_count: u32,
    nodes: &[PhysicalNode],
) -> Vec<ConsumerPortRef> {
    let mut ports = (0..output_count)
        .map(|index| {
            ConsumerPortRef::Output(OutputTerminalIndex(
                u32::try_from(index).expect("output count must fit public index"),
            ))
        })
        .collect::<Vec<_>>();
    ports.extend(
        (0..discard_count).map(|index| ConsumerPortRef::Discard(DiscardTerminalIndex(index))),
    );
    for node in nodes {
        ports.extend(
            (0..node.node_type.input_port_count()).map(|port| ConsumerPortRef::Node {
                node: node.id,
                port,
            }),
        );
    }
    ports.sort();
    ports
}

fn write_rates(bytes: &mut Vec<u8>, rates: &[Rational]) {
    write_len(bytes, rates.len());
    for rate in rates {
        write_rational(bytes, rate);
    }
}

fn write_profile(bytes: &mut Vec<u8>, profile: NodeProfile) {
    write_u32(bytes, profile.splitter2);
    write_u32(bytes, profile.splitter3);
    write_u32(bytes, profile.merger2);
    write_u32(bytes, profile.merger3);
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
    bytes.extend_from_slice(
        &u64::try_from(length)
            .expect("canonical encoding length must fit u64")
            .to_be_bytes(),
    );
}

fn write_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

#[cfg(test)]
mod tests {
    use crate::topology::TopologyState;

    use super::*;
    use crate::propagation::PropagationState;
    use solver_reference::canonicalize_graph;

    fn rational(value: &str) -> Rational {
        value.parse().unwrap()
    }

    fn problem(inputs: &[&str], outputs: &[&str]) -> Problem {
        Problem {
            inputs: inputs.iter().map(|value| rational(value)).collect(),
            outputs: outputs.iter().map(|value| rational(value)).collect(),
            max_link_rate: 100.into(),
        }
    }

    fn node(id: u32, node_type: NodeType) -> PhysicalNode {
        PhysicalNode {
            id: NodeId(id),
            node_type,
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

    fn link(producer: ProducerPortRef, consumer: ConsumerPortRef, flow: &str) -> PhysicalLink {
        PhysicalLink {
            producer,
            consumer,
            flow: rational(flow),
        }
    }

    fn partial_link(
        producer: ProducerPortRef,
        consumer: ConsumerPortRef,
        flow: Option<&str>,
    ) -> PartialLink {
        PartialLink {
            producer,
            consumer,
            flow: flow.map(rational),
        }
    }

    fn partial(problem: Problem, graph: &PhysicalGraph, remaining: NodeProfile) -> PartialTopology {
        PartialTopology {
            discard_count: discard_count_from_links(&graph.links),
            problem,
            nodes: graph.nodes.clone(),
            links: graph
                .links
                .iter()
                .map(|link| PartialLink {
                    producer: link.producer,
                    consumer: link.consumer,
                    flow: Some(link.flow.clone()),
                })
                .collect(),
            remaining_profile: remaining,
        }
    }

    fn assert_witness_leaf_encoder_matches_reference(problem: &Problem, graph: &PhysicalGraph) {
        let topology = partial(problem.clone(), graph, NodeProfile::default());
        let incidence = IncidenceGraph::build(&topology, None, true);
        let groups = incidence.labeling_groups(&topology);
        let mut encoder = WitnessEncoder::new(&topology, &incidence, &groups);
        let mut seed = 0x1317_a125_u64;
        for _ in 0..128 {
            let mut ranks = vec![None; incidence.base_colors.len()];
            for group in &groups {
                let mut shuffled = group.clone();
                for i in (1..shuffled.len()).rev() {
                    seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                    shuffled.swap(i, usize::try_from(seed % (i as u64 + 1)).unwrap());
                }
                for (rank, vertex) in shuffled.into_iter().enumerate() {
                    ranks[vertex] = Some(u32::try_from(rank).unwrap());
                }
            }
            assert_eq!(
                encoder.encode(&ranks),
                encode_witness(problem, &incidence.relabel(&topology, &ranks))
            );
        }
    }

    fn dense_reference_normalize_positive_scale(row: &mut [Rational]) {
        let denominator_lcm = row.iter().fold(BigInt::one(), |accumulator, value| {
            accumulator.lcm(value.denominator())
        });
        let integers = row
            .iter()
            .map(|value| value.numerator() * (&denominator_lcm / value.denominator()))
            .collect::<Vec<_>>();
        let gcd = integers
            .iter()
            .filter(|value| !value.is_zero())
            .fold(BigInt::zero(), |accumulator, value| {
                accumulator.gcd(&value.abs())
            });
        if gcd.is_zero() {
            return;
        }
        for (value, integer) in row.iter_mut().zip(integers) {
            *value = Rational::from(integer / &gcd);
        }
    }
    fn dense_reference_rational_rref(
        mut rows: Vec<Vec<Rational>>,
        variable_count: usize,
    ) -> Vec<Vec<Rational>> {
        rows.retain(|row| row.iter().any(|value| !value.is_zero()));
        let mut pivot_row = 0;
        for column in 0..variable_count {
            let Some(selected) = (pivot_row..rows.len()).find(|&row| !rows[row][column].is_zero())
            else {
                continue;
            };
            rows.swap(pivot_row, selected);
            let pivot = rows[pivot_row][column].clone();
            for value in &mut rows[pivot_row] {
                *value = &*value / &pivot;
            }
            let normalized_pivot = rows[pivot_row].clone();
            for (row_index, row) in rows.iter_mut().enumerate() {
                if row_index == pivot_row || row[column].is_zero() {
                    continue;
                }
                let factor = row[column].clone();
                for (value, pivot_value) in row.iter_mut().zip(&normalized_pivot) {
                    *value = &*value - &factor * pivot_value;
                }
            }
            pivot_row += 1;
            if pivot_row == rows.len() {
                break;
            }
        }
        let inconsistent = rows.iter().any(|row| {
            row[..variable_count].iter().all(Rational::is_zero) && !row[variable_count].is_zero()
        });
        if inconsistent {
            let mut contradiction = vec![Rational::zero(); variable_count + 1];
            contradiction[variable_count] = Rational::one();
            // Every inconsistent affine system has the empty solution set. Once
            // `0 = 1` belongs to the augmented row space, retaining variable-pivot
            // rows would make equivalent contradictions depend on their generating
            // rows because pivots intentionally stop before the RHS column.
            rows.clear();
            rows.push(contradiction);
        } else {
            rows.retain(|row| row[..variable_count].iter().any(|value| !value.is_zero()));
        }
        rows.sort();
        rows.dedup();
        rows
    }
    fn dense_reference_reduce_modulo_equalities(
        row: &mut [Rational],
        equalities: &[Vec<Rational>],
        variable_count: usize,
    ) {
        for equality in equalities {
            let Some(pivot) = equality[..variable_count]
                .iter()
                .position(|value| !value.is_zero())
            else {
                continue;
            };
            if row[pivot].is_zero() {
                continue;
            }
            let factor = row[pivot].clone();
            for (value, equality_value) in row.iter_mut().zip(equality) {
                *value = &*value - &factor * equality_value;
            }
        }
    }
    fn dense_reference_primitive_inequality_basis(
        capacity: &Rational,
        variable_count: usize,
        equalities: &[Vec<Rational>],
    ) -> Vec<(SemanticInequalityRelation, Vec<Rational>)> {
        let mut rows = Vec::with_capacity(variable_count.saturating_mul(2));
        for variable in 0..variable_count {
            let mut positive = vec![Rational::zero(); variable_count + 1];
            positive[variable] = Rational::from(-1);
            rows.push((SemanticInequalityRelation::LessThan, positive));

            let mut bounded = vec![Rational::zero(); variable_count + 1];
            bounded[variable] = Rational::one();
            bounded[variable_count] = capacity.clone();
            rows.push((SemanticInequalityRelation::LessThanOrEqual, bounded));
        }
        for (_, row) in &mut rows {
            dense_reference_reduce_modulo_equalities(row, equalities, variable_count);
            dense_reference_normalize_positive_scale(row);
        }
        rows.sort();
        rows.dedup();
        rows
    }

    fn read_varint(input: &mut &[u8]) -> usize {
        let mut value = 0;
        let mut shift = 0;
        loop {
            let byte = input[0];
            *input = &input[1..];
            value |= usize::from(byte & 127) << shift;
            if byte & 128 == 0 {
                return value;
            }
            shift += 7;
        }
    }

    fn read_compact_rational(input: &mut &[u8]) -> Rational {
        let tag = input[0];
        *input = &input[1..];
        match tag {
            0 => Rational::zero(),
            1 => Rational::one(),
            2 => Rational::from(-1),
            3 | 4 => {
                let len = read_varint(input);
                let numerator = BigInt::from_signed_bytes_le(&input[..len]);
                *input = &input[len..];
                let denominator = if tag == 4 {
                    let len = read_varint(input);
                    let value = BigInt::from_signed_bytes_le(&input[..len]);
                    *input = &input[len..];
                    value
                } else {
                    BigInt::one()
                };
                Rational::new(numerator, denominator).unwrap()
            }
            _ => panic!("invalid compact rational"),
        }
    }

    fn assert_sparse_row_round_trip(row: &[Rational]) {
        let mut compact = Vec::new();
        write_sparse_row(&mut compact, row);
        let mut input = compact.as_slice();
        let mut decoded = vec![Rational::zero(); row.len()];
        for _ in 0..read_varint(&mut input) {
            let index = read_varint(&mut input);
            decoded[index] = read_compact_rational(&mut input);
        }
        assert!(input.is_empty());
        assert_eq!(decoded, row);
        let mut legacy = Vec::new();
        let mut restored = Vec::new();
        for value in row {
            write_rational(&mut legacy, value);
        }
        for value in &decoded {
            write_rational(&mut restored, value);
        }
        assert_eq!(
            legacy, restored,
            "compact keys retain every old semantic coefficient"
        );
    }

    #[test]
    fn compact_semantics_round_trip_sparse_indices_and_arbitrary_exact_numbers() {
        for width in [0, 1, 128, 300] {
            let mut row = vec![Rational::zero(); width];
            for (i, text) in [
                "1",
                "-1",
                "128",
                "-129",
                "2/3",
                "-100000000000000000000000000000003/17",
            ]
            .iter()
            .enumerate()
            {
                if width != 0 {
                    row[(i * 53) % width] = rational(text);
                }
            }
            assert_sparse_row_round_trip(&row);
        }
        let row = vec![Rational::zero(); 300];
        let mut encoded = Vec::new();
        write_sparse_row(&mut encoded, &row);
        assert_eq!(encoded.len(), 1, "a zero row needs only its nonzero count");
    }

    #[test]
    fn sparse_elimination_matches_dense_reference_exactly() {
        let mut seed = 0x7b19_c21d_u64;
        for variables in 1..=7 {
            for sample in 0..24 {
                let rows = (0..sample % 9)
                    .map(|_| {
                        (0..=variables)
                            .map(|_| {
                                seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                                let numerator = i64::try_from(seed % 13).unwrap() - 6;
                                let denominator = 1 + (seed >> 8) % 7;
                                let mut value: Rational =
                                    format!("{numerator}/{denominator}").parse().unwrap();
                                if sample % 8 == 0 {
                                    value = &value
                                        * &rational("100000000000000000000000000000000000003");
                                }
                                value
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>();
                let expected = dense_reference_rational_rref(rows.clone(), variables);
                let actual = rational_rref(rows, variables);
                assert_eq!(actual, expected, "variables={variables}, sample={sample}");
                for row in &actual {
                    assert_sparse_row_round_trip(row);
                }
                for capacity in ["1/7", "5", "100000000000000000000000000000000000003"] {
                    let capacity = rational(capacity);
                    assert_eq!(
                        primitive_inequality_basis(&capacity, variables, &actual),
                        dense_reference_primitive_inequality_basis(&capacity, variables, &expected)
                    );
                    for (_, row) in primitive_inequality_basis(&capacity, variables, &actual) {
                        assert_sparse_row_round_trip(&row);
                    }
                }
            }
        }
    }

    #[test]
    fn direct_bound_rows_match_general_reduction_for_wide_rref_systems() {
        for variables in [0, 1, 4, 31, 64] {
            for contradiction in [false, true] {
                let mut equations = Vec::new();
                for pivot in (0..variables).step_by(2) {
                    let mut row =
                        assignment_row(variables, pivot, rational("100000000000000000000003/17"));
                    if pivot + 1 < variables {
                        row[pivot + 1] = rational("-2/3");
                    }
                    equations.push(row.clone());
                    equations.push(row); // Redundancy must disappear in RREF.
                }
                if contradiction {
                    equations.push(sum_assignment_row(variables, &[], Rational::one()));
                }
                let basis = rational_rref(equations, variables);
                for capacity in [rational("1/7"), rational("1200")] {
                    let mut expected = Vec::new();
                    for variable in 0..variables {
                        for (relation, sign, rhs) in [
                            (SemanticInequalityRelation::LessThan, -1, Rational::zero()),
                            (
                                SemanticInequalityRelation::LessThanOrEqual,
                                1,
                                capacity.clone(),
                            ),
                        ] {
                            let mut row = vec![Rational::zero(); variables + 1];
                            row[variable] = sign.into();
                            row[variables] = rhs;
                            reduce_modulo_equalities(&mut row, &basis, variables);
                            normalize_positive_scale(&mut row);
                            expected.push((relation, row));
                        }
                    }
                    expected.sort();
                    expected.dedup();
                    assert_eq!(
                        primitive_inequality_basis(&capacity, variables, &basis),
                        expected
                    );
                }
            }
        }
    }

    #[test]
    fn active_cancellation_discards_partial_witness_labeling() {
        use std::{thread, time::Duration};
        let mut problem = problem(
            &["256"],
            &["128", "64", "32", "16", "8", "4", "2", "1", "1"],
        );
        problem.max_link_rate = 1200.into();
        let mut graph = PhysicalGraph {
            nodes: (0..8).map(|i| node(i, NodeType::Splitter2)).collect(),
            links: vec![link(input(0), consumer(0, 0), "256")],
        };
        for i in 0..8 {
            let flow = (128_u32 >> i).to_string();
            graph.links.push(link(producer(i, 0), output(i), &flow));
            graph.links.push(link(
                producer(i, 1),
                if i == 7 {
                    output(8)
                } else {
                    consumer(i + 1, 0)
                },
                &flow,
            ));
        }
        solver_validation::validate_solution(&problem, &graph).unwrap();
        let cancel = AtomicBool::new(false);
        let started = Instant::now();
        thread::scope(|scope| {
            scope.spawn(|| {
                thread::sleep(Duration::from_millis(20));
                cancel.store(true, Ordering::Relaxed);
            });
            assert!(canonicalize_witness_cancellable(&problem, &graph, &cancel).is_none());
        });
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn legacy_no_discard_witness_protocol_is_byte_stable() {
        const LEGACY_HEX: &str = concat!(
            "7361746973666163746f72792d63616e6f6e6963616c2d67726170680001",
            "0000000000000001000000000000000131",
            "0000000000000001000000000000000131",
            "0000000000000000",
            "0000000000000001",
            "0000000000",
            "0000000000",
            "000000000000000131"
        );
        let specification = problem(&["1"], &["1"]);
        let graph = PhysicalGraph {
            nodes: Vec::new(),
            links: vec![link(input(0), output(0), "1")],
        };
        let production = canonicalize_witness(&specification, &graph);
        assert_eq!(
            production.key,
            canonicalize_graph(&specification, &graph).key
        );

        let expected = LEGACY_HEX
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(production.key.as_bytes(), expected);
    }

    #[test]
    fn anonymous_discard_relabeling_is_quotiented_and_consumer_tags_are_injective() {
        let specification = problem(&["1", "1", "1"], &["1"]);
        let left = PhysicalGraph {
            nodes: Vec::new(),
            links: vec![
                link(input(0), output(0), "1"),
                link(input(1), discard(0), "1"),
                link(input(2), discard(1), "1"),
            ],
        };
        let right = PhysicalGraph {
            nodes: Vec::new(),
            links: vec![
                link(input(2), discard(0), "1"),
                link(input(0), discard(1), "1"),
                link(input(1), output(0), "1"),
            ],
        };
        assert_witness_leaf_encoder_matches_reference(&specification, &left);
        let production = canonicalize_witness(&specification, &left);
        assert_eq!(production, canonicalize_witness(&specification, &right));
        assert_eq!(
            production.key,
            canonicalize_graph(&specification, &left).key
        );

        let role_specification = problem(&["1", "1", "2"], &["2"]);
        let nodes = vec![node(0, NodeType::Merger2)];
        let common = vec![
            link(input(0), consumer(0, 0), "1"),
            link(input(1), consumer(0, 1), "1"),
        ];
        let node_to_output = PhysicalGraph {
            nodes: nodes.clone(),
            links: common
                .iter()
                .cloned()
                .chain([
                    link(producer(0, 0), output(0), "2"),
                    link(input(2), discard(0), "2"),
                ])
                .collect(),
        };
        let node_to_discard = PhysicalGraph {
            nodes,
            links: common
                .into_iter()
                .chain([
                    link(producer(0, 0), discard(0), "2"),
                    link(input(2), output(0), "2"),
                ])
                .collect(),
        };
        assert_ne!(
            canonicalize_witness(&role_specification, &node_to_output).key,
            canonicalize_witness(&role_specification, &node_to_discard).key,
            "output tag 0 and discard tag 2 must encode distinct physical roles"
        );
    }

    #[test]
    fn open_discard_ports_share_one_canonical_orbit_and_enter_state_semantics() {
        let topology = PartialTopology {
            problem: problem(&["1", "1", "1"], &["1"]),
            nodes: Vec::new(),
            links: Vec::new(),
            discard_count: 2,
            remaining_profile: NodeProfile::default(),
        };
        assert_eq!(
            canonicalize_open_port(
                &topology,
                OpenPortRef::Consumer(ConsumerPortRef::Discard(DiscardTerminalIndex(0)))
            ),
            canonicalize_open_port(
                &topology,
                OpenPortRef::Consumer(ConsumerPortRef::Discard(DiscardTerminalIndex(1)))
            )
        );

        let mut no_discards = topology.clone();
        no_discards.discard_count = 0;
        assert_ne!(
            canonicalize_state(&topology),
            canonicalize_state(&no_discards),
            "the state key must encode discard variables, bounds, and their sum row"
        );
    }

    #[test]
    fn state_key_ignores_node_terminal_port_and_link_storage_labels() {
        let problem = problem(&["1", "1"], &["1/2", "1/2", "1"]);
        let left = PartialTopology {
            discard_count: 0,
            problem: problem.clone(),
            nodes: vec![node(40, NodeType::Splitter2), node(9, NodeType::Merger2)],
            links: vec![
                partial_link(input(0), consumer(40, 0), Some("1")),
                partial_link(producer(40, 0), output(0), None),
                partial_link(producer(40, 1), output(1), None),
                partial_link(input(1), consumer(9, 0), Some("1")),
                partial_link(producer(9, 0), output(2), Some("1")),
            ],
            remaining_profile: NodeProfile {
                splitter3: 1,
                ..NodeProfile::default()
            },
        };
        let right = PartialTopology {
            discard_count: 0,
            problem,
            nodes: vec![node(700, NodeType::Merger2), node(3, NodeType::Splitter2)],
            links: vec![
                partial_link(producer(700, 0), output(2), Some("1")),
                partial_link(input(0), consumer(700, 1), Some("1")),
                partial_link(producer(3, 0), output(1), None),
                partial_link(input(1), consumer(3, 0), Some("1")),
                partial_link(producer(3, 1), output(0), None),
            ],
            remaining_profile: NodeProfile {
                splitter3: 1,
                ..NodeProfile::default()
            },
        };

        assert_eq!(canonicalize_state(&left), canonicalize_state(&right));
    }

    #[test]
    fn scc_summary_key_canonicalizes_region_and_exact_known_port_values() {
        let specification = problem(&["2"], &["1", "1"]);
        let left = PartialTopology {
            discard_count: 0,
            problem: specification.clone(),
            nodes: vec![node(40, NodeType::Splitter2)],
            links: vec![
                partial_link(input(0), consumer(40, 0), None),
                partial_link(producer(40, 0), output(0), None),
                partial_link(producer(40, 1), output(1), None),
            ],
            remaining_profile: NodeProfile::default(),
        };
        let right = PartialTopology {
            discard_count: 0,
            problem: specification,
            nodes: vec![node(91, NodeType::Splitter2)],
            links: vec![
                partial_link(producer(91, 0), output(1), None),
                partial_link(input(0), consumer(91, 0), None),
                partial_link(producer(91, 1), output(0), None),
            ],
            remaining_profile: NodeProfile::default(),
        };
        let left_region = [NodeId(40)].into_iter().collect();
        let right_region = [NodeId(91)].into_iter().collect();
        let left_known = [(producer(40, 0), rational("1"))].into_iter().collect();
        let right_known = [(producer(91, 1), rational("1"))].into_iter().collect();
        let no_consumers = BTreeMap::new();

        let left_key =
            canonicalize_scc_summary_input(&left, &left_region, &left_known, &no_consumers);
        let right_key =
            canonicalize_scc_summary_input(&right, &right_region, &right_known, &no_consumers);
        assert_eq!(left_key, right_key);

        let left_labeled = canonicalize_scc_summary_input_with_relabeling(
            &left,
            &left_region,
            &left_known,
            &no_consumers,
        );
        let right_labeled = canonicalize_scc_summary_input_with_relabeling(
            &right,
            &right_region,
            &right_known,
            &no_consumers,
        );
        assert_eq!(left_labeled.key, right_labeled.key);
        assert_eq!(
            left_labeled.producer_relabeling[&producer(40, 0)],
            right_labeled.producer_relabeling[&producer(91, 1)],
            "the same winning labeling that produced the key must transport deductions"
        );
        assert_eq!(
            left_labeled.producer_relabeling[&producer(40, 1)],
            right_labeled.producer_relabeling[&producer(91, 0)],
            "the complete relabeling preserves correlations between two deduction variables"
        );

        let changed_known = [(producer(91, 1), rational("3/2"))].into_iter().collect();
        assert_ne!(
            right_key,
            canonicalize_scc_summary_input(&right, &right_region, &changed_known, &no_consumers,)
        );
        assert_ne!(
            left_key,
            canonicalize_scc_summary_input(&left, &BTreeSet::new(), &left_known, &no_consumers,),
            "region membership is part of the semantic key"
        );
    }

    #[test]
    fn exact_known_link_equalities_are_normalized_without_exposing_labels() {
        let specification = problem(&["2"], &["1", "1"]);
        let left = PartialTopology {
            discard_count: 0,
            problem: specification.clone(),
            nodes: vec![node(0, NodeType::Splitter2)],
            links: vec![
                partial_link(input(0), consumer(0, 0), Some("2")),
                partial_link(producer(0, 0), output(0), Some("1")),
                partial_link(producer(0, 1), output(1), Some("1")),
            ],
            remaining_profile: NodeProfile::default(),
        };
        let relabelled = PartialTopology {
            discard_count: 0,
            problem: specification,
            nodes: vec![node(91, NodeType::Splitter2)],
            links: vec![
                partial_link(producer(91, 0), output(1), Some("1")),
                partial_link(input(0), consumer(91, 0), Some("2")),
                partial_link(producer(91, 1), output(0), Some("1")),
            ],
            remaining_profile: NodeProfile::default(),
        };
        assert_eq!(canonicalize_state(&left), canonicalize_state(&relabelled));

        let mut differing_flow = relabelled.clone();
        differing_flow.links[0].flow = Some(rational("1/2"));
        assert_ne!(
            canonicalize_state(&left),
            canonicalize_state(&differing_flow)
        );

        let mut unresolved = relabelled;
        unresolved.links[0].flow = None;
        assert_eq!(canonicalize_state(&left), canonicalize_state(&unresolved));

        let free_internal = PartialTopology {
            discard_count: 0,
            problem: problem(&["1"], &["1"]),
            nodes: vec![node(0, NodeType::Splitter2), node(1, NodeType::Merger2)],
            links: vec![partial_link(
                producer(0, 0),
                ConsumerPortRef::Node {
                    node: NodeId(1),
                    port: 0,
                },
                None,
            )],
            remaining_profile: NodeProfile::default(),
        };
        let mut constrained_internal = free_internal.clone();
        constrained_internal.links[0].flow = Some(rational("1"));
        assert_ne!(
            canonicalize_state(&free_internal),
            canonicalize_state(&constrained_internal),
            "a genuinely additional exact equality must refine the semantic row space"
        );
    }

    #[test]
    fn propagated_link_flow_snapshots_are_canonical_state_facts() {
        let specification = problem(&["2"], &["1", "1"]);
        let left_partial = PartialTopology {
            discard_count: 0,
            problem: specification.clone(),
            nodes: vec![node(0, NodeType::Splitter2)],
            links: vec![
                partial_link(input(0), consumer(0, 0), None),
                partial_link(producer(0, 0), output(0), None),
                partial_link(producer(0, 1), output(1), None),
            ],
            remaining_profile: NodeProfile::default(),
        };
        let right_partial = PartialTopology {
            discard_count: 0,
            problem: specification.clone(),
            nodes: vec![node(0, NodeType::Splitter2)],
            links: vec![
                partial_link(producer(0, 1), output(0), None),
                partial_link(input(0), consumer(0, 0), None),
                partial_link(producer(0, 0), output(1), None),
            ],
            remaining_profile: NodeProfile::default(),
        };
        let left = TopologyState::from_partial_topology(&left_partial).unwrap();
        let right = TopologyState::from_partial_topology(&right_partial).unwrap();
        let left_propagation = PropagationState::new(&left, &specification.max_link_rate).unwrap();
        let right_propagation =
            PropagationState::new(&right, &specification.max_link_rate).unwrap();
        let left_snapshot = left_propagation
            .partial_topology_with_known_link_flows(&left)
            .unwrap();
        let right_snapshot = right_propagation
            .partial_topology_with_known_link_flows(&right)
            .unwrap();

        assert_eq!(
            left_snapshot
                .links
                .iter()
                .map(|link| link.flow.clone())
                .collect::<BTreeSet<_>>(),
            [Some(rational("1")), Some(rational("2"))]
                .into_iter()
                .collect()
        );
        assert_eq!(
            canonicalize_state(&left.partial_topology()),
            canonicalize_state(&left_snapshot)
        );
        assert_eq!(
            canonicalize_state(&left_snapshot),
            canonicalize_state(&right_snapshot)
        );
    }

    #[test]
    fn open_port_known_values_are_rederived_from_the_encoded_primitive_system() {
        let specification = problem(&["2"], &["1", "1"]);
        let prefix = PartialTopology {
            discard_count: 0,
            problem: specification.clone(),
            nodes: vec![node(0, NodeType::Splitter2)],
            links: vec![partial_link(input(0), consumer(0, 0), None)],
            remaining_profile: NodeProfile::default(),
        };
        let first_topology = TopologyState::from_partial_topology(&prefix).unwrap();
        let first = PropagationState::new(&first_topology, &specification.max_link_rate).unwrap();
        for port in [producer(0, 0), producer(0, 1)] {
            let variable = first_topology.producer_ports()[&port].flow_var;
            assert_eq!(first.known_value(variable).unwrap(), Some(rational("1")));
        }

        let first_snapshot = first
            .partial_topology_with_known_link_flows(&first_topology)
            .unwrap();
        let rebuilt_topology = TopologyState::from_partial_topology(&first_snapshot).unwrap();
        let rebuilt =
            PropagationState::new(&rebuilt_topology, &specification.max_link_rate).unwrap();
        for port in [producer(0, 0), producer(0, 1)] {
            let variable = rebuilt_topology.producer_ports()[&port].flow_var;
            assert_eq!(rebuilt.known_value(variable).unwrap(), Some(rational("1")));
        }
        let rebuilt_snapshot = rebuilt
            .partial_topology_with_known_link_flows(&rebuilt_topology)
            .unwrap();
        assert_eq!(
            canonicalize_state(&first_snapshot),
            canonicalize_state(&rebuilt_snapshot)
        );
    }

    #[test]
    fn encoded_topology_determines_every_primitive_milestone_four_equation() {
        let base = PartialTopology {
            discard_count: 0,
            problem: problem(&["1", "2"], &["3"]),
            nodes: vec![node(0, NodeType::Merger2)],
            links: vec![partial_link(input(0), consumer(0, 0), None)],
            remaining_profile: NodeProfile::default(),
        };

        // External assignments are determined by a terminal's exact rate and
        // association. Unequal-rate terminals cannot be exchanged canonically.
        let mut different_external_association = base.clone();
        different_external_association.links[0].producer = input(1);
        assert_ne!(
            canonicalize_state(&base),
            canonicalize_state(&different_external_association)
        );

        // Operator rows are selected solely by node type and its canonical port
        // roles. Changing the type changes the canonical state while preserving
        // the occupied input endpoint.
        let mut different_operator_equation = base.clone();
        different_operator_equation.nodes[0].node_type = NodeType::Merger3;
        assert_ne!(
            canonicalize_state(&base),
            canonicalize_state(&different_operator_equation)
        );

        // A physical link contributes exactly the equality between its two
        // encoded endpoint variables. Moving an endpoint to a nonautomorphic
        // port changes the key.
        let mut different_link_equality = base.clone();
        different_link_equality.links[0].consumer = output(0);
        assert_ne!(
            canonicalize_state(&base),
            canonicalize_state(&different_link_equality)
        );
    }

    #[test]
    fn partial_key_distinguishes_open_ports_and_remaining_inventory() {
        let problem = problem(&["1"], &["1/2", "1/2"]);
        let open = PartialTopology {
            discard_count: 0,
            problem: problem.clone(),
            nodes: vec![node(0, NodeType::Splitter2)],
            links: vec![partial_link(input(0), consumer(0, 0), None)],
            remaining_profile: NodeProfile::default(),
        };
        let less_open = PartialTopology {
            discard_count: 0,
            problem: problem.clone(),
            nodes: open.nodes.clone(),
            links: vec![
                partial_link(input(0), consumer(0, 0), None),
                partial_link(producer(0, 0), output(0), None),
            ],
            remaining_profile: NodeProfile::default(),
        };
        let remaining = PartialTopology {
            discard_count: 0,
            problem,
            nodes: open.nodes.clone(),
            links: open.links.clone(),
            remaining_profile: NodeProfile {
                merger2: 1,
                ..NodeProfile::default()
            },
        };

        assert_ne!(canonicalize_state(&open), canonicalize_state(&less_open));
        assert_ne!(canonicalize_state(&open), canonicalize_state(&remaining));
    }

    #[test]
    fn marked_link_orbits_survive_full_relabeling_and_link_reordering() {
        let problem = problem(&["1"], &["1"]);
        let left = PartialTopology {
            discard_count: 0,
            problem: problem.clone(),
            nodes: vec![node(0, NodeType::Splitter2), node(1, NodeType::Merger2)],
            links: vec![
                partial_link(input(0), consumer(0, 0), None),
                partial_link(producer(0, 0), consumer(1, 0), None),
                partial_link(producer(0, 1), consumer(1, 1), None),
                partial_link(producer(1, 0), output(0), None),
            ],
            remaining_profile: NodeProfile::default(),
        };
        let right = PartialTopology {
            discard_count: 0,
            problem,
            nodes: vec![node(70, NodeType::Merger2), node(30, NodeType::Splitter2)],
            links: vec![
                partial_link(producer(70, 0), output(0), None),
                partial_link(producer(30, 1), consumer(70, 0), None),
                partial_link(input(0), consumer(30, 0), None),
                partial_link(producer(30, 0), consumer(70, 1), None),
            ],
            remaining_profile: NodeProfile::default(),
        };

        assert_eq!(
            canonicalize_marked_link(&left, 1),
            canonicalize_marked_link(&right, 1)
        );
        assert_eq!(
            canonicalize_marked_link(&left, 2),
            canonicalize_marked_link(&right, 3)
        );
    }

    #[test]
    fn marked_open_port_keys_survive_three_node_terminal_and_port_relabeling() {
        let problem = problem(&["1", "1", "3", "5"], &["10"]);
        let left = PartialTopology {
            discard_count: 0,
            problem: problem.clone(),
            nodes: vec![
                node(0, NodeType::Merger2),
                node(1, NodeType::Merger2),
                node(2, NodeType::Merger2),
            ],
            links: vec![
                partial_link(input(0), consumer(0, 0), None),
                partial_link(input(1), consumer(0, 1), None),
                partial_link(input(2), consumer(1, 0), None),
                partial_link(input(3), consumer(2, 0), None),
                partial_link(producer(0, 0), output(0), None),
            ],
            remaining_profile: NodeProfile::default(),
        };
        let right = PartialTopology {
            discard_count: 0,
            problem,
            nodes: vec![
                node(0, NodeType::Merger2),
                node(1, NodeType::Merger2),
                node(2, NodeType::Merger2),
            ],
            links: vec![
                partial_link(producer(0, 0), output(0), None),
                partial_link(input(3), consumer(1, 0), None),
                partial_link(input(2), consumer(2, 1), None),
                partial_link(input(1), consumer(0, 1), None),
                partial_link(input(0), consumer(0, 0), None),
            ],
            remaining_profile: NodeProfile::default(),
        };
        let left_rate_three = canonicalize_open_port(&left, OpenPortRef::Consumer(consumer(1, 1)));
        let left_rate_five = canonicalize_open_port(&left, OpenPortRef::Consumer(consumer(2, 1)));

        assert_ne!(left_rate_three, left_rate_five);
        assert_eq!(
            left_rate_three,
            canonicalize_open_port(&right, OpenPortRef::Consumer(consumer(2, 0)))
        );
        assert_eq!(
            left_rate_five,
            canonicalize_open_port(&right, OpenPortRef::Consumer(consumer(1, 1)))
        );
    }

    #[test]
    fn production_witness_protocol_matches_reference_on_many_tiny_graphs() {
        let direct_problem = problem(&["1", "1"], &["1", "1"]);
        let direct = PhysicalGraph {
            nodes: Vec::new(),
            links: vec![
                link(input(1), output(0), "1"),
                link(input(0), output(1), "1"),
            ],
        };
        let unit_problem = problem(&["1"], &["1"]);
        let parallel = PhysicalGraph {
            nodes: vec![node(17, NodeType::Splitter2), node(4, NodeType::Merger2)],
            links: vec![
                link(producer(4, 0), output(0), "1"),
                link(producer(17, 1), consumer(4, 0), "1/2"),
                link(input(0), consumer(17, 0), "1"),
                link(producer(17, 0), consumer(4, 1), "1/2"),
            ],
        };
        let feedback = PhysicalGraph {
            nodes: vec![node(17, NodeType::Splitter2), node(4, NodeType::Merger2)],
            links: vec![
                link(input(0), consumer(4, 0), "1"),
                link(producer(17, 1), consumer(4, 1), "1"),
                link(producer(4, 0), consumer(17, 0), "2"),
                link(producer(17, 0), output(0), "1"),
            ],
        };
        let tree_problem = problem(&["1"], &["1/2", "1/4", "1/4"]);
        let tree = PhysicalGraph {
            nodes: vec![node(91, NodeType::Splitter2), node(7, NodeType::Splitter2)],
            links: vec![
                link(input(0), consumer(91, 0), "1"),
                link(producer(91, 0), consumer(7, 0), "1/2"),
                link(producer(91, 1), output(0), "1/2"),
                link(producer(7, 0), output(1), "1/4"),
                link(producer(7, 1), output(2), "1/4"),
            ],
        };

        let ternary_problem = problem(&["3"], &["3"]);
        let ternary = PhysicalGraph {
            nodes: vec![node(29, NodeType::Splitter3), node(8, NodeType::Merger3)],
            links: vec![
                link(input(0), consumer(29, 0), "3"),
                link(producer(29, 0), consumer(8, 2), "1"),
                link(producer(29, 1), consumer(8, 0), "1"),
                link(producer(29, 2), consumer(8, 1), "1"),
                link(producer(8, 0), output(0), "3"),
            ],
        };
        for (problem, graph) in [
            (&direct_problem, &direct),
            (&unit_problem, &parallel),
            (&unit_problem, &feedback),
            (&tree_problem, &tree),
            (&ternary_problem, &ternary),
        ] {
            assert_witness_leaf_encoder_matches_reference(problem, graph);
            let production = canonicalize_witness(problem, graph);
            let reference = solver_reference::canonicalize_graph(problem, graph);
            assert_eq!(
                production.key, reference.key,
                "production graph: {:#?}\nreference graph: {:#?}",
                production.graph, reference.graph
            );
            assert_eq!(production.graph, reference.graph);
        }
    }

    #[test]
    fn production_and_reference_agree_across_tiny_label_and_port_variants() {
        let problem = problem(&["1"], &["1"]);
        for splitter_id in [0, 5, 99] {
            for merger_id in [1, 7, 200] {
                if splitter_id == merger_id {
                    continue;
                }
                for swap in [false, true] {
                    let first = u8::from(swap);
                    let second = u8::from(!swap);
                    let graph = PhysicalGraph {
                        nodes: vec![
                            node(merger_id, NodeType::Merger2),
                            node(splitter_id, NodeType::Splitter2),
                        ],
                        links: vec![
                            link(input(0), consumer(splitter_id, 0), "1"),
                            link(
                                producer(splitter_id, first),
                                consumer(merger_id, second),
                                "1/2",
                            ),
                            link(
                                producer(splitter_id, second),
                                consumer(merger_id, first),
                                "1/2",
                            ),
                            link(producer(merger_id, 0), output(0), "1"),
                        ],
                    };
                    let production = canonicalize_witness(&problem, &graph);
                    let reference = solver_reference::canonicalize_graph(&problem, &graph);
                    assert_eq!(production.key, reference.key);
                    assert_eq!(production.graph, reference.graph);
                }
            }
        }
    }

    #[test]
    fn analytic_witness_port_minimum_matches_reference_with_protocol_tags_and_flows() {
        let surplus_problem = problem(&["3"], &["2"]);
        let surplus = PhysicalGraph {
            nodes: vec![node(9, NodeType::Splitter3), node(4, NodeType::Merger2)],
            links: vec![
                link(producer(9, 0), discard(0), "1"),
                link(producer(4, 0), output(0), "2"),
                link(producer(9, 2), consumer(4, 0), "1"),
                link(input(0), consumer(9, 0), "3"),
                link(producer(9, 1), consumer(4, 1), "1"),
            ],
        };
        let unequal_problem = problem(&["6"], &["6"]);
        let unequal_parallel = PhysicalGraph {
            nodes: vec![node(29, NodeType::Splitter3), node(8, NodeType::Merger3)],
            links: vec![
                link(producer(29, 0), consumer(8, 2), "3"),
                link(input(0), consumer(29, 0), "6"),
                link(producer(29, 1), consumer(8, 0), "1"),
                link(producer(8, 0), output(0), "6"),
                link(producer(29, 2), consumer(8, 1), "2"),
            ],
        };

        for (problem, graph) in [
            (&surplus_problem, &surplus),
            (&unequal_problem, &unequal_parallel),
        ] {
            let production = canonicalize_witness(problem, graph);
            let reference = canonicalize_graph(problem, graph);
            assert_eq!(production.key, reference.key);
            assert_eq!(production.graph, reference.graph);
        }
    }

    #[test]
    fn full_witness_flow_changes_the_authoritative_key() {
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
            canonicalize_witness(&problem, &one).key,
            canonicalize_witness(&problem, &half).key
        );
        assert_eq!(
            canonicalize_effective_layout(&problem, &one),
            canonicalize_effective_layout(&problem, &half),
            "display dedupe deliberately ignores the chosen feasible flow witness"
        );
    }

    #[test]
    fn complete_graph_can_be_snapshotted_without_changing_state_semantics() {
        let problem = problem(&["1"], &["1"]);
        let graph = PhysicalGraph {
            nodes: Vec::new(),
            links: vec![link(input(0), output(0), "1")],
        };
        let snapshot = partial(problem, &graph, NodeProfile::default());
        assert_eq!(snapshot.links[0].flow, Some(rational("1")));
        assert!(!canonicalize_state(&snapshot).as_bytes().is_empty());
    }

    #[test]
    fn exact_rref_ignores_row_order_scaling_and_redundancy() {
        let expected = vec![vec![rational("1"), rational("2"), rational("3")]];
        let left = rational_rref(
            vec![
                vec![rational("2"), rational("4"), rational("6")],
                vec![rational("-3"), rational("-6"), rational("-9")],
                vec![rational("1"), rational("2"), rational("3")],
            ],
            2,
        );
        let right = rational_rref(
            vec![
                vec![rational("1"), rational("2"), rational("3")],
                vec![
                    rational("2000000000000000000000000000000"),
                    rational("4000000000000000000000000000000"),
                    rational("6000000000000000000000000000000"),
                ],
            ],
            2,
        );
        assert_eq!(left, expected);
        assert_eq!(right, expected);
    }

    #[test]
    fn affine_rref_preserves_rhs_signs_and_collapses_all_contradictions() {
        let solved = rational_rref(
            vec![
                vec![rational("1"), rational("-1"), rational("1")],
                vec![rational("0"), rational("1"), rational("2")],
            ],
            2,
        );
        assert_eq!(
            solved,
            vec![
                vec![rational("0"), rational("1"), rational("2")],
                vec![rational("1"), rational("0"), rational("3")],
            ]
        );

        let contradiction = vec![vec![rational("0"), rational("1")]];
        assert_eq!(
            rational_rref(
                vec![
                    vec![rational("1"), rational("0")],
                    vec![rational("0"), rational("1")],
                ],
                1,
            ),
            contradiction
        );
        assert_eq!(
            rational_rref(
                vec![
                    vec![rational("1"), rational("1")],
                    vec![rational("0"), rational("1")],
                ],
                1,
            ),
            contradiction
        );
    }

    #[test]
    fn reduced_physical_bounds_preserve_strictness_direction_and_exact_boundaries() {
        let capacity = rational("5");
        let at_capacity = rational_rref(vec![assignment_row(1, 0, capacity.clone())], 1);
        assert_eq!(
            primitive_inequality_basis(&capacity, 1, &at_capacity),
            vec![
                (
                    SemanticInequalityRelation::LessThan,
                    vec![rational("0"), rational("1")],
                ),
                (
                    SemanticInequalityRelation::LessThanOrEqual,
                    vec![rational("0"), rational("0")],
                ),
            ]
        );

        let at_zero = rational_rref(vec![assignment_row(1, 0, Rational::zero())], 1);
        let zero_bounds = primitive_inequality_basis(&capacity, 1, &at_zero);
        assert!(zero_bounds.contains(&(
            SemanticInequalityRelation::LessThan,
            vec![rational("0"), rational("0")],
        )));
        let above = rational_rref(vec![assignment_row(1, 0, rational("6"))], 1);
        assert!(primitive_inequality_basis(&capacity, 1, &above).contains(&(
            SemanticInequalityRelation::LessThanOrEqual,
            vec![rational("0"), rational("-1")],
        )));

        let huge = rational("123456789012345678901234567890/98765432109876543210987654321");
        let huge_basis = rational_rref(vec![assignment_row(1, 0, huge.clone())], 1);
        assert_eq!(huge_basis[0][1], huge);
    }

    #[test]
    fn marked_link_keys_identify_invariant_parallel_link_orbits() {
        let problem = problem(&["1"], &["1"]);
        let topology = PartialTopology {
            discard_count: 0,
            problem,
            nodes: vec![node(8, NodeType::Splitter2), node(2, NodeType::Merger2)],
            links: vec![
                partial_link(input(0), consumer(8, 0), None),
                partial_link(producer(8, 0), consumer(2, 0), None),
                partial_link(producer(8, 1), consumer(2, 1), None),
                partial_link(producer(2, 0), output(0), None),
            ],
            remaining_profile: NodeProfile::default(),
        };

        assert_eq!(
            canonicalize_marked_link(&topology, 1),
            canonicalize_marked_link(&topology, 2)
        );
        let distinct_keys = (0..topology.links.len())
            .map(|link| canonicalize_marked_link(&topology, link))
            .collect::<BTreeSet<_>>();
        assert_eq!(distinct_keys.len(), 3);
    }
}
