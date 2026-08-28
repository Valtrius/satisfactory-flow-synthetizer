//! Optional cumulative hotspot timers for production-search profiling.
//!
//! Timers accumulate across workers via atomics. They are diagnostic only and
//! do not affect search decisions. Enable recording with [`install_recorder`];
//! read with [`peek_snapshot`] or finish with [`take_snapshot`].

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

static WITNESS_SEARCH_NS: AtomicU64 = AtomicU64::new(0);
static WITNESS_REFINE_NS: AtomicU64 = AtomicU64::new(0);
static WITNESS_LEAF_NS: AtomicU64 = AtomicU64::new(0);
static WITNESS_BRANCHES: AtomicU64 = AtomicU64::new(0);
static WITNESS_LEAVES: AtomicU64 = AtomicU64::new(0);
static ENABLED: AtomicBool = AtomicBool::new(false);

static SNAPSHOT_NS: AtomicU64 = AtomicU64::new(0);
static STATE_CANONICALIZE_NS: AtomicU64 = AtomicU64::new(0);
static LEGAL_DECISIONS_NS: AtomicU64 = AtomicU64::new(0);
static APPLY_DECISION_NS: AtomicU64 = AtomicU64::new(0);
static PROPAGATION_SYNC_NS: AtomicU64 = AtomicU64::new(0);
static SCC_CANONICALIZE_NS: AtomicU64 = AtomicU64::new(0);
static SCC_ALGEBRA_NS: AtomicU64 = AtomicU64::new(0);
static REACHABILITY_NS: AtomicU64 = AtomicU64::new(0);
static EVALUATE_COMPLETE_NS: AtomicU64 = AtomicU64::new(0);
static GRAPH_CANON_NS: AtomicU64 = AtomicU64::new(0);
static GRAPH_CANON_CALLS: AtomicU64 = AtomicU64::new(0);
static INCIDENCE_BUILD_NS: AtomicU64 = AtomicU64::new(0);
static DENSE_GRAPH_NS: AtomicU64 = AtomicU64::new(0);
static LABELING_NS: AtomicU64 = AtomicU64::new(0);
static RELABEL_NS: AtomicU64 = AtomicU64::new(0);
static EQUALITY_NS: AtomicU64 = AtomicU64::new(0);
static INEQUALITY_NS: AtomicU64 = AtomicU64::new(0);
static SEMANTIC_ENCODING_NS: AtomicU64 = AtomicU64::new(0);

/// Named nanosecond accumulators for one profiling session.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HotspotSnapshot {
    pub witness_leaves: u64,
    pub witness_branches: u64,
    pub witness_leaf_ns: u64,
    pub witness_refine_ns: u64,
    pub witness_search_ns: u64,
    pub snapshot_ns: u64,
    pub state_canonicalize_ns: u64,
    pub legal_decisions_ns: u64,
    pub apply_decision_ns: u64,
    pub propagation_sync_ns: u64,
    pub scc_canonicalize_ns: u64,
    pub scc_algebra_ns: u64,
    pub reachability_ns: u64,
    pub evaluate_complete_ns: u64,
    pub graph_canon_ns: u64,
    pub graph_canon_calls: u64,
    pub incidence_build_ns: u64,
    pub dense_graph_ns: u64,
    pub labeling_ns: u64,
    pub relabel_ns: u64,
    pub equality_ns: u64,
    pub inequality_ns: u64,
    pub semantic_encoding_ns: u64,
}

impl HotspotSnapshot {
    /// Total accounted nanoseconds across top-level search buckets.
    #[must_use]
    pub const fn accounted_ns(&self) -> u64 {
        self.snapshot_ns
            .saturating_add(self.state_canonicalize_ns)
            .saturating_add(self.legal_decisions_ns)
            .saturating_add(self.apply_decision_ns)
            .saturating_add(self.propagation_sync_ns)
            .saturating_add(self.scc_canonicalize_ns)
            .saturating_add(self.scc_algebra_ns)
            .saturating_add(self.reachability_ns)
            .saturating_add(self.evaluate_complete_ns)
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
        witness_leaves: WITNESS_LEAVES.load(Ordering::Relaxed),
        witness_branches: WITNESS_BRANCHES.load(Ordering::Relaxed),
        witness_leaf_ns: WITNESS_LEAF_NS.load(Ordering::Relaxed),
        witness_refine_ns: WITNESS_REFINE_NS.load(Ordering::Relaxed),
        witness_search_ns: WITNESS_SEARCH_NS.load(Ordering::Relaxed),
        snapshot_ns: SNAPSHOT_NS.load(Ordering::Relaxed),
        state_canonicalize_ns: STATE_CANONICALIZE_NS.load(Ordering::Relaxed),
        legal_decisions_ns: LEGAL_DECISIONS_NS.load(Ordering::Relaxed),
        apply_decision_ns: APPLY_DECISION_NS.load(Ordering::Relaxed),
        propagation_sync_ns: PROPAGATION_SYNC_NS.load(Ordering::Relaxed),
        scc_canonicalize_ns: SCC_CANONICALIZE_NS.load(Ordering::Relaxed),
        scc_algebra_ns: SCC_ALGEBRA_NS.load(Ordering::Relaxed),
        reachability_ns: REACHABILITY_NS.load(Ordering::Relaxed),
        evaluate_complete_ns: EVALUATE_COMPLETE_NS.load(Ordering::Relaxed),
        graph_canon_ns: GRAPH_CANON_NS.load(Ordering::Relaxed),
        graph_canon_calls: GRAPH_CANON_CALLS.load(Ordering::Relaxed),
        incidence_build_ns: INCIDENCE_BUILD_NS.load(Ordering::Relaxed),
        dense_graph_ns: DENSE_GRAPH_NS.load(Ordering::Relaxed),
        labeling_ns: LABELING_NS.load(Ordering::Relaxed),
        relabel_ns: RELABEL_NS.load(Ordering::Relaxed),
        equality_ns: EQUALITY_NS.load(Ordering::Relaxed),
        inequality_ns: INEQUALITY_NS.load(Ordering::Relaxed),
        semantic_encoding_ns: SEMANTIC_ENCODING_NS.load(Ordering::Relaxed),
    }
}

fn clear_buckets() {
    for bucket in [
        &WITNESS_LEAVES,
        &WITNESS_BRANCHES,
        &WITNESS_LEAF_NS,
        &WITNESS_REFINE_NS,
        &WITNESS_SEARCH_NS,
        &SNAPSHOT_NS,
        &STATE_CANONICALIZE_NS,
        &LEGAL_DECISIONS_NS,
        &APPLY_DECISION_NS,
        &PROPAGATION_SYNC_NS,
        &SCC_CANONICALIZE_NS,
        &SCC_ALGEBRA_NS,
        &REACHABILITY_NS,
        &EVALUATE_COMPLETE_NS,
        &GRAPH_CANON_NS,
        &GRAPH_CANON_CALLS,
        &INCIDENCE_BUILD_NS,
        &DENSE_GRAPH_NS,
        &LABELING_NS,
        &RELABEL_NS,
        &EQUALITY_NS,
        &INEQUALITY_NS,
        &SEMANTIC_ENCODING_NS,
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

pub(crate) fn record_graph_canon(elapsed: Duration) {
    add_bucket(&GRAPH_CANON_NS, elapsed);
    add_count(&GRAPH_CANON_CALLS, 1);
}

/// Sub-buckets overlap the existing top-level search buckets; never sum both.
pub(crate) enum CanonicalPhase {
    WitnessLeaf,
    WitnessSearch,
    IncidenceBuild,
    DenseGraph,
    Labeling,
    Relabel,
    Equality,
    Inequality,
    SemanticEncoding,
}

pub(crate) struct CanonicalTimer {
    phase: CanonicalPhase,
    started: Option<std::time::Instant>,
}

impl CanonicalTimer {
    pub(crate) fn start(phase: CanonicalPhase) -> Self {
        Self {
            phase,
            started: ENABLED
                .load(Ordering::Relaxed)
                .then(std::time::Instant::now),
        }
    }
}

impl Drop for CanonicalTimer {
    fn drop(&mut self) {
        let Some(started) = self.started else { return };
        let bucket = match self.phase {
            CanonicalPhase::WitnessLeaf => &WITNESS_LEAF_NS,
            CanonicalPhase::WitnessSearch => &WITNESS_SEARCH_NS,
            CanonicalPhase::IncidenceBuild => &INCIDENCE_BUILD_NS,
            CanonicalPhase::DenseGraph => &DENSE_GRAPH_NS,
            CanonicalPhase::Labeling => &LABELING_NS,
            CanonicalPhase::Relabel => &RELABEL_NS,
            CanonicalPhase::Equality => &EQUALITY_NS,
            CanonicalPhase::Inequality => &INEQUALITY_NS,
            CanonicalPhase::SemanticEncoding => &SEMANTIC_ENCODING_NS,
        };
        add_bucket(bucket, started.elapsed());
    }
}

pub(crate) fn record_witness_branch() {
    add_count(&WITNESS_BRANCHES, 1);
}
pub(crate) fn record_witness_leaf() {
    add_count(&WITNESS_LEAVES, 1);
}
