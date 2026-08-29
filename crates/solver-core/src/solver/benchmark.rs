//! Benchmark-only access to exact production proof obligations.
//!
//! This bypasses earlier N/L groups and the optional constructor. Exhaustion
//! proves only the selected work, never global optimality or minimum-N enumeration.

mod prefix;
pub use prefix::{PrefixIdentity, PreparedPrefix, prepare_prefix};
mod root;
pub use root::{PreparedRoot, RootIdentity, prepare_root};

use super::{
    AtomicBool, BTreeMap, ParallelismOptions, Preparation, Problem, ProfileGroupRun,
    ProfileSearchResult, ProfileTaskResult, ProofLedger, SearchInstrumentation, SolverError,
    enumerate_accounted_profile_groups, prepare_problem, restore_and_validate,
};
use solver_api::{BestKnownSolution, IncompleteReason, NodeProfile};

#[derive(Clone, Copy, Debug)]
pub struct FixedWorkload {
    pub node_count: u32,
    pub link_count: u32,
    /// None selects every feasible profile at this exact N/L.
    pub profile: Option<NodeProfile>,
    pub worker_count: usize,
    /// Within-group policies only; remaining-group concurrency has no meaning here.
    pub parallelism: ParallelismOptions,
    /// False retains the best witness per profile, but still exhausts each profile.
    pub collect_all_witnesses: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum BenchmarkError {
    #[error("invalid fixed workload: {0}")]
    Invalid(&'static str),
    #[error(transparent)]
    Solver(#[from] SolverError),
}

#[derive(Debug)]
pub struct FixedProfileResult {
    pub profile: NodeProfile,
    pub exhausted: bool,
    pub incomplete_reason: Option<IncompleteReason>,
    pub roots: usize,
    pub roots_exhausted: usize,
    pub witnesses: Vec<BestKnownSolution>,
    pub instrumentation: SearchInstrumentation,
}

/// Lists the exact-accounted profiles admitted by the normalized workload.
///
/// # Errors
/// Rejects invalid input, empty selections, mismatched profiles or zero workers.
pub fn profiles(
    problem: &Problem,
    workload: &FixedWorkload,
) -> Result<Vec<NodeProfile>, BenchmarkError> {
    let (_, profiles) = prepare(problem, workload)?;
    Ok(profiles.into_iter().map(|p| p.profile).collect())
}

fn prepare(
    problem: &Problem,
    workload: &FixedWorkload,
) -> Result<
    (
        crate::NormalizedProblem,
        Vec<crate::profile::AccountedProfile>,
    ),
    BenchmarkError,
> {
    if workload.worker_count == 0 {
        return Err(BenchmarkError::Invalid("worker count must be positive"));
    }
    if workload.parallelism.parallel_remaining_groups
        || (workload.parallelism.work_stealing && !workload.parallelism.shared_state_cache)
    {
        return Err(BenchmarkError::Invalid(
            "fixed work needs within-group policies and sharing for donation",
        ));
    }
    let Preparation::Prepared(normalized) = prepare_problem(problem).map_err(SolverError::from)?
    else {
        return Err(BenchmarkError::Invalid("problem has no prepared search"));
    };
    let inputs = u32::try_from(normalized.inputs.len())
        .map_err(|_| BenchmarkError::Invalid("too many inputs"))?;
    let outputs = u32::try_from(normalized.outputs.len())
        .map_err(|_| BenchmarkError::Invalid("too many outputs"))?;
    let selected = enumerate_accounted_profile_groups(
        workload.node_count,
        inputs,
        outputs,
        &normalized.surplus,
        &normalized.max_link_rate,
    )
    .map_err(SolverError::from)?
    .into_iter()
    .find(|group| group.link_count == workload.link_count)
    .into_iter()
    .flat_map(|group| group.profiles)
    .filter(|p| workload.profile.is_none_or(|profile| profile == p.profile))
    .collect::<Vec<_>>();
    if selected.is_empty() {
        return Err(BenchmarkError::Invalid(
            "no profiles at the selected exact N/L",
        ));
    }
    Ok((normalized, selected))
}

/// Runs the selected obligation through production partition planning, search,
/// ledger folding and caller-unit witness validation. All workers join before return.
///
/// # Errors
/// Rejects invalid selections and propagates search, validation or worker failures.
pub fn run(
    problem: &Problem,
    workload: &FixedWorkload,
    cancel: &AtomicBool,
    progress: &(dyn Fn(&SearchInstrumentation) + Sync),
) -> Result<Vec<FixedProfileResult>, BenchmarkError> {
    let (normalized, selected) = prepare(problem, workload)?;
    let mut ledger = ProofLedger::default();
    ledger
        .begin_node(workload.node_count)
        .map_err(SolverError::from)?;
    let tasks = ProfileGroupRun {
        problem: &normalized,
        node_count: workload.node_count,
        link_count: workload.link_count,
        requested_workers: workload.worker_count,
        parallelism: workload.parallelism,
        cancel,
        collect_all_witnesses: workload.collect_all_witnesses,
        #[cfg(test)]
        panic_next_root_worker: None,
    }
    .run(selected, &mut ledger, progress)?;
    tasks
        .into_iter()
        .map(|task| {
            let ProfileTaskResult::Search(result) = task.result else {
                return Err(SolverError::ProfileWorkerPanicked {
                    profile_index: task.index,
                }
                .into());
            };
            let (exhausted, incomplete_reason, best_witness, stats) = match *result {
                ProfileSearchResult::Exhausted {
                    best_witness,
                    stats,
                    ..
                } => (true, None, best_witness, stats),
                ProfileSearchResult::Incomplete {
                    reason,
                    best_witness,
                    stats,
                    ..
                } => (false, Some(reason), best_witness, stats),
                ProfileSearchResult::Failed { error, .. } => {
                    return Err(SolverError::ProfileSearch(error).into());
                }
            };
            let witnesses = if workload.collect_all_witnesses {
                task.witnesses
            } else {
                best_witness.into_iter().collect()
            };
            let mut unique = BTreeMap::new();
            for witness in witnesses {
                let restored = restore_and_validate(
                    problem,
                    &normalized,
                    witness,
                    workload.node_count,
                    task.accounted.accounting,
                )?;
                unique.insert(restored.canonical_graph_key.clone(), restored);
            }
            Ok(FixedProfileResult {
                profile: task.accounted.profile,
                exhausted,
                incomplete_reason,
                roots: task.roots.len(),
                roots_exhausted: task.roots.iter().filter(|r| r.exhausted).count(),
                witnesses: unique.into_values().collect(),
                instrumentation: stats.instrumentation,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use solver_api::{CanonicalGraphKey, PhysicalGraph};

    pub(super) fn fixture() -> (Problem, FixedWorkload) {
        (
            Problem {
                inputs: vec![2.into(), 3.into()],
                outputs: vec![1.into(), 4.into()],
                max_link_rate: 1200.into(),
            },
            FixedWorkload {
                node_count: 2,
                link_count: 1,
                profile: None,
                worker_count: 4,
                parallelism: ParallelismOptions {
                    deep_partitions: true,
                    ..ParallelismOptions::default()
                },
                collect_all_witnesses: true,
            },
        )
    }

    #[test]
    fn exact_group_matches_the_corresponding_full_enumeration() {
        let (problem, mut workload) = fixture();
        let cancel = AtomicBool::new(false);
        let expected = std::sync::Mutex::new(BTreeMap::<CanonicalGraphKey, PhysicalGraph>::new());
        let outcome = super::super::enumerate_with_observer(
            &problem,
            &super::super::SolveOptions {
                max_nodes: Some(2),
                worker_count: 4,
                ..super::super::SolveOptions::default()
            },
            &cancel,
            &|event| {
                if let solver_api::SolverEvent::SolutionFound(s) = event
                    && s.link_count == 1
                {
                    expected
                        .lock()
                        .unwrap()
                        .insert(s.canonical_graph_key, s.graph);
                }
            },
        )
        .unwrap();
        assert!(matches!(outcome, solver_api::SolveResult::Optimal(_)));
        let expected = expected.into_inner().unwrap();
        assert!(!expected.is_empty());
        for (shared_state_cache, work_stealing) in [(false, false), (true, false), (true, true)] {
            workload.parallelism.shared_state_cache = shared_state_cache;
            workload.parallelism.work_stealing = work_stealing;
            for workers in [1, 4] {
                workload.worker_count = workers;
                let results = run(&problem, &workload, &cancel, &|_| {}).unwrap();
                assert!(
                    results
                        .iter()
                        .all(|r| r.exhausted && r.roots == r.roots_exhausted)
                );
                let actual = results
                    .into_iter()
                    .flat_map(|r| r.witnesses)
                    .map(|s| (s.canonical_graph_key, s.graph))
                    .collect::<BTreeMap<_, _>>();
                assert_eq!(actual, expected);
            }
        }
    }

    #[test]
    fn selected_profile_and_best_mode_preserve_exact_witnesses() {
        let (problem, mut workload) = fixture();
        let cancel = AtomicBool::new(false);
        let all = run(&problem, &workload, &cancel, &|_| {}).unwrap();
        for profile in all {
            workload.profile = Some(profile.profile);
            workload.collect_all_witnesses = false;
            let best = run(&problem, &workload, &cancel, &|_| {}).unwrap();
            assert_eq!(best.len(), 1);
            assert!(best[0].exhausted);
            assert_eq!(best[0].witnesses.first(), profile.witnesses.first());
        }
    }

    #[test]
    fn cancellation_and_invalid_selection_never_prove_an_empty_group() {
        let (problem, mut workload) = fixture();
        for (sharing, donation) in [(false, false), (true, false), (true, true)] {
            workload.parallelism.shared_state_cache = sharing;
            workload.parallelism.work_stealing = donation;
            let cancelled = run(&problem, &workload, &AtomicBool::new(true), &|_| {}).unwrap();
            assert!(
                cancelled.iter().all(|r| !r.exhausted
                    && r.incomplete_reason.is_some()
                    && r.witnesses.is_empty())
            );
        }
        workload.parallelism.shared_state_cache = false;
        assert!(profiles(&problem, &workload).is_err());
        workload.parallelism = ParallelismOptions {
            parallel_remaining_groups: true,
            ..ParallelismOptions::default()
        };
        assert!(profiles(&problem, &workload).is_err());
        workload.parallelism = ParallelismOptions::default();
        workload.link_count = 99;
        assert!(profiles(&problem, &workload).is_err());
        workload.link_count = 1;
        workload.profile = Some(NodeProfile::default());
        assert!(profiles(&problem, &workload).is_err());
        workload.profile = None;
        workload.worker_count = 0;
        assert!(profiles(&problem, &workload).is_err());
    }
}
