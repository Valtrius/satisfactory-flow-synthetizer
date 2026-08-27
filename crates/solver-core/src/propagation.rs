//! Rollback exact propagation for one incrementally materialized topology.
//!
//! Sparse integer rows are the authoritative statement of physical equality
//! semantics. The weighted union-find is only a fast representation of proven
//! one-variable values and two-variable ratios. Every promotion comes either
//! directly from a physical equation or from an equivalent fraction-free row
//! basis. Consequently, a promotion cannot remove a valid solution.
//!
//! Every retained row cites a causal decision-context proof node. A promoted
//! known value or representative ratio cites the retained authoritative row
//! proofs used by exact elimination. This parent set is deliberately broad:
//! extra valid equations can reduce reuse, but cannot create a false
//! impossibility proof. Capacity contradictions cite both those exact facts
//! and the physical bound axioms. Search therefore recovers a no-good core by
//! traversing the proof already attached to the contradiction instead of
//! inventing parents later. Append-only undo logs restore proof node identity
//! and fact provenance exactly when topology search rolls back.
//!
//! Pruning is deliberately one-sided. This module reports `Pruned` only after
//! an exact equation contradiction, an exact nonpositive/over-capacity known
//! flow, or a negative exact ratio between two variables that must both be
//! positive. Unresolved equations and inequalities always remain feasible.

use std::collections::BTreeMap;

use num::{BigInt, BigRational, Integer, One};
use solver_api::{ConsumerPortRef, NodeId, NodeType, PhysicalNode, ProducerPortRef, Rational};
use thiserror::Error;

use crate::{
    algebra::{
        inequality::{ExactInequality, InequalityEvaluation, physical_flow_constraints},
        sparse::{
            Consistency, RowCheckpoint, RowInsertion, SparseAlgebraError, SparseRow, SparseSystem,
        },
        weighted::{ConstraintOutcome, WeightedCheckpoint, WeightedError, WeightedUnionFind},
    },
    canonical::PartialTopology,
    components::Component,
    no_good::{ConflictRule, NoGoodScope, ProofNodeId, ProvenanceDag, ProvenanceRecorder},
    topology::{
        AppliedComponentBatch, Direction, FlowVarId, Link, Port, PortClass, PortOwner,
        TopologyState,
    },
};

/// An exact mathematical reason why the current topology branch is impossible.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PropagationConflict {
    /// The authoritative sparse equations have no rational solution.
    SparseInconsistency,
    /// A direct or promoted exact equality conflicts with an existing fact.
    ExactConstraintContradiction,
    /// A physical flow is known to violate mandatory strict positivity.
    NonPositiveKnown {
        /// Violating physical flow variable.
        variable: FlowVarId,
        /// Exact nonpositive value.
        value: Rational,
    },
    /// A physical flow is known to exceed the mandatory exact link capacity.
    CapacityExceeded {
        /// Violating physical flow variable.
        variable: FlowVarId,
        /// Exact known flow.
        value: Rational,
        /// Exact inclusive capacity bound.
        capacity: Rational,
    },
    /// Two mandatory-positive physical flows have a proven negative ratio.
    NegativeRatio {
        /// Variable related to the representative.
        variable: FlowVarId,
        /// Deterministic union-find representative.
        representative: FlowVarId,
        /// Exact factor in `variable = factor * representative`.
        factor: Rational,
    },
    /// A fully known exact bound row evaluated to false.
    ExactBoundViolation {
        /// Stable insertion index of the violated bound row.
        constraint_index: usize,
    },
}

/// Exact propagation status for the current speculative branch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PropagationOutcome {
    /// No exact contradiction is currently proven.
    Feasible,
    /// The branch is eliminated by the contained exact proof.
    Pruned(PropagationConflict),
}

impl PropagationOutcome {
    /// Returns whether propagation has proved this branch impossible.
    #[must_use]
    pub const fn is_pruned(&self) -> bool {
        matches!(self, Self::Pruned(_))
    }
}

/// Result of promoting exact consequences proved by one open-SCC summary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SccPropagationUpdate {
    /// Whether at least one previously unknown value or ratio was added.
    pub changed: bool,
    /// Feasibility after ordinary propagation and every exact physical bound.
    pub outcome: PropagationOutcome,
}

/// Internal or incremental-topology contract failure, never a pruning proof.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum PropagationError {
    /// Capacity must be strictly positive before propagation begins.
    #[error("propagation capacity must be positive, got {capacity}")]
    NonPositiveCapacity { capacity: Rational },
    /// The topology no longer extends the prefix already synchronized.
    #[error("topology changed outside append-only mutation plus coordinated rollback")]
    TopologyNotAppendOnly,
    /// Two explicit physical ports reused one exact flow variable.
    #[error("flow variable {variable:?} is attached to more than one physical port")]
    DuplicateFlowVariable { variable: FlowVarId },
    /// A node or link referred to a physical port absent from the topology.
    #[error("physical endpoint has no registered flow variable")]
    MissingPort,
    /// A flow snapshot was requested before synchronizing the current topology.
    #[error("propagation state is not synchronized with the supplied topology")]
    TopologyOutOfSync,
    /// Propagation is only constructed after global input-deficit rejection.
    #[error("propagation topology has less total input than requested output")]
    InputDeficit,
    /// A component summary did not exactly reproduce its flattened primitive
    /// equality row space or its declared physical footprint.
    #[error("component projected-equivalence certificate is invalid: {0}")]
    InvalidComponentCertificate(&'static str),
    /// Fraction-free sparse elimination violated an internal exactness invariant.
    #[error(transparent)]
    Sparse(#[from] SparseAlgebraError),
    /// The weighted fast path received a malformed operation.
    #[error(transparent)]
    Weighted(#[from] WeightedError),
}

/// Opaque rollback point spanning every semantic propagation layer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropagationCheckpoint {
    weighted: WeightedCheckpoint,
    sparse: RowCheckpoint,
    row_proof_len: usize,
    bound_len: usize,
    bound_proof_len: usize,
    port_len: usize,
    node_len: usize,
    link_len: usize,
    fact_undo_len: usize,
    provenance_len: usize,
    decision_context: ProofNodeId,
    conflict_proof: Option<ProofNodeId>,
    outcome: PropagationOutcome,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct KnownValueProof {
    value: Rational,
    proof: ProofNodeId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RepresentativeRatioProof {
    representative: FlowVarId,
    factor: Rational,
    proof: ProofNodeId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum FactUndo {
    KnownValueAdded(FlowVarId),
    RepresentativeRatioChanged {
        variable: FlowVarId,
        previous: Option<RepresentativeRatioProof>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PortEndpoint {
    Producer(ProducerPortRef),
    Consumer(ConsumerPortRef),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PortSignature {
    endpoint: PortEndpoint,
    owner: PortOwner,
    direction: Direction,
    symmetry_class: PortClass,
    known_flow: Option<Rational>,
}

impl PortSignature {
    fn new(endpoint: PortEndpoint, port: &Port) -> Self {
        Self {
            endpoint,
            owner: port.owner,
            direction: port.direction,
            symmetry_class: port.symmetry_class,
            known_flow: port.known_flow.clone(),
        }
    }
}

/// Exact incremental propagation state paired with one [`TopologyState`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropagationState {
    capacity: Rational,
    weighted: WeightedUnionFind,
    sparse: SparseSystem,
    row_proofs: Vec<ProofNodeId>,
    bounds: Vec<ExactInequality>,
    bound_proofs: Vec<ProofNodeId>,
    registered_ports: BTreeMap<FlowVarId, PortSignature>,
    registered_nodes: Vec<PhysicalNode>,
    registered_links: Vec<Link>,
    known_value_proofs: BTreeMap<FlowVarId, KnownValueProof>,
    representative_ratio_proofs: BTreeMap<FlowVarId, RepresentativeRatioProof>,
    fact_undo: Vec<FactUndo>,
    provenance: ProvenanceRecorder,
    decision_context: ProofNodeId,
    conflict_proof: Option<ProofNodeId>,
    outcome: PropagationOutcome,
}

impl PropagationState {
    /// Builds and fully propagates the supplied initial topology.
    ///
    /// An initially impossible topology is returned normally with
    /// [`PropagationOutcome::Pruned`] available through
    /// [`Self::current_outcome`]. Algebra implementation failures remain
    /// [`PropagationError`] values.
    ///
    /// # Errors
    ///
    /// Returns an error for a nonpositive capacity, malformed incremental
    /// topology, or an internal exact-algebra failure.
    pub fn new(topology: &TopologyState, capacity: &Rational) -> Result<Self, PropagationError> {
        if !capacity.is_positive() {
            return Err(PropagationError::NonPositiveCapacity {
                capacity: capacity.clone(),
            });
        }
        let mut provenance = ProvenanceRecorder::new(NoGoodScope::SolveLocal);
        let scope_root = provenance.scope_root();
        let decision_context = provenance.derived(ConflictRule::ExactPropagation, [scope_root]);
        let mut state = Self {
            capacity: capacity.clone(),
            weighted: WeightedUnionFind::new(),
            sparse: SparseSystem::new(),
            row_proofs: Vec::new(),
            bounds: Vec::new(),
            bound_proofs: Vec::new(),
            registered_ports: BTreeMap::new(),
            registered_nodes: Vec::new(),
            registered_links: Vec::new(),
            known_value_proofs: BTreeMap::new(),
            representative_ratio_proofs: BTreeMap::new(),
            fact_undo: Vec::new(),
            provenance,
            decision_context,
            conflict_proof: None,
            outcome: PropagationOutcome::Feasible,
        };
        let outcome = state.synchronize_after_topology_mutation(topology)?;
        if !outcome.is_pruned() {
            state.register_initial_discard_balance(topology)?;
        }
        Ok(state)
    }

    /// Returns the current branch status established by the last fixed point.
    #[must_use]
    pub const fn current_outcome(&self) -> &PropagationOutcome {
        &self.outcome
    }

    /// Captures every rollback-capable semantic layer and processed prefix.
    #[must_use]
    pub fn checkpoint(&self) -> PropagationCheckpoint {
        PropagationCheckpoint {
            weighted: self.weighted.checkpoint(),
            sparse: self.sparse.checkpoint(),
            row_proof_len: self.row_proofs.len(),
            bound_len: self.bounds.len(),
            bound_proof_len: self.bound_proofs.len(),
            port_len: self.registered_ports.len(),
            node_len: self.registered_nodes.len(),
            link_len: self.registered_links.len(),
            fact_undo_len: self.fact_undo.len(),
            provenance_len: self.provenance.checkpoint(),
            decision_context: self.decision_context,
            conflict_proof: self.conflict_proof,
            outcome: self.outcome.clone(),
        }
    }

    /// Restores all facts, rows, bounds, registered prefixes, and proof status.
    ///
    /// # Panics
    ///
    /// Panics for a checkpoint beyond the current state, including a stale
    /// checkpoint reused after rolling back past it.
    pub fn rollback(&mut self, checkpoint: PropagationCheckpoint) {
        assert!(checkpoint.bound_len <= self.bounds.len());
        assert!(checkpoint.row_proof_len <= self.row_proofs.len());
        assert!(checkpoint.bound_proof_len <= self.bound_proofs.len());
        assert!(checkpoint.port_len <= self.registered_ports.len());
        assert!(checkpoint.node_len <= self.registered_nodes.len());
        assert!(checkpoint.link_len <= self.registered_links.len());
        assert!(checkpoint.fact_undo_len <= self.fact_undo.len());

        self.weighted.rollback(checkpoint.weighted);
        self.sparse.rollback(checkpoint.sparse);
        self.row_proofs.truncate(checkpoint.row_proof_len);
        self.bounds.truncate(checkpoint.bound_len);
        self.bound_proofs.truncate(checkpoint.bound_proof_len);
        while self.registered_ports.len() > checkpoint.port_len {
            self.registered_ports
                .pop_last()
                .expect("registered port length was checked");
        }
        self.registered_nodes.truncate(checkpoint.node_len);
        self.registered_links.truncate(checkpoint.link_len);
        while self.fact_undo.len() > checkpoint.fact_undo_len {
            match self.fact_undo.pop().expect("fact undo length was checked") {
                FactUndo::KnownValueAdded(variable) => {
                    self.known_value_proofs
                        .remove(&variable)
                        .expect("known-value undo owns the fact");
                }
                FactUndo::RepresentativeRatioChanged { variable, previous } => {
                    if let Some(previous) = previous {
                        self.representative_ratio_proofs.insert(variable, previous);
                    } else {
                        self.representative_ratio_proofs.remove(&variable);
                    }
                }
            }
        }
        self.provenance.rollback(checkpoint.provenance_len);
        self.decision_context = checkpoint.decision_context;
        self.conflict_proof = checkpoint.conflict_proof;
        self.outcome = checkpoint.outcome;
    }

    /// Registers an appended topology suffix and computes an exact fixed point.
    ///
    /// Sparse rows are retained even when their consequences are promoted into
    /// the weighted fast path. Any internal error rolls this method back to its
    /// entry checkpoint. A proven `Pruned` result is retained until the caller
    /// rolls back the surrounding branch checkpoint.
    ///
    /// # Errors
    ///
    /// Returns an error if the topology does not extend the registered prefix
    /// or exact elimination violates an implementation invariant.
    pub fn synchronize_after_topology_mutation(
        &mut self,
        topology: &TopologyState,
    ) -> Result<PropagationOutcome, PropagationError> {
        if self.outcome.is_pruned() {
            return Ok(self.outcome.clone());
        }

        let checkpoint = self.checkpoint();
        match self.synchronize_inner(topology) {
            Ok(outcome) => {
                self.outcome = outcome.clone();
                Ok(outcome)
            }
            Err(error) => {
                self.rollback(checkpoint);
                Err(error)
            }
        }
    }

    /// Registers one physically expanded component through its exact projected
    /// boundary semantics, then computes the ordinary exact fixed point.
    ///
    /// The appended physical ports and their mandatory bounds are registered,
    /// but primitive rows for the component's internal nodes and links are not
    /// inserted in the hot loop. Component construction/import has already
    /// proved that `R*x=0`, `y=T*x`, and every canonical `K` row span exactly
    /// the flattened witness's primitive equality row space and stored an
    /// immutable certification token. This method checks that token and cheap
    /// batch dimensions, then installs those equivalent rows plus the ordinary
    /// anchor-link equality. Thus the optimization cannot restrict or enlarge
    /// the physical completion set without rebuilding internal equations here.
    ///
    /// Any error rolls every propagation layer back to the entry checkpoint.
    /// A mathematical contradiction is returned as [`PropagationOutcome::Pruned`].
    ///
    /// # Errors
    ///
    /// Returns an internal error for a stale topology prefix, malformed batch,
    /// invalid exact certificate, or algebra implementation failure.
    pub fn synchronize_after_component_batch(
        &mut self,
        topology: &TopologyState,
        component: &Component,
        batch: &AppliedComponentBatch,
    ) -> Result<PropagationOutcome, PropagationError> {
        if self.outcome.is_pruned() {
            return Ok(self.outcome.clone());
        }
        let checkpoint = self.checkpoint();
        match self.synchronize_component_inner(topology, component, batch) {
            Ok(outcome) => {
                self.outcome = outcome.clone();
                Ok(outcome)
            }
            Err(error) => {
                self.rollback(checkpoint);
                Err(error)
            }
        }
    }

    /// Promotes exact value and homogeneous-ratio consequences proved by an
    /// open-SCC elimination, then restores the ordinary propagation fixed point
    /// and checks all mandatory physical bounds.
    ///
    /// This is crate-private because its soundness contract is strict: every
    /// supplied fact must be a consequence of the current materialized SCC's
    /// authoritative physical equations. Search obtains exactly those facts
    /// from `summarize_open_scc`; arbitrary guesses must never enter here.
    /// Adding a logical consequence cannot remove a valid completion. The
    /// broad retained row-proof parent set is conservative but sound, and all
    /// mutations participate in the normal checkpoint/rollback logs.
    pub(crate) fn promote_open_scc_deductions(
        &mut self,
        known_values: &[(FlowVarId, Rational)],
        homogeneous_ratios: &[(FlowVarId, FlowVarId, Rational)],
    ) -> Result<SccPropagationUpdate, PropagationError> {
        if self.outcome.is_pruned() {
            return Ok(SccPropagationUpdate {
                changed: false,
                outcome: self.outcome.clone(),
            });
        }

        let checkpoint = self.checkpoint();
        match self.promote_open_scc_deductions_inner(known_values, homogeneous_ratios) {
            Ok(update) => {
                self.outcome = update.outcome.clone();
                Ok(update)
            }
            Err(error) => {
                self.rollback(checkpoint);
                Err(error)
            }
        }
    }

    /// Returns one exact propagated value, if currently proved.
    ///
    /// # Errors
    ///
    /// Returns [`PropagationError::Weighted`] when `variable` is not registered.
    pub fn known_value(&self, variable: FlowVarId) -> Result<Option<Rational>, PropagationError> {
        Ok(self.weighted.known_value(variable)?)
    }

    /// Returns the retained exact proof for a currently known value.
    ///
    /// The returned DAG contains only nodes reachable from that fact. Its root
    /// and all parent indices are stable across deterministic replay.
    #[must_use]
    pub fn known_value_provenance(&self, variable: FlowVarId) -> Option<ProvenanceDag> {
        self.known_value_proofs
            .get(&variable)
            .map(|fact| self.provenance.extract(fact.proof))
    }

    /// Returns the retained proof for the current union-find representative ratio.
    #[must_use]
    pub fn representative_ratio_provenance(&self, variable: FlowVarId) -> Option<ProvenanceDag> {
        self.representative_ratio_proofs
            .get(&variable)
            .map(|fact| self.provenance.extract(fact.proof))
    }

    /// Returns the proof captured when the current branch became impossible.
    #[must_use]
    pub fn conflict_provenance(&self) -> Option<ProvenanceDag> {
        self.conflict_proof
            .map(|proof| self.provenance.extract(proof))
    }

    /// Copies the topology snapshot and fills every exactly known link flow.
    ///
    /// # Errors
    ///
    /// Returns [`PropagationError::TopologyOutOfSync`] unless the supplied
    /// topology exactly matches the synchronized node, link, and port prefix.
    pub fn partial_topology_with_known_link_flows(
        &self,
        topology: &TopologyState,
    ) -> Result<PartialTopology, PropagationError> {
        self.require_fully_synchronized(topology)?;
        let mut partial = topology.partial_topology();
        for (partial_link, link) in partial.links.iter_mut().zip(topology.links()) {
            let variable = topology
                .producer_ports()
                .get(&link.producer)
                .ok_or(PropagationError::MissingPort)?
                .flow_var;
            partial_link.flow = self.weighted.known_value(variable)?;
        }
        Ok(partial)
    }

    fn synchronize_inner(
        &mut self,
        topology: &TopologyState,
    ) -> Result<PropagationOutcome, PropagationError> {
        let current_ports = collect_port_signatures(topology)?;
        self.validate_registered_prefix(topology, &current_ports)?;
        let proof_context = self.topology_proof_context(topology);

        let mut direct_conflict = None;
        for (&variable, signature) in &current_ports {
            if self.registered_ports.contains_key(&variable) {
                continue;
            }
            if self
                .registered_ports
                .last_key_value()
                .is_some_and(|(&last, _)| variable <= last)
            {
                return Err(PropagationError::TopologyNotAppendOnly);
            }
            self.register_port(
                variable,
                signature.clone(),
                proof_context,
                &mut direct_conflict,
            )?;
        }

        for node in &topology.nodes()[self.registered_nodes.len()..] {
            self.register_node(topology, node, proof_context, &mut direct_conflict)?;
            self.registered_nodes.push(node.clone());
        }
        for link in &topology.links()[self.registered_links.len()..] {
            self.register_link(topology, link, proof_context, &mut direct_conflict)?;
            self.registered_links.push(link.clone());
        }

        if let Some(conflict) = direct_conflict {
            return Ok(self.pruned_outcome(conflict));
        }
        if let Some(conflict) = self.propagate_fixed_point()? {
            return Ok(self.pruned_outcome(conflict));
        }
        Ok(match self.check_exact_bounds()? {
            Some(conflict) => self.pruned_outcome(conflict),
            None => PropagationOutcome::Feasible,
        })
    }

    fn promote_open_scc_deductions_inner(
        &mut self,
        known_values: &[(FlowVarId, Rational)],
        homogeneous_ratios: &[(FlowVarId, FlowVarId, Rational)],
    ) -> Result<SccPropagationUpdate, PropagationError> {
        let mut changed = false;
        let mut contradiction = false;
        for (variable, value) in known_values {
            match self.weighted.assign(*variable, value)? {
                ConstraintOutcome::Changed => changed = true,
                ConstraintOutcome::AlreadySatisfied => {}
                ConstraintOutcome::Contradiction => contradiction = true,
            }
        }
        for (lhs, rhs, factor) in homogeneous_ratios {
            match self.weighted.relate(*lhs, factor, *rhs)? {
                ConstraintOutcome::Changed => changed = true,
                ConstraintOutcome::AlreadySatisfied => {}
                ConstraintOutcome::Contradiction => contradiction = true,
            }
        }

        let conflict = if contradiction {
            Some(PropagationConflict::ExactConstraintContradiction)
        } else {
            self.propagate_fixed_point()?
        };
        let outcome = if let Some(conflict) = conflict {
            self.pruned_outcome(conflict)
        } else if let Some(conflict) = self.check_exact_bounds()? {
            self.pruned_outcome(conflict)
        } else {
            PropagationOutcome::Feasible
        };
        Ok(SccPropagationUpdate { changed, outcome })
    }

    fn synchronize_component_inner(
        &mut self,
        topology: &TopologyState,
        component: &Component,
        batch: &AppliedComponentBatch,
    ) -> Result<PropagationOutcome, PropagationError> {
        verify_component_batch(
            topology,
            component,
            batch,
            self.registered_nodes.len(),
            self.registered_links.len(),
        )?;
        if !component.is_projection_certified() {
            return Err(PropagationError::InvalidComponentCertificate(
                "component lacks an immutable projected-equivalence proof",
            ));
        }

        let current_ports = collect_port_signatures(topology)?;
        self.validate_registered_prefix(topology, &current_ports)?;
        let proof_context = self.topology_proof_context(topology);
        let mut direct_conflict = None;
        for (&variable, signature) in &current_ports {
            if self.registered_ports.contains_key(&variable) {
                continue;
            }
            if self
                .registered_ports
                .last_key_value()
                .is_some_and(|(&last, _)| variable <= last)
            {
                return Err(PropagationError::TopologyNotAppendOnly);
            }
            self.register_port(
                variable,
                signature.clone(),
                proof_context,
                &mut direct_conflict,
            )?;
        }

        self.registered_nodes
            .extend_from_slice(&topology.nodes()[batch.node_start..]);
        let input_variables = batch
            .boundary_inputs
            .iter()
            .copied()
            .map(|port| consumer_variable(topology, port))
            .collect::<Result<Vec<_>, _>>()?;
        let output_variables = batch
            .boundary_outputs
            .iter()
            .copied()
            .map(|port| producer_variable(topology, port))
            .collect::<Result<Vec<_>, _>>()?;

        for row in component.domain().equalities().rows() {
            self.insert_sparse_row(
                rational_sparse_row(input_variables.iter().copied().zip(row.iter().cloned())),
                proof_context,
            );
        }
        for (output, coefficients) in output_variables
            .iter()
            .copied()
            .zip(component.transfer().rows())
        {
            self.insert_sparse_row(
                component_expression_row(output, &input_variables, coefficients),
                proof_context,
            );
        }
        for (&link_index, coefficients) in batch
            .internal_link_indices
            .iter()
            .zip(component.internal_flow_map().rows())
        {
            let link = &topology.links()[link_index];
            let producer = producer_variable(topology, link.producer)?;
            let consumer = consumer_variable(topology, link.consumer)?;
            self.insert_sparse_row(
                component_expression_row(producer, &input_variables, coefficients),
                proof_context,
            );
            self.insert_sparse_row(
                component_expression_row(consumer, &input_variables, coefficients),
                proof_context,
            );
            self.registered_links.push(link.clone());
        }

        let anchor = &topology.links()[batch.anchor_link_index];
        self.register_link(topology, anchor, proof_context, &mut direct_conflict)?;
        self.registered_links.push(anchor.clone());

        if let Some(conflict) = direct_conflict {
            return Ok(self.pruned_outcome(conflict));
        }
        if let Some(conflict) = self.propagate_fixed_point()? {
            return Ok(self.pruned_outcome(conflict));
        }
        Ok(match self.check_exact_bounds()? {
            Some(conflict) => self.pruned_outcome(conflict),
            None => PropagationOutcome::Feasible,
        })
    }

    fn register_initial_discard_balance(
        &mut self,
        topology: &TopologyState,
    ) -> Result<(), PropagationError> {
        let surplus = topology
            .problem()
            .surplus()
            .ok_or(PropagationError::InputDeficit)?;
        let discard_variables = topology
            .consumer_ports()
            .iter()
            .filter_map(|(endpoint, port)| {
                matches!(endpoint, ConsumerPortRef::Discard(_)).then_some(port.flow_var)
            })
            .collect::<Vec<_>>();
        let scope_root = self.provenance.scope_root();
        let proof_context = self
            .provenance
            .derived(ConflictRule::ExactPropagation, [scope_root]);
        self.insert_sparse_row(
            rational_sparse_affine_row(
                discard_variables
                    .into_iter()
                    .map(|variable| (variable, Rational::one())),
                &surplus,
            ),
            proof_context,
        );
        self.outcome = if let Some(conflict) = self.propagate_fixed_point()? {
            self.pruned_outcome(conflict)
        } else {
            match self.check_exact_bounds()? {
                Some(conflict) => self.pruned_outcome(conflict),
                None => PropagationOutcome::Feasible,
            }
        };
        Ok(())
    }

    fn validate_registered_prefix(
        &self,
        topology: &TopologyState,
        current_ports: &BTreeMap<FlowVarId, PortSignature>,
    ) -> Result<(), PropagationError> {
        if !topology.nodes().starts_with(&self.registered_nodes)
            || !topology.links().starts_with(&self.registered_links)
            || self
                .registered_ports
                .iter()
                .any(|(variable, signature)| current_ports.get(variable) != Some(signature))
        {
            return Err(PropagationError::TopologyNotAppendOnly);
        }
        Ok(())
    }

    fn require_fully_synchronized(&self, topology: &TopologyState) -> Result<(), PropagationError> {
        let current_ports = collect_port_signatures(topology)?;
        if topology.nodes() != self.registered_nodes
            || topology.links() != self.registered_links
            || current_ports != self.registered_ports
        {
            return Err(PropagationError::TopologyOutOfSync);
        }
        Ok(())
    }

    fn topology_proof_context(&mut self, topology: &TopologyState) -> ProofNodeId {
        let new_links = &topology.links()[self.registered_links.len()..];
        if new_links.is_empty() {
            return self.decision_context;
        }
        let mut parents = vec![self.decision_context];
        for link in new_links {
            parents.push(self.provenance.decision(link.decision_id));
        }
        self.decision_context = self
            .provenance
            .derived(ConflictRule::ExactPropagation, parents);
        self.decision_context
    }

    fn insert_sparse_row(&mut self, row: SparseRow, proof: ProofNodeId) {
        if self.sparse.insert(row) == RowInsertion::Added {
            self.row_proofs.push(proof);
        }
        debug_assert_eq!(self.row_proofs.len(), self.sparse.rows().len());
    }

    fn refresh_fact_proofs(&mut self) -> Result<(), PropagationError> {
        let facts = self
            .registered_ports
            .keys()
            .copied()
            .map(|variable| {
                Ok((
                    variable,
                    self.weighted.known_value(variable)?,
                    self.weighted.representative(variable)?,
                ))
            })
            .collect::<Result<Vec<_>, PropagationError>>()?;
        let parents = if self.row_proofs.is_empty() {
            vec![self.provenance.scope_root()]
        } else {
            self.row_proofs.clone()
        };
        let proof = self
            .provenance
            .derived(ConflictRule::ExactPropagation, parents);

        for (variable, known, (representative, factor)) in facts {
            if let Some(value) = known {
                self.record_known_value_proof(variable, value, proof);
            }
            self.record_representative_ratio_proof(variable, representative, factor, proof);
        }
        Ok(())
    }

    fn record_known_value_proof(
        &mut self,
        variable: FlowVarId,
        value: Rational,
        proof: ProofNodeId,
    ) {
        if let Some(existing) = self.known_value_proofs.get(&variable) {
            debug_assert_eq!(existing.value, value);
            return;
        }
        self.fact_undo.push(FactUndo::KnownValueAdded(variable));
        self.known_value_proofs
            .insert(variable, KnownValueProof { value, proof });
    }

    fn record_representative_ratio_proof(
        &mut self,
        variable: FlowVarId,
        representative: FlowVarId,
        factor: Rational,
        proof: ProofNodeId,
    ) {
        if self
            .representative_ratio_proofs
            .get(&variable)
            .is_some_and(|existing| {
                existing.representative == representative && existing.factor == factor
            })
        {
            return;
        }
        let previous = self.representative_ratio_proofs.get(&variable).cloned();
        self.fact_undo
            .push(FactUndo::RepresentativeRatioChanged { variable, previous });
        self.representative_ratio_proofs.insert(
            variable,
            RepresentativeRatioProof {
                representative,
                factor,
                proof,
            },
        );
    }

    fn pruned_outcome(&mut self, conflict: PropagationConflict) -> PropagationOutcome {
        let rule = match conflict {
            PropagationConflict::SparseInconsistency
            | PropagationConflict::ExactConstraintContradiction => ConflictRule::ExactPropagation,
            PropagationConflict::NonPositiveKnown { .. }
            | PropagationConflict::CapacityExceeded { .. }
            | PropagationConflict::NegativeRatio { .. }
            | PropagationConflict::ExactBoundViolation { .. } => ConflictRule::ExactCapacity,
        };
        let mut parents = if rule == ConflictRule::ExactPropagation {
            self.row_proofs.clone()
        } else {
            self.bound_proofs
                .iter()
                .copied()
                .chain(self.known_value_proofs.values().map(|fact| fact.proof))
                .chain(
                    self.representative_ratio_proofs
                        .values()
                        .map(|fact| fact.proof),
                )
                .collect()
        };
        if parents.is_empty() {
            parents.push(self.provenance.scope_root());
        }
        let proof = self.provenance.derived(rule, parents);
        self.conflict_proof = Some(proof);
        PropagationOutcome::Pruned(conflict)
    }

    fn register_port(
        &mut self,
        variable: FlowVarId,
        signature: PortSignature,
        proof_context: ProofNodeId,
        conflict: &mut Option<PropagationConflict>,
    ) -> Result<(), PropagationError> {
        if !self.weighted.add_variable(variable) {
            return Err(PropagationError::DuplicateFlowVariable { variable });
        }
        let constraints = physical_flow_constraints(variable, &self.capacity);
        let bound_proof = self
            .provenance
            .derived(ConflictRule::ExactCapacity, [proof_context]);
        self.bound_proofs
            .extend(std::iter::repeat_n(bound_proof, constraints.len()));
        self.bounds.extend(constraints);
        if let Some(value) = &signature.known_flow {
            self.insert_sparse_row(constant_row(variable, value), proof_context);
            record_constraint_outcome(self.weighted.assign(variable, value)?, conflict);
        }
        self.registered_ports.insert(variable, signature);
        Ok(())
    }

    fn register_node(
        &mut self,
        topology: &TopologyState,
        node: &PhysicalNode,
        proof_context: ProofNodeId,
        conflict: &mut Option<PropagationConflict>,
    ) -> Result<(), PropagationError> {
        let output_variables = (0..node.node_type.output_port_count())
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
        let input_variables = (0..node.node_type.input_port_count())
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

        match node.node_type {
            NodeType::Splitter2 | NodeType::Splitter3 => {
                let input = input_variables[0];
                let first_output = output_variables[0];
                for &output in &output_variables[1..] {
                    self.insert_sparse_row(equality_row(output, first_output), proof_context);
                    record_constraint_outcome(
                        self.weighted
                            .relate(output, &Rational::one(), first_output)?,
                        conflict,
                    );
                }
                self.insert_sparse_row(
                    SparseRow::new(
                        std::iter::once((input, BigInt::one())).chain(
                            output_variables
                                .iter()
                                .copied()
                                .map(|output| (output, BigInt::from(-1))),
                        ),
                        BigInt::from(0),
                    ),
                    proof_context,
                );
                let arity = Rational::from(output_variables.len());
                record_constraint_outcome(
                    self.weighted.relate(input, &arity, first_output)?,
                    conflict,
                );
            }
            NodeType::Merger2 | NodeType::Merger3 => {
                let output = output_variables[0];
                self.insert_sparse_row(
                    SparseRow::new(
                        input_variables
                            .iter()
                            .copied()
                            .map(|input| (input, BigInt::one()))
                            .chain(std::iter::once((output, BigInt::from(-1)))),
                        BigInt::from(0),
                    ),
                    proof_context,
                );
            }
        }
        Ok(())
    }

    fn register_link(
        &mut self,
        topology: &TopologyState,
        link: &Link,
        proof_context: ProofNodeId,
        conflict: &mut Option<PropagationConflict>,
    ) -> Result<(), PropagationError> {
        let producer = producer_variable(topology, link.producer)?;
        let consumer = consumer_variable(topology, link.consumer)?;
        self.insert_sparse_row(equality_row(producer, consumer), proof_context);
        record_constraint_outcome(
            self.weighted.relate(producer, &Rational::one(), consumer)?,
            conflict,
        );
        Ok(())
    }

    fn propagate_fixed_point(&mut self) -> Result<Option<PropagationConflict>, PropagationError> {
        loop {
            self.refresh_fact_proofs()?;
            let known = self.known_big_rationals()?;
            let analysis = self
                .sparse
                .analyze_over(self.registered_ports.keys().copied(), &known)?;
            if analysis.consistency == Consistency::Inconsistent {
                return Ok(Some(PropagationConflict::SparseInconsistency));
            }

            let mut changed = false;
            for deduction in analysis.known_values {
                let value = Rational::from(deduction.value);
                match self.weighted.assign(deduction.variable, &value)? {
                    ConstraintOutcome::Changed => changed = true,
                    ConstraintOutcome::AlreadySatisfied => {}
                    ConstraintOutcome::Contradiction => {
                        return Ok(Some(PropagationConflict::ExactConstraintContradiction));
                    }
                }
            }
            for deduction in analysis.homogeneous_ratios {
                let factor = Rational::from(deduction.factor);
                match self
                    .weighted
                    .relate(deduction.lhs, &factor, deduction.rhs)?
                {
                    ConstraintOutcome::Changed => changed = true,
                    ConstraintOutcome::AlreadySatisfied => {}
                    ConstraintOutcome::Contradiction => {
                        return Ok(Some(PropagationConflict::ExactConstraintContradiction));
                    }
                }
            }
            if !changed {
                self.refresh_fact_proofs()?;
                return Ok(None);
            }
        }
    }

    fn check_exact_bounds(&mut self) -> Result<Option<PropagationConflict>, PropagationError> {
        self.refresh_fact_proofs()?;
        let mut known = BTreeMap::new();
        for &variable in self.registered_ports.keys() {
            let (representative, factor) = self.weighted.representative(variable)?;
            if factor.is_negative() {
                return Ok(Some(PropagationConflict::NegativeRatio {
                    variable,
                    representative,
                    factor,
                }));
            }
            if let Some(value) = self.weighted.known_value(variable)? {
                if !value.is_positive() {
                    return Ok(Some(PropagationConflict::NonPositiveKnown {
                        variable,
                        value,
                    }));
                }
                if value > self.capacity {
                    return Ok(Some(PropagationConflict::CapacityExceeded {
                        variable,
                        value,
                        capacity: self.capacity.clone(),
                    }));
                }
                known.insert(variable, value);
            }
        }

        for (constraint_index, constraint) in self.bounds.iter().enumerate() {
            if constraint.evaluate_known(&known) == InequalityEvaluation::Violated {
                return Ok(Some(PropagationConflict::ExactBoundViolation {
                    constraint_index,
                }));
            }
        }
        Ok(None)
    }

    fn known_big_rationals(&self) -> Result<BTreeMap<FlowVarId, BigRational>, PropagationError> {
        let mut known = BTreeMap::new();
        for &variable in self.registered_ports.keys() {
            if let Some(value) = self.weighted.known_value(variable)? {
                known.insert(variable, value.into_big_rational());
            }
        }
        Ok(known)
    }
}

fn record_constraint_outcome(
    outcome: ConstraintOutcome,
    conflict: &mut Option<PropagationConflict>,
) {
    if outcome == ConstraintOutcome::Contradiction && conflict.is_none() {
        *conflict = Some(PropagationConflict::ExactConstraintContradiction);
    }
}

fn collect_port_signatures(
    topology: &TopologyState,
) -> Result<BTreeMap<FlowVarId, PortSignature>, PropagationError> {
    let mut signatures = BTreeMap::new();
    for (&endpoint, port) in topology.producer_ports() {
        if signatures
            .insert(
                port.flow_var,
                PortSignature::new(PortEndpoint::Producer(endpoint), port),
            )
            .is_some()
        {
            return Err(PropagationError::DuplicateFlowVariable {
                variable: port.flow_var,
            });
        }
    }
    for (&endpoint, port) in topology.consumer_ports() {
        if signatures
            .insert(
                port.flow_var,
                PortSignature::new(PortEndpoint::Consumer(endpoint), port),
            )
            .is_some()
        {
            return Err(PropagationError::DuplicateFlowVariable {
                variable: port.flow_var,
            });
        }
    }
    Ok(signatures)
}

fn verify_component_batch(
    topology: &TopologyState,
    component: &Component,
    batch: &AppliedComponentBatch,
    registered_node_count: usize,
    registered_link_count: usize,
) -> Result<(), PropagationError> {
    let witness = component.canonical_witness();
    if batch.node_start != registered_node_count
        || batch.link_start != registered_link_count
        || batch.node_count != witness.nodes.len()
        || batch.link_count != witness.internal_links.len() + 1
        || batch.internal_link_count != component.internal_link_count()
        || batch.consumed_profile != component.profile()
        || topology.nodes().len() != batch.node_start + batch.node_count
        || topology.links().len() != batch.link_start + batch.link_count
        || batch.internal_link_indices
            != (batch.link_start..batch.link_start + witness.internal_links.len())
                .collect::<Vec<_>>()
        || batch.anchor_link_index != batch.link_start + witness.internal_links.len()
    {
        return Err(PropagationError::InvalidComponentCertificate(
            "batch ranges or physical costs disagree",
        ));
    }

    for (offset, local) in witness.nodes.iter().enumerate() {
        let physical = &topology.nodes()[batch.node_start + offset];
        if local.id.0 != u32::try_from(offset).unwrap_or(u32::MAX)
            || physical.id.0 != u32::try_from(batch.node_start + offset).unwrap_or(u32::MAX)
            || physical.node_type != local.node_type
        {
            return Err(PropagationError::InvalidComponentCertificate(
                "appended nodes do not match the canonical witness",
            ));
        }
    }
    let expected_inputs = witness
        .boundary_inputs
        .iter()
        .copied()
        .map(|port| remap_consumer(port, batch.node_start))
        .collect::<Result<Vec<_>, _>>()?;
    let expected_outputs = witness
        .boundary_outputs
        .iter()
        .copied()
        .map(|port| remap_producer(port, batch.node_start))
        .collect::<Result<Vec<_>, _>>()?;
    if batch.boundary_inputs != expected_inputs || batch.boundary_outputs != expected_outputs {
        return Err(PropagationError::InvalidComponentCertificate(
            "global boundary mapping disagrees with the canonical witness",
        ));
    }
    for (&index, local) in batch
        .internal_link_indices
        .iter()
        .zip(&witness.internal_links)
    {
        let physical = &topology.links()[index];
        if physical.producer != remap_producer(local.producer, batch.node_start)?
            || physical.consumer != remap_consumer(local.consumer, batch.node_start)?
        {
            return Err(PropagationError::InvalidComponentCertificate(
                "appended internal links do not match canonical K order",
            ));
        }
    }
    let anchor = &topology.links()[batch.anchor_link_index];
    let attaches_input = batch.boundary_inputs.contains(&anchor.consumer)
        && !is_appended_component_producer(anchor.producer, batch);
    let attaches_output = batch.boundary_outputs.contains(&anchor.producer)
        && !is_appended_component_consumer(anchor.consumer, batch);
    if attaches_input == attaches_output {
        return Err(PropagationError::InvalidComponentCertificate(
            "anchor must cross exactly one component boundary",
        ));
    }
    Ok(())
}

fn is_appended_component_producer(
    reference: ProducerPortRef,
    batch: &AppliedComponentBatch,
) -> bool {
    matches!(reference, ProducerPortRef::Node { node, .. }
    if usize::try_from(node.0).is_ok_and(|index| {
        (batch.node_start..batch.node_start + batch.node_count).contains(&index)
    }))
}

fn is_appended_component_consumer(
    reference: ConsumerPortRef,
    batch: &AppliedComponentBatch,
) -> bool {
    matches!(reference, ConsumerPortRef::Node { node, .. }
    if usize::try_from(node.0).is_ok_and(|index| {
        (batch.node_start..batch.node_start + batch.node_count).contains(&index)
    }))
}

fn remap_producer(
    reference: ProducerPortRef,
    node_start: usize,
) -> Result<ProducerPortRef, PropagationError> {
    let ProducerPortRef::Node { node, port } = reference else {
        return Err(PropagationError::InvalidComponentCertificate(
            "component producer endpoint is external",
        ));
    };
    let global = node_start
        .checked_add(usize::try_from(node.0).expect("u32 fits usize"))
        .and_then(|index| u32::try_from(index).ok())
        .ok_or(PropagationError::InvalidComponentCertificate(
            "component producer remap overflowed",
        ))?;
    Ok(ProducerPortRef::Node {
        node: NodeId(global),
        port,
    })
}

fn remap_consumer(
    reference: ConsumerPortRef,
    node_start: usize,
) -> Result<ConsumerPortRef, PropagationError> {
    let ConsumerPortRef::Node { node, port } = reference else {
        return Err(PropagationError::InvalidComponentCertificate(
            "component consumer endpoint is external",
        ));
    };
    let global = node_start
        .checked_add(usize::try_from(node.0).expect("u32 fits usize"))
        .and_then(|index| u32::try_from(index).ok())
        .ok_or(PropagationError::InvalidComponentCertificate(
            "component consumer remap overflowed",
        ))?;
    Ok(ConsumerPortRef::Node {
        node: NodeId(global),
        port,
    })
}

fn component_expression_row(
    target: FlowVarId,
    inputs: &[FlowVarId],
    coefficients: &[Rational],
) -> SparseRow {
    rational_sparse_row(
        std::iter::once((target, Rational::one())).chain(
            inputs
                .iter()
                .copied()
                .zip(coefficients.iter().map(|coefficient| -coefficient)),
        ),
    )
}

fn rational_sparse_row(terms: impl IntoIterator<Item = (FlowVarId, Rational)>) -> SparseRow {
    rational_sparse_affine_row(terms, &Rational::zero())
}

fn rational_sparse_affine_row(
    terms: impl IntoIterator<Item = (FlowVarId, Rational)>,
    rhs: &Rational,
) -> SparseRow {
    let terms = terms.into_iter().collect::<Vec<_>>();
    let denominator_lcm = terms
        .iter()
        .fold(rhs.denominator().clone(), |accumulator, (_, value)| {
            accumulator.lcm(value.denominator())
        });
    let rhs_multiplier = &denominator_lcm / rhs.denominator();
    SparseRow::new(
        terms.into_iter().map(|(variable, value)| {
            let multiplier = &denominator_lcm / value.denominator();
            (variable, value.numerator() * multiplier)
        }),
        rhs.numerator() * rhs_multiplier,
    )
}

fn producer_variable(
    topology: &TopologyState,
    endpoint: ProducerPortRef,
) -> Result<FlowVarId, PropagationError> {
    topology
        .producer_ports()
        .get(&endpoint)
        .map(|port| port.flow_var)
        .ok_or(PropagationError::MissingPort)
}

fn consumer_variable(
    topology: &TopologyState,
    endpoint: ConsumerPortRef,
) -> Result<FlowVarId, PropagationError> {
    topology
        .consumer_ports()
        .get(&endpoint)
        .map(|port| port.flow_var)
        .ok_or(PropagationError::MissingPort)
}

fn constant_row(variable: FlowVarId, value: &Rational) -> SparseRow {
    SparseRow::new(
        [(variable, value.denominator().clone())],
        value.numerator().clone(),
    )
}

fn equality_row(left: FlowVarId, right: FlowVarId) -> SparseRow {
    SparseRow::new(
        [(left, BigInt::one()), (right, BigInt::from(-1))],
        BigInt::from(0),
    )
}

#[cfg(test)]
mod tests {
    use solver_api::{DiscardTerminalIndex, NodeId, NodeProfile, Problem};

    use super::*;
    use crate::{
        canonical::{PartialLink, canonicalize_state},
        components::Component,
        problem::{NormalizedProblem, Preparation, prepare_problem},
        scc::{
            DeclaredBoundaryInput, DeclaredBoundaryOutput, FrozenSubsystemAnalysis,
            FrozenSubsystemDeclaration, analyze_frozen_subsystem,
        },
        topology::{ConsumerChoice, ProducerChoice, TopologyDecision},
    };

    fn rational(value: &str) -> Rational {
        value.parse().unwrap()
    }

    fn problem(inputs: &[&str], outputs: &[&str], capacity: &str) -> Problem {
        Problem {
            inputs: inputs.iter().map(|value| rational(value)).collect(),
            outputs: outputs.iter().map(|value| rational(value)).collect(),
            max_link_rate: rational(capacity),
        }
    }

    fn normalized(problem: &Problem) -> NormalizedProblem {
        match prepare_problem(problem).unwrap() {
            Preparation::Prepared(problem) => problem,
            Preparation::GloballyUnsat(proof) => {
                panic!("test problem unexpectedly globally unsatisfiable: {proof:?}")
            }
        }
    }

    // Fixture call sites deliberately transfer their one-shot DTO into the
    // reconstructed state, which keeps nested topology declarations legible.
    #[allow(clippy::needless_pass_by_value)]
    fn topology(partial: PartialTopology) -> TopologyState {
        TopologyState::from_partial_topology(&partial).unwrap()
    }

    fn partial(
        problem: Problem,
        nodes: Vec<PhysicalNode>,
        links: Vec<(ProducerPortRef, ConsumerPortRef)>,
    ) -> PartialTopology {
        PartialTopology {
            discard_count: 0,
            problem,
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
        }
    }

    fn node(id: u32, node_type: NodeType) -> PhysicalNode {
        PhysicalNode {
            id: NodeId(id),
            node_type,
        }
    }

    fn producer_node(node: u32, port: u8) -> ProducerPortRef {
        ProducerPortRef::Node {
            node: NodeId(node),
            port,
        }
    }

    fn consumer_node(node: u32, port: u8) -> ConsumerPortRef {
        ConsumerPortRef::Node {
            node: NodeId(node),
            port,
        }
    }

    fn all_variables(topology: &TopologyState) -> Vec<FlowVarId> {
        let mut variables = topology
            .producer_ports()
            .values()
            .chain(topology.consumer_ports().values())
            .map(|port| port.flow_var)
            .collect::<Vec<_>>();
        variables.sort_unstable();
        variables
    }

    fn splitter_component() -> Component {
        let physical = topology(partial(
            problem(&["2"], &["1", "1"], "10"),
            vec![node(0, NodeType::Splitter2)],
            Vec::new(),
        ));
        let analysis = analyze_frozen_subsystem(
            &physical,
            &FrozenSubsystemDeclaration {
                nodes: vec![NodeId(0)],
                boundary_inputs: vec![DeclaredBoundaryInput {
                    port: consumer_node(0, 0),
                }],
                boundary_outputs: vec![
                    DeclaredBoundaryOutput {
                        port: producer_node(0, 0),
                    },
                    DeclaredBoundaryOutput {
                        port: producer_node(0, 1),
                    },
                ],
            },
        )
        .unwrap();
        let FrozenSubsystemAnalysis::Symbolic(frozen) = analysis else {
            panic!("splitter must have a symbolic frozen contract");
        };
        Component::from_frozen(*frozen).unwrap()
    }

    fn assert_semantically_equal(
        left: &PropagationState,
        right: &PropagationState,
        topology: &TopologyState,
    ) {
        assert_eq!(left.current_outcome(), right.current_outcome());
        for variable in all_variables(topology) {
            assert_eq!(
                left.known_value(variable).unwrap(),
                right.known_value(variable).unwrap()
            );
        }
        assert_eq!(
            left.partial_topology_with_known_link_flows(topology)
                .unwrap(),
            right
                .partial_topology_with_known_link_flows(topology)
                .unwrap()
        );
    }

    fn staged_scc_consequences(
        capacity: &str,
    ) -> (TopologyState, PropagationState, [FlowVarId; 3]) {
        let specification = problem(&["1"], &["1"], capacity);
        let topology = topology(partial(
            specification.clone(),
            vec![node(0, NodeType::Merger2)],
            Vec::new(),
        ));
        let mut propagation =
            PropagationState::new(&topology, &specification.max_link_rate).unwrap();
        let first = topology.consumer_ports()[&consumer_node(0, 0)].flow_var;
        let second = topology.consumer_ports()[&consumer_node(0, 1)].flow_var;
        let output = topology.producer_ports()[&producer_node(0, 0)].flow_var;

        // Stage the authoritative rows without running the ordinary fixed
        // point. This models the instant after an SCC elimination proved its
        // consequences and lets the focused test observe their promotion.
        let proof = propagation.decision_context;
        propagation.insert_sparse_row(constant_row(second, &Rational::one()), proof);
        propagation.insert_sparse_row(
            SparseRow::new(
                [(first, BigInt::one()), (second, BigInt::from(-2))],
                BigInt::from(0),
            ),
            proof,
        );
        (topology, propagation, [first, second, output])
    }

    #[test]
    fn open_scc_deductions_reenter_the_ordinary_fixed_point_and_rollback() {
        let (_topology, mut propagation, [first, second, output]) = staged_scc_consequences("4");
        let baseline = propagation.clone();
        let checkpoint = propagation.checkpoint();

        let update = propagation
            .promote_open_scc_deductions(
                &[(second, Rational::one())],
                &[(first, second, Rational::from(2))],
            )
            .unwrap();
        assert!(update.changed);
        assert_eq!(update.outcome, PropagationOutcome::Feasible);
        assert_eq!(
            propagation.known_value(first).unwrap(),
            Some(Rational::from(2))
        );
        assert_eq!(
            propagation.known_value(output).unwrap(),
            Some(Rational::from(3)),
            "the merger equation must run after the SCC facts are promoted"
        );

        propagation.rollback(checkpoint);
        assert_eq!(propagation, baseline);
    }

    #[test]
    fn open_scc_deductions_enforce_exact_physical_capacity() {
        let (_topology, mut propagation, [first, second, output]) = staged_scc_consequences("2");
        let update = propagation
            .promote_open_scc_deductions(
                &[(second, Rational::one())],
                &[(first, second, Rational::from(2))],
            )
            .unwrap();
        assert!(matches!(
            update.outcome,
            PropagationOutcome::Pruned(PropagationConflict::CapacityExceeded {
                variable,
                value,
                capacity,
            }) if variable == output && value == Rational::from(3) && capacity == Rational::from(2)
        ));
        assert!(propagation.conflict_provenance().is_some());
    }

    #[test]
    fn splitter_equal_outputs_and_conservation_reach_a_fixed_point() {
        let specification = problem(&["6"], &["3", "3"], "6");
        let topology = topology(partial(
            specification.clone(),
            vec![node(0, NodeType::Splitter2)],
            vec![(
                ProducerPortRef::Input(solver_api::InputTerminalIndex(0)),
                consumer_node(0, 0),
            )],
        ));
        let propagation = PropagationState::new(&topology, &specification.max_link_rate).unwrap();
        assert_eq!(propagation.current_outcome(), &PropagationOutcome::Feasible);
        for port in [producer_node(0, 0), producer_node(0, 1)] {
            let variable = topology.producer_ports()[&port].flow_var;
            assert_eq!(
                propagation.known_value(variable).unwrap(),
                Some(Rational::from(3))
            );
        }
        assert!(propagation.sparse.rows().len() >= 6);
    }

    #[test]
    fn derived_values_and_ratios_retain_a_stable_parent_chain() {
        let specification = problem(&["6"], &["3", "3"], "6");
        let topology = topology(partial(
            specification.clone(),
            vec![node(0, NodeType::Splitter2)],
            vec![(
                ProducerPortRef::Input(solver_api::InputTerminalIndex(0)),
                consumer_node(0, 0),
            )],
        ));
        let propagation = PropagationState::new(&topology, &specification.max_link_rate).unwrap();
        let output = topology.producer_ports()[&producer_node(0, 0)].flow_var;

        let value_proof = propagation
            .known_value_provenance(output)
            .expect("splitter output is exactly known");
        let ratio_proof = propagation
            .representative_ratio_provenance(output)
            .expect("splitter output has a representative relation");
        for proof in [&value_proof, &ratio_proof] {
            assert_eq!(proof.decisions(), vec![crate::topology::DecisionId(0)]);
            assert!(matches!(
                proof.nodes[usize::try_from(proof.root.0).unwrap()],
                crate::no_good::ProvenanceNode::Derived {
                    rule: ConflictRule::ExactPropagation,
                    ..
                }
            ));
            assert!(proof.nodes.iter().any(|node| matches!(
                node,
                crate::no_good::ProvenanceNode::Derived {
                    rule: ConflictRule::ExactPropagation,
                    parents
                } if parents.iter().any(|parent| matches!(
                    proof.nodes[usize::try_from(parent.0).unwrap()],
                    crate::no_good::ProvenanceNode::Derived { .. }
                ))
            )));
        }
    }

    #[test]
    fn component_summary_matches_primitive_rebuild_and_rolls_back() {
        let specification = problem(&["2"], &["1", "1"], "2");
        let normalized = normalized(&specification);
        let profile = NodeProfile {
            splitter2: 1,
            ..NodeProfile::default()
        };
        let component = splitter_component();
        let mut topology = TopologyState::new(&normalized, profile).unwrap();
        let mut macro_propagation =
            PropagationState::new(&topology, &specification.max_link_rate).unwrap();
        let original_topology = topology.clone();
        let original_propagation = macro_propagation.clone();
        let topology_checkpoint = topology.checkpoint();
        let propagation_checkpoint = macro_propagation.checkpoint();

        let attachment = topology.component_attachments(&component)[0];
        let batch = topology.apply_component(&component, attachment).unwrap();
        let macro_outcome = macro_propagation
            .synchronize_after_component_batch(&topology, &component, &batch)
            .unwrap();
        assert_eq!(macro_outcome, PropagationOutcome::Feasible);

        let primitive = PropagationState::new(&topology, &specification.max_link_rate).unwrap();
        assert_semantically_equal(&macro_propagation, &primitive, &topology);
        let macro_snapshot = macro_propagation
            .partial_topology_with_known_link_flows(&topology)
            .unwrap();
        let primitive_snapshot = primitive
            .partial_topology_with_known_link_flows(&topology)
            .unwrap();
        assert_eq!(macro_snapshot, primitive_snapshot);
        assert_eq!(
            canonicalize_state(&macro_snapshot),
            canonicalize_state(&primitive_snapshot)
        );

        macro_propagation.rollback(propagation_checkpoint);
        topology.rollback(topology_checkpoint);
        assert_eq!(topology, original_topology);
        assert_eq!(macro_propagation, original_propagation);
    }

    #[test]
    fn merger_conservation_combines_exact_input_constants() {
        let specification = problem(&["2", "4"], &["6"], "6");
        let topology = topology(partial(
            specification.clone(),
            vec![node(0, NodeType::Merger2)],
            vec![
                (
                    ProducerPortRef::Input(solver_api::InputTerminalIndex(0)),
                    consumer_node(0, 0),
                ),
                (
                    ProducerPortRef::Input(solver_api::InputTerminalIndex(1)),
                    consumer_node(0, 1),
                ),
            ],
        ));
        let propagation = PropagationState::new(&topology, &specification.max_link_rate).unwrap();
        let output = topology.producer_ports()[&producer_node(0, 0)].flow_var;
        assert_eq!(
            propagation.known_value(output).unwrap(),
            Some(Rational::from(6))
        );
    }

    #[test]
    fn checkpoint_restores_rows_union_find_bounds_prefixes_and_outcome() {
        let specification = problem(&["6"], &["3", "3"], "6");
        let root = topology(partial(specification.clone(), vec![], vec![]));
        let mut propagation = PropagationState::new(&root, &specification.max_link_rate).unwrap();
        let before = propagation.clone();
        let checkpoint = propagation.checkpoint();

        let extended = topology(partial(
            specification.clone(),
            vec![node(0, NodeType::Splitter2)],
            vec![(
                ProducerPortRef::Input(solver_api::InputTerminalIndex(0)),
                consumer_node(0, 0),
            )],
        ));
        propagation
            .synchronize_after_topology_mutation(&extended)
            .unwrap();
        assert_ne!(propagation, before);
        propagation.rollback(checkpoint);
        assert_eq!(propagation, before);
        assert_eq!(propagation.current_outcome(), &PropagationOutcome::Feasible);
    }

    #[test]
    fn rollback_and_replay_restore_fact_provenance_exactly() {
        let specification = problem(&["6"], &["3", "3"], "6");
        let mut topology = TopologyState::new(
            &normalized(&specification),
            NodeProfile {
                splitter2: 1,
                ..NodeProfile::default()
            },
        )
        .unwrap();
        let mut propagation =
            PropagationState::new(&topology, &specification.max_link_rate).unwrap();
        let decision = topology
            .legal_decisions()
            .into_iter()
            .find(|decision| {
                matches!(
                    decision,
                    TopologyDecision {
                        consumer: ConsumerChoice::NewNode {
                            node_type: NodeType::Splitter2,
                            ..
                        },
                        ..
                    } | TopologyDecision {
                        producer: ProducerChoice::NewNode {
                            node_type: NodeType::Splitter2,
                            ..
                        },
                        ..
                    }
                )
            })
            .expect("an external frontier can materialize the splitter");
        let topology_checkpoint = topology.checkpoint();
        let propagation_checkpoint = propagation.checkpoint();

        topology.apply(decision).unwrap();
        propagation
            .synchronize_after_topology_mutation(&topology)
            .unwrap();
        let output = topology.producer_ports()[&producer_node(0, 0)].flow_var;
        let first = propagation
            .known_value_provenance(output)
            .expect("splitter output is known after the decision");

        propagation.rollback(propagation_checkpoint);
        topology.rollback(topology_checkpoint);
        assert!(propagation.known_value_provenance(output).is_none());

        topology.apply(decision).unwrap();
        propagation
            .synchronize_after_topology_mutation(&topology)
            .unwrap();
        assert_eq!(
            propagation
                .known_value_provenance(output)
                .expect("replayed splitter output is known"),
            first
        );
    }

    #[test]
    fn exact_capacity_is_allowed_and_any_excess_is_pruned() {
        let specification = problem(&["4"], &["4"], "4");
        let direct = topology(partial(
            specification.clone(),
            vec![],
            vec![(
                ProducerPortRef::Input(solver_api::InputTerminalIndex(0)),
                ConsumerPortRef::Output(solver_api::OutputTerminalIndex(0)),
            )],
        ));
        assert_eq!(
            PropagationState::new(&direct, &rational("4"))
                .unwrap()
                .current_outcome(),
            &PropagationOutcome::Feasible
        );
        assert!(matches!(
            PropagationState::new(
                &direct,
                &rational("399999999999999999999/100000000000000000000")
            )
            .unwrap()
            .current_outcome(),
            PropagationOutcome::Pruned(PropagationConflict::CapacityExceeded { .. })
        ));

        let over_capacity = PropagationState::new(
            &direct,
            &rational("399999999999999999999/100000000000000000000"),
        )
        .unwrap();
        let proof = over_capacity
            .conflict_provenance()
            .expect("capacity prune retains its exact proof");
        assert_eq!(proof.decisions(), vec![crate::topology::DecisionId(0)]);
        assert!(matches!(
            proof.nodes[usize::try_from(proof.root.0).unwrap()],
            crate::no_good::ProvenanceNode::Derived {
                rule: ConflictRule::ExactCapacity,
                ..
            }
        ));
    }

    #[test]
    fn one_discard_is_assigned_the_exact_surplus() {
        let specification = problem(&["1", "1"], &["1"], "1");
        let normalized = normalized(&specification);
        let topology =
            TopologyState::new_with_discards(&normalized, NodeProfile::default(), 1).unwrap();
        let propagation = PropagationState::new(&topology, &normalized.max_link_rate).unwrap();
        let discard =
            topology.consumer_ports()[&ConsumerPortRef::Discard(DiscardTerminalIndex(0))].flow_var;
        assert_eq!(
            propagation.known_value(discard).unwrap(),
            Some(Rational::one())
        );
        assert_eq!(propagation.current_outcome(), &PropagationOutcome::Feasible);
    }

    #[test]
    fn minimum_capacity_partition_and_discard_rollback_are_exact() {
        let specification = problem(&["1", "1", "1"], &["1"], "1");
        let normalized = normalized(&specification);
        let mut topology =
            TopologyState::new_with_discards(&normalized, NodeProfile::default(), 2).unwrap();
        let mut propagation = PropagationState::new(&topology, &normalized.max_link_rate).unwrap();
        let original_topology = topology.clone();
        let original_propagation = propagation.clone();
        let topology_checkpoint = topology.checkpoint();
        let propagation_checkpoint = propagation.checkpoint();

        while !topology.is_complete() {
            let decision = topology.legal_decisions()[0];
            topology.apply(decision).unwrap();
            assert_eq!(
                propagation
                    .synchronize_after_topology_mutation(&topology)
                    .unwrap(),
                PropagationOutcome::Feasible
            );
        }
        for index in 0..2 {
            let variable = topology.consumer_ports()
                [&ConsumerPortRef::Discard(DiscardTerminalIndex(index))]
                .flow_var;
            assert_eq!(
                propagation.known_value(variable).unwrap(),
                Some(Rational::one())
            );
        }

        propagation.rollback(propagation_checkpoint);
        topology.rollback(topology_checkpoint);
        assert_eq!(propagation, original_propagation);
        assert_eq!(topology, original_topology);
    }

    #[test]
    fn too_few_discard_lines_are_proven_over_capacity() {
        let specification = problem(&["1", "1", "1"], &["1"], "1");
        let normalized = normalized(&specification);
        let topology =
            TopologyState::new_with_discards(&normalized, NodeProfile::default(), 1).unwrap();
        assert!(matches!(
            PropagationState::new(&topology, &normalized.max_link_rate)
                .unwrap()
                .current_outcome(),
            PropagationOutcome::Pruned(PropagationConflict::CapacityExceeded { value, capacity, .. })
                if *value == Rational::from(2) && *capacity == Rational::one()
        ));
    }

    #[test]
    fn splitter_cycle_forces_zero_and_positivity_prunes() {
        let specification = problem(&["1"], &["1"], "10");
        let cyclic = topology(partial(
            specification.clone(),
            vec![node(0, NodeType::Splitter2), node(1, NodeType::Splitter2)],
            vec![
                (producer_node(0, 0), consumer_node(1, 0)),
                (producer_node(1, 0), consumer_node(0, 0)),
            ],
        ));
        assert!(matches!(
            PropagationState::new(&cyclic, &specification.max_link_rate)
                .unwrap()
                .current_outcome(),
            PropagationOutcome::Pruned(PropagationConflict::NonPositiveKnown { value, .. })
                if value.is_zero()
        ));
    }

    #[test]
    fn promoted_negative_ratio_is_an_exact_positivity_impossibility() {
        let specification = problem(&["1"], &["1"], "10");
        let topology = topology(partial(
            specification.clone(),
            vec![node(0, NodeType::Merger2)],
            vec![],
        ));
        let mut propagation =
            PropagationState::new(&topology, &specification.max_link_rate).unwrap();
        let left = topology.consumer_ports()[&consumer_node(0, 0)].flow_var;
        let right = topology.consumer_ports()[&consumer_node(0, 1)].flow_var;
        let scope_root = propagation.provenance.scope_root();
        let proof = propagation
            .provenance
            .derived(ConflictRule::ExactPropagation, [scope_root]);
        propagation.insert_sparse_row(
            SparseRow::new(
                [(left, BigInt::one()), (right, BigInt::one())],
                BigInt::from(0),
            ),
            proof,
        );
        assert_eq!(propagation.propagate_fixed_point().unwrap(), None);
        assert!(
            propagation
                .weighted
                .ratio(left, right)
                .unwrap()
                .unwrap()
                .is_negative()
        );
        assert!(matches!(
            propagation.check_exact_bounds().unwrap(),
            Some(
                PropagationConflict::NegativeRatio { .. }
                    | PropagationConflict::NonPositiveKnown { .. }
            )
        ));
    }

    #[test]
    fn arbitrarily_large_rational_scale_stays_exact_in_link_snapshot() {
        let huge = "100000000000000000000000000000000000000000000000003/7";
        let specification = problem(&[huge], &[huge], huge);
        let direct = topology(partial(
            specification.clone(),
            vec![],
            vec![(
                ProducerPortRef::Input(solver_api::InputTerminalIndex(0)),
                ConsumerPortRef::Output(solver_api::OutputTerminalIndex(0)),
            )],
        ));
        let propagation = PropagationState::new(&direct, &specification.max_link_rate).unwrap();
        assert_eq!(
            propagation
                .partial_topology_with_known_link_flows(&direct)
                .unwrap()
                .links[0]
                .flow,
            Some(rational(huge))
        );
    }

    fn splitter_prefixes(specification: &Problem) -> Vec<TopologyState> {
        let root = topology(partial(specification.clone(), vec![], vec![]));
        let node_only_link = vec![(
            ProducerPortRef::Input(solver_api::InputTerminalIndex(0)),
            consumer_node(0, 0),
        )];
        vec![
            root,
            topology(partial(
                specification.clone(),
                vec![node(0, NodeType::Splitter2)],
                node_only_link.clone(),
            )),
            topology(partial(
                specification.clone(),
                vec![node(0, NodeType::Splitter2)],
                node_only_link
                    .iter()
                    .copied()
                    .chain([(
                        producer_node(0, 0),
                        ConsumerPortRef::Output(solver_api::OutputTerminalIndex(0)),
                    )])
                    .collect(),
            )),
            topology(partial(
                specification.clone(),
                vec![node(0, NodeType::Splitter2)],
                vec![
                    (
                        ProducerPortRef::Input(solver_api::InputTerminalIndex(0)),
                        consumer_node(0, 0),
                    ),
                    (
                        producer_node(0, 0),
                        ConsumerPortRef::Output(solver_api::OutputTerminalIndex(0)),
                    ),
                    (
                        producer_node(0, 1),
                        ConsumerPortRef::Output(solver_api::OutputTerminalIndex(1)),
                    ),
                ],
            )),
        ]
    }

    #[test]
    fn random_prefix_incremental_and_rebuild_propagation_agree_after_rollback() {
        let specification = problem(&["6"], &["3", "3"], "6");
        let prefixes = splitter_prefixes(&specification);
        for seed in 0_u64..24 {
            let highest = 1 + usize::try_from(seed % 3).unwrap();
            let rollback_to =
                usize::try_from((seed * 17 + 5) % u64::try_from(highest).unwrap()).unwrap();
            let mut incremental =
                PropagationState::new(&prefixes[0], &specification.max_link_rate).unwrap();
            let mut checkpoints = vec![incremental.checkpoint()];
            for topology in &prefixes[1..=highest] {
                incremental
                    .synchronize_after_topology_mutation(topology)
                    .unwrap();
                let rebuilt =
                    PropagationState::new(topology, &specification.max_link_rate).unwrap();
                assert_semantically_equal(&incremental, &rebuilt, topology);
                checkpoints.push(incremental.checkpoint());
            }

            incremental.rollback(checkpoints[rollback_to].clone());
            let rebuilt =
                PropagationState::new(&prefixes[rollback_to], &specification.max_link_rate)
                    .unwrap();
            assert_semantically_equal(&incremental, &rebuilt, &prefixes[rollback_to]);
            for topology in &prefixes[rollback_to + 1..=highest] {
                incremental
                    .synchronize_after_topology_mutation(topology)
                    .unwrap();
            }
            let rebuilt =
                PropagationState::new(&prefixes[highest], &specification.max_link_rate).unwrap();
            assert_semantically_equal(&incremental, &rebuilt, &prefixes[highest]);
        }
    }

    #[test]
    fn public_constructor_works_with_a_real_lazy_topology_mutation() {
        let specification = problem(&["6"], &["3", "3"], "6");
        let mut topology = TopologyState::new(
            &normalized(&specification),
            NodeProfile {
                splitter2: 1,
                ..NodeProfile::default()
            },
        )
        .unwrap();
        let mut propagation =
            PropagationState::new(&topology, &specification.max_link_rate).unwrap();
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
                )
            })
            .or_else(|| {
                topology.legal_decisions().into_iter().find(|decision| {
                    matches!(
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
            })
            .unwrap();
        topology.apply(decision).unwrap();
        assert_eq!(
            propagation
                .synchronize_after_topology_mutation(&topology)
                .unwrap(),
            PropagationOutcome::Feasible
        );
        assert_eq!(propagation.registered_nodes.len(), 1);
        assert_eq!(propagation.registered_links.len(), 1);
    }
}
