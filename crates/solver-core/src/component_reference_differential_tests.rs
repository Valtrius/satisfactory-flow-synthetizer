//! Differential checks between the production component optimizer and the
//! deliberately independent exhaustive component-topology oracle.
//!
//! The oracle enumerates raw physical port bijections without calling any
//! production search, propagation, SCC, component, memoization, or pruning
//! code.  These tests then certify each oracle topology through the ordinary
//! exact frozen-subsystem path and independently solve/validate every concrete
//! witness before it is allowed to become an expected winner.

use std::{collections::BTreeMap, sync::atomic::AtomicBool};

use crate::{
    canonical::{PartialLink, PartialTopology},
    component_application::{
        ComponentApplicationKey, ComponentApplicationRequest, match_component_application,
    },
    component_optimizer::{
        ComponentOptimizationOutcome, optimize_component_application,
        verify_application_optimality_manifest,
    },
    components::{
        Component, ComponentCost, DominanceDecision, try_prove_global_dominance,
        verify_global_component_dominance,
    },
    scc::{
        DeclaredBoundaryInput, DeclaredBoundaryOutput, FrozenSubsystemAnalysis,
        FrozenSubsystemDeclaration, analyze_frozen_subsystem,
    },
    topology::TopologyState,
};
use solver_api::{NodeProfile, Problem, Rational};
use solver_reference::{
    CanonicalComponentTopology, ComponentCell, ComponentCellEnumeration,
    ComponentEnumerationBounds, component_cells, enumerate_component_cell,
};
use solver_validation::{solve_topology, validate_solution};

#[derive(Clone)]
struct OracleCandidate {
    topology: CanonicalComponentTopology,
    component: Component,
    canonical_inputs: Vec<Rational>,
    capacity: Rational,
}

fn complete_cell(cell: ComponentCell) -> Vec<CanonicalComponentTopology> {
    match enumerate_component_cell(cell, &AtomicBool::new(false)).unwrap() {
        ComponentCellEnumeration::Complete {
            topologies,
            manifest,
        } => {
            assert_eq!(
                manifest.canonical_topology_keys,
                topologies
                    .iter()
                    .map(|topology| topology.key.clone())
                    .collect::<Vec<_>>()
            );
            topologies
        }
        ComponentCellEnumeration::Cancelled { .. } => {
            panic!("an unset cancellation flag cannot yield a partial cell")
        }
    }
}

fn component_from_oracle_topology(topology: &CanonicalComponentTopology) -> Option<Component> {
    let input_count = usize::try_from(topology.cell.boundary_input_count).unwrap();
    let output_count = usize::try_from(topology.cell.boundary_output_count).unwrap();

    // This balanced dummy problem only supplies terminal dimensions to the
    // partial topology.  Frozen analysis derives R/T/K symbolically and does
    // not use these concrete rates as a correctness shortcut.
    let input_rate = Rational::from(topology.cell.boundary_output_count);
    let output_rate = Rational::from(topology.cell.boundary_input_count);
    let total_rate = &input_rate * &Rational::from(topology.cell.boundary_input_count);
    let partial = PartialTopology {
        problem: Problem {
            inputs: vec![input_rate; input_count],
            outputs: vec![output_rate; output_count],
            max_link_rate: total_rate,
        },
        nodes: topology.nodes.clone(),
        links: topology
            .internal_links
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
    let state = TopologyState::from_partial_topology(&partial).unwrap();
    let declaration = FrozenSubsystemDeclaration {
        nodes: topology.nodes.iter().map(|node| node.id).collect(),
        boundary_inputs: topology
            .boundary_inputs
            .iter()
            .copied()
            .map(|port| DeclaredBoundaryInput { port })
            .collect(),
        boundary_outputs: topology
            .boundary_outputs
            .iter()
            .copied()
            .map(|port| DeclaredBoundaryOutput { port })
            .collect(),
    };
    match analyze_frozen_subsystem(&state, &declaration).unwrap() {
        FrozenSubsystemAnalysis::Symbolic(frozen) => Component::from_frozen(*frozen).ok(),
        FrozenSubsystemAnalysis::Singular { .. } => None,
    }
}

fn witness_order<T: Clone>(canonical: &[T], order: &[usize]) -> Vec<T> {
    assert_eq!(canonical.len(), order.len());
    let mut witness = vec![None; canonical.len()];
    for (canonical_index, &witness_index) in order.iter().enumerate() {
        assert!(
            witness[witness_index]
                .replace(canonical[canonical_index].clone())
                .is_none()
        );
    }
    witness.into_iter().collect::<Option<Vec<_>>>().unwrap()
}

fn independently_validate_application(candidate: &OracleCandidate) -> ComponentApplicationKey {
    let application = candidate
        .component
        .evaluate(&candidate.canonical_inputs, &candidate.capacity)
        .unwrap();
    let witness_inputs = witness_order(
        &application.inputs,
        &candidate.component.witness_mapping().input_order,
    );
    let witness_outputs = witness_order(
        &application.outputs,
        &candidate.component.witness_mapping().output_order,
    );
    let problem = Problem {
        inputs: witness_inputs,
        outputs: witness_outputs,
        max_link_rate: candidate.capacity.clone(),
    };
    let solved = solve_topology(&problem, &candidate.topology.synthetic_graph()).unwrap();
    validate_solution(&problem, &solved).unwrap();

    ComponentApplicationRequest::new(
        candidate.component.boundary(),
        &application.inputs,
        &application.outputs,
        &candidate.capacity,
    )
    .unwrap()
    .key()
    .clone()
}

fn oracle_candidates(bounds: ComponentEnumerationBounds) -> Vec<OracleCandidate> {
    let mut candidates = Vec::new();
    for cell in component_cells(bounds).unwrap() {
        for topology in complete_cell(cell) {
            let Some(component) = component_from_oracle_topology(&topology) else {
                continue;
            };
            assert_eq!(component.profile(), cell.profile);
            assert_eq!(component.internal_link_count(), cell.internal_link_count);
            let candidate = OracleCandidate {
                canonical_inputs: vec![Rational::one(); component.boundary().input_count()],
                capacity: Rational::from(1_000_u32),
                topology,
                component,
            };
            if candidate
                .component
                .evaluate(&candidate.canonical_inputs, &candidate.capacity)
                .is_ok()
            {
                candidates.push(candidate);
            }
        }
    }
    candidates
}

fn application_groups(
    bounds: ComponentEnumerationBounds,
) -> BTreeMap<ComponentApplicationKey, Vec<OracleCandidate>> {
    let mut groups = BTreeMap::<ComponentApplicationKey, Vec<OracleCandidate>>::new();
    for candidate in oracle_candidates(bounds) {
        let key = independently_validate_application(&candidate);
        groups.entry(key).or_default().push(candidate);
    }
    for candidates in groups.values_mut() {
        candidates.sort_by(|left, right| {
            (left.component.cost(), left.component.canonical_key())
                .cmp(&(right.component.cost(), right.component.canonical_key()))
        });
        candidates.dedup_by(|left, right| {
            left.component.canonical_key() == right.component.canonical_key()
        });
    }
    groups
}

fn assert_production_matches_oracle(key: &ComponentApplicationKey, candidates: &[OracleCandidate]) {
    let expected = candidates
        .first()
        .expect("oracle application has a witness");
    let incumbent = candidates.last().unwrap();
    let outcome =
        optimize_component_application(key, &incumbent.component, &AtomicBool::new(false)).unwrap();
    let ComponentOptimizationOutcome::Complete(proven) = outcome else {
        panic!("an uncancelled finite application proof must complete")
    };

    assert_eq!(proven.component.cost(), expected.component.cost());
    assert_eq!(
        proven.component.canonical_key(),
        expected.component.canonical_key()
    );
    assert!(verify_application_optimality_manifest(
        &proven.proof,
        &proven.component
    ));
    assert!(
        match_component_application(&proven.component, key)
            .unwrap()
            .is_some()
    );

    // Soundness bridge: each production profile obligation must cover exactly
    // the canonical topology count obtained by the independent raw-bijection
    // oracle for the same finite cell.  No candidate feasibility pruning is
    // allowed to change this structural count.
    for obligation in &proven.proof.exhausted_profiles {
        let cell = ComponentCell::new(
            obligation.profile,
            u32::try_from(key.inputs().len()).unwrap(),
            u32::try_from(key.outputs().len()).unwrap(),
        )
        .unwrap();
        assert_eq!(cell.internal_link_count, obligation.internal_link_count);
        let reference_topologies = complete_cell(cell);
        assert_eq!(
            obligation.canonical_topologies_checked,
            u64::try_from(reference_topologies.len()).unwrap(),
            "production/reference topology coverage diverged for {cell:?}"
        );
    }
}

#[test]
fn every_one_node_application_matches_the_reference_cell_oracle() {
    let groups = application_groups(ComponentEnumerationBounds {
        max_nodes: 1,
        max_boundary_ports: None,
    });
    assert_eq!(groups.len(), 4, "one application per physical node type");
    for (key, candidates) in groups {
        assert_production_matches_oracle(&key, &candidates);
    }
}

#[test]
fn exhaustive_two_node_one_by_one_applications_match_cost_and_canonical_winner() {
    let groups = application_groups(ComponentEnumerationBounds {
        max_nodes: 2,
        max_boundary_ports: Some(2),
    });
    assert!(!groups.is_empty());
    assert!(
        groups.values().any(|candidates| candidates.len() > 1),
        "the differential must exercise a real canonical tie or dominance choice"
    );
    for (key, candidates) in groups {
        assert_eq!(key.inputs().len(), 1);
        assert_eq!(key.outputs().len(), 1);
        assert_production_matches_oracle(&key, &candidates);
    }
}

#[test]
fn cancellation_never_manufactures_a_component_proof_or_reference_manifest() {
    let cell = ComponentCell::new(
        NodeProfile {
            splitter2: 1,
            splitter3: 0,
            merger2: 1,
            merger3: 0,
        },
        1,
        1,
    )
    .unwrap();
    assert!(matches!(
        enumerate_component_cell(cell, &AtomicBool::new(true)).unwrap(),
        ComponentCellEnumeration::Cancelled { .. }
    ));

    let (key, candidates) = application_groups(ComponentEnumerationBounds {
        max_nodes: 1,
        max_boundary_ports: Some(3),
    })
    .into_iter()
    .next()
    .unwrap();
    let incumbent = &candidates[0].component;
    let outcome = optimize_component_application(&key, incumbent, &AtomicBool::new(true)).unwrap();
    let ComponentOptimizationOutcome::Incomplete {
        best_known,
        exhausted_profiles,
    } = outcome
    else {
        panic!("pre-cancellation must not return a persistable optimality proof")
    };
    assert!(exhausted_profiles.is_empty());
    assert_eq!(best_known.canonical_key(), incumbent.canonical_key());
    assert!(
        match_component_application(&best_known, &key)
            .unwrap()
            .is_some()
    );
}

#[test]
fn manifests_and_winners_are_repeatable_and_incumbent_independent() {
    let cell = ComponentCell::new(
        NodeProfile {
            splitter2: 1,
            splitter3: 0,
            merger2: 1,
            merger3: 0,
        },
        1,
        1,
    )
    .unwrap();
    let first = enumerate_component_cell(cell, &AtomicBool::new(false)).unwrap();
    let second = enumerate_component_cell(cell, &AtomicBool::new(false)).unwrap();
    assert_eq!(first, second);

    let groups = application_groups(ComponentEnumerationBounds {
        max_nodes: 2,
        max_boundary_ports: Some(2),
    });
    let (key, candidates) = groups
        .iter()
        .find(|(_, candidates)| candidates.len() > 1)
        .expect("need multiple independently enumerated incumbents");
    let run = |incumbent: &Component| {
        let ComponentOptimizationOutcome::Complete(proven) =
            optimize_component_application(key, incumbent, &AtomicBool::new(false)).unwrap()
        else {
            panic!("uncancelled proof must complete")
        };
        (proven.component.canonical_key().clone(), proven.proof)
    };
    assert_eq!(run(&candidates[0].component), run(&candidates[0].component));
    assert_eq!(
        run(&candidates[0].component),
        run(&candidates.last().unwrap().component),
        "a certified upper bound may shorten work but cannot change the winner"
    );
}

#[test]
fn reference_oracle_exercises_both_lexicographic_cost_coordinates() {
    let groups = application_groups(ComponentEnumerationBounds {
        max_nodes: 2,
        max_boundary_ports: Some(2),
    });
    let costs = groups
        .values()
        .flatten()
        .map(|candidate| candidate.component.cost())
        .collect::<Vec<_>>();
    assert!(costs.contains(&ComponentCost {
        nodes: 2,
        internal_links: 2,
    }));
    assert!(
        costs
            .iter()
            .any(|cost| { cost.nodes == 2 && cost.internal_links > 2 })
    );
}

#[test]
fn every_small_proven_dominance_preserves_exact_application_and_capacity() {
    let candidates = oracle_candidates(ComponentEnumerationBounds {
        max_nodes: 2,
        max_boundary_ports: Some(2),
    });
    let mut proven_pairs = 0_u32;
    for dominating in &candidates {
        for dominated in &candidates {
            if dominating.component.canonical_key() == dominated.component.canonical_key() {
                continue;
            }
            let DominanceDecision::Proven(proof) =
                try_prove_global_dominance(&dominating.component, &dominated.component)
            else {
                continue;
            };
            proven_pairs += 1;
            assert!(verify_global_component_dominance(
                &proof,
                &dominating.component,
                &dominated.component
            ));

            for rate in 1_u32..=3 {
                let dominated_inputs =
                    vec![Rational::from(rate); dominated.component.boundary().input_count()];
                let probe = dominated
                    .component
                    .evaluate(&dominated_inputs, &Rational::from(10_000_u32))
                    .unwrap();
                let exact_capacity = std::cmp::max(
                    probe.capacity.boundary_peak.clone(),
                    probe.capacity.internal_peak.clone(),
                );
                let dominated_application = dominated
                    .component
                    .evaluate(&dominated_inputs, &exact_capacity)
                    .unwrap();
                let dominating_inputs =
                    witness_order(&dominated_inputs, &proof.dominating_input_order);
                let dominating_application = dominating
                    .component
                    .evaluate(&dominating_inputs, &exact_capacity)
                    .expect("a proven dominator must fit whenever the dominated component fits");
                let oriented_outputs = proof
                    .dominating_output_order
                    .iter()
                    .map(|&index| dominating_application.outputs[index].clone())
                    .collect::<Vec<_>>();
                assert_eq!(oriented_outputs, dominated_application.outputs);
                assert!(
                    dominating_application.capacity.internal_peak
                        <= dominated_application.capacity.internal_peak
                );
            }
        }
    }
    assert!(
        proven_pairs > 0,
        "the bounded oracle must exercise elimination"
    );
}
