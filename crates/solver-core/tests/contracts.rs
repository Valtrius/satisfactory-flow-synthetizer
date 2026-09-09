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
    let actual = solver_core::solve_problem(problem, &opts, &cancel, &|_| {}).unwrap();
    let reference = solver_reference::solve_problem(problem, &opts, &cancel).unwrap();
    assert_eq!(actual.proof, reference.proof, "{problem:?}");
    assert_eq!(actual.enumeration, reference.enumeration, "{problem:?}");
    match (&actual.result, &reference.result) {
        (SolveResult::Optimal(a), SolveResult::Optimal(b)) => {
            assert_eq!((a.node_count, a.link_count), (b.node_count, b.link_count));
            assert_eq!(validate_solution(problem, &a.graph).unwrap(), a.validation);
        }
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
                {
                    let canonical = solver_reference::canonicalize_graph(problem, &s.graph);
                    let (_, materialized) = solver_validation::canonical_layout(problem, &s.graph);
                    assert_eq!(
                        validate_solution(problem, &materialized).unwrap(),
                        s.validation
                    );
                    let mut restored = canonical.graph.clone();
                    let mut inputs = (0..problem.inputs.len()).collect::<Vec<_>>();
                    inputs.sort_by_key(|&i| &problem.inputs[i]);
                    let mut outputs = (0..problem.outputs.len()).collect::<Vec<_>>();
                    outputs.sort_by_key(|&i| &problem.outputs[i]);
                    for link in &mut restored.links {
                        if let solver_api::ProducerPortRef::Input(ref mut id) = link.producer {
                            id.0 = u32::try_from(inputs[id.0 as usize]).unwrap();
                        }
                        if let solver_api::ConsumerPortRef::Output(ref mut id) = link.consumer {
                            id.0 = u32::try_from(outputs[id.0 as usize]).unwrap();
                        }
                    }
                    assert_eq!(validate_solution(problem, &restored).unwrap(), s.validation);
                    let (_, reference_materialized) =
                        solver_validation::canonical_layout(problem, &restored);
                    assert_eq!(
                        materialized, reference_materialized,
                        "full materialized witness must ignore all allowed relabelings"
                    );
                    (canonical.key, (canonical.graph, s.validation.clone()))
                }
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
        for mode in [SolveMode::OneMinNL, SolveMode::AllMinNL, SolveMode::AllMinN] {
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
        SolveMode::AllMinN,
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
        compare(&p, SolveMode::AllMinN, 2);
    }
}

#[test]
fn node_cap_and_global_contradictions_are_distinct() {
    let opts = options(SolveMode::OneMinNL, 0, 1);
    for p in [
        problem(&["1"], &["2"], "3"),
        problem(&["3"], &["1"], "2"),
        problem(&["2", "2"], &["3"], "2"),
    ] {
        let outcome =
            solver_core::solve_problem(&p, &opts, &AtomicBool::new(false), &|_| {}).unwrap();
        assert!(matches!(outcome.result, SolveResult::GloballyUnsat(_)));
    }
    let p = problem(&["3"], &["1", "2"], "3");
    let outcome = solver_core::solve_problem(&p, &opts, &AtomicBool::new(false), &|_| {}).unwrap();
    assert!(
        matches!(outcome.result,SolveResult::Incomplete(ref r) if matches!(r.reason,IncompleteReason::ResourceLimit {..}))
    );
}

#[test]
fn cancellation_preserves_streamed_incumbent_and_enumeration() {
    for mode in [SolveMode::OneMinNL, SolveMode::AllMinN] {
        let p = problem(&["2", "3"], &["1", "4"], "5");
        let cancel = AtomicBool::new(false);
        let delivered = Mutex::new(Vec::new());
        let outcome =
            solver_core::solve_problem(&p, &options(mode, 2, 2), &cancel, &|event| match event {
                SolverEvent::Incumbent(s) if mode == SolveMode::OneMinNL => {
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
        if mode != SolveMode::OneMinNL {
            assert!(matches!(
                outcome.enumeration,
                EnumerationStatus::AllMinN {
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
        let outcome = solver_core::solve_problem(
            &p,
            &options(SolveMode::AllMinN, 2, workers),
            &AtomicBool::new(false),
            &|_| {},
        )
        .unwrap();
        outcomes.push(outcome);
    }
    for pair in outcomes.windows(2) {
        assert_eq!(
            preferred(&pair[0]).node_count,
            preferred(&pair[1]).node_count
        );
        let objects = |outcome: &SolveOutcome| {
            outcome
                .solutions
                .iter()
                .map(|solution| {
                    let canonical = solver_reference::canonicalize_graph(&p, &solution.graph);
                    (
                        canonical.key,
                        (canonical.graph, solution.validation.clone()),
                    )
                })
                .collect::<BTreeMap<_, _>>()
        };
        assert_eq!(objects(&pair[0]), objects(&pair[1]));
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
fn optimal_returns_a_valid_minimum_without_requiring_a_particular_tie() {
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
        let result = solver_core::solve_problem(
            &problem,
            &options(SolveMode::OneMinNL, 7, 4),
            &cancel,
            &|_| {},
        )
        .unwrap();
        let _ = send.send(());
        deadline.join().unwrap();
        result
    });
    let best = preferred(&actual);
    assert_eq!((best.node_count, best.link_count), (7, 8));
    validate_solution(&problem, &best.graph).unwrap();
    let key = solver_validation::layout_key(&problem, &best.graph);
    assert!(
        ordered
            .iter()
            .filter(|row| row.1 == 8)
            .any(|row| solver_validation::layout_key(&problem, &row.3) == key)
    );
}

#[test]
fn optimal_returns_first_incumbent_without_claiming_equal_link_exhaustion() {
    let p = problem(&["2", "3"], &["1", "4"], "5");
    let first = Mutex::new(None);
    let records = Mutex::new(Vec::<serde_json::Value>::new());
    let proof_owner = Mutex::new(None::<usize>);
    let actual = solver_core::solve_problem(
        &p,
        &options(SolveMode::OneMinNL, 2, 4),
        &AtomicBool::new(false),
        &|event| {
            if let SolverEvent::Progress(progress) = &event {
                for diagnostic in &progress.custom {
                    if diagnostic.name == "astra.portfolio_proof_owner"
                        && let solver_api::DiagnosticValue::Text(value) = &diagnostic.value
                    {
                        *proof_owner.lock().unwrap() = Some(value.parse().unwrap());
                    }
                    if diagnostic.name == "astra.root"
                        && let solver_api::DiagnosticValue::Text(value) = &diagnostic.value
                    {
                        records
                            .lock()
                            .unwrap()
                            .push(serde_json::from_str(value).unwrap());
                    }
                }
            }
            if let SolverEvent::Incumbent(solution) = event {
                first.lock().unwrap().get_or_insert(solution);
            }
        },
    )
    .unwrap();
    assert_eq!(preferred(&actual), first.into_inner().unwrap().unwrap());
    let all = solver_core::solve_problem(
        &p,
        &options(SolveMode::AllMinNL, 2, 4),
        &AtomicBool::new(false),
        &|_| {},
    )
    .unwrap();
    let (SolveResult::Optimal(a), SolveResult::Optimal(b)) = (&actual.result, &all.result) else {
        panic!("both requests must finish")
    };
    assert_eq!((a.node_count, a.link_count), (b.node_count, b.link_count));
    assert_eq!(
        a.proof.link_groups_exhausted + 1,
        b.proof.link_groups_exhausted
    );
    assert_eq!(actual.enumeration, EnumerationStatus::NotRequested);
    assert!(actual.solutions.is_empty());
    if std::env::var_os("ASTRA_DIAGNOSTICS").is_some_and(|value| value == "1") {
        let records = records.into_inner().unwrap();
        let owner = proof_owner
            .into_inner()
            .unwrap()
            .expect("independent proof owner");
        assert!(records.iter().any(|root| root["state"] == "optimum"));
        assert_eq!(
            records
                .iter()
                .filter(|root| root["state"] == "exhausted" && root["branch"] == owner)
                .count() as u64,
            a.proof.root_partitions_exhausted
        );
        for root in records {
            let phases = ["check_s", "validation_s", "identity_s"]
                .iter()
                .map(|k| root[k].as_f64().unwrap())
                .sum::<f64>();
            assert!(phases <= root["wall_s"].as_f64().unwrap());
            assert!(root["duplicates"].as_u64().unwrap() <= root["models"].as_u64().unwrap());
        }
    }
}
