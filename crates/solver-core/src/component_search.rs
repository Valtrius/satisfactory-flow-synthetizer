//! Live component discovery and application-specific optimization.
//!
//! This module is deliberately outside the physical DFS. It may add certified
//! batch construction paths, but it cannot reject a state or suppress an
//! ordinary link decision. Repository records are treated only as feasible
//! incumbents. The current process redoes the finite application proof before
//! publishing a work-table `Complete` value.

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    convert::Infallible,
    sync::{Mutex, MutexGuard, atomic::AtomicBool},
};

use solver_api::{ConsumerPortRef, NodeId, ProducerPortRef, Rational};

use crate::{
    component_application::{
        ComponentApplicationKey, ComponentApplicationRequest, match_component_application,
    },
    component_optimizer::{
        ComponentOptimizationOutcome, ProvenApplicationComponent, optimize_component_application,
        verify_application_optimality_manifest,
    },
    components::{Component, ComponentCanonicalKey, ComponentParetoFrontier},
    propagation::PropagationState,
    scc::{
        DeclaredBoundaryInput, DeclaredBoundaryOutput, FrozenSubsystemAnalysis,
        FrozenSubsystemDeclaration, analyze_frozen_subsystem, detect_affected_sccs,
    },
    topology::TopologyState,
    work_table::{
        ComponentProvider, ComponentRepository, ComponentWorkCompletion, ComponentWorkOutcome,
        ComponentWorkTable,
    },
};

/// A certified component and its exact concrete application request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DiscoveredComponent {
    pub component: Component,
    pub request: ComponentApplicationRequest,
}

/// Counters and optional result from one live application request.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ComponentResolution {
    /// Proof-complete application winner, when live work finished.
    pub component: Option<Component>,
    /// Provider/repository lookups performed by the work owner.
    pub source_lookups: u64,
    /// Loaded records that reconstructed as exact feasible incumbents.
    pub source_hits: u64,
    /// Primitive application optimizations started by the work owner.
    pub optimizations: u64,
}

/// Search-facing component service.
///
/// Implementations may return no component for any reason. Search must retain
/// every primitive transition regardless of the result.
pub trait ComponentResolver: Send + Sync {
    /// Returns all proof-complete components learned by this process in
    /// deterministic canonical order.
    fn ordered_components(&self) -> Vec<Component>;

    /// Proves the best component for this candidate's concrete request, or
    /// reports that the optional acceleration is unavailable.
    fn resolve(
        &self,
        candidate: &Component,
        request: &ComponentApplicationRequest,
        cancel: &AtomicBool,
    ) -> ComponentResolution;
}

/// Empty provider/repository used by the default in-memory production solve.
#[derive(Clone, Copy, Debug, Default)]
pub struct EmptyComponentStore;

impl ComponentRepository<ComponentApplicationKey, ProvenApplicationComponent>
    for EmptyComponentStore
{
    type Error = Infallible;

    fn load_complete(
        &self,
        _key: &ComponentApplicationKey,
    ) -> Result<Option<ProvenApplicationComponent>, Self::Error> {
        Ok(None)
    }

    fn store_complete(
        &self,
        _key: &ComponentApplicationKey,
        _value: &ProvenApplicationComponent,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl ComponentProvider<ComponentApplicationKey, ProvenApplicationComponent>
    for EmptyComponentStore
{
    type Error = Infallible;

    fn provide(
        &self,
        _key: &ComponentApplicationKey,
    ) -> Result<Option<ProvenApplicationComponent>, Self::Error> {
        Ok(None)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ComponentWorkFailure {
    Cancelled,
    OptimizationFailed,
}

/// Shared live component engine backed by the generic provider/repository ports.
///
/// Loaded records never enter the work table as completed proofs. The engine
/// re-imports their physical witness, checks the concrete request, and uses the
/// best valid record only to tighten the feasible upper bound. It then runs the
/// current process's primitive optimizer. This means a malformed, stale, or
/// unavailable store can cost time but cannot create a false proof.
pub struct ApplicationComponentRuntime<'a, R, P> {
    repository: &'a R,
    provider: &'a P,
    work: ComponentWorkTable<
        ComponentApplicationKey,
        ProvenApplicationComponent,
        ComponentWorkFailure,
    >,
    catalog: Mutex<ComponentParetoFrontier>,
}

impl<'a, R, P> ApplicationComponentRuntime<'a, R, P> {
    /// Creates an empty process-local work table and catalog.
    #[must_use]
    pub fn new(repository: &'a R, provider: &'a P) -> Self {
        Self {
            repository,
            provider,
            work: ComponentWorkTable::new(),
            catalog: Mutex::new(ComponentParetoFrontier::new()),
        }
    }

    /// Inserts a certified symbolic component as a macro candidate.
    ///
    /// This does not claim application optimality. The component remains only
    /// an additional physical construction path. The live catalog removes an
    /// alternative only when [`ComponentParetoFrontier`] proves the exact
    /// fixed-profile domain/cost/peak dominance theorem. In particular, a
    /// cheaper component with a worse internal capacity peak remains available.
    pub fn insert_component(&self, component: Component) {
        lock_unpoisoned(&self.catalog).insert(component);
    }
}

impl<R, P> ComponentResolver for ApplicationComponentRuntime<'_, R, P>
where
    R: ComponentRepository<ComponentApplicationKey, ProvenApplicationComponent>,
    P: ComponentProvider<ComponentApplicationKey, ProvenApplicationComponent>,
{
    fn ordered_components(&self) -> Vec<Component> {
        lock_unpoisoned(&self.catalog).iter().cloned().collect()
    }

    fn resolve(
        &self,
        candidate: &Component,
        request: &ComponentApplicationRequest,
        cancel: &AtomicBool,
    ) -> ComponentResolution {
        let key = request.key().clone();
        let mut source_lookups = 0_u64;
        let mut source_hits = 0_u64;
        let mut optimizations = 0_u64;
        let mut newly_computed = None;
        let outcome = self.work.run(key.clone(), |_| {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return ComponentWorkCompletion::Incomplete(ComponentWorkFailure::Cancelled);
            }

            let mut incumbent = candidate.clone();
            source_lookups = source_lookups.saturating_add(1);
            if let Ok(Some(loaded)) = self.provider.provide(&key)
                && let Some(component) = verified_feasible_incumbent(&loaded, &key)
            {
                source_hits = source_hits.saturating_add(1);
                retain_better_incumbent(&mut incumbent, component);
            }
            source_lookups = source_lookups.saturating_add(1);
            if let Ok(Some(loaded)) = self.repository.load_complete(&key)
                && let Some(component) = verified_feasible_incumbent(&loaded, &key)
            {
                source_hits = source_hits.saturating_add(1);
                retain_better_incumbent(&mut incumbent, component);
            }

            // A source ledger is not an exhaustion certificate for this
            // process. It can only reduce the finite incumbent bound above.
            optimizations = optimizations.saturating_add(1);
            let optimized = match optimize_component_application(&key, &incumbent, cancel) {
                Ok(ComponentOptimizationOutcome::Complete(proven)) => *proven,
                Ok(ComponentOptimizationOutcome::Incomplete { .. }) => {
                    return ComponentWorkCompletion::Incomplete(ComponentWorkFailure::Cancelled);
                }
                Err(_) => {
                    return ComponentWorkCompletion::Incomplete(
                        ComponentWorkFailure::OptimizationFailed,
                    );
                }
            };
            if !verify_application_optimality_manifest(&optimized.proof, &optimized.component)
                || match_component_application(&optimized.component, &key)
                    .ok()
                    .flatten()
                    .is_none()
            {
                return ComponentWorkCompletion::Incomplete(
                    ComponentWorkFailure::OptimizationFailed,
                );
            }
            newly_computed = Some(optimized.clone());
            ComponentWorkCompletion::Complete(optimized)
        });

        let component = match outcome {
            ComponentWorkOutcome::Complete(proven) => {
                if let Some(computed) = newly_computed.as_ref() {
                    // Store failures affect acceleration only. The in-memory
                    // proof and every primitive parent transition remain live.
                    let _ = self.repository.store_complete(&key, computed);
                }
                let component = proven.component.clone();
                self.insert_component(component.clone());
                Some(component)
            }
            ComponentWorkOutcome::Incomplete(_)
            | ComponentWorkOutcome::RecursiveDependency
            | ComponentWorkOutcome::Abandoned => None,
        };

        ComponentResolution {
            component,
            source_lookups,
            source_hits,
            optimizations,
        }
    }
}

fn verified_feasible_incumbent(
    loaded: &ProvenApplicationComponent,
    key: &ComponentApplicationKey,
) -> Option<Component> {
    // Rebuild the exact physical contract. The loaded application ledger is
    // intentionally ignored here and cannot discharge live exhaustion.
    let component = Component::import_verified(&loaded.component.export_record()).ok()?;
    match_component_application(&component, key)
        .ok()
        .flatten()
        .map(|_| component)
}

fn retain_better_incumbent(incumbent: &mut Component, candidate: Component) {
    if (candidate.cost(), candidate.canonical_key()) < (incumbent.cost(), incumbent.canonical_key())
    {
        *incumbent = candidate;
    }
}

/// Finds deterministic frozen subsystems whose complete boundary vector is
/// already exact in the active branch.
///
/// Each declaration partitions every selected-node port into either a fixed
/// internal link or an explicit boundary. Future construction therefore cannot
/// alter the internal topology or attach to an undeclared internal port. It may
/// connect the declared boundary into arbitrary outside feedback, including a
/// larger dynamic graph SCC. Discovery never changes the branch and every
/// failure is an optional cache miss.
pub(crate) fn discover_components(
    topology: &TopologyState,
    propagation: &PropagationState,
) -> Vec<DiscoveredComponent> {
    let mut discovered =
        BTreeMap::<(ComponentApplicationKey, ComponentCanonicalKey), DiscoveredComponent>::new();
    for nodes in discovery_node_sets(topology) {
        let Some(candidate) = freeze_node_set(topology, propagation, &nodes) else {
            continue;
        };
        discovered.insert(
            (
                candidate.request.key().clone(),
                candidate.component.canonical_key().clone(),
            ),
            candidate,
        );
    }
    discovered.into_values().collect()
}

fn discovery_node_sets(topology: &TopologyState) -> Vec<Vec<NodeId>> {
    let mut sets = BTreeSet::new();
    for node in topology.nodes() {
        sets.insert(vec![node.id]);
    }

    // Maximal weak components capture the common acyclic pipeline case without
    // enumerating every node subset. Singletons above still expose each local
    // splitter or merger as soon as its rates become exact.
    let mut adjacency = topology
        .nodes()
        .iter()
        .map(|node| (node.id, BTreeSet::new()))
        .collect::<BTreeMap<_, _>>();
    for link in topology.links() {
        if let (Some(left), Some(right)) =
            (producer_node(link.producer), consumer_node(link.consumer))
        {
            adjacency.entry(left).or_default().insert(right);
            adjacency.entry(right).or_default().insert(left);
        }
    }
    let mut unseen = adjacency.keys().copied().collect::<BTreeSet<_>>();
    while let Some(root) = unseen.pop_first() {
        let mut queue = VecDeque::from([root]);
        let mut nodes = Vec::new();
        while let Some(node) = queue.pop_front() {
            nodes.push(node);
            if let Some(neighbors) = adjacency.get(&node) {
                for &neighbor in neighbors {
                    if unseen.remove(&neighbor) {
                        queue.push_back(neighbor);
                    }
                }
            }
        }
        nodes.sort_unstable();
        if nodes.len() > 1 {
            sets.insert(nodes);
        }
    }

    // A directed cyclic region is useful independently of the larger weak
    // component. Freezing it does not claim that the graph-level SCC is final.
    if let Ok(sccs) = detect_affected_sccs(topology, None) {
        for region in sccs.regions.into_iter().filter(|region| region.cyclic) {
            sets.insert(region.nodes);
        }
    }
    sets.into_iter().collect()
}

fn freeze_node_set(
    topology: &TopologyState,
    propagation: &PropagationState,
    nodes: &[NodeId],
) -> Option<DiscoveredComponent> {
    let node_set = nodes.iter().copied().collect::<BTreeSet<_>>();
    let mut internal_producers = BTreeSet::new();
    let mut internal_consumers = BTreeSet::new();
    for link in topology.links() {
        let producer_inside =
            producer_node(link.producer).is_some_and(|node| node_set.contains(&node));
        let consumer_inside =
            consumer_node(link.consumer).is_some_and(|node| node_set.contains(&node));
        if producer_inside && consumer_inside {
            internal_producers.insert(link.producer);
            internal_consumers.insert(link.consumer);
        }
    }

    let mut boundary_inputs = Vec::new();
    let mut boundary_outputs = Vec::new();
    for node in topology
        .nodes()
        .iter()
        .filter(|node| node_set.contains(&node.id))
    {
        for port in 0..node.node_type.input_port_count() {
            let port = ConsumerPortRef::Node {
                node: node.id,
                port,
            };
            if !internal_consumers.contains(&port) {
                boundary_inputs.push(DeclaredBoundaryInput { port });
            }
        }
        for port in 0..node.node_type.output_port_count() {
            let port = ProducerPortRef::Node {
                node: node.id,
                port,
            };
            if !internal_producers.contains(&port) {
                boundary_outputs.push(DeclaredBoundaryOutput { port });
            }
        }
    }
    if boundary_inputs.is_empty() || boundary_outputs.is_empty() {
        return None;
    }
    boundary_inputs.sort_by_key(|boundary| boundary.port);
    boundary_outputs.sort_by_key(|boundary| boundary.port);

    let analysis = analyze_frozen_subsystem(
        topology,
        &FrozenSubsystemDeclaration {
            nodes: nodes.to_vec(),
            boundary_inputs,
            boundary_outputs,
        },
    )
    .ok()?;
    let FrozenSubsystemAnalysis::Symbolic(frozen) = analysis else {
        return None;
    };
    let component = Component::from_frozen(*frozen).ok()?;
    let inputs = (0..component.boundary().input_count())
        .map(|index| {
            let endpoint = component.frozen_input_endpoint(index)?;
            known_consumer_flow(topology, propagation, endpoint)
        })
        .collect::<Option<Vec<_>>>()?;
    let outputs = (0..component.boundary().output_count())
        .map(|index| {
            let endpoint = component.frozen_output_endpoint(index)?;
            known_producer_flow(topology, propagation, endpoint)
        })
        .collect::<Option<Vec<_>>>()?;
    let request = ComponentApplicationRequest::new(
        component.boundary(),
        &inputs,
        &outputs,
        &topology.problem().max_link_rate,
    )
    .ok()?;
    match_component_application(&component, request.key())
        .ok()
        .flatten()?;
    Some(DiscoveredComponent { component, request })
}

fn known_consumer_flow(
    topology: &TopologyState,
    propagation: &PropagationState,
    port: ConsumerPortRef,
) -> Option<Rational> {
    let variable = topology.consumer_ports().get(&port)?.flow_var;
    propagation.known_value(variable).ok().flatten()
}

fn known_producer_flow(
    topology: &TopologyState,
    propagation: &PropagationState,
    port: ProducerPortRef,
) -> Option<Rational> {
    let variable = topology.producer_ports().get(&port)?.flow_var;
    propagation.known_value(variable).ok().flatten()
}

const fn producer_node(port: ProducerPortRef) -> Option<NodeId> {
    match port {
        ProducerPortRef::Input(_) => None,
        ProducerPortRef::Node { node, .. } => Some(node),
    }
}

const fn consumer_node(port: ConsumerPortRef) -> Option<NodeId> {
    match port {
        ConsumerPortRef::Output(_) | ConsumerPortRef::Discard(_) => None,
        ConsumerPortRef::Node { node, .. } => Some(node),
    }
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use solver_api::{
        InputTerminalIndex, NodeProfile, NodeType, OutputTerminalIndex, PhysicalNode, Problem,
    };

    use super::*;
    use crate::{
        canonical::{PartialLink, PartialTopology},
        component_optimizer::optimize_component_application,
        solver::{SolveOptions, solve, solve_with_component_resolver},
        topology::TopologyState,
    };

    fn rational(value: &str) -> Rational {
        value.parse().unwrap()
    }

    fn splitter_topology() -> (TopologyState, PropagationState) {
        let problem = Problem {
            inputs: vec![rational("2")],
            outputs: vec![rational("1"), rational("1")],
            max_link_rate: rational("2"),
        };
        let topology = TopologyState::from_partial_topology(&PartialTopology {
            problem,
            nodes: vec![PhysicalNode {
                id: NodeId(0),
                node_type: NodeType::Splitter2,
            }],
            links: vec![
                PartialLink {
                    producer: ProducerPortRef::Input(InputTerminalIndex(0)),
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
                    consumer: ConsumerPortRef::Output(OutputTerminalIndex(0)),
                    flow: None,
                },
                PartialLink {
                    producer: ProducerPortRef::Node {
                        node: NodeId(0),
                        port: 1,
                    },
                    consumer: ConsumerPortRef::Output(OutputTerminalIndex(1)),
                    flow: None,
                },
            ],
            discard_count: 0,
            remaining_profile: NodeProfile::default(),
        })
        .unwrap();
        let propagation = PropagationState::new(&topology, &rational("2")).unwrap();
        (topology, propagation)
    }

    fn splitter_discovery() -> DiscoveredComponent {
        let (topology, propagation) = splitter_topology();
        let discovered = discover_components(&topology, &propagation);
        assert_eq!(discovered.len(), 1);
        discovered.into_iter().next().unwrap()
    }

    fn redundant_identity_component() -> Component {
        let problem = Problem {
            inputs: vec![rational("1")],
            outputs: vec![rational("1")],
            max_link_rate: rational("1"),
        };
        let nodes = vec![
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
        ];
        let internal = [
            ((0, 0), (1, 0)),
            ((0, 1), (1, 1)),
            ((1, 0), (2, 0)),
            ((2, 0), (3, 0)),
            ((2, 1), (3, 1)),
        ];
        let topology = TopologyState::from_partial_topology(&PartialTopology {
            problem,
            nodes,
            links: internal
                .into_iter()
                .map(
                    |((producer_node, producer_port), (consumer_node, consumer_port))| {
                        PartialLink {
                            producer: ProducerPortRef::Node {
                                node: NodeId(producer_node),
                                port: producer_port,
                            },
                            consumer: ConsumerPortRef::Node {
                                node: NodeId(consumer_node),
                                port: consumer_port,
                            },
                            flow: None,
                        }
                    },
                )
                .collect(),
            discard_count: 0,
            remaining_profile: NodeProfile::default(),
        })
        .unwrap();
        let FrozenSubsystemAnalysis::Symbolic(frozen) = analyze_frozen_subsystem(
            &topology,
            &FrozenSubsystemDeclaration {
                nodes: vec![NodeId(0), NodeId(1), NodeId(2), NodeId(3)],
                boundary_inputs: vec![DeclaredBoundaryInput {
                    port: ConsumerPortRef::Node {
                        node: NodeId(0),
                        port: 0,
                    },
                }],
                boundary_outputs: vec![DeclaredBoundaryOutput {
                    port: ProducerPortRef::Node {
                        node: NodeId(3),
                        port: 0,
                    },
                }],
            },
        )
        .unwrap() else {
            panic!("serial split-merge identity must be uniquely solvable");
        };
        Component::from_frozen(*frozen).unwrap()
    }

    fn two_node_identity_component(feedback: bool) -> Component {
        let problem = Problem {
            inputs: vec![rational("1")],
            outputs: vec![rational("1")],
            max_link_rate: rational("10"),
        };
        let nodes = vec![
            PhysicalNode {
                id: NodeId(0),
                node_type: NodeType::Splitter2,
            },
            PhysicalNode {
                id: NodeId(1),
                node_type: NodeType::Merger2,
            },
        ];
        let (internal, boundary_input, boundary_output) = if feedback {
            feedback_identity_parts()
        } else {
            acyclic_identity_parts()
        };
        let topology = TopologyState::from_partial_topology(&PartialTopology {
            problem,
            nodes,
            links: internal,
            discard_count: 0,
            remaining_profile: NodeProfile::default(),
        })
        .unwrap();
        let FrozenSubsystemAnalysis::Symbolic(frozen) = analyze_frozen_subsystem(
            &topology,
            &FrozenSubsystemDeclaration {
                nodes: vec![NodeId(0), NodeId(1)],
                boundary_inputs: vec![DeclaredBoundaryInput {
                    port: boundary_input,
                }],
                boundary_outputs: vec![DeclaredBoundaryOutput {
                    port: boundary_output,
                }],
            },
        )
        .unwrap() else {
            panic!("two-node identity must be uniquely solvable");
        };
        Component::from_frozen(*frozen).unwrap()
    }

    fn feedback_identity_parts() -> (Vec<PartialLink>, ConsumerPortRef, ProducerPortRef) {
        (
            vec![
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
            ConsumerPortRef::Node {
                node: NodeId(1),
                port: 1,
            },
            ProducerPortRef::Node {
                node: NodeId(0),
                port: 1,
            },
        )
    }

    fn acyclic_identity_parts() -> (Vec<PartialLink>, ConsumerPortRef, ProducerPortRef) {
        (
            vec![
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
                PartialLink {
                    producer: ProducerPortRef::Node {
                        node: NodeId(0),
                        port: 1,
                    },
                    consumer: ConsumerPortRef::Node {
                        node: NodeId(1),
                        port: 1,
                    },
                    flow: None,
                },
            ],
            ConsumerPortRef::Node {
                node: NodeId(0),
                port: 0,
            },
            ProducerPortRef::Node {
                node: NodeId(1),
                port: 0,
            },
        )
    }

    #[test]
    fn active_exact_branch_freezes_a_component_without_closing_future_feedback() {
        let discovered = splitter_discovery();
        assert_eq!(discovered.component.cost().nodes, 1);
        assert_eq!(discovered.component.internal_link_count(), 0);
        assert_eq!(discovered.request.key().inputs(), &[rational("2")]);
        assert_eq!(
            discovered.request.key().outputs(),
            &[rational("1"), rational("1")]
        );
        assert_eq!(discovered.request.key().max_link_rate(), &rational("2"));
    }

    #[test]
    fn live_catalog_prunes_only_the_proven_fixed_profile_peak_dominance() {
        let acyclic = two_node_identity_component(false);
        let feedback = two_node_identity_component(true);
        assert_eq!(acyclic.profile(), feedback.profile());
        assert_eq!(acyclic.cost(), feedback.cost());
        assert_ne!(
            acyclic.internal_flow_map(),
            feedback.internal_flow_map(),
            "the alternatives must exercise capacity-peak dominance"
        );

        let store = EmptyComponentStore;
        let left = ApplicationComponentRuntime::new(&store, &store);
        left.insert_component(feedback.clone());
        left.insert_component(acyclic.clone());
        let right = ApplicationComponentRuntime::new(&store, &store);
        right.insert_component(acyclic.clone());
        right.insert_component(feedback);

        assert_eq!(left.ordered_components(), vec![acyclic.clone()]);
        assert_eq!(right.ordered_components(), vec![acyclic]);
    }

    #[derive(Clone)]
    struct LoadedProvider(ProvenApplicationComponent);

    impl ComponentProvider<ComponentApplicationKey, ProvenApplicationComponent> for LoadedProvider {
        type Error = Infallible;

        fn provide(
            &self,
            _key: &ComponentApplicationKey,
        ) -> Result<Option<ProvenApplicationComponent>, Self::Error> {
            Ok(Some(self.0.clone()))
        }
    }

    #[derive(Clone)]
    struct LoadedRepository(ProvenApplicationComponent);

    impl ComponentRepository<ComponentApplicationKey, ProvenApplicationComponent> for LoadedRepository {
        type Error = Infallible;

        fn load_complete(
            &self,
            _key: &ComponentApplicationKey,
        ) -> Result<Option<ProvenApplicationComponent>, Self::Error> {
            Ok(Some(self.0.clone()))
        }

        fn store_complete(
            &self,
            _key: &ComponentApplicationKey,
            _value: &ProvenApplicationComponent,
        ) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    #[test]
    fn loaded_ledger_is_only_an_incumbent_and_live_work_reproves_it() {
        let discovered = splitter_discovery();
        let ComponentOptimizationOutcome::Complete(proven) = optimize_component_application(
            discovered.request.key(),
            &discovered.component,
            &AtomicBool::new(false),
        )
        .unwrap() else {
            panic!("uncancelled splitter proof must complete");
        };
        let mut untrusted = *proven;
        untrusted.proof.exhausted_profiles.clear();
        assert!(!verify_application_optimality_manifest(
            &untrusted.proof,
            &untrusted.component
        ));

        let repository = EmptyComponentStore;
        let provider = LoadedProvider(untrusted);
        let runtime = ApplicationComponentRuntime::new(&repository, &provider);
        let first = runtime.resolve(
            &discovered.component,
            &discovered.request,
            &AtomicBool::new(false),
        );
        assert!(first.component.is_some());
        assert_eq!(first.source_lookups, 2);
        assert_eq!(first.source_hits, 1);
        assert_eq!(first.optimizations, 1);

        let scaled = ComponentApplicationRequest::new(
            discovered.component.boundary(),
            &[rational("20")],
            &[rational("10"), rational("10")],
            &rational("20"),
        )
        .unwrap();
        assert_eq!(scaled.key(), discovered.request.key());
        let second = runtime.resolve(&discovered.component, &scaled, &AtomicBool::new(false));
        assert!(second.component.is_some());
        assert_eq!(second.source_lookups, 0);
        assert_eq!(second.optimizations, 0);
    }

    #[test]
    fn completed_application_work_replaces_a_more_expensive_candidate() {
        let candidate = redundant_identity_component();
        assert_eq!(candidate.cost().nodes, 4);
        let request = ComponentApplicationRequest::new(
            candidate.boundary(),
            &[rational("1")],
            &[rational("1")],
            &rational("1"),
        )
        .unwrap();
        let store = EmptyComponentStore;
        let runtime = ApplicationComponentRuntime::new(&store, &store);
        let resolution = runtime.resolve(&candidate, &request, &AtomicBool::new(false));
        let winner = resolution.component.unwrap();
        assert_eq!(winner.cost().nodes, 2);
        assert!(winner.cost() < candidate.cost());
        assert!(
            match_component_application(&winner, request.key())
                .unwrap()
                .is_some()
        );
    }

    #[derive(Clone, Copy)]
    struct BrokenStore;

    impl ComponentRepository<ComponentApplicationKey, ProvenApplicationComponent> for BrokenStore {
        type Error = &'static str;

        fn load_complete(
            &self,
            _key: &ComponentApplicationKey,
        ) -> Result<Option<ProvenApplicationComponent>, Self::Error> {
            Err("unavailable")
        }

        fn store_complete(
            &self,
            _key: &ComponentApplicationKey,
            _value: &ProvenApplicationComponent,
        ) -> Result<(), Self::Error> {
            Err("read only")
        }
    }

    impl ComponentProvider<ComponentApplicationKey, ProvenApplicationComponent> for BrokenStore {
        type Error = &'static str;

        fn provide(
            &self,
            _key: &ComponentApplicationKey,
        ) -> Result<Option<ProvenApplicationComponent>, Self::Error> {
            Err("unavailable")
        }
    }

    #[test]
    fn source_and_store_errors_leave_the_primitive_incumbent_available() {
        let discovered = splitter_discovery();
        let store = BrokenStore;
        let runtime = ApplicationComponentRuntime::new(&store, &store);
        let resolved = runtime.resolve(
            &discovered.component,
            &discovered.request,
            &AtomicBool::new(false),
        );
        assert!(resolved.component.is_some());
        assert_eq!(resolved.source_lookups, 2);
        assert_eq!(resolved.source_hits, 0);
        assert_eq!(resolved.optimizations, 1);
    }

    #[test]
    fn cancelled_component_work_is_not_cached_as_complete() {
        let discovered = splitter_discovery();
        let store = EmptyComponentStore;
        let runtime = ApplicationComponentRuntime::new(&store, &store);
        let cancelled = runtime.resolve(
            &discovered.component,
            &discovered.request,
            &AtomicBool::new(true),
        );
        assert!(cancelled.component.is_none());
        assert_eq!(cancelled.optimizations, 0);

        let completed = runtime.resolve(
            &discovered.component,
            &discovered.request,
            &AtomicBool::new(false),
        );
        assert!(completed.component.is_some());
        assert_eq!(completed.optimizations, 1);
    }

    #[test]
    fn warm_cold_and_source_position_return_the_same_outer_witness() {
        let discovered = splitter_discovery();
        let ComponentOptimizationOutcome::Complete(proven) = optimize_component_application(
            discovered.request.key(),
            &discovered.component,
            &AtomicBool::new(false),
        )
        .unwrap() else {
            panic!("uncancelled splitter proof must complete");
        };
        let proven = *proven;
        let problem = Problem {
            inputs: vec![rational("2")],
            outputs: vec![rational("1"), rational("1")],
            max_link_rate: rational("2"),
        };
        let options = SolveOptions {
            max_nodes: Some(1),
            ..SolveOptions::default()
        };
        let cancel = AtomicBool::new(false);
        let cold = solve(&problem, &options, &cancel).unwrap();

        let empty = EmptyComponentStore;
        let provider = LoadedProvider(proven.clone());
        let provider_runtime = ApplicationComponentRuntime::new(&empty, &provider);
        let from_provider =
            solve_with_component_resolver(&problem, &options, &cancel, &provider_runtime).unwrap();

        let repository = LoadedRepository(proven);
        let repository_runtime = ApplicationComponentRuntime::new(&repository, &empty);
        let from_repository =
            solve_with_component_resolver(&problem, &options, &cancel, &repository_runtime)
                .unwrap();

        assert_eq!(from_provider, cold);
        assert_eq!(from_repository, cold);
    }
}
