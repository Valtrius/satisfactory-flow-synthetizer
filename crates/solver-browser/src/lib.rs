//! Production single-worker browser adapter. JavaScript owns the cvc5 session
//! and worker lifetime; this module owns exact search and application projection.
mod protocol;
#[cfg(test)]
mod tests;

use protocol::{Dispatch, Recovery, Update, encode, recovery};
use solver_api::{PreparedProblem, RunOptions, SolveMode, SolverEvent};
use solver_core::{
    Counts, Failure,
    leaf::{Completion, LeafDriver},
    planner::{ExactPlanner, LeafId, PlannerEvent},
};
use std::collections::BTreeMap;
use synthetizer_app::{
    jobs::{JobSnapshot, SolveRequest, project_outcome},
    solution::Solution,
};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

const MAX_REQUEST_BYTES: usize = 256 * 1024;
const MAX_IDENTIFIER_BYTES: usize = 256;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
#[must_use]
pub fn browser_solver_version() -> u32 {
    1
}

/// Empty admission state, before request preparation or a backend download.
/// # Errors
/// Rejects invalid job identity or serialization failures.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn initial_job_json(job_id: &str, started_at_ms: u64) -> Result<String, String> {
    check_job_id(job_id)?;
    let snapshot = JobSnapshot::new(job_id.to_owned(), started_at_ms);
    let recovery = recovery(
        &snapshot,
        SolveMode::OneMinNL,
        None,
        &solver_api::ProofSummary::default(),
    )?;
    encode(&Update {
        dispatch: Vec::new(),
        packets: vec![snapshot],
        recovery,
        done: false,
    })
}

fn check_job_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > MAX_IDENTIFIER_BYTES {
        return Err("invalid browser job identifier".into());
    }
    Ok(())
}

fn leaf_id(source: &str) -> Result<LeafId, String> {
    if source.len() > 80 {
        return Err("invalid leaf identifier".into());
    }
    let (group, index) = serde_json::from_str(source).map_err(|error| error.to_string())?;
    Ok(LeafId { group, index })
}

/// One independent Boolean search, with one active leaf at a time. No callbacks,
/// clocks, native process handles or cross-worker atomics enter this object.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub struct BrowserRun {
    prepared: PreparedProblem,
    mode: SolveMode,
    planner: ExactPlanner,
    leaves: BTreeMap<LeafId, Option<LeafDriver>>,
    snapshot: JobSnapshot,
    elapsed: u64,
    last_progress: u64,
    terminal_sent: bool,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl BrowserRun {
    /// Parse the real application request with bounded input and exact rates.
    /// `max_nodes` is an optional inclusive host resource limit, never an UNSAT bound.
    /// # Errors
    /// Returns malformed requests, invalid rates or unsupported host options.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(constructor))]
    pub fn new(
        source: &str,
        job_id: &str,
        started_at_ms: u64,
        max_nodes: Option<u32>,
    ) -> Result<Self, String> {
        check_job_id(job_id)?;
        if source.len() > MAX_REQUEST_BYTES {
            return Err("Browser solve request exceeds 256 KiB.".into());
        }
        let request: SolveRequest =
            serde_json::from_str(source).map_err(|error| error.to_string())?;
        if request
            .problem
            .inputs
            .iter()
            .chain(&request.problem.outputs)
            .any(|endpoint| endpoint.id.len() > MAX_IDENTIFIER_BYTES)
        {
            return Err("Endpoint identifier exceeds 256 bytes.".into());
        }
        let prepared = request
            .problem
            .prepare()
            .map_err(|error| error.to_string())?;
        let options = RunOptions {
            mode: request.solve_mode,
            max_nodes,
            worker_count: 1,
        };
        let planner = ExactPlanner::new(&prepared.problem, options, Counts::Boolean)
            .map_err(|error| error.to_string())?;
        Ok(Self {
            prepared,
            mode: request.solve_mode,
            planner,
            leaves: BTreeMap::new(),
            snapshot: JobSnapshot::new(job_id.to_owned(), started_at_ms),
            elapsed: 0,
            last_progress: 0,
            terminal_sent: false,
        })
    }

    /// Drain live packets and dispatch work, or project a sealed mathematical outcome.
    /// A completion packet is only a proposal until the page retires this worker
    /// and accepts it. The page may still accept cancellation before that point.
    /// # Errors
    /// Rejects post-terminal use, broken dispatch contracts or presentation failures.
    pub fn poll(&mut self, elapsed_ms: u64) -> Result<String, String> {
        if self.terminal_sent {
            return Err("browser job already finished".into());
        }
        self.elapsed = self.elapsed.max(elapsed_ms);
        let mut update = self.planner.poll(self.elapsed);
        let mut dispatch = Vec::new();
        for task in update.dispatch {
            if !self.leaves.is_empty() {
                return Err("single-worker planner dispatched concurrent leaves".into());
            }
            let leaf = if task.root.impossible {
                None
            } else {
                Some(LeafDriver::new(
                    self.planner.context().ok_or("missing prepared leaf")?,
                    task.spec,
                ))
            };
            self.leaves.insert(task.id, leaf);
            dispatch.push(Dispatch {
                id: (task.id.group, task.id.index),
                impossible: task.root.impossible,
            });
        }
        if self.elapsed.saturating_sub(self.last_progress) >= 100 && self.planner.result().is_none()
        {
            update.events.push(self.planner.progress(self.elapsed));
        }
        let mut packets = Vec::new();
        for event in update.events {
            self.project_event(event, &mut packets)?;
        }
        let recovery = self.recovery()?;
        if let Some(outcome) = self.planner.outcome() {
            if !self.leaves.is_empty() {
                return Err("completed browser job retained a leaf".into());
            }
            project_outcome(&mut self.snapshot, self.mode, &self.prepared, &outcome)
                .map_err(|error| error.to_string())?;
            self.snapshot.finish_update();
            packets.push(self.snapshot.clone());
            self.terminal_sent = true;
        }
        encode(&Update {
            dispatch,
            packets,
            recovery,
            done: self.terminal_sent,
        })
    }

    /// Process one reply. The host must publish any accepted witness with `poll`
    /// and await the page's receipt before executing the next blocking command.
    /// # Errors
    /// Rejects unknown/retired leaves and malformed or inconclusive backend replies.
    // wasm-bindgen's optional string ABI requires ownership.
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
        encode(&protocol::Action {
            commands: action.commands,
            witnessed,
            completion: action.completion.map(|result| match result {
                Completion::Exhausted => "exhausted",
                Completion::Optimum => "optimum",
            }),
        })
    }

    /// Retire a leaf only after disposing its cvc5 session, including error paths.
    /// # Errors
    /// Rejects stale IDs, invalid verdicts and invalid proof transitions.
    pub fn retire(&mut self, id: &str, verdict: &str, detail: &str) -> Result<(), String> {
        let id = leaf_id(id)?;
        let result = match verdict {
            "exhausted" => Ok(Completion::Exhausted),
            "optimum" => Ok(Completion::Optimum),
            "failed" => Err(Failure::Worker(detail.chars().take(2048).collect())),
            _ => return Err("unknown browser retirement verdict".into()),
        };
        if self.leaves.remove(&id).is_none() {
            return Err("unknown or retired browser leaf".into());
        }
        self.planner
            .accept(PlannerEvent::Retired(id, result), self.elapsed)
            .map_err(|error| error.to_string())
    }
}

impl BrowserRun {
    fn project_event(
        &mut self,
        event: SolverEvent,
        packets: &mut Vec<JobSnapshot>,
    ) -> Result<(), String> {
        match event {
            SolverEvent::Progress(progress) => {
                self.last_progress = self.elapsed;
                packets.push(self.snapshot.update_progress(progress));
            }
            SolverEvent::Incumbent(mut best) if self.mode == SolveMode::OneMinNL => {
                best.canonical_graph_key =
                    solver_validation::layout_key(&self.prepared.problem, &best.graph);
                self.snapshot.result = Some(
                    Solution::from_best(&self.prepared, &best)
                        .map_err(|error| error.to_string())?,
                );
                self.snapshot.finish_update();
                packets.push(self.snapshot.clone());
            }
            SolverEvent::SolutionFound(mut best) => {
                best.canonical_graph_key =
                    solver_validation::layout_key(&self.prepared.problem, &best.graph);
                let solution = Solution::from_best(&self.prepared, &best)
                    .map_err(|error| error.to_string())?;
                if let Some(packet) = self.snapshot.append_solution(solution) {
                    packets.push(packet);
                }
            }
            SolverEvent::Incumbent(_) => {}
        }
        Ok(())
    }

    fn recovery(&self) -> Result<Recovery, String> {
        recovery(
            &self.snapshot,
            self.mode,
            self.planner.best(),
            self.planner.proof(),
        )
    }
}
