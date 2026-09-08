//! Production API benchmark. Positional arguments match `profile_case`; engine is explicit.
use solver_api::{
    BestKnownSolution, CanonicalGraphKey, ConsumerPortRef, Problem, ProducerPortRef, RunOptions,
    SolveMode, SolveResult, SolverEvent,
};
use std::{
    collections::BTreeMap,
    fmt::Write,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
use synthetizer_app::runtime::{SolverEngine, solve};

fn hex(key: &CanonicalGraphKey) -> String {
    key.as_bytes().iter().fold(String::new(), |mut s, b| {
        write!(s, "{b:02x}").unwrap();
        s
    })
}
fn canonical(problem: &Problem, mut solution: BestKnownSolution) -> BestKnownSolution {
    let solver_core::Preparation::Prepared(normalized) =
        solver_core::prepare_problem(problem).unwrap()
    else {
        panic!("witness for impossible problem")
    };
    let scaled = Problem {
        inputs: normalized.inputs.as_slice().to_vec(),
        outputs: normalized.outputs.as_slice().to_vec(),
        max_link_rate: normalized.max_link_rate.clone(),
    };
    for link in &mut solution.graph.links {
        link.flow = &link.flow / &normalized.original_scale;
        if let ProducerPortRef::Input(ref mut id) = link.producer {
            id.0 = u32::try_from(
                normalized
                    .terminal_mapping
                    .normalized_input(id.0 as usize)
                    .unwrap(),
            )
            .unwrap();
        }
        if let ConsumerPortRef::Output(ref mut id) = link.consumer {
            id.0 = u32::try_from(
                normalized
                    .terminal_mapping
                    .normalized_output(id.0 as usize)
                    .unwrap(),
            )
            .unwrap();
        }
    }
    let witness = solver_core::canonical::canonicalize_witness(&scaled, &solution.graph);
    solution.graph = witness.graph;
    solution.canonical_graph_key = witness.key;
    for link in &mut solution.graph.links {
        link.flow = &link.flow * &normalized.original_scale;
        if let ProducerPortRef::Input(ref mut id) = link.producer {
            id.0 = u32::try_from(
                normalized
                    .terminal_mapping
                    .original_input(id.0 as usize)
                    .unwrap(),
            )
            .unwrap();
        }
        if let ConsumerPortRef::Output(ref mut id) = link.consumer {
            id.0 = u32::try_from(
                normalized
                    .terminal_mapping
                    .original_output(id.0 as usize)
                    .unwrap(),
            )
            .unwrap();
        }
    }
    solution.validation = solver_validation::validate_solution(problem, &solution.graph).unwrap();
    solution
}

#[allow(clippy::too_many_lines)]
fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(
        args.len(),
        10,
        "seconds workers max_nodes engine baseline output mode off case.json"
    );
    let seconds: u64 = args[1].parse().unwrap();
    let workers = args[2].parse().unwrap();
    let max_nodes = args[3].parse().unwrap();
    let engine = match args[4].as_str() {
        "custom" => SolverEngine::Custom,
        "z3" => SolverEngine::Z3,
        "astra" => SolverEngine::Astra,
        _ => panic!("unknown engine"),
    };
    assert_eq!(
        args[5], "baseline",
        "production API comparisons use baseline stage"
    );
    assert_eq!(
        args[8], "off",
        "production comparisons exclude profiling instrumentation"
    );
    let mode = match args[7].as_str() {
        "optimal" => SolveMode::Optimal,
        "minimum_links" => SolveMode::AllAtMinimumNodesAndMinimumLinks,
        "all" => SolveMode::AllAtMinimumNodes,
        _ => panic!("unknown scope"),
    };
    let case: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&args[9]).unwrap()).unwrap();
    let problem: Problem = serde_json::from_value(case["problem"].clone()).unwrap();
    let cancel = AtomicBool::new(false);
    let deadline = AtomicBool::new(false);
    let first = Mutex::new(None);
    let progress = Mutex::new(None);
    let min_links = Mutex::new(None);
    let started = Instant::now();
    let native = thread::scope(|scope| {
        let (send, receive) = mpsc::channel();
        let timer_cancel = &cancel;
        let timer_deadline = &deadline;
        let timer = scope.spawn(move || {
            if receive.recv_timeout(Duration::from_secs(seconds)).is_err() {
                timer_deadline.store(true, Ordering::Relaxed);
                timer_cancel.store(true, Ordering::Relaxed);
            }
        });
        let outcome = solve(
            engine,
            &problem,
            &RunOptions {
                mode,
                max_nodes: Some(max_nodes),
                worker_count: workers,
            },
            &cancel,
            &|event| match event {
                SolverEvent::Incumbent(s) | SolverEvent::SolutionFound(s) => {
                    solver_validation::validate_solution(&problem, &s.graph).unwrap();
                    first
                        .lock()
                        .unwrap()
                        .get_or_insert(started.elapsed().as_secs_f64());
                }
                SolverEvent::Progress(p) => {
                    for d in &p.custom {
                        if d.name == "astra.minimum_links_complete_ms"
                            && let solver_api::DiagnosticValue::Integer(ms) = &d.value
                        {
                            *min_links.lock().unwrap() = Some(ms.parse::<f64>().unwrap() / 1000.0);
                        }
                    }
                    *progress.lock().unwrap() = Some(p);
                }
            },
        );
        let _ = send.send(());
        timer.join().unwrap();
        outcome.expect("solver API failure")
    });
    let wall = started.elapsed().as_secs_f64();
    let post = Instant::now();
    let mut outcome = native.result.clone();
    let mut layouts = BTreeMap::new();
    for s in &native.solutions {
        let s = canonical(&problem, s.clone());
        layouts.insert(s.canonical_graph_key.clone(), s);
    }
    let mut preferred_key = None;
    let status = match &mut outcome {
        SolveResult::Optimal(s) => {
            let best = canonical(
                &problem,
                BestKnownSolution {
                    node_count: s.node_count,
                    link_count: s.link_count,
                    physical_link_count: s.physical_link_count,
                    discard_link_count: s.discard_link_count,
                    canonical_graph_key: s.canonical_graph_key.clone(),
                    graph: s.graph.clone(),
                    validation: s.validation.clone(),
                },
            );
            s.graph = best.graph.clone();
            s.canonical_graph_key = best.canonical_graph_key.clone();
            preferred_key = Some(hex(&s.canonical_graph_key));
            layouts.insert(best.canonical_graph_key.clone(), best);
            format!("Optimal(N={}, L={})", s.node_count, s.link_count)
        }
        SolveResult::Incomplete(s) => {
            if let Some(best) = &mut s.best_known {
                *best = canonical(&problem, best.clone());
                if mode == SolveMode::Optimal {
                    layouts.insert(best.canonical_graph_key.clone(), best.clone());
                }
            }
            format!("Incomplete({:?})", s.reason)
        }
        SolveResult::GloballyUnsat(s) => format!("GloballyUnsat({:?})", s.reason),
    };
    let keys: Vec<_> = layouts.keys().map(hex).collect();
    let last = progress.into_inner().unwrap();
    let complete = matches!(outcome, SolveResult::Optimal(_));
    let result = serde_json::json!({
        "case":case["name"],"problem":problem,"engine":args[4],"stage":args[5],"mode":args[7],"workers":workers,
        "max_nodes":max_nodes,"timeout_s":seconds,"hotspot_recording":false,"status":status,"wall_s":wall,
        "first_valid_s":first.into_inner().unwrap(),"optimal_complete_s":(complete&&mode==SolveMode::Optimal).then_some(wall),
        "minimum_links_complete_s":if mode==SolveMode::AllAtMinimumNodesAndMinimumLinks&&complete {Some(wall)} else {min_links.into_inner().unwrap()},
        "all_complete_s":(complete&&mode==SolveMode::AllAtMinimumNodes).then_some(wall),
        "native_outcome":native,"outcome":outcome,"deadline_fired":deadline.load(Ordering::Relaxed),
        "validated":true,"layout_keys":keys,"layouts":layouts.len(),"preferred_key":preferred_key,
        "solutions":layouts.values().collect::<Vec<_>>(),"diagnostics":last.as_ref().map(|p|&p.custom),"last_progress":last,
        "hotspots":{},"accounted_timer_s":0,"activity":null,"comparison_canonicalization_s":post.elapsed().as_secs_f64(),
        "cvc5_executable":(engine==SolverEngine::Astra).then(solver_astra::cvc5_executable)
    });
    std::fs::write(&args[6], serde_json::to_vec_pretty(&result).unwrap()).unwrap();
    println!("{status}; engine={} complete_wall={wall}s", args[4]);
}
