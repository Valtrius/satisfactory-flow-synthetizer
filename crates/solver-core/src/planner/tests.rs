use super::*;
use crate::{leaf::LeafDriver, profile::ProfileLinkAccounting};
use solver_api::{EnumerationStatus, NodeProfile};

fn problem(inputs: &[&str], outputs: &[&str], capacity: &str) -> Problem {
    Problem {
        inputs: inputs.iter().map(|s| s.parse().unwrap()).collect(),
        outputs: outputs.iter().map(|s| s.parse().unwrap()).collect(),
        max_link_rate: capacity.parse().unwrap(),
    }
}

fn options(mode: SolveMode, workers: usize) -> RunOptions {
    RunOptions {
        mode,
        worker_count: workers,
        max_nodes: Some(3),
    }
}

fn direct(mode: SolveMode) -> (ExactPlanner, LeafTask, BestKnownSolution) {
    let mut planner = ExactPlanner::new(
        &problem(&["1/3"], &["1/3"], "1"),
        options(mode, 1),
        Counts::Boolean,
    )
    .unwrap();
    let update = planner.poll(0);
    assert_eq!(update.dispatch.len(), 1);
    let task = update.dispatch[0];
    let mut leaf = LeafDriver::new(planner.context().unwrap(), task.spec);
    leaf.advance(None, false, &mut ()).unwrap();
    leaf.advance(Some("sat"), false, &mut ()).unwrap();
    let witness = leaf
        .advance(Some("((e0 true))"), false, &mut ())
        .unwrap()
        .witness
        .unwrap();
    (planner, task, witness)
}

#[test]
fn cancellation_before_sealing_wins_and_sealed_results_reject_late_packets() {
    for cancel_before_seal in [true, false] {
        let (mut planner, task, witness) = direct(SolveMode::AllMinNL);
        planner
            .accept(PlannerEvent::Witness(task.id, witness.clone()), 1)
            .unwrap();
        planner
            .accept(PlannerEvent::Retired(task.id, Ok(Completion::Exhausted)), 2)
            .unwrap();
        assert!(
            planner.result().is_none(),
            "retirement is not the completion seal"
        );
        if cancel_before_seal {
            assert!(planner.cancel());
        }
        let update = planner.poll(3);
        assert!(update.dispatch.is_empty());
        let outcome = planner.outcome().unwrap();
        if cancel_before_seal {
            assert!(
                matches!(outcome.result,SolveResult::Incomplete(ref r) if r.reason==IncompleteReason::Cancelled && r.best_known.is_some())
            );
            assert!(matches!(
                outcome.enumeration,
                EnumerationStatus::AllMinNL {
                    complete: false,
                    ..
                }
            ));
        } else {
            assert!(matches!(outcome.result, SolveResult::Optimal(_)));
            assert!(matches!(
                outcome.enumeration,
                EnumerationStatus::AllMinNL { complete: true, .. }
            ));
        }
        assert!(!planner.cancel());
        assert!(
            planner
                .accept(PlannerEvent::Witness(task.id, witness), 4)
                .is_err()
        );
        assert!(
            planner
                .accept(PlannerEvent::Retired(task.id, Ok(Completion::Exhausted)), 5)
                .is_err()
        );
        assert_eq!(planner.outcome().unwrap(), outcome);
    }
}

#[test]
fn a_valid_witness_is_not_completion_and_failure_keeps_it_without_exhaustion() {
    let (mut planner, task, witness) = direct(SolveMode::AllMinNL);
    planner
        .accept(PlannerEvent::Witness(task.id, witness.clone()), 1)
        .unwrap();
    planner
        .accept(PlannerEvent::Witness(task.id, witness), 2)
        .unwrap();
    let update = planner.poll(3);
    assert_eq!(
        update
            .events
            .iter()
            .filter(|e| matches!(e, SolverEvent::SolutionFound(_)))
            .count(),
        1
    );
    assert!(planner.result().is_none());
    assert!(
        planner
            .accept(
                PlannerEvent::Retired(
                    task.id,
                    Err(Failure::Worker("unknown (RESOURCEOUT)".into()))
                ),
                4
            )
            .is_err()
    );
    assert!(planner.poll(5).dispatch.is_empty());
    let outcome = planner.outcome().unwrap();
    assert!(
        matches!(outcome.result,SolveResult::Incomplete(ref r) if r.best_known.is_some() && matches!(r.reason,IncompleteReason::WorkerFailed {..}))
    );
    assert_eq!(outcome.solutions.len(), 1);
    assert_eq!(planner.proof().root_partitions_exhausted, 0);
    assert_eq!(planner.proof().link_groups_exhausted, 0);
}

#[test]
fn first_optimum_requires_a_witness_and_does_not_claim_equal_link_exhaustion() {
    let (mut planner, task, witness) = direct(SolveMode::OneMinNL);
    planner
        .accept(PlannerEvent::Witness(task.id, witness), 1)
        .unwrap();
    planner
        .accept(PlannerEvent::Retired(task.id, Ok(Completion::Optimum)), 2)
        .unwrap();
    assert!(planner.poll(3).dispatch.is_empty());
    let outcome = planner.outcome().unwrap();
    assert!(matches!(outcome.result, SolveResult::Optimal(_)));
    assert_eq!(outcome.enumeration, EnumerationStatus::NotRequested);
    assert!(outcome.solutions.is_empty());
    assert_eq!(planner.proof().link_groups_exhausted, 0);
    let (mut planner, task, _) = direct(SolveMode::OneMinNL);
    assert!(
        planner
            .accept(PlannerEvent::Retired(task.id, Ok(Completion::Optimum)), 0)
            .is_err()
    );
    assert!(planner.poll(1).dispatch.is_empty());
    assert!(matches!(planner.result(), Some(SolveResult::Incomplete(_))));
}

fn group(mode: SolveMode, workers: usize) -> Group {
    let problem = problem(&["2"], &["1", "1"], "2");
    let Preparation::Prepared(normalized) = prepare_problem(&problem).unwrap() else {
        panic!()
    };
    let context = LeafContext::new(problem, normalized);
    Group::new(
        7,
        1,
        AccountedProfileGroup {
            link_count: 0,
            profiles: vec![crate::profile::AccountedProfile {
                profile: NodeProfile {
                    splitter2: 1,
                    ..NodeProfile::default()
                },
                accounting: ProfileLinkAccounting {
                    link_count: 0,
                    discard_link_count: 0,
                    physical_link_count: 3,
                },
            }],
        },
        &context,
        options(mode, workers),
        Counts::Boolean,
        &ProofSummary::default(),
    )
    .unwrap()
}

#[test]
fn static_children_replace_parents_and_only_the_full_cover_exhausts() {
    let mut group = group(SolveMode::AllMinNL, 8);
    let tasks = group.dispatch();
    assert_eq!(tasks.len(), 4);
    let roots: Vec<_> = tasks
        .iter()
        .map(|task| (task.root.source, task.root.second_source))
        .collect();
    assert_eq!(
        roots,
        vec![
            (Some(1), Some(0)),
            (Some(0), Some(0)),
            (Some(1), Some(1)),
            (Some(0), Some(1))
        ]
    );
    let mut proof = ProofSummary::default();
    for task in &tasks[..3] {
        group
            .retire(task.id, Ok(Completion::Exhausted), false, &mut proof)
            .unwrap();
    }
    assert!(!group.covered());
    assert_eq!(proof.profiles_exhausted, 0);
    assert!(
        group
            .retire(tasks[0].id, Ok(Completion::Exhausted), false, &mut proof)
            .is_err()
    );
    group
        .retire(tasks[3].id, Ok(Completion::Exhausted), false, &mut proof)
        .unwrap();
    assert!(group.covered());
    assert_eq!(proof.profiles_exhausted, 1);
    assert_eq!(proof.root_partitions_exhausted, 4);
}

#[test]
fn adaptive_parent_and_child_covers_have_one_owner_and_wait_for_retirement() {
    for parent_wins in [true, false] {
        let mut group = group(SolveMode::AllMinNL, 2);
        let parents = group.dispatch();
        assert_eq!(
            parents.iter().map(|t| t.root.source).collect::<Vec<_>>(),
            vec![Some(1), Some(0)]
        );
        assert!(parents.iter().all(|task| task.root.second_source.is_none()));
        let mut proof = ProofSummary::default();
        group
            .retire(parents[1].id, Ok(Completion::Exhausted), false, &mut proof)
            .unwrap();
        assert!(
            group.dispatch().is_empty(),
            "no adaptive children before a validated witness"
        );
        group.witness(parents[0].id, 17).unwrap();
        let first = group.dispatch();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].root.second_source, Some(0));
        assert_eq!(group.active, 2);
        if parent_wins {
            group
                .retire(parents[0].id, Ok(Completion::Exhausted), false, &mut proof)
                .unwrap();
            assert!(group.covered());
            assert_eq!(
                group.active, 1,
                "covered does not mean the competing backend is retired"
            );
            assert!(group.dispatch().is_empty());
            group
                .retire(first[0].id, Err(Failure::Cancelled), true, &mut proof)
                .unwrap();
            assert_eq!(proof.root_partitions_exhausted, 2);
        } else {
            group
                .retire(first[0].id, Ok(Completion::Exhausted), false, &mut proof)
                .unwrap();
            assert!(!group.covered());
            let second = group.dispatch();
            assert_eq!(second.len(), 1);
            assert_eq!(second[0].root.second_source, Some(1));
            group
                .retire(second[0].id, Ok(Completion::Exhausted), false, &mut proof)
                .unwrap();
            assert!(group.covered());
            assert_eq!(group.active, 1);
            group
                .retire(parents[0].id, Ok(Completion::Exhausted), true, &mut proof)
                .unwrap();
            assert_eq!(proof.root_partitions_exhausted, 3);
        }
        assert_eq!(group.active, 0);
        assert_eq!(proof.profiles_exhausted, 1);
        let evidence = group.evidence();
        assert_eq!(evidence.jobs[0].committed, parent_wins);
        assert_eq!(evidence.jobs[2].committed, !parent_wins);
        assert_eq!(evidence.jobs[2].trigger_ms, Some(17));
    }
}

#[test]
fn cancel_waits_for_all_backends_and_never_dispatches_queued_roots() {
    let mut planner = ExactPlanner::new(
        &problem(&["2"], &["1", "1"], "2"),
        options(SolveMode::AllMinNL, 8),
        Counts::Boolean,
    )
    .unwrap();
    let tasks = planner.poll(0).dispatch;
    assert_eq!(tasks.len(), 4);
    assert!(planner.cancel());
    assert!(planner.poll(1).dispatch.is_empty());
    for (index, task) in tasks.iter().enumerate() {
        planner
            .accept(PlannerEvent::Retired(task.id, Err(Failure::Cancelled)), 2)
            .unwrap();
        assert!(planner.poll(3).dispatch.is_empty());
        assert_eq!(planner.result().is_some(), index + 1 == tasks.len());
    }
    assert_eq!(planner.proof().root_partitions_exhausted, 0);
}

#[test]
fn all_min_n_advances_to_higher_links_at_the_same_minimum_node_count() {
    // A synthetic witness tests only the coordinator transition; real physical
    // witnesses and complete sets are independently checked in integration tests.
    let (mut planner, task, mut witness) = direct(SolveMode::AllMinN);
    let context = planner.context().unwrap();
    let groups = enumerate_accounted_profile_groups(
        3,
        1,
        1,
        &context.normalized.surplus,
        &context.normalized.max_link_rate,
    )
    .unwrap();
    assert!(groups.len() > 1);
    planner.group = None;
    planner.node = 3;
    planner.groups = groups.into();
    planner.loaded_node = true;
    let first = planner.poll(0).dispatch;
    let minimum_links = first[0].links;
    witness.node_count = 3;
    witness.link_count = minimum_links;
    planner
        .accept(PlannerEvent::Witness(first[0].id, witness), 1)
        .unwrap();
    let mut pending = first;
    loop {
        for task in pending {
            planner
                .accept(PlannerEvent::Retired(task.id, Ok(Completion::Exhausted)), 2)
                .unwrap();
        }
        pending = planner.poll(3).dispatch;
        assert!(planner.result().is_none());
        if pending[0].links > minimum_links {
            break;
        }
    }
    assert!(pending.iter().all(|task| task.nodes == 3));
    assert_eq!(planner.proof().link_groups_exhausted, 1);
    assert!(
        planner
            .accept(PlannerEvent::Retired(task.id, Ok(Completion::Exhausted)), 4)
            .is_err(),
        "old group packets must not discharge new obligations"
    );
}

#[test]
fn streaming_planners_keep_only_identity_keys_and_never_claim_to_return_a_collection() {
    let (_, _, witness) = direct(SolveMode::AllMinNL);
    let original = problem(&["1/3"], &["1/3"], "1");
    let mut planner =
        ExactPlanner::streaming(&original, options(SolveMode::AllMinNL, 1), Counts::Boolean)
            .unwrap();
    let task = planner.poll(0).dispatch[0];
    planner
        .accept(PlannerEvent::Witness(task.id, witness.clone()), 1)
        .unwrap();
    planner
        .accept(PlannerEvent::Witness(task.id, witness), 2)
        .unwrap();
    let events = planner.poll(3).events;
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, SolverEvent::SolutionFound(_)))
            .count(),
        1
    );
    assert!(planner.layouts.is_empty());
    assert_eq!(planner.layout_count(), 1);
    planner
        .accept(PlannerEvent::Retired(task.id, Ok(Completion::Exhausted)), 4)
        .unwrap();
    let _ = planner.poll(5);
    assert!(planner.outcome().is_none());
    let Some(SolveResult::Optimal(result)) = planner.public_result() else {
        panic!()
    };
    assert_eq!(
        result.canonical_graph_key,
        solver_validation::layout_key(&original, &result.graph)
    );
    assert_eq!(result.proof.root_partitions_exhausted, 1);
}
