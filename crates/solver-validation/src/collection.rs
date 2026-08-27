use solver_api::{
    BestKnownSolution, CanonicalGraphKey, Problem, SolveMode, SolveObserver, SolveOutcome,
    SolveResult, SolverEvent,
};
use std::{collections::BTreeMap, sync::Mutex};

use crate::{layout_key, normalize_outcome_identity};

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
    pub fn finish(self, result: SolveResult, mode: SolveMode) -> SolveOutcome {
        let mut outcome = SolveOutcome::new(
            result,
            mode,
            self.state
                .into_inner()
                .expect("solution collector")
                .solutions
                .into_values()
                .collect(),
        );
        normalize_outcome_identity(self.problem, &mut outcome);
        outcome
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
