//! Host-independent incremental leaf protocol and exact model acceptance.
//! A host owns the backend session, executes each command batch, and supplies its
//! reply. After any failure the leaf is poisoned. A session must be retired before
//! reporting its completion to the planner.
use crate::{
    Counts, Failure, NormalizedProblem, encoding::Encoding, profile::AccountedProfile,
    restore::restore,
};
use solver_api::{BestKnownSolution, CanonicalGraphKey, Problem, SolveMode};
use solver_validation::{ValidationError, layout_key, solve_topology, validate_solution};
use std::collections::BTreeSet;

#[derive(Clone)]
pub struct LeafContext {
    pub original: Problem,
    pub normalized: NormalizedProblem,
    pub problem: Problem,
}

impl LeafContext {
    pub(crate) fn new(original: Problem, normalized: NormalizedProblem) -> Self {
        Self {
            problem: Problem {
                inputs: normalized.inputs.as_slice().to_vec(),
                outputs: normalized.outputs.as_slice().to_vec(),
                max_link_rate: normalized.max_link_rate.clone(),
            },
            original,
            normalized,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LeafSpec {
    pub profile: AccountedProfile,
    pub source: Option<usize>,
    pub second_source: Option<usize>,
    pub mode: SolveMode,
    pub counts: Counts,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Completion {
    Exhausted,
    Optimum,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResponseKind {
    CheckSat,
    Model,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeafPhase {
    Validation,
    Identity,
}

/// Worker-local telemetry has no Send/Sync or clock requirement.
pub trait LeafTelemetry {
    fn start(&mut self, _phase: LeafPhase) {}
    fn stop(&mut self, _phase: LeafPhase) {}
    fn model(&mut self) {}
    fn duplicate(&mut self) {}
    fn valid(&mut self) {}
}
impl LeafTelemetry for () {}

/// Deliver an accepted witness before executing the next batch. A synchronous
/// backend can block in that batch, so delaying delivery would lose the witness
/// if its worker is terminated. Dispose the backend before reporting completion.
pub struct LeafAction {
    pub commands: String,
    pub response: Option<ResponseKind>,
    pub witness: Option<BestKnownSolution>,
    pub completion: Option<Completion>,
}

#[derive(Clone, Copy)]
enum State {
    New,
    Check,
    Model,
    Sealed,
}

pub struct LeafDriver {
    context: LeafContext,
    spec: LeafSpec,
    encoding: Encoding,
    seen: BTreeSet<CanonicalGraphKey>,
    state: State,
}

impl LeafDriver {
    #[must_use]
    pub fn new(context: &LeafContext, spec: LeafSpec) -> Self {
        Self {
            context: context.clone(),
            spec,
            encoding: Encoding::new(&context.problem, spec.profile, spec.counts),
            seen: BTreeSet::new(),
            state: State::New,
        }
    }

    /// Advance exactly one protocol reply. None is valid only on the initial call.
    /// Cancellation is a host observation, not an atomic in a private Wasm heap.
    /// # Errors
    /// Returns interruption, malformed/unexpected replies, or failed exact model
    /// validation. All errors poison the driver, including backend `unknown`.
    pub fn advance(
        &mut self,
        reply: Option<&str>,
        cancelled: bool,
        telemetry: &mut impl LeafTelemetry,
    ) -> Result<LeafAction, Failure> {
        let state = std::mem::replace(&mut self.state, State::Sealed);
        if matches!(state, State::Sealed) {
            return Err(Failure::Worker("reply after leaf retirement".into()));
        }
        if cancelled {
            return Err(Failure::Cancelled);
        }
        match (state, reply) {
            (State::New, None) => {
                let mut commands = self.encoding.script.clone();
                if let Some(source) = self.spec.source {
                    commands.push_str(&self.encoding.output_source_assertion(0, source)?);
                }
                if let Some(source) = self.spec.second_source {
                    commands.push_str(&self.encoding.output_source_assertion(1, source)?);
                }
                commands.push_str("(check-sat)\n");
                self.state = State::Check;
                Ok(LeafAction {
                    commands,
                    response: Some(ResponseKind::CheckSat),
                    witness: None,
                    completion: None,
                })
            }
            (State::Check, Some("unsat")) => Ok(LeafAction {
                commands: String::new(),
                response: None,
                witness: None,
                completion: Some(Completion::Exhausted),
            }),
            (State::Check, Some("sat")) => {
                self.state = State::Model;
                Ok(LeafAction {
                    commands: self.encoding.query(),
                    response: Some(ResponseKind::Model),
                    witness: None,
                    completion: None,
                })
            }
            (State::Check, Some(other)) => Err(Failure::Worker(format!(
                "cvc5 did not finish its proof: {other}"
            ))),
            (State::Model, Some(reply)) => self.model(reply, telemetry),
            _ => Err(Failure::Worker("unexpected leaf protocol reply".into())),
        }
    }

    fn model(
        &mut self,
        reply: &str,
        telemetry: &mut impl LeafTelemetry,
    ) -> Result<LeafAction, Failure> {
        let (graph, mut commands) = self.encoding.model(reply)?;
        telemetry.model();
        let witness = self.accept_model(&graph, telemetry)?;
        let optimum = witness.is_some() && self.spec.mode == SolveMode::OneMinNL;
        if optimum {
            commands.clear();
        } else {
            commands.push_str("(check-sat)\n");
            self.state = State::Check;
        }
        Ok(LeafAction {
            commands,
            response: (!optimum).then_some(ResponseKind::CheckSat),
            witness,
            completion: optimum.then_some(Completion::Optimum),
        })
    }

    fn accept_model(
        &mut self,
        graph: &solver_api::PhysicalGraph,
        telemetry: &mut impl LeafTelemetry,
    ) -> Result<Option<BestKnownSolution>, Failure> {
        telemetry.start(LeafPhase::Validation);
        let solved = solve_topology(&self.context.problem, graph);
        telemetry.stop(LeafPhase::Validation);
        let solved = match solved {
            Ok(graph) => graph,
            Err(
                ValidationError::NonUniqueSteadyState { .. }
                | ValidationError::NonUniqueCyclicScc { .. }
                | ValidationError::InconsistentCyclicScc { .. },
            ) => return Ok(None),
            Err(error) => {
                return Err(Failure::Worker(format!(
                    "Solver model failed independent reconstruction: {error}"
                )));
            }
        };
        telemetry.start(LeafPhase::Validation);
        let validation = validate_solution(&self.context.problem, &solved);
        telemetry.stop(LeafPhase::Validation);
        let validation = validation.map_err(|e| Failure::Worker(e.to_string()))?;
        if validation.node_count != self.spec.profile.profile.node_count()
            || validation.link_count != self.spec.profile.accounting.link_count
        {
            return Err(Failure::Worker("Solver model objective mismatch".into()));
        }
        telemetry.start(LeafPhase::Identity);
        let identity = layout_key(&self.context.problem, &solved);
        telemetry.stop(LeafPhase::Identity);
        if !self.seen.insert(identity.clone()) {
            telemetry.duplicate();
            return Ok(None);
        }
        telemetry.start(LeafPhase::Validation);
        let raw = restore(
            &self.context.original,
            &self.context.normalized,
            solved,
            identity,
        );
        telemetry.stop(LeafPhase::Validation);
        let raw = raw?;
        telemetry.valid();
        Ok(Some(raw))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Preparation, prepare_problem, profile::ProfileLinkAccounting};
    use solver_api::NodeProfile;

    fn direct(mode: SolveMode) -> LeafDriver {
        let problem = Problem {
            inputs: vec!["1/3".parse().unwrap()],
            outputs: vec!["1/3".parse().unwrap()],
            max_link_rate: "1".parse().unwrap(),
        };
        let Preparation::Prepared(normalized) = prepare_problem(&problem).unwrap() else {
            panic!()
        };
        LeafDriver::new(
            &LeafContext::new(problem, normalized),
            LeafSpec {
                profile: AccountedProfile {
                    profile: NodeProfile::default(),
                    accounting: ProfileLinkAccounting {
                        link_count: 0,
                        discard_link_count: 0,
                        physical_link_count: 1,
                    },
                },
                source: None,
                second_source: None,
                mode,
                counts: Counts::Boolean,
            },
        )
    }

    #[test]
    fn exact_leaf_protocol_keeps_enumeration_distinct_from_first_optimum() {
        for mode in [SolveMode::OneMinNL, SolveMode::AllMinNL, SolveMode::AllMinN] {
            let mut leaf = direct(mode);
            assert_eq!(
                leaf.advance(None, false, &mut ()).unwrap().response,
                Some(ResponseKind::CheckSat)
            );
            assert_eq!(
                leaf.advance(Some("sat"), false, &mut ()).unwrap().response,
                Some(ResponseKind::Model)
            );
            let action = leaf.advance(Some("((e0 true))"), false, &mut ()).unwrap();
            assert_eq!(
                action.witness.unwrap().graph.links[0].flow,
                "1/3".parse().unwrap()
            );
            if mode == SolveMode::OneMinNL {
                assert_eq!(action.completion, Some(Completion::Optimum));
                assert!(action.commands.is_empty());
            } else {
                assert!(action.completion.is_none());
                assert_eq!(
                    leaf.advance(Some("unsat"), false, &mut ())
                        .unwrap()
                        .completion,
                    Some(Completion::Exhausted)
                );
            }
            assert!(leaf.advance(Some("unsat"), false, &mut ()).is_err());
        }
    }

    #[test]
    fn unknown_malformed_models_and_cancellation_poison_the_leaf() {
        for reply in [
            "unknown",
            "unknown (RESOURCEOUT)",
            "(error bad)",
            "sat unsat",
            "",
        ] {
            let mut leaf = direct(SolveMode::AllMinNL);
            leaf.advance(None, false, &mut ()).unwrap();
            assert!(leaf.advance(Some(reply), false, &mut ()).is_err());
            assert!(leaf.advance(Some("unsat"), false, &mut ()).is_err());
        }
        for reply in [
            "((e0 maybe))",
            "e0 true",
            "(e0 true)",
            "(((e0 true)))",
            "((e0 true)) trailing",
            "((e0 true extra))",
            "((e0 true) (e0 false))",
            "((missing true))",
        ] {
            let mut leaf = direct(SolveMode::AllMinNL);
            leaf.advance(None, false, &mut ()).unwrap();
            leaf.advance(Some("sat"), false, &mut ()).unwrap();
            assert!(
                leaf.advance(Some(reply), false, &mut ()).is_err(),
                "{reply}"
            );
            assert!(leaf.advance(Some("unsat"), false, &mut ()).is_err());
        }
        let mut leaf = direct(SolveMode::AllMinNL);
        leaf.advance(None, false, &mut ()).unwrap();
        assert!(matches!(
            leaf.advance(Some("unsat"), true, &mut ()),
            Err(Failure::Cancelled)
        ));
    }
}
