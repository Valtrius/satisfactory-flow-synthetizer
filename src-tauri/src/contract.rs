//! Job request envelope. Mathematical contracts live in solver-api.
use serde::{Deserialize, Serialize};
pub use solver_api::{GlobalUnsatProof as UnsatProof, SolverProgress};
pub use synthetizer_app::runtime::Solution;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SolveRequest {
    #[serde(flatten)]
    pub problem: solver_api::ProblemRequest,
    #[serde(default)]
    pub solve_mode: solver_api::SolveMode,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_job_envelope_keeps_the_existing_flat_request_shape() {
        let request: SolveRequest = serde_json::from_value(serde_json::json!({
            "inputs": [], "outputs": [{"id": "o", "name": "Output", "rate": "1/3"}],
            "beltRate": "1"
        }))
        .unwrap();
        assert!(
            serde_json::to_value(&request)
                .unwrap()
                .get("engine")
                .is_none()
        );
        assert_eq!(request.solve_mode, solver_api::SolveMode::OneMinNL);
        assert_eq!(
            request.problem.prepare().unwrap().problem.inputs[0].to_string(),
            "1/3"
        );
        let json = serde_json::to_value(&request).unwrap();
        assert!(json.get("problem").is_none());
        assert_eq!(json["beltRate"], "1");
    }

    #[test]
    fn both_job_engines_reject_automatic_supply_above_one_belt() {
        for engine in ["custom", "z3", "astra"] {
            let request: SolveRequest = serde_json::from_value(serde_json::json!({
                "engine": engine, "inputs": [], "outputs": [
                    {"id": "a", "name": "A", "rate": "60"},
                    {"id": "b", "name": "B", "rate": "60"}
                ], "beltRate": "100"
            }))
            .unwrap();
            assert!(
                request
                    .problem
                    .prepare()
                    .unwrap_err()
                    .to_string()
                    .contains("split the inputs explicitly")
            );
        }
    }

    #[test]
    fn deserializes_the_minimum_link_enumeration_mode() {
        let request: SolveRequest = serde_json::from_value(serde_json::json!({
            "inputs": [], "outputs": [{"id": "o", "name": "Output", "rate": "1"}],
            "beltRate": "1", "solveMode": "all_min_nl"
        }))
        .unwrap();
        assert_eq!(request.solve_mode, solver_api::SolveMode::AllMinNL);
    }
}
