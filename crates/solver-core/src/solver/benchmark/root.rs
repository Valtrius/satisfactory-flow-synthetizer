//! Exact replay of one frozen adaptive root partition.
use super::{BenchmarkError, FixedProfileResult, FixedWorkload, prepare};
use crate::{
    NormalizedProblem,
    profile::AccountedProfile,
    search::{
        ProfileSearchResult, RootPartition, RootPartitionPlan, SearchExecution,
        plan_adaptive_partitions, search_profile_root_partition_with_execution,
    },
    solver::{SolverError, restore_and_validate},
    telemetry::SearchInstrumentation,
};
use solver_api::Problem;
use std::{collections::BTreeMap, sync::atomic::AtomicBool};

/// Frozen identity of one partition in the production adaptive plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RootIdentity {
    pub target: usize,
    pub ordinal: u32,
    pub plan_keys: Vec<Vec<u8>>,
    pub stable_key: Vec<u8>,
}

/// Prepared serial replay. Its exhaustion applies only to the selected root.
pub struct PreparedRoot {
    caller: Problem,
    normalized: NormalizedProblem,
    accounted: AccountedProfile,
    partition: RootPartition,
    collect_all: bool,
    identity: RootIdentity,
}

/// Rebuilds the production adaptive plan and selects one stable ordinal.
///
/// # Errors
/// Rejects non-p1 policies, ambiguous profiles and ordinals outside the plan.
pub fn prepare_root(
    problem: &Problem,
    workload: &FixedWorkload,
    ordinal: u32,
) -> Result<PreparedRoot, BenchmarkError> {
    if workload.profile.is_none()
        || !workload.parallelism.deep_partitions
        || workload.parallelism.shared_state_cache
        || workload.parallelism.work_stealing
        || workload.parallelism.parallel_remaining_groups
    {
        return Err(BenchmarkError::Invalid(
            "root replay needs one profile and the p1 policy",
        ));
    }
    let (normalized, mut profiles) = prepare(problem, workload)?;
    if profiles.len() != 1 {
        return Err(BenchmarkError::Invalid(
            "root replay needs exactly one admitted profile",
        ));
    }
    let accounted = profiles.remove(0);
    let target = workload.worker_count.saturating_mul(4);
    let RootPartitionPlan::Partitions(partitions) = plan_adaptive_partitions(
        &normalized,
        accounted.profile,
        &AtomicBool::new(false),
        accounted.accounting,
        target,
    ) else {
        return Err(BenchmarkError::Invalid(
            "root replay plan completed before producing partitions",
        ));
    };
    let plan_keys = partitions
        .iter()
        .map(|partition| partition.stable_key().to_vec())
        .collect::<Vec<_>>();
    let Some(partition) = partitions
        .into_iter()
        .find(|partition| partition.id().ordinal() == ordinal)
    else {
        return Err(BenchmarkError::Invalid(
            "root ordinal is outside the adaptive plan",
        ));
    };
    let identity = RootIdentity {
        target,
        ordinal,
        stable_key: partition.stable_key().to_vec(),
        plan_keys,
    };
    Ok(PreparedRoot {
        caller: problem.clone(),
        normalized,
        accounted,
        partition,
        collect_all: workload.collect_all_witnesses,
        identity,
    })
}

impl PreparedRoot {
    #[must_use]
    pub const fn identity(&self) -> &RootIdentity {
        &self.identity
    }

    /// Runs the selected root with fresh production search state.
    ///
    /// # Errors
    /// Rejects changed identity and propagates search or validation failures.
    pub fn run(
        &self,
        expected: &RootIdentity,
        cancel: &AtomicBool,
        progress: &(dyn Fn(&SearchInstrumentation) + Sync),
    ) -> Result<FixedProfileResult, BenchmarkError> {
        if expected != &self.identity {
            return Err(BenchmarkError::Invalid(
                "adaptive root plan identity changed",
            ));
        }
        let result = search_profile_root_partition_with_execution(
            &self.normalized,
            self.accounted.profile,
            cancel,
            Some(self.accounted.accounting),
            &self.partition,
            self.collect_all,
            Some(progress),
            SearchExecution::default(),
        );
        let (exhausted, reason, best, witnesses, stats) = match result {
            ProfileSearchResult::Exhausted {
                best_witness,
                witnesses,
                stats,
            } => (true, None, best_witness, witnesses, stats),
            ProfileSearchResult::Incomplete {
                reason,
                best_witness,
                witnesses,
                stats,
            } => (false, Some(reason), best_witness, witnesses, stats),
            ProfileSearchResult::Failed { error, .. } => {
                return Err(SolverError::ProfileSearch(error).into());
            }
        };
        let mut unique = BTreeMap::new();
        for witness in if self.collect_all {
            witnesses
        } else {
            best.into_iter().collect()
        } {
            let restored = restore_and_validate(
                &self.caller,
                &self.normalized,
                witness,
                self.accounted.profile.node_count(),
                self.accounted.accounting,
            )?;
            unique.insert(restored.canonical_graph_key.clone(), restored);
        }
        Ok(FixedProfileResult {
            profile: self.accounted.profile,
            exhausted,
            incomplete_reason: reason,
            roots: 1,
            roots_exhausted: usize::from(exhausted),
            witnesses: unique.into_values().collect(),
            instrumentation: stats.instrumentation,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frozen_adaptive_roots_cover_the_selected_profile() {
        let (problem, mut workload) = super::super::tests::fixture();
        workload.profile = Some(super::super::profiles(&problem, &workload).unwrap()[0]);
        let cancel = AtomicBool::new(false);
        let full = super::super::run(&problem, &workload, &cancel, &|_| {}).unwrap();
        let first = prepare_root(&problem, &workload, 0).unwrap();
        let plan_len = first.identity().plan_keys.len();
        let mut union = BTreeMap::new();
        for ordinal in 0..u32::try_from(plan_len).unwrap() {
            let root = prepare_root(&problem, &workload, ordinal).unwrap();
            let repeated = prepare_root(&problem, &workload, ordinal).unwrap();
            assert_eq!(root.identity(), repeated.identity());
            let result = root
                .run(root.identity(), &AtomicBool::new(false), &|_| {})
                .unwrap();
            assert!(result.exhausted && result.roots == 1 && result.roots_exhausted == 1);
            for witness in result.witnesses {
                union.insert(witness.canonical_graph_key.clone(), witness);
            }
        }
        let expected = full[0]
            .witnesses
            .iter()
            .cloned()
            .map(|witness| (witness.canonical_graph_key.clone(), witness))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(union, expected);

        let mut changed = first.identity().clone();
        changed.stable_key.push(0);
        assert!(
            first
                .run(&changed, &AtomicBool::new(false), &|_| {})
                .is_err()
        );
        assert!(prepare_root(&problem, &workload, u32::MAX).is_err());
    }
}
