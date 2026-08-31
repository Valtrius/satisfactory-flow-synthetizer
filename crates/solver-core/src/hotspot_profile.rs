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
static PROPAGATION_SYNC_CALLS: AtomicU64 = AtomicU64::new(0);
static PROPAGATION_PORT_SCAN_NS: AtomicU64 = AtomicU64::new(0);
static PROPAGATION_REGISTER_NS: AtomicU64 = AtomicU64::new(0);
static PROPAGATION_FIXED_POINT_NS: AtomicU64 = AtomicU64::new(0);
static PROPAGATION_FIXED_POINT_PASSES: AtomicU64 = AtomicU64::new(0);
static PROPAGATION_SPARSE_ANALYZE_NS: AtomicU64 = AtomicU64::new(0);
static PROPAGATION_SPARSE_PROFILED_CALLS: AtomicU64 = AtomicU64::new(0);
static PROPAGATION_SPARSE_PREPARATION_NS: AtomicU64 = AtomicU64::new(0);
static PROPAGATION_SPARSE_FORWARD_NS: AtomicU64 = AtomicU64::new(0);
static PROPAGATION_SPARSE_BACK_REDUCTION_NS: AtomicU64 = AtomicU64::new(0);
static PROPAGATION_SPARSE_DEDUCTIONS_NS: AtomicU64 = AtomicU64::new(0);
static PROPAGATION_SPARSE_INPUT_ROWS: AtomicU64 = AtomicU64::new(0);
static PROPAGATION_SPARSE_ACTIVE_ROWS: AtomicU64 = AtomicU64::new(0);
static PROPAGATION_SPARSE_VARIABLES: AtomicU64 = AtomicU64::new(0);
static PROPAGATION_SPARSE_NONZERO_TERMS: AtomicU64 = AtomicU64::new(0);
static PROPAGATION_SPARSE_CAUSE_CALLS: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];
static PROPAGATION_SPARSE_ROW_BUCKET_CALLS: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];
static PROPAGATION_SPARSE_ROW_BUCKET_NS: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];
static PROPAGATION_SPARSE_VARIABLE_BUCKET_CALLS: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];
static PROPAGATION_SPARSE_VARIABLE_BUCKET_NS: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];
static PROPAGATION_SPARSE_TERM_BUCKET_CALLS: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];
static PROPAGATION_SPARSE_TERM_BUCKET_NS: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];
static PROPAGATION_BOUNDS_NS: AtomicU64 = AtomicU64::new(0);
static SCC_CANONICALIZE_NS: AtomicU64 = AtomicU64::new(0);
static SCC_ALGEBRA_NS: AtomicU64 = AtomicU64::new(0);
static REACHABILITY_NS: AtomicU64 = AtomicU64::new(0);
static EVALUATE_COMPLETE_NS: AtomicU64 = AtomicU64::new(0);
static GRAPH_CANON_NS: AtomicU64 = AtomicU64::new(0);
static GRAPH_CANON_CALLS: AtomicU64 = AtomicU64::new(0);
static STATE_GRAPH_CANON_NS: AtomicU64 = AtomicU64::new(0);
static STATE_GRAPH_CANON_CALLS: AtomicU64 = AtomicU64::new(0);
static OPEN_PORT_GRAPH_CANON_NS: AtomicU64 = AtomicU64::new(0);
static OPEN_PORT_GRAPH_CANON_CALLS: AtomicU64 = AtomicU64::new(0);
static MARKED_LINK_GRAPH_CANON_NS: AtomicU64 = AtomicU64::new(0);
static MARKED_LINK_GRAPH_CANON_CALLS: AtomicU64 = AtomicU64::new(0);
static OTHER_GRAPH_CANON_NS: AtomicU64 = AtomicU64::new(0);
static OTHER_GRAPH_CANON_CALLS: AtomicU64 = AtomicU64::new(0);
static SCC_CANONICALIZE_CALLS: AtomicU64 = AtomicU64::new(0);
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
    pub propagation_sync_calls: u64,
    pub propagation_port_scan_ns: u64,
    pub propagation_register_ns: u64,
    pub propagation_fixed_point_ns: u64,
    pub propagation_fixed_point_passes: u64,
    pub propagation_sparse_analyze_ns: u64,
    pub propagation_sparse_profiled_calls: u64,
    pub propagation_sparse_preparation_ns: u64,
    pub propagation_sparse_forward_ns: u64,
    pub propagation_sparse_back_reduction_ns: u64,
    pub propagation_sparse_deductions_ns: u64,
    pub propagation_sparse_input_rows: u64,
    pub propagation_sparse_active_rows: u64,
    pub propagation_sparse_variables: u64,
    pub propagation_sparse_nonzero_terms: u64,
    pub propagation_sparse_cause_calls: [u64; 4],
    pub propagation_sparse_row_bucket_calls: [u64; 4],
    pub propagation_sparse_row_bucket_ns: [u64; 4],
    pub propagation_sparse_variable_bucket_calls: [u64; 4],
    pub propagation_sparse_variable_bucket_ns: [u64; 4],
    pub propagation_sparse_term_bucket_calls: [u64; 4],
    pub propagation_sparse_term_bucket_ns: [u64; 4],
    pub propagation_bounds_ns: u64,
    pub scc_canonicalize_ns: u64,
    pub scc_algebra_ns: u64,
    pub reachability_ns: u64,
    pub evaluate_complete_ns: u64,
    pub graph_canon_ns: u64,
    pub graph_canon_calls: u64,
    pub state_graph_canon_ns: u64,
    pub state_graph_canon_calls: u64,
    pub open_port_graph_canon_ns: u64,
    pub open_port_graph_canon_calls: u64,
    pub marked_link_graph_canon_ns: u64,
    pub marked_link_graph_canon_calls: u64,
    pub other_graph_canon_ns: u64,
    pub other_graph_canon_calls: u64,
    pub scc_canonicalize_calls: u64,
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
        propagation_sync_calls: PROPAGATION_SYNC_CALLS.load(Ordering::Relaxed),
        propagation_port_scan_ns: PROPAGATION_PORT_SCAN_NS.load(Ordering::Relaxed),
        propagation_register_ns: PROPAGATION_REGISTER_NS.load(Ordering::Relaxed),
        propagation_fixed_point_ns: PROPAGATION_FIXED_POINT_NS.load(Ordering::Relaxed),
        propagation_fixed_point_passes: PROPAGATION_FIXED_POINT_PASSES.load(Ordering::Relaxed),
        propagation_sparse_analyze_ns: PROPAGATION_SPARSE_ANALYZE_NS.load(Ordering::Relaxed),
        propagation_sparse_profiled_calls: PROPAGATION_SPARSE_PROFILED_CALLS
            .load(Ordering::Relaxed),
        propagation_sparse_preparation_ns: PROPAGATION_SPARSE_PREPARATION_NS
            .load(Ordering::Relaxed),
        propagation_sparse_forward_ns: PROPAGATION_SPARSE_FORWARD_NS.load(Ordering::Relaxed),
        propagation_sparse_back_reduction_ns: PROPAGATION_SPARSE_BACK_REDUCTION_NS
            .load(Ordering::Relaxed),
        propagation_sparse_deductions_ns: PROPAGATION_SPARSE_DEDUCTIONS_NS.load(Ordering::Relaxed),
        propagation_sparse_input_rows: PROPAGATION_SPARSE_INPUT_ROWS.load(Ordering::Relaxed),
        propagation_sparse_active_rows: PROPAGATION_SPARSE_ACTIVE_ROWS.load(Ordering::Relaxed),
        propagation_sparse_variables: PROPAGATION_SPARSE_VARIABLES.load(Ordering::Relaxed),
        propagation_sparse_nonzero_terms: PROPAGATION_SPARSE_NONZERO_TERMS.load(Ordering::Relaxed),
        propagation_sparse_cause_calls: load_array(&PROPAGATION_SPARSE_CAUSE_CALLS),
        propagation_sparse_row_bucket_calls: load_array(&PROPAGATION_SPARSE_ROW_BUCKET_CALLS),
        propagation_sparse_row_bucket_ns: load_array(&PROPAGATION_SPARSE_ROW_BUCKET_NS),
        propagation_sparse_variable_bucket_calls: load_array(
            &PROPAGATION_SPARSE_VARIABLE_BUCKET_CALLS,
        ),
        propagation_sparse_variable_bucket_ns: load_array(&PROPAGATION_SPARSE_VARIABLE_BUCKET_NS),
        propagation_sparse_term_bucket_calls: load_array(&PROPAGATION_SPARSE_TERM_BUCKET_CALLS),
        propagation_sparse_term_bucket_ns: load_array(&PROPAGATION_SPARSE_TERM_BUCKET_NS),
        propagation_bounds_ns: PROPAGATION_BOUNDS_NS.load(Ordering::Relaxed),
        scc_canonicalize_ns: SCC_CANONICALIZE_NS.load(Ordering::Relaxed),
        scc_algebra_ns: SCC_ALGEBRA_NS.load(Ordering::Relaxed),
        reachability_ns: REACHABILITY_NS.load(Ordering::Relaxed),
        evaluate_complete_ns: EVALUATE_COMPLETE_NS.load(Ordering::Relaxed),
        graph_canon_ns: GRAPH_CANON_NS.load(Ordering::Relaxed),
        graph_canon_calls: GRAPH_CANON_CALLS.load(Ordering::Relaxed),
        state_graph_canon_ns: STATE_GRAPH_CANON_NS.load(Ordering::Relaxed),
        state_graph_canon_calls: STATE_GRAPH_CANON_CALLS.load(Ordering::Relaxed),
        open_port_graph_canon_ns: OPEN_PORT_GRAPH_CANON_NS.load(Ordering::Relaxed),
        open_port_graph_canon_calls: OPEN_PORT_GRAPH_CANON_CALLS.load(Ordering::Relaxed),
        marked_link_graph_canon_ns: MARKED_LINK_GRAPH_CANON_NS.load(Ordering::Relaxed),
        marked_link_graph_canon_calls: MARKED_LINK_GRAPH_CANON_CALLS.load(Ordering::Relaxed),
        other_graph_canon_ns: OTHER_GRAPH_CANON_NS.load(Ordering::Relaxed),
        other_graph_canon_calls: OTHER_GRAPH_CANON_CALLS.load(Ordering::Relaxed),
        scc_canonicalize_calls: SCC_CANONICALIZE_CALLS.load(Ordering::Relaxed),
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
        &PROPAGATION_SYNC_CALLS,
        &PROPAGATION_PORT_SCAN_NS,
        &PROPAGATION_REGISTER_NS,
        &PROPAGATION_FIXED_POINT_NS,
        &PROPAGATION_FIXED_POINT_PASSES,
        &PROPAGATION_SPARSE_ANALYZE_NS,
        &PROPAGATION_SPARSE_PROFILED_CALLS,
        &PROPAGATION_SPARSE_PREPARATION_NS,
        &PROPAGATION_SPARSE_FORWARD_NS,
        &PROPAGATION_SPARSE_BACK_REDUCTION_NS,
        &PROPAGATION_SPARSE_DEDUCTIONS_NS,
        &PROPAGATION_SPARSE_INPUT_ROWS,
        &PROPAGATION_SPARSE_ACTIVE_ROWS,
        &PROPAGATION_SPARSE_VARIABLES,
        &PROPAGATION_SPARSE_NONZERO_TERMS,
        &PROPAGATION_BOUNDS_NS,
        &SCC_CANONICALIZE_NS,
        &SCC_ALGEBRA_NS,
        &REACHABILITY_NS,
        &EVALUATE_COMPLETE_NS,
        &GRAPH_CANON_NS,
        &GRAPH_CANON_CALLS,
        &STATE_GRAPH_CANON_NS,
        &STATE_GRAPH_CANON_CALLS,
        &OPEN_PORT_GRAPH_CANON_NS,
        &OPEN_PORT_GRAPH_CANON_CALLS,
        &MARKED_LINK_GRAPH_CANON_NS,
        &MARKED_LINK_GRAPH_CANON_CALLS,
        &OTHER_GRAPH_CANON_NS,
        &OTHER_GRAPH_CANON_CALLS,
        &SCC_CANONICALIZE_CALLS,
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
    for buckets in [
        &PROPAGATION_SPARSE_CAUSE_CALLS,
        &PROPAGATION_SPARSE_ROW_BUCKET_CALLS,
        &PROPAGATION_SPARSE_ROW_BUCKET_NS,
        &PROPAGATION_SPARSE_VARIABLE_BUCKET_CALLS,
        &PROPAGATION_SPARSE_VARIABLE_BUCKET_NS,
        &PROPAGATION_SPARSE_TERM_BUCKET_CALLS,
        &PROPAGATION_SPARSE_TERM_BUCKET_NS,
    ] {
        for bucket in buckets {
            bucket.store(0, Ordering::Relaxed);
        }
    }
}

fn load_array(buckets: &[AtomicU64; 4]) -> [u64; 4] {
    std::array::from_fn(|index| buckets[index].load(Ordering::Relaxed))
}

fn add_bucket(target: &AtomicU64, elapsed: Duration) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    target.fetch_add(duration_ns(elapsed), Ordering::Relaxed);
}

fn duration_ns(elapsed: Duration) -> u64 {
    u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX)
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

#[must_use]
pub(crate) fn recorder_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

pub(crate) fn record_propagation_sync_call() {
    add_count(&PROPAGATION_SYNC_CALLS, 1);
}

#[derive(Clone, Copy)]
pub(crate) enum PropagationPhase {
    PortScan,
    Register,
    FixedPoint,
    SparseAnalyze,
    Bounds,
}

pub(crate) fn record_propagation_phase(elapsed: Duration, phase: PropagationPhase) {
    let bucket = match phase {
        PropagationPhase::PortScan => &PROPAGATION_PORT_SCAN_NS,
        PropagationPhase::Register => &PROPAGATION_REGISTER_NS,
        PropagationPhase::FixedPoint => &PROPAGATION_FIXED_POINT_NS,
        PropagationPhase::SparseAnalyze => &PROPAGATION_SPARSE_ANALYZE_NS,
        PropagationPhase::Bounds => &PROPAGATION_BOUNDS_NS,
    };
    add_bucket(bucket, elapsed);
}

pub(crate) fn record_propagation_fixed_point_pass() {
    add_count(&PROPAGATION_FIXED_POINT_PASSES, 1);
}

#[derive(Clone, Copy)]
pub(crate) enum SparsePassCause {
    Initial,
    ValueDeductions,
    RatioDeductions,
    ValueAndRatioDeductions,
}

pub(crate) fn record_sparse_profile(
    profile: &crate::algebra::sparse::SparseProfile,
    cause: SparsePassCause,
) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let total_ns = duration_ns(profile.total);
    PROPAGATION_SPARSE_PROFILED_CALLS.fetch_add(1, Ordering::Relaxed);
    PROPAGATION_SPARSE_PREPARATION_NS
        .fetch_add(duration_ns(profile.preparation), Ordering::Relaxed);
    PROPAGATION_SPARSE_FORWARD_NS.fetch_add(duration_ns(profile.forward), Ordering::Relaxed);
    PROPAGATION_SPARSE_BACK_REDUCTION_NS
        .fetch_add(duration_ns(profile.back_reduction), Ordering::Relaxed);
    PROPAGATION_SPARSE_DEDUCTIONS_NS.fetch_add(duration_ns(profile.deductions), Ordering::Relaxed);
    PROPAGATION_SPARSE_INPUT_ROWS.fetch_add(count(profile.input_rows), Ordering::Relaxed);
    PROPAGATION_SPARSE_ACTIVE_ROWS.fetch_add(count(profile.active_rows), Ordering::Relaxed);
    PROPAGATION_SPARSE_VARIABLES.fetch_add(count(profile.variable_count), Ordering::Relaxed);
    PROPAGATION_SPARSE_NONZERO_TERMS.fetch_add(count(profile.nonzero_terms), Ordering::Relaxed);

    let cause_index = match cause {
        SparsePassCause::Initial => 0,
        SparsePassCause::ValueDeductions => 1,
        SparsePassCause::RatioDeductions => 2,
        SparsePassCause::ValueAndRatioDeductions => 3,
    };
    PROPAGATION_SPARSE_CAUSE_CALLS[cause_index].fetch_add(1, Ordering::Relaxed);
    record_shape_bucket(
        profile.active_rows,
        [16, 32, 48],
        total_ns,
        &PROPAGATION_SPARSE_ROW_BUCKET_CALLS,
        &PROPAGATION_SPARSE_ROW_BUCKET_NS,
    );
    record_shape_bucket(
        profile.variable_count,
        [16, 32, 48],
        total_ns,
        &PROPAGATION_SPARSE_VARIABLE_BUCKET_CALLS,
        &PROPAGATION_SPARSE_VARIABLE_BUCKET_NS,
    );
    record_shape_bucket(
        profile.nonzero_terms,
        [64, 128, 192],
        total_ns,
        &PROPAGATION_SPARSE_TERM_BUCKET_CALLS,
        &PROPAGATION_SPARSE_TERM_BUCKET_NS,
    );
}

fn count(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn record_shape_bucket(
    value: usize,
    boundaries: [usize; 3],
    elapsed_ns: u64,
    calls: &[AtomicU64; 4],
    time: &[AtomicU64; 4],
) {
    let index = boundaries
        .iter()
        .position(|boundary| value < *boundary)
        .unwrap_or(3);
    calls[index].fetch_add(1, Ordering::Relaxed);
    time[index].fetch_add(elapsed_ns, Ordering::Relaxed);
}

pub(crate) fn record_scc_canonicalize(elapsed: Duration) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    SCC_CANONICALIZE_NS.fetch_add(duration_ns(elapsed), Ordering::Relaxed);
    SCC_CANONICALIZE_CALLS.fetch_add(1, Ordering::Relaxed);
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

#[derive(Clone, Copy)]
pub(crate) enum GraphCanonPurpose {
    State,
    OpenPort,
    MarkedLink,
    Other,
}

pub(crate) fn record_graph_canon(elapsed: Duration, purpose: GraphCanonPurpose) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let ns = duration_ns(elapsed);
    GRAPH_CANON_NS.fetch_add(ns, Ordering::Relaxed);
    GRAPH_CANON_CALLS.fetch_add(1, Ordering::Relaxed);
    let (time, calls) = match purpose {
        GraphCanonPurpose::State => (&STATE_GRAPH_CANON_NS, &STATE_GRAPH_CANON_CALLS),
        GraphCanonPurpose::OpenPort => (&OPEN_PORT_GRAPH_CANON_NS, &OPEN_PORT_GRAPH_CANON_CALLS),
        GraphCanonPurpose::MarkedLink => {
            (&MARKED_LINK_GRAPH_CANON_NS, &MARKED_LINK_GRAPH_CANON_CALLS)
        }
        GraphCanonPurpose::Other => (&OTHER_GRAPH_CANON_NS, &OTHER_GRAPH_CANON_CALLS),
    };
    time.fetch_add(ns, Ordering::Relaxed);
    calls.fetch_add(1, Ordering::Relaxed);
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
