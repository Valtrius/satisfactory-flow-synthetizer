use solver_api::{
    BestKnownSolution, EnumerationStatus, IncompleteReason, Problem, RunOptions, SolveMode,
    SolveOutcome, SolveResult, SolverEvent,
};
use solver_validation::validate_solution;
use std::{
    collections::BTreeMap,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

fn problem(inputs: &[&str], outputs: &[&str], capacity: &str) -> Problem {
    Problem {
        inputs: inputs.iter().map(|s| s.parse().unwrap()).collect(),
        outputs: outputs.iter().map(|s| s.parse().unwrap()).collect(),
        max_link_rate: capacity.parse().unwrap(),
    }
}
fn options(mode: SolveMode, cap: u32, workers: usize) -> RunOptions {
    RunOptions {
        mode,
        max_nodes: Some(cap),
        worker_count: workers,
    }
}
fn preferred(outcome: &SolveOutcome) -> BestKnownSolution {
    let SolveResult::Optimal(s) = &outcome.result else {
        panic!("{outcome:?}")
    };
    BestKnownSolution {
        node_count: s.node_count,
        link_count: s.link_count,
        physical_link_count: s.physical_link_count,
        discard_link_count: s.discard_link_count,
        canonical_graph_key: s.canonical_graph_key.clone(),
        graph: s.graph.clone(),
        validation: s.validation.clone(),
    }
}
fn compare(problem: &Problem, mode: SolveMode, cap: u32) -> SolveOutcome {
    let cancel = AtomicBool::new(false);
    let opts = options(mode, cap, 2);
    let actual = solver_astra::solve_problem(problem, &opts, &cancel, &|_| {}).unwrap();
    let reference = solver_reference::solve_problem(problem, &opts, &cancel).unwrap();
    assert_eq!(actual.proof, reference.proof, "{problem:?}");
    assert_eq!(actual.enumeration, reference.enumeration, "{problem:?}");
    match (&actual.result, &reference.result) {
        (SolveResult::Optimal(_), SolveResult::Optimal(_)) => assert_eq!(
            preferred(&actual),
            preferred(&reference),
            "full preferred witness for {problem:?}"
        ),
        (SolveResult::Incomplete(a), SolveResult::Incomplete(b)) => {
            assert_eq!(a.best_known, b.best_known);
        }
        (SolveResult::GloballyUnsat(a), SolveResult::GloballyUnsat(b)) => {
            assert_eq!(a.reason, b.reason);
        }
        _ => panic!("result mismatch: {actual:?} vs {reference:?}"),
    }
    let objects = |outcome: &SolveOutcome| {
        outcome
            .solutions
            .iter()
            .map(|s| {
                assert_eq!(validate_solution(problem, &s.graph).unwrap(), s.validation);
                (s.canonical_graph_key.clone(), s.clone())
            })
            .collect::<BTreeMap<_, _>>()
    };
    assert_eq!(
        objects(&actual),
        objects(&reference),
        "full enumeration witnesses for {problem:?}"
    );
    actual
}

#[test]
fn exact_scopes_match_reference_full_witnesses() {
    for p in [
        problem(&["2", "3"], &["1", "4"], "5"),
        problem(&["3"], &["1", "2"], "3"),
        problem(&["1/3"], &["1/6", "1/6"], "1/3"),
        problem(&["2", "1"], &["1", "1"], "3"),
        problem(&["6"], &["2"], "6"),
        problem(&["2", "2"], &["2", "2"], "2"),
        problem(&["1", "2"], &["1"], "2"),
    ] {
        for mode in [
            SolveMode::Optimal,
            SolveMode::AllAtMinimumNodesAndMinimumLinks,
            SolveMode::AllAtMinimumNodes,
        ] {
            compare(&p, mode, 2);
        }
    }
}

#[test]
fn cyclic_fifths_match_reference_with_capacity_above_supply() {
    // Denominator five requires feedback; internal flow reaches six although
    // external supply is five. This exercises direct splitter flow equations
    // together with merger sums and independent cyclic SCC reconstruction.
    let result = compare(
        &problem(&["5"], &["2", "2", "1"], "6"),
        SolveMode::AllAtMinimumNodes,
        3,
    );
    assert!(preferred(&result).validation.cyclic_scc_count > 0);
}

#[test]
fn arbitrary_rational_requests_and_scale_match_reference() {
    // Deterministic requests unrelated to corpus names, including bounded UNSAT.
    let mut seed = 781u32;
    for _ in 0..12 {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let total = 2 + seed % 5;
        let left = 1 + (seed / 7) % (total - 1);
        let denominator = 1 + (seed / 31) % 13;
        let p = problem(
            &[&format!("{total}/{denominator}")],
            &[
                &format!("{left}/{denominator}"),
                &format!("{}/{denominator}", total - left),
            ],
            &format!("{total}/{denominator}"),
        );
        compare(&p, SolveMode::AllAtMinimumNodes, 2);
    }
}

#[test]
fn node_cap_and_global_contradictions_are_distinct() {
    let opts = options(SolveMode::Optimal, 0, 1);
    for p in [
        problem(&["1"], &["2"], "3"),
        problem(&["3"], &["1"], "2"),
        problem(&["2", "2"], &["3"], "2"),
    ] {
        let outcome =
            solver_astra::solve_problem(&p, &opts, &AtomicBool::new(false), &|_| {}).unwrap();
        assert!(matches!(outcome.result, SolveResult::GloballyUnsat(_)));
    }
    let p = problem(&["3"], &["1", "2"], "3");
    let outcome = solver_astra::solve_problem(&p, &opts, &AtomicBool::new(false), &|_| {}).unwrap();
    assert!(
        matches!(outcome.result,SolveResult::Incomplete(ref r) if matches!(r.reason,IncompleteReason::ResourceLimit {..}))
    );
}

#[test]
fn cancellation_preserves_streamed_incumbent_and_enumeration() {
    for mode in [SolveMode::Optimal, SolveMode::AllAtMinimumNodes] {
        let p = problem(&["2", "3"], &["1", "4"], "5");
        let cancel = AtomicBool::new(false);
        let delivered = Mutex::new(Vec::new());
        let outcome =
            solver_astra::solve_problem(&p, &options(mode, 2, 2), &cancel, &|event| match event {
                SolverEvent::Incumbent(s) if mode == SolveMode::Optimal => {
                    delivered.lock().unwrap().push(s);
                    cancel.store(true, Ordering::Relaxed);
                }
                SolverEvent::SolutionFound(s) => {
                    delivered.lock().unwrap().push(s);
                    cancel.store(true, Ordering::Relaxed);
                }
                _ => {}
            })
            .unwrap();
        let SolveResult::Incomplete(result) = &outcome.result else {
            panic!("{outcome:?}")
        };
        assert_eq!(result.reason, IncompleteReason::Cancelled);
        assert!(!delivered.lock().unwrap().is_empty());
        let best = result.best_known.as_ref().unwrap();
        validate_solution(&p, &best.graph).unwrap();
        if mode != SolveMode::Optimal {
            assert!(matches!(
                outcome.enumeration,
                EnumerationStatus::AllAtMinimumNodes {
                    complete: false,
                    ..
                }
            ));
            for s in delivered.lock().unwrap().iter() {
                assert!(outcome.solutions.contains(s));
            }
        }
    }
}

#[test]
fn completed_results_do_not_depend_on_worker_count() {
    let p = problem(&["2", "1"], &["1", "1"], "3");
    let mut outcomes = Vec::new();
    for workers in [1, 2, 4] {
        let outcome = solver_astra::solve_problem(
            &p,
            &options(SolveMode::AllAtMinimumNodes, 2, workers),
            &AtomicBool::new(false),
            &|_| {},
        )
        .unwrap();
        outcomes.push(outcome);
    }
    for pair in outcomes.windows(2) {
        assert_eq!(preferred(&pair[0]), preferred(&pair[1]));
        assert_eq!(pair[0].solutions, pair[1].solutions);
        assert_eq!(pair[0].proof, pair[1].proof);
        assert_eq!(pair[0].enumeration, pair[1].enumeration);
        let (SolveResult::Optimal(a), SolveResult::Optimal(b)) = (&pair[0].result, &pair[1].result)
        else {
            panic!("both runs must complete")
        };
        // Worker counts change the partition count, but not completed profiles
        // or the mathematical proof and result above.
        let mut proof_a = a.proof.clone();
        let mut proof_b = b.proof.clone();
        proof_a.root_partitions_exhausted = 0;
        proof_b.root_partitions_exhausted = 0;
        assert_eq!(proof_a, proof_b);
    }
}

#[test]
fn preferred_witness_uses_reference_byte_order_when_the_optimum_has_several_layouts() {
    use solver_api::{ConsumerPortRef, PhysicalGraph, ProducerPortRef};
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/preferred-order.json")).unwrap();
    let problem: Problem = serde_json::from_value(fixture["problem"].clone()).unwrap();
    let graphs: Vec<PhysicalGraph> = serde_json::from_value(fixture["graphs"].clone()).unwrap();
    let expected: PhysicalGraph =
        serde_json::from_value(fixture["preferredGraph"].clone()).unwrap();
    let solver_core::Preparation::Prepared(normalized) =
        solver_core::prepare_problem(&problem).unwrap()
    else {
        panic!("feasible fixture")
    };
    let normalized_problem = Problem {
        inputs: normalized.inputs.as_slice().to_vec(),
        outputs: normalized.outputs.as_slice().to_vec(),
        max_link_rate: normalized.max_link_rate.clone(),
    };
    let mut ordered = Vec::new();
    for graph in graphs {
        let validation = validate_solution(&problem, &graph).unwrap();
        let mut scaled = graph.clone();
        for link in &mut scaled.links {
            link.flow = &link.flow / &normalized.original_scale;
            if let ProducerPortRef::Input(ref mut id) = link.producer {
                id.0 = u32::try_from(
                    normalized
                        .terminal_mapping
                        .normalized_input(id.0 as usize)
                        .unwrap(),
                )
                .unwrap();
            }
            if let ConsumerPortRef::Output(ref mut id) = link.consumer {
                id.0 = u32::try_from(
                    normalized
                        .terminal_mapping
                        .normalized_output(id.0 as usize)
                        .unwrap(),
                )
                .unwrap();
            }
        }
        let reference = solver_reference::canonicalize_graph(&normalized_problem, &scaled);
        let production = solver_core::canonical::canonicalize_witness(&normalized_problem, &scaled);
        assert_eq!(production.key, reference.key);
        assert_eq!(production.graph, reference.graph);
        ordered.push((
            validation.node_count,
            validation.link_count,
            reference.key,
            graph,
        ));
    }
    ordered.sort_by(|a, b| (&a.0, &a.1, &a.2).cmp(&(&b.0, &b.1, &b.2)));
    assert_eq!(ordered[0].3, expected);
    assert_eq!(ordered.iter().filter(|row| row.1 == 8).count(), 3);

    let cancel = AtomicBool::new(false);
    let actual = std::thread::scope(|scope| {
        let (send, receive) = std::sync::mpsc::channel();
        let flag = &cancel;
        let deadline = scope.spawn(move || {
            if receive
                .recv_timeout(std::time::Duration::from_mins(1))
                .is_err()
            {
                flag.store(true, Ordering::Relaxed);
            }
        });
        let result = solver_astra::solve_problem(
            &problem,
            &options(SolveMode::Optimal, 7, 4),
            &cancel,
            &|_| {},
        )
        .unwrap();
        let _ = send.send(());
        deadline.join().unwrap();
        result
    });
    assert_eq!(preferred(&actual).graph, expected);
}
