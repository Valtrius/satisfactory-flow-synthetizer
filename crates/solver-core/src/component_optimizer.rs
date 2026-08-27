//! Exact application-specific component optimizer.
//!
//! This deliberately simple primitive enumerator is the completeness path for
//! Milestone 7 component work. It enumerates every labeled physical port
//! bijection for each feasible node profile, quotients completed topologies,
//! solves and validates them independently, and stops only after the first
//! equal-cost group is fully exhausted. Component macros may later add faster
//! construction paths, but are not needed by this proof.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::{AtomicBool, Ordering},
};

use solver_api::{
    ConsumerPortRef, InputTerminalIndex, NodeId, NodeProfile, NodeType, OutputTerminalIndex,
    PhysicalGraph, PhysicalLink, PhysicalNode, Problem, ProducerPortRef, Rational,
};
use solver_validation::{ValidationError, solve_topology, validate_solution};
use thiserror::Error;

use crate::{
    canonical::{PartialLink, PartialTopology, canonicalize_witness},
    component_application::{
        ApplicationOptimalityProof, ComponentApplicationKey, ComponentApplicationMatch,
        ComponentProfileProof, match_component_application,
    },
    components::{
        Component, ComponentApplicationError, ComponentCanonicalKey, ComponentCost, ComponentError,
    },
    scc::{
        DeclaredBoundaryInput, DeclaredBoundaryOutput, FrozenSubsystemAnalysis,
        FrozenSubsystemDeclaration, analyze_frozen_subsystem,
    },
    topology::TopologyState,
};

/// Internal failure while proving one concrete component optimum.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ComponentOptimizationError {
    /// The supplied upper-bound component does not realize the exact work key.
    #[error("certified incumbent does not realize the requested boundary application")]
    IncumbentNotApplicable,
    /// Exact component evaluation failed internally.
    #[error("component application failed internally: {0}")]
    Application(String),
    /// A finite physical count cannot be represented exactly.
    #[error("component search physical count overflow")]
    CountOverflow,
    /// Primitive topology construction violated an internal invariant.
    #[error("component topology construction failed: {0}")]
    Topology(String),
    /// Frozen-subsystem analysis failed internally.
    #[error("component frozen-subsystem analysis failed: {0}")]
    Frozen(String),
    /// Component certification failed internally.
    #[error("component certification failed: {0}")]
    Certification(String),
    /// A structurally complete enumerated graph failed unexpectedly.
    #[error("component candidate validator failed internally: {0}")]
    Validation(String),
    /// Exhaustion did not rediscover the independently feasible upper bound.
    #[error("component search exhausted its certified upper bound without a winner")]
    MissingCertifiedIncumbent,
}

impl From<ComponentApplicationError> for ComponentOptimizationError {
    fn from(error: ComponentApplicationError) -> Self {
        Self::Application(error.to_string())
    }
}

impl From<ComponentError> for ComponentOptimizationError {
    fn from(error: ComponentError) -> Self {
        Self::Certification(error.to_string())
    }
}

/// A concrete application optimum safe to persist transactionally.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProvenApplicationComponent {
    /// Deterministic canonical winning component.
    pub component: Component,
    /// Exact boundary isomorphism at the proof key.
    pub application: ComponentApplicationMatch,
    /// Finite exact optimality ledger scoped to that key only.
    pub proof: ApplicationOptimalityProof,
}

/// Result of a synchronous concrete component optimization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComponentOptimizationOutcome {
    /// Every lower obligation and the winning equal-cost group were exhausted.
    Complete(Box<ProvenApplicationComponent>),
    /// Cancellation interrupted an obligation. This value is never persistable
    /// as a completed proof.
    Incomplete {
        /// Smallest certified upper bound encountered before cancellation.
        best_known: Box<Component>,
        /// Only fully exhausted profile obligations.
        exhausted_profiles: Vec<ComponentProfileProof>,
    },
}

/// Proves the best component for exactly one concrete boundary-rate/capacity key.
///
/// The incumbent supplies a finite certified upper bound. Node count is
/// exhausted in ascending order; profiles are grouped by their fixed internal
/// link count and each equal-link group is finished before selecting its
/// smallest canonical witness. Every synthetic boundary link is required to
/// terminate at an internal node port, so zero-node/direct-terminal bypasses
/// are outside the component universe.
///
/// # Errors
///
/// Returns an internal error for a non-applicable incumbent or a violated
/// physical/certification invariant. Cancellation is an ordinary
/// [`ComponentOptimizationOutcome::Incomplete`] result.
pub fn optimize_component_application(
    key: &ComponentApplicationKey,
    incumbent: &Component,
    cancel: &AtomicBool,
) -> Result<ComponentOptimizationOutcome, ComponentOptimizationError> {
    let Some(_) = match_component_application(incumbent, key)? else {
        return Err(ComponentOptimizationError::IncumbentNotApplicable);
    };
    let upper = incumbent.cost();
    let mut exhausted_profiles = Vec::new();
    let mut best_known = incumbent.clone();

    for node_count in 1..=upper.nodes {
        let groups =
            application_profile_groups(node_count, key.inputs().len(), key.outputs().len())?;
        for (internal_link_count, profiles) in groups {
            if node_count == upper.nodes && internal_link_count > upper.internal_links {
                break;
            }
            let mut group_best = None;
            for profile in profiles {
                if cancel.load(Ordering::Relaxed) {
                    return Ok(ComponentOptimizationOutcome::Incomplete {
                        best_known: Box::new(best_known),
                        exhausted_profiles,
                    });
                }
                let result = enumerate_profile_application(key, profile, cancel)?;
                if let Some(candidate) = result.best {
                    retain_component(&mut group_best, candidate.clone());
                    if component_order(&candidate) < component_order(&best_known) {
                        best_known = candidate;
                    }
                }
                if result.cancelled {
                    return Ok(ComponentOptimizationOutcome::Incomplete {
                        best_known: Box::new(best_known),
                        exhausted_profiles,
                    });
                }
                exhausted_profiles.push(ComponentProfileProof {
                    node_count,
                    internal_link_count,
                    profile,
                    physical_bijections_checked: result.physical_bijections_checked,
                    canonical_topologies_checked: result.canonical_topologies_checked,
                });
            }
            if let Some(component) = group_best {
                let application = match_component_application(&component, key)?
                    .ok_or(ComponentOptimizationError::MissingCertifiedIncumbent)?;
                let proof = ApplicationOptimalityProof {
                    key: key.clone(),
                    winner: component.canonical_key().clone(),
                    cost: component.cost(),
                    exhausted_profiles,
                };
                debug_assert!(verify_application_optimality_manifest(&proof, &component));
                return Ok(ComponentOptimizationOutcome::Complete(Box::new(
                    ProvenApplicationComponent {
                        component,
                        application,
                        proof,
                    },
                )));
            }
        }
    }
    Err(ComponentOptimizationError::MissingCertifiedIncumbent)
}

fn retain_component(target: &mut Option<Component>, candidate: Component) {
    if target
        .as_ref()
        .is_none_or(|current| component_order(&candidate) < component_order(current))
    {
        *target = Some(candidate);
    }
}

fn component_order(component: &Component) -> (ComponentCost, &ComponentCanonicalKey) {
    (component.cost(), component.canonical_key())
}

/// Re-verifies that a persisted application ledger covers exactly every
/// deterministic profile obligation through the winning equal-cost group.
///
/// This validates proof scope and finite hierarchical accounting. It does not
/// trust the winner's stored equations: callers must also import the component
/// through [`Component::import_verified`], which redoes physical projection
/// certification.
#[must_use]
pub fn verify_application_optimality_manifest(
    proof: &ApplicationOptimalityProof,
    winner: &Component,
) -> bool {
    if proof.winner != *winner.canonical_key()
        || proof.cost != winner.cost()
        || match_component_application(winner, &proof.key)
            .ok()
            .flatten()
            .is_none()
    {
        return false;
    }
    let Ok(expected) = expected_profile_manifest(
        proof.cost,
        proof.key.inputs().len(),
        proof.key.outputs().len(),
    ) else {
        return false;
    };
    if expected.len() != proof.exhausted_profiles.len() {
        return false;
    }
    expected
        .iter()
        .zip(&proof.exhausted_profiles)
        .all(|((nodes, links, profile), actual)| {
            *nodes == actual.node_count
                && *links == actual.internal_link_count
                && *profile == actual.profile
        })
}

fn expected_profile_manifest(
    cost: ComponentCost,
    input_count: usize,
    output_count: usize,
) -> Result<Vec<(u32, u32, NodeProfile)>, ComponentOptimizationError> {
    let mut result = Vec::new();
    for node_count in 1..=cost.nodes {
        for (links, profiles) in application_profile_groups(node_count, input_count, output_count)?
        {
            if node_count == cost.nodes && links > cost.internal_links {
                break;
            }
            result.extend(
                profiles
                    .into_iter()
                    .map(|profile| (node_count, links, profile)),
            );
            if node_count == cost.nodes && links == cost.internal_links {
                return Ok(result);
            }
        }
    }
    Ok(result)
}

fn application_profile_groups(
    node_count: u32,
    input_count: usize,
    output_count: usize,
) -> Result<BTreeMap<u32, Vec<NodeProfile>>, ComponentOptimizationError> {
    let inputs =
        u64::try_from(input_count).map_err(|_| ComponentOptimizationError::CountOverflow)?;
    let outputs =
        u64::try_from(output_count).map_err(|_| ComponentOptimizationError::CountOverflow)?;
    let mut groups = BTreeMap::<u32, Vec<NodeProfile>>::new();
    for splitter2 in 0..=node_count {
        for splitter3 in 0..=node_count - splitter2 {
            for merger2 in 0..=node_count - splitter2 - splitter3 {
                let merger3 = node_count - splitter2 - splitter3 - merger2;
                let profile = NodeProfile {
                    splitter2,
                    splitter3,
                    merger2,
                    merger3,
                };
                let producers = 2_u64 * u64::from(splitter2)
                    + 3_u64 * u64::from(splitter3)
                    + u64::from(merger2)
                    + u64::from(merger3);
                let consumers = u64::from(splitter2)
                    + u64::from(splitter3)
                    + 2_u64 * u64::from(merger2)
                    + 3_u64 * u64::from(merger3);
                let Some(from_producers) = producers.checked_sub(outputs) else {
                    continue;
                };
                let Some(from_consumers) = consumers.checked_sub(inputs) else {
                    continue;
                };
                if from_producers != from_consumers {
                    continue;
                }
                let links = u32::try_from(from_producers)
                    .map_err(|_| ComponentOptimizationError::CountOverflow)?;
                groups.entry(links).or_default().push(profile);
            }
        }
    }
    for profiles in groups.values_mut() {
        profiles.sort();
    }
    Ok(groups)
}

struct ProfileEnumerationResult {
    best: Option<Component>,
    physical_bijections_checked: u64,
    canonical_topologies_checked: u64,
    cancelled: bool,
}

fn enumerate_profile_application(
    key: &ComponentApplicationKey,
    profile: NodeProfile,
    cancel: &AtomicBool,
) -> Result<ProfileEnumerationResult, ComponentOptimizationError> {
    let problem = Problem {
        inputs: key.inputs().to_vec(),
        outputs: key.outputs().to_vec(),
        max_link_rate: key.max_link_rate().clone(),
    };
    let nodes = physical_nodes(profile)?;
    let producers = producer_ports(&nodes, key.inputs().len())?;
    let consumers = consumer_ports(&nodes, key.outputs().len())?;
    if producers.len() != consumers.len() {
        return Err(ComponentOptimizationError::Topology(
            "balanced component profile produced unequal port sets".to_owned(),
        ));
    }
    let mut enumerator = ProfileEnumerator {
        key,
        problem: &problem,
        nodes: &nodes,
        producers: &producers,
        consumers: &consumers,
        cancel,
        used_consumers: vec![false; consumers.len()],
        links: Vec::with_capacity(producers.len()),
        seen_topologies: BTreeSet::new(),
        best: None,
        physical_bijections_checked: 0,
        cancelled: false,
    };
    enumerator.extend(0)?;
    Ok(ProfileEnumerationResult {
        best: enumerator.best,
        physical_bijections_checked: enumerator.physical_bijections_checked,
        canonical_topologies_checked: u64::try_from(enumerator.seen_topologies.len())
            .unwrap_or(u64::MAX),
        cancelled: enumerator.cancelled,
    })
}

struct ProfileEnumerator<'a> {
    key: &'a ComponentApplicationKey,
    problem: &'a Problem,
    nodes: &'a [PhysicalNode],
    producers: &'a [ProducerPortRef],
    consumers: &'a [ConsumerPortRef],
    cancel: &'a AtomicBool,
    used_consumers: Vec<bool>,
    links: Vec<PhysicalLink>,
    seen_topologies: BTreeSet<Vec<u8>>,
    best: Option<Component>,
    physical_bijections_checked: u64,
    cancelled: bool,
}

impl ProfileEnumerator<'_> {
    fn extend(&mut self, producer_index: usize) -> Result<(), ComponentOptimizationError> {
        if self.cancel.load(Ordering::Relaxed) {
            self.cancelled = true;
            return Ok(());
        }
        if producer_index == self.producers.len() {
            self.physical_bijections_checked = self.physical_bijections_checked.saturating_add(1);
            return self.evaluate_complete();
        }
        let producer = self.producers[producer_index];
        for consumer_index in 0..self.consumers.len() {
            if self.used_consumers[consumer_index] {
                continue;
            }
            let consumer = self.consumers[consumer_index];
            if !legal_component_link(producer, consumer) {
                continue;
            }
            self.used_consumers[consumer_index] = true;
            self.links.push(PhysicalLink {
                producer,
                consumer,
                flow: Rational::zero(),
            });
            self.extend(producer_index + 1)?;
            self.links.pop();
            self.used_consumers[consumer_index] = false;
            if self.cancelled {
                return Ok(());
            }
        }
        Ok(())
    }

    fn evaluate_complete(&mut self) -> Result<(), ComponentOptimizationError> {
        let graph = PhysicalGraph {
            nodes: self.nodes.to_vec(),
            links: self.links.clone(),
        };
        let topology_key = canonicalize_witness(self.problem, &graph).key.into_bytes();
        if !self.seen_topologies.insert(topology_key) {
            return Ok(());
        }
        let solved = match solve_topology(self.problem, &graph) {
            Ok(graph) => graph,
            Err(error) if candidate_rejection(&error) => return Ok(()),
            Err(error) => return Err(ComponentOptimizationError::Validation(error.to_string())),
        };
        let canonical = canonicalize_witness(self.problem, &solved).graph;
        match validate_solution(self.problem, &canonical) {
            Ok(_) => {}
            Err(error) if candidate_rejection(&error) => return Ok(()),
            Err(error) => return Err(ComponentOptimizationError::Validation(error.to_string())),
        }
        let component = component_from_synthetic_graph(self.problem, &canonical)?;
        let component = Component::import_verified(&component.export_record())?;
        if match_component_application(&component, self.key)?.is_some() {
            retain_component(&mut self.best, component);
        }
        Ok(())
    }
}

fn legal_component_link(producer: ProducerPortRef, consumer: ConsumerPortRef) -> bool {
    match (producer, consumer) {
        (ProducerPortRef::Input(_), ConsumerPortRef::Output(_) | ConsumerPortRef::Discard(_))
        | (ProducerPortRef::Node { .. }, ConsumerPortRef::Discard(_)) => false,
        (
            ProducerPortRef::Node { node: producer, .. },
            ConsumerPortRef::Node { node: consumer, .. },
        ) => producer != consumer,
        _ => true,
    }
}

fn component_from_synthetic_graph(
    problem: &Problem,
    graph: &PhysicalGraph,
) -> Result<Component, ComponentOptimizationError> {
    let mut boundary_inputs = vec![None; problem.inputs.len()];
    let mut boundary_outputs = vec![None; problem.outputs.len()];
    for link in &graph.links {
        match (link.producer, link.consumer) {
            (ProducerPortRef::Input(input), consumer @ ConsumerPortRef::Node { .. }) => {
                let slot = boundary_inputs.get_mut(input.0 as usize).ok_or_else(|| {
                    ComponentOptimizationError::Topology(
                        "input terminal index is out of range".to_owned(),
                    )
                })?;
                if slot.replace(consumer).is_some() {
                    return Err(ComponentOptimizationError::Topology(
                        "input terminal appears more than once".to_owned(),
                    ));
                }
            }
            (producer @ ProducerPortRef::Node { .. }, ConsumerPortRef::Output(output)) => {
                let slot = boundary_outputs.get_mut(output.0 as usize).ok_or_else(|| {
                    ComponentOptimizationError::Topology(
                        "output terminal index is out of range".to_owned(),
                    )
                })?;
                if slot.replace(producer).is_some() {
                    return Err(ComponentOptimizationError::Topology(
                        "output terminal appears more than once".to_owned(),
                    ));
                }
            }
            (ProducerPortRef::Node { .. }, ConsumerPortRef::Node { .. }) => {}
            _ => {
                return Err(ComponentOptimizationError::Topology(
                    "synthetic boundary link did not terminate at an internal node".to_owned(),
                ));
            }
        }
    }
    let boundary_inputs = boundary_inputs
        .into_iter()
        .map(|port| {
            port.map(|port| DeclaredBoundaryInput { port })
                .ok_or_else(|| {
                    ComponentOptimizationError::Topology(
                        "synthetic input has no internal endpoint".to_owned(),
                    )
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let boundary_outputs = boundary_outputs
        .into_iter()
        .map(|port| {
            port.map(|port| DeclaredBoundaryOutput { port })
                .ok_or_else(|| {
                    ComponentOptimizationError::Topology(
                        "synthetic output has no internal endpoint".to_owned(),
                    )
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let topology = TopologyState::from_partial_topology(&PartialTopology {
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
        discard_count: 0,
        remaining_profile: NodeProfile::default(),
    })
    .map_err(|error| ComponentOptimizationError::Topology(error.to_string()))?;
    let declaration = FrozenSubsystemDeclaration {
        nodes: graph.nodes.iter().map(|node| node.id).collect(),
        boundary_inputs,
        boundary_outputs,
    };
    let analysis = analyze_frozen_subsystem(&topology, &declaration)
        .map_err(|error| ComponentOptimizationError::Frozen(error.to_string()))?;
    let FrozenSubsystemAnalysis::Symbolic(frozen) = analysis else {
        return Err(ComponentOptimizationError::Frozen(
            "validated complete graph produced a singular frozen subsystem".to_owned(),
        ));
    };
    Component::from_frozen(*frozen).map_err(Into::into)
}

fn physical_nodes(profile: NodeProfile) -> Result<Vec<PhysicalNode>, ComponentOptimizationError> {
    let mut nodes = Vec::new();
    for (node_type, count) in [
        (NodeType::Splitter2, profile.splitter2),
        (NodeType::Splitter3, profile.splitter3),
        (NodeType::Merger2, profile.merger2),
        (NodeType::Merger3, profile.merger3),
    ] {
        for _ in 0..count {
            let id = u32::try_from(nodes.len())
                .map_err(|_| ComponentOptimizationError::CountOverflow)?;
            nodes.push(PhysicalNode {
                id: NodeId(id),
                node_type,
            });
        }
    }
    Ok(nodes)
}

fn producer_ports(
    nodes: &[PhysicalNode],
    input_count: usize,
) -> Result<Vec<ProducerPortRef>, ComponentOptimizationError> {
    let mut ports = (0..input_count)
        .map(|index| {
            u32::try_from(index)
                .map(InputTerminalIndex)
                .map(ProducerPortRef::Input)
                .map_err(|_| ComponentOptimizationError::CountOverflow)
        })
        .collect::<Result<Vec<_>, _>>()?;
    for node in nodes {
        for port in 0..node.node_type.output_port_count() {
            ports.push(ProducerPortRef::Node {
                node: node.id,
                port,
            });
        }
    }
    Ok(ports)
}

fn consumer_ports(
    nodes: &[PhysicalNode],
    output_count: usize,
) -> Result<Vec<ConsumerPortRef>, ComponentOptimizationError> {
    let mut ports = (0..output_count)
        .map(|index| {
            u32::try_from(index)
                .map(OutputTerminalIndex)
                .map(ConsumerPortRef::Output)
                .map_err(|_| ComponentOptimizationError::CountOverflow)
        })
        .collect::<Result<Vec<_>, _>>()?;
    for node in nodes {
        for port in 0..node.node_type.input_port_count() {
            ports.push(ConsumerPortRef::Node {
                node: node.id,
                port,
            });
        }
    }
    Ok(ports)
}

fn candidate_rejection(error: &ValidationError) -> bool {
    matches!(
        error,
        ValidationError::NodeUnreachableFromInput { .. }
            | ValidationError::NodeCannotReachOutput { .. }
            | ValidationError::InconsistentSteadyState
            | ValidationError::NonUniqueSteadyState { .. }
            | ValidationError::InconsistentCyclicScc { .. }
            | ValidationError::NonUniqueCyclicScc { .. }
            | ValidationError::IncorrectOutputRate { .. }
            | ValidationError::NonPositiveResolvedFlow { .. }
            | ValidationError::ResolvedFlowAboveCapacity { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        component_application::ComponentApplicationRequest, components::BoundarySignature,
    };

    fn rational(value: &str) -> Rational {
        value.parse().unwrap()
    }

    fn splitter_incumbent() -> Component {
        let problem = Problem {
            inputs: vec![rational("2")],
            outputs: vec![rational("1"), rational("1")],
            max_link_rate: rational("2"),
        };
        let graph = PhysicalGraph {
            nodes: vec![PhysicalNode {
                id: NodeId(0),
                node_type: NodeType::Splitter2,
            }],
            links: vec![
                PhysicalLink {
                    producer: ProducerPortRef::Input(InputTerminalIndex(0)),
                    consumer: ConsumerPortRef::Node {
                        node: NodeId(0),
                        port: 0,
                    },
                    flow: rational("2"),
                },
                PhysicalLink {
                    producer: ProducerPortRef::Node {
                        node: NodeId(0),
                        port: 0,
                    },
                    consumer: ConsumerPortRef::Output(OutputTerminalIndex(0)),
                    flow: rational("1"),
                },
                PhysicalLink {
                    producer: ProducerPortRef::Node {
                        node: NodeId(0),
                        port: 1,
                    },
                    consumer: ConsumerPortRef::Output(OutputTerminalIndex(1)),
                    flow: rational("1"),
                },
            ],
        };
        component_from_synthetic_graph(&problem, &graph).unwrap()
    }

    fn splitter_key(capacity: &str) -> ComponentApplicationKey {
        ComponentApplicationRequest::new(
            &BoundarySignature::from_feedback_relation(1, 2, vec![true, true]).unwrap(),
            &[rational("2")],
            &[rational("1"), rational("1")],
            &rational(capacity),
        )
        .unwrap()
        .key()
        .clone()
    }

    #[test]
    fn primitive_optimizer_proves_and_validates_the_equal_link_group() {
        let incumbent = splitter_incumbent();
        let outcome =
            optimize_component_application(&splitter_key("2"), &incumbent, &AtomicBool::new(false))
                .unwrap();
        let ComponentOptimizationOutcome::Complete(proven) = outcome else {
            panic!("expected a completed component proof");
        };
        assert_eq!(
            proven.component.cost(),
            ComponentCost {
                nodes: 1,
                internal_links: 0
            }
        );
        assert_eq!(proven.component.canonical_key(), incumbent.canonical_key());
        assert!(verify_application_optimality_manifest(
            &proven.proof,
            &proven.component
        ));
        assert_eq!(proven.application.application.inputs, vec![rational("2")]);
    }

    #[test]
    fn capacity_is_part_of_the_exact_incumbent_scope() {
        let incumbent = splitter_incumbent();
        assert!(
            match_component_application(&incumbent, &splitter_key("2"))
                .unwrap()
                .is_some()
        );
        let too_small = ComponentApplicationRequest::new(
            &BoundarySignature::from_feedback_relation(1, 2, vec![true, true]).unwrap(),
            &[rational("2")],
            &[rational("1"), rational("1")],
            &rational("3/2"),
        );
        assert!(too_small.is_err(), "boundary input itself exceeds B");
    }

    #[test]
    fn cancellation_never_constructs_a_complete_persistable_proof() {
        let outcome = optimize_component_application(
            &splitter_key("2"),
            &splitter_incumbent(),
            &AtomicBool::new(true),
        )
        .unwrap();
        assert!(matches!(
            outcome,
            ComponentOptimizationOutcome::Incomplete {
                exhausted_profiles,
                ..
            } if exhausted_profiles.is_empty()
        ));
    }

    #[test]
    fn manifest_rejects_a_missing_equal_cost_profile_obligation() {
        let ComponentOptimizationOutcome::Complete(mut proven) = optimize_component_application(
            &splitter_key("2"),
            &splitter_incumbent(),
            &AtomicBool::new(false),
        )
        .unwrap() else {
            panic!("expected complete proof");
        };
        assert!(proven.proof.exhausted_profiles.pop().is_some());
        assert!(!verify_application_optimality_manifest(
            &proven.proof,
            &proven.component
        ));
    }
}
