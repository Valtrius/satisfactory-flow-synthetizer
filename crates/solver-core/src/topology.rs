//! Mutable lazy-materialized physical topology with undo-log rollback.

use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicBool, Ordering},
};

use solver_api::{
    ConsumerPortRef, DiscardTerminalIndex, InputTerminalIndex, NodeId, NodeProfile, NodeType,
    OutputTerminalIndex, PhysicalNode, Problem, ProducerPortRef, Rational,
};
use thiserror::Error;

use crate::{
    canonical::{
        CanonicalOpenPortKey, MarkedLinkCanonicalKey, PartialLink, PartialTopology,
        canonicalize_marked_link, canonicalize_marked_link_cancellable, canonicalize_open_port,
        canonicalize_open_port_cancellable,
    },
    problem::NormalizedProblem,
};

/// Stable graph-local physical link identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LinkId(pub u32);

/// Stable identifier of an applied structural decision.
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

    /// Reconstructs a topology state from a partial snapshot for tests.
    ///
    /// Canonicalization supplies contiguous node identifiers. Existing links are
    /// loaded as committed history; the returned undo log starts at that parent.
    #[cfg(test)]
    #[allow(clippy::too_many_lines)]
    pub(crate) fn from_partial_topology(topology: &PartialTopology) -> Result<Self, TopologyError> {
        let mut state = Self {
            problem: topology.problem.clone(),
            remaining: topology.remaining_profile.into(),
            nodes: Vec::new(),
            producer_ports: BTreeMap::new(),
            consumer_ports: BTreeMap::new(),
            links: Vec::new(),
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

    /// Returns true when every remaining port is connected and the profile inventory is empty.
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
        self.selected_open_orbit_inner(None).unwrap_or_default()
    }

    // The outer option reports cancellation; the inner option reports a
    // complete topology with no remaining open-port orbit.
    #[allow(clippy::option_option)]
    pub(crate) fn selected_open_orbit_cancellable(
        &self,
        cancel: &AtomicBool,
    ) -> Option<Option<OpenPortOrbit>> {
        self.selected_open_orbit_inner(Some(cancel))
    }

    #[allow(clippy::option_option)]
    fn selected_open_orbit_inner(
        &self,
        cancel: Option<&AtomicBool>,
    ) -> Option<Option<OpenPortOrbit>> {
        let topology = self.partial_topology();
        let mut orbits = Vec::new();
        for representative in self.open_representatives() {
            if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
                return None;
            }
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
            let canonical_key = match cancel {
                Some(flag) => canonicalize_open_port_cancellable(&topology, representative, flag)?,
                None => canonicalize_open_port(&topology, representative),
            };
            orbits.push(OpenPortOrbit {
                representative,
                legal_partner_count,
                has_known_flow,
                is_external,
                symmetry_class,
                canonical_key,
            });
        }
        Some(orbits.into_iter().min_by(|left, right| {
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
        }))
    }

    /// Lists legal decisions in canonical marked-child order.
    ///
    /// Raw endpoint references appear only after the invariant child key and
    /// can therefore choose a concrete member inside one automorphism orbit
    /// without changing the quotient search order.
    #[must_use]
    pub fn legal_decisions(&mut self) -> Vec<TopologyDecision> {
        self.legal_decisions_cancellable(&AtomicBool::new(false))
            .unwrap_or_default()
    }

    /// Cancellation-aware form of [`Self::legal_decisions`].
    ///
    /// Marked-child canonicalization for each candidate can dominate search time,
    /// so the production search must be able to abandon this enumeration.
    pub(crate) fn legal_decisions_cancellable(
        &mut self,
        cancel: &AtomicBool,
    ) -> Option<Vec<TopologyDecision>> {
        let Some(orbit) = self.selected_open_orbit_cancellable(cancel)? else {
            return Some(Vec::new());
        };
        self.legal_decisions_for_orbit_cancellable(&orbit, cancel)
    }

    pub(crate) fn legal_decisions_for_orbit_cancellable(
        &mut self,
        orbit: &OpenPortOrbit,
        cancel: &AtomicBool,
    ) -> Option<Vec<TopologyDecision>> {
        Some(
            self.keyed_decisions_for(orbit.representative, Some(cancel))?
                .into_iter()
                .map(|(_, decision)| decision)
                .collect(),
        )
    }

    /// Returns the invariant ordered marked-child keys of all legal decisions.
    #[must_use]
    pub fn ordered_decision_child_keys(&mut self) -> Vec<MarkedLinkCanonicalKey> {
        self.selected_open_orbit().map_or_else(Vec::new, |orbit| {
            self.keyed_decisions_for(orbit.representative, None)
                .unwrap_or_default()
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

    /// Applies a decision already known to be in [`Self::legal_decisions`].
    ///
    /// Search uses this after a cancellable legal-decision enumeration so it does
    /// not pay a second full marked-child canonicalization pass on every branch.
    pub(crate) fn apply_legal_decision(
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

    /// Pairs each legal physical decision with its canonical marked-child key.
    #[must_use]
    fn keyed_decisions_for(
        &mut self,
        anchor: OpenPortRef,
        cancel: Option<&AtomicBool>,
    ) -> Option<Vec<(MarkedLinkCanonicalKey, TopologyDecision)>> {
        let mut keyed = Vec::new();
        for decision in self.decisions_for(anchor) {
            if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
                return None;
            }
            let checkpoint = self.checkpoint();
            let key = match self
                .apply_legal_decision(decision)
                .ok()
                .and_then(|decision_id| self.link_index_for(decision_id))
            {
                Some(link) => match cancel {
                    Some(flag) => {
                        canonicalize_marked_link_cancellable(&self.partial_topology(), link, flag)
                    }
                    None => Some(canonicalize_marked_link(&self.partial_topology(), link)),
                },
                None => None,
            };
            self.rollback(checkpoint);
            if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
                return None;
            }
            if let Some(key) = key {
                keyed.push((key, decision));
            }
        }
        keyed.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
        keyed.dedup_by(|left, right| left.0 == right.0);
        Some(keyed)
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

// The certificate checks are deliberately contiguous: every physical port must
// appear exactly once as either an internal endpoint or a declared boundary.
// Splitting this proof across helpers would make that partition harder to audit.
#[allow(clippy::too_many_lines)]
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
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;
    use crate::{Preparation, canonical::canonicalize_state, prepare_problem};

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
    fn legal_decision_enumeration_honors_cancellation() {
        let mut state =
            TopologyState::new(&normalized(&["3"], &["1", "1", "1"]), profile(0, 1, 0, 0)).unwrap();
        let cancel = AtomicBool::new(true);
        assert!(state.legal_decisions_cancellable(&cancel).is_none());
        cancel.store(false, Ordering::Relaxed);
        assert!(
            !state
                .legal_decisions_cancellable(&cancel)
                .unwrap()
                .is_empty()
        );
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
    fn legal_decision_paths_cover_splitter_and_merger() {
        fn walk(state: &mut TopologyState, depth: usize) -> usize {
            if state.is_complete() {
                assert!(depth > 0);
                return 1;
            }
            let mut total = 0;
            for decision in state.legal_decisions() {
                let checkpoint = state.checkpoint();
                state.apply(decision).unwrap();
                total += walk(state, depth + 1);
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
