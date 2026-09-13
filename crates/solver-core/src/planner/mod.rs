//! Exact objective and root planning without threads, processes, atomics or clocks.
//! Each instance owns one branch's proof ledger. Hosts deliver events, execute
//! dispatched leaves, and retire all active backend sessions before completion.
mod group;
mod roots;
#[cfg(test)]
mod tests;
use crate::{
    Counts, Failure, Preparation,
    leaf::{Completion, LeafContext, LeafSpec},
    lower_bound::baseline_lower_bounds,
    prepare_problem,
    profile::{AccountedProfileGroup, enumerate_accounted_profile_groups},
};
use group::Group;
pub use roots::Root;
use solver_api::{
    BestKnownSolution, CanonicalGraphKey, Diagnostic, IncompleteReason, IncompleteResult,
    LinkConstraint, OptimalSolution, Problem, ProofSummary, RunOptions, SolveMode, SolveOutcome,
    SolvePhase, SolveResult, SolverError, SolverEvent, SolverProgress,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct LeafId {
    pub group: u64,
    pub index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LeafTask {
    pub id: LeafId,
    pub nodes: u32,
    pub links: u32,
    pub root: Root,
    pub spec: LeafSpec,
}

pub enum PlannerEvent {
    /// Trusted output from the exact leaf driver, not an imported proof claim.
    Witness(LeafId, BestKnownSolution),
    /// The session is already disposed; this is not merely a worker's SAT packet.
    Retired(LeafId, Result<Completion, Failure>),
}

pub struct JobEvidence {
    pub index: usize,
    pub parent: usize,
    pub child_count: usize,
    pub committed: bool,
    pub trigger_ms: Option<u64>,
}

pub struct GroupEvidence {
    pub group: u64,
    pub nodes: u32,
    pub links: u32,
    pub adaptive: bool,
    pub jobs: Vec<JobEvidence>,
}

#[derive(Default)]
pub struct PlannerUpdate {
    pub dispatch: Vec<LeafTask>,
    /// Internal branch events. As in native solving, the host must establish
    /// caller-scale public identities before exposing these through the app API.
    pub events: Vec<SolverEvent>,
    pub closed_groups: Vec<GroupEvidence>,
}

pub struct ExactPlanner {
    context: Option<LeafContext>,
    options: RunOptions,
    counts: Counts,
    proof: ProofSummary,
    best: Option<BestKnownSolution>,
    layouts: BTreeMap<CanonicalGraphKey, BestKnownSolution>,
    streamed_keys: Option<BTreeSet<CanonicalGraphKey>>,
    node: u32,
    groups: VecDeque<AccountedProfileGroup>,
    loaded_node: bool,
    group: Option<Group>,
    next_group: u64,
    minimum_links_ms: Option<u64>,
    initial_result: Option<SolveResult>,
    initial_progress: bool,
    cancelled: bool,
    failure: Option<IncompleteReason>,
    sealed: Option<SolveResult>,
    pending: PlannerUpdate,
}

impl ExactPlanner {
    /// Create one independent exact branch. Portfolio ownership is a host choice;
    /// never add the partial proof counters of different instances.
    /// # Errors
    /// Returns invalid request/options; sound-bound resource failures are retained
    /// as incomplete results for the first poll.
    pub fn new(
        problem: &Problem,
        options: RunOptions,
        counts: Counts,
    ) -> Result<Self, SolverError> {
        problem.validate()?;
        if options.worker_count == 0 {
            return Err(SolverError::InvalidOptions(
                "worker count must be positive".into(),
            ));
        }
        let prepared =
            prepare_problem(problem).map_err(|e| SolverError::InvalidProblem(e.to_string()))?;
        let (context, initial_result) = match prepared {
            Preparation::Prepared(normalized) => {
                (Some(LeafContext::new(problem.clone(), normalized)), None)
            }
            Preparation::GloballyUnsat(result) => (None, Some(SolveResult::GloballyUnsat(result))),
        };
        let mut planner = Self {
            proof: ProofSummary {
                proof_version: u32::from(context.is_some()),
                ..ProofSummary::default()
            },
            context,
            options,
            counts,
            best: None,
            layouts: BTreeMap::new(),
            streamed_keys: None,
            node: 0,
            groups: VecDeque::new(),
            loaded_node: false,
            group: None,
            next_group: 0,
            minimum_links_ms: None,
            initial_result,
            initial_progress: true,
            cancelled: false,
            failure: None,
            sealed: None,
            pending: PlannerUpdate::default(),
        };
        if let Some(context) = &planner.context {
            match baseline_lower_bounds(&context.normalized) {
                Ok(bound) => {
                    planner.node = bound.combined_nodes;
                    planner.proof.initial_node_lower_bound = bound.combined_nodes;
                }
                Err(error) => {
                    planner.failure = Some(IncompleteReason::ResourceLimit {
                        detail: error.to_string(),
                    });
                }
            }
        }
        Ok(planner)
    }

    /// Keep deduplication keys but hand graph storage to the host. This planner
    /// exposes a sealed result, not an in-memory complete collection.
    /// # Errors
    /// Returns the same preparation errors as `new`.
    pub fn streaming(
        problem: &Problem,
        options: RunOptions,
        counts: Counts,
    ) -> Result<Self, SolverError> {
        let mut planner = Self::new(problem, options, counts)?;
        planner.streamed_keys = Some(BTreeSet::new());
        Ok(planner)
    }

    #[must_use]
    pub fn layout_count(&self) -> usize {
        self.streamed_keys
            .as_ref()
            .map_or(self.layouts.len(), BTreeSet::len)
    }

    #[must_use]
    pub fn context(&self) -> Option<&LeafContext> {
        self.context.as_ref()
    }

    #[must_use]
    pub fn best(&self) -> Option<&BestKnownSolution> {
        self.best.as_ref()
    }

    #[must_use]
    pub fn proof(&self) -> &ProofSummary {
        &self.proof
    }

    /// Accept cancellation until the explicit completion-sealing poll. A later
    /// host/job cancellation can still override presentation, outside this branch.
    pub fn cancel(&mut self) -> bool {
        if self.sealed.is_some() {
            return false;
        }
        self.cancelled = true;
        true
    }

    /// Poison an unfinished branch after a host transport/scheduling failure.
    pub fn fail(&mut self, failure: Failure) {
        if self.sealed.is_some() {
            return;
        }
        match failure {
            Failure::Cancelled => self.cancelled = true,
            Failure::Worker(detail) => {
                self.failure = Some(IncompleteReason::WorkerFailed { detail });
            }
        }
    }

    /// Deliver a witness or retired leaf result. No callback needs Send or Sync.
    /// # Errors
    /// Rejects stale/duplicate events, inconsistent objectives and invalid proof
    /// transitions. A rejected event poisons an unfinished branch, not its ledger.
    pub fn accept(&mut self, event: PlannerEvent, elapsed_ms: u64) -> Result<(), Failure> {
        if self.sealed.is_some() {
            return Err(Failure::Worker("event after completion sealing".into()));
        }
        let result = self.apply(event, elapsed_ms);
        if let Err(error) = &result {
            self.fail(error.clone());
        }
        result
    }

    fn apply(&mut self, event: PlannerEvent, elapsed_ms: u64) -> Result<(), Failure> {
        let stopping = self.cancelled || self.failure.is_some();
        let group = self
            .group
            .as_mut()
            .ok_or_else(|| Failure::Worker("leaf event without an active group".into()))?;
        match event {
            PlannerEvent::Retired(id, result) => {
                group.retire(id, result, stopping, &mut self.proof)
            }
            PlannerEvent::Witness(id, solution) => {
                group.require_running(id)?;
                if solution.node_count != group.nodes || solution.link_count != group.links {
                    return Err(Failure::Worker(
                        "witness does not belong to the current objective group".into(),
                    ));
                }
                group.witness(id, elapsed_ms)?;
                if self.best.as_ref().is_none_or(|best| {
                    (solution.node_count, solution.link_count) < (best.node_count, best.link_count)
                }) {
                    self.best = Some(solution.clone());
                    self.pending
                        .events
                        .push(SolverEvent::Incumbent(solution.clone()));
                }
                if self.options.mode != SolveMode::OneMinNL
                    && !self.layouts.contains_key(&solution.canonical_graph_key)
                    && !self
                        .streamed_keys
                        .as_ref()
                        .is_some_and(|keys| keys.contains(&solution.canonical_graph_key))
                {
                    if let Some(keys) = &mut self.streamed_keys {
                        keys.insert(solution.canonical_graph_key.clone());
                    } else {
                        self.layouts
                            .insert(solution.canonical_graph_key.clone(), solution.clone());
                    }
                    self.pending
                        .events
                        .push(SolverEvent::SolutionFound(solution));
                }
                Ok(())
            }
        }
    }

    /// Compute commands from current events. Elapsed time is host data only.
    /// Completion is sealed here, and only after every dispatched leaf retires.
    #[must_use]
    pub fn poll(&mut self, elapsed_ms: u64) -> PlannerUpdate {
        if self.sealed.is_none() {
            self.initial_events(elapsed_ms);
            self.advance(elapsed_ms);
        }
        std::mem::take(&mut self.pending)
    }

    fn initial_events(&mut self, elapsed_ms: u64) {
        if !self.initial_progress {
            return;
        }
        self.initial_progress = false;
        if self.context.is_some() {
            self.pending.events.push(self.progress_at(
                None,
                None,
                SolvePhase::ComputingLowerBound,
                elapsed_ms,
                None,
            ));
            self.pending.events.push(self.progress_at(
                Some(self.node),
                None,
                SolvePhase::ComputingLowerBound,
                elapsed_ms,
                None,
            ));
        }
    }

    fn advance(&mut self, elapsed_ms: u64) {
        loop {
            if self.cancelled || self.failure.is_some() {
                if self.group.as_ref().is_none_or(|group| group.active == 0) {
                    self.close_group();
                    let reason = if self.cancelled {
                        IncompleteReason::Cancelled
                    } else {
                        self.failure.clone().expect("failure checked")
                    };
                    self.sealed = Some(self.incomplete(reason));
                }
                return;
            }
            if let Some(result) = self.initial_result.take() {
                self.sealed = Some(result);
                return;
            }
            if let Some(group) = &self.group {
                if group.active == 0 && (group.optimum || group.covered()) {
                    if !self.finish_group(elapsed_ms) {
                        return;
                    }
                    continue;
                }
                if group.optimum || group.covered() {
                    return;
                }
                let group = self.group.as_mut().expect("active group");
                self.pending.dispatch.extend(group.dispatch());
                if group.active == 0 {
                    self.fail(Failure::Worker("unfinished Solver profile group".into()));
                    continue;
                }
                return;
            }
            if !self.loaded_node && !self.load_node() {
                continue;
            }
            if let Some(group) = self.groups.pop_front() {
                let context = self.context.as_ref().expect("prepared search");
                match Group::new(
                    self.next_group,
                    self.node,
                    group,
                    context,
                    self.options,
                    self.counts,
                    &self.proof,
                ) {
                    Ok(group) => {
                        self.pending.events.push(self.progress_at(
                            Some(self.node),
                            Some(group.links),
                            SolvePhase::Searching,
                            elapsed_ms,
                            None,
                        ));
                        self.group = Some(group);
                        if let Some(next) = self.next_group.checked_add(1) {
                            self.next_group = next;
                        } else {
                            self.fail(Failure::Worker("group identifier exhausted".into()));
                        }
                    }
                    Err(error) => self.fail(error),
                }
            } else {
                if self.best.is_some() {
                    self.seal_optimum();
                    return;
                }
                self.proof.node_counts_exhausted_through = Some(self.node);
                if let Some(next) = self.node.checked_add(1) {
                    self.node = next;
                    self.loaded_node = false;
                } else {
                    self.failure = Some(IncompleteReason::ResourceLimit {
                        detail: "public node index exhausted".into(),
                    });
                }
            }
        }
    }

    fn load_node(&mut self) -> bool {
        if self.options.max_nodes.is_some_and(|cap| self.node > cap) {
            self.failure = Some(IncompleteReason::ResourceLimit {
                detail: "maximum node count exhausted".into(),
            });
            return false;
        }
        let context = self.context.as_ref().expect("prepared search");
        match enumerate_accounted_profile_groups(
            self.node,
            u32::try_from(context.original.inputs.len()).expect("prepared inputs"),
            u32::try_from(context.original.outputs.len()).expect("prepared outputs"),
            &context.normalized.surplus,
            &context.normalized.max_link_rate,
        ) {
            Ok(groups) => {
                self.groups = groups.into();
                self.loaded_node = true;
                true
            }
            Err(error) => {
                self.failure = Some(IncompleteReason::ResourceLimit {
                    detail: error.to_string(),
                });
                false
            }
        }
    }

    fn finish_group(&mut self, elapsed_ms: u64) -> bool {
        let group = self.group.as_ref().expect("completed group");
        let optimum = group.optimum;
        let (nodes, links) = (group.nodes, group.links);
        if !optimum && !group.profile_count_matches(&self.proof) {
            self.fail(Failure::Worker("unfinished Solver profile group".into()));
            return true;
        }
        self.close_group();
        if optimum {
            self.seal_optimum();
            return false;
        }
        self.proof.link_groups_exhausted += 1;
        if self.best.is_some() {
            self.minimum_links_ms.get_or_insert(elapsed_ms);
            self.pending.events.push(self.progress_at(
                Some(nodes),
                Some(links),
                SolvePhase::Enumerating,
                elapsed_ms,
                None,
            ));
            if self.options.mode != SolveMode::AllMinN {
                self.seal_optimum();
                return false;
            }
        }
        true
    }

    fn close_group(&mut self) {
        if let Some(group) = self.group.take() {
            self.pending.closed_groups.push(group.evidence());
        }
    }

    #[must_use]
    pub fn stopping(&self) -> bool {
        self.cancelled
            || self.failure.is_some()
            || self
                .group
                .as_ref()
                .is_some_and(|group| group.optimum || group.covered())
    }

    #[must_use]
    pub fn result(&self) -> Option<&SolveResult> {
        self.sealed.as_ref()
    }

    /// Build a public outcome after sealing, restoring authoritative caller-scale
    /// identities. A host must still honor cancellation accepted before its own
    /// terminal presentation seal, as the native facade does.
    #[must_use]
    pub fn outcome(&self) -> Option<SolveOutcome> {
        if self.streamed_keys.is_some() {
            return None;
        }
        let mut outcome = SolveOutcome::new(
            self.sealed.clone()?,
            self.options.mode,
            self.layouts.values().cloned().collect(),
        );
        if let Some(context) = &self.context {
            solver_validation::normalize_outcome_identity(&context.original, &mut outcome);
        }
        outcome
            .solutions
            .sort_by(|left, right| left.canonical_graph_key.cmp(&right.canonical_graph_key));
        Some(outcome)
    }

    /// Return the sealed mathematical result with caller-scale identity, without
    /// claiming to contain the externally stored enumeration collection.
    #[must_use]
    pub fn public_result(&self) -> Option<SolveResult> {
        let mut single = SolveOutcome::new(self.sealed.clone()?, SolveMode::OneMinNL, Vec::new());
        if let Some(context) = &self.context {
            solver_validation::normalize_outcome_identity(&context.original, &mut single);
        }
        Some(single.result)
    }

    #[must_use]
    pub fn progress(&self, elapsed_ms: u64) -> SolverEvent {
        let links = self.group.as_ref().map(|group| group.links);
        self.progress_at(
            Some(self.node),
            links,
            SolvePhase::Searching,
            elapsed_ms,
            None,
        )
    }

    #[must_use]
    pub fn root_progress(
        &self,
        nodes: u32,
        links: u32,
        elapsed_ms: u64,
        trace: String,
    ) -> SolverEvent {
        self.progress_at(
            Some(nodes),
            Some(links),
            SolvePhase::Searching,
            elapsed_ms,
            Some(trace),
        )
    }

    fn progress_at(
        &self,
        nodes: Option<u32>,
        links: Option<u32>,
        phase: SolvePhase,
        elapsed_ms: u64,
        trace: Option<String>,
    ) -> SolverEvent {
        let mut custom = vec![Diagnostic::counter(
            "solver.profiles_exhausted",
            "Completed Solver profiles",
            self.proof.profiles_exhausted,
        )];
        if let Some(ms) = self.minimum_links_ms {
            custom.push(Diagnostic::counter(
                "solver.minimum_links_complete_ms",
                "Minimum-link enumeration completed",
                ms,
            ));
        }
        if let Some(trace) = trace {
            custom.push(Diagnostic::text(
                "solver.root",
                "Solver root evidence",
                trace,
            ));
        }
        SolverEvent::Progress(SolverProgress {
            phase,
            elapsed_ms,
            node_count: (phase != SolvePhase::ComputingLowerBound)
                .then_some(nodes)
                .flatten(),
            link_constraint: links.map(LinkConstraint::Exact),
            node_lower_bound: nodes,
            best_node_count: self.best.as_ref().map(|b| b.node_count),
            best_link_count: self.best.as_ref().map(|b| b.link_count),
            solutions_found: self.layout_count() as u64,
            custom,
        })
    }

    fn incomplete(&self, reason: IncompleteReason) -> SolveResult {
        SolveResult::Incomplete(IncompleteResult {
            reason,
            best_known: self.best.clone(),
            proof: self.proof.clone(),
        })
    }

    fn seal_optimum(&mut self) {
        let Some(best) = &self.best else {
            self.fail(Failure::Worker(
                "completed SAT result has no witness".into(),
            ));
            return;
        };
        self.sealed = Some(SolveResult::Optimal(OptimalSolution {
            node_count: best.node_count,
            link_count: best.link_count,
            physical_link_count: best.physical_link_count,
            discard_link_count: best.discard_link_count,
            canonical_graph_key: best.canonical_graph_key.clone(),
            graph: best.graph.clone(),
            validation: best.validation.clone(),
            proof: self.proof.clone(),
        }));
    }
}
