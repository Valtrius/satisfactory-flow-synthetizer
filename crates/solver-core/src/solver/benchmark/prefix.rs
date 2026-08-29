//! Exact subtree replay with an explicit, benchmark-only proof scope.
use super::{BenchmarkError, FixedProfileResult, FixedWorkload, prepare};
use crate::{
    NormalizedProblem,
    profile::AccountedProfile,
    search::{
        ProfileSearchResult, RootPartition, SearchExecution, prefix_benchmark,
        search_profile_root_partition_with_execution,
    },
    solver::{SolverError, restore_and_validate},
    telemetry::SearchInstrumentation,
};
use solver_api::Problem;
use std::{collections::BTreeMap, sync::atomic::AtomicBool};

/// Exact selection certificate. A route ordinal alone is not a workload identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrefixIdentity {
    pub frontiers: Vec<Vec<Vec<u8>>>,
    pub route: Vec<usize>,
    pub decisions: Vec<String>,
    pub stable_key: Vec<u8>,
}

/// Prepared subtree; it cannot discharge any containing profile or N/L group.
pub struct PreparedPrefix {
    caller: Problem,
    normalized: NormalizedProblem,
    accounted: AccountedProfile,
    partition: RootPartition,
    collect_all: bool,
    identity: PrefixIdentity,
}

/// Selects one reproducible path through sorted canonical child frontiers.
/// This is bounded-work discovery, not a production scheduling policy.
///
/// # Errors
/// Rejects invalid selections, nonserial prefix budgets or depths outside 1..=12.
pub fn prepare_prefix(
    problem: &Problem,
    workload: &FixedWorkload,
    depth: usize,
    pick: usize,
) -> Result<PreparedPrefix, BenchmarkError> {
    if !(1..=12).contains(&depth)
        || workload.worker_count != 1
        || workload.profile.is_none()
        || workload.parallelism != crate::ParallelismOptions::default()
    {
        return Err(BenchmarkError::Invalid(
            "prefix needs depth 1..=12, baseline scheduling, one worker and one profile",
        ));
    }
    let (normalized, mut profiles) = prepare(problem, workload)?;
    let accounted = profiles.remove(0);
    let plan = prefix_benchmark::plan(
        &normalized,
        accounted.profile,
        accounted.accounting,
        depth,
        pick,
    )
    .map_err(SolverError::ProfileSearch)?;
    let identity = PrefixIdentity {
        stable_key: plan.partition.stable_key().to_vec(),
        frontiers: plan.frontiers,
        route: plan.route,
        decisions: plan.decisions,
    };
    Ok(PreparedPrefix {
        caller: problem.clone(),
        normalized,
        accounted,
        partition: plan.partition,
        collect_all: workload.collect_all_witnesses,
        identity,
    })
}

impl PreparedPrefix {
    #[must_use]
    pub const fn identity(&self) -> &PrefixIdentity {
        &self.identity
    }

    /// Runs exactly the prepared subtree with fresh production search state.
    /// Identity validation is required before accepting any local exhaustion.
    ///
    /// # Errors
    /// Rejects changed identity and propagates search/independent validation failures.
    pub fn run(
        &self,
        expected: &PrefixIdentity,
        cancel: &AtomicBool,
        progress: &(dyn Fn(&SearchInstrumentation) + Sync),
    ) -> Result<FixedProfileResult, BenchmarkError> {
        if expected != &self.identity {
            return Err(BenchmarkError::Invalid(
                "prefix frontier/path identity changed",
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
    fn prefix_identity_cancellation_and_witnesses_are_scope_local() {
        let (problem, mut workload) = super::super::tests::fixture();
        workload.worker_count = 1;
        workload.parallelism = crate::ParallelismOptions::default();
        let full =
            super::super::run(&problem, &workload, &AtomicBool::new(false), &|_| {}).unwrap();
        let reference = full.iter().find(|p| !p.witnesses.is_empty()).unwrap();
        workload.profile = Some(reference.profile);
        let prefix = prepare_prefix(&problem, &workload, 2, 0).unwrap();
        let repeat = prepare_prefix(&problem, &workload, 2, 0).unwrap();
        assert_eq!(prefix.identity(), repeat.identity());
        let result = prefix
            .run(prefix.identity(), &AtomicBool::new(false), &|_| {})
            .unwrap();
        assert!(result.exhausted && result.roots_exhausted == 1);
        assert!(
            result
                .witnesses
                .iter()
                .all(|w| reference.witnesses.contains(w))
        );
        // Every first-level child together covers the full selected tiny profile.
        let first = prepare_prefix(&problem, &workload, 1, 0).unwrap();
        let mut union = BTreeMap::new();
        for pick in 0..first.identity().frontiers[0].len() {
            let child = prepare_prefix(&problem, &workload, 1, pick).unwrap();
            let result = child
                .run(child.identity(), &AtomicBool::new(false), &|_| {})
                .unwrap();
            assert!(result.exhausted);
            for witness in result.witnesses {
                union.insert(witness.canonical_graph_key.clone(), witness);
            }
        }
        assert_eq!(union.into_values().collect::<Vec<_>>(), reference.witnesses);
        let cancelled = prefix
            .run(prefix.identity(), &AtomicBool::new(true), &|_| {})
            .unwrap();
        assert!(!cancelled.exhausted && cancelled.roots_exhausted == 0);
        let mut changed = prefix.identity().clone();
        changed.stable_key.push(0);
        assert!(
            prefix
                .run(&changed, &AtomicBool::new(false), &|_| {})
                .is_err()
        );
        let mut changed = prefix.identity().clone();
        changed.decisions.push("different decision".to_string());
        assert!(
            prefix
                .run(&changed, &AtomicBool::new(false), &|_| {})
                .is_err()
        );
        let mut changed = prefix.identity().clone();
        changed.frontiers.clear();
        assert!(
            prefix
                .run(&changed, &AtomicBool::new(false), &|_| {})
                .is_err()
        );
        assert!(prepare_prefix(&problem, &workload, 0, 0).is_err());
        workload.parallelism.shared_state_cache = true;
        assert!(prepare_prefix(&problem, &workload, 2, 0).is_err());
        workload.parallelism = crate::ParallelismOptions::default();
        workload.worker_count = 2;
        assert!(prepare_prefix(&problem, &workload, 2, 0).is_err());
    }
}
