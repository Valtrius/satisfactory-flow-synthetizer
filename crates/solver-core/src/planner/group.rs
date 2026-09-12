use super::roots::{AdaptiveLedger, RootLedger, refine_roots};
use super::{GroupEvidence, JobEvidence, LeafId, LeafTask, Root};
use crate::{
    Counts, Failure,
    leaf::{Completion, LeafContext, LeafSpec},
    lower_bound::profile_impossibility,
    profile::{AccountedProfile, AccountedProfileGroup},
};
use solver_api::{ProofSummary, RunOptions, SolveMode};
use std::collections::VecDeque;

enum Coverage {
    Static(RootLedger),
    Adaptive(AdaptiveLedger),
}

#[derive(Clone, Copy)]
enum JobState {
    Queued,
    Running,
    Retired(Option<Completion>),
    Skipped,
}

struct Job {
    root: Root,
    parent: usize,
    trigger_ms: Option<u64>,
    state: JobState,
    witnessed: bool,
}

pub(super) struct Group {
    pub id: u64,
    pub nodes: u32,
    pub links: u32,
    tasks: Vec<AccountedProfile>,
    parents: Vec<Root>,
    jobs: Vec<Job>,
    queue: VecDeque<usize>,
    coverage: Coverage,
    pub active: usize,
    gate_ms: Option<u64>,
    pub optimum: bool,
    completed_before: u64,
    inputs: usize,
    options: RunOptions,
    counts: Counts,
}

impl Group {
    pub fn new(
        id: u64,
        nodes: u32,
        group: AccountedProfileGroup,
        context: &LeafContext,
        options: RunOptions,
        counts: Counts,
        proof: &ProofSummary,
    ) -> Result<Self, Failure> {
        let AccountedProfileGroup {
            link_count: links,
            profiles: tasks,
        } = group;
        let mut roots = Vec::new();
        for (profile, task) in tasks.iter().enumerate() {
            let impossible =
                profile_impossibility(&context.normalized, task.profile, task.accounting).is_some();
            if !impossible && options.worker_count > 1 && !context.problem.outputs.is_empty() {
                let sources = context
                    .problem
                    .inputs
                    .len()
                    .checked_add(task.profile.node_count() as usize)
                    .ok_or_else(|| Failure::Worker("producer index overflow".into()))?;
                // Every owner remains an obligation, dispatched in descending order.
                for source in (0..sources).rev() {
                    roots.push(Root {
                        profile,
                        source: Some(source),
                        second_source: None,
                        impossible: false,
                    });
                }
            } else {
                roots.push(Root {
                    profile,
                    source: None,
                    second_source: None,
                    impossible,
                });
            }
        }
        let adaptive = options.mode == SolveMode::AllMinNL
            && counts == Counts::Boolean
            && options.worker_count > 1
            && context.problem.outputs.len() >= 2
            && roots.iter().filter(|root| !root.impossible).count() >= options.worker_count;
        if !adaptive && options.mode == SolveMode::AllMinNL && counts == Counts::Boolean {
            roots = refine_roots(
                roots,
                &tasks,
                context.problem.inputs.len(),
                context.problem.outputs.len(),
                options.worker_count,
            );
        }
        let coverage = if adaptive {
            Coverage::Adaptive(AdaptiveLedger::new(&roots, tasks.len()))
        } else {
            Coverage::Static(RootLedger::new(&roots, tasks.len()))
        };
        let jobs: Vec<_> = roots
            .iter()
            .enumerate()
            .map(|(parent, &root)| Job {
                root,
                parent,
                trigger_ms: None,
                state: JobState::Queued,
                witnessed: false,
            })
            .collect();
        Ok(Self {
            id,
            nodes,
            links,
            tasks,
            queue: (0..jobs.len()).collect(),
            jobs,
            parents: roots,
            coverage,
            active: 0,
            gate_ms: None,
            optimum: false,
            completed_before: proof.profiles_exhausted,
            inputs: context.problem.inputs.len(),
            options,
            counts,
        })
    }

    pub fn covered(&self) -> bool {
        match &self.coverage {
            Coverage::Static(ledger) => ledger.complete(),
            Coverage::Adaptive(ledger) => ledger.base.complete(),
        }
    }

    pub fn profile_count_matches(&self, proof: &ProofSummary) -> bool {
        proof.profiles_exhausted - self.completed_before == self.tasks.len() as u64
    }

    pub fn require_running(&self, id: LeafId) -> Result<(), Failure> {
        if id.group != self.id
            || !self
                .jobs
                .get(id.index)
                .is_some_and(|job| matches!(job.state, JobState::Running))
        {
            return Err(Failure::Worker(
                "unknown, stale or retired leaf event".into(),
            ));
        }
        Ok(())
    }

    pub fn witness(&mut self, id: LeafId, now_ms: u64) -> Result<(), Failure> {
        self.require_running(id)?;
        self.jobs[id.index].witnessed = true;
        self.gate_ms.get_or_insert(now_ms);
        Ok(())
    }

    /// Completion means the host has already disposed the leaf's backend session.
    pub fn retire(
        &mut self,
        id: LeafId,
        result: Result<Completion, Failure>,
        stopping: bool,
        proof: &mut ProofSummary,
    ) -> Result<(), Failure> {
        self.require_running(id)?;
        let job = &mut self.jobs[id.index];
        job.state = JobState::Retired(result.as_ref().ok().copied());
        self.active -= 1;
        match result {
            Ok(Completion::Exhausted) => match &mut self.coverage {
                Coverage::Static(ledger) => ledger.finish(id.index, proof),
                Coverage::Adaptive(ledger) => {
                    ledger.finish(job.parent, job.root.second_source, proof)
                }
            },
            Ok(Completion::Optimum)
                if self.options.mode == SolveMode::OneMinNL && job.witnessed =>
            {
                self.optimum = true;
                Ok(())
            }
            Ok(Completion::Optimum) => Err(Failure::Worker(
                "early stop without a validated single-optimum witness".into(),
            )),
            Err(Failure::Cancelled) if stopping || self.optimum || self.covered() => Ok(()),
            Err(error) => Err(error),
        }
    }

    pub fn dispatch(&mut self) -> Vec<LeafTask> {
        let mut tasks = Vec::new();
        if self.covered() || self.optimum {
            return tasks;
        }
        loop {
            while self.active < self.options.worker_count {
                let Some(index) = self.queue.pop_front() else {
                    break;
                };
                let job = &mut self.jobs[index];
                if let Coverage::Adaptive(ledger) = &self.coverage
                    && ledger.choice[job.parent].is_some()
                {
                    job.state = JobState::Skipped;
                    continue;
                }
                job.state = JobState::Running;
                self.active += 1;
                tasks.push(LeafTask {
                    id: LeafId {
                        group: self.id,
                        index,
                    },
                    nodes: self.nodes,
                    links: self.links,
                    root: job.root,
                    spec: LeafSpec {
                        profile: self.tasks[job.root.profile],
                        source: job.root.source,
                        second_source: job.root.second_source,
                        mode: self.options.mode,
                        counts: self.counts,
                    },
                });
            }
            // Unstarted parents stay ahead of every child. A validated witness and
            // spare slot are both necessary before introducing alternative covers.
            if self.active >= self.options.worker_count
                || !self.queue.is_empty()
                || !self.queue_children()
            {
                break;
            }
        }
        tasks
    }

    fn queue_children(&mut self) -> bool {
        let Some(trigger_ms) = self.gate_ms else {
            return false;
        };
        let Coverage::Adaptive(ledger) = &mut self.coverage else {
            return false;
        };
        for (parent, &root) in self.parents.iter().enumerate() {
            if root.impossible
                || root.source.is_none()
                || ledger.choice[parent].is_some()
                || ledger.children[parent].is_some()
            {
                continue;
            }
            let children = self.inputs + self.tasks[root.profile].profile.node_count() as usize;
            ledger.children[parent] = Some(vec![false; children]);
            for source in 0..children {
                self.jobs.push(Job {
                    root: Root {
                        second_source: Some(source),
                        ..root
                    },
                    parent,
                    trigger_ms: Some(trigger_ms),
                    state: JobState::Queued,
                    witnessed: false,
                });
                self.queue.push_back(self.jobs.len() - 1);
            }
        }
        self.queue
            .make_contiguous()
            .sort_by_key(|&index| self.jobs[index].root.second_source);
        !self.queue.is_empty()
    }

    pub fn evidence(&self) -> GroupEvidence {
        let adaptive = matches!(self.coverage, Coverage::Adaptive(_));
        let jobs = self
            .jobs
            .iter()
            .enumerate()
            .map(|(index, job)| {
                let (committed, child_count) = match &self.coverage {
                    Coverage::Static(_) => (
                        matches!(job.state, JobState::Retired(Some(Completion::Exhausted))),
                        0,
                    ),
                    Coverage::Adaptive(ledger) => (
                        ledger.committed(job.parent, job.root.second_source),
                        ledger.children[job.parent].as_ref().map_or(0, Vec::len),
                    ),
                };
                JobEvidence {
                    index,
                    parent: job.parent,
                    child_count,
                    committed,
                    trigger_ms: job.trigger_ms,
                }
            })
            .collect();
        GroupEvidence {
            group: self.id,
            nodes: self.nodes,
            links: self.links,
            adaptive,
            jobs,
        }
    }
}
