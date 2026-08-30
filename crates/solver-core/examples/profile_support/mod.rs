//! Shared dual-engine profiling and exact cross-engine layout comparison.

#![allow(clippy::cast_precision_loss, clippy::too_many_lines)]

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use solver_api::{
    BestKnownSolution, CanonicalGraphKey, ConsumerPortRef, Diagnostic, InputTerminalIndex,
    OutputTerminalIndex, PhysicalGraph, Problem, ProducerPortRef, Rational, SolvePhase,
    SolverEvent,
};
use solver_core::{
    ParallelismOptions, Preparation, SolveOptions,
    canonical::{canonicalize_effective_layout, canonicalize_witness},
    enumerate_minimum_links_with_observer, enumerate_with_observer,
    hotspot_profile::{self, HotspotSnapshot},
    prepare_problem, solve_with_observer,
};
use solver_validation::validate_solution;

#[derive(Clone, Copy)]
#[allow(dead_code)] // Named examples use this; the file-based example uses run_file.
pub struct ProfileCase {
    pub name: &'static str,
    pub inputs: &'static [i64],
    pub outputs: &'static [i64],
    pub belt_rate: i64,
    pub default_timeout_seconds: u64,
    pub default_max_nodes: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EngineSelection {
    Custom,
    Z3,
    Both,
}

impl EngineSelection {
    fn parse(value: Option<&str>) -> Self {
        match value {
            Some("custom") => Self::Custom,
            Some("z3") => Self::Z3,
            Some("both") | None => Self::Both,
            Some(other) => panic!("unknown engine '{other}'; expected custom, z3, or both"),
        }
    }

    const fn includes_custom(self) -> bool {
        matches!(self, Self::Custom | Self::Both)
    }

    const fn includes_z3(self) -> bool {
        matches!(self, Self::Z3 | Self::Both)
    }
}

struct CustomRun {
    wall: Duration,
    first_valid: Option<Duration>,
    status: String,
    outcome: Option<solver_api::SolveResult>,
    deadline_fired: bool,
    preferred_key: Option<CanonicalGraphKey>,
    layouts: BTreeMap<CanonicalGraphKey, BestKnownSolution>,
    last_progress: Option<ProgressLine>,
    hotspots: HotspotSnapshot,
    activity: solver_core::diagnostics::ActivitySnapshot,
}

struct Z3Run {
    wall: Duration,
    status: String,
    emitted_count: usize,
    terminal_count: usize,
    layouts: Vec<BestKnownSolution>,
}

struct CrossCanonicalZ3 {
    layouts: BTreeMap<CanonicalGraphKey, CrossLayout>,
    effective_layouts: BTreeMap<CanonicalGraphKey, CrossLayout>,
    duplicate_groups: Vec<(CanonicalGraphKey, usize)>,
    normalization_wall: Duration,
}

#[derive(Clone)]
struct CrossLayout {
    link_count: u32,
    cyclic_scc_count: u32,
    peak_rate: Rational,
}

#[derive(Clone, Debug)]
struct ProgressLine {
    phase: SolvePhase,
    obligation: Option<String>,
    custom: Vec<Diagnostic>,
    snapshot: solver_api::SolverProgress,
}

#[allow(dead_code)]
pub fn run(case: ProfileCase) {
    run_problem(
        case.name,
        &api_problem(&case),
        case.default_timeout_seconds,
        case.default_max_nodes,
    );
}

/// A file case uses exact rational strings and the same positional options as the named examples.
#[allow(dead_code)]
pub fn run_file() {
    let path = argument(9).expect("expected case JSON path as argument 9");
    let case: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).expect("read case JSON"))
            .expect("parse case JSON");
    let problem: Problem = serde_json::from_value(case["problem"].clone()).expect("exact problem");
    run_problem(case["name"].as_str().expect("case name"), &problem, 180, 12);
}

fn run_problem(
    name: &str,
    problem: &Problem,
    default_timeout_seconds: u64,
    default_max_nodes: u32,
) {
    let seconds = argument(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(default_timeout_seconds);
    let workers = argument(2)
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(available_workers);
    let max_nodes = argument(3)
        .and_then(|value| value.parse().ok())
        .unwrap_or(default_max_nodes);
    let engine_argument = argument(4);
    let engines = EngineSelection::parse(engine_argument.as_deref());
    let parallelism = parallelism_stage(argument(5).as_deref());
    let mode = match argument(7).as_deref().unwrap_or("all") {
        "all" => solver_api::SolveMode::AllAtMinimumNodes,
        "minimum_links" => solver_api::SolveMode::AllAtMinimumNodesAndMinimumLinks,
        "optimal" => solver_api::SolveMode::Optimal,
        other => panic!("unknown mode: {other}; expected all, minimum_links, or optimal"),
    };
    let record_hotspots = match argument(8).as_deref().unwrap_or("on") {
        "on" => true,
        "off" => false,
        other => panic!("unknown hotspot recording setting: {other}"),
    };
    println!("dual-engine profile: {name} @{}", problem.max_link_rate);
    println!(
        "timeout={seconds}s custom_workers={workers} custom_max_nodes={max_nodes} engines={engines:?}"
    );
    println!("Z3 manages its own portfolio from the machine's available parallelism.");

    let custom = engines.includes_custom().then(|| {
        run_custom(
            problem,
            seconds,
            workers,
            max_nodes,
            parallelism,
            mode,
            record_hotspots,
        )
    });
    let z3 = engines
        .includes_z3()
        .then(|| run_z3(problem, seconds, max_nodes, mode));

    if let Some(custom) = &custom {
        print_custom(problem, custom);
        if let Some(path) = argument(6) {
            let keys: Vec<_> = custom
                .layouts
                .keys()
                .map(|key| {
                    key.as_bytes().iter().fold(String::new(), |mut text, byte| {
                        write!(&mut text, "{byte:02x}").unwrap();
                        text
                    })
                })
                .collect();
            for solution in custom.layouts.values() {
                validate_solution(problem, &solution.graph).expect("benchmark witness validation");
            }
            if let Some(solver_api::SolveResult::Incomplete(incomplete)) = &custom.outcome
                && let Some(best) = &incomplete.best_known
            {
                validate_solution(problem, &best.graph).expect("benchmark incumbent validation");
            }
            let result = serde_json::json!({
                "case": name, "problem": problem, "stage": argument(5).unwrap_or_else(|| "baseline".into()),
                "mode": match mode {
                    solver_api::SolveMode::Optimal => "optimal",
                    solver_api::SolveMode::AllAtMinimumNodesAndMinimumLinks => "minimum_links",
                    solver_api::SolveMode::AllAtMinimumNodes => "all",
                },
                "hotspot_recording": record_hotspots,
                "workers": workers, "max_nodes": max_nodes, "timeout_s": seconds,
                "status": custom.status, "wall_s": custom.wall.as_secs_f64(),
                "first_valid_s": custom.first_valid.map(|time| time.as_secs_f64()),
                "validated": true,
                "outcome": custom.outcome, "deadline_fired": custom.deadline_fired,
                "last_progress": custom.last_progress.as_ref().map(|p| &p.snapshot),
                "solutions": custom.layouts.values().collect::<Vec<_>>(),
                "accounted_timer_s": ns_to_s(custom.hotspots.accounted_ns()),
                "hotspots": hotspot_json(&custom.hotspots),
                "activity": record_hotspots.then(|| activity_json(&custom.activity)),
                "layout_keys": keys, "layouts": custom.layouts.len(),
                "preferred_key": custom.preferred_key.as_ref().map(|key| key.as_bytes().iter().fold(String::new(), |mut text, byte| { write!(&mut text, "{byte:02x}").unwrap(); text })),
                "diagnostics": custom.last_progress.as_ref().map(|p| &p.custom),
            });
            std::fs::write(path, serde_json::to_vec_pretty(&result).unwrap()).unwrap();
        }
    }
    let cross_z3 = z3.as_ref().map(|run| canonicalize_z3(problem, run));
    if let (Some(z3), Some(cross)) = (&z3, &cross_z3) {
        print_z3(z3, cross);
    }
    if let (Some(custom), Some(z3)) = (&custom, &cross_z3) {
        print_comparison(problem, custom, z3);
    }
}

pub(super) fn parallelism_stage(stage: Option<&str>) -> ParallelismOptions {
    let mut options = ParallelismOptions::default();
    match stage.unwrap_or("baseline") {
        "baseline" => {}
        "p1" => options.deep_partitions = true,
        "shared" => options.shared_state_cache = true,
        "groups" => options.parallel_remaining_groups = true,
        "p14" => {
            options.deep_partitions = true;
            options.parallel_remaining_groups = true;
        }
        "p12" | "p123" | "p124" | "p1234" => {
            options.deep_partitions = true;
            options.shared_state_cache = true;
            options.work_stealing = matches!(stage, Some("p123" | "p1234"));
            options.parallel_remaining_groups = matches!(stage, Some("p124" | "p1234"));
        }
        other => panic!("unknown parallelism stage: {other}"),
    }
    options
}

fn argument(index: usize) -> Option<String> {
    std::env::args().nth(index)
}

fn available_workers() -> usize {
    thread::available_parallelism().map_or(1, std::num::NonZero::get)
}

#[allow(dead_code)]
fn api_problem(case: &ProfileCase) -> Problem {
    Problem {
        inputs: case.inputs.iter().copied().map(Rational::from).collect(),
        outputs: case.outputs.iter().copied().map(Rational::from).collect(),
        max_link_rate: Rational::from(case.belt_rate),
    }
}

#[allow(clippy::too_many_arguments)]
fn run_custom(
    problem: &Problem,
    seconds: u64,
    workers: usize,
    max_nodes: u32,
    parallelism: ParallelismOptions,
    mode: solver_api::SolveMode,
    record_hotspots: bool,
) -> CustomRun {
    let options = SolveOptions {
        max_nodes: Some(max_nodes),
        worker_count: workers,
        parallelism,
    };
    let cancel = Arc::new(AtomicBool::new(false));
    let last_progress = Arc::new(Mutex::new(None::<ProgressLine>));
    let layouts = Arc::new(Mutex::new(
        BTreeMap::<CanonicalGraphKey, BestKnownSolution>::new(),
    ));
    let progress_slot = Arc::clone(&last_progress);
    let layout_slot = Arc::clone(&layouts);
    let first_valid = Arc::new(Mutex::new(None::<Instant>));
    let first_valid_slot = Arc::clone(&first_valid);
    let observer = move |event: SolverEvent| match event {
        SolverEvent::Progress(progress) => {
            *progress_slot.lock().expect("progress lock") = Some(ProgressLine {
                snapshot: progress.clone(),
                phase: progress.phase,
                obligation: Some(format!(
                    "N={:?} L={:?}",
                    progress.node_count, progress.link_constraint
                )),
                custom: progress.custom,
            });
        }
        SolverEvent::SolutionFound(solution) => {
            first_valid_slot
                .lock()
                .expect("first witness lock")
                .get_or_insert_with(Instant::now);
            layout_slot
                .lock()
                .expect("layout lock")
                .insert(solution.canonical_graph_key.clone(), solution);
        }
        SolverEvent::Incumbent(_) => {
            first_valid_slot
                .lock()
                .expect("first witness lock")
                .get_or_insert_with(Instant::now);
        }
    };

    if record_hotspots {
        hotspot_profile::install_recorder();
        solver_core::diagnostics::install();
    }
    let (finished_tx, finished_rx) = mpsc::channel();
    let cancel_for_timer = Arc::clone(&cancel);
    let deadline_fired = Arc::new(AtomicBool::new(false));
    let timer_fired = Arc::clone(&deadline_fired);
    let timer_progress = Arc::clone(&last_progress);
    let live_path = record_hotspots
        .then(|| argument(6))
        .flatten()
        .map(std::path::PathBuf::from);
    let started = Instant::now();
    let (heartbeat_tx, heartbeat_rx) = mpsc::channel();
    let heartbeat = live_path.as_ref().map(|path| {
        let mut file =
            std::fs::File::create(path.with_extension("heartbeat.log")).expect("create heartbeat");
        let flag = Arc::clone(&cancel);
        thread::spawn(move || {
            loop {
                let returned = heartbeat_rx.recv_timeout(Duration::from_secs(5))
                    != Err(mpsc::RecvTimeoutError::Timeout);
                if let Err(error) = write_heartbeat(
                    &mut file,
                    started.elapsed(),
                    flag.load(Ordering::Relaxed),
                    returned,
                ) {
                    eprintln!("Cannot write diagnostic heartbeat: {error}");
                    break;
                }
                if returned {
                    break;
                }
            }
        })
    });
    let timer = thread::spawn(move || {
        if finished_rx
            .recv_timeout(Duration::from_secs(seconds))
            .is_err()
        {
            timer_fired.store(true, Ordering::Relaxed);
            cancel_for_timer.store(true, Ordering::Relaxed);
            drop(solver_core::diagnostics::ActivitySpan::start(
                "cancel_requested",
                0,
                None,
                None,
                None,
                0,
            ));
            let live = hotspot_profile::peek_snapshot();
            eprintln!(
                "Custom timer fired: graph_canons={} state_canon_elapsed_sum={:.1}s complete_elapsed_sum={:.1}s",
                live.graph_canon_calls,
                ns_to_s(live.state_canonicalize_ns),
                ns_to_s(live.evaluate_complete_ns),
            );
            let mut sample = 0;
            loop {
                if let Some(path) = &live_path {
                    let progress = timer_progress.lock().expect("progress lock").clone();
                    let snapshot = serde_json::json!({
                        "diagnostic_only": true,
                        "solver_returned": false,
                        "elapsed_s": started.elapsed().as_secs_f64(),
                        "cancel_requested": true,
                        "last_progress": progress.as_ref().map(|p| &p.snapshot),
                        "hotspots": hotspot_json(&hotspot_profile::peek_snapshot()),
                        "activity": activity_json(&solver_core::diagnostics::peek()),
                    });
                    let sample_path = path.with_extension(format!("live-{sample:04}.json"));
                    if let Err(error) = write_live_snapshot(&sample_path, &snapshot) {
                        eprintln!("Cannot persist cancellation diagnostic: {error}");
                    }
                }
                sample += 1;
                if finished_rx.recv_timeout(Duration::from_secs(15))
                    != Err(mpsc::RecvTimeoutError::Timeout)
                {
                    break;
                }
            }
        }
    });
    let result = match mode {
        solver_api::SolveMode::AllAtMinimumNodes => {
            enumerate_with_observer(problem, &options, &cancel, &observer)
        }
        solver_api::SolveMode::AllAtMinimumNodesAndMinimumLinks => {
            enumerate_minimum_links_with_observer(problem, &options, &cancel, &observer)
        }
        solver_api::SolveMode::Optimal => {
            solve_with_observer(problem, &options, &cancel, &observer)
        }
    };
    let wall = started.elapsed();
    drop(observer);
    let _ = finished_tx.send(());
    let _ = heartbeat_tx.send(());
    if let Some(heartbeat) = heartbeat {
        let _ = heartbeat.join();
    }
    let _ = timer.join();
    let hotspots = hotspot_profile::take_snapshot();
    let activity = solver_core::diagnostics::take();
    if let Ok(solver_api::SolveResult::Optimal(solution)) = &result
        && mode == solver_api::SolveMode::Optimal
    {
        layouts.lock().expect("layout lock").insert(
            solution.canonical_graph_key.clone(),
            BestKnownSolution {
                node_count: solution.node_count,
                link_count: solution.link_count,
                physical_link_count: solution.physical_link_count,
                discard_link_count: solution.discard_link_count,
                canonical_graph_key: solution.canonical_graph_key.clone(),
                graph: solution.graph.clone(),
                validation: solution.validation.clone(),
            },
        );
    }
    let preferred_key = match &result {
        Ok(solver_api::SolveResult::Optimal(solution)) => {
            Some(solution.canonical_graph_key.clone())
        }
        _ => None,
    };
    if let Ok(solver_api::SolveResult::Incomplete(incomplete)) = &result
        && mode == solver_api::SolveMode::Optimal
        && let Some(best) = &incomplete.best_known
    {
        layouts
            .lock()
            .expect("layout lock")
            .insert(best.canonical_graph_key.clone(), best.clone());
    }
    let status = match &result {
        Ok(solver_api::SolveResult::Optimal(solution)) => format!(
            "Optimal(N={}, L={})",
            solution.node_count, solution.link_count
        ),
        Ok(solver_api::SolveResult::Incomplete(incomplete)) => {
            format!("Incomplete({:?})", incomplete.reason)
        }
        Ok(solver_api::SolveResult::GloballyUnsat(proof)) => {
            format!("GloballyUnsat({:?})", proof.reason)
        }
        Err(error) => format!("Error({error})"),
    };

    CustomRun {
        wall,
        first_valid: first_valid
            .lock()
            .expect("first witness lock")
            .map(|time| time.duration_since(started)),
        status,
        outcome: result.ok(),
        deadline_fired: deadline_fired.load(Ordering::Relaxed),
        preferred_key,
        layouts: Arc::try_unwrap(layouts)
            .expect("custom layout observer retained")
            .into_inner()
            .expect("layout lock"),
        last_progress: Arc::try_unwrap(last_progress)
            .expect("custom progress observer retained")
            .into_inner()
            .expect("progress lock"),
        hotspots,
        activity,
    }
}

fn run_z3(problem: &Problem, seconds: u64, max_nodes: u32, mode: solver_api::SolveMode) -> Z3Run {
    let options = solver_api::RunOptions {
        mode,
        worker_count: available_workers(),
        max_nodes: Some(max_nodes),
    };
    let cancel = Arc::new(AtomicBool::new(false));
    let emitted = Arc::new(Mutex::new(Vec::<BestKnownSolution>::new()));
    let emitted_slot = Arc::clone(&emitted);
    let (finished_tx, finished_rx) = mpsc::channel();
    let cancel_for_timer = Arc::clone(&cancel);
    let timer = thread::spawn(move || {
        if finished_rx
            .recv_timeout(Duration::from_secs(seconds))
            .is_err()
        {
            cancel_for_timer.store(true, Ordering::Relaxed);
            eprintln!("Z3 timer fired");
        }
    });

    let started = Instant::now();
    let result = solver_z3::solve_problem(problem, &options, &cancel, &move |event| {
        if let SolverEvent::SolutionFound(solution) = event {
            emitted_slot
                .lock()
                .expect("Z3 emitted layout lock")
                .push(solution);
        }
    });
    let wall = started.elapsed();
    let _ = finished_tx.send(());
    let _ = timer.join();
    let emitted = Arc::try_unwrap(emitted)
        .expect("Z3 layout observer retained")
        .into_inner()
        .expect("Z3 emitted layout lock");
    let emitted_count = emitted.len();
    let (status, terminal) = match result {
        Ok(outcome) => (format!("{:?}", outcome.enumeration), outcome.solutions),
        Err(error) => (format!("Error({error})"), Vec::new()),
    };
    let terminal_count = terminal.len();
    let layouts = if terminal.is_empty() {
        emitted
    } else {
        terminal
    };

    Z3Run {
        wall,
        status,
        emitted_count,
        terminal_count,
        layouts,
    }
}

fn canonicalize_z3(problem: &Problem, run: &Z3Run) -> CrossCanonicalZ3 {
    let started = Instant::now();
    let mut layouts = BTreeMap::new();
    let mut layouts_by_effective_key = BTreeMap::new();
    let mut multiplicities = BTreeMap::<CanonicalGraphKey, usize>::new();
    for solution in &run.layouts {
        let graph = solution.graph.clone();
        let validation = validate_solution(problem, &graph)
            .unwrap_or_else(|error| panic!("shared validator rejected Z3 layout: {error}"));
        let (normalized_problem, normalized_graph) = normalize_like_custom(problem, &graph);
        let canonical = canonicalize_witness(&normalized_problem, &normalized_graph);
        let effective = effective_layout_key(problem, &graph);
        *multiplicities.entry(canonical.key.clone()).or_default() += 1;
        layouts.entry(canonical.key).or_insert(CrossLayout {
            link_count: operator_link_count(&graph),
            cyclic_scc_count: validation.cyclic_scc_count,
            peak_rate: operator_peak_rate(&graph),
        });
        layouts_by_effective_key
            .entry(effective)
            .or_insert(CrossLayout {
                link_count: operator_link_count(&graph),
                cyclic_scc_count: validation.cyclic_scc_count,
                peak_rate: operator_peak_rate(&graph),
            });
    }
    let duplicate_groups = multiplicities
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .collect();
    CrossCanonicalZ3 {
        layouts,
        effective_layouts: layouts_by_effective_key,
        duplicate_groups,
        normalization_wall: started.elapsed(),
    }
}

pub fn normalize_like_custom(problem: &Problem, graph: &PhysicalGraph) -> (Problem, PhysicalGraph) {
    let Preparation::Prepared(normalized) =
        prepare_problem(problem).expect("profile case must be a valid problem")
    else {
        panic!("profile case must not be globally unsatisfiable");
    };
    let mut graph = graph.clone();
    for link in &mut graph.links {
        if let ProducerPortRef::Input(input) = link.producer {
            let original = usize::try_from(input.0).expect("input index must fit usize");
            let canonical = normalized
                .terminal_mapping
                .normalized_input(original)
                .and_then(|index| u32::try_from(index).ok())
                .expect("every caller input must have a normalized index");
            link.producer = ProducerPortRef::Input(InputTerminalIndex(canonical));
        }
        if let ConsumerPortRef::Output(output) = link.consumer {
            let original = usize::try_from(output.0).expect("output index must fit usize");
            let canonical = normalized
                .terminal_mapping
                .normalized_output(original)
                .and_then(|index| u32::try_from(index).ok())
                .expect("every caller output must have a normalized index");
            link.consumer = ConsumerPortRef::Output(OutputTerminalIndex(canonical));
        }
        link.flow = &link.flow / &normalized.original_scale;
    }
    (
        Problem {
            inputs: normalized.inputs.as_slice().to_vec(),
            outputs: normalized.outputs.as_slice().to_vec(),
            max_link_rate: normalized.max_link_rate,
        },
        graph,
    )
}

fn effective_layout_key(problem: &Problem, graph: &PhysicalGraph) -> CanonicalGraphKey {
    canonicalize_effective_layout(problem, graph)
}

fn print_custom(problem: &Problem, run: &CustomRun) {
    let effective = custom_effective_layouts(problem, run);
    println!();
    println!("CUSTOM");
    println!(
        "status={} wall={:.3}s emitted_exact={} shared_canonical={}",
        run.status,
        run.wall.as_secs_f64(),
        run.layouts.len(),
        effective.len(),
    );
    if let Some(progress) = &run.last_progress {
        println!(
            "last_progress phase={:?} {}",
            progress.phase,
            progress.obligation.as_deref().unwrap_or("")
        );
        for diagnostic in &progress.custom {
            println!("{}={:?}", diagnostic.name, diagnostic.value);
        }
    }
    print_hotspots(&run.hotspots, run.wall);
    print_distribution("Custom layout distribution", effective.into_values());
}

fn custom_effective_layouts(
    problem: &Problem,
    run: &CustomRun,
) -> BTreeMap<CanonicalGraphKey, CrossLayout> {
    let mut effective = BTreeMap::new();
    for solution in run.layouts.values() {
        effective
            .entry(effective_layout_key(problem, &solution.graph))
            .or_insert(CrossLayout {
                link_count: operator_link_count(&solution.graph),
                cyclic_scc_count: solution.validation.cyclic_scc_count,
                peak_rate: operator_peak_rate(&solution.graph),
            });
    }
    effective
}

fn print_z3(run: &Z3Run, cross: &CrossCanonicalZ3) {
    println!();
    println!("Z3");
    println!(
        "status={} wall={:.3}s emitted={} terminal={} shared_canonical={} normalization={:.3}s",
        run.status,
        run.wall.as_secs_f64(),
        run.emitted_count,
        run.terminal_count,
        cross.effective_layouts.len(),
        cross.normalization_wall.as_secs_f64(),
    );
    println!(
        "shared-canonical duplicate_groups={} duplicate_layouts={}",
        cross.duplicate_groups.len(),
        cross
            .duplicate_groups
            .iter()
            .map(|(_, count)| count - 1)
            .sum::<usize>()
    );
    print_distribution(
        "Z3 layout distribution",
        cross.effective_layouts.values().cloned(),
    );
}

fn print_comparison(problem: &Problem, custom: &CustomRun, z3: &CrossCanonicalZ3) {
    let custom_keys = custom.layouts.keys().cloned().collect::<BTreeSet<_>>();
    let z3_keys = z3.layouts.keys().cloned().collect::<BTreeSet<_>>();
    let missing_from_custom = z3_keys.difference(&custom_keys).collect::<Vec<_>>();
    let missing_from_z3 = custom_keys.difference(&z3_keys).collect::<Vec<_>>();
    println!();
    println!("CROSS-ENGINE EXACT COMPARISON");
    println!(
        "custom={} z3_shared_canonical={} common={} missing_from_custom={} missing_from_z3={}",
        custom_keys.len(),
        z3_keys.len(),
        custom_keys.intersection(&z3_keys).count(),
        missing_from_custom.len(),
        missing_from_z3.len(),
    );
    print_key_sample("missing_from_custom", &missing_from_custom);
    print_key_sample("missing_from_z3", &missing_from_z3);
    if missing_from_custom.is_empty() && missing_from_z3.is_empty() {
        println!("verdict=EXACT_SET_MATCH");
    } else {
        println!("verdict=SET_MISMATCH");
    }

    let custom_effective = custom_effective_layouts(problem, custom)
        .into_keys()
        .collect::<BTreeSet<_>>();
    let z3_effective = z3
        .effective_layouts
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    let missing_from_custom = z3_effective.difference(&custom_effective).count();
    let missing_from_z3 = custom_effective.difference(&z3_effective).count();
    println!("CROSS-ENGINE EFFECTIVE-TOPOLOGY COMPARISON");
    println!(
        "custom={} z3={} common={} missing_from_custom={} missing_from_z3={}",
        custom_effective.len(),
        z3_effective.len(),
        custom_effective.intersection(&z3_effective).count(),
        missing_from_custom,
        missing_from_z3,
    );
    println!(
        "verdict={}",
        if missing_from_custom == 0 && missing_from_z3 == 0 {
            "EFFECTIVE_SET_MATCH"
        } else {
            "EFFECTIVE_SET_MISMATCH"
        }
    );
    let peak_differences = custom_effective_layouts(problem, custom)
        .iter()
        .filter_map(|(key, custom_layout)| {
            z3.effective_layouts
                .get(key)
                .map(|z3_layout| (custom_layout, z3_layout))
        })
        .filter(|(custom_layout, z3_layout)| custom_layout.peak_rate != z3_layout.peak_rate)
        .count();
    println!(
        "representative_peak_rate_differences={peak_differences} (cyclic circulation may choose different valid rates)"
    );
}

fn print_distribution(label: &str, layouts: impl IntoIterator<Item = CrossLayout>) {
    let mut distribution = BTreeMap::<(u32, u32), usize>::new();
    for layout in layouts {
        *distribution
            .entry((layout.link_count, layout.cyclic_scc_count))
            .or_default() += 1;
    }
    println!("{label} by (operator_belts, cyclic_sccs): {distribution:?}");
}

fn operator_link_count(graph: &PhysicalGraph) -> u32 {
    u32::try_from(
        graph
            .links
            .iter()
            .filter(|link| {
                matches!(link.producer, ProducerPortRef::Node { .. })
                    && matches!(link.consumer, ConsumerPortRef::Node { .. })
            })
            .count(),
    )
    .expect("profile graph link count must fit u32")
}

fn operator_peak_rate(graph: &PhysicalGraph) -> Rational {
    graph
        .links
        .iter()
        .filter(|link| {
            matches!(link.producer, ProducerPortRef::Node { .. })
                && matches!(link.consumer, ConsumerPortRef::Node { .. })
        })
        .map(|link| link.flow.clone())
        .max()
        .unwrap_or_else(|| Rational::from(0))
}

fn print_key_sample(label: &str, keys: &[&CanonicalGraphKey]) {
    if keys.is_empty() {
        return;
    }
    let sample = keys
        .iter()
        .take(8)
        .map(|key| key_fingerprint(key.as_bytes()))
        .collect::<Vec<_>>();
    println!("{label}_sample={sample:?}");
}

fn key_fingerprint(bytes: &[u8]) -> String {
    let hash = bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3)
    });
    format!("{hash:016x}/{}", bytes.len())
}

fn print_hotspots(hotspots: &HotspotSnapshot, wall: Duration) {
    let mut rows = [
        ("state_canonicalize", hotspots.state_canonicalize_ns),
        ("legal_decisions", hotspots.legal_decisions_ns),
        ("propagation_sync", hotspots.propagation_sync_ns),
        ("evaluate_complete", hotspots.evaluate_complete_ns),
        ("scc_canonicalize", hotspots.scc_canonicalize_ns),
        ("scc_algebra", hotspots.scc_algebra_ns),
        ("reachability", hotspots.reachability_ns),
        ("snapshot", hotspots.snapshot_ns),
    ];
    rows.sort_by_key(|(_, ns)| std::cmp::Reverse(*ns));
    println!(
        "hotspots wall={:.3}s accounted_elapsed={:.3}s",
        wall.as_secs_f64(),
        ns_to_s(hotspots.accounted_ns())
    );
    for (name, ns) in rows.into_iter().filter(|(_, ns)| *ns > 0) {
        println!("  {name:<20} {:>9.3}s", ns_to_s(ns));
    }
    let average_ms = if hotspots.graph_canon_calls == 0 {
        0.0
    } else {
        ns_to_s(hotspots.graph_canon_ns) / hotspots.graph_canon_calls as f64 * 1000.0
    };
    println!(
        "  graph_canon_leaf     {:>9.3}s calls={} avg={average_ms:.3}ms",
        ns_to_s(hotspots.graph_canon_ns),
        hotspots.graph_canon_calls,
    );
    println!("  canonical_subphases_ns={}", hotspot_json(hotspots));
}

pub fn hotspot_json(h: &HotspotSnapshot) -> serde_json::Value {
    serde_json::json!({
        "snapshot_ns": h.snapshot_ns,
        "apply_decision_ns": h.apply_decision_ns,
        "reachability_ns": h.reachability_ns,
        "witness_search_ns": h.witness_search_ns,
        "witness_refine_ns": h.witness_refine_ns,
        "witness_leaf_ns": h.witness_leaf_ns,
        "witness_branches": h.witness_branches,
        "witness_leaves": h.witness_leaves,
        "state_canonicalize_ns": h.state_canonicalize_ns,
        "legal_decisions_ns": h.legal_decisions_ns,
        "propagation_sync_ns": h.propagation_sync_ns,
        "evaluate_complete_ns": h.evaluate_complete_ns,
        "scc_canonicalize_ns": h.scc_canonicalize_ns,
        "scc_algebra_ns": h.scc_algebra_ns,
        "graph_canon_ns": h.graph_canon_ns,
        "graph_canon_calls": h.graph_canon_calls,
        "state_graph_canon_ns": h.state_graph_canon_ns,
        "state_graph_canon_calls": h.state_graph_canon_calls,
        "open_port_graph_canon_ns": h.open_port_graph_canon_ns,
        "open_port_graph_canon_calls": h.open_port_graph_canon_calls,
        "marked_link_graph_canon_ns": h.marked_link_graph_canon_ns,
        "marked_link_graph_canon_calls": h.marked_link_graph_canon_calls,
        "other_graph_canon_ns": h.other_graph_canon_ns,
        "other_graph_canon_calls": h.other_graph_canon_calls,
        "scc_canonicalize_calls": h.scc_canonicalize_calls,
        "incidence_build_ns": h.incidence_build_ns,
        "dense_graph_ns": h.dense_graph_ns,
        "labeling_ns": h.labeling_ns,
        "relabel_ns": h.relabel_ns,
        "equality_ns": h.equality_ns,
        "inequality_ns": h.inequality_ns,
        "semantic_encoding_ns": h.semantic_encoding_ns,
    })
}

fn ns_to_s(ns: u64) -> f64 {
    ns as f64 / 1_000_000_000.0
}

pub fn activity_json(trace: &solver_core::diagnostics::ActivitySnapshot) -> serde_json::Value {
    let record_json = |r: &solver_core::diagnostics::ActivityRecord| {
        serde_json::json!({
            "kind": r.kind, "node_count": r.node_count, "link_count": r.link_count,
            "profile": r.profile, "root": r.root, "worker_budget": r.worker_budget,
            "start_ns": r.start_ns, "end_ns": r.end_ns, "thread": r.thread,
        })
    };
    serde_json::json!({"dropped": trace.dropped,
        "records": trace.records.iter().map(record_json).collect::<Vec<_>>(),
        "active": trace.active.iter().map(record_json).collect::<Vec<_>>(),
    })
}

pub fn write_live_snapshot(
    path: &std::path::Path,
    value: &serde_json::Value,
) -> std::io::Result<()> {
    // Each sample has a unique name. A killed writer leaves only a .tmp file,
    // so an earlier complete diagnostic remains available.
    use std::io::Write as _;
    let pending = path.with_extension("tmp");
    let mut file = std::fs::File::create(&pending)?;
    serde_json::to_writer(&mut file, value)?;
    file.flush()?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(pending, path)
}

pub fn write_heartbeat(
    writer: &mut impl std::io::Write,
    elapsed: Duration,
    cancelled: bool,
    returned: bool,
) -> std::io::Result<()> {
    use std::io::Write as _;
    let counts = solver_core::diagnostics::heartbeat();
    // A separate thread and stack buffer avoid the allocating JSON/trace-lock
    // path. This remains diagnostic information, never a solver result.
    let mut line = std::io::Cursor::new([0_u8; 256]);
    writeln!(
        &mut line,
        "diagnostic_only=true elapsed_ms={} cancel={} solver_returned={} root_searches={} state_cache_drops={} scc_cache_drops={}",
        elapsed.as_millis(),
        cancelled,
        returned,
        counts.root_searches,
        counts.state_cache_drops,
        counts.scc_cache_drops
    )?;
    writer.write_all(&line.get_ref()[..usize::try_from(line.position()).unwrap()])?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_diagnostic_is_separate_from_solver_results_and_keeps_previous_samples() {
        let directory =
            std::env::temp_dir().join(format!("custom-live-test-{}", std::process::id()));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("sample.live-0000.json");
        let value = serde_json::json!({"diagnostic_only": true, "solver_returned": false});
        write_live_snapshot(&path, &value).unwrap();
        write_live_snapshot(&directory.join("sample.live-0001.json"), &value).unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&std::fs::read(path).unwrap()).unwrap(),
            value
        );
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 2);
        std::fs::remove_file(directory.join("sample.live-0000.json")).unwrap();
        std::fs::remove_file(directory.join("sample.live-0001.json")).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn heartbeat_uses_a_bounded_writer_and_distinguishes_cancellation_from_return() {
        let mut output = std::io::Cursor::new([0_u8; 256]);
        write_heartbeat(&mut output, Duration::from_millis(123), true, false).unwrap();
        let text =
            std::str::from_utf8(&output.get_ref()[..usize::try_from(output.position()).unwrap()])
                .unwrap();
        assert!(
            text.contains("diagnostic_only=true elapsed_ms=123 cancel=true solver_returned=false")
        );
        assert!(text.contains("state_cache_drops="));
    }

    fn tiny() -> Problem {
        Problem {
            inputs: vec![2.into(), 3.into()],
            outputs: vec![1.into(), 4.into()],
            max_link_rate: 1200.into(),
        }
    }

    #[test]
    fn file_runner_records_finite_bound_as_incomplete() {
        let run = run_custom(
            &tiny(),
            30,
            2,
            1,
            ParallelismOptions::default(),
            solver_api::SolveMode::Optimal,
            false,
        );
        let Some(solver_api::SolveResult::Incomplete(result)) = run.outcome else {
            panic!("expected finite bound")
        };
        assert_eq!(result.proof.node_counts_exhausted_through, Some(1));
        assert!(matches!(
            result.reason,
            solver_api::IncompleteReason::ResourceLimit { .. }
        ));
        assert!(result.best_known.is_none());
        assert!(run.preferred_key.is_none());
        assert!(!run.deadline_fired);
    }

    #[test]
    fn file_runner_records_both_actual_modes_and_validated_witnesses() {
        for mode in [
            solver_api::SolveMode::Optimal,
            solver_api::SolveMode::AllAtMinimumNodesAndMinimumLinks,
            solver_api::SolveMode::AllAtMinimumNodes,
        ] {
            let problem = tiny();
            let run = run_custom(
                &problem,
                30,
                2,
                2,
                ParallelismOptions::default(),
                mode,
                false,
            );
            assert!(matches!(
                run.outcome,
                Some(solver_api::SolveResult::Optimal(_))
            ));
            assert!(run.first_valid.is_some());
            assert!(!run.layouts.is_empty());
            if mode == solver_api::SolveMode::Optimal {
                assert_eq!(run.layouts.len(), 1);
            }
            for solution in run.layouts.values() {
                validate_solution(&problem, &solution.graph).unwrap();
            }
        }
    }
}
