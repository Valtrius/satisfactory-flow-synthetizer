use serde::{Deserialize, Serialize};

use crate::Rational;

/// Exact external rates and the mandatory capacity of every physical link.
///
/// Input vector positions are [`crate::InputTerminalIndex`] values. Output vector positions are
/// [`crate::OutputTerminalIndex`] values. A valid solver request has at least one rate on each
/// side, all rates and `max_link_rate` are strictly positive, and every external rate is no
/// greater than `max_link_rate`. Total input may exceed total requested output;
/// the exact surplus is carried by anonymous discard lines. Output demand above
/// total input is a finite global UNSAT proof rather than malformed syntax.
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
