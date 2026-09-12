use crate::protocol::{
    Dispatch, Event, IndexedSolution, LeafWork, Options, Scheduling, Strategy, Update, encode,
    recovery,
};
use solver_api::{
    BestKnownSolution, CanonicalGraphKey, Diagnostic, IncompleteReason, IncompleteResult,
    OptimalSolution, PreparedProblem, RunOptions, SolveMode, SolveOutcome, SolveResult,
    SolverEvent,
};
use solver_core::{
    Counts, Failure,
    leaf::Completion,
    planner::{ExactPlanner, LeafId, PlannerEvent},
};
use std::collections::BTreeMap;
use synthetizer_app::{
    jobs::{JobSnapshot, project_outcome},
    solution::Solution,
};

struct Active {
    branch: usize,
    leaf: LeafId,
    stopping: bool,
}

/// A separate coordinator worker owns the independent proof ledgers. The page
/// owns compute workers and graph storage, and confirms retirement before events.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
pub struct BrowserCoordinator {
    prepared: PreparedProblem,
    mode: SolveMode,
    options: Options,
    branches: Vec<ExactPlanner>,
    active: BTreeMap<String, Active>,
    snapshot: JobSnapshot,
    keys: BTreeMap<CanonicalGraphKey, usize>,
    best: Option<BestKnownSolution>,
    pending: Vec<IndexedSolution>,
    winner: Option<usize>,
    interruption: Option<IncompleteReason>,
    result_changed: bool,
    terminal: bool,
    elapsed: u64,
    scheduling: Scheduling,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
impl BrowserCoordinator {
    /// Prepare an exact application request and a bounded independent-worker budget.
    /// # Errors
    /// Rejects invalid requests, zero budgets and unsupported browser options.
    #[cfg_attr(
        target_arch = "wasm32",
        wasm_bindgen::prelude::wasm_bindgen(constructor)
    )]
    pub fn new(source: &str, id: &str, started_at_ms: u64, options: &str) -> Result<Self, String> {
        let request = crate::request(source, id)?;
        let prepared = request
            .problem
            .prepare()
            .map_err(|error| error.to_string())?;
        if options.len() > 4096 {
            return Err("browser options exceed 4 KiB".into());
        }
        let options: Options = serde_json::from_str(options).map_err(|error| error.to_string())?;
        // The page limits this budget to navigator.hardwareConcurrency. Keep
        // Rust's wire range portable without imposing a smaller desktop cap.
        if options.worker_count == 0
            || u32::try_from(options.worker_count).is_err()
            || options.max_layouts == 0
            || options.max_identity_bytes == 0
        {
            return Err("browser workers must be a positive 32-bit count; collection budgets must be positive".into());
        }
        let budgets = match options.strategy {
            Strategy::Portfolio if options.worker_count > 1 => vec![
                (Counts::Sparse, options.worker_count.div_ceil(2)),
                (Counts::Boolean, options.worker_count / 2),
            ],
            Strategy::Sparse => vec![(Counts::Sparse, options.worker_count)],
            Strategy::Portfolio | Strategy::Boolean => {
                vec![(Counts::Boolean, options.worker_count)]
            }
        };
        let mut branches = Vec::new();
        for &(counts, worker_count) in &budgets {
            branches.push(
                ExactPlanner::streaming(
                    &prepared.problem,
                    RunOptions {
                        mode: request.solve_mode,
                        max_nodes: options.max_nodes,
                        worker_count,
                    },
                    counts,
                )
                .map_err(|error| error.to_string())?,
            );
        }
        Ok(Self {
            prepared,
            mode: request.solve_mode,
            options,
            branches,
            active: BTreeMap::new(),
            snapshot: JobSnapshot::new(id.to_owned(), started_at_ms),
            keys: BTreeMap::new(),
            best: None,
            pending: Vec::new(),
            winner: None,
            interruption: None,
            result_changed: true,
            terminal: false,
            elapsed: 0,
            scheduling: Scheduling {
                budgets: budgets.iter().map(|(_, count)| *count).collect(),
                ..Scheduling::default()
            },
        })
    }

    /// Accept one worker event. Retirement requires prior session disposal or
    /// page-owned worker termination. This is not a public witness import API.
    /// # Errors
    /// Rejects malformed, foreign, duplicate and post-seal events.
    pub fn accept(&mut self, source: &str, elapsed_ms: u64) -> Result<(), String> {
        if self.terminal {
            return Err("browser coordinator already finished".into());
        }
        let result = self.accept_event(source, elapsed_ms);
        if let Err(detail) = &result {
            self.interrupt(IncompleteReason::WorkerFailed {
                detail: detail.clone(),
            });
        }
        result
    }

    fn accept_event(&mut self, source: &str, elapsed_ms: u64) -> Result<(), String> {
        if source.len() > crate::MAX_EVENT_BYTES {
            return Err("browser event exceeds 16 MiB".into());
        }
        self.elapsed = self.elapsed.max(elapsed_ms);
        let event: Event = serde_json::from_str(source).map_err(|error| error.to_string())?;
        match event {
            Event::Interrupt { reason, detail } => {
                self.interrupt(match reason.as_str() {
                    "cancelled" => IncompleteReason::Cancelled,
                    "resource_limit" => IncompleteReason::ResourceLimit { detail },
                    "failed" => IncompleteReason::WorkerFailed { detail },
                    _ => return Err("invalid browser interruption reason".into()),
                });
            }
            Event::Witness { id, mut witness } => {
                let active = self
                    .active
                    .get(&id)
                    .ok_or("unknown or retired browser leaf")?;
                let validation =
                    solver_validation::validate_solution(&self.prepared.problem, &witness.graph)
                        .map_err(|error| error.to_string())?;
                if validation != witness.validation
                    || (
                        witness.node_count,
                        witness.link_count,
                        witness.physical_link_count,
                        witness.discard_link_count,
                    ) != (
                        validation.node_count,
                        validation.link_count,
                        validation.physical_link_count,
                        validation.discard_link_count,
                    )
                {
                    return Err("inconsistent browser witness metadata".into());
                }
                // Every branch and the paged collection use the restored caller identity.
                witness.canonical_graph_key =
                    solver_validation::layout_key(&self.prepared.problem, &witness.graph);
                self.branches[active.branch]
                    .accept(PlannerEvent::Witness(active.leaf, *witness), self.elapsed)
                    .map_err(|error| error.to_string())?;
            }
            Event::Retired {
                id,
                verdict,
                detail,
            } => {
                let result = match verdict.as_str() {
                    "exhausted" => Ok(Completion::Exhausted),
                    "optimum" => Ok(Completion::Optimum),
                    "cancelled" => Err(Failure::Cancelled),
                    "failed" => Err(Failure::Worker(detail)),
                    _ => return Err("invalid browser leaf verdict".into()),
                };
                let active = self
                    .active
                    .remove(&id)
                    .ok_or("unknown or retired browser leaf")?;
                // A failed leaf poisons only its branch. Another independent
                // branch may still establish the whole requested proof.
                let _ = self.branches[active.branch]
                    .accept(PlannerEvent::Retired(active.leaf, result), self.elapsed);
            }
        }
        Ok(())
    }

    /// Return bounded live deltas, dispatches and explicit retirement requests.
    /// All graphs leave this coordinator after the page acknowledges their batch.
    /// # Errors
    /// Returns broken coverage, projection or protocol contracts.
    pub fn poll(&mut self, elapsed_ms: u64) -> Result<String, String> {
        if self.terminal {
            return Err("browser coordinator already finished".into());
        }
        self.elapsed = self.elapsed.max(elapsed_ms);
        let dispatch = self.poll_branches()?;
        if let Some(winner) = self.winner {
            for (branch, planner) in self.branches.iter_mut().enumerate() {
                if branch != winner {
                    planner.cancel();
                }
            }
        }
        let stop = self
            .active
            .iter_mut()
            .filter_map(|(id, active)| {
                if self.branches[active.branch].stopping() && !active.stopping {
                    active.stopping = true;
                    Some(id.clone())
                } else {
                    None
                }
            })
            .collect();
        self.scheduling.active = self.active.len();
        self.scheduling.peak_active = self.scheduling.peak_active.max(self.active.len());
        if self.active.len() > self.options.worker_count {
            return Err("browser worker budget exceeded".into());
        }
        let owner = self.winner.unwrap_or(0);
        self.update_progress(owner);
        let packet = if self.result_changed {
            self.result_changed = false;
            self.snapshot.finish_update();
            self.snapshot.clone()
        } else {
            self.snapshot.update_progress(
                self.snapshot
                    .progress
                    .clone()
                    .ok_or("missing browser progress")?,
            )
        };
        let mut packets = vec![packet];
        let recovery = recovery(
            &self.snapshot,
            self.mode,
            self.best.as_ref(),
            self.branches[owner].proof(),
        )?;
        if self.active.is_empty()
            && self
                .branches
                .iter()
                .all(|planner| planner.result().is_some())
        {
            let result = self.result(owner)?;
            // Only scalar proof/result projection uses this temporary outcome.
            // Enumeration graphs are in the host collection, never this Vec.
            let outcome = SolveOutcome::new(result, self.mode, Vec::new());
            project_outcome(&mut self.snapshot, self.mode, &self.prepared, &outcome)
                .map_err(|error| error.to_string())?;
            self.snapshot.finish_update();
            packets.push(self.snapshot.clone());
            self.terminal = true;
            self.scheduling.proof_owner = Some(owner);
        }
        let preferred_index = self
            .best
            .as_ref()
            .and_then(|best| self.keys.get(&best.canonical_graph_key))
            .copied();
        encode(&Update {
            dispatch,
            stop,
            packets,
            append: std::mem::take(&mut self.pending),
            count: self.keys.len(),
            preferred_index,
            recovery,
            done: self.terminal,
            scheduling: Scheduling {
                dispatched: self.scheduling.dispatched,
                second_output_roots: self.scheduling.second_output_roots,
                adaptive_groups: self.scheduling.adaptive_groups,
                adaptive_children: self.scheduling.adaptive_children,
                peak_active: self.scheduling.peak_active,
                active: self.scheduling.active,
                identity_bytes: self.scheduling.identity_bytes,
                proof_owner: self.scheduling.proof_owner,
                budgets: self.scheduling.budgets.clone(),
            },
        })
    }
}

impl BrowserCoordinator {
    fn poll_branches(&mut self) -> Result<Vec<Dispatch>, String> {
        let mut dispatch = Vec::new();
        for branch in 0..self.branches.len() {
            let update = self.branches[branch].poll(self.elapsed);
            for event in update.events {
                self.event(branch, event)?;
            }
            for group in update.closed_groups {
                if group.adaptive {
                    self.scheduling.adaptive_groups += 1;
                    self.scheduling.adaptive_children += group
                        .jobs
                        .iter()
                        .filter(|job| job.trigger_ms.is_some())
                        .count();
                }
            }
            for task in update.dispatch {
                let id = format!("{branch}:{}:{}", task.id.group, task.id.index);
                if task.root.impossible {
                    self.branches[branch]
                        .accept(
                            PlannerEvent::Retired(task.id, Ok(Completion::Exhausted)),
                            self.elapsed,
                        )
                        .map_err(|error| error.to_string())?;
                } else {
                    let source = encode(&LeafWork::new(self.prepared.problem.clone(), task.spec))?;
                    self.active.insert(
                        id.clone(),
                        Active {
                            branch,
                            leaf: task.id,
                            stopping: false,
                        },
                    );
                    self.scheduling.dispatched += 1;
                    self.scheduling.second_output_roots +=
                        usize::from(task.root.second_source.is_some());
                    dispatch.push(Dispatch { id, branch, source });
                }
            }
            if self.winner.is_none()
                && self.interruption.is_none()
                && self.branches[branch]
                    .result()
                    .is_some_and(|result| !matches!(result, SolveResult::Incomplete(_)))
            {
                self.winner = Some(branch);
            }
        }
        Ok(dispatch)
    }

    fn update_progress(&mut self, owner: usize) {
        if let SolverEvent::Progress(mut progress) = self.branches[owner].progress(self.elapsed) {
            progress.solutions_found = self.keys.len() as u64;
            progress.best_node_count = self.best.as_ref().map(|best| best.node_count);
            progress.best_link_count = self.best.as_ref().map(|best| best.link_count);
            progress.custom.push(Diagnostic::counter(
                "solver.browser_workers",
                "Browser compute workers",
                self.options.worker_count,
            ));
            progress.custom.push(Diagnostic::counter(
                "solver.browser_active",
                "Active browser leaves",
                self.active.len(),
            ));
            progress.custom.push(Diagnostic::counter(
                "solver.portfolio_branch",
                "Independent proof branch",
                owner,
            ));
            self.snapshot.progress = Some(progress);
        }
    }

    fn interrupt(&mut self, reason: IncompleteReason) {
        self.interruption.get_or_insert(reason);
        for branch in &mut self.branches {
            branch.cancel();
        }
    }

    fn event(&mut self, _branch: usize, event: SolverEvent) -> Result<(), String> {
        match event {
            SolverEvent::Incumbent(best) => {
                if self.best.as_ref().is_none_or(|prior| {
                    (best.node_count, best.link_count) < (prior.node_count, prior.link_count)
                }) {
                    self.snapshot.result = Some(
                        Solution::from_best(&self.prepared, &best)
                            .map_err(|error| error.to_string())?,
                    );
                    self.best = Some(best);
                    self.result_changed = true;
                }
            }
            SolverEvent::SolutionFound(best) => {
                if !self.keys.contains_key(&best.canonical_graph_key) {
                    let solution = Solution::from_best(&self.prepared, &best)
                        .map_err(|error| error.to_string())?;
                    let index = self.keys.len();
                    self.scheduling.identity_bytes += best.canonical_graph_key.as_bytes().len();
                    self.keys.insert(best.canonical_graph_key, index);
                    self.pending.push(IndexedSolution { index, solution });
                    if self.keys.len() >= self.options.max_layouts
                        || self.scheduling.identity_bytes >= self.options.max_identity_bytes
                    {
                        self.interrupt(IncompleteReason::ResourceLimit { detail: format!("Browser collection budget reached: {} layouts or {} identity bytes. Accepted layouts were kept; enumeration is incomplete.", self.options.max_layouts, self.options.max_identity_bytes) });
                    }
                }
            }
            SolverEvent::Progress(_) => {}
        }
        Ok(())
    }

    fn result(&self, owner: usize) -> Result<SolveResult, String> {
        let result = self.branches[owner]
            .public_result()
            .ok_or("missing browser proof owner")?;
        let proof = match &result {
            SolveResult::Optimal(value) => &value.proof,
            SolveResult::Incomplete(value) => &value.proof,
            SolveResult::GloballyUnsat(value) => &value.proof,
        };
        if let Some(reason) = &self.interruption {
            return Ok(SolveResult::Incomplete(IncompleteResult {
                reason: reason.clone(),
                best_known: self.best.clone(),
                proof: proof.clone(),
            }));
        }
        match result {
            SolveResult::Optimal(mut optimal) => {
                // Every branch key enters the global union before publication.
                // The owner's set is therefore a subset; equal cardinality
                // establishes set equality, without combining proof ledgers.
                if self.mode != SolveMode::OneMinNL
                    && self.keys.len() != self.branches[owner].layout_count()
                {
                    return Err(
                        "complete browser owner disagrees with the collected layout set".into(),
                    );
                }
                if let Some(best) = &self.best {
                    if (best.node_count, best.link_count)
                        != (optimal.node_count, optimal.link_count)
                    {
                        return Err("browser proof owner contradicts a validated witness".into());
                    }
                    optimal = OptimalSolution {
                        node_count: best.node_count,
                        link_count: best.link_count,
                        physical_link_count: best.physical_link_count,
                        discard_link_count: best.discard_link_count,
                        canonical_graph_key: best.canonical_graph_key.clone(),
                        graph: best.graph.clone(),
                        validation: best.validation.clone(),
                        proof: optimal.proof,
                    };
                }
                Ok(SolveResult::Optimal(optimal))
            }
            SolveResult::Incomplete(mut incomplete) => {
                incomplete.best_known.clone_from(&self.best);
                Ok(SolveResult::Incomplete(incomplete))
            }
            SolveResult::GloballyUnsat(proof) => {
                if self.best.is_some() {
                    return Err("global contradiction conflicts with a validated witness".into());
                }
                Ok(SolveResult::GloballyUnsat(proof))
            }
        }
    }
}
