#[derive(Clone, Copy, PartialEq, Eq)]
enum AdaptiveChoice {
    Parent,
    Children,
}

struct AdaptiveLedger {
    base: RootLedger,
    choice: Vec<Option<AdaptiveChoice>>,
    children: Vec<Option<Vec<bool>>>,
}

impl AdaptiveLedger {
    fn new(roots: &[Root], profiles: usize) -> Self {
        Self {
            base: RootLedger::new(roots, profiles),
            choice: vec![None; roots.len()],
            children: vec![None; roots.len()],
        }
    }

    fn finish(
        &mut self,
        parent: usize,
        child: Option<usize>,
        proof: &mut ProofSummary,
    ) -> Result<(), Failure> {
        let choice = self
            .choice
            .get_mut(parent)
            .ok_or_else(|| Failure::Worker("unknown adaptive parent".into()))?;
        // A competing parent or child may finish after the other proof won.
        if choice.is_some() {
            return Ok(());
        }
        let (selected, units) = if let Some(child) = child {
            let children = self.children[parent]
                .as_mut()
                .ok_or_else(|| Failure::Worker("unregistered adaptive children".into()))?;
            let done = children
                .get_mut(child)
                .ok_or_else(|| Failure::Worker("unknown adaptive child".into()))?;
            if *done {
                return Err(Failure::Worker("duplicate adaptive child".into()));
            }
            *done = true;
            if !children.iter().all(|done| *done) {
                return Ok(());
            }
            (AdaptiveChoice::Children, children.len())
        } else {
            (AdaptiveChoice::Parent, 1)
        };
        self.base.finish(parent, proof)?;
        proof.root_partitions_exhausted += u64::try_from(units - 1).expect("root count fits u64");
        *choice = Some(selected);
        Ok(())
    }

    fn committed(&self, parent: usize, child: Option<usize>) -> bool {
        self.choice[parent]
            == Some(if child.is_some() {
                AdaptiveChoice::Children
            } else {
                AdaptiveChoice::Parent
            })
    }
}

#[derive(Clone, Copy)]
struct AdaptiveJob {
    root: Root,
    parent: usize,
    trigger_s: Option<f64>,
}

impl Search<'_> {
    /// Parents retain their running backend. Only a validated current-group witness
    /// enables children, which occupy spare worker slots. Either the original parent
    /// or every disjoint child discharges its obligation, never both.
    #[allow(clippy::too_many_lines)]
    fn adaptive_group(
        &mut self,
        nodes: u32,
        links: u32,
        tasks: &[AccountedProfile],
        parents: &[Root],
    ) -> Result<Completion, Failure> {
        use std::fmt::Write as _;
        let completed_before = self.proof.profiles_exhausted;
        let mut ledger = AdaptiveLedger::new(parents, tasks.len());
        let mut jobs: Vec<_> = parents
            .iter()
            .enumerate()
            .map(|(parent, &root)| AdaptiveJob {
                root,
                parent,
                trigger_s: None,
            })
            .collect();
        let mut queue: std::collections::VecDeque<_> = (0..jobs.len()).collect();
        let mut finished = BTreeSet::new();
        let mut traces = Vec::new();
        let mut errors = Vec::new();
        let mut active = 0;
        let mut gate = None;
        let stop = AtomicBool::new(false);
        let (send, receive) = mpsc::channel();
        let normalized = &self.normalized;
        let problem = &self.problem;
        let original = self.original;
        let cancel = self.cancel;
        let mode = self.options.mode;
        let counts = self.counts;
        let workers = self.options.worker_count;
        let origin = self.started;
        let diagnostics = std::env::var_os("SOLVER_DIAGNOSTICS").is_some_and(|v| v == "1");
        let mut optimum = false;
        thread::scope(|scope| {
            let mut handles = Vec::new();
            let mut last = Instant::now();
            loop {
                if cancel.load(Ordering::Relaxed) || ledger.base.complete() || !errors.is_empty() {
                    stop.store(true, Ordering::Relaxed);
                }
                while !stop.load(Ordering::Relaxed) && active < workers {
                    let Some(index) = queue.pop_front() else {
                        break;
                    };
                    let job = jobs[index];
                    if ledger.choice[job.parent].is_some() {
                        continue;
                    }
                    let stop = &stop;
                    let send = send.clone();
                    active += 1;
                    handles.push(scope.spawn(move || {
                        let mut stats = diagnostics.then(RootStats::default);
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            if job.root.impossible {
                                Ok(Completion::Exhausted)
                            } else {
                                profile(
                                    problem,
                                    original,
                                    normalized,
                                    tasks[job.root.profile],
                                    job.root.source,
                                    job.root.second_source,
                                    mode,
                                    counts,
                                    cancel,
                                    stop,
                                    &send,
                                    &mut stats,
                                )
                            }
                        }))
                        .unwrap_or_else(|_| {
                            Err(Failure::Worker("adaptive worker panicked".into()))
                        });
                        let trace = stats.map(|stats| {
                            let verdict = match &result {
                                Ok(Completion::Exhausted) => "exhausted",
                                Ok(Completion::Optimum) => "optimum",
                                Err(Failure::Cancelled) => "cancelled",
                                Err(Failure::Worker(_)) => "failed",
                            };
                            stats.record(index, nodes, links, &job.root, verdict, origin)
                        });
                        let _ = send.send(Message::Done(index, result, trace));
                    }));
                }
                if active == 0 {
                    break;
                }
                match receive.recv_timeout(Duration::from_millis(25)) {
                    Ok(message @ Message::Valid(_)) => {
                        let mut state = GroupState {
                            best: &mut self.best,
                            layouts: &mut self.layouts,
                            proof: &mut self.proof,
                            observer: self.observer,
                            mode,
                            ledger: &mut ledger.base,
                            optimum: &mut optimum,
                        };
                        if let Err(error) = state.accept(message) {
                            errors.push(error);
                        }
                        // All lower N/L obligations were exhausted before this group.
                        if self
                            .best
                            .as_ref()
                            .is_some_and(|b| b.node_count == nodes && b.link_count == links)
                        {
                            gate.get_or_insert_with(|| origin.elapsed().as_secs_f64());
                        }
                    }
                    Ok(Message::Done(index, result, trace)) => {
                        if !finished.insert(index) {
                            errors
                                .push(Failure::Worker("duplicate adaptive job completion".into()));
                            continue;
                        }
                        active -= 1;
                        let job = jobs[index];
                        if let Some(trace) = trace {
                            traces.push((index, trace));
                        }
                        match result {
                            Ok(Completion::Exhausted) => {
                                if let Err(error) = ledger.finish(
                                    job.parent,
                                    job.root.second_source,
                                    &mut self.proof,
                                ) {
                                    errors.push(error);
                                }
                            }
                            Ok(Completion::Optimum) => errors
                                .push(Failure::Worker("early stop in adaptive enumeration".into())),
                            Err(Failure::Cancelled)
                                if stop.load(Ordering::Relaxed)
                                    || cancel.load(Ordering::Relaxed) => {}
                            Err(error) => errors.push(error),
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        errors.push(Failure::Worker("adaptive channel disconnected".into()));
                        break;
                    }
                }
                // Never queue children ahead of unstarted parents. A spare slot is
                // required, and a completed parent immediately makes its queued children unnecessary.
                if let Some(trigger_s) = gate.filter(|_| active < workers && queue.is_empty()) {
                    for (parent, &root) in parents.iter().enumerate() {
                        if root.impossible
                            || root.source.is_none()
                            || ledger.choice[parent].is_some()
                            || ledger.children[parent].is_some()
                        {
                            continue;
                        }
                        let children = problem.inputs.len()
                            + tasks[root.profile].profile.node_count() as usize;
                        ledger.children[parent] = Some(vec![false; children]);
                        for source in 0..children {
                            jobs.push(AdaptiveJob {
                                root: Root {
                                    second_source: Some(source),
                                    ..root
                                },
                                parent,
                                trigger_s: Some(trigger_s),
                            });
                            queue.push_back(jobs.len() - 1);
                        }
                    }
                    // Give each parent one child per round rather than filling all
                    // spare slots with the children of the first parent.
                    queue
                        .make_contiguous()
                        .sort_by_key(|&index| jobs[index].root.second_source);
                }
                if last.elapsed() >= Duration::from_millis(250) {
                    let state = GroupState {
                        best: &mut self.best,
                        layouts: &mut self.layouts,
                        proof: &mut self.proof,
                        observer: self.observer,
                        mode,
                        ledger: &mut ledger.base,
                        optimum: &mut optimum,
                    };
                    state.progress(nodes, links, origin, None);
                    last = Instant::now();
                }
            }
            for handle in handles {
                if handle.join().is_err() {
                    errors.push(Failure::Worker("adaptive worker join failed".into()));
                }
            }
        });
        // Evidence is published after choosing the disjoint cover. Finished searches
        // outside that cover stay visible with proof_committed=false.
        for (index, mut trace) in traces {
            let job = jobs[index];
            let committed = ledger.committed(job.parent, job.root.second_source);
            let children = ledger.children[job.parent].as_ref().map_or(0, Vec::len);
            let trigger = job
                .trigger_s
                .map_or_else(|| "null".into(), |s| s.to_string());
            trace.pop();
            write!(trace, ",\"parent_root\":{},\"child_count\":{children},\"proof_committed\":{committed},\"refinement_trigger_s\":{trigger}}}", job.parent).expect("writing to a String cannot fail");
            let state = GroupState {
                best: &mut self.best,
                layouts: &mut self.layouts,
                proof: &mut self.proof,
                observer: self.observer,
                mode,
                ledger: &mut ledger.base,
                optimum: &mut optimum,
            };
            state.progress(nodes, links, origin, Some(trace));
        }
        if let Some(index) = errors.iter().position(|e| matches!(e, Failure::Worker(_))) {
            return Err(errors.swap_remove(index));
        }
        if cancel.load(Ordering::Relaxed) || !errors.is_empty() {
            return Err(Failure::Cancelled);
        }
        if !ledger.base.complete()
            || self.proof.profiles_exhausted - completed_before != tasks.len() as u64
        {
            return Err(Failure::Worker("unfinished adaptive profile group".into()));
        }
        Ok(Completion::Exhausted)
    }
}

#[cfg(test)]
mod adaptive_tests {
    use super::*;
    fn ledger() -> AdaptiveLedger {
        let roots = vec![Root {
            profile: 0,
            source: Some(0),
            second_source: None,
            impossible: false,
        }];
        let mut ledger = AdaptiveLedger::new(&roots, 1);
        ledger.children[0] = Some(vec![false; 3]);
        ledger
    }
    #[test]
    fn partial_children_never_exhaust_and_duplicate_children_fail() {
        let mut ledger = ledger();
        let mut proof = ProofSummary::default();
        ledger.finish(0, Some(0), &mut proof).unwrap();
        ledger.finish(0, Some(1), &mut proof).unwrap();
        assert!(!ledger.base.complete());
        assert_eq!(proof.root_partitions_exhausted, 0);
        assert!(ledger.finish(0, Some(1), &mut proof).is_err());
        assert!(ledger.finish(0, Some(9), &mut proof).is_err());
    }
    #[test]
    fn parent_and_complete_children_are_alternative_proofs() {
        for parent_first in [true, false] {
            let mut ledger = ledger();
            let mut proof = ProofSummary::default();
            ledger.finish(0, Some(0), &mut proof).unwrap();
            if parent_first {
                ledger.finish(0, None, &mut proof).unwrap();
            }
            ledger.finish(0, Some(1), &mut proof).unwrap();
            ledger.finish(0, Some(2), &mut proof).unwrap();
            ledger.finish(0, None, &mut proof).unwrap();
            assert!(ledger.base.complete());
            assert_eq!(proof.profiles_exhausted, 1);
            assert_eq!(
                proof.root_partitions_exhausted,
                if parent_first { 1 } else { 3 }
            );
            assert_eq!(ledger.committed(0, None), parent_first);
            assert_eq!(ledger.committed(0, Some(0)), !parent_first);
        }
    }
}
