//! Optional cumulative hotspot timers for production-search profiling.
//!
//! Timers accumulate across workers via atomics. They are diagnostic only and
//! do not affect search decisions. Enable recording with [`install_recorder`];
//! read with [`peek_snapshot`] or finish with [`take_snapshot`].

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

static ENABLED: AtomicBool = AtomicBool::new(false);

// Search-frame buckets (mutually exclusive with each other; canonical_last is
// the outer wall for the constructibility predicate and nests the cl_* fields).
static SNAPSHOT_NS: AtomicU64 = AtomicU64::new(0);
static STATE_CANONICALIZE_NS: AtomicU64 = AtomicU64::new(0);
static NO_GOOD_MATCH_NS: AtomicU64 = AtomicU64::new(0);
static LEGAL_DECISIONS_NS: AtomicU64 = AtomicU64::new(0);
static APPLY_DECISION_NS: AtomicU64 = AtomicU64::new(0);
static PROPAGATION_SYNC_NS: AtomicU64 = AtomicU64::new(0);
static SCC_CANONICALIZE_NS: AtomicU64 = AtomicU64::new(0);
static SCC_ALGEBRA_NS: AtomicU64 = AtomicU64::new(0);
static REACHABILITY_NS: AtomicU64 = AtomicU64::new(0);
static CANONICAL_LAST_NS: AtomicU64 = AtomicU64::new(0);
static EVALUATE_COMPLETE_NS: AtomicU64 = AtomicU64::new(0);
static COMPONENT_APPLY_NS: AtomicU64 = AtomicU64::new(0);
static LEARN_NO_GOOD_NS: AtomicU64 = AtomicU64::new(0);

// Nested breakdown of canonical_last (sums toward CANONICAL_LAST_NS).
static CL_ADMISSIBLE_NS: AtomicU64 = AtomicU64::new(0);
static CL_MIN_SELECT_NS: AtomicU64 = AtomicU64::new(0);
static CL_ADMISSIBLE_CALLS: AtomicU64 = AtomicU64::new(0);
static CL_REVERSE_CHECKS: AtomicU64 = AtomicU64::new(0);
static CL_REVERSE_TARGET_NS: AtomicU64 = AtomicU64::new(0);
static CL_REVERSE_PARENT_CANON_NS: AtomicU64 = AtomicU64::new(0);
static CL_CONSTRUCTIBLE_LOOKUP_NS: AtomicU64 = AtomicU64::new(0);
static CL_RECREATE_NS: AtomicU64 = AtomicU64::new(0);
static CL_RECREATE_CALLS: AtomicU64 = AtomicU64::new(0);
static CL_RECREATE_LEGAL_NS: AtomicU64 = AtomicU64::new(0);
static CL_CONSTRUCTIBLE_HITS: AtomicU64 = AtomicU64::new(0);
static CL_CONSTRUCTIBLE_MISSES: AtomicU64 = AtomicU64::new(0);
static CL_CONSTRUCTIBLE_MISS_NS: AtomicU64 = AtomicU64::new(0);

// Leaf work shared by many callers (nests under several outer buckets).
static GRAPH_CANON_NS: AtomicU64 = AtomicU64::new(0);
static GRAPH_CANON_CALLS: AtomicU64 = AtomicU64::new(0);

/// Named nanosecond accumulators for one profiling session.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HotspotSnapshot {
    pub snapshot_ns: u64,
    pub state_canonicalize_ns: u64,
    pub no_good_match_ns: u64,
    pub legal_decisions_ns: u64,
    pub apply_decision_ns: u64,
    pub propagation_sync_ns: u64,
    pub scc_canonicalize_ns: u64,
    pub scc_algebra_ns: u64,
    pub reachability_ns: u64,
    pub canonical_last_ns: u64,
    pub evaluate_complete_ns: u64,
    pub component_apply_ns: u64,
    pub learn_no_good_ns: u64,
    pub cl_admissible_ns: u64,
    pub cl_min_select_ns: u64,
    pub cl_admissible_calls: u64,
    pub cl_reverse_checks: u64,
    pub cl_reverse_target_ns: u64,
    pub cl_reverse_parent_canon_ns: u64,
    pub cl_constructible_lookup_ns: u64,
    pub cl_recreate_ns: u64,
    pub cl_recreate_calls: u64,
    pub cl_recreate_legal_ns: u64,
    pub cl_constructible_hits: u64,
    pub cl_constructible_misses: u64,
    pub cl_constructible_miss_ns: u64,
    pub graph_canon_ns: u64,
    pub graph_canon_calls: u64,
}

impl HotspotSnapshot {
    /// Total accounted nanoseconds across top-level search buckets.
    #[must_use]
    pub const fn accounted_ns(&self) -> u64 {
        self.snapshot_ns
            .saturating_add(self.state_canonicalize_ns)
            .saturating_add(self.no_good_match_ns)
            .saturating_add(self.legal_decisions_ns)
            .saturating_add(self.apply_decision_ns)
            .saturating_add(self.propagation_sync_ns)
            .saturating_add(self.scc_canonicalize_ns)
            .saturating_add(self.scc_algebra_ns)
            .saturating_add(self.reachability_ns)
            .saturating_add(self.canonical_last_ns)
            .saturating_add(self.evaluate_complete_ns)
            .saturating_add(self.component_apply_ns)
            .saturating_add(self.learn_no_good_ns)
    }
}

/// Starts a fresh profiling session and clears prior counters.
pub fn install_recorder() {
    clear_buckets();
    ENABLED.store(true, Ordering::Relaxed);
}

/// Stops recording and returns the cumulative counters.
pub fn take_snapshot() -> HotspotSnapshot {
    ENABLED.store(false, Ordering::Relaxed);
    let snapshot = load_snapshot();
    clear_buckets();
    snapshot
}

/// Returns the live counters without stopping the session.
#[must_use]
pub fn peek_snapshot() -> HotspotSnapshot {
    load_snapshot()
}

fn load_snapshot() -> HotspotSnapshot {
    HotspotSnapshot {
        snapshot_ns: SNAPSHOT_NS.load(Ordering::Relaxed),
        state_canonicalize_ns: STATE_CANONICALIZE_NS.load(Ordering::Relaxed),
        no_good_match_ns: NO_GOOD_MATCH_NS.load(Ordering::Relaxed),
        legal_decisions_ns: LEGAL_DECISIONS_NS.load(Ordering::Relaxed),
        apply_decision_ns: APPLY_DECISION_NS.load(Ordering::Relaxed),
        propagation_sync_ns: PROPAGATION_SYNC_NS.load(Ordering::Relaxed),
        scc_canonicalize_ns: SCC_CANONICALIZE_NS.load(Ordering::Relaxed),
        scc_algebra_ns: SCC_ALGEBRA_NS.load(Ordering::Relaxed),
        reachability_ns: REACHABILITY_NS.load(Ordering::Relaxed),
        canonical_last_ns: CANONICAL_LAST_NS.load(Ordering::Relaxed),
        evaluate_complete_ns: EVALUATE_COMPLETE_NS.load(Ordering::Relaxed),
        component_apply_ns: COMPONENT_APPLY_NS.load(Ordering::Relaxed),
        learn_no_good_ns: LEARN_NO_GOOD_NS.load(Ordering::Relaxed),
        cl_admissible_ns: CL_ADMISSIBLE_NS.load(Ordering::Relaxed),
        cl_min_select_ns: CL_MIN_SELECT_NS.load(Ordering::Relaxed),
        cl_admissible_calls: CL_ADMISSIBLE_CALLS.load(Ordering::Relaxed),
        cl_reverse_checks: CL_REVERSE_CHECKS.load(Ordering::Relaxed),
        cl_reverse_target_ns: CL_REVERSE_TARGET_NS.load(Ordering::Relaxed),
        cl_reverse_parent_canon_ns: CL_REVERSE_PARENT_CANON_NS.load(Ordering::Relaxed),
        cl_constructible_lookup_ns: CL_CONSTRUCTIBLE_LOOKUP_NS.load(Ordering::Relaxed),
        cl_recreate_ns: CL_RECREATE_NS.load(Ordering::Relaxed),
        cl_recreate_calls: CL_RECREATE_CALLS.load(Ordering::Relaxed),
        cl_recreate_legal_ns: CL_RECREATE_LEGAL_NS.load(Ordering::Relaxed),
        cl_constructible_hits: CL_CONSTRUCTIBLE_HITS.load(Ordering::Relaxed),
        cl_constructible_misses: CL_CONSTRUCTIBLE_MISSES.load(Ordering::Relaxed),
        cl_constructible_miss_ns: CL_CONSTRUCTIBLE_MISS_NS.load(Ordering::Relaxed),
        graph_canon_ns: GRAPH_CANON_NS.load(Ordering::Relaxed),
        graph_canon_calls: GRAPH_CANON_CALLS.load(Ordering::Relaxed),
    }
}

fn clear_buckets() {
    for bucket in [
        &SNAPSHOT_NS,
        &STATE_CANONICALIZE_NS,
        &NO_GOOD_MATCH_NS,
        &LEGAL_DECISIONS_NS,
        &APPLY_DECISION_NS,
        &PROPAGATION_SYNC_NS,
        &SCC_CANONICALIZE_NS,
        &SCC_ALGEBRA_NS,
        &REACHABILITY_NS,
        &CANONICAL_LAST_NS,
        &EVALUATE_COMPLETE_NS,
        &COMPONENT_APPLY_NS,
        &LEARN_NO_GOOD_NS,
        &CL_ADMISSIBLE_NS,
        &CL_MIN_SELECT_NS,
        &CL_ADMISSIBLE_CALLS,
        &CL_REVERSE_CHECKS,
        &CL_REVERSE_TARGET_NS,
        &CL_REVERSE_PARENT_CANON_NS,
        &CL_CONSTRUCTIBLE_LOOKUP_NS,
        &CL_RECREATE_NS,
        &CL_RECREATE_CALLS,
        &CL_RECREATE_LEGAL_NS,
        &CL_CONSTRUCTIBLE_HITS,
        &CL_CONSTRUCTIBLE_MISSES,
        &CL_CONSTRUCTIBLE_MISS_NS,
        &GRAPH_CANON_NS,
        &GRAPH_CANON_CALLS,
    ] {
        bucket.store(0, Ordering::Relaxed);
    }
}

fn add_bucket(target: &AtomicU64, elapsed: Duration) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let ns = u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX);
    target.fetch_add(ns, Ordering::Relaxed);
}

fn add_count(target: &AtomicU64, count: u64) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    target.fetch_add(count, Ordering::Relaxed);
}

pub(crate) fn record_snapshot(elapsed: Duration) {
    add_bucket(&SNAPSHOT_NS, elapsed);
}

pub(crate) fn record_state_canonicalize(elapsed: Duration) {
    add_bucket(&STATE_CANONICALIZE_NS, elapsed);
}

pub(crate) fn record_no_good_match(elapsed: Duration) {
    add_bucket(&NO_GOOD_MATCH_NS, elapsed);
}

pub(crate) fn record_legal_decisions(elapsed: Duration) {
    add_bucket(&LEGAL_DECISIONS_NS, elapsed);
}

pub(crate) fn record_apply_decision(elapsed: Duration) {
    add_bucket(&APPLY_DECISION_NS, elapsed);
}

pub(crate) fn record_propagation_sync(elapsed: Duration) {
    add_bucket(&PROPAGATION_SYNC_NS, elapsed);
}

pub(crate) fn record_scc_canonicalize(elapsed: Duration) {
    add_bucket(&SCC_CANONICALIZE_NS, elapsed);
}

pub(crate) fn record_scc_algebra(elapsed: Duration) {
    add_bucket(&SCC_ALGEBRA_NS, elapsed);
}

pub(crate) fn record_reachability(elapsed: Duration) {
    add_bucket(&REACHABILITY_NS, elapsed);
}

pub(crate) fn record_evaluate_complete(elapsed: Duration) {
    add_bucket(&EVALUATE_COMPLETE_NS, elapsed);
}

pub(crate) fn record_component_apply(elapsed: Duration) {
    add_bucket(&COMPONENT_APPLY_NS, elapsed);
}

pub(crate) fn record_learn_no_good(elapsed: Duration) {
    add_bucket(&LEARN_NO_GOOD_NS, elapsed);
}

pub(crate) fn record_cl_admissible(elapsed: Duration) {
    add_bucket(&CL_ADMISSIBLE_NS, elapsed);
    add_count(&CL_ADMISSIBLE_CALLS, 1);
}

pub(crate) fn record_cl_min_select(elapsed: Duration) {
    add_bucket(&CL_MIN_SELECT_NS, elapsed);
}

pub(crate) fn record_cl_reverse_check() {
    add_count(&CL_REVERSE_CHECKS, 1);
}

pub(crate) fn record_cl_reverse_target(elapsed: Duration) {
    add_bucket(&CL_REVERSE_TARGET_NS, elapsed);
}

pub(crate) fn record_cl_reverse_parent_canon(elapsed: Duration) {
    add_bucket(&CL_REVERSE_PARENT_CANON_NS, elapsed);
}

pub(crate) fn record_cl_constructible_lookup(elapsed: Duration) {
    add_bucket(&CL_CONSTRUCTIBLE_LOOKUP_NS, elapsed);
}

pub(crate) fn record_cl_recreate(elapsed: Duration) {
    add_bucket(&CL_RECREATE_NS, elapsed);
    add_count(&CL_RECREATE_CALLS, 1);
}

pub(crate) fn record_cl_recreate_legal(elapsed: Duration) {
    add_bucket(&CL_RECREATE_LEGAL_NS, elapsed);
}

pub(crate) fn record_cl_constructible_hit() {
    add_count(&CL_CONSTRUCTIBLE_HITS, 1);
}

pub(crate) fn record_cl_constructible_miss(elapsed: Duration) {
    add_bucket(&CL_CONSTRUCTIBLE_MISS_NS, elapsed);
    add_count(&CL_CONSTRUCTIBLE_MISSES, 1);
}

pub(crate) fn record_graph_canon(elapsed: Duration) {
    add_bucket(&GRAPH_CANON_NS, elapsed);
    add_count(&GRAPH_CANON_CALLS, 1);
}
