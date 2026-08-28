//! Optional bounded timing trace. Observations never discharge proof obligations.
use solver_api::NodeProfile;
use std::{
    cell::RefCell,
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

static ENABLED: AtomicBool = AtomicBool::new(false);
static TRACE: Mutex<Option<Trace>> = Mutex::new(None);
static GENERATION: AtomicU64 = AtomicU64::new(0);
const MAX_RECORDS: usize = 50_000;
static ROOT_SEARCHES: AtomicU64 = AtomicU64::new(0);
static STATE_DROPS: AtomicU64 = AtomicU64::new(0);
static SCC_DROPS: AtomicU64 = AtomicU64::new(0);

/// Allocation-free phase counts for an independent diagnostic heartbeat.
#[derive(Clone, Copy, Debug, Default)]
pub struct Heartbeat {
    pub root_searches: u64,
    pub state_cache_drops: u64,
    pub scc_cache_drops: u64,
}

#[must_use]
pub fn heartbeat() -> Heartbeat {
    Heartbeat {
        root_searches: ROOT_SEARCHES.load(Ordering::Relaxed),
        state_cache_drops: STATE_DROPS.load(Ordering::Relaxed),
        scc_cache_drops: SCC_DROPS.load(Ordering::Relaxed),
    }
}

fn phase_counter(kind: &str) -> Option<&'static AtomicU64> {
    match kind {
        "root_search" => Some(&ROOT_SEARCHES),
        "state_cache_drop" => Some(&STATE_DROPS),
        "scc_cache_drop" => Some(&SCC_DROPS),
        _ => None,
    }
}

#[derive(Clone, Debug)]
pub struct ActivityRecord {
    pub kind: &'static str,
    pub node_count: u32,
    pub link_count: Option<u32>,
    pub profile: Option<NodeProfile>,
    pub root: Option<u32>,
    pub worker_budget: usize,
    pub start_ns: u64,
    pub end_ns: u64,
    pub thread: String,
}

#[derive(Clone, Default, Debug)]
pub struct ActivitySnapshot {
    pub records: Vec<ActivityRecord>,
    pub dropped: u64,
    /// Sampled open intervals, never completed work or proof evidence.
    pub active: Vec<ActivityRecord>,
}

struct Trace {
    started: Instant,
    snapshot: ActivitySnapshot,
    active: BTreeMap<u64, ActivityRecord>,
    next_id: u64,
    labelings: Vec<Arc<ActiveLabeling>>,
}

struct ActiveLabeling {
    thread: String,
    node_count: AtomicU32,
    start_ns: AtomicU64,
}

struct LocalLabeling {
    generation: u64,
    epoch: Instant,
    slot: Arc<ActiveLabeling>,
}

thread_local! {
    static LABELING: RefCell<Option<LocalLabeling>> = const { RefCell::new(None) };
}

impl Trace {
    fn new() -> Self {
        Self {
            started: Instant::now(),
            snapshot: ActivitySnapshot::default(),
            active: BTreeMap::new(),
            next_id: 0,
            labelings: Vec::new(),
        }
    }

    fn peek(&self) -> ActivitySnapshot {
        let mut snapshot = self.snapshot.clone();
        let now = nanos(self.started.elapsed().as_nanos());
        snapshot.active = self
            .active
            .values()
            .cloned()
            .map(|mut r| {
                r.end_ns = now;
                r
            })
            .collect();
        for slot in &self.labelings {
            let start_ns = slot.start_ns.load(Ordering::Acquire);
            let node_count = slot.node_count.load(Ordering::Relaxed);
            if start_ns == 0 || start_ns > now || start_ns != slot.start_ns.load(Ordering::Acquire)
            {
                continue;
            }
            snapshot.active.push(ActivityRecord {
                kind: "in_flight_partial_labeling",
                node_count,
                link_count: None,
                profile: None,
                root: None,
                worker_budget: 1,
                start_ns,
                end_ns: now,
                thread: slot.thread.clone(),
            });
        }
        snapshot
    }

    fn push(&mut self, record: ActivityRecord) {
        if self.snapshot.records.len() < MAX_RECORDS {
            self.snapshot.records.push(record);
        } else {
            self.snapshot.dropped = self.snapshot.dropped.saturating_add(1);
        }
    }
}

/// Start one process-wide diagnostic session before spawning solver workers.
pub fn install() {
    ROOT_SEARCHES.store(0, Ordering::Relaxed);
    STATE_DROPS.store(0, Ordering::Relaxed);
    SCC_DROPS.store(0, Ordering::Relaxed);
    *TRACE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Trace::new());
    GENERATION.fetch_add(1, Ordering::Relaxed);
    ENABLED.store(true, Ordering::Relaxed);
}

/// Non-destructive diagnostic sample. Different workers may advance while it is collected.
#[must_use]
pub fn peek() -> ActivitySnapshot {
    TRACE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .as_ref()
        .map_or_else(ActivitySnapshot::default, Trace::peek)
}

/// Read only after all diagnostic workers have joined.
#[must_use]
pub fn take() -> ActivitySnapshot {
    ENABLED.store(false, Ordering::Relaxed);
    TRACE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take()
        .map_or_else(ActivitySnapshot::default, |trace| trace.snapshot)
}

pub struct ActivitySpan {
    record: Option<ActivityRecord>,
    id: u64,
}

/// Records only long calls or calls that observe cancellation on return.
/// After registering once per thread/session, entry only updates thread-local atomics.
pub(crate) struct SlowCall {
    started: Option<Instant>,
    node_count: u32,
}

impl SlowCall {
    pub(crate) fn start(node_count: u32) -> Self {
        let started = ENABLED.load(Ordering::Relaxed).then(Instant::now);
        if let Some(started) = started {
            LABELING.with(|local| {
                let mut local = local.borrow_mut();
                let generation = GENERATION.load(Ordering::Relaxed);
                if local.as_ref().is_none_or(|l| l.generation != generation) {
                    let mut guard = TRACE
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    if let Some(trace) = guard.as_mut() {
                        let slot = Arc::new(ActiveLabeling {
                            thread: format!("{:?}", std::thread::current().id()),
                            node_count: AtomicU32::new(node_count),
                            start_ns: AtomicU64::new(0),
                        });
                        // Worker slots are bounded independently of completed records.
                        if trace.labelings.len() < MAX_RECORDS {
                            trace.labelings.push(Arc::clone(&slot));
                        } else {
                            trace.snapshot.dropped = trace.snapshot.dropped.saturating_add(1);
                        }
                        *local = Some(LocalLabeling {
                            generation,
                            epoch: trace.started,
                            slot,
                        });
                    }
                }
                if let Some(local) = local.as_ref() {
                    local.slot.node_count.store(node_count, Ordering::Relaxed);
                    local.slot.start_ns.store(
                        nanos(started.saturating_duration_since(local.epoch).as_nanos()).max(1),
                        Ordering::Release,
                    );
                }
            });
        }
        Self {
            started,
            node_count,
        }
    }

    pub(crate) fn finish(self, cancelled: bool) {
        let Some(started) = self.started else {
            return;
        };
        let ended = Instant::now();
        LABELING.with(|local| {
            if let Some(local) = local.borrow().as_ref() {
                local.slot.start_ns.store(0, Ordering::Release);
            }
        });
        if !cancelled && ended.duration_since(started) < Duration::from_millis(50) {
            return;
        }
        let mut guard = TRACE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(trace) = guard.as_mut() {
            trace.push(ActivityRecord {
                kind: if cancelled {
                    "partial_labeling_at_cancel"
                } else {
                    "slow_partial_labeling"
                },
                node_count: self.node_count,
                link_count: None,
                profile: None,
                root: None,
                worker_budget: 1,
                start_ns: nanos(started.saturating_duration_since(trace.started).as_nanos()),
                end_ns: nanos(ended.saturating_duration_since(trace.started).as_nanos()),
                thread: format!("{:?}", std::thread::current().id()),
            });
        }
    }
}

impl ActivitySpan {
    #[must_use]
    pub fn start(
        kind: &'static str,
        node_count: u32,
        link_count: Option<u32>,
        profile: Option<NodeProfile>,
        root: Option<u32>,
        worker_budget: usize,
    ) -> Self {
        if !ENABLED.load(Ordering::Relaxed) {
            return Self {
                record: None,
                id: 0,
            };
        }
        let mut trace = TRACE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut id = 0;
        let record = trace.as_mut().map(|trace| {
            id = trace.next_id;
            trace.next_id += 1;
            let record = ActivityRecord {
                kind,
                node_count,
                link_count,
                profile,
                root,
                worker_budget,
                start_ns: nanos(trace.started.elapsed().as_nanos()),
                end_ns: 0,
                thread: format!("{:?}", std::thread::current().id()),
            };
            if let Some(counter) = phase_counter(kind) {
                counter.fetch_add(1, Ordering::Relaxed);
            }
            if trace.active.len() < MAX_RECORDS {
                trace.active.insert(id, record.clone());
            } else {
                trace.snapshot.dropped = trace.snapshot.dropped.saturating_add(1);
            }
            record
        });
        Self { record, id }
    }
}

fn nanos(value: u128) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

impl Drop for ActivitySpan {
    fn drop(&mut self) {
        let Some(mut record) = self.record.take() else {
            return;
        };
        // Publish completion before acquiring the trace mutex. A blocked trace
        // writer cannot make this heartbeat report an already-finished drop.
        if let Some(counter) = phase_counter(record.kind) {
            counter.fetch_sub(1, Ordering::Relaxed);
        }
        let mut guard = TRACE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(trace) = guard.as_mut() {
            trace.active.remove(&self.id);
            record.end_ns = nanos(trace.started.elapsed().as_nanos());
            trace.push(record);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_samples_separate_open_phases_and_labelings_from_completed_work() {
        let mut trace = Trace::new();
        let record = ActivityRecord {
            kind: "root_search",
            node_count: 4,
            link_count: Some(5),
            profile: None,
            root: Some(2),
            worker_budget: 1,
            start_ns: 0,
            end_ns: 0,
            thread: "worker".into(),
        };
        trace.active.insert(0, record);
        let labeling = Arc::new(ActiveLabeling {
            thread: "worker".into(),
            node_count: AtomicU32::new(4),
            start_ns: AtomicU64::new(1),
        });
        trace.labelings.push(Arc::clone(&labeling));
        let sample = trace.peek();
        assert!(sample.records.is_empty());
        assert_eq!(sample.active.len(), 2);
        assert!(
            sample
                .active
                .iter()
                .all(|r| r.thread == "worker" && r.start_ns <= r.end_ns)
        );
        assert!(
            sample
                .active
                .iter()
                .any(|r| r.kind == "in_flight_partial_labeling")
        );
        // Sampling neither closes spans nor drains completed observations.
        assert_eq!(trace.peek().active.len(), 2);
        labeling.start_ns.store(0, Ordering::Release);
        let mut record = trace.active.remove(&0).unwrap();
        record.end_ns = nanos(trace.started.elapsed().as_nanos());
        trace.push(record);
        let finished = trace.peek();
        assert!(finished.active.is_empty());
        assert_eq!(finished.records.len(), 1);
    }
    #[test]
    fn bounds_record_storage_and_reports_truncation() {
        let mut trace = Trace::new();
        let record = ActivityRecord {
            kind: "root",
            node_count: 2,
            link_count: Some(1),
            profile: None,
            root: Some(0),
            worker_budget: 4,
            start_ns: 10,
            end_ns: 20,
            thread: "test".into(),
        };
        for _ in 0..MAX_RECORDS + 2 {
            trace.push(record.clone());
        }
        assert_eq!(trace.snapshot.records.len(), MAX_RECORDS);
        assert_eq!(trace.snapshot.dropped, 2);
        assert!(
            trace
                .snapshot
                .records
                .iter()
                .all(|r| r.start_ns <= r.end_ns)
        );
    }
}
