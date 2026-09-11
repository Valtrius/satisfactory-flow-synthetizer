//! Browser-safe exact witness and selected-solution verification. This is not a search engine.
//!
//! Both entry points return JSON and run without native threads, cvc5, or canonicalization.
//! The caller must run them in a terminable worker, including for imported data.

use serde::{Deserialize, Serialize};
use solver_api::{
    BestKnownSolution, ConsumerPortRef, PhysicalGraph, PhysicalLink, PhysicalNode, ProblemRequest,
    ProducerPortRef, Rational,
};
use synthetizer_app::presentation::{PresentationSolution, present_best_known_solution};

mod share;
pub use share::{share_from_presentation_json, verify_share_json};

const MAX_PAYLOAD_BYTES: usize = 262_144;
const MAX_NODES: usize = 256;
const MAX_LINKS: usize = 1_024;
const MAX_RATE_BYTES: usize = 256;
const MAX_ID_BYTES: usize = 128;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    request: ProblemRequest,
    graph: Graph,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Graph {
    nodes: Vec<PhysicalNode>,
    links: Vec<Link>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Link {
    producer: ProducerPortRef,
    consumer: ConsumerPortRef,
    // Keep strings until size checks have completed, before parsing big integers.
    flow: Option<String>,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Response {
    Verified {
        graph: PhysicalGraph,
        solution: Box<PresentationSolution>,
    },
    Rejected {
        error: String,
    },
}

/// Version of this verification bridge, independent of a future public share format.
#[must_use]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn verifier_version() -> u32 {
    1
}

/// Verify every supplied exact flow and rebuild display data without trusting proof metadata.
#[must_use]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn verify_witness_json(payload: &str) -> String {
    respond(payload, false)
}

/// Recover exact flows from a topology whose links must omit `flow`.
///
/// A separate entry point prevents a bad supplied witness from silently being repaired.
#[must_use]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn reconstruct_topology_json(payload: &str) -> String {
    respond(payload, true)
}

fn respond(payload: &str, reconstruct: bool) -> String {
    let response =
        verify(payload, reconstruct).unwrap_or_else(|error| Response::Rejected { error });
    // All response values have supported JSON representations, with exact rates as strings.
    serde_json::to_string(&response).expect("verification response is JSON serializable")
}

fn verify(payload: &str, reconstruct: bool) -> Result<Response, String> {
    if payload.len() > MAX_PAYLOAD_BYTES {
        return Err("verification payload exceeds 256 KiB".into());
    }
    let request: Request = serde_json::from_str(payload).map_err(|e| e.to_string())?;
    check_bounds(&request)?;
    let prepared = request.request.prepare().map_err(|e| e.to_string())?;
    let links = request
        .graph
        .links
        .into_iter()
        .map(|link| {
            let flow = match (reconstruct, link.flow) {
                (true, None) => Rational::zero(),
                (true, Some(_)) => return Err("topology links must omit flow".into()),
                (false, None) => return Err("witness links must include exact flow".into()),
                (false, Some(flow)) => flow.parse::<Rational>().map_err(|e| e.to_string())?,
            };
            Ok(PhysicalLink {
                producer: link.producer,
                consumer: link.consumer,
                flow,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut graph = PhysicalGraph {
        nodes: request.graph.nodes,
        links,
    };
    if reconstruct {
        graph = solver_validation::solve_topology(&prepared.problem, &graph)
            .map_err(|e| e.to_string())?;
    }
    let validation = solver_validation::validate_solution(&prepared.problem, &graph)
        .map_err(|e| e.to_string())?;
    let best = BestKnownSolution {
        node_count: validation.node_count,
        link_count: validation.link_count,
        physical_link_count: validation.physical_link_count,
        discard_link_count: validation.discard_link_count,
        // Presentation does not use or emit this field. No identity is asserted by this bridge.
        canonical_graph_key: solver_api::CanonicalGraphKey::default(),
        graph,
        validation,
    };
    let solution =
        present_best_known_solution(&prepared, &best, None).map_err(|e| e.to_string())?;
    Ok(Response::Verified {
        graph: best.graph,
        solution: Box::new(solution),
    })
}

fn check_bounds(value: &Request) -> Result<(), String> {
    if value.graph.nodes.len() > MAX_NODES || value.graph.links.len() > MAX_LINKS {
        return Err("verification graph exceeds 256 operators or 1024 links".into());
    }
    check_rate(&value.request.belt_rate)?;
    for endpoints in [&value.request.inputs, &value.request.outputs] {
        if endpoints.len() > 24 {
            return Err("at most 24 inputs and outputs are allowed".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        for endpoint in endpoints {
            check_rate(&endpoint.rate)?;
            if endpoint.id.is_empty()
                || endpoint.id.len() > MAX_ID_BYTES
                || !ids.insert(&endpoint.id)
            {
                return Err(
                    "endpoint IDs must be nonempty, unique per side and at most 128 bytes".into(),
                );
            }
            // The shared preparer checks the 80-character limit before display construction.
            if endpoint.name.len() > 320 {
                return Err("endpoint name is too long".into());
            }
        }
    }
    for link in &value.graph.links {
        if let Some(flow) = &link.flow {
            check_rate(flow)?;
        }
    }
    Ok(())
}

fn check_rate(value: &str) -> Result<(), String> {
    if value.len() > MAX_RATE_BYTES {
        Err("rate literal exceeds 256 bytes".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn direct() -> Value {
        json!({
            "request": {"inputs": [], "outputs": [{"id": "o", "name": "", "rate": "1/3"}], "beltRate": "1"},
            "graph": {"nodes": [], "links": [{"producer": {"owner": "input", "port": 0}, "consumer": {"owner": "output", "port": 0}, "flow": "1/3"}]}
        })
    }

    #[test]
    fn verification_is_exact_and_does_not_claim_optimality() {
        let result: Value =
            serde_json::from_str(&verify_witness_json(&direct().to_string())).unwrap();
        assert_eq!(result["kind"], "verified");
        assert_eq!(result["graph"]["links"][0]["flow"], "1/3");
        assert_eq!(result["solution"]["status"], "best_known");
        assert!(result["solution"]["proof"].is_null());
        assert_eq!(result["solution"]["totalInput"]["exact"], "1/3");
    }

    #[test]
    fn reconstruction_and_supplied_witnesses_have_distinct_contracts() {
        let mut input = direct();
        input["graph"]["links"][0]["flow"] = json!("2/3");
        assert!(verify_witness_json(&input.to_string()).contains("rejected"));
        assert!(reconstruct_topology_json(&input.to_string()).contains("must omit flow"));
        input["graph"]["links"][0]
            .as_object_mut()
            .unwrap()
            .remove("flow");
        assert!(verify_witness_json(&input.to_string()).contains("must include exact flow"));
        let result: Value =
            serde_json::from_str(&reconstruct_topology_json(&input.to_string())).unwrap();
        assert_eq!(result["graph"]["links"][0]["flow"], "1/3");
    }

    #[test]
    fn rejects_invalid_json_claimed_proofs_and_oversized_numbers() {
        assert!(verify_witness_json("[").contains("rejected"));
        let mut input = direct();
        input["proof"] = json!({"minimumNodeCount": 0});
        assert!(verify_witness_json(&input.to_string()).contains("unknown field"));
        input.as_object_mut().unwrap().remove("proof");
        input["request"]["outputs"][0]["rate"] = json!("9".repeat(257));
        assert!(verify_witness_json(&input.to_string()).contains("rate literal exceeds"));
        assert!(
            verify_witness_json(&" ".repeat(MAX_PAYLOAD_BYTES + 1)).contains("exceeds 256 KiB")
        );
    }
}
