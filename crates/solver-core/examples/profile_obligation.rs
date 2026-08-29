//! Fixed production N/L/profile work. No global solve or optimality claim.
mod profile_support;

use serde_json::{Value, json};
use solver_api::Problem;
use solver_core::{
    diagnostics, hotspot_profile,
    solver::benchmark::{self, FixedWorkload},
    telemetry::SearchInstrumentation,
};
use std::{
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

fn workload(value: &Value) -> FixedWorkload {
    let number = |key: &str| {
        value[key]
            .as_u64()
            .unwrap_or_else(|| panic!("missing {key}"))
    };
    let mode = value["mode"].as_str().expect("mode");
    let stage = value["stage"].as_str().expect("stage");
    assert!(matches!(mode, "best" | "all"), "unknown fixed-work mode");
    assert!(
        matches!(stage, "baseline" | "p1" | "p12" | "p123"),
        "a fixed group has no remaining-group scheduler"
    );
    assert!(number("node_count") <= 64, "benchmark N cap must be <=64");
    assert!((1..=4096).contains(&number("workers")));
    assert!((1..=86400).contains(&number("timeout_s")));
    assert!(value["hotspots"].is_boolean());
    FixedWorkload {
        node_count: u32::try_from(number("node_count")).unwrap(),
        link_count: u32::try_from(number("link_count")).unwrap(),
        profile: value
            .get("profile")
            .filter(|p| !p.is_null())
            .map(|p| serde_json::from_value(p.clone()).expect("profile")),
        worker_count: usize::try_from(number("workers")).unwrap(),
        parallelism: profile_support::parallelism_stage(Some(stage)),
        collect_all_witnesses: mode == "all",
    }
}

fn prepare_prefix(
    request: &Value,
    problem: &Problem,
    selection: &FixedWorkload,
    list_only: bool,
) -> (Option<benchmark::PreparedPrefix>, Option<Value>) {
    let prefix = request.get("prefix").map(|recipe| {
        benchmark::prepare_prefix(
            problem,
            selection,
            usize::try_from(recipe["depth"].as_u64().expect("prefix depth")).unwrap(),
            usize::try_from(recipe["pick"].as_u64().expect("prefix pick")).unwrap(),
        )
        .expect("valid prefix selection")
    });
    let prefix_identity = prefix.as_ref().map(|prepared| {
        let id = prepared.identity();
        json!({"version":1,"frontiers":id.frontiers,"route":id.route,
            "decisions":id.decisions,"stable_key":id.stable_key})
    });
    if !list_only {
        if let Some(identity) = &prefix_identity {
            assert_eq!(
                request.get("prefix_identity"),
                Some(identity),
                "changed or missing frozen prefix identity"
            );
        } else {
            assert!(
                request.get("prefix_identity").is_none(),
                "prefix identity without a selection"
            );
        }
    }
    (prefix, prefix_identity)
}

fn prepare_root(
    request: &Value,
    problem: &Problem,
    selection: &FixedWorkload,
    list_only: bool,
) -> (Option<benchmark::PreparedRoot>, Option<Value>) {
    let root = request.get("root").map(|recipe| {
        benchmark::prepare_root(
            problem,
            selection,
            u32::try_from(recipe["ordinal"].as_u64().expect("root ordinal")).unwrap(),
        )
        .expect("valid adaptive root selection")
    });
    let root_identity = root.as_ref().map(|prepared| {
        let id = prepared.identity();
        json!({"version":1,"target":id.target,"ordinal":id.ordinal,
            "plan_keys":id.plan_keys,"stable_key":id.stable_key})
    });
    if !list_only {
        if let Some(identity) = &root_identity {
            assert_eq!(
                request.get("root_identity"),
                Some(identity),
                "changed or missing frozen adaptive root identity"
            );
        } else {
            assert!(
                request.get("root_identity").is_none(),
                "root identity without a selection"
            );
        }
    }
    (root, root_identity)
}

fn load_request(request_path: &Path) -> (Value, Problem) {
    let request: Value = serde_json::from_slice(&std::fs::read(request_path).unwrap()).unwrap();
    let case_path = request_path
        .parent()
        .unwrap()
        .join(request["case_file"].as_str().expect("case_file"));
    let case: Value = serde_json::from_slice(&std::fs::read(case_path).unwrap()).unwrap();
    let problem: Problem = serde_json::from_value(case["problem"].clone()).expect("problem");
    assert_eq!(
        problem.max_link_rate,
        1200.into(),
        "benchmark capacity must be 1200"
    );
    (request, problem)
}

fn profile_reports(result: Vec<benchmark::FixedProfileResult>) -> Vec<Value> {
    result.into_iter().map(|p| {
        json!({"profile":p.profile,"exhausted":p.exhausted,"incomplete_reason":p.incomplete_reason,
            "roots":p.roots,"roots_exhausted":p.roots_exhausted,
            "diagnostics":p.instrumentation.diagnostics(), "solutions":p.witnesses})
    }).collect::<Vec<_>>()
}

fn print_listing(
    problem: &Problem,
    profiles: &[solver_api::NodeProfile],
    identities: [Option<&Value>; 2],
) {
    let [prefix_identity, root_identity] = identities;
    println!(
        "{}",
        json!({"problem":problem,"profiles":profiles,"prefix_identity":prefix_identity,
            "root_identity":root_identity})
    );
}

fn assert_exclusive_selection(request: &Value) {
    assert!(
        request.get("prefix").is_none() || request.get("root").is_none(),
        "prefix and adaptive root selections are mutually exclusive"
    );
}

fn proof_scope(has_prefix: bool, has_root: bool) -> &'static str {
    match (has_prefix, has_root) {
        (true, false) => "selected_prefix",
        (false, true) => "selected_root",
        (false, false) => "selected_profiles",
        (true, true) => unreachable!("exclusive selection checked before preparation"),
    }
}

fn request_path() -> (bool, PathBuf, PathBuf) {
    let args = std::env::args().collect::<Vec<_>>();
    assert_eq!(
        args.len(),
        3,
        "usage: profile_obligation request.json output.json | --list request.json"
    );
    let list_only = args[1] == "--list";
    let path = Path::new(if list_only { &args[2] } else { &args[1] }).to_path_buf();
    (list_only, path, PathBuf::from(&args[2]))
}

fn main() {
    let (list_only, request_path, output_path) = request_path();
    let (request, problem) = load_request(&request_path);
    let selection = workload(&request);
    assert_exclusive_selection(&request);
    let expected = benchmark::profiles(&problem, &selection).expect("valid exact selection");
    let preparation_started = Instant::now();
    let (prefix, prefix_identity) = prepare_prefix(&request, &problem, &selection, list_only);
    let (root, root_identity) = prepare_root(&request, &problem, &selection, list_only);
    let preparation_s = preparation_started.elapsed().as_secs_f64();
    if list_only {
        print_listing(
            &problem,
            &expected,
            [prefix_identity.as_ref(), root_identity.as_ref()],
        );
        return;
    }
    let output = output_path.as_path();
    let record = request["hotspots"].as_bool().unwrap();
    if record {
        hotspot_profile::install_recorder();
        diagnostics::install();
    }
    let cancel = AtomicBool::new(false);
    let deadline_fired = AtomicBool::new(false);
    let progress = Mutex::new(SearchInstrumentation::default());
    let (finished_tx, finished_rx) = mpsc::channel();
    let (heartbeat_tx, heartbeat_rx) = mpsc::channel();
    let (live_tx, live_rx) = mpsc::channel();
    let started = Instant::now();
    let result = thread::scope(|scope| {
        let timer_fired = &deadline_fired;
        let timer_cancel = &cancel;
        let timeout = Duration::from_secs(request["timeout_s"].as_u64().unwrap());
        scope.spawn(move || {
            if finished_rx.recv_timeout(timeout).is_err() {
                timer_fired.store(true, Ordering::Relaxed);
                timer_cancel.store(true, Ordering::Relaxed);
                drop(diagnostics::ActivitySpan::start("cancel_requested", selection.node_count,
                    Some(selection.link_count), selection.profile, None, 0));
            }
        });
        if record {
            let mut heartbeat = std::fs::File::create(output.with_extension("heartbeat.log")).unwrap();
            let flag = &cancel;
            scope.spawn(move || loop {
                let returned = heartbeat_rx.recv_timeout(Duration::from_secs(5)) != Err(mpsc::RecvTimeoutError::Timeout);
                profile_support::write_heartbeat(&mut heartbeat, started.elapsed(), flag.load(Ordering::Relaxed), returned).unwrap();
                if returned { break; }
            });
            let progress = &progress;
            let cancel = &cancel;
            scope.spawn(move || {
                let mut sequence = 0_u32;
                while live_rx.recv_timeout(Duration::from_secs(15)) == Err(mpsc::RecvTimeoutError::Timeout) {
                    sequence += 1;
                    let sample = json!({"diagnostic_only":true,"solver_returned":false,
                        "elapsed_s":started.elapsed().as_secs_f64(),"cancel_requested":cancel.load(Ordering::Relaxed),
                        "diagnostics":progress.lock().unwrap().diagnostics(),
                        "hotspots":profile_support::hotspot_json(&hotspot_profile::peek_snapshot()),
                        "activity":profile_support::activity_json(&diagnostics::peek())});
                    profile_support::write_live_snapshot(&output.with_extension(format!("live-{sequence:04}.json")), &sample).unwrap();
                }
            });
        }
        let publish = |snapshot: &SearchInstrumentation| {
            *progress.lock().unwrap() = snapshot.clone();
        };
        let result = match (&prefix, &root) {
            (Some(prepared), None) => prepared
                .run(prepared.identity(), &cancel, &publish)
                .map(|r| vec![r]),
            (None, Some(prepared)) => prepared
                .run(prepared.identity(), &cancel, &publish)
                .map(|r| vec![r]),
            (None, None) => benchmark::run(&problem, &selection, &cancel, &publish),
            (Some(_), Some(_)) => unreachable!("exclusive selection checked before preparation"),
        };
        let _ = finished_tx.send(());
        let _ = heartbeat_tx.send(());
        let _ = live_tx.send(());
        result
    }).expect("fixed production workload failed");
    let wall_s = started.elapsed().as_secs_f64();
    let exhausted = result.iter().all(|p| p.exhausted);
    let profiles = profile_reports(result);
    let proof_scope = proof_scope(prefix.is_some(), root.is_some());
    let report = json!({"schema_version":1,"kind":"fixed_obligation","scope":proof_scope,
        "request":request,"problem":problem,"expected_profiles":expected,
        "prefix_identity":prefix_identity,"root_identity":root_identity,
        "preparation_s":preparation_s,
        "status":if exhausted {"exhausted"} else {"incomplete"}, "profiles":profiles,
        "validated":true,"wall_s":wall_s,"deadline_fired":deadline_fired.load(Ordering::Relaxed),
        "hotspots":profile_support::hotspot_json(&hotspot_profile::take_snapshot()),
        "activity":profile_support::activity_json(&diagnostics::take())});
    std::fs::write(output, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    println!("fixed workload exhausted={exhausted} wall={wall_s:.3}s");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_mode_and_exact_selection_are_not_global_solve_options() {
        let selected = workload(&json!({"mode":"best","stage":"p1","node_count":11,
            "link_count":18,"profile":{"splitter2":2,"splitter3":4,"merger2":3,"merger3":2},
            "workers":32,"timeout_s":60,"hotspots":true}));
        assert_eq!((selected.node_count, selected.link_count), (11, 18));
        assert!(!selected.collect_all_witnesses);
        assert!(selected.parallelism.deep_partitions && selected.profile.is_some());
    }
}
