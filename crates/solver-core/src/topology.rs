//! Mutable lazy-materialized physical topology with undo-log rollback.

use std::collections::{BTreeMap, BTreeSet};

use solver_api::{
    ConsumerPortRef, DiscardTerminalIndex, InputTerminalIndex, NodeId, NodeProfile, NodeType,
    OutputTerminalIndex, PhysicalNode, Problem, ProducerPortRef, Rational,
};
use thiserror::Error;

use crate::{
    canonical::{
        CanonicalOpenPortKey, MarkedLinkCanonicalKey, PartialLink, PartialTopology,
        canonicalize_marked_link, canonicalize_open_port,
    },
    components::Component,
    problem::NormalizedProblem,
};

/// Stable graph-local physical link identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LinkId(pub u32);

/// Stable structural decision identifier used by proof provenance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DecisionId(pub u64);

/// Exact-flow variable attached to one explicit physical port.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FlowVarId(pub u32);

/// Direction of a physical port.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Direction {
    Producer,
    Consumer,
}

/// Local physical-port symmetry class.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PortClass {
    External,
    Discard,
    Unique,
    SplitterOutputs,
    MergerInputs,
}

/// Owner of an explicit physical port.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PortOwner {
    Input(InputTerminalIndex),
    Output(OutputTerminalIndex),
    Discard(DiscardTerminalIndex),
    Node(NodeId),
}

/// Compact explicit port state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Port {
    pub owner: PortOwner,
    pub direction: Direction,
    pub symmetry_class: PortClass,
    pub connection: Option<LinkId>,
    pub flow_var: FlowVarId,
    pub known_flow: Option<Rational>,
}

/// One physical link in the mutable partial topology.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub id: LinkId,
    pub decision_id: DecisionId,
    pub producer: ProducerPortRef,
    pub consumer: ConsumerPortRef,
}

/// Unmaterialized inventory of a fixed node-type profile.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RemainingProfile {
    pub splitter2: u32,
    pub splitter3: u32,
    pub merger2: u32,
    pub merger3: u32,
}

impl RemainingProfile {
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.splitter2 == 0 && self.splitter3 == 0 && self.merger2 == 0 && self.merger3 == 0
    }

    #[must_use]
    pub const fn as_node_profile(self) -> NodeProfile {
        NodeProfile {
            splitter2: self.splitter2,
            splitter3: self.splitter3,
            merger2: self.merger2,
            merger3: self.merger3,
        }
    }

    fn available(self, node_type: NodeType) -> u32 {
        match node_type {
            NodeType::Splitter2 => self.splitter2,
            NodeType::Splitter3 => self.splitter3,
            NodeType::Merger2 => self.merger2,
            NodeType::Merger3 => self.merger3,
        }
    }

    fn take(&mut self, node_type: NodeType) -> Result<(), TopologyError> {
        let count = match node_type {
            NodeType::Splitter2 => &mut self.splitter2,
            NodeType::Splitter3 => &mut self.splitter3,
            NodeType::Merger2 => &mut self.merger2,
            NodeType::Merger3 => &mut self.merger3,
        };
        *count = count
            .checked_sub(1)
            .ok_or(TopologyError::NodeTypeExhausted(node_type))?;
        Ok(())
    }

    fn give_back(&mut self, node_type: NodeType) {
        let count = match node_type {
            NodeType::Splitter2 => &mut self.splitter2,
            NodeType::Splitter3 => &mut self.splitter3,
            NodeType::Merger2 => &mut self.merger2,
            NodeType::Merger3 => &mut self.merger3,
        };
        *count = count
            .checked_add(1)
            .expect("rollback restores a prior u32 count");
    }
}

impl From<NodeProfile> for RemainingProfile {
    fn from(profile: NodeProfile) -> Self {
        Self {
            splitter2: profile.splitter2,
            splitter3: profile.splitter3,
            merger2: profile.merger2,
            merger3: profile.merger3,
        }
    }
}

/// Existing open port selected by deterministic MRV ordering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OpenPortRef {
    Producer(ProducerPortRef),
    Consumer(ConsumerPortRef),
}

/// Canonical local open-port orbit and its exact legal partner count.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenPortOrbit {
    pub representative: OpenPortRef,
    pub legal_partner_count: usize,
    pub has_known_flow: bool,
    pub is_external: bool,
    pub symmetry_class: PortClass,
    /// Isomorphism-invariant identity used for the final MRV tie-break.
    pub canonical_key: CanonicalOpenPortKey,
}

/// Producer endpoint selected by one topology decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProducerChoice {
    Existing(ProducerPortRef),
    NewNode { node_type: NodeType, port: u8 },
}

/// Consumer endpoint selected by one topology decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConsumerChoice {
    Existing(ConsumerPortRef),
    NewNode { node_type: NodeType, port: u8 },
}

/// One legal structural link decision. At most one endpoint may own a new node.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TopologyDecision {
    pub producer: ProducerChoice,
    pub consumer: ConsumerChoice,
}

/// One canonical component boundary attached to the currently selected frontier.
///
/// A macro initially makes exactly one parent/component attachment. All other
/// declared component boundaries remain ordinary open physical ports, so a
/// later link may place the expanded subsystem inside a larger graph SCC.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ComponentAttachment {
    ExistingProducerToInput {
        producer: ProducerPortRef,
        component_input: usize,
    },
    OutputToExistingConsumer {
        component_output: usize,
        consumer: ConsumerPortRef,
    },
}

/// Exact physical footprint appended by one component macro transition.
///
/// Boundary vectors and internal-link indices use the component's canonical
/// coordinate order. They are therefore safe for transactional R/T/K
/// registration without pairing unrelated sorted collections.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppliedComponentBatch {
    pub node_start: usize,
    pub node_count: usize,
    pub link_start: usize,
    pub link_count: usize,
    pub internal_link_indices: Vec<usize>,
    pub anchor_link_index: usize,
    pub boundary_inputs: Vec<ConsumerPortRef>,
    pub boundary_outputs: Vec<ProducerPortRef>,
    pub consumed_profile: NodeProfile,
    pub internal_link_count: u32,
}

/// One rollback-tracked frozen component instance in the current physical state.
///
/// The physical nodes and links remain flattened for canonicalization and final
/// witness reconstruction. Dynamic SCC algebra may instead contract this exact
/// node set to one quotient vertex and use the component's immutable projected
/// `R/T/K` contract. Later links may attach only to `boundary_inputs` and
/// `boundary_outputs`, including feedback through arbitrary outside structure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SealedMacroInstance {
    component: Component,
    nodes: Vec<NodeId>,
    internal_link_indices: Vec<usize>,
    boundary_inputs: Vec<ConsumerPortRef>,
    boundary_outputs: Vec<ProducerPortRef>,
}

impl SealedMacroInstance {
    #[must_use]
    pub(crate) const fn component(&self) -> &Component {
        &self.component
    }

    #[must_use]
    pub(crate) fn nodes(&self) -> &[NodeId] {
        &self.nodes
    }

    #[must_use]
    pub(crate) fn internal_link_indices(&self) -> &[usize] {
        &self.internal_link_indices
    }

    #[must_use]
    pub(crate) fn boundary_inputs(&self) -> &[ConsumerPortRef] {
        &self.boundary_inputs
    }

    #[must_use]
    pub(crate) fn boundary_outputs(&self) -> &[ProducerPortRef] {
        &self.boundary_outputs
    }
}

/// Opaque undo-log checkpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Checkpoint {
    undo_len: usize,
    next_decision: u64,
    next_flow_var: u32,
}

/// Invalid mutable topology transition.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum TopologyError {
    #[error("terminal count does not fit the public u32 index type")]
    TerminalCountOverflow,
    #[error("physical node count does not fit NodeId")]
    NodeCountOverflow,
    #[error("physical port count does not fit FlowVarId")]
    FlowVariableOverflow,
    #[error("physical link count does not fit LinkId")]
    LinkCountOverflow,
    #[error("decision identifier overflowed")]
    DecisionOverflow,
    #[error("node type {0:?} has no remaining instance")]
    NodeTypeExhausted(NodeType),
    #[error("a decision may materialize at most one new node")]
    TwoNewNodes,
    #[error("producer port {0:?} does not exist")]
    MissingProducer(ProducerPortRef),
    #[error("consumer port {0:?} does not exist")]
    MissingConsumer(ConsumerPortRef),
    #[error("producer port {0:?} is already connected")]
    ProducerAlreadyConnected(ProducerPortRef),
    #[error("consumer port {0:?} is already connected")]
    ConsumerAlreadyConnected(ConsumerPortRef),
    #[error("a node cannot connect directly to itself")]
    DirectSelfLink,
    #[error("decision is not legal for the current MRV orbit")]
    NonCanonicalDecision,
    #[error("canonical partial topology nodes are not contiguous and index ordered")]
    NonContiguousNodes,
    #[error("component profile is not available in the remaining fixed profile")]
    ComponentProfileUnavailable,
    #[error("component attachment is not anchored at the selected open-port orbit")]
    NonCanonicalComponentAttachment,
    #[error("component expansion certificate is malformed: {0}")]
    MalformedComponent(&'static str),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Undo {
    Connected {
        producer: ProducerPortRef,
        consumer: ConsumerPortRef,
    },
    Materialized {
        node: PhysicalNode,
    },
    SealedMacroRegistered,
}

/// Mutable fixed-profile topology search state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TopologyState {
    problem: Problem,
    remaining: RemainingProfile,
    nodes: Vec<PhysicalNode>,
    producer_ports: BTreeMap<ProducerPortRef, Port>,
    consumer_ports: BTreeMap<ConsumerPortRef, Port>,
    links: Vec<Link>,
    sealed_macros: Vec<SealedMacroInstance>,
    undo: Vec<Undo>,
    next_decision: u64,
    next_flow_var: u32,
}

impl TopologyState {
    /// Initializes only caller-visible external ports. Profile nodes remain anonymous.
    ///
    /// # Errors
    ///
    /// Returns an error when a terminal count or initial flow-variable index
    /// cannot be represented by the compact public identifier types.
    pub fn new(problem: &NormalizedProblem, profile: NodeProfile) -> Result<Self, TopologyError> {
        Self::new_with_discards(problem, profile, 0)
    }

    /// Initializes a fixed profile with `discard_count` anonymous surplus sinks.
    ///
    /// # Errors
    ///
    /// Returns an error when a terminal or compact flow-variable identifier
    /// cannot be represented.
    pub fn new_with_discards(
        problem: &NormalizedProblem,
        profile: NodeProfile,
        discard_count: u32,
    ) -> Result<Self, TopologyError> {
        let canonical_problem = Problem {
            inputs: problem.inputs.as_slice().to_vec(),
            outputs: problem.outputs.as_slice().to_vec(),
            max_link_rate: problem.max_link_rate.clone(),
        };
        let mut state = Self {
            problem: canonical_problem,
            remaining: profile.into(),
            nodes: Vec::new(),
            producer_ports: BTreeMap::new(),
            consumer_ports: BTreeMap::new(),
            links: Vec::new(),
            sealed_macros: Vec::new(),
            undo: Vec::new(),
            next_decision: 0,
            next_flow_var: 0,
        };
        for (index, rate) in state.problem.inputs.clone().into_iter().enumerate() {
            let index = InputTerminalIndex(
                u32::try_from(index).map_err(|_| TopologyError::TerminalCountOverflow)?,
            );
            let flow_var = state.allocate_flow_var()?;
            state.producer_ports.insert(
                ProducerPortRef::Input(index),
                Port {
                    owner: PortOwner::Input(index),
                    direction: Direction::Producer,
                    symmetry_class: PortClass::External,
                    connection: None,
                    flow_var,
                    known_flow: Some(rate),
                },
            );
        }
        for (index, rate) in state.problem.outputs.clone().into_iter().enumerate() {
            let index = OutputTerminalIndex(
                u32::try_from(index).map_err(|_| TopologyError::TerminalCountOverflow)?,
            );
            let flow_var = state.allocate_flow_var()?;
            state.consumer_ports.insert(
                ConsumerPortRef::Output(index),
                Port {
                    owner: PortOwner::Output(index),
                    direction: Direction::Consumer,
                    symmetry_class: PortClass::External,
                    connection: None,
                    flow_var,
                    known_flow: Some(rate),
                },
            );
        }
        for index in 0..discard_count {
            let index = DiscardTerminalIndex(index);
            let flow_var = state.allocate_flow_var()?;
            state.consumer_ports.insert(
                ConsumerPortRef::Discard(index),
                Port {
                    owner: PortOwner::Discard(index),
                    direction: Direction::Consumer,
                    symmetry_class: PortClass::Discard,
                    connection: None,
                    flow_var,
                    known_flow: None,
                },
            );
        }
        Ok(state)
    }

    /// Reconstructs a canonical partial state for admissible reverse-transition checks.
    ///
    /// Canonicalization supplies contiguous node identifiers. Existing links are
    /// loaded as committed history; the returned undo log starts at that parent.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn from_partial_topology(topology: &PartialTopology) -> Result<Self, TopologyError> {
        let mut state = Self {
            problem: topology.problem.clone(),
            remaining: topology.remaining_profile.into(),
            nodes: Vec::new(),
            producer_ports: BTreeMap::new(),
            consumer_ports: BTreeMap::new(),
            links: Vec::new(),
            sealed_macros: Vec::new(),
            undo: Vec::new(),
            next_decision: 0,
            next_flow_var: 0,
        };
        for (index, rate) in topology.problem.inputs.iter().cloned().enumerate() {
            let index = InputTerminalIndex(
                u32::try_from(index).map_err(|_| TopologyError::TerminalCountOverflow)?,
            );
            let flow_var = state.allocate_flow_var()?;
            state.producer_ports.insert(
                ProducerPortRef::Input(index),
                Port {
                    owner: PortOwner::Input(index),
                    direction: Direction::Producer,
                    symmetry_class: PortClass::External,
                    connection: None,
                    flow_var,
                    known_flow: Some(rate),
                },
            );
        }
        for (index, rate) in topology.problem.outputs.iter().cloned().enumerate() {
            let index = OutputTerminalIndex(
                u32::try_from(index).map_err(|_| TopologyError::TerminalCountOverflow)?,
            );
            let flow_var = state.allocate_flow_var()?;
            state.consumer_ports.insert(
                ConsumerPortRef::Output(index),
                Port {
                    owner: PortOwner::Output(index),
                    direction: Direction::Consumer,
                    symmetry_class: PortClass::External,
                    connection: None,
                    flow_var,
                    known_flow: Some(rate),
                },
            );
        }
        for index in 0..topology.discard_count {
            let index = DiscardTerminalIndex(index);
            let flow_var = state.allocate_flow_var()?;
            state.consumer_ports.insert(
                ConsumerPortRef::Discard(index),
                Port {
                    owner: PortOwner::Discard(index),
                    direction: Direction::Consumer,
                    symmetry_class: PortClass::Discard,
                    connection: None,
                    flow_var,
                    known_flow: None,
                },
            );
        }
        for (index, node) in topology.nodes.iter().enumerate() {
            if node.id.0 != u32::try_from(index).map_err(|_| TopologyError::NodeCountOverflow)? {
                return Err(TopologyError::NonContiguousNodes);
            }
            for port in 0..node.node_type.output_port_count() {
                let flow_var = state.allocate_flow_var()?;
                state.producer_ports.insert(
                    ProducerPortRef::Node {
                        node: node.id,
                        port,
                    },
                    Port {
                        owner: PortOwner::Node(node.id),
                        direction: Direction::Producer,
                        symmetry_class: match node.node_type {
                            NodeType::Splitter2 | NodeType::Splitter3 => PortClass::SplitterOutputs,
                            NodeType::Merger2 | NodeType::Merger3 => PortClass::Unique,
                        },
                        connection: None,
                        flow_var,
                        known_flow: None,
                    },
                );
            }
            for port in 0..node.node_type.input_port_count() {
                let flow_var = state.allocate_flow_var()?;
                state.consumer_ports.insert(
                    ConsumerPortRef::Node {
                        node: node.id,
                        port,
                    },
                    Port {
                        owner: PortOwner::Node(node.id),
                        direction: Direction::Consumer,
                        symmetry_class: match node.node_type {
                            NodeType::Splitter2 | NodeType::Splitter3 => PortClass::Unique,
                            NodeType::Merger2 | NodeType::Merger3 => PortClass::MergerInputs,
                        },
                        connection: None,
                        flow_var,
                        known_flow: None,
                    },
                );
            }
            state.nodes.push(node.clone());
        }
        for link in &topology.links {
            state.connect(link.producer, link.consumer)?;
        }
        state.undo.clear();
        Ok(state)
    }

    #[must_use]
    pub fn checkpoint(&self) -> Checkpoint {
        Checkpoint {
            undo_len: self.undo.len(),
            next_decision: self.next_decision,
            next_flow_var: self.next_flow_var,
        }
    }

    /// Restores exactly the semantic state at `checkpoint`.
    ///
    /// # Panics
    ///
    /// Panics when the checkpoint was taken from a later state or a different
    /// topology state.
    pub fn rollback(&mut self, checkpoint: Checkpoint) {
        assert!(
            checkpoint.undo_len <= self.undo.len(),
            "invalid rollback checkpoint"
        );
        while self.undo.len() > checkpoint.undo_len {
            match self.undo.pop().expect("checked nonempty undo log") {
                Undo::Connected { producer, consumer } => {
                    let link = self.links.pop().expect("connection undo owns last link");
                    debug_assert_eq!(link.producer, producer);
                    debug_assert_eq!(link.consumer, consumer);
                    self.producer_ports
                        .get_mut(&producer)
                        .expect("connected producer still exists")
                        .connection = None;
                    self.consumer_ports
                        .get_mut(&consumer)
                        .expect("connected consumer still exists")
                        .connection = None;
                }
                Undo::Materialized { node } => {
                    let removed = self
                        .nodes
                        .pop()
                        .expect("materialization undo owns last node");
                    debug_assert_eq!(removed, node);
                    for port in 0..node.node_type.output_port_count() {
                        self.producer_ports.remove(&ProducerPortRef::Node {
                            node: node.id,
                            port,
                        });
                    }
                    for port in 0..node.node_type.input_port_count() {
                        self.consumer_ports.remove(&ConsumerPortRef::Node {
                            node: node.id,
                            port,
                        });
                    }
                    self.remaining.give_back(node.node_type);
                }
                Undo::SealedMacroRegistered => {
                    self.sealed_macros
                        .pop()
                        .expect("sealed-macro undo owns the last instance");
                }
            }
        }
        self.next_decision = checkpoint.next_decision;
        self.next_flow_var = checkpoint.next_flow_var;
    }

    #[must_use]
    pub fn remaining_profile(&self) -> RemainingProfile {
        self.remaining
    }

    #[must_use]
    pub fn nodes(&self) -> &[PhysicalNode] {
        &self.nodes
    }

    #[must_use]
    pub const fn problem(&self) -> &Problem {
        &self.problem
    }

    #[must_use]
    pub fn links(&self) -> &[Link] {
        &self.links
    }

    #[must_use]
    pub fn producer_ports(&self) -> &BTreeMap<ProducerPortRef, Port> {
        &self.producer_ports
    }

    #[must_use]
    pub fn consumer_ports(&self) -> &BTreeMap<ConsumerPortRef, Port> {
        &self.consumer_ports
    }

    /// Borrows immutable component contractions registered on this proof path.
    ///
    /// This metadata is not part of physical canonical identity. Each contract
    /// was certified equivalent to the flattened rows, so primitive construction
    /// of the same physical state has the same completion set.
    #[must_use]
    pub(crate) fn sealed_macro_instances(&self) -> &[SealedMacroInstance] {
        &self.sealed_macros
    }

    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.remaining.is_empty()
            && self
                .producer_ports
                .values()
                .all(|port| port.connection.is_some())
            && self
                .consumer_ports
                .values()
                .all(|port| port.connection.is_some())
    }

    #[must_use]
    /// Copies the current flattened physical state into its immutable canonical DTO.
    ///
    /// # Panics
    ///
    /// Panics only if an in-memory discard-terminal count no longer fits `u32`.
    /// Construction and reconstruction both originate this count from a `u32`,
    /// so reaching that case would indicate internal state corruption.
    pub fn partial_topology(&self) -> PartialTopology {
        PartialTopology {
            problem: self.problem.clone(),
            nodes: self.nodes.clone(),
            links: self
                .links
                .iter()
                .map(|link| PartialLink {
                    producer: link.producer,
                    consumer: link.consumer,
                    flow: None,
                })
                .collect(),
            discard_count: u32::try_from(
                self.consumer_ports
                    .keys()
                    .filter(|port| matches!(port, ConsumerPortRef::Discard(_)))
                    .count(),
            )
            .expect("discard terminal count originated as u32"),
            remaining_profile: self.remaining.as_node_profile(),
        }
    }

    #[must_use]
    pub fn link_index_for(&self, decision_id: DecisionId) -> Option<usize> {
        self.links
            .iter()
            .position(|link| link.decision_id == decision_id)
    }

    /// Chooses deterministic MRV, then known-flow, external, and canonical order.
    #[must_use]
    pub fn selected_open_orbit(&self) -> Option<OpenPortOrbit> {
        let topology = self.partial_topology();
        self.open_representatives()
            .into_iter()
            .map(|representative| {
                let legal_partner_count = self.decisions_for(representative).len();
                let (has_known_flow, is_external, symmetry_class) = match representative {
                    OpenPortRef::Producer(reference) => {
                        let port = &self.producer_ports[&reference];
                        (
                            port.known_flow.is_some(),
                            matches!(port.owner, PortOwner::Input(_)),
                            port.symmetry_class,
                        )
                    }
                    OpenPortRef::Consumer(reference) => {
                        let port = &self.consumer_ports[&reference];
                        (
                            port.known_flow.is_some(),
                            matches!(port.owner, PortOwner::Output(_) | PortOwner::Discard(_)),
                            port.symmetry_class,
                        )
                    }
                };
                OpenPortOrbit {
                    representative,
                    legal_partner_count,
                    has_known_flow,
                    is_external,
                    symmetry_class,
                    canonical_key: canonicalize_open_port(&topology, representative),
                }
            })
            .min_by(|left, right| {
                let left_key = (
                    left.legal_partner_count,
                    !left.has_known_flow,
                    !left.is_external,
                    matches!(left.symmetry_class, PortClass::Unique),
                    &left.canonical_key,
                    left.representative,
                );
                let right_key = (
                    right.legal_partner_count,
                    !right.has_known_flow,
                    !right.is_external,
                    matches!(right.symmetry_class, PortClass::Unique),
                    &right.canonical_key,
                    right.representative,
                );
                left_key.cmp(&right_key)
            })
    }

    /// Lists legal decisions in canonical marked-child order.
    ///
    /// Raw endpoint references appear only after the invariant child key and
    /// can therefore choose a concrete member inside one automorphism orbit
    /// without changing the quotient search order.
    #[must_use]
    pub fn legal_decisions(&mut self) -> Vec<TopologyDecision> {
        self.selected_open_orbit().map_or_else(Vec::new, |orbit| {
            self.keyed_decisions_for(orbit.representative)
                .into_iter()
                .map(|(_, decision)| decision)
                .collect()
        })
    }

    /// Returns the invariant ordered marked-child keys of all legal decisions.
    #[must_use]
    pub fn ordered_decision_child_keys(&mut self) -> Vec<MarkedLinkCanonicalKey> {
        self.selected_open_orbit().map_or_else(Vec::new, |orbit| {
            self.keyed_decisions_for(orbit.representative)
                .into_iter()
                .map(|(key, _)| key)
                .collect()
        })
    }

    /// Applies a member of the current selected open-port orbit.
    ///
    /// # Errors
    ///
    /// Returns an error for a stale/non-MRV decision, exhausted node inventory,
    /// invalid or occupied endpoints, forbidden self-links, or compact-ID
    /// overflow.
    pub fn apply(&mut self, decision: TopologyDecision) -> Result<DecisionId, TopologyError> {
        if !self.legal_decisions().contains(&decision) {
            return Err(TopologyError::NonCanonicalDecision);
        }
        self.apply_legal_decision(decision)
    }

    /// Lists the canonical boundary coordinates that can attach `component`
    /// to the currently selected physical frontier.
    ///
    /// The list is deliberately not quotiented by component automorphisms.
    /// Search removes equivalent macro children by their full physical
    /// [`crate::canonical::StateKey`], which is a stronger and label-invariant
    /// equivalence test.
    #[must_use]
    pub fn component_attachments(&self, component: &Component) -> Vec<ComponentAttachment> {
        let Some(orbit) = self.selected_open_orbit() else {
            return Vec::new();
        };
        match orbit.representative {
            OpenPortRef::Producer(producer) => (0..component.boundary().input_count())
                .map(
                    |component_input| ComponentAttachment::ExistingProducerToInput {
                        producer,
                        component_input,
                    },
                )
                .collect(),
            OpenPortRef::Consumer(consumer) => (0..component.boundary().output_count())
                .map(
                    |component_output| ComponentAttachment::OutputToExistingConsumer {
                        component_output,
                        consumer,
                    },
                )
                .collect(),
        }
    }

    /// Atomically expands one certified component and attaches one boundary to
    /// the selected MRV frontier.
    ///
    /// The flattened physical nodes and internal links are appended in the
    /// component witness's canonical order. Every undeclared component port is
    /// required to be occupied by exactly one internal link; declared but
    /// unattached boundaries stay open. Any error restores the exact pre-batch
    /// state, including compact identifier counters.
    ///
    /// # Errors
    ///
    /// Returns an error for a malformed expansion certificate, unavailable
    /// fixed-profile inventory, stale/non-MRV anchor, occupied endpoint, or
    /// compact identifier overflow.
    pub fn apply_component(
        &mut self,
        component: &Component,
        attachment: ComponentAttachment,
    ) -> Result<AppliedComponentBatch, TopologyError> {
        self.prevalidate_component(component, attachment)?;
        let checkpoint = self.checkpoint();
        let result = self.apply_component_prevalidated(component, attachment);
        if result.is_err() {
            self.rollback(checkpoint);
        }
        result
    }

    fn apply_component_prevalidated(
        &mut self,
        component: &Component,
        attachment: ComponentAttachment,
    ) -> Result<AppliedComponentBatch, TopologyError> {
        let witness = component.canonical_witness();
        let node_start = self.nodes.len();
        let link_start = self.links.len();
        for local in &witness.nodes {
            let materialized = self.materialize(local.node_type)?;
            debug_assert_eq!(
                usize::try_from(materialized.id.0).expect("u32 fits usize"),
                node_start + usize::try_from(local.id.0).expect("u32 fits usize")
            );
        }

        let boundary_inputs = witness
            .boundary_inputs
            .iter()
            .copied()
            .map(|port| remap_component_consumer(port, node_start))
            .collect::<Result<Vec<_>, _>>()?;
        let boundary_outputs = witness
            .boundary_outputs
            .iter()
            .copied()
            .map(|port| remap_component_producer(port, node_start))
            .collect::<Result<Vec<_>, _>>()?;

        let mut internal_link_indices = Vec::with_capacity(witness.internal_links.len());
        for link in &witness.internal_links {
            let producer = remap_component_producer(link.producer, node_start)?;
            let consumer = remap_component_consumer(link.consumer, node_start)?;
            internal_link_indices.push(self.links.len());
            self.connect(producer, consumer)?;
        }

        let anchor_link_index = self.links.len();
        match attachment {
            ComponentAttachment::ExistingProducerToInput {
                producer,
                component_input,
            } => {
                self.connect(producer, boundary_inputs[component_input])?;
            }
            ComponentAttachment::OutputToExistingConsumer {
                component_output,
                consumer,
            } => {
                self.connect(boundary_outputs[component_output], consumer)?;
            }
        }

        let batch = AppliedComponentBatch {
            node_start,
            node_count: witness.nodes.len(),
            link_start,
            link_count: witness.internal_links.len() + 1,
            internal_link_indices,
            anchor_link_index,
            boundary_inputs,
            boundary_outputs,
            consumed_profile: component.profile(),
            internal_link_count: component.internal_link_count(),
        };
        let nodes = self.nodes[node_start..]
            .iter()
            .map(|node| node.id)
            .collect::<Vec<_>>();
        self.sealed_macros.push(SealedMacroInstance {
            component: component.clone(),
            nodes,
            internal_link_indices: batch.internal_link_indices.clone(),
            boundary_inputs: batch.boundary_inputs.clone(),
            boundary_outputs: batch.boundary_outputs.clone(),
        });
        self.undo.push(Undo::SealedMacroRegistered);

        Ok(batch)
    }

    fn prevalidate_component(
        &self,
        component: &Component,
        attachment: ComponentAttachment,
    ) -> Result<(), TopologyError> {
        let witness = component.canonical_witness();
        if !remaining_contains(self.remaining, component.profile()) {
            return Err(TopologyError::ComponentProfileUnavailable);
        }
        validate_component_witness(component)?;
        self.prevalidate_component_capacity(witness)?;

        let selected = self
            .selected_open_orbit()
            .ok_or(TopologyError::NonCanonicalComponentAttachment)?
            .representative;
        match attachment {
            ComponentAttachment::ExistingProducerToInput {
                producer,
                component_input,
            } => {
                if selected != OpenPortRef::Producer(producer)
                    || component_input >= witness.boundary_inputs.len()
                {
                    return Err(TopologyError::NonCanonicalComponentAttachment);
                }
                if self
                    .producer_ports
                    .get(&producer)
                    .ok_or(TopologyError::MissingProducer(producer))?
                    .connection
                    .is_some()
                {
                    return Err(TopologyError::ProducerAlreadyConnected(producer));
                }
            }
            ComponentAttachment::OutputToExistingConsumer {
                component_output,
                consumer,
            } => {
                if selected != OpenPortRef::Consumer(consumer)
                    || component_output >= witness.boundary_outputs.len()
                {
                    return Err(TopologyError::NonCanonicalComponentAttachment);
                }
                if self
                    .consumer_ports
                    .get(&consumer)
                    .ok_or(TopologyError::MissingConsumer(consumer))?
                    .connection
                    .is_some()
                {
                    return Err(TopologyError::ConsumerAlreadyConnected(consumer));
                }
            }
        }
        Ok(())
    }

    fn prevalidate_component_capacity(
        &self,
        witness: &crate::components::CanonicalComponentWitness,
    ) -> Result<(), TopologyError> {
        let node_end = self
            .nodes
            .len()
            .checked_add(witness.nodes.len())
            .ok_or(TopologyError::NodeCountOverflow)?;
        if node_end > 0 {
            u32::try_from(node_end - 1).map_err(|_| TopologyError::NodeCountOverflow)?;
        }
        let new_ports = witness.nodes.iter().try_fold(0_u32, |count, node| {
            count
                .checked_add(u32::from(node.node_type.input_port_count()))
                .and_then(|value| value.checked_add(u32::from(node.node_type.output_port_count())))
                .ok_or(TopologyError::FlowVariableOverflow)
        })?;
        self.next_flow_var
            .checked_add(new_ports)
            .ok_or(TopologyError::FlowVariableOverflow)?;

        let added_links = witness
            .internal_links
            .len()
            .checked_add(1)
            .ok_or(TopologyError::LinkCountOverflow)?;
        let link_end = self
            .links
            .len()
            .checked_add(added_links)
            .ok_or(TopologyError::LinkCountOverflow)?;
        if link_end > 0 {
            u32::try_from(link_end - 1).map_err(|_| TopologyError::LinkCountOverflow)?;
        }
        self.next_decision
            .checked_add(u64::try_from(added_links).map_err(|_| TopologyError::DecisionOverflow)?)
            .ok_or(TopologyError::DecisionOverflow)?;
        Ok(())
    }

    fn apply_legal_decision(
        &mut self,
        decision: TopologyDecision,
    ) -> Result<DecisionId, TopologyError> {
        if matches!(decision.producer, ProducerChoice::NewNode { .. })
            && matches!(decision.consumer, ConsumerChoice::NewNode { .. })
        {
            return Err(TopologyError::TwoNewNodes);
        }
        let producer = match decision.producer {
            ProducerChoice::Existing(reference) => reference,
            ProducerChoice::NewNode { node_type, port } => {
                let node = self.materialize(node_type)?;
                ProducerPortRef::Node {
                    node: node.id,
                    port,
                }
            }
        };
        let consumer = match decision.consumer {
            ConsumerChoice::Existing(reference) => reference,
            ConsumerChoice::NewNode { node_type, port } => {
                let node = self.materialize(node_type)?;
                ConsumerPortRef::Node {
                    node: node.id,
                    port,
                }
            }
        };
        self.connect(producer, consumer)
    }

    fn keyed_decisions_for(
        &mut self,
        anchor: OpenPortRef,
    ) -> Vec<(MarkedLinkCanonicalKey, TopologyDecision)> {
        let mut keyed = Vec::new();
        for decision in self.decisions_for(anchor) {
            let checkpoint = self.checkpoint();
            let key = self
                .apply_legal_decision(decision)
                .ok()
                .and_then(|decision_id| self.link_index_for(decision_id))
                .map(|link| canonicalize_marked_link(&self.partial_topology(), link));
            self.rollback(checkpoint);
            if let Some(key) = key {
                keyed.push((key, decision));
            }
        }
        keyed.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
        keyed.dedup_by(|left, right| left.0 == right.0);
        keyed
    }

    fn allocate_flow_var(&mut self) -> Result<FlowVarId, TopologyError> {
        let id = FlowVarId(self.next_flow_var);
        self.next_flow_var = self
            .next_flow_var
            .checked_add(1)
            .ok_or(TopologyError::FlowVariableOverflow)?;
        Ok(id)
    }

    fn materialize(&mut self, node_type: NodeType) -> Result<PhysicalNode, TopologyError> {
        self.remaining.take(node_type)?;
        let id =
            NodeId(u32::try_from(self.nodes.len()).map_err(|_| TopologyError::NodeCountOverflow)?);
        let node = PhysicalNode { id, node_type };
        for port in 0..node_type.output_port_count() {
            let flow_var = self.allocate_flow_var()?;
            self.producer_ports.insert(
                ProducerPortRef::Node { node: id, port },
                Port {
                    owner: PortOwner::Node(id),
                    direction: Direction::Producer,
                    symmetry_class: match node_type {
                        NodeType::Splitter2 | NodeType::Splitter3 => PortClass::SplitterOutputs,
                        NodeType::Merger2 | NodeType::Merger3 => PortClass::Unique,
                    },
                    connection: None,
                    flow_var,
                    known_flow: None,
                },
            );
        }
        for port in 0..node_type.input_port_count() {
            let flow_var = self.allocate_flow_var()?;
            self.consumer_ports.insert(
                ConsumerPortRef::Node { node: id, port },
                Port {
                    owner: PortOwner::Node(id),
                    direction: Direction::Consumer,
                    symmetry_class: match node_type {
                        NodeType::Splitter2 | NodeType::Splitter3 => PortClass::Unique,
                        NodeType::Merger2 | NodeType::Merger3 => PortClass::MergerInputs,
                    },
                    connection: None,
                    flow_var,
                    known_flow: None,
                },
            );
        }
        self.nodes.push(node.clone());
        self.undo.push(Undo::Materialized { node: node.clone() });
        Ok(node)
    }

    fn connect(
        &mut self,
        producer: ProducerPortRef,
        consumer: ConsumerPortRef,
    ) -> Result<DecisionId, TopologyError> {
        if self
            .producer_ports
            .get(&producer)
            .ok_or(TopologyError::MissingProducer(producer))?
            .connection
            .is_some()
        {
            return Err(TopologyError::ProducerAlreadyConnected(producer));
        }
        if self
            .consumer_ports
            .get(&consumer)
            .ok_or(TopologyError::MissingConsumer(consumer))?
            .connection
            .is_some()
        {
            return Err(TopologyError::ConsumerAlreadyConnected(consumer));
        }
        if direct_self_link(producer, consumer) {
            return Err(TopologyError::DirectSelfLink);
        }
        let id =
            LinkId(u32::try_from(self.links.len()).map_err(|_| TopologyError::LinkCountOverflow)?);
        let decision_id = DecisionId(self.next_decision);
        self.next_decision = self
            .next_decision
            .checked_add(1)
            .ok_or(TopologyError::DecisionOverflow)?;
        self.producer_ports
            .get_mut(&producer)
            .expect("validated")
            .connection = Some(id);
        self.consumer_ports
            .get_mut(&consumer)
            .expect("validated")
            .connection = Some(id);
        self.links.push(Link {
            id,
            decision_id,
            producer,
            consumer,
        });
        self.undo.push(Undo::Connected { producer, consumer });
        Ok(decision_id)
    }

    fn open_representatives(&self) -> Vec<OpenPortRef> {
        let mut result = Vec::new();
        for (&reference, port) in &self.producer_ports {
            if port.connection.is_none() && self.is_producer_representative(reference, port) {
                result.push(OpenPortRef::Producer(reference));
            }
        }
        for (&reference, port) in &self.consumer_ports {
            if port.connection.is_none() && self.is_consumer_representative(reference, port) {
                result.push(OpenPortRef::Consumer(reference));
            }
        }
        result
    }

    /// Unused same-class ports have identical future partner sets. Retaining the
    /// smallest local index removes only an exact physical port permutation.
    /// Equal-rate unused external terminals are also interchangeable: their
    /// fixed equations and all legal partners agree, so swapping them is an
    /// automorphism of every completion.
    fn is_producer_representative(&self, reference: ProducerPortRef, port: &Port) -> bool {
        if let ProducerPortRef::Input(InputTerminalIndex(index)) = reference {
            return !(0..index).any(|earlier| {
                self.producer_ports
                    .get(&ProducerPortRef::Input(InputTerminalIndex(earlier)))
                    .is_some_and(|candidate| {
                        candidate.connection.is_none() && candidate.known_flow == port.known_flow
                    })
            });
        }
        let ProducerPortRef::Node { node, port: index } = reference else {
            return true;
        };
        if port.symmetry_class != PortClass::SplitterOutputs {
            return true;
        }
        !(0..index).any(|earlier| {
            self.producer_ports
                .get(&ProducerPortRef::Node {
                    node,
                    port: earlier,
                })
                .is_some_and(|candidate| candidate.connection.is_none())
        })
    }

    fn is_consumer_representative(&self, reference: ConsumerPortRef, port: &Port) -> bool {
        if let ConsumerPortRef::Output(OutputTerminalIndex(index)) = reference {
            return !(0..index).any(|earlier| {
                self.consumer_ports
                    .get(&ConsumerPortRef::Output(OutputTerminalIndex(earlier)))
                    .is_some_and(|candidate| {
                        candidate.connection.is_none() && candidate.known_flow == port.known_flow
                    })
            });
        }
        if let ConsumerPortRef::Discard(DiscardTerminalIndex(index)) = reference {
            return !(0..index).any(|earlier| {
                self.consumer_ports
                    .get(&ConsumerPortRef::Discard(DiscardTerminalIndex(earlier)))
                    .is_some_and(|candidate| candidate.connection.is_none())
            });
        }
        let ConsumerPortRef::Node { node, port: index } = reference else {
            return true;
        };
        if port.symmetry_class != PortClass::MergerInputs {
            return true;
        }
        !(0..index).any(|earlier| {
            self.consumer_ports
                .get(&ConsumerPortRef::Node {
                    node,
                    port: earlier,
                })
                .is_some_and(|candidate| candidate.connection.is_none())
        })
    }

    fn decisions_for(&self, anchor: OpenPortRef) -> Vec<TopologyDecision> {
        let mut decisions = Vec::new();
        match anchor {
            OpenPortRef::Producer(producer) => {
                for (&consumer, port) in &self.consumer_ports {
                    if port.connection.is_none()
                        && self.is_consumer_representative(consumer, port)
                        && !direct_self_link(producer, consumer)
                        && !self.would_strand_unmaterialized_nodes()
                    {
                        decisions.push(TopologyDecision {
                            producer: ProducerChoice::Existing(producer),
                            consumer: ConsumerChoice::Existing(consumer),
                        });
                    }
                }
                for node_type in NODE_TYPES {
                    if self.remaining.available(node_type) > 0 {
                        decisions.push(TopologyDecision {
                            producer: ProducerChoice::Existing(producer),
                            consumer: ConsumerChoice::NewNode { node_type, port: 0 },
                        });
                    }
                }
            }
            OpenPortRef::Consumer(consumer) => {
                for (&producer, port) in &self.producer_ports {
                    if port.connection.is_none()
                        && self.is_producer_representative(producer, port)
                        && !direct_self_link(producer, consumer)
                        && !self.would_strand_unmaterialized_nodes()
                    {
                        decisions.push(TopologyDecision {
                            producer: ProducerChoice::Existing(producer),
                            consumer: ConsumerChoice::Existing(consumer),
                        });
                    }
                }
                for node_type in NODE_TYPES {
                    if self.remaining.available(node_type) > 0 {
                        decisions.push(TopologyDecision {
                            producer: ProducerChoice::NewNode { node_type, port: 0 },
                            consumer: ConsumerChoice::Existing(consumer),
                        });
                    }
                }
            }
        }
        decisions.sort_unstable();
        decisions.dedup();
        decisions
    }

    /// Consuming the final producer and consumer frontier while nodes remain
    /// cannot be completed: lazy materialization requires attachment to an
    /// existing open port. Eliminating this transition is therefore a direct
    /// impossibility proof, not a heuristic connectivity preference.
    fn would_strand_unmaterialized_nodes(&self) -> bool {
        !self.remaining.is_empty()
            && self
                .producer_ports
                .values()
                .filter(|port| port.connection.is_none())
                .count()
                == 1
            && self
                .consumer_ports
                .values()
                .filter(|port| port.connection.is_none())
                .count()
                == 1
    }
}

const NODE_TYPES: [NodeType; 4] = [
    NodeType::Splitter2,
    NodeType::Splitter3,
    NodeType::Merger2,
    NodeType::Merger3,
];

fn remaining_contains(remaining: RemainingProfile, required: NodeProfile) -> bool {
    remaining.splitter2 >= required.splitter2
        && remaining.splitter3 >= required.splitter3
        && remaining.merger2 >= required.merger2
        && remaining.merger3 >= required.merger3
}

// The certificate checks are deliberately contiguous: every physical port must
// appear exactly once as either an internal endpoint or a declared boundary.
// Splitting this proof across helpers would make that partition harder to audit.
#[allow(clippy::too_many_lines)]
fn validate_component_witness(component: &Component) -> Result<(), TopologyError> {
    let witness = component.canonical_witness();
    if witness.nodes.is_empty() {
        return Err(TopologyError::MalformedComponent(
            "the physical witness has no nodes",
        ));
    }
    if witness.internal_links.len()
        != usize::try_from(component.internal_link_count())
            .map_err(|_| TopologyError::LinkCountOverflow)?
        || component.internal_flow_map().row_count() != witness.internal_links.len()
    {
        return Err(TopologyError::MalformedComponent(
            "internal links do not align with K rows",
        ));
    }
    let input_count = witness.boundary_inputs.len();
    let output_count = witness.boundary_outputs.len();
    if input_count != component.boundary().input_count()
        || output_count != component.boundary().output_count()
        || component.transfer().column_count() != input_count
        || component.transfer().row_count() != output_count
        || component.internal_flow_map().column_count() != input_count
        || component.domain().input_count() != input_count
    {
        return Err(TopologyError::MalformedComponent(
            "boundary and matrix dimensions disagree",
        ));
    }

    let mut observed_profile = NodeProfile::default();
    let mut all_producers = BTreeSet::new();
    let mut all_consumers = BTreeSet::new();
    for (index, node) in witness.nodes.iter().enumerate() {
        if usize::try_from(node.id.0).expect("u32 fits usize") != index {
            return Err(TopologyError::MalformedComponent(
                "local node identifiers are not contiguous",
            ));
        }
        increment_profile(&mut observed_profile, node.node_type)?;
        for port in 0..node.node_type.output_port_count() {
            all_producers.insert(ProducerPortRef::Node {
                node: node.id,
                port,
            });
        }
        for port in 0..node.node_type.input_port_count() {
            all_consumers.insert(ConsumerPortRef::Node {
                node: node.id,
                port,
            });
        }
    }
    if observed_profile != component.profile() {
        return Err(TopologyError::MalformedComponent(
            "witness node profile disagrees with component cost",
        ));
    }

    let mut used_producers = BTreeSet::new();
    let mut used_consumers = BTreeSet::new();
    for (index, link) in witness.internal_links.iter().enumerate() {
        if !all_producers.contains(&link.producer)
            || !all_consumers.contains(&link.consumer)
            || direct_self_link(link.producer, link.consumer)
        {
            return Err(TopologyError::MalformedComponent(
                "internal link has an invalid physical endpoint",
            ));
        }
        if !used_producers.insert(link.producer) || !used_consumers.insert(link.consumer) {
            return Err(TopologyError::MalformedComponent(
                "an internal endpoint is used more than once",
            ));
        }
        let Some(coefficients) = component.internal_flow_map().row(index) else {
            return Err(TopologyError::MalformedComponent(
                "internal link has no matching K row",
            ));
        };
        if coefficients != link.coefficients {
            return Err(TopologyError::MalformedComponent(
                "internal link order does not match K",
            ));
        }
    }
    for &producer in &witness.boundary_outputs {
        if !all_producers.contains(&producer) || !used_producers.insert(producer) {
            return Err(TopologyError::MalformedComponent(
                "boundary output is invalid or occupied internally",
            ));
        }
    }
    for &consumer in &witness.boundary_inputs {
        if !all_consumers.contains(&consumer) || !used_consumers.insert(consumer) {
            return Err(TopologyError::MalformedComponent(
                "boundary input is invalid or occupied internally",
            ));
        }
    }
    if used_producers != all_producers || used_consumers != all_consumers {
        return Err(TopologyError::MalformedComponent(
            "an undeclared physical port is not occupied internally",
        ));
    }
    Ok(())
}

fn increment_profile(profile: &mut NodeProfile, node_type: NodeType) -> Result<(), TopologyError> {
    let count = match node_type {
        NodeType::Splitter2 => &mut profile.splitter2,
        NodeType::Splitter3 => &mut profile.splitter3,
        NodeType::Merger2 => &mut profile.merger2,
        NodeType::Merger3 => &mut profile.merger3,
    };
    *count = count
        .checked_add(1)
        .ok_or(TopologyError::NodeCountOverflow)?;
    Ok(())
}

fn remap_component_producer(
    reference: ProducerPortRef,
    node_start: usize,
) -> Result<ProducerPortRef, TopologyError> {
    let ProducerPortRef::Node { node, port } = reference else {
        return Err(TopologyError::MalformedComponent(
            "component producer endpoint is external",
        ));
    };
    let global = node_start
        .checked_add(usize::try_from(node.0).expect("u32 fits usize"))
        .ok_or(TopologyError::NodeCountOverflow)?;
    Ok(ProducerPortRef::Node {
        node: NodeId(u32::try_from(global).map_err(|_| TopologyError::NodeCountOverflow)?),
        port,
    })
}

fn remap_component_consumer(
    reference: ConsumerPortRef,
    node_start: usize,
) -> Result<ConsumerPortRef, TopologyError> {
    let ConsumerPortRef::Node { node, port } = reference else {
        return Err(TopologyError::MalformedComponent(
            "component consumer endpoint is external",
        ));
    };
    let global = node_start
        .checked_add(usize::try_from(node.0).expect("u32 fits usize"))
        .ok_or(TopologyError::NodeCountOverflow)?;
    Ok(ConsumerPortRef::Node {
        node: NodeId(u32::try_from(global).map_err(|_| TopologyError::NodeCountOverflow)?),
        port,
    })
}

fn direct_self_link(producer: ProducerPortRef, consumer: ConsumerPortRef) -> bool {
    matches!(
        (producer, consumer),
        (
            ProducerPortRef::Node { node: left, .. },
            ConsumerPortRef::Node { node: right, .. }
        ) if left == right
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::{
        Preparation,
        algebra::sparse::Consistency,
        canonical::{canonicalize_state, is_canonical_last_link},
        components::Component,
        prepare_problem,
        scc::{
            DeclaredBoundaryInput, DeclaredBoundaryOutput, FrozenSubsystemAnalysis,
            FrozenSubsystemDeclaration, analyze_frozen_subsystem, detect_affected_sccs,
            summarize_open_scc,
        },
    };

    fn normalized(inputs: &[&str], outputs: &[&str]) -> NormalizedProblem {
        let problem = Problem {
            inputs: inputs.iter().map(|rate| rate.parse().unwrap()).collect(),
            outputs: outputs.iter().map(|rate| rate.parse().unwrap()).collect(),
            max_link_rate: "1000".parse().unwrap(),
        };
        match prepare_problem(&problem).unwrap() {
            Preparation::Prepared(problem) => problem,
            Preparation::GloballyUnsat(proof) => panic!("unexpected proof: {proof:?}"),
        }
    }

    const fn profile(s2: u32, s3: u32, m2: u32, m3: u32) -> NodeProfile {
        NodeProfile {
            splitter2: s2,
            splitter3: s3,
            merger2: m2,
            merger3: m3,
        }
    }

    fn splitter_component(reverse_outputs: bool) -> Component {
        let physical = TopologyState::from_partial_topology(&PartialTopology {
            discard_count: 0,
            problem: Problem {
                inputs: vec!["2".parse().unwrap()],
                outputs: vec!["1".parse().unwrap(), "1".parse().unwrap()],
                max_link_rate: "10".parse().unwrap(),
            },
            nodes: vec![PhysicalNode {
                id: NodeId(0),
                node_type: NodeType::Splitter2,
            }],
            links: Vec::new(),
            remaining_profile: NodeProfile::default(),
        })
        .unwrap();
        let mut boundary_outputs = vec![
            DeclaredBoundaryOutput {
                port: ProducerPortRef::Node {
                    node: NodeId(0),
                    port: 0,
                },
            },
            DeclaredBoundaryOutput {
                port: ProducerPortRef::Node {
                    node: NodeId(0),
                    port: 1,
                },
            },
        ];
        if reverse_outputs {
            boundary_outputs.reverse();
        }
        let analysis = analyze_frozen_subsystem(
            &physical,
            &FrozenSubsystemDeclaration {
                nodes: vec![NodeId(0)],
                boundary_inputs: vec![DeclaredBoundaryInput {
                    port: ConsumerPortRef::Node {
                        node: NodeId(0),
                        port: 0,
                    },
                }],
                boundary_outputs,
            },
        )
        .unwrap();
        let FrozenSubsystemAnalysis::Symbolic(frozen) = analysis else {
            panic!("splitter must have a symbolic frozen contract");
        };
        Component::from_frozen(*frozen).unwrap()
    }

    fn merge_split_component() -> Component {
        let physical = TopologyState::from_partial_topology(&PartialTopology {
            discard_count: 0,
            problem: Problem {
                inputs: vec!["1".parse().unwrap(), "1".parse().unwrap()],
                outputs: vec!["1".parse().unwrap(), "1".parse().unwrap()],
                max_link_rate: "10".parse().unwrap(),
            },
            nodes: vec![
                PhysicalNode {
                    id: NodeId(0),
                    node_type: NodeType::Merger2,
                },
                PhysicalNode {
                    id: NodeId(1),
                    node_type: NodeType::Splitter2,
                },
            ],
            links: vec![PartialLink {
                producer: ProducerPortRef::Node {
                    node: NodeId(0),
                    port: 0,
                },
                consumer: ConsumerPortRef::Node {
                    node: NodeId(1),
                    port: 0,
                },
                flow: None,
            }],
            remaining_profile: NodeProfile::default(),
        })
        .unwrap();
        let analysis = analyze_frozen_subsystem(
            &physical,
            &FrozenSubsystemDeclaration {
                nodes: vec![NodeId(0), NodeId(1)],
                boundary_inputs: vec![
                    DeclaredBoundaryInput {
                        port: ConsumerPortRef::Node {
                            node: NodeId(0),
                            port: 0,
                        },
                    },
                    DeclaredBoundaryInput {
                        port: ConsumerPortRef::Node {
                            node: NodeId(0),
                            port: 1,
                        },
                    },
                ],
                boundary_outputs: vec![
                    DeclaredBoundaryOutput {
                        port: ProducerPortRef::Node {
                            node: NodeId(1),
                            port: 0,
                        },
                    },
                    DeclaredBoundaryOutput {
                        port: ProducerPortRef::Node {
                            node: NodeId(1),
                            port: 1,
                        },
                    },
                ],
            },
        )
        .unwrap();
        let FrozenSubsystemAnalysis::Symbolic(frozen) = analysis else {
            panic!("merge-split component must be uniquely solvable");
        };
        Component::from_frozen(*frozen).unwrap()
    }

    fn parallel_identity_component() -> Component {
        let mut links = Vec::new();
        for (splitter, merger) in [(0_u32, 1_u32), (2, 3)] {
            for port in 0..2 {
                links.push(PartialLink {
                    producer: ProducerPortRef::Node {
                        node: NodeId(splitter),
                        port,
                    },
                    consumer: ConsumerPortRef::Node {
                        node: NodeId(merger),
                        port,
                    },
                    flow: None,
                });
            }
        }
        let physical = TopologyState::from_partial_topology(&PartialTopology {
            discard_count: 0,
            problem: Problem {
                inputs: vec!["1".parse().unwrap(), "1".parse().unwrap()],
                outputs: vec!["1".parse().unwrap(), "1".parse().unwrap()],
                max_link_rate: "10".parse().unwrap(),
            },
            nodes: vec![
                PhysicalNode {
                    id: NodeId(0),
                    node_type: NodeType::Splitter2,
                },
                PhysicalNode {
                    id: NodeId(1),
                    node_type: NodeType::Merger2,
                },
                PhysicalNode {
                    id: NodeId(2),
                    node_type: NodeType::Splitter2,
                },
                PhysicalNode {
                    id: NodeId(3),
                    node_type: NodeType::Merger2,
                },
            ],
            links,
            remaining_profile: NodeProfile::default(),
        })
        .unwrap();
        let analysis = analyze_frozen_subsystem(
            &physical,
            &FrozenSubsystemDeclaration {
                nodes: vec![NodeId(0), NodeId(1), NodeId(2), NodeId(3)],
                boundary_inputs: vec![
                    DeclaredBoundaryInput {
                        port: ConsumerPortRef::Node {
                            node: NodeId(0),
                            port: 0,
                        },
                    },
                    DeclaredBoundaryInput {
                        port: ConsumerPortRef::Node {
                            node: NodeId(2),
                            port: 0,
                        },
                    },
                ],
                boundary_outputs: vec![
                    DeclaredBoundaryOutput {
                        port: ProducerPortRef::Node {
                            node: NodeId(1),
                            port: 0,
                        },
                    },
                    DeclaredBoundaryOutput {
                        port: ProducerPortRef::Node {
                            node: NodeId(3),
                            port: 0,
                        },
                    },
                ],
            },
        )
        .unwrap();
        let FrozenSubsystemAnalysis::Symbolic(frozen) = analysis else {
            panic!("parallel identity component must be uniquely solvable");
        };
        Component::from_frozen(*frozen).unwrap()
    }

    fn feedback_identity_component() -> Component {
        let physical = TopologyState::from_partial_topology(&PartialTopology {
            discard_count: 0,
            problem: Problem {
                inputs: vec!["1".parse().unwrap()],
                outputs: vec!["1".parse().unwrap()],
                max_link_rate: "10".parse().unwrap(),
            },
            nodes: vec![
                PhysicalNode {
                    id: NodeId(0),
                    node_type: NodeType::Splitter2,
                },
                PhysicalNode {
                    id: NodeId(1),
                    node_type: NodeType::Merger2,
                },
            ],
            links: vec![
                PartialLink {
                    producer: ProducerPortRef::Node {
                        node: NodeId(1),
                        port: 0,
                    },
                    consumer: ConsumerPortRef::Node {
                        node: NodeId(0),
                        port: 0,
                    },
                    flow: None,
                },
                PartialLink {
                    producer: ProducerPortRef::Node {
                        node: NodeId(0),
                        port: 0,
                    },
                    consumer: ConsumerPortRef::Node {
                        node: NodeId(1),
                        port: 0,
                    },
                    flow: None,
                },
            ],
            remaining_profile: NodeProfile::default(),
        })
        .unwrap();
        let analysis = analyze_frozen_subsystem(
            &physical,
            &FrozenSubsystemDeclaration {
                nodes: vec![NodeId(0), NodeId(1)],
                boundary_inputs: vec![DeclaredBoundaryInput {
                    port: ConsumerPortRef::Node {
                        node: NodeId(1),
                        port: 1,
                    },
                }],
                boundary_outputs: vec![DeclaredBoundaryOutput {
                    port: ProducerPortRef::Node {
                        node: NodeId(0),
                        port: 1,
                    },
                }],
            },
        )
        .unwrap();
        let FrozenSubsystemAnalysis::Symbolic(frozen) = analysis else {
            panic!("feedback identity component must be uniquely solvable");
        };
        Component::from_frozen(*frozen).unwrap()
    }

    #[test]
    fn root_contains_only_external_ports_and_direct_link_completes() {
        let mut state =
            TopologyState::new(&normalized(&["1"], &["1"]), profile(0, 0, 0, 0)).unwrap();
        assert!(state.nodes().is_empty());
        let decisions = state.legal_decisions();
        assert_eq!(decisions.len(), 1);
        state.apply(decisions[0]).unwrap();
        assert!(state.is_complete());
    }

    #[test]
    fn anonymous_discard_ports_are_quotiented_and_rollback_exactly() {
        let problem = normalized(&["1", "1", "1"], &["1"]);
        assert_eq!(problem.surplus, Rational::from(2));
        let mut state = TopologyState::new_with_discards(&problem, profile(0, 0, 0, 0), 2).unwrap();
        assert_eq!(state.partial_topology().discard_count, 2);
        assert_eq!(
            state
                .consumer_ports()
                .keys()
                .filter(|port| matches!(port, ConsumerPortRef::Discard(_)))
                .count(),
            2
        );
        assert_eq!(
            state
                .open_representatives()
                .into_iter()
                .filter(|port| {
                    matches!(port, OpenPortRef::Consumer(ConsumerPortRef::Discard(_)))
                })
                .count(),
            1,
            "unused anonymous discards have identical completion sets"
        );

        let original = state.clone();
        let checkpoint = state.checkpoint();
        let first = state.legal_decisions()[0];
        state.apply(first).unwrap();
        let discard_decision = state
            .legal_decisions()
            .into_iter()
            .find(|decision| {
                matches!(
                    decision.consumer,
                    ConsumerChoice::Existing(ConsumerPortRef::Discard(_))
                )
            })
            .unwrap();
        state.apply(discard_decision).unwrap();
        state.rollback(checkpoint);
        assert_eq!(state, original);
    }

    #[test]
    fn zero_surplus_root_declares_no_discard_ports() {
        let problem = normalized(&["1"], &["1"]);
        let state = TopologyState::new(&problem, profile(0, 0, 0, 0)).unwrap();
        assert_eq!(problem.surplus, Rational::zero());
        assert_eq!(state.partial_topology().discard_count, 0);
        assert!(
            state
                .consumer_ports()
                .keys()
                .all(|port| !matches!(port, ConsumerPortRef::Discard(_)))
        );
    }

    #[test]
    fn a_new_node_is_materialized_only_by_an_attaching_link() {
        let mut state =
            TopologyState::new(&normalized(&["2"], &["1", "1"]), profile(1, 0, 0, 0)).unwrap();
        assert!(state.nodes().is_empty());
        let decision = state
            .legal_decisions()
            .into_iter()
            .find(|decision| {
                matches!(
                    decision.consumer,
                    ConsumerChoice::NewNode {
                        node_type: NodeType::Splitter2,
                        ..
                    }
                ) || matches!(
                    decision.producer,
                    ProducerChoice::NewNode {
                        node_type: NodeType::Splitter2,
                        ..
                    }
                )
            })
            .unwrap();
        state.apply(decision).unwrap();
        assert_eq!(state.nodes().len(), 1);
        assert!(state.remaining_profile().is_empty());
    }

    #[test]
    fn checkpoint_rollback_restores_exact_semantic_state() {
        let mut state =
            TopologyState::new(&normalized(&["2"], &["1", "1"]), profile(1, 0, 0, 0)).unwrap();
        let original = state.clone();
        let checkpoint = state.checkpoint();
        let decision = state.legal_decisions()[0];
        let first_id = state.apply(decision).unwrap();
        state.rollback(checkpoint);
        assert_eq!(state, original);
        let replayed_id = state.apply(decision).unwrap();
        assert_eq!(
            replayed_id, first_id,
            "decision IDs are stable per proof path"
        );
        state.rollback(checkpoint);
        assert_eq!(state, original);
    }

    #[test]
    fn component_batch_expands_physical_witness_and_rolls_back_exactly() {
        let component = splitter_component(false);
        let mut state =
            TopologyState::new(&normalized(&["2"], &["1", "1"]), profile(1, 0, 0, 0)).unwrap();
        let original = state.clone();
        let checkpoint = state.checkpoint();
        let attachments = state.component_attachments(&component);
        assert!(!attachments.is_empty());
        let batch = state.apply_component(&component, attachments[0]).unwrap();

        assert_eq!(batch.node_start, 0);
        assert_eq!(batch.node_count, 1);
        assert_eq!(batch.link_start, 0);
        assert_eq!(batch.link_count, 1);
        assert!(batch.internal_link_indices.is_empty());
        assert_eq!(batch.anchor_link_index, 0);
        assert_eq!(batch.consumed_profile, profile(1, 0, 0, 0));
        assert_eq!(batch.internal_link_count, 0);
        assert!(state.remaining_profile().is_empty());
        assert_eq!(state.nodes().len(), 1);
        assert_eq!(state.links().len(), 1);
        assert_eq!(state.sealed_macro_instances().len(), 1);
        assert_eq!(state.sealed_macro_instances()[0].nodes(), &[NodeId(0)]);
        let connected_boundaries = batch
            .boundary_inputs
            .iter()
            .filter(|port| state.consumer_ports()[port].connection.is_some())
            .count()
            + batch
                .boundary_outputs
                .iter()
                .filter(|port| state.producer_ports()[port].connection.is_some())
                .count();
        assert_eq!(connected_boundaries, 1);

        state.rollback(checkpoint);
        assert!(state.sealed_macro_instances().is_empty());
        assert_eq!(state, original);
    }

    #[test]
    fn sealed_internal_cycle_is_contracted_and_only_declared_boundaries_remain_open() {
        let component = feedback_identity_component();
        let mut state =
            TopologyState::new(&normalized(&["1"], &["1"]), profile(1, 0, 1, 0)).unwrap();
        let original = state.clone();
        let checkpoint = state.checkpoint();
        let attachment = state.component_attachments(&component)[0];
        let batch = state.apply_component(&component, attachment).unwrap();

        let quotient = detect_affected_sccs(&state, None).unwrap();
        assert_eq!(quotient.regions.len(), 1);
        assert_eq!(quotient.regions[0].nodes.len(), 2);
        assert!(
            !quotient.regions[0].cyclic,
            "the certified internal feedback is not a dynamic quotient cycle"
        );
        let flattened = TopologyState::from_partial_topology(&state.partial_topology()).unwrap();
        assert!(
            detect_affected_sccs(&flattened, None)
                .unwrap()
                .regions
                .iter()
                .any(|region| region.cyclic),
            "the reconstruction still contains the physical internal cycle"
        );

        let declared_inputs = batch
            .boundary_inputs
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let declared_outputs = batch
            .boundary_outputs
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        assert!(state.consumer_ports().iter().all(|(reference, port)| {
            !matches!(reference, ConsumerPortRef::Node { .. })
                || port.connection.is_some()
                || declared_inputs.contains(reference)
        }));
        assert!(state.producer_ports().iter().all(|(reference, port)| {
            !matches!(reference, ProducerPortRef::Node { .. })
                || port.connection.is_some()
                || declared_outputs.contains(reference)
        }));
        let internal = &state.links()[batch.internal_link_indices[0]];
        assert!(matches!(
            state.connect(
                internal.producer,
                ConsumerPortRef::Output(OutputTerminalIndex(0))
            ),
            Err(TopologyError::ProducerAlreadyConnected(_))
        ));

        state.rollback(checkpoint);
        assert_eq!(detect_affected_sccs(&state, None).unwrap().regions.len(), 0);
        assert_eq!(state, original);
    }

    #[test]
    fn later_declared_boundary_feedback_is_a_singular_quotient_cycle() {
        let component = parallel_identity_component();
        let mut state =
            TopologyState::new(&normalized(&["1"], &["1"]), profile(2, 0, 2, 0)).unwrap();
        let original = state.clone();
        let checkpoint = state.checkpoint();
        let attachment = state.component_attachments(&component)[0];
        let batch = state.apply_component(&component, attachment).unwrap();
        let one = Rational::one();
        let zero = Rational::zero();
        let (free_input, matching_output) = component
            .transfer()
            .rows()
            .enumerate()
            .find_map(|(output, row)| {
                (state.producer_ports()[&batch.boundary_outputs[output]]
                    .connection
                    .is_none())
                .then(|| {
                    batch
                        .boundary_inputs
                        .iter()
                        .enumerate()
                        .find(|(input, port)| {
                            state.consumer_ports()[port].connection.is_none()
                                && row.iter().enumerate().all(|(index, coefficient)| {
                                    coefficient == if index == *input { &one } else { &zero }
                                })
                        })
                        .map(|(input, _)| (input, output))
                })
                .flatten()
            })
            .unwrap();
        state
            .connect(
                batch.boundary_outputs[matching_output],
                batch.boundary_inputs[free_input],
            )
            .unwrap();
        let feedback_index = state.links().len() - 1;

        let partition = detect_affected_sccs(&state, Some(feedback_index)).unwrap();
        let region = partition
            .affected_regions()
            .find(|region| region.cyclic)
            .unwrap();
        assert_eq!(region.nodes.len(), batch.node_count);
        let summary = summarize_open_scc(&state, region, &BTreeMap::new()).unwrap();
        assert_eq!(summary.contracted_macro_count, 1);
        assert_eq!(summary.algebra.consistency, Consistency::Consistent);
        assert!(
            !summary.algebra.is_unique(),
            "a growable singular macro-level SCC remains live"
        );

        state.rollback(checkpoint);
        assert_eq!(state, original);
    }

    #[test]
    fn larger_macro_scc_matches_the_separately_flattened_primitive_rowspace_and_rolls_back() {
        let component = merge_split_component();
        let mut state =
            TopologyState::new(&normalized(&["1"], &["1"]), profile(2, 0, 1, 0)).unwrap();
        let original = state.clone();
        let checkpoint = state.checkpoint();
        let attachment = state.component_attachments(&component)[0];
        let batch = state.apply_component(&component, attachment).unwrap();
        let free_input = batch
            .boundary_inputs
            .iter()
            .copied()
            .find(|port| state.consumer_ports()[port].connection.is_none())
            .unwrap();
        let free_output = batch
            .boundary_outputs
            .iter()
            .copied()
            .find(|port| state.producer_ports()[port].connection.is_none())
            .unwrap();
        let outside = state.materialize(NodeType::Splitter2).unwrap();
        state
            .connect(
                free_output,
                ConsumerPortRef::Node {
                    node: outside.id,
                    port: 0,
                },
            )
            .unwrap();
        state
            .connect(
                ProducerPortRef::Node {
                    node: outside.id,
                    port: 0,
                },
                free_input,
            )
            .unwrap();
        let feedback_index = state.links().len() - 1;

        let quotient = detect_affected_sccs(&state, Some(feedback_index)).unwrap();
        let quotient_region = quotient
            .affected_regions()
            .find(|region| region.cyclic)
            .unwrap();
        assert_eq!(quotient_region.nodes.len(), batch.node_count + 1);
        assert!(quotient_region.nodes.contains(&outside.id));
        let quotient_summary =
            summarize_open_scc(&state, quotient_region, &BTreeMap::new()).unwrap();
        assert_eq!(quotient_summary.contracted_macro_count, 1);
        assert!(!quotient_summary.algebra.known_values.is_empty());

        let flattened = TopologyState::from_partial_topology(&state.partial_topology()).unwrap();
        let flattened_partition = detect_affected_sccs(&flattened, Some(feedback_index)).unwrap();
        let flattened_region = flattened_partition
            .affected_regions()
            .find(|region| region.cyclic)
            .unwrap();
        assert_eq!(quotient_region.nodes, flattened_region.nodes);
        let flattened_summary =
            summarize_open_scc(&flattened, flattened_region, &BTreeMap::new()).unwrap();
        assert_eq!(flattened_summary.contracted_macro_count, 0);
        assert_eq!(quotient_summary.algebra, flattened_summary.algebra);

        state.rollback(checkpoint);
        assert_eq!(detect_affected_sccs(&state, None).unwrap().regions.len(), 0);
        assert_eq!(state, original);
    }

    #[test]
    fn unavailable_component_profile_fails_without_mutation() {
        let component = splitter_component(false);
        let mut state =
            TopologyState::new(&normalized(&["1"], &["1"]), profile(0, 0, 0, 0)).unwrap();
        let original = state.clone();
        let attachment = ComponentAttachment::ExistingProducerToInput {
            producer: ProducerPortRef::Input(InputTerminalIndex(0)),
            component_input: 0,
        };
        assert_eq!(
            state.apply_component(&component, attachment),
            Err(TopologyError::ComponentProfileUnavailable)
        );
        assert_eq!(state, original);
    }

    #[test]
    fn repeated_deterministic_mutation_and_rollback_is_lossless() {
        let mut state =
            TopologyState::new(&normalized(&["3"], &["1", "2"]), profile(1, 0, 1, 0)).unwrap();
        let root = state.clone();
        let mut seed = 0x9e37_79b9_u64;
        for _ in 0..64 {
            let checkpoint = state.checkpoint();
            let before = state.clone();
            let decisions = state.legal_decisions();
            if !decisions.is_empty() {
                seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                let index =
                    usize::try_from(seed % u64::try_from(decisions.len()).unwrap()).unwrap();
                state.apply(decisions[index]).unwrap();
            }
            state.rollback(checkpoint);
            assert_eq!(state, before);
        }
        assert_eq!(state, root);
    }

    #[test]
    fn symmetric_unused_ports_have_one_local_representative() {
        let mut split =
            TopologyState::new(&normalized(&["2"], &["1", "1"]), profile(1, 0, 0, 0)).unwrap();
        let attach = split
            .legal_decisions()
            .into_iter()
            .find(|decision| {
                matches!(
                    decision.consumer,
                    ConsumerChoice::NewNode {
                        node_type: NodeType::Splitter2,
                        ..
                    }
                ) || matches!(
                    decision.producer,
                    ProducerChoice::NewNode {
                        node_type: NodeType::Splitter2,
                        ..
                    }
                )
            })
            .unwrap();
        split.apply(attach).unwrap();
        let count = split
            .open_representatives()
            .into_iter()
            .filter(|port| matches!(port, OpenPortRef::Producer(ProducerPortRef::Node { .. })))
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn decisions_never_materialize_two_nodes_at_once() {
        let mut state =
            TopologyState::new(&normalized(&["2"], &["2"]), profile(1, 0, 1, 0)).unwrap();
        assert!(state.legal_decisions().iter().all(|decision| !matches!(
            (decision.producer, decision.consumer),
            (
                ProducerChoice::NewNode { .. },
                ConsumerChoice::NewNode { .. }
            )
        )));
    }

    #[test]
    fn admissible_canonical_paths_cover_splitter_and_merger() {
        fn walk(state: &mut TopologyState, depth: usize) -> usize {
            if state.is_complete() {
                assert!(depth > 0);
                return 1;
            }
            let mut total = 0;
            for decision in state.legal_decisions() {
                let checkpoint = state.checkpoint();
                let id = state.apply(decision).unwrap();
                let link = state.link_index_for(id).unwrap();
                let accepted = is_canonical_last_link(&state.partial_topology(), link);
                if accepted {
                    total += walk(state, depth + 1);
                }
                state.rollback(checkpoint);
            }
            total
        }

        let mut state =
            TopologyState::new(&normalized(&["2"], &["1", "1"]), profile(1, 0, 0, 0)).unwrap();
        assert!(walk(&mut state, 0) > 0);

        let mut state =
            TopologyState::new(&normalized(&["1", "2"], &["3"]), profile(0, 0, 1, 0)).unwrap();
        assert!(walk(&mut state, 0) > 0);
    }

    #[test]
    // Keeping the exhaustive relabeling matrix in one test makes its invariant
    // comparison and raw-label counterexample auditable as a single proof case.
    #[allow(clippy::too_many_lines)]
    fn canonical_mrv_is_exhaustive_over_three_node_and_port_relabelings() {
        const PERMUTATIONS: [[u32; 3]; 6] = [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ];

        fn fixture(
            node_labels: [u32; 3],
            swapped_ports: u8,
            swap_equal_inputs: bool,
        ) -> PartialTopology {
            let physical_port = |old_node: usize, logical_port: u8| {
                logical_port ^ u8::from(swapped_ports & (1 << old_node) != 0)
            };
            let source = |logical_input: u32| {
                let index = if swap_equal_inputs && logical_input < 2 {
                    1 - logical_input
                } else {
                    logical_input
                };
                ProducerPortRef::Input(InputTerminalIndex(index))
            };
            let merger_input = |old_node: usize, logical_port: u8| ConsumerPortRef::Node {
                node: NodeId(node_labels[old_node]),
                port: physical_port(old_node, logical_port),
            };
            let merger_output = |old_node: usize| ProducerPortRef::Node {
                node: NodeId(node_labels[old_node]),
                port: 0,
            };
            let mut links = vec![
                PartialLink {
                    producer: source(0),
                    consumer: merger_input(0, 0),
                    flow: None,
                },
                PartialLink {
                    producer: source(1),
                    consumer: merger_input(0, 1),
                    flow: None,
                },
                PartialLink {
                    producer: source(2),
                    consumer: merger_input(1, 0),
                    flow: None,
                },
                PartialLink {
                    producer: source(3),
                    consumer: merger_input(2, 0),
                    flow: None,
                },
                PartialLink {
                    producer: merger_output(0),
                    consumer: ConsumerPortRef::Output(OutputTerminalIndex(0)),
                    flow: None,
                },
            ];
            if swapped_ports.count_ones() % 2 == 1 {
                links.reverse();
            }
            PartialTopology {
                discard_count: 0,
                problem: Problem {
                    inputs: ["1", "1", "3", "5"]
                        .into_iter()
                        .map(|rate| rate.parse().unwrap())
                        .collect(),
                    outputs: vec!["10".parse().unwrap()],
                    max_link_rate: "10".parse().unwrap(),
                },
                nodes: (0..3)
                    .map(|id| PhysicalNode {
                        id: NodeId(id),
                        node_type: NodeType::Merger2,
                    })
                    .collect(),
                links,
                remaining_profile: NodeProfile::default(),
            }
        }

        let baseline_topology = fixture(PERMUTATIONS[0], 0, false);
        let baseline_state_key = canonicalize_state(&baseline_topology);
        let mut baseline = TopologyState::from_partial_topology(&baseline_topology).unwrap();
        let baseline_orbit = baseline.selected_open_orbit().unwrap();
        let baseline_children = baseline.ordered_decision_child_keys();
        assert!(!baseline_children.is_empty());
        let mut raw_representatives = std::collections::BTreeSet::new();

        for permutation in PERMUTATIONS {
            for swapped_ports in 0..8 {
                for swap_equal_inputs in [false, true] {
                    let topology = fixture(permutation, swapped_ports, swap_equal_inputs);
                    assert_eq!(canonicalize_state(&topology), baseline_state_key);
                    let mut state = TopologyState::from_partial_topology(&topology).unwrap();
                    let orbit = state.selected_open_orbit().unwrap();
                    raw_representatives.insert(orbit.representative);
                    assert_eq!(
                        orbit.legal_partner_count,
                        baseline_orbit.legal_partner_count
                    );
                    assert_eq!(orbit.has_known_flow, baseline_orbit.has_known_flow);
                    assert_eq!(orbit.is_external, baseline_orbit.is_external);
                    assert_eq!(orbit.symmetry_class, baseline_orbit.symmetry_class);
                    assert_eq!(orbit.canonical_key, baseline_orbit.canonical_key);
                    assert_eq!(state.ordered_decision_child_keys(), baseline_children);
                }
            }
        }
        assert!(
            raw_representatives.len() > 1,
            "fixture must exercise distinct raw representatives of one invariant choice"
        );
    }
}
