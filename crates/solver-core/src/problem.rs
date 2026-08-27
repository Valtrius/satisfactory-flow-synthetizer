use std::cmp::Ordering;

use num::{BigInt, BigRational, Integer, One, Signed, Zero};
use solver_api::{
    ExternalTerminal, GlobalUnsatProof, GlobalUnsatReason, InputTerminalIndex, OutputTerminalIndex,
    Problem, ProofSummary, Rational,
};
use thiserror::Error;

/// A sorted multiset of normalized external rates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RateMultiset(Vec<Rational>);

impl RateMultiset {
    #[must_use]
    pub fn as_slice(&self) -> &[Rational] {
        &self.0
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[must_use]
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &Rational> {
        self.0.iter()
    }
}

/// Maps canonical terminal positions to and from caller-visible positions.
///
/// Equal-rate terminals use their original index as the deterministic tie-break.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalMapping {
    normalized_inputs_to_original: Vec<usize>,
    normalized_outputs_to_original: Vec<usize>,
    original_inputs_to_normalized: Vec<usize>,
    original_outputs_to_normalized: Vec<usize>,
}

impl TerminalMapping {
    #[must_use]
    pub fn original_input(&self, normalized_index: usize) -> Option<usize> {
        self.normalized_inputs_to_original
            .get(normalized_index)
            .copied()
    }

    #[must_use]
    pub fn original_output(&self, normalized_index: usize) -> Option<usize> {
        self.normalized_outputs_to_original
            .get(normalized_index)
            .copied()
    }

    #[must_use]
    pub fn normalized_input(&self, original_index: usize) -> Option<usize> {
        self.original_inputs_to_normalized
            .get(original_index)
            .copied()
    }

    #[must_use]
    pub fn normalized_output(&self, original_index: usize) -> Option<usize> {
        self.original_outputs_to_normalized
            .get(original_index)
            .copied()
    }

    #[must_use]
    pub fn normalized_inputs_to_original(&self) -> &[usize] {
        &self.normalized_inputs_to_original
    }

    #[must_use]
    pub fn normalized_outputs_to_original(&self) -> &[usize] {
        &self.normalized_outputs_to_original
    }
}

/// The exact canonical problem searched by later milestones.
///
/// `original_scale` is the positive value `s` for which every original external
/// rate equals its normalized rate multiplied by `s`. Capacity is divided by the
/// same `s`, so capacity comparisons are unchanged by normalization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedProblem {
    pub inputs: RateMultiset,
    pub outputs: RateMultiset,
    /// Exact nonnegative input surplus in the same global scale as every rate.
    pub surplus: Rational,
    pub max_link_rate: Rational,
    pub original_scale: Rational,
    pub terminal_mapping: TerminalMapping,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Preparation {
    Prepared(NormalizedProblem),
    GloballyUnsat(GlobalUnsatProof),
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum InvalidProblem {
    #[error("at least one input is required")]
    MissingInputs,
    #[error("at least one output is required")]
    MissingOutputs,
    #[error("{count} inputs cannot be represented by the public terminal index type")]
    TooManyInputs { count: usize },
    #[error("{count} outputs cannot be represented by the public terminal index type")]
    TooManyOutputs { count: usize },
    #[error("maximum link rate must be greater than zero, got {rate}")]
    NonPositiveMaxLinkRate { rate: Rational },
    #[error("input {index} rate must be greater than zero, got {rate}")]
    NonPositiveInputRate { index: usize, rate: Rational },
    #[error("output {index} rate must be greater than zero, got {rate}")]
    NonPositiveOutputRate { index: usize, rate: Rational },
}

/// Validate all finite global conditions and build the canonical normalized problem.
///
/// # Errors
///
/// Returns [`InvalidProblem`] only for malformed problem semantics such as an empty
/// terminal side or a non-positive rate. Insufficient input and external
/// capacity failures return finite [`GlobalUnsatProof`] values.
pub fn prepare_problem(problem: &Problem) -> Result<Preparation, InvalidProblem> {
    validate_problem(problem)?;

    let input_values = problem
        .inputs
        .iter()
        .map(to_big_rational)
        .collect::<Vec<_>>();
    let output_values = problem
        .outputs
        .iter()
        .map(to_big_rational)
        .collect::<Vec<_>>();
    let total_input = input_values
        .iter()
        .fold(BigRational::zero(), |sum, rate| sum + rate);
    let total_output = output_values
        .iter()
        .fold(BigRational::zero(), |sum, rate| sum + rate);

    // No splitter/merger network can create flow. Surplus is instead consumed
    // by explicit anonymous discard links and remains a satisfiable condition.
    if total_input < total_output {
        return Ok(Preparation::GloballyUnsat(GlobalUnsatProof {
            reason: GlobalUnsatReason::InsufficientInput {
                total_input: from_big_rational(&total_input),
                total_output: from_big_rational(&total_output),
            },
            proof: ProofSummary::default(),
        }));
    }

    let capacity = to_big_rational(&problem.max_link_rate);
    if let Some((index, rate)) = input_values
        .iter()
        .enumerate()
        .find(|(_, rate)| *rate > &capacity)
    {
        let terminal_index = u32::try_from(index).map_err(|_| InvalidProblem::TooManyInputs {
            count: problem.inputs.len(),
        })?;
        return Ok(Preparation::GloballyUnsat(GlobalUnsatProof {
            reason: GlobalUnsatReason::ExternalRateExceedsCapacity {
                terminal: ExternalTerminal::Input(InputTerminalIndex(terminal_index)),
                rate: from_big_rational(rate),
                max_link_rate: problem.max_link_rate.clone(),
            },
            proof: ProofSummary::default(),
        }));
    }
    if let Some((index, rate)) = output_values
        .iter()
        .enumerate()
        .find(|(_, rate)| *rate > &capacity)
    {
        let terminal_index = u32::try_from(index).map_err(|_| InvalidProblem::TooManyOutputs {
            count: problem.outputs.len(),
        })?;
        return Ok(Preparation::GloballyUnsat(GlobalUnsatProof {
            reason: GlobalUnsatReason::ExternalRateExceedsCapacity {
                terminal: ExternalTerminal::Output(OutputTerminalIndex(terminal_index)),
                rate: from_big_rational(rate),
                max_link_rate: problem.max_link_rate.clone(),
            },
            proof: ProofSummary::default(),
        }));
    }

    Ok(Preparation::Prepared(normalize_problem(
        &input_values,
        &output_values,
        &capacity,
    )))
}

fn validate_problem(problem: &Problem) -> Result<(), InvalidProblem> {
    if problem.inputs.is_empty() {
        return Err(InvalidProblem::MissingInputs);
    }
    if problem.outputs.is_empty() {
        return Err(InvalidProblem::MissingOutputs);
    }
    if u32::try_from(problem.inputs.len() - 1).is_err() {
        return Err(InvalidProblem::TooManyInputs {
            count: problem.inputs.len(),
        });
    }
    if u32::try_from(problem.outputs.len() - 1).is_err() {
        return Err(InvalidProblem::TooManyOutputs {
            count: problem.outputs.len(),
        });
    }

    if to_big_rational(&problem.max_link_rate) <= BigRational::zero() {
        return Err(InvalidProblem::NonPositiveMaxLinkRate {
            rate: problem.max_link_rate.clone(),
        });
    }
    for (index, rate) in problem.inputs.iter().enumerate() {
        if to_big_rational(rate) <= BigRational::zero() {
            return Err(InvalidProblem::NonPositiveInputRate {
                index,
                rate: rate.clone(),
            });
        }
    }
    for (index, rate) in problem.outputs.iter().enumerate() {
        if to_big_rational(rate) <= BigRational::zero() {
            return Err(InvalidProblem::NonPositiveOutputRate {
                index,
                rate: rate.clone(),
            });
        }
    }
    Ok(())
}

fn normalize_problem(
    inputs: &[BigRational],
    outputs: &[BigRational],
    capacity: &BigRational,
) -> NormalizedProblem {
    let denominator_lcm = inputs
        .iter()
        .chain(outputs)
        .map(BigRational::denom)
        .fold(BigInt::one(), |lcm, denominator| lcm.lcm(denominator));
    let rate_gcd = inputs
        .iter()
        .chain(outputs)
        .map(|rate| {
            let multiplier = &denominator_lcm / rate.denom();
            (rate.numer() * multiplier).abs()
        })
        .fold(BigInt::zero(), |gcd, value| gcd.gcd(&value));
    debug_assert!(rate_gcd.is_positive());

    let original_scale = BigRational::new(rate_gcd, denominator_lcm);
    let (normalized_inputs, input_mapping) = normalize_side(inputs, &original_scale);
    let (normalized_outputs, output_mapping) = normalize_side(outputs, &original_scale);
    let normalized_surplus = (inputs.iter().sum::<BigRational>()
        - outputs.iter().sum::<BigRational>())
        / &original_scale;
    debug_assert!(normalized_surplus >= BigRational::zero());

    NormalizedProblem {
        inputs: RateMultiset(normalized_inputs),
        outputs: RateMultiset(normalized_outputs),
        surplus: from_big_rational(&normalized_surplus),
        max_link_rate: from_big_rational(&(capacity / &original_scale)),
        original_scale: from_big_rational(&original_scale),
        terminal_mapping: TerminalMapping {
            original_inputs_to_normalized: invert_mapping(&input_mapping),
            original_outputs_to_normalized: invert_mapping(&output_mapping),
            normalized_inputs_to_original: input_mapping,
            normalized_outputs_to_original: output_mapping,
        },
    }
}

fn normalize_side(
    rates: &[BigRational],
    original_scale: &BigRational,
) -> (Vec<Rational>, Vec<usize>) {
    let mut normalized = rates
        .iter()
        .enumerate()
        .map(|(original_index, rate)| (rate / original_scale, original_index))
        .collect::<Vec<_>>();
    normalized.sort_by(|(left_rate, left_index), (right_rate, right_index)| {
        let rate_order = left_rate.cmp(right_rate);
        if rate_order == Ordering::Equal {
            left_index.cmp(right_index)
        } else {
            rate_order
        }
    });

    let rates = normalized
        .iter()
        .map(|(rate, _)| from_big_rational(rate))
        .collect();
    let mapping = normalized
        .into_iter()
        .map(|(_, original_index)| original_index)
        .collect();
    (rates, mapping)
}

fn invert_mapping(normalized_to_original: &[usize]) -> Vec<usize> {
    let mut original_to_normalized = vec![0; normalized_to_original.len()];
    for (normalized_index, &original_index) in normalized_to_original.iter().enumerate() {
        original_to_normalized[original_index] = normalized_index;
    }
    original_to_normalized
}

// The API type owns parsing and serialization. These two adapters keep this
// milestone independent of its internal arbitrary-precision representation.
fn to_big_rational(value: &Rational) -> BigRational {
    value.as_big_rational().clone()
}

fn from_big_rational(value: &BigRational) -> Rational {
    Rational::from(value.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rational(value: &str) -> Rational {
        value.parse().unwrap()
    }

    fn problem(inputs: &[&str], outputs: &[&str], max_link_rate: &str) -> Problem {
        Problem {
            inputs: inputs.iter().map(|value| rational(value)).collect(),
            outputs: outputs.iter().map(|value| rational(value)).collect(),
            max_link_rate: rational(max_link_rate),
        }
    }

    fn prepared(problem: &Problem) -> NormalizedProblem {
        match prepare_problem(problem).unwrap() {
            Preparation::Prepared(problem) => problem,
            Preparation::GloballyUnsat(proof) => panic!("unexpected global proof: {proof:?}"),
        }
    }

    fn exact_values(rates: &RateMultiset) -> Vec<String> {
        rates.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn normalizes_fraction_and_decimal_rates_with_one_global_scale() {
        let normalized = prepared(&problem(&["0.5", "1/3"], &["1/6", "2/3"], "1"));
        assert_eq!(exact_values(&normalized.inputs), vec!["2", "3"]);
        assert_eq!(exact_values(&normalized.outputs), vec!["1", "4"]);
        assert_eq!(normalized.original_scale, rational("1/6"));
        assert_eq!(normalized.max_link_rate, rational("6"));
    }

    #[test]
    fn normalization_is_invariant_under_one_common_scale() {
        let base = prepared(&problem(&["2", "3"], &["1", "4"], "6"));
        let scaled = prepared(&problem(&["20", "30"], &["10", "40"], "60"));

        assert_eq!(base.inputs, scaled.inputs);
        assert_eq!(base.outputs, scaled.outputs);
        assert_eq!(base.max_link_rate, scaled.max_link_rate);
        assert_eq!(base.original_scale, rational("1"));
        assert_eq!(scaled.original_scale, rational("10"));

        let surplus_base = prepared(&problem(&["2", "3"], &["1"], "6"));
        let surplus_scaled = prepared(&problem(&["20", "30"], &["10"], "60"));
        assert_eq!(surplus_base.inputs, surplus_scaled.inputs);
        assert_eq!(surplus_base.outputs, surplus_scaled.outputs);
        assert_eq!(surplus_base.surplus, surplus_scaled.surplus);
        assert_eq!(surplus_base.max_link_rate, surplus_scaled.max_link_rate);
        assert_eq!(surplus_base.surplus, rational("4"));
    }

    #[test]
    fn equal_rates_map_by_original_index_after_rate_sorting() {
        let normalized = prepared(&problem(&["5", "2", "5"], &["5", "5", "2"], "10"));

        assert_eq!(exact_values(&normalized.inputs), vec!["2", "5", "5"]);
        assert_eq!(exact_values(&normalized.outputs), vec!["2", "5", "5"]);
        assert_eq!(
            normalized.terminal_mapping.normalized_inputs_to_original(),
            &[1, 0, 2]
        );
        assert_eq!(
            normalized.terminal_mapping.normalized_outputs_to_original(),
            &[2, 0, 1]
        );
        assert_eq!(normalized.terminal_mapping.normalized_input(0), Some(1));
        assert_eq!(normalized.terminal_mapping.normalized_input(2), Some(2));
        assert_eq!(normalized.terminal_mapping.normalized_output(1), Some(2));
    }

    #[test]
    fn input_deficit_is_a_global_unsat_proof_but_surplus_is_prepared() {
        assert_eq!(
            prepare_problem(&problem(&["2"], &["3"], "10")),
            Ok(Preparation::GloballyUnsat(GlobalUnsatProof {
                reason: GlobalUnsatReason::InsufficientInput {
                    total_input: rational("2"),
                    total_output: rational("3"),
                },
                proof: ProofSummary::default(),
            }))
        );

        let surplus = prepared(&problem(&["3"], &["2"], "10"));
        assert_eq!(surplus.surplus, rational("1"));
    }

    #[test]
    fn surplus_and_capacity_use_the_same_global_normalization_scale() {
        let normalized = prepared(&problem(&["0.5", "1/3"], &["1/6"], "1"));
        assert_eq!(exact_values(&normalized.inputs), vec!["2", "3"]);
        assert_eq!(exact_values(&normalized.outputs), vec!["1"]);
        assert_eq!(normalized.surplus, rational("4"));
        assert_eq!(normalized.max_link_rate, rational("6"));
        assert_eq!(normalized.original_scale, rational("1/6"));
    }

    #[test]
    fn rejects_empty_terminal_sides() {
        assert_eq!(
            prepare_problem(&problem(&[], &["1"], "1")),
            Err(InvalidProblem::MissingInputs)
        );
        assert_eq!(
            prepare_problem(&problem(&["1"], &[], "1")),
            Err(InvalidProblem::MissingOutputs)
        );
    }

    #[test]
    fn rejects_non_positive_external_rates_and_capacity() {
        assert_eq!(
            prepare_problem(&problem(&["0"], &["1"], "1")),
            Err(InvalidProblem::NonPositiveInputRate {
                index: 0,
                rate: rational("0"),
            })
        );
        assert_eq!(
            prepare_problem(&problem(&["1"], &["-1"], "1")),
            Err(InvalidProblem::NonPositiveOutputRate {
                index: 0,
                rate: rational("-1"),
            })
        );
        assert_eq!(
            prepare_problem(&problem(&["1"], &["1"], "0")),
            Err(InvalidProblem::NonPositiveMaxLinkRate {
                rate: rational("0"),
            })
        );
        assert_eq!(
            prepare_problem(&problem(&["1"], &["1"], "-1")),
            Err(InvalidProblem::NonPositiveMaxLinkRate {
                rate: rational("-1"),
            })
        );
    }

    #[test]
    fn accepts_external_rate_at_capacity_and_proves_rate_above_it_unsat() {
        assert!(matches!(
            prepare_problem(&problem(&["5"], &["5"], "5")),
            Ok(Preparation::Prepared(_))
        ));
        assert_eq!(
            prepare_problem(&problem(&["5"], &["5"], "4")),
            Ok(Preparation::GloballyUnsat(GlobalUnsatProof {
                reason: GlobalUnsatReason::ExternalRateExceedsCapacity {
                    terminal: ExternalTerminal::Input(InputTerminalIndex(0)),
                    rate: rational("5"),
                    max_link_rate: rational("4"),
                },
                proof: ProofSummary::default(),
            }))
        );
        assert_eq!(
            prepare_problem(&problem(&["3", "3"], &["6"], "4")),
            Ok(Preparation::GloballyUnsat(GlobalUnsatProof {
                reason: GlobalUnsatReason::ExternalRateExceedsCapacity {
                    terminal: ExternalTerminal::Output(OutputTerminalIndex(0)),
                    rate: rational("6"),
                    max_link_rate: rational("4"),
                },
                proof: ProofSummary::default(),
            }))
        );
    }
}
