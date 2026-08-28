//! Static refinement preserves the union of complete continuation sets.
use super::{
    AtomicBool, DfsResult, Duration, InitializedSearch, Instant, NodeProfile, NormalizedProblem,
    Ordering, ProfileLinkAccounting, ProfileSearchError, PropagationState, RootPartition,
    RootPartitionId, RootPartitionPlan, SearchContext, SearchFeatures, TopologyDecision,
    TopologyState, finish_profile_search, initialize_profile_search,
    plan_profile_root_partitions_with_accounting, prefix_state_key, prepare_applied_child,
    replay_prefix, topology_error,
};

struct Leaf {
    partition: RootPartition,
    state: TopologyState,
    propagation: Option<PropagationState>,
    decisions: Vec<TopologyDecision>,
}

pub(crate) fn plan_adaptive_partitions(
    problem: &NormalizedProblem,
    profile: NodeProfile,
    cancel: &AtomicBool,
    accounting: ProfileLinkAccounting,
    target: usize,
) -> RootPartitionPlan {
    let original =
        plan_profile_root_partitions_with_accounting(problem, profile, cancel, Some(accounting));
    let RootPartitionPlan::Partitions(partitions) = original else {
        return original;
    };
    if partitions.len() >= target || partitions.iter().any(|p| p.path.is_empty()) {
        return RootPartitionPlan::Partitions(partitions);
    }
    let started = Instant::now();
    let initialized = initialize_profile_search(
        problem,
        profile,
        cancel,
        SearchFeatures::PRODUCTION,
        Some(accounting),
    );
    let Ok(mut initialized) = initialized else {
        return RootPartitionPlan::Immediate(initialized.err().unwrap());
    };
    let result = refine(&partitions, &mut initialized, target, started);
    match result {
        Ok(partitions) => RootPartitionPlan::Partitions(partitions),
        Err(result) => RootPartitionPlan::Immediate(Box::new(finish_profile_search(
            initialized.context,
            started,
            result,
        ))),
    }
}

fn refine(
    partitions: &[RootPartition],
    initialized: &mut InitializedSearch<'_>,
    target: usize,
    started: Instant,
) -> Result<Vec<RootPartition>, DfsResult> {
    let mut leaves = Vec::new();
    for partition in partitions {
        let mut state = initialized.state.clone();
        let mut propagation = initialized.propagation.clone();
        let decisions = match replay_prefix(
            &mut state,
            &mut propagation,
            &mut initialized.context,
            partition,
        ) {
            Ok(()) => next_decisions(
                &mut state,
                &initialized.context,
                partition.path.len(),
                started,
            )?,
            Err(DfsResult::Exhausted(_)) => Vec::new(),
            Err(result) => return Err(result),
        };
        leaves.push(Leaf {
            partition: partition.clone(),
            state,
            propagation,
            decisions,
        });
    }
    while leaves.len() < target && started.elapsed() < Duration::from_millis(250) {
        let selected = leaves
            .iter()
            .enumerate()
            .filter(|(_, leaf)| !leaf.decisions.is_empty())
            .max_by_key(|(index, leaf)| (leaf.decisions.len(), std::cmp::Reverse(*index)))
            .map(|(index, _)| index);
        let Some(index) = selected else { break };
        let parent = &leaves[index];
        let mut children = Vec::new();
        for &decision in &parent.decisions {
            if initialized.context.cancel.load(Ordering::Relaxed) {
                return Err(DfsResult::Incomplete);
            }
            let mut state = parent.state.clone();
            let mut propagation = parent.propagation.clone();
            let id = state
                .apply_legal_decision(decision)
                .map_err(|e| DfsResult::Failed(topology_error(&e)))?;
            let link = state
                .link_index_for(id)
                .ok_or(DfsResult::Failed(ProfileSearchError::MissingAppliedLink))?;
            match prepare_applied_child(
                &mut state,
                &mut propagation,
                &mut initialized.context,
                link,
            ) {
                Ok(()) => {}
                // This child has a direct rejection proof. No completion is removed.
                Err(DfsResult::Exhausted(_)) => continue,
                Err(result) => return Err(result),
            }
            let mut path = parent.partition.path.clone();
            path.push(decision);
            let stable_key =
                prefix_state_key(&state, propagation.as_ref(), initialized.context.cancel)?;
            let decisions = next_decisions(&mut state, &initialized.context, path.len(), started)?;
            children.push(Leaf {
                partition: RootPartition {
                    id: RootPartitionId(0),
                    stable_key,
                    path,
                },
                state,
                propagation,
                decisions,
            });
        }
        if children.is_empty() {
            // Retain a searchable dead leaf instead of manufacturing an empty proof plan.
            leaves[index].decisions.clear();
        } else {
            leaves.remove(index);
            leaves.extend(children);
            leaves.sort_by(|a, b| a.partition.stable_key.cmp(&b.partition.stable_key));
            leaves.dedup_by(|a, b| a.partition.stable_key == b.partition.stable_key);
        }
    }
    leaves.sort_by(|a, b| a.partition.stable_key.cmp(&b.partition.stable_key));
    leaves
        .into_iter()
        .enumerate()
        .map(|(index, mut leaf)| {
            leaf.partition.id = RootPartitionId(u32::try_from(index).map_err(|_| {
                DfsResult::Failed(ProfileSearchError::InvalidRootPartition("too many leaves"))
            })?);
            Ok(leaf.partition)
        })
        .collect()
}

fn next_decisions(
    state: &mut TopologyState,
    context: &SearchContext<'_>,
    depth: usize,
    started: Instant,
) -> Result<Vec<TopologyDecision>, DfsResult> {
    if depth >= 4 || state.is_complete() || started.elapsed() >= Duration::from_millis(250) {
        return Ok(Vec::new());
    }
    state
        .legal_decisions_cancellable(context.cancel)
        .ok_or(DfsResult::Incomplete)
}
