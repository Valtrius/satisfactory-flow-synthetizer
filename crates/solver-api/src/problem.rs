use serde::{Deserialize, Serialize};

use crate::Rational;

/// Exact external rates and the mandatory capacity of every physical link.
///
/// Input vector positions are [`crate::InputTerminalIndex`] values. Output vector positions are
/// [`crate::OutputTerminalIndex`] values. A valid solver request has at least one rate on each
/// side, and all rates and `max_link_rate` are strictly positive. Total input may exceed
/// total requested output; the exact surplus is carried by anonymous discard lines.
/// External rates above capacity and output demand above total input are finite global
/// UNSAT proofs for raw problems, not malformed syntax. [`crate::ProblemRequest::prepare`]
/// rejects capacity violations earlier with user-facing terminal errors.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    /// Exact rates entering the network.
    pub inputs: Vec<Rational>,
    /// Exact rates required at external outputs.
    pub outputs: Vec<Rational>,
    /// Inclusive capacity bound applied to every physical link.
    pub max_link_rate: Rational,
}

impl Problem {
    /// Checks syntax/domain requirements without treating impossibility as invalid input.
    ///
    /// # Errors
    /// Returns an error for empty sides, nonpositive rates, or unrepresentable indices.
    pub fn validate(&self) -> Result<(), crate::SolverError> {
        if self.inputs.is_empty() || self.outputs.is_empty() {
            return Err(crate::SolverError::InvalidProblem(
                "at least one input and output are required".to_owned(),
            ));
        }
        if self.inputs.len() > u32::MAX as usize || self.outputs.len() > u32::MAX as usize {
            return Err(crate::SolverError::InvalidProblem(
                "too many terminals".to_owned(),
            ));
        }
        if !self.max_link_rate.is_positive()
            || self
                .inputs
                .iter()
                .chain(&self.outputs)
                .any(|rate| !rate.is_positive())
        {
            return Err(crate::SolverError::InvalidProblem(
                "all rates and belt capacity must be positive".to_owned(),
            ));
        }
        Ok(())
    }

    /// Finite contradictions, evaluated only after `validate` succeeds.
    #[must_use]
    pub fn global_contradiction(&self) -> Option<crate::GlobalUnsatProof> {
        use crate::{
            ExternalTerminal, GlobalUnsatProof, GlobalUnsatReason, InputTerminalIndex,
            OutputTerminalIndex, ProofSummary,
        };
        let reason = if self.total_input() < self.total_output() {
            GlobalUnsatReason::InsufficientInput {
                total_input: self.total_input(),
                total_output: self.total_output(),
            }
        } else if let Some((i, rate)) = self
            .inputs
            .iter()
            .enumerate()
            .find(|(_, r)| *r > &self.max_link_rate)
        {
            GlobalUnsatReason::ExternalRateExceedsCapacity {
                terminal: ExternalTerminal::Input(InputTerminalIndex(u32::try_from(i).ok()?)),
                rate: rate.clone(),
                max_link_rate: self.max_link_rate.clone(),
            }
        } else if let Some((i, rate)) = self
            .outputs
            .iter()
            .enumerate()
            .find(|(_, r)| *r > &self.max_link_rate)
        {
            GlobalUnsatReason::ExternalRateExceedsCapacity {
                terminal: ExternalTerminal::Output(OutputTerminalIndex(u32::try_from(i).ok()?)),
                rate: rate.clone(),
                max_link_rate: self.max_link_rate.clone(),
            }
        } else {
            return None;
        };
        Some(GlobalUnsatProof {
            reason,
            proof: ProofSummary::default(),
        })
    }
    /// Returns the exact sum of all external input rates.
    #[must_use]
    pub fn total_input(&self) -> Rational {
        self.inputs.iter().sum()
    }

    /// Returns the exact sum of all external output rates.
    #[must_use]
    pub fn total_output(&self) -> Rational {
        self.outputs.iter().sum()
    }

    /// Returns whether exact external flow conservation holds.
    #[must_use]
    pub fn is_conserved(&self) -> bool {
        self.total_input() == self.total_output()
    }

    /// Returns whether the inputs can cover every requested output exactly.
    #[must_use]
    pub fn has_sufficient_input(&self) -> bool {
        self.total_input() >= self.total_output()
    }

    /// Returns the exact nonnegative input surplus, or `None` for an input deficit.
    #[must_use]
    pub fn surplus(&self) -> Option<Rational> {
        let input = self.total_input();
        let output = self.total_output();
        (input >= output).then(|| &input - &output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn totals_rates_exactly() {
        let problem = Problem {
            inputs: vec!["0.1".parse().unwrap(), "1/5".parse().unwrap()],
            outputs: vec!["3/10".parse().unwrap()],
            max_link_rate: 1.into(),
        };
        assert_eq!(problem.total_input(), "3/10".parse().unwrap());
        assert!(problem.is_conserved());
        assert!(problem.has_sufficient_input());
        assert_eq!(problem.surplus(), Some(Rational::zero()));
    }

    #[test]
    fn computes_exact_surplus_and_detects_input_deficit() {
        let surplus = Problem {
            inputs: vec!["7/3".parse().unwrap()],
            outputs: vec!["1/3".parse().unwrap()],
            max_link_rate: 3.into(),
        };
        assert_eq!(surplus.surplus(), Some(2.into()));

        let deficit = Problem {
            inputs: vec![1.into()],
            outputs: vec![2.into()],
            max_link_rate: 2.into(),
        };
        assert!(!deficit.has_sufficient_input());
        assert_eq!(deficit.surplus(), None);
    }

    #[test]
    fn serializes_every_rate_as_a_string() {
        let problem = Problem {
            inputs: vec!["1/3".parse().unwrap()],
            outputs: vec!["1/3".parse().unwrap()],
            max_link_rate: 1200.into(),
        };
        let json = serde_json::to_value(problem).unwrap();
        assert_eq!(json["inputs"][0], "1/3");
        assert_eq!(json["outputs"][0], "1/3");
        assert_eq!(json["maxLinkRate"], "1200");
    }
}
