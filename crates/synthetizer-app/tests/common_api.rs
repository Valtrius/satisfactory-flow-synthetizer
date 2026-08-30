use solver_api::{
    ConsumerPortRef, EnumerationStatus, IncompleteReason, LinkConstraint, NodeId, Problem,
    ProducerPortRef, RunOptions, SolveMode, SolveOutcome, SolveResult, SolverEvent,
};
use solver_validation::{layout_key, validate_solution};
use std::{
    collections::BTreeSet,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use synthetizer_app::runtime::{SolverEngine, solve};

fn problem(inputs: &[&str], outputs: &[&str], capacity: &str) -> Problem {
    Problem {
        inputs: inputs.iter().map(|r| r.parse().unwrap()).collect(),
        outputs: outputs.iter().map(|r| r.parse().unwrap()).collect(),
        max_link_rate: capacity.parse().unwrap(),
    }
}

fn all_solvers(problem: &Problem, mode: SolveMode, max_nodes: u32) -> Vec<SolveOutcome> {
    let options = RunOptions {
        mode,
        max_nodes: Some(max_nodes),
        worker_count: 1,
    };
    let cancel = AtomicBool::new(false);
    let mut outcomes = [SolverEngine::Custom, SolverEngine::Z3]
        .into_iter()
        .map(|engine| solve(engine, problem, &options, &cancel, &|_| {}).unwrap())
        .collect::<Vec<_>>();
    outcomes.push(solver_reference::solve_problem(problem, &options, &cancel).unwrap());
    outcomes
}

#[test]
fn all_solvers_return_exact_optima_and_round_trip_the_same_schema() {
    let problem = problem(&["3"], &["1", "2"], "3");
    let outcomes = all_solvers(&problem, SolveMode::Optimal, 2);
    for outcome in outcomes {
        let SolveResult::Optimal(best) = &outcome.result else {
            panic!("{outcome:?}")
        };
        assert_eq!((best.node_count, best.link_count), (2, 2));
        assert_eq!(
            validate_solution(&problem, &best.graph).unwrap(),
            best.validation
        );
        assert_eq!(outcome.proof.minimum_node_count, Some(2));
        assert_eq!(outcome.proof.minimum_link_count, Some(2));
        assert_eq!(outcome.enumeration, EnumerationStatus::NotRequested);
        assert!(outcome.solutions.is_empty());
        let json = serde_json::to_string(&outcome).unwrap();
        assert_eq!(
            serde_json::from_str::<SolveOutcome>(&json).unwrap(),
            outcome
        );
    }
}

#[test]
fn complete_enumeration_matches_the_independent_reference_layout_set() {
    for problem in [
        problem(&["3"], &["1", "2"], "3"),
        problem(&["2", "3"], &["1", "4"], "5"),
        problem(&["2", "1"], &["1", "1"], "3"),
        problem(&["1/3"], &["1/6", "1/6"], "1"),
    ] {
        let outcomes = all_solvers(&problem, SolveMode::AllAtMinimumNodes, 2);
        let sets = outcomes
            .iter()
            .map(|outcome| {
                assert!(matches!(
                    outcome.enumeration,
                    EnumerationStatus::AllAtMinimumNodes { complete: true, .. }
                ));
                assert!(!outcome.solutions.is_empty());
                let keys = outcome
                    .solutions
                    .iter()
                    .map(|s| {
                        assert_eq!(validate_solution(&problem, &s.graph).unwrap(), s.validation);
                        assert_eq!(Some(s.node_count), outcome.proof.minimum_node_count);
                        assert_eq!(s.canonical_graph_key, layout_key(&problem, &s.graph));
                        layout_key(&problem, &s.graph)
                    })
                    .collect::<BTreeSet<_>>();
                assert_eq!(keys.len(), outcome.solutions.len());
                keys
            })
            .collect::<Vec<_>>();
        assert_eq!(sets[0], sets[2], "Custom versus Reference: {problem:?}");
        assert_eq!(sets[1], sets[2], "Z3 versus Reference: {problem:?}");
    }
}

#[test]
fn minimum_link_enumeration_matches_the_independent_reference_layout_set() {
    for problem in [
        problem(&["3"], &["1", "2"], "3"),
        problem(&["2", "3"], &["1", "4"], "5"),
        problem(&["2", "1"], &["1", "1"], "3"),
    ] {
        let outcomes = all_solvers(&problem, SolveMode::AllAtMinimumNodesAndMinimumLinks, 2);
        let sets = outcomes
            .iter()
            .map(|outcome| {
                assert!(matches!(
                    outcome.enumeration,
                    EnumerationStatus::AllAtMinimumNodesAndMinimumLinks { complete: true, .. }
                ));
                let minimum_links = outcome.proof.minimum_link_count.unwrap();
                assert!(!outcome.solutions.is_empty());
                outcome
                    .solutions
                    .iter()
                    .map(|solution| {
                        assert_eq!(solution.link_count, minimum_links);
                        assert_eq!(
                            validate_solution(&problem, &solution.graph).unwrap(),
                            solution.validation
                        );
                        layout_key(&problem, &solution.graph)
                    })
                    .collect::<BTreeSet<_>>()
            })
            .collect::<Vec<_>>();
        assert_eq!(sets[0], sets[2], "Custom versus Reference: {problem:?}");
        assert_eq!(sets[1], sets[2], "Z3 versus Reference: {problem:?}");
    }
}

#[test]
fn bounded_search_is_incomplete_but_finite_contradictions_are_unsat() {
    for outcome in all_solvers(&problem(&["3"], &["1", "2"], "3"), SolveMode::Optimal, 0) {
        assert!(
            matches!(outcome.result, SolveResult::Incomplete(ref r) if matches!(r.reason, IncompleteReason::ResourceLimit { .. }))
        );
        assert_eq!(outcome.proof.minimum_node_count, None);
    }
    for problem in [problem(&["1"], &["2"], "3"), problem(&["4"], &["2"], "3")] {
        for outcome in all_solvers(&problem, SolveMode::Optimal, 0) {
            assert!(matches!(outcome.result, SolveResult::GloballyUnsat(_)));
        }
    }
}

#[test]
fn cancellation_keeps_an_incumbent_without_claiming_link_optimality_or_enumeration() {
    let problem = problem(&["3"], &["1", "2"], "3");
    for engine in [SolverEngine::Custom, SolverEngine::Z3] {
        let cancel = AtomicBool::new(false);
        let saw_enumeration = AtomicBool::new(false);
        let outcome = solve(
            engine,
            &problem,
            &RunOptions::default(),
            &cancel,
            &|event| {
                if matches!(event, SolverEvent::Incumbent(_)) {
                    cancel.store(true, Ordering::Relaxed);
                }
                if matches!(event, SolverEvent::SolutionFound(_)) {
                    saw_enumeration.store(true, Ordering::Relaxed);
                }
            },
        )
        .unwrap();
        let SolveResult::Incomplete(incomplete) = outcome.result else {
            panic!("{engine:?}: {outcome:?}")
        };
        assert!(!saw_enumeration.load(Ordering::Relaxed));
        assert_eq!(incomplete.reason, IncompleteReason::Cancelled);
        validate_solution(&problem, &incomplete.best_known.unwrap().graph).unwrap();
        assert_eq!(outcome.proof.minimum_link_count, None);
        assert_eq!(outcome.enumeration, EnumerationStatus::NotRequested);
        assert!(outcome.solutions.is_empty());
    }
}

#[test]
fn interrupted_enumeration_retains_delivered_layouts_in_the_return_value() {
    let problem = problem(&["3"], &["1", "2"], "3");
    for engine in [SolverEngine::Custom, SolverEngine::Z3] {
        let cancel = AtomicBool::new(false);
        let delivered = Mutex::new(Vec::new());
        let options = RunOptions {
            mode: SolveMode::AllAtMinimumNodes,
            ..RunOptions::default()
        };
        let outcome = solve(engine, &problem, &options, &cancel, &|event| {
            if let SolverEvent::SolutionFound(solution) = event {
                delivered.lock().unwrap().push(solution);
                cancel.store(true, Ordering::Relaxed);
            }
        })
        .unwrap();
        assert!(
            matches!(outcome.result, SolveResult::Incomplete(_)),
            "{engine:?}: {outcome:?}"
        );
        assert_eq!(
            outcome.enumeration,
            EnumerationStatus::AllAtMinimumNodes {
                node_count: Some(2),
                complete: false
            }
        );
        assert_eq!(outcome.proof.minimum_link_count, None);
        assert_eq!(outcome.solutions, *delivered.lock().unwrap());
        assert!(!outcome.solutions.is_empty());
    }
}

#[test]
fn progress_distinguishes_exact_link_obligations_from_z3_caps() {
    let problem = problem(&["3"], &["1", "2"], "3");
    for engine in [SolverEngine::Custom, SolverEngine::Z3] {
        let events = Mutex::new(Vec::new());
        solve(
            engine,
            &problem,
            &RunOptions::default(),
            &AtomicBool::new(false),
            &|event| {
                if let SolverEvent::Progress(progress) = event {
                    events.lock().unwrap().push(progress);
                }
            },
        )
        .unwrap();
        let events = events.into_inner().unwrap();
        let constraints = events
            .iter()
            .filter_map(|p| p.link_constraint)
            .collect::<Vec<_>>();
        for progress in events {
            let json = serde_json::to_value(progress).unwrap();
            assert!(json["custom"].is_array());
            assert!(json.get("engine").is_none());
        }
        assert!(!constraints.is_empty());
        assert!(constraints.iter().all(|c| matches!(
            (engine, c),
            (SolverEngine::Custom, LinkConstraint::Exact(_))
                | (SolverEngine::Z3, LinkConstraint::AtMost(_))
        )));
    }
}

#[test]
fn custom_publishes_the_computed_bound_even_when_the_run_cannot_start_search() {
    let problem = problem(&["3"], &["1", "2"], "3");
    let events = Mutex::new(Vec::new());
    let outcome = solve(
        SolverEngine::Custom,
        &problem,
        &RunOptions {
            max_nodes: Some(0),
            ..RunOptions::default()
        },
        &AtomicBool::new(false),
        &|event| {
            events.lock().unwrap().push(event);
        },
    )
    .unwrap();
    let SolveResult::Incomplete(stopped) = outcome.result else {
        panic!()
    };
    assert!(matches!(
        stopped.reason,
        IncompleteReason::ResourceLimit { .. }
    ));
    assert!(stopped.proof.initial_node_lower_bound > 0);
    let events = events.into_inner().unwrap();
    let bounds = events
        .iter()
        .filter_map(|event| match event {
            SolverEvent::Progress(progress)
                if progress.phase == solver_api::SolvePhase::ComputingLowerBound =>
            {
                Some(progress)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(bounds.first().unwrap().node_lower_bound, None);
    let computed = bounds.last().unwrap();
    assert_eq!(
        computed.node_lower_bound,
        Some(stopped.proof.initial_node_lower_bound)
    );
    assert_eq!(computed.node_count, None);
    assert_eq!(computed.link_constraint, None);
    assert!(events.iter().all(|event| !matches!(event, SolverEvent::Progress(p) if p.phase == solver_api::SolvePhase::Searching)));
}

#[test]
fn presentation_payloads_omit_layout_keys_but_live_deduplication_keeps_them() {
    use solver_api::{EndpointRequest, ProblemRequest};
    use synthetizer_app::runtime::Solution;
    let endpoints = |rates: &[&str]| {
        rates
            .iter()
            .enumerate()
            .map(|(i, rate)| EndpointRequest {
                id: i.to_string(),
                name: String::new(),
                rate: (*rate).to_owned(),
            })
            .collect()
    };
    let prepared = ProblemRequest {
        inputs: endpoints(&["2", "3"]),
        outputs: endpoints(&["1", "4"]),
        belt_rate: "5".to_owned(),
    }
    .prepare()
    .unwrap();
    for engine in [SolverEngine::Custom, SolverEngine::Z3] {
        let outcome = solve(
            engine,
            &prepared.problem,
            &RunOptions {
                mode: SolveMode::AllAtMinimumNodes,
                max_nodes: Some(2),
                worker_count: 1,
            },
            &AtomicBool::new(false),
            &|_| {},
        )
        .unwrap();
        let solutions = outcome
            .solutions
            .iter()
            .map(|best| Solution::from_best(engine, &prepared, best).unwrap())
            .collect::<Vec<_>>();
        assert!(solutions.len() > 1);
        for solution in &solutions {
            assert!(!solution.layout_key.as_bytes().is_empty());
            assert!(solution.has_same_layout(&solution.clone()));
            let json = serde_json::to_value(solution).unwrap();
            assert!(json.get("layoutKey").is_none());
            assert!(json["nodes"].is_array());
            assert!(json["validation"].is_object());
        }
        assert!(!solutions[0].has_same_layout(&solutions[1]));
    }
}

#[test]
fn layout_identity_ignores_storage_node_ids_and_symmetric_ports() {
    let problem = problem(&["3"], &["1", "2"], "3");
    let outcome = solver_reference::solve_problem(
        &problem,
        &RunOptions {
            max_nodes: Some(2),
            ..RunOptions::default()
        },
        &AtomicBool::new(false),
    )
    .unwrap();
    let SolveResult::Optimal(best) = outcome.result else {
        panic!()
    };
    let mut renamed = best.graph.clone();
    let kinds = renamed
        .nodes
        .iter()
        .map(|node| (node.id, node.node_type))
        .collect::<std::collections::BTreeMap<_, _>>();
    for link in &mut renamed.links {
        if let ProducerPortRef::Node { node, port } = &mut link.producer {
            *port = kinds[node].output_port_count() - 1 - *port;
            *node = NodeId(100 - node.0);
        }
        if let ConsumerPortRef::Node { node, port } = &mut link.consumer {
            *port = kinds[node].input_port_count() - 1 - *port;
            *node = NodeId(100 - node.0);
        }
    }
    for node in &mut renamed.nodes {
        node.id = NodeId(100 - node.id.0);
    }
    renamed.nodes.reverse();
    renamed.links.reverse();
    validate_solution(&problem, &renamed).unwrap();
    assert_eq!(
        layout_key(&problem, &best.graph),
        layout_key(&problem, &renamed)
    );
    // Rate colors remain part of the identity, unlike names and index order.
    let scaled = Problem {
        inputs: vec![6.into()],
        outputs: vec![2.into(), 4.into()],
        max_link_rate: 6.into(),
    };
    assert_ne!(
        layout_key(&problem, &best.graph),
        layout_key(&scaled, &best.graph)
    );
}

#[test]
fn the_shared_presenter_preserves_names_capacity_discard_and_feedback_for_both_engines() {
    use solver_api::{EndpointRequest, ProblemRequest};
    use synthetizer_app::presentation::{
        GraphNodeKind, PresentedSolveOutcome, present_solve_result,
    };
    let endpoints = |rates: &[&str]| {
        rates
            .iter()
            .enumerate()
            .map(|(i, rate)| EndpointRequest {
                id: format!("terminal-{i}"),
                name: format!("User name {i}"),
                rate: (*rate).to_owned(),
            })
            .collect()
    };
    for (inputs, outputs, expected_n, discards, feedback) in [
        (
            vec!["1200", "1200", "1200"],
            vec!["1200"],
            0,
            Some(2),
            false,
        ),
        (vec!["120"], vec!["60"], 1, Some(1), false),
        (vec!["1"], vec!["1/5"], 3, None, true),
    ] {
        let prepared = ProblemRequest {
            inputs: endpoints(&inputs),
            outputs: endpoints(&outputs),
            belt_rate: "1200".to_owned(),
        }
        .prepare()
        .unwrap();
        for engine in [SolverEngine::Custom, SolverEngine::Z3] {
            let outcome = solve(
                engine,
                &prepared.problem,
                &RunOptions {
                    max_nodes: Some(3),
                    ..RunOptions::default()
                },
                &AtomicBool::new(false),
                &|_| {},
            )
            .unwrap();
            let PresentedSolveOutcome::Optimal(display) =
                present_solve_result(&prepared, &outcome.result).unwrap()
            else {
                panic!("{engine:?}: {outcome:?}")
            };
            assert_eq!(display.status, "proven_optimal");
            assert!(display.proof.is_some());
            assert_eq!(display.stats.node_count, expected_n);
            if let Some(discards) = discards {
                assert_eq!(display.stats.discard_link_count, discards);
            }
            assert_eq!(
                display.stats.discard_link_count,
                display.validation.discard_link_count
            );
            assert_eq!(display.stats.feedback_loops > 0, feedback);
            assert_eq!(
                display
                    .nodes
                    .iter()
                    .filter(|n| n.kind == GraphNodeKind::Discard)
                    .count(),
                display.stats.discard_link_count as usize
            );
            assert!(
                display
                    .nodes
                    .iter()
                    .filter(|n| matches!(n.kind, GraphNodeKind::Input | GraphNodeKind::Output))
                    .all(|n| n.label.contains("User name"))
            );
            assert!(display.edges.iter().all(|edge| {
                edge.rate.exact.parse::<solver_api::Rational>().unwrap()
                    <= prepared.problem.max_link_rate
            }));
            assert_eq!(
                display.edges.iter().filter(|edge| edge.feedback).count(),
                display.stats.feedback_loops as usize
            );
        }
    }
}
