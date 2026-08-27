//! Shared request preparation. Automatic supply is exactly one belt.

use crate::{Problem, Rational};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const MAX_ENDPOINTS_PER_SIDE: usize = 24;
const MAX_ENDPOINT_NAME_CHARS: usize = 80;

/// One caller-named external terminal.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointRequest {
    pub id: String,
    pub name: String,
    pub rate: String,
}

/// Stable application request. Rates remain strings until exact parsing.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProblemRequest {
    pub inputs: Vec<EndpointRequest>,
    pub outputs: Vec<EndpointRequest>,
    pub belt_rate: String,
}

/// Presentation metadata aligned with one original-order solver terminal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalMetadata {
    pub id: String,
    pub name: String,
    pub rate: Rational,
}

/// Exact solver problem plus caller-facing terminal identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedProblem {
    pub problem: Problem,
    pub inputs: Vec<TerminalMetadata>,
    pub outputs: Vec<TerminalMetadata>,
}

/// Invalid application request detected without floating point.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum PrepareRequestError {
    #[error("at most {MAX_ENDPOINTS_PER_SIDE} inputs and outputs are allowed")]
    TooManyEndpoints,
    #[error("belt capacity: {0}")]
    InvalidCapacity(String),
    #[error("belt capacity must be greater than zero")]
    NonPositiveCapacity,
    #[error(
        "automatic input rate {total} exceeds belt capacity {capacity}; split the inputs explicitly yourself"
    )]
    AutomaticInputExceedsCapacity {
        total: Box<Rational>,
        capacity: Box<Rational>,
    },
    #[error(
        "{side} {index} rate {rate} exceeds belt capacity {capacity}; each terminal must fit on one belt"
    )]
    ExternalRateExceedsCapacity {
        side: &'static str,
        index: usize,
        rate: Box<Rational>,
        capacity: Box<Rational>,
    },
    #[error("add at least one output")]
    MissingOutputs,
    #[error("{side} {index}: {message}")]
    InvalidRate {
        side: &'static str,
        index: usize,
        message: String,
    },
    #[error("{side} {index} rate must be greater than zero")]
    NonPositiveRate { side: &'static str, index: usize },
    #[error("{side} {index} name is too long")]
    NameTooLong { side: &'static str, index: usize },
}

impl ProblemRequest {
    /// Parses and validates the exact application boundary.
    ///
    /// An empty input list derives automatic supply from the exact output sum and
    /// requires that supply to fit on a single belt. Larger totals require explicit inputs.
    ///
    /// # Errors
    ///
    /// Returns [`PrepareRequestError`] for malformed/nonpositive rates, missing
    /// outputs, capacity violations, or excessive endpoint/name counts.
    pub fn prepare(&self) -> Result<PreparedProblem, PrepareRequestError> {
        if self.inputs.len() > MAX_ENDPOINTS_PER_SIDE || self.outputs.len() > MAX_ENDPOINTS_PER_SIDE
        {
            return Err(PrepareRequestError::TooManyEndpoints);
        }
        let capacity = self
            .belt_rate
            .parse::<Rational>()
            .map_err(|error| PrepareRequestError::InvalidCapacity(error.to_string()))?;
        if !capacity.is_positive() {
            return Err(PrepareRequestError::NonPositiveCapacity);
        }
        if self.outputs.is_empty() {
            return Err(PrepareRequestError::MissingOutputs);
        }
        let outputs = parse_terminals(&self.outputs, "output", &capacity)?;
        let inputs = if self.inputs.is_empty() {
            let rate = outputs
                .iter()
                .map(|terminal| terminal.rate.clone())
                .sum::<Rational>();
            if rate > capacity {
                return Err(PrepareRequestError::AutomaticInputExceedsCapacity {
                    total: Box::new(rate),
                    capacity: Box::new(capacity),
                });
            }
            vec![TerminalMetadata {
                id: "automatic-input".to_owned(),
                name: "Automatic supply".to_owned(),
                rate,
            }]
        } else {
            parse_terminals(&self.inputs, "input", &capacity)?
        };
        if inputs.len() > MAX_ENDPOINTS_PER_SIDE {
            return Err(PrepareRequestError::TooManyEndpoints);
        }
        Ok(PreparedProblem {
            problem: Problem {
                inputs: inputs
                    .iter()
                    .map(|terminal| terminal.rate.clone())
                    .collect(),
                outputs: outputs
                    .iter()
                    .map(|terminal| terminal.rate.clone())
                    .collect(),
                max_link_rate: capacity,
            },
            inputs,
            outputs,
        })
    }
}

fn parse_terminals(
    requests: &[EndpointRequest],
    side: &'static str,
    capacity: &Rational,
) -> Result<Vec<TerminalMetadata>, PrepareRequestError> {
    requests
        .iter()
        .enumerate()
        .map(|(offset, request)| {
            let index = offset + 1;
            let rate = request.rate.parse::<Rational>().map_err(|error| {
                PrepareRequestError::InvalidRate {
                    side,
                    index,
                    message: error.to_string(),
                }
            })?;
            if !rate.is_positive() {
                return Err(PrepareRequestError::NonPositiveRate { side, index });
            }
            if rate > *capacity {
                return Err(PrepareRequestError::ExternalRateExceedsCapacity {
                    side,
                    index,
                    rate: Box::new(rate),
                    capacity: Box::new(capacity.clone()),
                });
            }
            let name = request.name.trim();
            if name.chars().count() > MAX_ENDPOINT_NAME_CHARS {
                return Err(PrepareRequestError::NameTooLong { side, index });
            }
            Ok(TerminalMetadata {
                id: request.id.clone(),
                name: if name.is_empty() {
                    format!("{} {index}", title_case(side))
                } else {
                    name.to_owned()
                },
                rate,
            })
        })
        .collect()
}

fn title_case(value: &str) -> String {
    let mut characters = value.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_uppercase().collect::<String>() + characters.as_str()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoint(id: &str, rate: &str) -> EndpointRequest {
        EndpointRequest {
            id: id.to_owned(),
            name: id.to_owned(),
            rate: rate.to_owned(),
        }
    }

    fn request(inputs: &[&str], outputs: &[&str], capacity: &str) -> ProblemRequest {
        ProblemRequest {
            inputs: inputs
                .iter()
                .enumerate()
                .map(|(index, rate)| endpoint(&format!("i{index}"), rate))
                .collect(),
            outputs: outputs
                .iter()
                .enumerate()
                .map(|(index, rate)| endpoint(&format!("o{index}"), rate))
                .collect(),
            belt_rate: capacity.to_owned(),
        }
    }

    #[test]
    fn empty_inputs_become_one_exact_output_sum_at_capacity() {
        let prepared = request(&[], &["40", "80"], "120").prepare().unwrap();
        assert_eq!(prepared.inputs.len(), 1);
        assert_eq!(prepared.inputs[0].name, "Automatic supply");
        assert_eq!(prepared.problem.inputs, vec![Rational::from(120_u32)]);
        assert_eq!(prepared.problem.outputs, vec![40_u32.into(), 80_u32.into()]);
    }

    #[test]
    fn automatic_input_above_capacity_requires_explicit_inputs() {
        assert_eq!(
            request(&[], &["40", "80"], "119").prepare(),
            Err(PrepareRequestError::AutomaticInputExceedsCapacity {
                total: Box::new(120.into()),
                capacity: Box::new(119.into())
            })
        );
        assert!(
            request(&["119", "1"], &["40", "80"], "119")
                .prepare()
                .is_ok()
        );
    }

    #[test]
    fn fractional_output_sum_uses_exact_arithmetic() {
        let prepared = request(&[], &["1/3", "0.1"], "13/30").prepare().unwrap();
        assert_eq!(prepared.problem.inputs, vec!["13/30".parse().unwrap()]);
        assert!(matches!(
            request(&[], &["0.1", "0.2"], "0.29999999999999999999").prepare(),
            Err(PrepareRequestError::AutomaticInputExceedsCapacity { .. })
        ));
    }

    #[test]
    fn explicit_surplus_is_preserved_for_core_discard_semantics() {
        let prepared = request(&["3", "2"], &["4"], "4").prepare().unwrap();
        assert_eq!(
            prepared.problem.inputs.iter().sum::<Rational>(),
            5_u32.into()
        );
        assert_eq!(
            prepared.problem.outputs.iter().sum::<Rational>(),
            4_u32.into()
        );
    }

    #[test]
    fn explicit_terminal_capacity_errors_identify_the_side_index_and_exact_rates() {
        for (inputs, outputs, side, index) in [
            (vec!["1", "2000"], vec!["1"], "input", 2),
            (vec!["1200", "1200"], vec!["2000"], "output", 1),
        ] {
            let error = request(&inputs, &outputs, "1200").prepare().unwrap_err();
            assert_eq!(
                error,
                PrepareRequestError::ExternalRateExceedsCapacity {
                    side,
                    index,
                    rate: Box::new(2000.into()),
                    capacity: Box::new(1200.into())
                }
            );
            assert!(error.to_string().contains(&format!(
                "{side} {index} rate 2000 exceeds belt capacity 1200"
            )));
        }
        assert!(request(&["1/3"], &["1/3"], "1/3").prepare().is_ok());
        assert!(matches!(
            request(&["0.30000000000000000001"], &["0.3"], "0.3").prepare(),
            Err(PrepareRequestError::ExternalRateExceedsCapacity { side: "input", .. })
        ));
    }

    #[test]
    fn missing_outputs_and_nonpositive_values_are_rejected() {
        assert_eq!(
            request(&["1"], &[], "1").prepare(),
            Err(PrepareRequestError::MissingOutputs),
        );
        assert!(matches!(
            request(&["0"], &["1"], "1").prepare(),
            Err(PrepareRequestError::NonPositiveRate {
                side: "input",
                index: 1,
            }),
        ));
    }
}
