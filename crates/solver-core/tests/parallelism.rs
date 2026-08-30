//! Full enumeration comparisons, including the independent tiny oracle.
use std::{
    collections::BTreeMap,
    sync::{Mutex, atomic::AtomicBool},
};

use solver_api::{
    BestKnownSolution, CanonicalGraphKey, Problem, Rational, RunOptions, SolveMode, SolveResult,
    SolverEvent,
};
use solver_core::{ParallelismOptions, SolveOptions, enumerate_with_observer, solve_with_observer};
use solver_validation::validate_solution;

fn stages() -> Vec<ParallelismOptions> {
    vec![
        ParallelismOptions {
            deep_partitions: true,
            shared_state_cache: true,
            work_stealing: true,
            parallel_remaining_groups: true,
        },
        ParallelismOptions {
            parallel_remaining_groups: true,
            ..ParallelismOptions::default()
        },
        ParallelismOptions {
            deep_partitions: true,
            parallel_remaining_groups: true,
            ..ParallelismOptions::default()
        },
        ParallelismOptions {
            deep_partitions: true,
            shared_state_cache: true,
            parallel_remaining_groups: true,
            ..ParallelismOptions::default()
        },
        ParallelismOptions {
            deep_partitions: true,
            shared_state_cache: true,
            work_stealing: true,
            parallel_remaining_groups: false,
        },
        ParallelismOptions::default(),
        ParallelismOptions {
            deep_partitions: true,
            ..ParallelismOptions::default()
        },
        ParallelismOptions {
            deep_partitions: true,
            shared_state_cache: true,
            ..ParallelismOptions::default()
        },
        ParallelismOptions {
            shared_state_cache: true,
            ..ParallelismOptions::default()
        },
    ]
}

fn enumerate(
    problem: &Problem,
    parallelism: ParallelismOptions,
    workers: usize,
) -> (SolveResult, BTreeMap<CanonicalGraphKey, BestKnownSolution>) {
    let delivered = Mutex::new(Vec::new());
    let result = enumerate_with_observer(
        problem,
        &SolveOptions {
            max_nodes: Some(2),
            worker_count: workers,
            parallelism,
        },
        &AtomicBool::new(false),
        &|event| {
            if let SolverEvent::SolutionFound(solution) = event {
                delivered.lock().unwrap().push(solution);
            }
        },
    )
    .unwrap();
    let mut layouts = BTreeMap::new();
    for solution in delivered.into_inner().unwrap() {
        validate_solution(problem, &solution.graph).unwrap();
        assert!(
            layouts
                .insert(solution.canonical_graph_key.clone(), solution)
                .is_none(),
            "duplicate event"
        );
    }
    (result, layouts)
}

#[test]
fn cancellation_preserves_delivered_layouts_for_every_stage() {
    use std::sync::atomic::Ordering;
    let problem = Problem {
        inputs: vec![2.into(), 3.into()],
        outputs: vec![1.into(), 4.into()],
        max_link_rate: 5.into(),
    };
    for parallelism in stages() {
        let cancel = AtomicBool::new(false);
        let delivered = Mutex::new(Vec::new());
        let result = enumerate_with_observer(
            &problem,
            &SolveOptions {
                max_nodes: Some(2),
                worker_count: 4,
                parallelism,
            },
            &cancel,
            &|event| {
                if let SolverEvent::SolutionFound(solution) = event {
                    delivered.lock().unwrap().push(solution);
                    cancel.store(true, Ordering::Relaxed);
                }
            },
        )
        .unwrap();
        let SolveResult::Incomplete(incomplete) = result else {
            panic!("cancelled stage claimed completion: {parallelism:?}")
        };
        assert!(incomplete.best_known.is_some());
        let delivered = delivered.into_inner().unwrap();
        assert!(!delivered.is_empty());
        for solution in delivered {
            validate_solution(&problem, &solution.graph).unwrap();
        }
    }
}

#[test]
fn active_cancellation_drains_every_experimental_scheduler() {
    use std::{
        sync::{atomic::Ordering, mpsc},
        thread,
        time::{Duration, Instant},
    };
    let problem = Problem {
        inputs: vec![40.into(), 25.into()],
        outputs: vec![13.into(), 13.into(), 13.into(), 13.into(), 13.into()],
        max_link_rate: 60.into(),
    };
    for parallelism in stages() {
        for (mode, delay) in [
            (SolveMode::AllAtMinimumNodes, 1),
            (SolveMode::AllAtMinimumNodes, 30),
            (SolveMode::Optimal, 1),
            (SolveMode::Optimal, 30),
        ] {
            let cancel = AtomicBool::new(false);
            let (done, finished) = mpsc::channel();
            let start = Instant::now();
            let result = thread::scope(|scope| {
                let timer_cancel = &cancel;
                scope.spawn(move || {
                    if finished.recv_timeout(Duration::from_millis(delay)).is_err() {
                        timer_cancel.store(true, Ordering::Relaxed);
                    }
                });
                let solve = match mode {
                    SolveMode::AllAtMinimumNodes => enumerate_with_observer,
                    SolveMode::AllAtMinimumNodesAndMinimumLinks => {
                        solver_core::enumerate_minimum_links_with_observer
                    }
                    SolveMode::Optimal => solve_with_observer,
                };
                let result = solve(
                    &problem,
                    &SolveOptions {
                        max_nodes: Some(6),
                        worker_count: 4,
                        parallelism,
                    },
                    &cancel,
                    &|_| {},
                )
                .unwrap();
                let _ = done.send(());
                result
            });
            assert!(
                matches!(result, SolveResult::Incomplete(_)),
                "stage {parallelism:?} did not remain incomplete"
            );
            assert!(start.elapsed() < Duration::from_secs(8));
        }
    }
}

#[test]
fn donation_without_shared_results_is_rejected() {
    let problem = Problem {
        inputs: vec![1.into()],
        outputs: vec![1.into()],
        max_link_rate: 1.into(),
    };
    assert!(matches!(
        enumerate_with_observer(
            &problem,
            &SolveOptions {
                parallelism: ParallelismOptions {
                    work_stealing: true,
                    ..ParallelismOptions::default()
                },
                ..SolveOptions::default()
            },
            &AtomicBool::new(false),
            &|_| {}
        ),
        Err(solver_core::SolverError::InvalidParallelism)
    ));
}

#[test]
fn full_enumeration_matches_reference_for_every_stage_and_worker_count() {
    for (inputs, outputs, capacity) in [
        (vec![2], vec![1, 1], 2),
        (vec![1, 1], vec![2], 2),
        (vec![2, 3], vec![1, 4], 5),
        (vec![3], vec![1], 2),
        (vec![2], vec![1], 2),
    ] {
        let problem = Problem {
            inputs: inputs.into_iter().map(Rational::from).collect(),
            outputs: outputs.into_iter().map(Rational::from).collect(),
            max_link_rate: Rational::from(capacity),
        };
        let oracle = solver_reference::solve_problem(
            &problem,
            &RunOptions {
                mode: SolveMode::AllAtMinimumNodes,
                max_nodes: Some(2),
                worker_count: 1,
            },
            &AtomicBool::new(false),
        )
        .unwrap();
        let expected: std::collections::BTreeSet<_> = oracle
            .solutions
            .iter()
            .map(|s| s.canonical_graph_key.clone())
            .collect();
        let (baseline_result, baseline) = enumerate(&problem, ParallelismOptions::default(), 1);
        for stage in stages() {
            for workers in [1, 4, 32] {
                let (result, actual) = enumerate(&problem, stage, workers);
                assert_eq!(actual, baseline, "stage={stage:?}, workers={workers}");
                let optimal = solve_with_observer(
                    &problem,
                    &SolveOptions {
                        max_nodes: Some(2),
                        worker_count: workers,
                        parallelism: stage,
                    },
                    &AtomicBool::new(false),
                    &|event| {
                        assert!(
                            !matches!(event, SolverEvent::SolutionFound(_)),
                            "optimal mode must not emit enumeration results"
                        );
                    },
                )
                .unwrap();
                match (&optimal, &result) {
                    (SolveResult::Optimal(single), SolveResult::Optimal(all)) => {
                        validate_solution(&problem, &single.graph).unwrap();
                        assert_eq!(
                            (single.node_count, single.link_count),
                            (all.node_count, all.link_count)
                        );
                        assert!(actual.contains_key(&single.canonical_graph_key));
                    }
                    (SolveResult::Incomplete(_), SolveResult::Incomplete(_))
                    | (SolveResult::GloballyUnsat(_), SolveResult::GloballyUnsat(_)) => {}
                    _ => panic!("solve and enumeration disagree: {optimal:?} vs {result:?}"),
                }
                let mut normalized = solver_api::SolveOutcome::new(
                    result.clone(),
                    SolveMode::AllAtMinimumNodes,
                    actual.into_values().collect(),
                );
                solver_validation::normalize_outcome_identity(&problem, &mut normalized);
                assert_eq!(
                    normalized
                        .solutions
                        .iter()
                        .map(|s| s.canonical_graph_key.clone())
                        .collect::<std::collections::BTreeSet<_>>(),
                    expected
                );
                match (&result, &oracle.result) {
                    (SolveResult::Optimal(a), SolveResult::Optimal(b)) => {
                        assert_eq!((a.node_count, a.link_count), (b.node_count, b.link_count));
                        let SolveResult::Optimal(base) = &baseline_result else {
                            panic!("baseline was not optimal")
                        };
                        assert_eq!(a.canonical_graph_key, base.canonical_graph_key);
                    }
                    (SolveResult::GloballyUnsat(_), SolveResult::GloballyUnsat(_))
                    | (SolveResult::Incomplete(_), SolveResult::Incomplete(_)) => {}
                    _ => panic!("result mismatch: {result:?} versus {:?}", oracle.result),
                }
            }
        }
    }
}
