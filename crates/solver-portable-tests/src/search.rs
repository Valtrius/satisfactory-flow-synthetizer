//! Test-only browser adapter for the production planner and leaf protocol.
use serde_json::{Value, json};
use solver_api::{Problem, RunOptions, SolveOutcome, SolveResult};
use solver_core::{
    Counts, Failure,
    leaf::{Completion, LeafDriver},
    planner::{ExactPlanner, LeafId, PlannerEvent},
};
use solver_validation::{canonical_layout, layout_key, validate_solution};
use std::collections::BTreeMap;
use wasm_bindgen::prelude::*;

/// Compare exact objectives, public proofs, enumeration and full canonical
/// witnesses. The preferred equal-optimum tie is deliberately not prescribed.
/// # Panics
/// Panics if a trusted test result contains an invalid graph or identity.
#[must_use]
pub fn summarize(problem: &Problem, outcome: &SolveOutcome) -> Value {
    let (kind, objective) = match &outcome.result {
        SolveResult::Optimal(result) => {
            assert_eq!(
                validate_solution(problem, &result.graph).unwrap(),
                result.validation
            );
            assert_eq!(
                layout_key(problem, &result.graph),
                result.canonical_graph_key
            );
            ("optimal", Some((result.node_count, result.link_count)))
        }
        SolveResult::Incomplete(result) => {
            if let Some(best) = &result.best_known {
                assert_eq!(
                    validate_solution(problem, &best.graph).unwrap(),
                    best.validation
                );
                assert_eq!(layout_key(problem, &best.graph), best.canonical_graph_key);
            }
            (
                "incomplete",
                result
                    .best_known
                    .as_ref()
                    .map(|best| (best.node_count, best.link_count)),
            )
        }
        SolveResult::GloballyUnsat(_) => ("globally_unsat", None),
    };
    let mut solutions = BTreeMap::new();
    for solution in &outcome.solutions {
        let validation = validate_solution(problem, &solution.graph).unwrap();
        assert_eq!(validation, solution.validation);
        let (key, graph) = canonical_layout(problem, &solution.graph);
        assert_eq!(key, solution.canonical_graph_key);
        assert!(
            solutions
                .insert(
                    key.clone(),
                    json!({"key": key, "graph": graph, "validation": validation})
                )
                .is_none()
        );
    }
    json!({"kind": kind, "objective": objective, "proof": outcome.proof,
        "enumeration": outcome.enumeration, "solutions": solutions.into_values().collect::<Vec<_>>()})
}

fn leaf_id(value: &str) -> Result<LeafId, String> {
    let (group, index) = serde_json::from_str(value).map_err(|error| error.to_string())?;
    Ok(LeafId { group, index })
}

/// Owned by one test worker. No JS callback implements a native Sync observer.
#[wasm_bindgen]
pub struct PortableRun {
    problem: Problem,
    planner: ExactPlanner,
    leaves: BTreeMap<LeafId, Option<LeafDriver>>,
    elapsed: u64,
}

#[wasm_bindgen]
impl PortableRun {
    /// Build a bounded, trusted qualification request, not a public import API.
    /// # Errors
    /// Returns malformed requests or invalid solver options.
    #[wasm_bindgen(constructor)]
    pub fn new(source: &str) -> Result<Self, String> {
        let value: Value = serde_json::from_str(source).map_err(|error| error.to_string())?;
        let problem: Problem =
            serde_json::from_value(value["problem"].clone()).map_err(|error| error.to_string())?;
        let options: RunOptions =
            serde_json::from_value(value["options"].clone()).map_err(|error| error.to_string())?;
        if options.worker_count != 1 || options.max_nodes.is_none_or(|limit| limit > 4) {
            return Err(
                "qualification requests require one worker and a node cap at most four".into(),
            );
        }
        let planner = ExactPlanner::new(&problem, options, Counts::Boolean)
            .map_err(|error| error.to_string())?;
        Ok(Self {
            problem,
            planner,
            leaves: BTreeMap::new(),
            elapsed: 0,
        })
    }

    /// Return work or a sealed result. The test host supplies deterministic time.
    /// # Panics
    /// Panics if the planner violates its own dispatch or preparation contract.
    pub fn poll(&mut self) -> String {
        self.elapsed += 1;
        let update = self.planner.poll(self.elapsed);
        let dispatch: Vec<_> = update
            .dispatch
            .into_iter()
            .map(|task| {
                let leaf = (!task.root.impossible).then(|| {
                    LeafDriver::new(self.planner.context().expect("prepared leaf"), task.spec)
                });
                assert!(self.leaves.insert(task.id, leaf).is_none());
                json!({"id": [task.id.group, task.id.index], "impossible": task.root.impossible})
            })
            .collect();
        let outcome = self.planner.outcome();
        if outcome.is_some() {
            assert!(self.leaves.is_empty());
        }
        json!({"dispatch": dispatch, "events": update.events,
            "done": outcome.as_ref().map(|outcome| summarize(&self.problem, outcome)),
            "stopping": self.planner.stopping(), "ledger": self.planner.proof()})
        .to_string()
    }

    /// Process one backend reply and retain an accepted witness in the planner.
    /// # Errors
    /// Returns protocol errors, stale IDs or failed exact reconstruction.
    // wasm-bindgen's optional string ABI requires an owned String.
    #[allow(clippy::needless_pass_by_value)]
    pub fn advance(&mut self, id: &str, reply: Option<String>) -> Result<String, String> {
        let id = leaf_id(id)?;
        let leaf = self
            .leaves
            .get_mut(&id)
            .and_then(Option::as_mut)
            .ok_or("unknown or impossible leaf")?;
        let action = leaf
            .advance(reply.as_deref(), self.planner.stopping(), &mut ())
            .map_err(|error| error.to_string())?;
        let witnessed = action.witness.is_some();
        if let Some(witness) = action.witness {
            self.planner
                .accept(PlannerEvent::Witness(id, witness), self.elapsed)
                .map_err(|error| error.to_string())?;
        }
        Ok(json!({"commands": action.commands, "witnessed": witnessed,
            "completion": action.completion.map(|result| match result { Completion::Exhausted => "exhausted", Completion::Optimum => "optimum" })}).to_string())
    }

    /// Call only after the JS host disposes the corresponding cvc5 session.
    /// # Errors
    /// Returns unknown IDs, invalid verdicts or invalid completion transitions.
    pub fn retire(&mut self, id: &str, verdict: &str, detail: &str) -> Result<(), String> {
        let id = leaf_id(id)?;
        let result = match verdict {
            "exhausted" => Ok(Completion::Exhausted),
            "optimum" => Ok(Completion::Optimum),
            "cancelled" => Err(Failure::Cancelled),
            "failed" => Err(Failure::Worker(detail.into())),
            _ => return Err("unknown retirement verdict".into()),
        };
        if self.leaves.remove(&id).is_none() {
            return Err("unknown or retired leaf".into());
        }
        self.planner
            .accept(PlannerEvent::Retired(id, result), self.elapsed)
            .map_err(|error| error.to_string())
    }

    pub fn cancel(&mut self) -> bool {
        self.planner.cancel()
    }
}
