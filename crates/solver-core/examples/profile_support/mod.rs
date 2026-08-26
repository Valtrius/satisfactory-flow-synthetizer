//! Shared dual-engine profiling and exact cross-engine layout comparison.

#![allow(clippy::cast_precision_loss, clippy::too_many_lines)]

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use solver_api::{
    BestKnownSolution, CanonicalGraphKey, ConsumerPortRef, DiscardTerminalIndex,
    InputTerminalIndex, NodeId, NodeType, OutputTerminalIndex, PhysicalGraph, PhysicalLink,
    PhysicalNode, Problem, ProducerPortRef, Rational, SearchInstrumentation, SolvePhase,
    SolverEvent,
};
use solver_core::{
    Preparation, SolveOptions,
    canonical::{canonicalize_effective_layout, canonicalize_witness},
    enumerate_with_observer,
    hotspot_profile::{self, HotspotSnapshot},
    prepare_problem,
};
use solver_validation::validate_solution;
use solver_z3::{
    GraphNode as Z3GraphNode, NodeKind as Z3NodeKind, Solution as Z3Solution,
    SolveRequest as Z3SolveRequest, SolveTermination as Z3Termination,
    SolverEvent as Z3SolverEvent,
};

#[derive(Clone, Copy)]
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
    status: String,
    layouts: BTreeMap<CanonicalGraphKey, BestKnownSolution>,
    last_progress: Option<ProgressLine>,
    hotspots: HotspotSnapshot,
}

struct Z3Run {
    wall: Duration,
    status: String,
    emitted_count: usize,
    terminal_count: usize,
    layouts: Vec<Z3Solution>,
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
    instrumentation: SearchInstrumentation,
}

pub fn run(case: ProfileCase) {
    let seconds = argument(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(case.default_timeout_seconds);
    let workers = argument(2)
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(available_workers);
    let max_nodes = argument(3)
        .and_then(|value| value.parse().ok())
        .unwrap_or(case.default_max_nodes);
    let engine_argument = argument(4);
    let engines = EngineSelection::parse(engine_argument.as_deref());
    let problem = api_problem(&case);

    println!("dual-engine profile: {} @{}", case.name, case.belt_rate);
    println!(
        "timeout={seconds}s custom_workers={workers} custom_max_nodes={max_nodes} engines={engines:?}"
    );
    println!("Z3 manages its own portfolio from the machine's available parallelism.");

    let custom = engines
        .includes_custom()
        .then(|| run_custom(&problem, seconds, workers, max_nodes));
    let z3 = engines.includes_z3().then(|| run_z3(&case, seconds));

    if let Some(custom) = &custom {
        print_custom(&problem, custom);
    }
    let cross_z3 = z3.as_ref().map(|run| canonicalize_z3(&problem, run));
    if let (Some(z3), Some(cross)) = (&z3, &cross_z3) {
        print_z3(z3, cross);
    }
    if let (Some(custom), Some(z3)) = (&custom, &cross_z3) {
        print_comparison(&problem, custom, z3);
    }
}

fn argument(index: usize) -> Option<String> {
    std::env::args().nth(index)
}

fn available_workers() -> usize {
    thread::available_parallelism().map_or(1, std::num::NonZero::get)
}

fn api_problem(case: &ProfileCase) -> Problem {
    Problem {
        inputs: case.inputs.iter().copied().map(Rational::from).collect(),
        outputs: case.outputs.iter().copied().map(Rational::from).collect(),
        max_link_rate: Rational::from(case.belt_rate),
    }
}

fn run_custom(problem: &Problem, seconds: u64, workers: usize, max_nodes: u32) -> CustomRun {
    let options = SolveOptions {
        max_nodes: Some(max_nodes),
        worker_count: workers,
    };
    let cancel = Arc::new(AtomicBool::new(false));
    let last_progress = Arc::new(Mutex::new(None::<ProgressLine>));
    let layouts = Arc::new(Mutex::new(
        BTreeMap::<CanonicalGraphKey, BestKnownSolution>::new(),
    ));
    let progress_slot = Arc::clone(&last_progress);
    let layout_slot = Arc::clone(&layouts);
    let observer = move |event: SolverEvent| match event {
        SolverEvent::Progress(progress) => {
            let obligation = progress.obligation.as_ref().map(|obligation| {
                format!(
                    "N={} L={:?} profile={:?} root={:?} profiles={}/{}",
                    obligation.node_count,
                    obligation.link_count,
                    obligation.profile,
                    obligation.root_partition,
                    progress.completed_profiles,
                    progress
                        .total_profiles
                        .map_or_else(|| "?".to_owned(), |total| total.to_string())
                )
            });
            *progress_slot.lock().expect("progress lock") = Some(ProgressLine {
                phase: progress.phase,
                obligation,
                instrumentation: progress.instrumentation,
            });
        }
        SolverEvent::SolutionFound(solution) => {
            layout_slot
                .lock()
                .expect("layout lock")
                .insert(solution.canonical_graph_key.clone(), solution);
        }
        SolverEvent::Incumbent(_) => {}
    };

    hotspot_profile::install_recorder();
    let (finished_tx, finished_rx) = mpsc::channel();
    let cancel_for_timer = Arc::clone(&cancel);
    let timer = thread::spawn(move || {
        if finished_rx
            .recv_timeout(Duration::from_secs(seconds))
            .is_err()
        {
            cancel_for_timer.store(true, Ordering::Relaxed);
            let live = hotspot_profile::peek_snapshot();
            eprintln!(
                "Custom timer fired: graph_canons={} state_canon_cpu={:.1}s complete_cpu={:.1}s",
                live.graph_canon_calls,
                ns_to_s(live.state_canonicalize_ns),
                ns_to_s(live.evaluate_complete_ns),
            );
        }
    });
    let started = Instant::now();
    let result = enumerate_with_observer(problem, &options, &cancel, &observer);
    let wall = started.elapsed();
    drop(observer);
    let _ = finished_tx.send(());
    let _ = timer.join();
    let hotspots = hotspot_profile::take_snapshot();
    let status = match result {
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
        status,
        layouts: Arc::try_unwrap(layouts)
            .expect("custom layout observer retained")
            .into_inner()
            .expect("layout lock"),
        last_progress: Arc::try_unwrap(last_progress)
            .expect("custom progress observer retained")
            .into_inner()
            .expect("progress lock"),
        hotspots,
    }
}

fn run_z3(case: &ProfileCase, seconds: u64) -> Z3Run {
    let request = Z3SolveRequest {
        inputs: case
            .inputs
            .iter()
            .enumerate()
            .map(|(index, rate)| solver_z3::EndpointRequest {
                id: format!("input-{index}"),
                name: format!("Input {}", index + 1),
                rate: rate.to_string(),
            })
            .collect(),
        outputs: case
            .outputs
            .iter()
            .enumerate()
            .map(|(index, rate)| solver_z3::EndpointRequest {
                id: format!("output-{index}"),
                name: format!("Output {}", index + 1),
                rate: rate.to_string(),
            })
            .collect(),
        belt_rate: case.belt_rate.to_string(),
        enumerate_all_at_n: true,
    };
    let cancel = Arc::new(AtomicBool::new(false));
    let emitted = Arc::new(Mutex::new(Vec::<Z3Solution>::new()));
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
    let result = solver_z3::solve_exact(&request, &cancel, move |event| {
        if let Z3SolverEvent::SolutionFound(solution) = event {
            emitted_slot
                .lock()
                .expect("Z3 emitted layout lock")
                .push(*solution);
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
        Ok(Z3Termination::Enumerated(solutions)) => {
            (format!("Enumerated({})", solutions.len()), solutions)
        }
        Ok(Z3Termination::Incomplete { solutions, error }) => (
            format!("Incomplete({}, {error})", solutions.len()),
            solutions,
        ),
        Ok(Z3Termination::Cancelled { solutions }) => {
            (format!("Cancelled({})", solutions.len()), solutions)
        }
        Ok(Z3Termination::Completed(solution)) => ("Completed(1)".to_owned(), vec![*solution]),
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
        let graph = z3_physical_graph(solution).unwrap_or_else(|error| {
            panic!("could not convert Z3 layout to the shared physical model: {error}")
        });
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

fn normalize_like_custom(problem: &Problem, graph: &PhysicalGraph) -> (Problem, PhysicalGraph) {
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

fn z3_physical_graph(solution: &Z3Solution) -> Result<PhysicalGraph, String> {
    let nodes_by_id = solution
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<HashMap<_, _>>();
    let mut operator_nodes = solution
        .nodes
        .iter()
        .filter_map(|node| z3_node_type(node).map(|node_type| (node.id.as_str(), node_type)))
        .collect::<Vec<_>>();
    operator_nodes.sort_by_key(|(id, _)| operator_suffix(id).unwrap_or(usize::MAX));
    let operator_ids = operator_nodes
        .iter()
        .enumerate()
        .map(|(index, (id, _))| {
            u32::try_from(index)
                .map(NodeId)
                .map(|node_id| ((*id).to_owned(), node_id))
                .map_err(|_| "operator count does not fit u32".to_owned())
        })
        .collect::<Result<HashMap<_, _>, _>>()?;
    let nodes = operator_nodes
        .into_iter()
        .map(|(id, node_type)| PhysicalNode {
            id: operator_ids[id],
            node_type,
        })
        .collect::<Vec<_>>();
    let discard_ids = solution
        .nodes
        .iter()
        .filter(|node| node.kind == Z3NodeKind::Discard)
        .enumerate()
        .map(|(index, node)| {
            u32::try_from(index)
                .map(DiscardTerminalIndex)
                .map(|discard| (node.id.as_str(), discard))
                .map_err(|_| "discard count does not fit u32".to_owned())
        })
        .collect::<Result<HashMap<_, _>, _>>()?;
    let links = solution
        .edges
        .iter()
        .map(|edge| {
            let source = nodes_by_id
                .get(edge.source.as_str())
                .ok_or_else(|| format!("unknown source {}", edge.source))?;
            let target = nodes_by_id
                .get(edge.target.as_str())
                .ok_or_else(|| format!("unknown target {}", edge.target))?;
            Ok(PhysicalLink {
                producer: z3_producer(source, edge.source_port, &operator_ids)?,
                consumer: z3_consumer(target, edge.target_port, &operator_ids, &discard_ids)?,
                flow: edge
                    .rate
                    .exact
                    .parse()
                    .map_err(|error| format!("invalid exact edge rate: {error}"))?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(PhysicalGraph { nodes, links })
}

fn z3_node_type(node: &Z3GraphNode) -> Option<NodeType> {
    match node.kind {
        Z3NodeKind::Splitter2 => Some(NodeType::Splitter2),
        Z3NodeKind::Splitter3 => Some(NodeType::Splitter3),
        Z3NodeKind::Merger2 => Some(NodeType::Merger2),
        Z3NodeKind::Merger3 => Some(NodeType::Merger3),
        Z3NodeKind::Input | Z3NodeKind::Output | Z3NodeKind::Discard => None,
    }
}

fn z3_producer(
    node: &Z3GraphNode,
    port: usize,
    operators: &HashMap<String, NodeId>,
) -> Result<ProducerPortRef, String> {
    match node.kind {
        Z3NodeKind::Input => terminal_suffix(&node.id, "input-")
            .and_then(|index| u32::try_from(index).ok())
            .map(InputTerminalIndex)
            .map(ProducerPortRef::Input)
            .ok_or_else(|| format!("invalid input node id {}", node.id)),
        Z3NodeKind::Splitter2
        | Z3NodeKind::Splitter3
        | Z3NodeKind::Merger2
        | Z3NodeKind::Merger3 => Ok(ProducerPortRef::Node {
            node: *operators
                .get(&node.id)
                .ok_or_else(|| format!("unknown operator {}", node.id))?,
            port: u8::try_from(port).map_err(|_| "producer port does not fit u8".to_owned())?,
        }),
        Z3NodeKind::Output | Z3NodeKind::Discard => {
            Err(format!("{} cannot produce a physical link", node.id))
        }
    }
}

fn z3_consumer(
    node: &Z3GraphNode,
    port: usize,
    operators: &HashMap<String, NodeId>,
    discards: &HashMap<&str, DiscardTerminalIndex>,
) -> Result<ConsumerPortRef, String> {
    match node.kind {
        Z3NodeKind::Output => terminal_suffix(&node.id, "output-")
            .and_then(|index| u32::try_from(index).ok())
            .map(OutputTerminalIndex)
            .map(ConsumerPortRef::Output)
            .ok_or_else(|| format!("invalid output node id {}", node.id)),
        Z3NodeKind::Discard => discards
            .get(node.id.as_str())
            .copied()
            .map(ConsumerPortRef::Discard)
            .ok_or_else(|| format!("unknown discard {}", node.id)),
        Z3NodeKind::Splitter2
        | Z3NodeKind::Splitter3
        | Z3NodeKind::Merger2
        | Z3NodeKind::Merger3 => Ok(ConsumerPortRef::Node {
            node: *operators
                .get(&node.id)
                .ok_or_else(|| format!("unknown operator {}", node.id))?,
            port: u8::try_from(port).map_err(|_| "consumer port does not fit u8".to_owned())?,
        }),
        Z3NodeKind::Input => Err(format!("{} cannot consume a physical link", node.id)),
    }
}

fn terminal_suffix(id: &str, prefix: &str) -> Option<usize> {
    id.strip_prefix(prefix)?.parse().ok()
}

fn operator_suffix(id: &str) -> Option<usize> {
    terminal_suffix(id, "operator-")
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
        let stats = &progress.instrumentation;
        println!(
            "work decisions={} states={} dupes={} prop={} capacity={} lb={} scc={}/{} peak_mem={}",
            stats.raw_structural_decisions,
            stats.canonical_states_retained,
            stats.canonical_duplicates_eliminated,
            stats.propagation_contradictions,
            stats.capacity_prunes,
            stats.lower_bound_prunes,
            stats.scc_solves,
            stats.scc_cache_hits,
            stats.peak_memory_bytes,
        );
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
        "hotspots wall={:.3}s accounted_worker_cpu={:.3}s",
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
}

fn ns_to_s(ns: u64) -> f64 {
    ns as f64 / 1_000_000_000.0
}
