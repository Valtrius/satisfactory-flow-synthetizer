use solver_api::{
    BestKnownSolution, CanonicalGraphKey, Problem, SolveMode, SolveObserver, SolveOutcome,
    SolveResult, SolverEvent,
};
use std::{collections::BTreeMap, sync::Mutex};

use crate::{identity::normalize_result_identity, layout_key};

/// One collector used by production engines, with optional advisory live delivery.
pub struct SolutionCollector<'a> {
    problem: &'a Problem,
    observer: &'a dyn SolveObserver,
    state: Mutex<Collection>,
}

#[derive(Default)]
struct Collection {
    solutions: BTreeMap<CanonicalGraphKey, BestKnownSolution>,
    best: Option<(u32, u32)>,
}

impl<'a> SolutionCollector<'a> {
    #[must_use]
    pub fn new(problem: &'a Problem, observer: &'a dyn SolveObserver) -> Self {
        Self {
            problem,
            observer,
            state: Mutex::new(Collection::default()),
        }
    }

    /// # Panics
    /// Panics if a previous collector call poisoned its lock.
    #[must_use]
    pub fn finish(self, mut result: SolveResult, mode: SolveMode) -> SolveOutcome {
        // Delivered solutions already carry the public key for this problem.
        // The terminal witness may still carry a private search identity.
        normalize_result_identity(self.problem, &mut result);
        SolveOutcome::new(
            result,
            mode,
            self.state
                .into_inner()
                .expect("solution collector")
                .solutions
                .into_values()
                .collect(),
        )
    }
}

impl SolveObserver for SolutionCollector<'_> {
    fn on_event(&self, mut event: SolverEvent) {
        {
            let mut state = self.state.lock().expect("solution collector");
            match &mut event {
                SolverEvent::Incumbent(solution) => {
                    solution.canonical_graph_key = layout_key(self.problem, &solution.graph);
                    let objective = (solution.node_count, solution.link_count);
                    state.best = Some(state.best.map_or(objective, |best| best.min(objective)));
                }
                SolverEvent::SolutionFound(solution) => {
                    let objective = (solution.node_count, solution.link_count);
                    state.best = Some(state.best.map_or(objective, |best| best.min(objective)));
                    let key = layout_key(self.problem, &solution.graph);
                    solution.canonical_graph_key = key.clone();
                    match state.solutions.entry(key) {
                        std::collections::btree_map::Entry::Occupied(_) => return,
                        std::collections::btree_map::Entry::Vacant(entry) => {
                            entry.insert(solution.clone());
                        }
                    }
                }
                SolverEvent::Progress(progress) => {
                    progress.solutions_found =
                        u64::try_from(state.solutions.len()).unwrap_or(u64::MAX);
                    progress.best_node_count = state.best.map(|(n, _)| n);
                    progress.best_link_count = state.best.map(|(_, l)| l);
                }
            }
        }
        // Observer failure cannot invalidate a mathematical result or lose the collection.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.observer.on_event(event);
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solver_api::{
        ConsumerPortRef, IncompleteReason, IncompleteResult, InputTerminalIndex, OptimalSolution,
        OutputTerminalIndex, PhysicalGraph, PhysicalLink, ProducerPortRef, ProofSummary,
    };

    #[test]
    fn private_keys_are_normalized_before_delivery_and_for_terminal_witnesses() {
        let problem = Problem {
            inputs: vec!["1/3".parse().unwrap()],
            outputs: vec!["1/3".parse().unwrap()],
            max_link_rate: 1.into(),
        };
        let graph = PhysicalGraph {
            nodes: Vec::new(),
            links: vec![PhysicalLink {
                producer: ProducerPortRef::Input(InputTerminalIndex(0)),
                consumer: ConsumerPortRef::Output(OutputTerminalIndex(0)),
                flow: "1/3".parse().unwrap(),
            }],
        };
        let validation = crate::validate_solution(&problem, &graph).unwrap();
        let private = BestKnownSolution {
            node_count: validation.node_count,
            link_count: validation.link_count,
            physical_link_count: validation.physical_link_count,
            discard_link_count: validation.discard_link_count,
            canonical_graph_key: CanonicalGraphKey::default(),
            graph,
            validation,
        };
        let expected = layout_key(&problem, &private.graph);
        for cancelled in [false, true] {
            for mode in [SolveMode::OneMinNL, SolveMode::AllMinNL, SolveMode::AllMinN] {
                let delivered = Mutex::new(Vec::new());
                let observer = |event| match event {
                    SolverEvent::Incumbent(solution) | SolverEvent::SolutionFound(solution) => {
                        assert_eq!(solution.canonical_graph_key, expected);
                        delivered.lock().unwrap().push(solution);
                    }
                    SolverEvent::Progress(_) => {}
                };
                let collector = SolutionCollector::new(&problem, &observer);
                collector.on_event(SolverEvent::Incumbent(private.clone()));
                collector.on_event(SolverEvent::SolutionFound(private.clone()));
                collector.on_event(SolverEvent::SolutionFound(private.clone()));
                let result = if cancelled {
                    SolveResult::Incomplete(IncompleteResult {
                        reason: IncompleteReason::Cancelled,
                        best_known: Some(private.clone()),
                        proof: ProofSummary::default(),
                    })
                } else {
                    SolveResult::Optimal(OptimalSolution {
                        node_count: private.node_count,
                        link_count: private.link_count,
                        physical_link_count: private.physical_link_count,
                        discard_link_count: private.discard_link_count,
                        canonical_graph_key: private.canonical_graph_key.clone(),
                        graph: private.graph.clone(),
                        validation: private.validation.clone(),
                        proof: ProofSummary::default(),
                    })
                };
                let outcome = collector.finish(result, mode);
                let terminal_key = match outcome.result {
                    SolveResult::Optimal(solution) => solution.canonical_graph_key,
                    SolveResult::Incomplete(incomplete) => {
                        incomplete.best_known.unwrap().canonical_graph_key
                    }
                    SolveResult::GloballyUnsat(_) => unreachable!(),
                };
                assert_eq!(terminal_key, expected);
                let delivered = delivered.into_inner().unwrap();
                assert_eq!(delivered.len(), 2);
                if mode == SolveMode::OneMinNL {
                    assert!(outcome.solutions.is_empty());
                } else {
                    assert_eq!(outcome.solutions, vec![delivered[1].clone()]);
                }
            }
        }
    }
}
