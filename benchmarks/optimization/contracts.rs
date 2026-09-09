// Appended to the existing integration suite in isolated variant builds only.
#[test]
fn experiment_enumeration_matches_reference_at_partitioning_worker_budgets() {
    for workers in [1, 2, 8, 16, 32] {
        for p in [
            problem(&["1/3"], &["1/6", "1/6"], "1/3"),
            problem(&["2", "1"], &["1", "1"], "3"),
            problem(&["2", "2"], &["2", "2"], "2"),
        ] {
            compare_workers(&p, SolveMode::AllMinNL, 2, workers);
        }
    }
    for mode in [SolveMode::AllMinNL, SolveMode::AllMinN] {
        let outcome = compare_workers(&problem(&["5"], &["2", "2", "1"], "6"), mode, 3, 32);
        assert!(preferred(&outcome).validation.cyclic_scc_count > 0);
    }
}

#[test]
fn experiment_cancellation_keeps_enumeration_incomplete() {
    let p = problem(&["2", "1"], &["1", "1"], "3");
    let cancel = AtomicBool::new(false);
    let delivered = Mutex::new(Vec::new());
    let outcome = solver_core::solve_problem(
        &p,
        &options(SolveMode::AllMinNL, 2, 32),
        &cancel,
        &|event| {
            if let SolverEvent::SolutionFound(solution) = event {
                delivered.lock().unwrap().push(solution);
                cancel.store(true, Ordering::Relaxed);
            }
        },
    ).unwrap();
    assert!(matches!(outcome.result, SolveResult::Incomplete(ref r) if r.reason == IncompleteReason::Cancelled));
    assert!(matches!(outcome.enumeration, EnumerationStatus::AllMinNL { complete: false, .. }));
    assert!(!delivered.lock().unwrap().is_empty());
    for solution in delivered.lock().unwrap().iter() {
        assert!(outcome.solutions.contains(solution));
        validate_solution(&p, &solution.graph).unwrap();
    }
}
