//! Deterministic benchmark selection only; never used by production scheduling.
use super::{
    AtomicBool, DfsResult, NodeProfile, NormalizedProblem, ProfileLinkAccounting,
    ProfileSearchError, RootPartition, SearchFeatures, initialize_profile_search, prefix_state_key,
    prepare_applied_child, terminal_root_partition, topology_error,
};

pub(crate) struct PrefixPlan {
    pub partition: RootPartition,
    /// Every accepted child key, in selection order, at each visited frontier.
    pub frontiers: Vec<Vec<Vec<u8>>>,
    pub route: Vec<usize>,
    pub decisions: Vec<String>,
}

pub(crate) fn plan(
    problem: &NormalizedProblem,
    profile: NodeProfile,
    accounting: ProfileLinkAccounting,
    depth: usize,
    pick: usize,
) -> Result<PrefixPlan, ProfileSearchError> {
    let cancel = AtomicBool::new(false);
    let mut initialized = initialize_profile_search(
        problem,
        profile,
        &cancel,
        SearchFeatures::PRODUCTION,
        Some(accounting),
    )
    .map_err(|_| {
        ProfileSearchError::InvalidRootPartition("prefix profile rejected at initialization")
    })?;
    let mut partition = terminal_root_partition();
    let mut frontiers = Vec::new();
    let mut route = Vec::new();
    for _ in 0..depth {
        if initialized.state.is_complete() {
            break;
        }
        let choices = initialized.state.legal_decisions();
        let mut children = Vec::new();
        for decision in choices {
            let mut state = initialized.state.clone();
            let mut propagation = initialized.propagation.clone();
            let id = state
                .apply_legal_decision(decision)
                .map_err(|e| topology_error(&e))?;
            let link = state
                .link_index_for(id)
                .ok_or(ProfileSearchError::MissingAppliedLink)?;
            match prepare_applied_child(
                &mut state,
                &mut propagation,
                &mut initialized.context,
                link,
            ) {
                Ok(()) => {}
                Err(DfsResult::Exhausted(_)) => continue,
                Err(DfsResult::Failed(error)) => return Err(error),
                Err(DfsResult::Incomplete) => {
                    return Err(ProfileSearchError::InvalidRootPartition(
                        "unexpected planning cancellation",
                    ));
                }
            }
            let key = prefix_state_key(&state, propagation.as_ref(), &cancel).map_err(|_| {
                ProfileSearchError::InvalidRootPartition("prefix key construction failed")
            })?;
            children.push((key, decision, state, propagation));
        }
        children.sort_by(|a, b| a.0.cmp(&b.0));
        children.dedup_by(|a, b| a.0 == b.0);
        frontiers.push(children.iter().map(|child| child.0.clone()).collect());
        if children.is_empty() {
            break;
        }
        let selected = pick % children.len();
        route.push(selected);
        let (key, decision, state, propagation) = children.swap_remove(selected);
        partition.path.push(decision);
        partition.stable_key = key;
        initialized.state = state;
        initialized.propagation = propagation;
    }
    let decisions = partition.path.iter().map(|d| format!("{d:?}")).collect();
    Ok(PrefixPlan {
        partition,
        frontiers,
        route,
        decisions,
    })
}
