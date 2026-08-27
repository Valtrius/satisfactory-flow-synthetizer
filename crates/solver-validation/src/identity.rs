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
    CanonicalGraphKey::from_bytes(bytes)
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
