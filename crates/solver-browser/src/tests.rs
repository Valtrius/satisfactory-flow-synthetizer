use super::*;
use serde_json::{Value, json};
use solver_api::{BestKnownSolution, PhysicalGraph};

fn request(inputs: &[&str], outputs: &[&str], mode: &str) -> String {
    let endpoints = |rates: &[&str]| {
        rates
            .iter()
            .enumerate()
            .map(|(index, rate)| json!({"id": index.to_string(), "name": "", "rate": rate}))
            .collect::<Vec<_>>()
    };
    json!({"inputs": endpoints(inputs), "outputs": endpoints(outputs), "beltRate": "1200", "solveMode": mode}).to_string()
}

fn run(mode: &str, options: &Value) -> BrowserCoordinator {
    BrowserCoordinator::new(
        &request(&["1/3"], &["1/3"], mode),
        "job",
        42,
        &options.to_string(),
    )
    .unwrap()
}
fn poll(run: &mut BrowserCoordinator) -> Value {
    serde_json::from_str(&run.poll(10).unwrap()).unwrap()
}
fn send(run: &mut BrowserCoordinator, value: &Value) -> Result<(), String> {
    run.accept(&value.to_string(), 9)
}
fn retire(run: &mut BrowserCoordinator, id: &Value, verdict: &str) {
    send(
        run,
        &json!({"kind":"retired", "id":id, "verdict":verdict,"detail":"injected failure"}),
    )
    .unwrap();
}
fn direct_witness(task: &Value) -> Value {
    let mut leaf = BrowserLeaf::new(task["source"].as_str().unwrap()).unwrap();
    assert!(leaf.advance(None).unwrap().contains("check-sat"));
    assert!(
        leaf.advance(Some("sat".into()))
            .unwrap()
            .contains("get-value")
    );
    let model: Value =
        serde_json::from_str(&leaf.advance(Some("((e0 true))".into())).unwrap()).unwrap();
    assert_eq!(model["witness"]["graph"]["links"][0]["flow"], "1/3");
    model["witness"].clone()
}

#[test]
fn bounded_real_requests_and_host_options_are_checked_before_dispatch() {
    for source in [
        " ".repeat(MAX_REQUEST_BYTES + 1),
        "{}".into(),
        request(&[], &["0"], "one_min_nl"),
        request(&[], &["1"], "invalid"),
    ] {
        assert!(BrowserCoordinator::new(&source, "job", 42, "{}").is_err());
    }
    for options in [
        json!({"workerCount":0}),
        json!({"workerCount":4_294_967_296_u64}),
        json!({"maxLayouts":0}),
        json!({"maxIdentityBytes":0}),
        json!({"strategy":"invalid"}),
        json!({"proof":1}),
    ] {
        assert!(
            BrowserCoordinator::new(
                &request(&[], &["1"], "one_min_nl"),
                "job",
                42,
                &options.to_string()
            )
            .is_err()
        );
    }
    assert!(BrowserCoordinator::new(&request(&[], &["1"], "one_min_nl"), "", 42, "{}").is_err());
    let mut valid = run("all_min_nl", &json!({}));
    let initial = poll(&mut valid);
    assert_eq!(initial["scheduling"]["budgets"], json!([1]));
    for reason in ["cancelled", "failed"] {
        assert_eq!(initial["recovery"][reason]["status"], reason);
        assert_eq!(
            initial["recovery"][reason]["sequence"],
            initial["packets"][0]["sequence"].as_u64().unwrap() + 1
        );
        assert_eq!(initial["recovery"][reason]["resultsOmitted"], true);
    }
}

#[test]
fn node_caps_are_incomplete_and_global_contradictions_need_no_compute_workers() {
    for mode in ["one_min_nl", "all_min_nl", "all_min_n"] {
        for workers in [1, 2, 4] {
            for (inputs, outputs, status) in [
                (vec!["3"], vec!["1", "2"], "incomplete"),
                (vec!["1"], vec!["2"], "unsat"),
            ] {
                let mut run = BrowserCoordinator::new(
                    &request(&inputs, &outputs, mode),
                    "job",
                    42,
                    &json!({"maxNodes":0,"workerCount":workers}).to_string(),
                )
                .unwrap();
                let value = poll(&mut run);
                assert_eq!(value["done"], true);
                assert!(value["dispatch"].as_array().unwrap().is_empty());
                let terminal = value["packets"].as_array().unwrap().last().unwrap();
                assert_eq!(terminal["status"], status);
                if status == "incomplete" {
                    assert_eq!(terminal["enumerationComplete"], false);
                    assert!(terminal["unsat"].is_null());
                }
                assert!(run.poll(11).is_err());
            }
        }
    }
}

#[test]
fn streamed_results_keep_caller_scale_identity_and_never_emit_the_collection_twice() {
    for mode in ["one_min_nl", "all_min_nl", "all_min_n"] {
        let mut run = run(mode, &json!({"workerCount":2}));
        let initial = poll(&mut run);
        let tasks = initial["dispatch"].as_array().unwrap();
        assert_eq!(tasks.len(), 2);
        for task in tasks {
            let witness = direct_witness(task);
            send(
                &mut run,
                &json!({"kind":"witness", "id":task["id"], "witness":witness}),
            )
            .unwrap();
        }
        let live = poll(&mut run);
        assert_eq!(live["done"], false);
        assert_eq!(
            live["append"].as_array().unwrap().len(),
            usize::from(mode != "one_min_nl")
        );
        assert_eq!(live["packets"][0]["result"]["totalInput"]["exact"], "1/3");
        assert_eq!(live["packets"][0]["result"]["status"], "best_known");
        assert!(live["packets"][0]["result"]["proof"].is_null());
        retire(
            &mut run,
            &tasks[1]["id"],
            if mode == "one_min_nl" {
                "optimum"
            } else {
                "exhausted"
            },
        );
        let proposal = poll(&mut run);
        assert_eq!(
            proposal["done"], false,
            "the winning branch must wait for its sibling's retirement"
        );
        assert_eq!(proposal["stop"], json!([tasks[0]["id"]]));
        retire(&mut run, &tasks[0]["id"], "cancelled");
        let final_value = poll(&mut run);
        assert_eq!(final_value["done"], true);
        assert_eq!(final_value["scheduling"]["proofOwner"], 1);
        assert!(final_value["append"].as_array().unwrap().is_empty());
        let terminal = final_value["packets"].as_array().unwrap().last().unwrap();
        assert_eq!(terminal["status"], "completed");
        assert_eq!(
            terminal["proof"],
            json!({"minimumNodeCount":0,"minimumLinkCount":0})
        );
        assert_eq!(terminal["enumerationComplete"], mode != "one_min_nl");
        assert!(terminal["results"].as_array().unwrap().is_empty());
        // A two-branch portfolio still returns one root/profile proof, not their sum.
        assert_eq!(
            terminal["result"]["proof"]["profilesExhausted"],
            u32::from(mode != "one_min_nl")
        );
        assert!(
            send(
                &mut run,
                &json!({"kind":"retired","id":tasks[1]["id"],"verdict":"exhausted","detail":""})
            )
            .is_err()
        );
    }
}

#[test]
fn failed_branch_does_not_invalidate_an_independent_complete_owner() {
    let mut run = run("all_min_nl", &json!({"workerCount":2}));
    let initial = poll(&mut run);
    let tasks = initial["dispatch"].as_array().unwrap();
    retire(&mut run, &tasks[0]["id"], "failed");
    let live = poll(&mut run);
    assert_eq!(live["done"], false);
    send(
        &mut run,
        &json!({"kind":"witness","id":tasks[1]["id"],"witness":direct_witness(&tasks[1])}),
    )
    .unwrap();
    retire(&mut run, &tasks[1]["id"], "exhausted");
    let done = poll(&mut run);
    assert_eq!(done["scheduling"]["proofOwner"], 1);
    assert_eq!(
        done["packets"].as_array().unwrap().last().unwrap()["status"],
        "completed"
    );
}

#[test]
fn cancellation_and_resource_guards_retain_the_accepted_prefix_without_exhaustion() {
    for external in [true, false] {
        let mut run = run(
            "all_min_nl",
            &if external {
                json!({"workerCount":2})
            } else {
                json!({"workerCount":2,"maxLayouts":1})
            },
        );
        let initial = poll(&mut run);
        let tasks = initial["dispatch"].as_array().unwrap();
        send(
            &mut run,
            &json!({"kind":"witness","id":tasks[0]["id"],"witness":direct_witness(&tasks[0])}),
        )
        .unwrap();
        if external {
            send(
                &mut run,
                &json!({"kind":"interrupt","reason":"cancelled","detail":""}),
            )
            .unwrap();
        }
        let stopping = poll(&mut run);
        assert_eq!(stopping["append"].as_array().unwrap().len(), 1);
        assert_eq!(stopping["done"], false);
        assert_eq!(stopping["stop"].as_array().unwrap().len(), 2);
        for task in tasks {
            retire(&mut run, &task["id"], "cancelled");
        }
        let done = poll(&mut run);
        let terminal = done["packets"].as_array().unwrap().last().unwrap();
        assert_eq!(done["count"], 1);
        assert_eq!(
            terminal["status"],
            if external { "cancelled" } else { "incomplete" }
        );
        assert_eq!(terminal["enumerationComplete"], false);
        assert_eq!(terminal["result"]["status"], "best_known");
        assert!(terminal["proof"]["minimumLinkCount"].is_null());
    }
}

#[test]
fn malformed_and_unknown_leaf_replies_poison_and_invalid_retirement_keeps_ownership() {
    let mut run = run("all_min_nl", &json!({}));
    let initial = poll(&mut run);
    let task = &initial["dispatch"][0];
    for response in ["unknown (RESOURCEOUT)", "sat unsat", "(error bad)"] {
        let mut leaf = BrowserLeaf::new(task["source"].as_str().unwrap()).unwrap();
        leaf.advance(None).unwrap();
        assert!(leaf.advance(Some(response.into())).is_err());
        assert!(leaf.advance(Some("unsat".into())).is_err());
    }
    assert!(
        send(
            &mut run,
            &json!({"kind":"retired","id":task["id"],"verdict":"unknown","detail":""})
        )
        .is_err()
    );
    assert_eq!(poll(&mut run)["done"], false);
    retire(&mut run, &task["id"], "cancelled");
    let done = poll(&mut run);
    assert_eq!(
        done["packets"].as_array().unwrap().last().unwrap()["status"],
        "failed"
    );
}

fn split_witness() -> Value {
    let request: synthetizer_app::jobs::SolveRequest =
        serde_json::from_str(&request(&["2"], &["1", "1"], "all_min_nl")).unwrap();
    let prepared = request.problem.prepare().unwrap();
    let graph: PhysicalGraph = serde_json::from_value(json!({"nodes":[{"id":0,"nodeType":"splitter2"}],"links":[
        {"producer":{"owner":"input","port":0},"consumer":{"owner":"node","port":{"node":0,"port":0}},"flow":"2"},
        {"producer":{"owner":"node","port":{"node":0,"port":0}},"consumer":{"owner":"output","port":0},"flow":"1"},
        {"producer":{"owner":"node","port":{"node":0,"port":1}},"consumer":{"owner":"output","port":1},"flow":"1"}
    ]})).unwrap();
    let validation = solver_validation::validate_solution(&prepared.problem, &graph).unwrap();
    serde_json::to_value(BestKnownSolution {
        node_count: 1,
        link_count: 0,
        physical_link_count: 3,
        discard_link_count: 0,
        canonical_graph_key: solver_validation::layout_key(&prepared.problem, &graph),
        graph,
        validation,
    })
    .unwrap()
}

#[test]
fn parallel_budgets_and_static_second_output_children_use_the_shared_planner() {
    for (count, expected) in [
        (1, vec![1]),
        (2, vec![1, 1]),
        (3, vec![2, 1]),
        (8, vec![4, 4]),
        (32, vec![16, 16]),
        (64, vec![32, 32]),
        (192, vec![96, 96]),
    ] {
        let mut run = run("one_min_nl", &json!({"workerCount":count}));
        assert_eq!(poll(&mut run)["scheduling"]["budgets"], json!(expected));
    }
    let mut run = BrowserCoordinator::new(
        &request(&["2"], &["1", "1"], "all_min_nl"),
        "job",
        42,
        "{\"workerCount\":8,\"strategy\":\"boolean\"}",
    )
    .unwrap();
    let initial = poll(&mut run);
    assert_eq!(initial["dispatch"].as_array().unwrap().len(), 4);
    assert_eq!(initial["scheduling"]["secondOutputRoots"], 4);
    for task in initial["dispatch"].as_array().unwrap() {
        let work: Value = serde_json::from_str(task["source"].as_str().unwrap()).unwrap();
        if work["source"] == 1 && work["secondSource"] == 1 {
            send(
                &mut run,
                &json!({"kind":"witness","id":task["id"],"witness":split_witness()}),
            )
            .unwrap();
        }
        retire(&mut run, &task["id"], "exhausted");
    }
    let done = poll(&mut run);
    assert_eq!(done["done"], true);
    assert_eq!(
        done["packets"].as_array().unwrap().last().unwrap()["result"]["proof"]["rootPartitionsExhausted"],
        4
    );
}

#[test]
fn adaptive_children_wait_for_a_validated_witness_and_parent_retirement() {
    let mut run = BrowserCoordinator::new(
        &request(&["2"], &["1", "1"], "all_min_nl"),
        "job",
        42,
        "{\"workerCount\":2,\"strategy\":\"boolean\"}",
    )
    .unwrap();
    let initial = poll(&mut run);
    let parents = initial["dispatch"].as_array().unwrap();
    retire(&mut run, &parents[1]["id"], "exhausted");
    assert!(poll(&mut run)["dispatch"].as_array().unwrap().is_empty());
    send(
        &mut run,
        &json!({"kind":"witness","id":parents[0]["id"],"witness":split_witness()}),
    )
    .unwrap();
    let refined = poll(&mut run);
    assert_eq!(refined["dispatch"].as_array().unwrap().len(), 1);
    retire(&mut run, &parents[0]["id"], "exhausted");
    let stopping = poll(&mut run);
    assert_eq!(stopping["done"], false);
    assert_eq!(stopping["stop"], json!([refined["dispatch"][0]["id"]]));
    retire(&mut run, &refined["dispatch"][0]["id"], "cancelled");
    let done = poll(&mut run);
    assert_eq!(done["done"], true);
    assert_eq!(done["scheduling"]["adaptiveGroups"], 1);
    assert_eq!(done["scheduling"]["adaptiveChildren"], 2);
    assert_eq!(
        done["packets"].as_array().unwrap().last().unwrap()["result"]["proof"]["rootPartitionsExhausted"],
        2
    );
}
