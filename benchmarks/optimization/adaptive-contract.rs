#[test]
fn adaptive_children_follow_optimum_and_keep_one_complete_proof_cover() {
    let p = problem(&["258"], &["195", "63"], "1200");
    let cancel = AtomicBool::new(false);
    let traces = Mutex::new(Vec::<serde_json::Value>::new());
    let outcome = solver_core::solve_problem(
        &p,
        &options(SolveMode::AllMinNL, 9, 32),
        &cancel,
        &|event| {
            if let SolverEvent::Progress(progress) = event {
                for diagnostic in progress.custom {
                    if diagnostic.name == "solver.root" {
                        let value = serde_json::to_value(diagnostic.value).unwrap();
                        traces
                            .lock()
                            .unwrap()
                            .push(serde_json::from_str(value["value"].as_str().unwrap()).unwrap());
                    }
                }
            }
        },
    )
    .unwrap();
    assert!(matches!(
        outcome.enumeration,
        EnumerationStatus::AllMinNL { complete: true, .. }
    ));
    assert_eq!(outcome.solutions.len(), 2);
    for solution in &outcome.solutions {
        validate_solution(&p, &solution.graph).unwrap();
    }
    let traces = traces.into_inner().unwrap();
    let children: Vec<_> = traces
        .iter()
        .filter(|r| r.get("parent_root").is_some() && !r["second_source"].is_null())
        .collect();
    assert!(!children.is_empty(), "adaptive path must actually run");
    for child in &children {
        assert!(
            child["refinement_trigger_s"].as_f64().unwrap() <= child["start_s"].as_f64().unwrap()
        );
        let parent = traces
            .iter()
            .find(|r| {
                r["branch"] == child["branch"]
                    && r["nodes"] == child["nodes"]
                    && r["links"] == child["links"]
                    && r["root"] == child["parent_root"]
            })
            .unwrap();
        assert!(parent["second_source"].is_null());
        assert_eq!(parent["source"], child["source"]);
        assert!(!(parent["proof_committed"] == true && child["proof_committed"] == true));
    }
    let mut timeline = Vec::new();
    for root in traces.iter().filter(|r| r.get("parent_root").is_some()) {
        let start = root["start_s"].as_f64().unwrap();
        let end = start + root["wall_s"].as_f64().unwrap();
        if end > start {
            timeline.push((start, 1_i32));
            timeline.push((end, -1_i32));
        }
        if root["proof_committed"] == true {
            assert_eq!(root["state"], "exhausted");
        }
    }
    timeline.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let mut running = 0;
    for (_, delta) in timeline {
        running += delta;
        assert!((0..=16).contains(&running));
    }
    assert_eq!(running, 0);
}

#[test]
fn adaptive_cancellation_after_child_dispatch_keeps_a_valid_incumbent() {
    let p = problem(&["258"], &["195", "63"], "1200");
    let cancel = AtomicBool::new(false);
    let started = std::time::Instant::now();
    let outcome = solver_core::solve_problem(
        &p,
        &options(SolveMode::AllMinNL, 9, 32),
        &cancel,
        &|event| {
            if let SolverEvent::Progress(progress) = event
                && progress.best_node_count.is_some()
                && started.elapsed() > std::time::Duration::from_secs(4)
            {
                cancel.store(true, Ordering::Relaxed);
            }
        },
    )
    .unwrap();
    assert!(cancel.load(Ordering::Relaxed));
    assert!(matches!(
        outcome.enumeration,
        EnumerationStatus::AllMinNL {
            complete: false,
            ..
        }
    ));
    let SolveResult::Incomplete(result) = outcome.result else {
        panic!("cancellation must stay incomplete")
    };
    assert_eq!(result.reason, IncompleteReason::Cancelled);
    let best = result
        .best_known
        .expect("validated optimum witness survives cancellation");
    validate_solution(&p, &best.graph).unwrap();
}
