use super::*;
use serde_json::{Value, json};

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

#[test]
fn preparation_rejects_unbounded_and_invalid_application_requests() {
    for source in [
        " ".repeat(MAX_REQUEST_BYTES + 1),
        "{}".into(),
        request(&[], &["0"], "one_min_nl"),
        request(&[], &["1"], "invalid"),
    ] {
        assert!(BrowserRun::new(&source, "job", 1, None).is_err());
    }
    assert!(BrowserRun::new(&request(&[], &["1/3"], "one_min_nl"), "job", 1, None).is_ok());
    assert!(initial_job_json("", 1).is_err());
}

#[test]
fn node_cap_and_global_contradiction_have_different_terminal_projection() {
    for mode in ["one_min_nl", "all_min_nl", "all_min_n"] {
        let mut limited =
            BrowserRun::new(&request(&["3"], &["1", "2"], mode), "limit", 42, Some(0)).unwrap();
        let packet: Value = serde_json::from_str(&limited.poll(10).unwrap()).unwrap();
        assert_eq!(packet["done"], true);
        let terminal = packet["packets"].as_array().unwrap().last().unwrap();
        assert_eq!(terminal["status"], "incomplete");
        assert_eq!(terminal["enumerationComplete"], false);
        assert!(terminal["unsat"].is_null());
        assert!(limited.poll(11).is_err());

        let mut impossible =
            BrowserRun::new(&request(&["1"], &["2"], mode), "unsat", 42, Some(0)).unwrap();
        let packet: Value = serde_json::from_str(&impossible.poll(10).unwrap()).unwrap();
        assert_eq!(packet["done"], true);
        assert_eq!(
            packet["packets"].as_array().unwrap().last().unwrap()["status"],
            "unsat"
        );
        assert!(packet["dispatch"].as_array().unwrap().is_empty());
    }
}

#[test]
fn unknown_poisoning_cannot_be_rehabilitated_with_unsat_or_duplicate_retirement() {
    let mut run =
        BrowserRun::new(&request(&["1"], &["1"], "all_min_nl"), "job", 42, Some(0)).unwrap();
    let packet: Value = serde_json::from_str(&run.poll(1).unwrap()).unwrap();
    let id = packet["dispatch"][0]["id"].to_string();
    assert!(run.advance(&id, None).is_ok());
    assert!(
        run.advance(&id, Some("unknown (RESOURCEOUT)".into()))
            .is_err()
    );
    assert!(run.advance(&id, Some("unsat".into())).is_err());
    assert_eq!(
        run.retire(&id, "failed", "resource exhaustion")
            .unwrap_err(),
        "resource exhaustion"
    );
    assert!(run.retire(&id, "exhausted", "").is_err());
    let packet: Value = serde_json::from_str(&run.poll(2).unwrap()).unwrap();
    let terminal = packet["packets"].as_array().unwrap().last().unwrap();
    assert_eq!(terminal["status"], "failed");
    assert_eq!(terminal["enumerationComplete"], false);
    assert_eq!(terminal["error"], "resource exhaustion");
}

#[test]
fn empty_admission_has_rust_projected_interruption_packets() {
    let packet: Value = serde_json::from_str(&initial_job_json("job", 42).unwrap()).unwrap();
    assert_eq!(packet["packets"][0]["status"], "running");
    assert_eq!(packet["recovery"]["cancelled"]["status"], "cancelled");
    assert_eq!(packet["recovery"]["failed"]["status"], "failed");
    for reason in ["cancelled", "failed"] {
        let recovery = &packet["recovery"][reason];
        assert_eq!(recovery["resultsOmitted"], true);
        assert_eq!(recovery["sequence"], 1);
        assert_eq!(recovery["enumerationComplete"], false);
        assert!(recovery["proof"]["minimumLinkCount"].is_null());
    }
}
