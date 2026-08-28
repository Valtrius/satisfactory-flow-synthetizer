//! Replay a saved solver witness without repeating topology search.
mod profile_support;

use solver_api::{CanonicalGraphKey, PhysicalGraph, Problem};
use solver_core::{canonical::canonicalize_witness_cancellable, hotspot_profile};
use solver_validation::validate_solution;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    assert_eq!(
        args.len(),
        4,
        "usage: profile_witness saved-result.json output.json cancel-after-ms; 0 means no cancellation"
    );
    let data: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&args[1]).unwrap()).unwrap();
    let problem: Problem = serde_json::from_value(data["problem"].clone()).unwrap();
    let value = &data["outcome"]["result"];
    let graph: PhysicalGraph = serde_json::from_value(value["graph"].clone()).unwrap();
    let expected: CanonicalGraphKey =
        serde_json::from_value(value["canonicalGraphKey"].clone()).unwrap();
    let cancel_ms: u64 = args[3].parse().unwrap();
    let start = Instant::now();
    validate_solution(&problem, &graph).expect("saved caller witness validation");
    let caller_validation_s = start.elapsed().as_secs_f64();
    let start = Instant::now();
    let (normalized, graph) = profile_support::normalize_like_custom(&problem, &graph);
    let normalization_s = start.elapsed().as_secs_f64();
    let start = Instant::now();
    validate_solution(&normalized, &graph).expect("normalized witness validation");
    let normalized_validation_s = start.elapsed().as_secs_f64();
    hotspot_profile::install_recorder();
    solver_core::diagnostics::install();
    let cancel = AtomicBool::new(false);
    let (finished, done) = mpsc::channel();
    let start = Instant::now();
    let canonical = thread::scope(|scope| {
        let timer_cancel = &cancel;
        scope.spawn(move || {
            if cancel_ms != 0 && done.recv_timeout(Duration::from_millis(cancel_ms)).is_err() {
                timer_cancel.store(true, Ordering::Relaxed);
                drop(solver_core::diagnostics::ActivitySpan::start(
                    "cancel_requested",
                    0,
                    None,
                    None,
                    None,
                    0,
                ));
            }
        });
        let result = canonicalize_witness_cancellable(&normalized, &graph, &cancel);
        let _ = finished.send(());
        result
    });
    let canonical_s = start.elapsed().as_secs_f64();
    let hotspots = hotspot_profile::take_snapshot();
    let activity = solver_core::diagnostics::take();
    let validation_s = canonical.as_ref().map(|result| {
        assert_eq!(
            result.key, expected,
            "replay changed the exact authoritative witness key"
        );
        let start = Instant::now();
        validate_solution(&normalized, &result.graph)
            .expect("replayed canonical witness validation");
        start.elapsed().as_secs_f64()
    });
    assert!(
        canonical.is_some() || cancel.load(Ordering::Relaxed),
        "interrupted replay without cancellation"
    );
    assert!(
        cancel_ms != 0 || canonical.is_some(),
        "uncancelled replay must complete"
    );
    let result = serde_json::json!({"kind":"witness_replay", "validated":true, "completed":canonical.is_some(),
        "key_equal":canonical.as_ref().map(|r| r.key == expected), "cancel_after_ms":cancel_ms,
        "cancelled":cancel.load(Ordering::Relaxed), "caller_validation_s":caller_validation_s,
        "normalization_s":normalization_s, "normalized_validation_s":normalized_validation_s,
        "canonical_s":canonical_s, "canonical_validation_s":validation_s,
        "hotspots":profile_support::hotspot_json(&hotspots), "activity":profile_support::activity_json(&activity)});
    std::fs::write(&args[2], serde_json::to_vec_pretty(&result).unwrap()).unwrap();
    println!(
        "completed={} canonical={canonical_s:.3}s cancellation={}ms",
        canonical.is_some(),
        cancel_ms
    );
}
