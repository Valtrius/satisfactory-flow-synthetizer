//! Public selected-solution schema. No proof, search mode, history or coordinates cross this boundary.
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use solver_api::NodeType;
use synthetizer_app::presentation::{GraphNodeKind, PresentationSolution};

use super::{MAX_LINKS, MAX_NODES, MAX_PAYLOAD_BYTES, Response};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Share {
    kind: ShareKind,
    version: u32,
    request: ShareRequest,
    topology: Topology,
}

#[derive(Deserialize, Serialize)]
enum ShareKind {
    #[serde(rename = "selected-solution")]
    SelectedSolution,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ShareRequest {
    inputs: Vec<Endpoint>,
    outputs: Vec<Endpoint>,
    belt_rate: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Endpoint {
    name: String,
    rate: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Topology {
    /// Array position is the graph-local node ID.
    nodes: Vec<NodeType>,
    links: Vec<Link>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Link {
    producer: Producer,
    consumer: Consumer,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NodePort {
    node: u32,
    port: u8,
}

#[derive(Deserialize, Serialize)]
#[serde(
    tag = "owner",
    content = "port",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum Producer {
    Input(u32),
    Node(NodePort),
}

#[derive(Deserialize, Serialize)]
#[serde(
    tag = "owner",
    content = "port",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum Consumer {
    Output(u32),
    Discard(u32),
    Node(NodePort),
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
enum ShareResponse {
    VerifiedShare {
        share: Share,
        solution: Box<PresentationSolution>,
    },
    Rejected {
        error: String,
    },
}

fn bounded<T: serde::de::DeserializeOwned>(payload: &str) -> Result<T, String> {
    if payload.len() > MAX_PAYLOAD_BYTES {
        return Err("selected solution exceeds 256 KiB".into());
    }
    serde_json::from_str(payload).map_err(|error| error.to_string())
}

fn respond(result: Result<ShareResponse, String>) -> String {
    serde_json::to_string(&result.unwrap_or_else(|error| ShareResponse::Rejected { error }))
        .expect("share response is JSON serializable")
}

/// Reconstruct and verify one bounded public topology without trusting any proof metadata.
#[must_use]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn verify_share_json(payload: &str) -> String {
    respond(bounded::<Share>(payload).and_then(verify_share))
}

impl ShareRequest {
    fn bridge(&self) -> serde_json::Value {
        let endpoints = |values: &[Endpoint], side: &str| {
            values
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    serde_json::json!({
                        "id": format!("{side}-{index}"), "name": value.name, "rate": value.rate
                    })
                })
                .collect::<Vec<_>>()
        };
        serde_json::json!({
            "inputs": endpoints(&self.inputs, "input"),
            "outputs": endpoints(&self.outputs, "output"), "beltRate": self.belt_rate
        })
    }
}

fn bridge(share: &Share, flows: Option<&[String]>) -> Result<String, String> {
    if share.version != 1 {
        return Err("unsupported selected-solution version".into());
    }
    if share.topology.nodes.len() > MAX_NODES || share.topology.links.len() > MAX_LINKS {
        return Err("selected solution exceeds 256 operators or 1024 links".into());
    }
    let nodes = share
        .topology
        .nodes
        .iter()
        .enumerate()
        .map(|(id, node_type)| serde_json::json!({"id": id, "nodeType": node_type}))
        .collect::<Vec<_>>();
    let links = share
        .topology
        .links
        .iter()
        .enumerate()
        .map(|(index, link)| {
            let mut value = serde_json::to_value(link).expect("link is JSON serializable");
            if let Some(flows) = flows {
                value["flow"] = serde_json::json!(flows[index]);
            }
            value
        })
        .collect::<Vec<_>>();
    Ok(serde_json::json!({"request": share.request.bridge(), "graph": {"nodes": nodes, "links": links}}).to_string())
}

fn verify_share(share: Share) -> Result<ShareResponse, String> {
    let payload = bridge(&share, None)?;
    match super::verify(&payload, true)? {
        Response::Verified { solution, .. } => Ok(ShareResponse::VerifiedShare { share, solution }),
        Response::Rejected { error } => Err(error),
    }
}

// This is an explicit migration input, not a second public share format. Derived display
// fields, including proof/status/labels, are ignored and regenerated after verification.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PresentationInput {
    request: ShareRequest,
    solution: DisplayGraph,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DisplayGraph {
    model_version: u32,
    nodes: Vec<DisplayNode>,
    edges: Vec<DisplayEdge>,
}

#[derive(Deserialize)]
struct DisplayNode {
    id: String,
    kind: GraphNodeKind,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DisplayEdge {
    id: String,
    source: String,
    target: String,
    source_port: u8,
    target_port: u8,
    rate: ExactRate,
}

#[derive(Deserialize)]
struct ExactRate {
    exact: String,
}

#[derive(Clone, Copy)]
enum Reference {
    Input(u32),
    Output(u32),
    Discard(u32),
    Node(u32),
}

fn index(id: &str, prefix: &str, count: usize) -> Result<u32, String> {
    let value = id
        .strip_prefix(prefix)
        .and_then(|value| value.parse::<u32>().ok())
        .ok_or_else(|| format!("unsupported presentation terminal ID: {id}"))?;
    if value as usize >= count || id != format!("{prefix}{value}") {
        return Err(format!(
            "ambiguous or out-of-range presentation terminal: {id}"
        ));
    }
    Ok(value)
}

/// Export a selected model-v4 presentation only after verifying its claimed exact edge flows.
/// Unknown legacy conventions fail rather than guessing the terminal mapping.
#[must_use]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub fn share_from_presentation_json(payload: &str) -> String {
    respond(bounded::<PresentationInput>(payload).and_then(from_presentation))
}

fn check_presentation(display: &DisplayGraph) -> Result<(), String> {
    if display.model_version != 4 {
        return Err("only modelVersion 4 presentation graphs can be shared; older or unknown terminal mappings are not guessed".into());
    }
    if display.edges.len() > MAX_LINKS || display.nodes.len() > MAX_NODES + MAX_LINKS + 48 {
        return Err("presentation graph exceeds sharing limits".into());
    }
    Ok(())
}

fn from_presentation(input: PresentationInput) -> Result<ShareResponse, String> {
    let display = input.solution;
    check_presentation(&display)?;
    let mut references = BTreeMap::new();
    let mut nodes = Vec::new();
    let mut input_count = 0;
    let mut output_count = 0;
    let mut discard_count = 0_u32;
    for node in &display.nodes {
        if node.id.len() > 128 {
            return Err("presentation node ID is too long".into());
        }
        let reference = match node.kind {
            GraphNodeKind::Input => {
                input_count += 1;
                Reference::Input(index(
                    &node.id,
                    "input-",
                    input.request.inputs.len().max(1),
                )?)
            }
            GraphNodeKind::Output => {
                output_count += 1;
                Reference::Output(index(&node.id, "output-", input.request.outputs.len())?)
            }
            GraphNodeKind::Discard => {
                let id = discard_count;
                discard_count += 1;
                Reference::Discard(id)
            }
            kind => {
                let node_type = match kind {
                    GraphNodeKind::Splitter2 => NodeType::Splitter2,
                    GraphNodeKind::Splitter3 => NodeType::Splitter3,
                    GraphNodeKind::Merger2 => NodeType::Merger2,
                    GraphNodeKind::Merger3 => NodeType::Merger3,
                    _ => unreachable!(),
                };
                let id = u32::try_from(nodes.len()).map_err(|error| error.to_string())?;
                nodes.push(node_type);
                Reference::Node(id)
            }
        };
        if references.insert(&node.id, reference).is_some() {
            return Err("duplicate presentation node ID".into());
        }
    }
    if input_count != input.request.inputs.len().max(1)
        || output_count != input.request.outputs.len()
    {
        return Err("presentation terminals do not match the input/output configuration".into());
    }
    let mut edge_ids = BTreeSet::new();
    let mut flows = Vec::new();
    let mut links = Vec::new();
    let mut used_discards = BTreeSet::new();
    for edge in display.edges {
        if edge.id.len() > 128 || !edge_ids.insert(edge.id) {
            return Err("invalid or duplicate presentation edge ID".into());
        }
        let producer = match references.get(&edge.source) {
            Some(Reference::Input(id)) if edge.source_port == 0 => Producer::Input(*id),
            Some(Reference::Node(node)) => Producer::Node(NodePort {
                node: *node,
                port: edge.source_port,
            }),
            _ => return Err("invalid presentation producer".into()),
        };
        let consumer = match references.get(&edge.target) {
            Some(Reference::Output(id)) if edge.target_port == 0 => Consumer::Output(*id),
            Some(Reference::Discard(id)) if edge.target_port == 0 => {
                used_discards.insert(*id);
                Consumer::Discard(*id)
            }
            Some(Reference::Node(node)) => Consumer::Node(NodePort {
                node: *node,
                port: edge.target_port,
            }),
            _ => return Err("invalid presentation consumer".into()),
        };
        flows.push(edge.rate.exact);
        links.push(Link { producer, consumer });
    }
    if used_discards.len() != discard_count as usize {
        return Err("unused presentation discard terminal".into());
    }
    let share = Share {
        kind: ShareKind::SelectedSolution,
        version: 1,
        request: input.request,
        topology: Topology { nodes, links },
    };
    // Validate supplied flows first. Do not repair a false witness by reconstructing it.
    let payload = bridge(&share, Some(&flows))?;
    match super::verify(&payload, false)? {
        Response::Verified { solution, .. } => Ok(ShareResponse::VerifiedShare { share, solution }),
        Response::Rejected { error } => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn direct() -> Value {
        json!({"kind":"selected-solution","version":1,
            "request":{"inputs":[],"outputs":[{"name":"Third","rate":"1/3"}],"beltRate":"1"},
            "topology":{"nodes":[],"links":[{"producer":{"owner":"input","port":0},"consumer":{"owner":"output","port":0}}]}})
    }
    fn verify(value: &Value) -> Value {
        serde_json::from_str(&verify_share_json(&value.to_string())).unwrap()
    }

    #[test]
    fn share_reconstructs_exact_flows_without_proofs() {
        let result = verify(&direct());
        assert_eq!(result["kind"], "verified-share");
        assert_eq!(result["solution"]["edges"][0]["rate"]["exact"], "1/3");
        assert_eq!(result["solution"]["status"], "best_known");
        assert!(result["solution"]["proof"].is_null());
        assert!(
            result["share"]["topology"]["links"][0]
                .get("flow")
                .is_none()
        );
    }

    #[test]
    fn public_schema_rejects_unknown_fields_at_every_level() {
        for path in [vec![], vec!["request"], vec!["topology"]] {
            let mut value = direct();
            let mut target = &mut value;
            for key in path {
                target = &mut target[key];
            }
            target["proof"] = json!({"minimumNodeCount":0});
            assert_eq!(verify(&value)["kind"], "rejected");
        }
        for location in ["endpoint", "link", "port"] {
            let mut value = direct();
            match location {
                "endpoint" => value["request"]["outputs"][0]["position"] = json!(0),
                "link" => value["topology"]["links"][0]["flow"] = json!("1/3"),
                _ => value["topology"]["links"][0]["producer"]["proof"] = json!(true),
            }
            assert_eq!(verify(&value)["kind"], "rejected", "{location}");
        }
        let mut value = direct();
        value["version"] = json!(2);
        assert_eq!(verify(&value)["kind"], "rejected");
        value["version"] = json!(1);
        value["request"]["outputs"][0]["rate"] = json!("9".repeat(257));
        assert_eq!(verify(&value)["kind"], "rejected");
        assert!(verify_share_json(&" ".repeat(MAX_PAYLOAD_BYTES + 1)).contains("exceeds"));
    }

    #[test]
    fn presentation_migration_checks_flows_and_drops_forged_labels() {
        let verified = verify(&direct());
        let mut input = json!({"request": direct()["request"], "solution": verified["solution"]});
        input["solution"]["status"] = json!("proven_optimal");
        input["solution"]["proof"] = json!({"invented":true});
        input["solution"]["nodes"][0]["label"] = json!("<script>forged</script>");
        let converted: Value =
            serde_json::from_str(&share_from_presentation_json(&input.to_string())).unwrap();
        assert_eq!(converted["kind"], "verified-share");
        assert_eq!(converted["solution"]["status"], "best_known");
        assert!(converted["solution"]["proof"].is_null());
        assert!(!converted.to_string().contains("forged"));
        input["solution"]["edges"][0]["rate"]["exact"] = json!("1/2");
        assert!(share_from_presentation_json(&input.to_string()).contains("rejected"));
        input["solution"]["modelVersion"] = json!(1);
        assert!(share_from_presentation_json(&input.to_string()).contains("older or unknown"));
    }

    #[test]
    fn corrupt_topologies_are_not_accepted_as_best_known() {
        let mut value = direct();
        value["topology"]["links"][0]["consumer"] =
            json!({"owner":"node","port":{"node":0,"port":0}});
        assert_eq!(verify(&value)["kind"], "rejected");
        value = direct();
        value["request"]["beltRate"] = json!("1/4");
        assert_eq!(verify(&value)["kind"], "rejected");
        value = direct();
        value["topology"]["links"] = json!([]);
        assert_eq!(verify(&value)["kind"], "rejected");
    }
}
