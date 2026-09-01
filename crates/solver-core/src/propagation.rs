//! Rollback exact propagation for one incrementally materialized topology.
//!
//! Sparse integer rows are the authoritative statement of physical equality
//! semantics. The weighted union-find is only a fast representation of proven
//! one-variable values and two-variable ratios. Every promotion comes either
//! directly from a physical equation or from an equivalent fraction-free row
//! basis. Consequently, a promotion cannot remove a valid solution.
//!
//! Pruning is deliberately one-sided. This module reports `Pruned` only after
//! an exact equation contradiction, an exact nonpositive/over-capacity known
//! flow, or a negative exact ratio between two variables that must both be
//! positive. Unresolved equations and inequalities always remain feasible.

use std::{collections::BTreeMap, time::Instant};

use num::{BigInt, BigRational, Integer, One};
use solver_api::{ConsumerPortRef, NodeType, PhysicalNode, ProducerPortRef, Rational};
use thiserror::Error;

use crate::{
    algebra::{
        sparse::{Consistency, RowCheckpoint, SparseAlgebraError, SparseRow, SparseSystem},
        weighted::{ConstraintOutcome, WeightedCheckpoint, WeightedError, WeightedUnionFind},
    },
    canonical::PartialTopology,
    hotspot_profile::{self, PropagationPhase, SparsePassCause},
    topology::{Direction, FlowVarId, Link, Port, PortClass, PortOwner, TopologyState},
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
    port_len: usize,
    node_len: usize,
    link_len: usize,
    outcome: PropagationOutcome,
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
    registered_ports: BTreeMap<FlowVarId, PortSignature>,
    registered_nodes: Vec<PhysicalNode>,
    registered_links: Vec<Link>,
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
        let mut state = Self {
            capacity: capacity.clone(),
            weighted: WeightedUnionFind::new(),
            sparse: SparseSystem::new(),
            registered_ports: BTreeMap::new(),
            registered_nodes: Vec::new(),
            registered_links: Vec::new(),
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
            port_len: self.registered_ports.len(),
            node_len: self.registered_nodes.len(),
            link_len: self.registered_links.len(),
            outcome: self.outcome.clone(),
        }
    }

    /// Restores all facts, rows, registered prefixes, and proof status.
    ///
    /// # Panics
    ///
    /// Panics for a checkpoint beyond the current state, including a stale
    /// checkpoint reused after rolling back past it.
    pub fn rollback(&mut self, checkpoint: PropagationCheckpoint) {
        assert!(checkpoint.port_len <= self.registered_ports.len());
        assert!(checkpoint.node_len <= self.registered_nodes.len());
        assert!(checkpoint.link_len <= self.registered_links.len());

        self.weighted.rollback(checkpoint.weighted);
        self.sparse.rollback(checkpoint.sparse);
        while self.registered_ports.len() > checkpoint.port_len {
            self.registered_ports
                .pop_last()
                .expect("registered port length was checked");
        }
        self.registered_nodes.truncate(checkpoint.node_len);
        self.registered_links.truncate(checkpoint.link_len);
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
        hotspot_profile::record_propagation_sync_call();
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
        let port_scan_started = hotspot_profile::recorder_enabled().then(Instant::now);
        let current_ports = collect_port_signatures(topology)?;
        self.validate_registered_prefix(topology, &current_ports)?;
        record_propagation_elapsed(port_scan_started, PropagationPhase::PortScan);

        let register_started = hotspot_profile::recorder_enabled().then(Instant::now);
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
            self.register_port(variable, signature.clone(), &mut direct_conflict)?;
        }

        for node in &topology.nodes()[self.registered_nodes.len()..] {
            self.register_node(topology, node, &mut direct_conflict)?;
            self.registered_nodes.push(node.clone());
        }
        for link in &topology.links()[self.registered_links.len()..] {
            self.register_link(topology, link, &mut direct_conflict)?;
            self.registered_links.push(link.clone());
        }
        record_propagation_elapsed(register_started, PropagationPhase::Register);

        if let Some(conflict) = direct_conflict {
            return Ok(PropagationOutcome::Pruned(conflict));
        }
        if let Some(conflict) = self.propagate_fixed_point()? {
            return Ok(PropagationOutcome::Pruned(conflict));
        }
        Ok(match self.check_exact_bounds()? {
            Some(conflict) => PropagationOutcome::Pruned(conflict),
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
            PropagationOutcome::Pruned(conflict)
        } else if let Some(conflict) = self.check_exact_bounds()? {
            PropagationOutcome::Pruned(conflict)
        } else {
            PropagationOutcome::Feasible
        };
        Ok(SccPropagationUpdate { changed, outcome })
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
        self.insert_sparse_row(rational_sparse_affine_row(
            discard_variables
                .into_iter()
                .map(|variable| (variable, Rational::one())),
            &surplus,
        ));
        self.outcome = if let Some(conflict) = self.propagate_fixed_point()? {
            PropagationOutcome::Pruned(conflict)
        } else {
            match self.check_exact_bounds()? {
                Some(conflict) => PropagationOutcome::Pruned(conflict),
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

    fn insert_sparse_row(&mut self, row: SparseRow) {
        let _ = self.sparse.insert(row);
    }

    fn register_port(
        &mut self,
        variable: FlowVarId,
        signature: PortSignature,
        conflict: &mut Option<PropagationConflict>,
    ) -> Result<(), PropagationError> {
        if !self.weighted.add_variable(variable) {
            return Err(PropagationError::DuplicateFlowVariable { variable });
        }
        if let Some(value) = &signature.known_flow {
            self.insert_sparse_row(constant_row(variable, value));
            record_constraint_outcome(self.weighted.assign(variable, value)?, conflict);
        }
        self.registered_ports.insert(variable, signature);
        Ok(())
    }

    fn register_node(
        &mut self,
        topology: &TopologyState,
        node: &PhysicalNode,
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
                    self.insert_sparse_row(equality_row(output, first_output));
                    record_constraint_outcome(
                        self.weighted
                            .relate(output, &Rational::one(), first_output)?,
                        conflict,
                    );
                }
                self.insert_sparse_row(SparseRow::new(
                    std::iter::once((input, BigInt::one())).chain(
                        output_variables
                            .iter()
                            .copied()
                            .map(|output| (output, BigInt::from(-1))),
                    ),
                    BigInt::from(0),
                ));
                let arity = Rational::from(output_variables.len());
                record_constraint_outcome(
                    self.weighted.relate(input, &arity, first_output)?,
                    conflict,
                );
            }
            NodeType::Merger2 | NodeType::Merger3 => {
                let output = output_variables[0];
                self.insert_sparse_row(SparseRow::new(
                    input_variables
                        .iter()
                        .copied()
                        .map(|input| (input, BigInt::one()))
                        .chain(std::iter::once((output, BigInt::from(-1)))),
                    BigInt::from(0),
                ));
            }
        }
        Ok(())
    }

    fn register_link(
        &mut self,
        topology: &TopologyState,
        link: &Link,
        conflict: &mut Option<PropagationConflict>,
    ) -> Result<(), PropagationError> {
        let producer = producer_variable(topology, link.producer)?;
        let consumer = consumer_variable(topology, link.consumer)?;
        self.insert_sparse_row(equality_row(producer, consumer));
        record_constraint_outcome(
            self.weighted.relate(producer, &Rational::one(), consumer)?,
            conflict,
        );
        Ok(())
    }

    fn propagate_fixed_point(&mut self) -> Result<Option<PropagationConflict>, PropagationError> {
        let started = hotspot_profile::recorder_enabled().then(Instant::now);
        let result = self.propagate_fixed_point_inner();
        record_propagation_elapsed(started, PropagationPhase::FixedPoint);
        result
    }

    fn propagate_fixed_point_inner(
        &mut self,
    ) -> Result<Option<PropagationConflict>, PropagationError> {
        let mut pass_cause = SparsePassCause::Initial;
        loop {
            hotspot_profile::record_propagation_fixed_point_pass();
            let known = self.known_big_rationals()?;
            let profiling_enabled = hotspot_profile::recorder_enabled();
            let analyze_started = profiling_enabled.then(Instant::now);
            let analysis = if profiling_enabled {
                let (analysis, profile) = self.sparse.analyze_over_complete_profiled(
                    self.registered_ports.keys().copied(),
                    &known,
                )?;
                hotspot_profile::record_sparse_profile(&profile, pass_cause);
                analysis
            } else {
                self.sparse
                    .analyze_over_complete(self.registered_ports.keys().copied(), &known)?
            };
            record_propagation_elapsed(analyze_started, PropagationPhase::SparseAnalyze);
            if analysis.consistency == Consistency::Inconsistent {
                return Ok(Some(PropagationConflict::SparseInconsistency));
            }

            let mut value_changed = false;
            for deduction in analysis.known_values {
                let value = Rational::from(deduction.value);
                match self.weighted.assign(deduction.variable, &value)? {
                    ConstraintOutcome::Changed => value_changed = true,
                    ConstraintOutcome::AlreadySatisfied => {}
                    ConstraintOutcome::Contradiction => {
                        return Ok(Some(PropagationConflict::ExactConstraintContradiction));
                    }
                }
            }
            let mut ratio_changed = false;
            for deduction in analysis.homogeneous_ratios {
                let factor = Rational::from(deduction.factor);
                match self
                    .weighted
                    .relate(deduction.lhs, &factor, deduction.rhs)?
                {
                    ConstraintOutcome::Changed => ratio_changed = true,
                    ConstraintOutcome::AlreadySatisfied => {}
                    ConstraintOutcome::Contradiction => {
                        return Ok(Some(PropagationConflict::ExactConstraintContradiction));
                    }
                }
            }
            pass_cause = match (value_changed, ratio_changed) {
                (true, true) => SparsePassCause::ValueAndRatioDeductions,
                (true, false) => SparsePassCause::ValueDeductions,
                (false, true) => SparsePassCause::RatioDeductions,
                (false, false) => return Ok(None),
            };
        }
    }

    fn check_exact_bounds(&mut self) -> Result<Option<PropagationConflict>, PropagationError> {
        let started = hotspot_profile::recorder_enabled().then(Instant::now);
        let result = self.check_exact_bounds_inner();
        record_propagation_elapsed(started, PropagationPhase::Bounds);
        result
    }

    fn check_exact_bounds_inner(
        &mut self,
    ) -> Result<Option<PropagationConflict>, PropagationError> {
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

fn record_propagation_elapsed(started: Option<Instant>, phase: PropagationPhase) {
    if let Some(started) = started {
        hotspot_profile::record_propagation_phase(started.elapsed(), phase);
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
        canonical::PartialLink,
        problem::{NormalizedProblem, Preparation, prepare_problem},
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
        propagation.insert_sparse_row(constant_row(second, &Rational::one()));
        propagation.insert_sparse_row(SparseRow::new(
            [(first, BigInt::one()), (second, BigInt::from(-2))],
            BigInt::from(0),
        ));
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
    fn checkpoint_restores_rows_union_find_prefixes_and_outcome() {
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
        propagation.insert_sparse_row(SparseRow::new(
            [(left, BigInt::one()), (right, BigInt::one())],
            BigInt::from(0),
        ));
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
}
