//! Exact colored-incidence identity for public layouts, independent of search engines.

use canonaut::structs::{CanonautManager, DenseGraph};
use solver_api::{CanonicalGraphKey, ConsumerPortRef, PhysicalGraph, Problem, ProducerPortRef};
use std::collections::BTreeMap;

/// Layout identity ignores flow choices, IDs, storage order, and symmetric port numbering.
/// Call only after validating the graph against the problem.
///
/// # Panics
/// Panics for invalid graph references or graphs exceeding u32 vertex indexing.
#[must_use]
pub fn layout_key(problem: &Problem, physical: &PhysicalGraph) -> CanonicalGraphKey {
    canonical_form(problem, physical, false).0
}

/// Deterministic full witness for offline comparisons. Call after exact validation.
/// This uses the same incidence labeling as layout identity; it does not choose an optimal tie.
/// # Panics
/// Panics for invalid graph references or graphs exceeding public index types.
#[must_use]
pub fn canonical_layout(
    problem: &Problem,
    physical: &PhysicalGraph,
) -> (CanonicalGraphKey, PhysicalGraph) {
    let (key, graph) = canonical_form(problem, physical, true);
    (key, graph.unwrap())
}

fn canonical_form(
    problem: &Problem,
    physical: &PhysicalGraph,
    materialize: bool,
) -> (CanonicalGraphKey, Option<PhysicalGraph>) {
    let mut colors = Vec::<String>::new();
    let mut edges = Vec::<(usize, usize)>::new();
    let mut vertex = |color: String| {
        let id = colors.len();
        colors.push(color);
        id
    };
    let inputs = problem
        .inputs
        .iter()
        .map(|rate| vertex(format!("input:{rate}")))
        .collect::<Vec<_>>();
    let outputs = problem
        .outputs
        .iter()
        .map(|rate| vertex(format!("output:{rate}")))
        .collect::<Vec<_>>();
    let nodes = physical
        .nodes
        .iter()
        .map(|node| (node.id, vertex(format!("operator:{:?}", node.node_type))))
        .collect::<BTreeMap<_, _>>();
    let mut link_vertices = Vec::new();
    for link in &physical.links {
        let source = match link.producer {
            ProducerPortRef::Input(index) => inputs[index.0 as usize],
            ProducerPortRef::Node { node, .. } => nodes[&node],
        };
        let target = match link.consumer {
            ConsumerPortRef::Output(index) => outputs[index.0 as usize],
            ConsumerPortRef::Node { node, .. } => nodes[&node],
            ConsumerPortRef::Discard(_) => vertex("discard".to_owned()),
        };
        // Different port colors preserve edge direction in the undirected incidence graph.
        // Effective splitter outputs and merger inputs are interchangeable.
        let producer = vertex("producer-port".to_owned());
        let consumer = vertex("consumer-port".to_owned());
        edges.extend([(source, producer), (producer, consumer), (consumer, target)]);
        link_vertices.push((producer, consumer, target));
    }
    let mut palette = colors.clone();
    palette.sort();
    palette.dedup();
    let color_ids = colors
        .iter()
        .map(|color| u32::try_from(palette.binary_search(color).unwrap()).unwrap())
        .collect::<Vec<_>>();
    let mut graph = DenseGraph::new(colors.len());
    for &(left, right) in &edges {
        graph.add_edge(left, right);
    }
    graph.set_colors(color_ids.clone());
    let mut manager = CanonautManager::new(colors.len()).with_canonization();
    manager.canonize_graph(&graph);
    let mut positions = vec![0; colors.len()];
    let mut bytes = b"layout-v1".to_vec();
    bytes.extend_from_slice(&u64::try_from(palette.len()).unwrap().to_le_bytes());
    for color in &palette {
        bytes.extend_from_slice(&u64::try_from(color.len()).unwrap().to_le_bytes());
        bytes.extend_from_slice(color.as_bytes());
    }
    bytes.extend_from_slice(&u64::try_from(colors.len()).unwrap().to_le_bytes());
    for (position, &original) in manager.labeling().iter().enumerate() {
        positions[original as usize] = position;
        bytes.extend_from_slice(&color_ids[original as usize].to_le_bytes());
    }
    let mut canonical_edges = edges
        .into_iter()
        .map(|(a, b)| {
            let (a, b) = (positions[a], positions[b]);
            (a.min(b), a.max(b))
        })
        .collect::<Vec<_>>();
    canonical_edges.sort_unstable();
    for (a, b) in canonical_edges {
        bytes.extend_from_slice(&u64::try_from(a).unwrap().to_le_bytes());
        bytes.extend_from_slice(&u64::try_from(b).unwrap().to_le_bytes());
    }
    let canonical = materialize.then(|| {
        relabel(
            physical,
            &inputs,
            &outputs,
            &nodes,
            &link_vertices,
            &positions,
            problem,
        )
    });
    (CanonicalGraphKey::from_bytes(bytes), canonical)
}

fn relabel(
    physical: &PhysicalGraph,
    inputs: &[usize],
    outputs: &[usize],
    nodes: &BTreeMap<solver_api::NodeId, usize>,
    link_vertices: &[(usize, usize, usize)],
    positions: &[usize],
    problem: &Problem,
) -> PhysicalGraph {
    use solver_api::{DiscardTerminalIndex, InputTerminalIndex, NodeId, OutputTerminalIndex};
    let terminal_map = |vertices: &[usize], rates: &[solver_api::Rational]| {
        let mut groups = BTreeMap::<_, Vec<usize>>::new();
        for (index, rate) in rates.iter().enumerate() {
            groups.entry(rate).or_default().push(index);
        }
        let mut map = vec![0u32; vertices.len()];
        for indices in groups.values() {
            let mut ordered = indices.clone();
            ordered.sort_by_key(|&i| positions[vertices[i]]);
            for (&original, &target) in ordered.iter().zip(indices) {
                map[original] = u32::try_from(target).unwrap();
            }
        }
        map
    };
    let input_map = terminal_map(inputs, &problem.inputs);
    let output_map = terminal_map(outputs, &problem.outputs);
    let mut graph = physical.clone();
    graph.nodes.sort_by_key(|node| positions[nodes[&node.id]]);
    let node_map = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id, NodeId(u32::try_from(i).unwrap())))
        .collect::<BTreeMap<_, _>>();
    for node in &mut graph.nodes {
        node.id = node_map[&node.id];
    }
    let mut order = (0..physical.links.len()).collect::<Vec<_>>();
    order.sort_by_key(|&i| positions[link_vertices[i].0]);
    let mut producer_ports = BTreeMap::<NodeId, u8>::new();
    let mut consumer_ports = BTreeMap::<NodeId, u8>::new();
    let mut discards = 0u32;
    graph.links = order
        .iter()
        .map(|&i| {
            let mut link = physical.links[i].clone();
            link.producer = match link.producer {
                ProducerPortRef::Input(id) => {
                    ProducerPortRef::Input(InputTerminalIndex(input_map[id.0 as usize]))
                }
                ProducerPortRef::Node { node, .. } => {
                    let node = node_map[&node];
                    let port = producer_ports.entry(node).or_default();
                    let reference = ProducerPortRef::Node { node, port: *port };
                    *port += 1;
                    reference
                }
            };
            link.consumer = match link.consumer {
                ConsumerPortRef::Output(id) => {
                    ConsumerPortRef::Output(OutputTerminalIndex(output_map[id.0 as usize]))
                }
                ConsumerPortRef::Discard(_) => {
                    let id = discards;
                    discards += 1;
                    ConsumerPortRef::Discard(DiscardTerminalIndex(id))
                }
                ConsumerPortRef::Node { node, .. } => {
                    let node = node_map[&node];
                    let port = consumer_ports.entry(node).or_default();
                    let reference = ConsumerPortRef::Node { node, port: *port };
                    *port += 1;
                    reference
                }
            };
            link
        })
        .collect();
    graph
}

/// Normalize public identities after a solver has finished its independent search.
/// Search engines may use different private canonical encodings internally.
///
/// # Panics
/// Panics if a returned witness has invalid graph references.
pub fn normalize_outcome_identity(problem: &Problem, outcome: &mut solver_api::SolveOutcome) {
    match &mut outcome.result {
        solver_api::SolveResult::Optimal(solution) => {
            solution.canonical_graph_key = layout_key(problem, &solution.graph);
        }
        solver_api::SolveResult::Incomplete(incomplete) => {
            if let Some(best) = &mut incomplete.best_known {
                best.canonical_graph_key = layout_key(problem, &best.graph);
            }
        }
        solver_api::SolveResult::GloballyUnsat(_) => {}
    }
    for solution in &mut outcome.solutions {
        solution.canonical_graph_key = layout_key(problem, &solution.graph);
    }
}
