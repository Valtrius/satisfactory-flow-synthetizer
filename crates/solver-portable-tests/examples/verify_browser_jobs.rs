//! Test-only native verification of packets emitted by the production browser app.
//! No search is run here. The expected corpus has already matched native search
//! against the independent exhaustive reference implementation.
use serde_json::{Value, json};
use solver_api::{CanonicalGraphKey, PhysicalGraph, Problem, ProblemRequest};
use std::{collections::BTreeMap, io::Read};

fn physical(request: &Value, display: &Value) -> (CanonicalGraphKey, Value) {
    let mut request = request.clone();
    request.as_object_mut().unwrap().remove("solveMode");
    request.as_object_mut().unwrap().remove("browserWorkers");
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

fn verify_paged_sources(case: &Value, source_keys: &[CanonicalGraphKey]) {
    let snapshot = &case["snapshot"];
    if let Some(count) = snapshot["collection"]["count"].as_u64() {
        assert!(
            snapshot["results"].as_array().unwrap().is_empty(),
            "paged job duplicated its graph collection"
        );
        assert_eq!(usize::try_from(count).unwrap(), source_keys.len());
    }
    if let Some(published) = case["published"].as_array() {
        for (index, row) in published.iter().enumerate() {
            assert_eq!(row["index"], index);
            assert_eq!(
                physical(&case["request"], &row["solution"]).0,
                source_keys[index],
                "paged source identity changed after publication"
            );
            assert_eq!(row["solution"]["status"], "best_known");
            assert!(row["solution"]["proof"].is_null());
        }
        assert_eq!(published.len(), source_keys.len());
    }
}

fn verify_case(case: &Value) -> Value {
    let request = &case["request"];
    let snapshot = &case["snapshot"];
    let result = (!snapshot["result"].is_null()).then(|| physical(request, &snapshot["result"]));
    let mut collection = BTreeMap::new();
    let mut source_keys = Vec::new();
    let displays = case["solutions"]
        .as_array()
        .unwrap_or_else(|| snapshot["results"].as_array().unwrap());
    for display in displays {
        let (key, graph) = physical(request, display);
        source_keys.push(key.clone());
        assert!(
            collection.insert(key, graph).is_none(),
            "duplicate browser layout"
        );
    }
    verify_paged_sources(case, &source_keys);
    if let Some(live) = case["live"].as_array() {
        let mut appended = 0;
        for packet in live {
            if packet["status"] != "running" || packet["result"].is_null() {
                continue;
            }
            assert_eq!(packet["result"]["status"], "best_known");
            assert!(packet["result"]["proof"].is_null());
            let (key, _) = physical(request, &packet["result"]);
            if let Some(index) = packet["collection"]["preferredIndex"].as_u64() {
                assert_eq!(
                    key,
                    source_keys[usize::try_from(index).unwrap()],
                    "paged preferred source index changed"
                );
            }
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
