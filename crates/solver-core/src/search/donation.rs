//! Bounded sibling donation with cooperative per-state joins.
//!
//! Cache entries are completed proofs, so a task never waits on an in-flight
//! cache owner. A joining worker executes ready tasks instead of blocking the
//! pool behind its own queued children. The caller publishes its state proof
//! only after every donated child has returned.
use super::{
    AtomicBool, BTreeMap, DfsResult, HashMap, NodeProfile, Ordering, Problem, ProfileSearchError,
    ProfileSearchStats, ProfileWitness, PropagationState, SearchContext, SearchFeatures,
    SharedStateCache, TopologyDecision, TopologyState, increment, retain_smallest_key,
    search_decision,
};
use std::{
    collections::VecDeque,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};

#[derive(Clone)]
struct Seed {
    problem: Problem,
    profile: NodeProfile,
    expected_node_count: u32,
    expected_link_count: Option<u32>,
    expected_physical_link_count: u32,
    expected_discard_link_count: u32,
    collect_all_witnesses: bool,
    features: SearchFeatures,
}

impl Seed {
    fn from_context(context: &SearchContext<'_>) -> Self {
        Self {
            problem: context.problem.clone(),
            profile: context.profile,
            expected_node_count: context.expected_node_count,
            expected_link_count: context.expected_link_count,
            expected_physical_link_count: context.expected_physical_link_count,
            expected_discard_link_count: context.expected_discard_link_count,
            collect_all_witnesses: context.collect_all_witnesses,
            features: context.features,
        }
    }

    fn context(self, cancel: &AtomicBool) -> SearchContext<'_> {
        SearchContext {
            problem: self.problem,
            profile: self.profile,
            expected_node_count: self.expected_node_count,
            expected_link_count: self.expected_link_count,
            expected_physical_link_count: self.expected_physical_link_count,
            expected_discard_link_count: self.expected_discard_link_count,
            cancel,
            cache: HashMap::new(),
            scc_cache: HashMap::new(),
            shared: None,
            donations: None,
            owned_cache_bytes: 0,
            best_witness: None,
            witnesses: BTreeMap::new(),
            collect_all_witnesses: self.collect_all_witnesses,
            features: self.features,
            stats: ProfileSearchStats::default(),
            progress: None,
            last_progress_at: None,
        }
    }
}

pub(super) struct Outcome {
    result: DfsResult,
    stats: ProfileSearchStats,
    best: Option<ProfileWitness>,
}

#[derive(Default)]
pub(super) struct Completion {
    outcome: Mutex<Option<Outcome>>,
    ready: Condvar,
}

struct Job {
    state: TopologyState,
    propagation: Option<PropagationState>,
    decision: TopologyDecision,
    seed: Seed,
    completion: Arc<Completion>,
}

pub(crate) struct DonationPool {
    queue: Mutex<VecDeque<Job>>,
    capacity: usize,
    #[cfg(test)]
    pub(super) panic_next_job: AtomicBool,
}

impl DonationPool {
    pub(crate) fn new(workers: usize) -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            capacity: workers,
            #[cfg(test)]
            panic_next_job: AtomicBool::new(false),
        }
    }

    pub(super) fn donate(
        &self,
        state: &TopologyState,
        propagation: Option<&PropagationState>,
        context: &mut SearchContext<'_>,
        decisions: &mut Vec<TopologyDecision>,
    ) -> Vec<Arc<Completion>> {
        let mut queue = self
            .queue
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut joins = Vec::new();
        while decisions.len() > 1 && queue.len() < self.capacity {
            let decision = decisions.pop().unwrap();
            let completion = Arc::new(Completion::default());
            // Register the join before making the child visible to another worker.
            joins.push(Arc::clone(&completion));
            queue.push_back(Job {
                state: state.clone(),
                propagation: propagation.cloned(),
                decision,
                seed: Seed::from_context(context),
                completion,
            });
            increment(&mut context.stats.instrumentation.donated_tasks);
        }
        joins
    }

    pub(crate) fn help(&self, cancel: &AtomicBool, shared: &SharedStateCache) -> bool {
        let job = self
            .queue
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .pop_back();
        let Some(mut job) = job else { return false };
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            #[cfg(test)]
            assert!(
                !self.panic_next_job.swap(false, Ordering::Relaxed),
                "injected donated task panic"
            );
            let mut context = job.seed.context(cancel);
            context.shared = Some(shared);
            context.donations = Some(self);
            let result = if cancel.load(Ordering::Relaxed) {
                DfsResult::Incomplete
            } else {
                increment(&mut context.stats.instrumentation.raw_structural_decisions);
                search_decision(
                    &mut job.state,
                    &mut job.propagation,
                    &mut context,
                    job.decision,
                )
            };
            Outcome {
                result,
                stats: context.stats,
                best: context.best_witness,
            }
        }))
        .unwrap_or_else(|_| Outcome {
            result: DfsResult::Failed(ProfileSearchError::InvalidRootPartition(
                "donated task panicked",
            )),
            stats: ProfileSearchStats::default(),
            best: None,
        });
        *job.completion
            .outcome
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(outcome);
        job.completion.ready.notify_all();
        true
    }

    pub(super) fn join(
        &self,
        joins: Vec<Arc<Completion>>,
        context: &mut SearchContext<'_>,
        result: &mut DfsResult,
    ) {
        for completion in joins {
            loop {
                let outcome = completion
                    .outcome
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take();
                if let Some(outcome) = outcome {
                    crate::solver::merge_instrumentation(
                        &mut context.stats.instrumentation,
                        &outcome.stats.instrumentation,
                    );
                    context.stats.state_cache_hits = context
                        .stats
                        .state_cache_hits
                        .saturating_add(outcome.stats.state_cache_hits);
                    context.stats.complete_topologies = context
                        .stats
                        .complete_topologies
                        .saturating_add(outcome.stats.complete_topologies);
                    context.stats.rejected_complete_topologies = context
                        .stats
                        .rejected_complete_topologies
                        .saturating_add(outcome.stats.rejected_complete_topologies);
                    context.stats.validated_cyclic_topologies = context
                        .stats
                        .validated_cyclic_topologies
                        .saturating_add(outcome.stats.validated_cyclic_topologies);
                    if let Some(witness) = outcome.best {
                        context.retain_witness(witness);
                    }
                    fold_result(result, outcome.result);
                    break;
                }
                if !self.help(
                    context.cancel,
                    context.shared.expect("donation requires shared witnesses"),
                ) {
                    let guard = completion
                        .outcome
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    if guard.is_none() {
                        let _ = completion
                            .ready
                            .wait_timeout(guard, Duration::from_millis(1));
                    }
                }
            }
        }
    }
}

pub(super) fn fold_result(total: &mut DfsResult, child: DfsResult) {
    match (&mut *total, child) {
        (DfsResult::Failed(_), _) => {}
        (_, DfsResult::Failed(error)) => *total = DfsResult::Failed(error),
        (DfsResult::Incomplete, _) => {}
        (_, DfsResult::Incomplete) => *total = DfsResult::Incomplete,
        (DfsResult::Exhausted(best), DfsResult::Exhausted(candidate)) => {
            retain_smallest_key(best, candidate);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, thread};

    fn context(cancel: &AtomicBool) -> SearchContext<'_> {
        Seed {
            problem: Problem {
                inputs: vec![1.into()],
                outputs: vec![1.into()],
                max_link_rate: 1.into(),
            },
            profile: NodeProfile::default(),
            expected_node_count: 0,
            expected_link_count: Some(0),
            expected_physical_link_count: 1,
            expected_discard_link_count: 0,
            collect_all_witnesses: true,
            features: SearchFeatures::PRODUCTION,
        }
        .context(cancel)
    }

    #[test]
    fn a_join_cannot_finish_before_its_delayed_child() {
        let cancel = AtomicBool::new(false);
        let shared = SharedStateCache::default();
        let pool = DonationPool::new(1);
        let child = Arc::new(Completion::default());
        let (started, start) = mpsc::channel();
        let (finished, finish) = mpsc::channel();
        thread::scope(|scope| {
            let child_ref = Arc::clone(&child);
            let pool_ref = &pool;
            let shared_ref = &shared;
            let cancel_ref = &cancel;
            scope.spawn(move || {
                let mut context = context(cancel_ref);
                context.shared = Some(shared_ref);
                let mut result = DfsResult::Exhausted(None);
                started.send(()).unwrap();
                pool_ref.join(vec![child_ref], &mut context, &mut result);
                finished
                    .send(matches!(result, DfsResult::Incomplete))
                    .unwrap();
            });
            start.recv_timeout(Duration::from_secs(2)).unwrap();
            assert!(matches!(finish.try_recv(), Err(mpsc::TryRecvError::Empty)));
            *child.outcome.lock().unwrap() = Some(Outcome {
                result: DfsResult::Incomplete,
                stats: ProfileSearchStats::default(),
                best: None,
            });
            child.ready.notify_all();
            assert!(finish.recv_timeout(Duration::from_secs(2)).unwrap());
        });
    }

    #[test]
    fn a_single_worker_drains_its_cancelled_donations() {
        use super::super::{canonicalize_state, initialize_profile_search};
        let problem = Problem {
            inputs: vec![2.into(), 3.into()],
            outputs: vec![1.into(), 4.into()],
            max_link_rate: 5.into(),
        };
        let crate::Preparation::Prepared(normalized) = crate::prepare_problem(&problem).unwrap()
        else {
            panic!("fixture rejected")
        };
        let cancel = AtomicBool::new(false);
        let shared = SharedStateCache::default();
        let pool = DonationPool::new(1);
        let profile = NodeProfile {
            splitter2: 1,
            merger2: 1,
            ..NodeProfile::default()
        };
        let mut initialized = initialize_profile_search(
            &normalized,
            profile,
            &cancel,
            SearchFeatures::PRODUCTION,
            None,
        )
        .unwrap_or_else(|_| panic!("fixture rejected"));
        initialized.context.shared = Some(&shared);
        let mut decisions = initialized.state.legal_decisions();
        let joins = pool.donate(
            &initialized.state,
            initialized.propagation.as_ref(),
            &mut initialized.context,
            &mut decisions,
        );
        assert!(!joins.is_empty());
        let key = canonicalize_state(&initialized.state.partial_topology());
        cancel.store(true, Ordering::Relaxed);
        let mut result = DfsResult::Exhausted(None);
        pool.join(joins, &mut initialized.context, &mut result);
        assert!(matches!(result, DfsResult::Incomplete));
        assert!(shared.lookup(None, &key).is_none());
        assert!(pool.queue.lock().unwrap().is_empty());
    }
}
