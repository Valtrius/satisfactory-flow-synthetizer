//! Test-only native verification of packets emitted by the production browser app.
//! No search is run here. The expected corpus has already matched native search
//! against the independent exhaustive reference implementation.
use serde_json::{Value, json};
use solver_api::{CanonicalGraphKey, PhysicalGraph, Problem, ProblemRequest};
use std::{collections::BTreeMap, io::Read};

fn physical(request: &Value, display: &Value) -> (CanonicalGraphKey, Value) {
    let mut request = request.clone();
    request.as_object_mut().unwrap().remove("solveMode");
    let prepared: ProblemRequest = serde_json::from_value(request.clone()).unwrap();
    let prepared = prepared.prepare().unwrap();
    for side in ["inputs", "outputs"] {
        for endpoint in request[side].as_array_mut().unwrap() {
            endpoint.as_object_mut().unwrap().remove("id");
        }
    }
    let verified: Value = serde_json::from_str(&solver_web::share_from_presentation_json(
        &json!({"request": request, "solution": display}).to_string(),
    ))
    .unwrap();
    assert_eq!(verified["kind"], "verified-share", "{verified}");
    let topology = &verified["share"]["topology"];
    let nodes: Vec<_> = topology["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(id, kind)| json!({"id": id, "nodeType": kind}))
        .collect();
    let links: Vec<_> = topology["links"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(index, link)| {
            let mut link = link.clone();
            link["flow"] = display["edges"][index]["rate"]["exact"].clone();
            link
        })
        .collect();
    let graph: PhysicalGraph =
        serde_json::from_value(json!({"nodes": nodes, "links": links})).unwrap();
    let validation = solver_validation::validate_solution(&prepared.problem, &graph).unwrap();
    assert_eq!(display["stats"]["nodeCount"], validation.node_count);
    assert_eq!(display["stats"]["linkCount"], validation.link_count);
    let (key, graph) = solver_validation::canonical_layout(&prepared.problem, &graph);
    (
        key.clone(),
        json!({"key": key, "graph": graph, "validation": validation}),
    )
}

fn verify_case(case: &Value) -> Value {
    let request = &case["request"];
    let snapshot = &case["snapshot"];
    let result = (!snapshot["result"].is_null()).then(|| physical(request, &snapshot["result"]));
    let mut collection = BTreeMap::new();
    let mut source_keys = Vec::new();
    for display in snapshot["results"].as_array().unwrap() {
        let (key, graph) = physical(request, display);
        source_keys.push(key.clone());
        assert!(
            collection.insert(key, graph).is_none(),
            "duplicate browser layout"
        );
    }
    if let Some(live) = case["live"].as_array() {
        let mut appended = 0;
        for packet in live {
            if packet["status"] != "running" || packet["result"].is_null() {
                continue;
            }
            assert_eq!(packet["result"]["status"], "best_known");
            assert!(packet["result"]["proof"].is_null());
            let (key, _) = physical(request, &packet["result"]);
            if packet["resultAppended"] == true {
                assert_eq!(
                    source_keys[appended], key,
                    "terminal projection reordered a published source index"
                );
                appended += 1;
            } else if request["solveMode"] == "one_min_nl" {
                assert_eq!(
                    result.as_ref().unwrap().0,
                    key,
                    "single-result identity changed at completion"
                );
            }
        }
    }
    let solutions: Vec<_> = collection.into_values().collect();
    if !case["expected"].is_null() {
        let expected = &case["expected"];
        let status = match expected["kind"].as_str().unwrap() {
            "optimal" => "completed",
            "globally_unsat" => "unsat",
            "incomplete" => "incomplete",
            _ => panic!("unknown reference outcome"),
        };
        assert_eq!(snapshot["status"], status, "{}: {snapshot}", case["name"]);
        assert_eq!(snapshot["proof"], expected["proof"]);
        assert_eq!(
            snapshot["enumerationComplete"],
            expected["enumeration"]["complete"]
                .as_bool()
                .unwrap_or(false)
        );
        assert_eq!(
            solutions,
            *expected["solutions"].as_array().unwrap(),
            "{}",
            case["name"]
        );
        let objective = result.as_ref().map(|(_, graph)| {
            json!([
                graph["validation"]["nodeCount"],
                graph["validation"]["linkCount"]
            ])
        });
        assert_eq!(objective.unwrap_or(Value::Null), expected["objective"]);
        if status == "completed" {
            assert_eq!(snapshot["result"]["status"], "proven_optimal");
        }
    }
    // Confirm the public exact-rate problem is the one used by the reference.
    if !case["problem"].is_null() {
        let public: synthetizer_app::jobs::SolveRequest =
            serde_json::from_value(request.clone()).unwrap();
        let expected: Problem = serde_json::from_value(case["problem"].clone()).unwrap();
        assert_eq!(public.problem.prepare().unwrap().problem, expected);
    }
    json!({"name": case["name"], "status": snapshot["status"], "result": result.map(|(_, graph)| graph), "solutions": solutions})
}

fn main() {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).unwrap();
    let cases: Vec<Value> = serde_json::from_str(&input).unwrap();
    let checked: Vec<_> = cases.iter().map(verify_case).collect();
    println!("{}", json!(checked));
}
