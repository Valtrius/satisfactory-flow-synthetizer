//! Native/browser qualification harness. Not linked into either application build.
mod primitives;
mod search;
use canonaut::structs::{CanonautManager, DenseGraph};
pub use search::{PortableRun, summarize};
use serde_json::{Value, json};
use solver_api::{ConsumerPortRef, PhysicalGraph, Problem, ProducerPortRef};
use solver_validation::{canonical_layout, validate_solution};
use wasm_bindgen::prelude::*;

/// Evaluate trusted test fixtures, never public imported graphs.
/// # Panics
/// Panics when a fixture is malformed or invalid.
#[must_use]
pub fn identity(input: &Value) -> Value {
    if input["kind"] == "physical" {
        let problem: Problem = serde_json::from_value(input["problem"].clone()).unwrap();
        let graph: PhysicalGraph = serde_json::from_value(input["graph"].clone()).unwrap();
        let validation = validate_solution(&problem, &graph).unwrap();
        let (key, canonical) = canonical_layout(&problem, &graph);
        assert_eq!(validate_solution(&problem, &canonical).unwrap(), validation);
        json!({"key": key, "graph": canonical, "validation": validation})
    } else {
        let colors: Vec<u32> = serde_json::from_value(input["colors"].clone()).unwrap();
        let edges: Vec<(usize, usize)> = serde_json::from_value(input["edges"].clone()).unwrap();
        let mut graph = DenseGraph::new(colors.len());
        graph.set_colors(colors.clone());
        for &(left, right) in &edges {
            graph.add_edge(left, right);
        }
        let mut manager = CanonautManager::new(colors.len()).with_canonization();
        manager.canonize_graph(&graph);
        let mut positions = vec![0; colors.len()];
        let ordered_colors: Vec<_> = manager
            .labeling()
            .iter()
            .enumerate()
            .map(|(index, &original)| {
                positions[original as usize] = index;
                colors[original as usize]
            })
            .collect();
        let mut edges: Vec<_> = edges
            .into_iter()
            .map(|(left, right)| {
                let (left, right) = (positions[left], positions[right]);
                (left.min(right), left.max(right))
            })
            .collect();
        edges.sort_unstable();
        json!({"colors": ordered_colors, "edges": edges})
    }
}

/// Make a storage/ID/port/terminal permutation of a trusted fixture.
/// # Panics
/// Panics for malformed fixture data.
#[must_use]
pub fn permuted(input: &Value, rotation: usize) -> Value {
    let mut input = input.clone();
    if input["kind"] != "physical" {
        let colors: Vec<u32> = serde_json::from_value(input["colors"].clone()).unwrap();
        let mut edges: Vec<(usize, usize)> =
            serde_json::from_value(input["edges"].clone()).unwrap();
        let map = |index| (colors.len() - 1 - index + rotation) % colors.len();
        let mut mapped = colors.clone();
        for (index, &color) in colors.iter().enumerate() {
            mapped[map(index)] = color;
        }
        for (left, right) in &mut edges {
            *left = map(*left);
            *right = map(*right);
        }
        edges.reverse();
        input["colors"] = json!(mapped);
        input["edges"] = json!(edges);
        return input;
    }
    let problem: Problem = serde_json::from_value(input["problem"].clone()).unwrap();
    let mut graph: PhysicalGraph = serde_json::from_value(input["graph"].clone()).unwrap();
    let original = graph.clone();
    let node_map: std::collections::BTreeMap<_, _> = original
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| {
            (
                node.id,
                solver_api::NodeId(
                    u32::try_from(100 + 7 * (original.nodes.len() - index)).unwrap(),
                ),
            )
        })
        .collect();
    let terminal = |index: u32, rates: &[solver_api::Rational]| {
        let peers: Vec<_> = rates
            .iter()
            .enumerate()
            .filter(|(_, rate)| *rate == &rates[index as usize])
            .map(|(i, _)| i)
            .collect();
        let offset = peers.iter().position(|&i| i == index as usize).unwrap();
        u32::try_from(peers[(offset + rotation + 1) % peers.len()]).unwrap()
    };
    let discard_count = original
        .links
        .iter()
        .filter(|link| matches!(link.consumer, ConsumerPortRef::Discard(_)))
        .count();
    for node in &mut graph.nodes {
        node.id = node_map[&node.id];
    }
    graph.nodes.reverse();
    graph.links.reverse();
    for link in &mut graph.links {
        match &mut link.producer {
            ProducerPortRef::Input(id) => id.0 = terminal(id.0, &problem.inputs),
            ProducerPortRef::Node { node, port } => {
                let kind = original
                    .nodes
                    .iter()
                    .find(|item| item.id == *node)
                    .unwrap()
                    .node_type;
                *port = (*port + 1) % kind.output_port_count();
                *node = node_map[node];
            }
        }
        match &mut link.consumer {
            ConsumerPortRef::Output(id) => id.0 = terminal(id.0, &problem.outputs),
            ConsumerPortRef::Discard(id) => {
                id.0 = u32::try_from(discard_count - 1 - id.0 as usize).unwrap();
            }
            ConsumerPortRef::Node { node, port } => {
                let kind = original
                    .nodes
                    .iter()
                    .find(|item| item.id == *node)
                    .unwrap()
                    .node_type;
                *port = (*port + 1) % kind.input_port_count();
                *node = node_map[node];
            }
        }
    }
    input["graph"] = json!(graph);
    input
}

/// Check saved native identities in this target, including equivalent permutations.
/// # Panics
/// Panics on a canonical identity mismatch or malformed trusted fixture.
#[wasm_bindgen]
#[must_use]
pub fn qualify_identity(source: &str) -> String {
    let fixture: Value = serde_json::from_str(source).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    let mut names = Vec::new();
    for case in cases {
        assert_eq!(
            identity(&case["input"]),
            case["expected"],
            "{}",
            case["name"]
        );
        for rotation in [0, 1, 7] {
            assert_eq!(
                identity(&permuted(&case["input"], rotation)),
                case["expected"],
                "{} rotation {rotation}",
                case["name"]
            );
        }
        names.push(case["name"].clone());
    }
    json!({"cases": names, "permutationsPerCase": 3}).to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    fn native_layout_v1_matches_pre_port_golden_keys() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/identity-v1.json");
        let response: serde_json::Value = serde_json::from_str(&super::qualify_identity(
            &std::fs::read_to_string(path).unwrap(),
        ))
        .unwrap();
        assert_eq!(response["cases"].as_array().unwrap().len(), 42);
    }
}
