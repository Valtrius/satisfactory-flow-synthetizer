//! Exact application request preparation for the greenfield solver boundary.

use serde::{Deserialize, Serialize};
use solver_api::{Problem, Rational};
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
pub struct AppSolveRequest {
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
pub struct PreparedAppRequest {
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

impl AppSolveRequest {
    /// Parses and validates the exact application boundary.
    ///
    /// An empty input list derives automatic supply from the exact output sum and
    /// partitions that sum across as many capacity-safe belts as needed (matching
    /// the Z3/source-app automatic-input behavior).
    ///
    /// # Errors
    ///
    /// Returns [`PrepareRequestError`] for malformed/nonpositive rates, missing
    /// outputs, or excessive endpoint/name counts.
    pub fn prepare(&self) -> Result<PreparedAppRequest, PrepareRequestError> {
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
        let outputs = parse_terminals(&self.outputs, "output")?;
        let inputs = if self.inputs.is_empty() {
            let rate = outputs
                .iter()
                .map(|terminal| terminal.rate.clone())
                .sum::<Rational>();
            automatic_inputs(&rate, &capacity)
        } else {
            parse_terminals(&self.inputs, "input")?
        };
        if inputs.len() > MAX_ENDPOINTS_PER_SIDE {
            return Err(PrepareRequestError::TooManyEndpoints);
        }
        Ok(PreparedAppRequest {
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

/// Splits `total` into one or more capacity-safe automatic input terminals.
fn automatic_inputs(total: &Rational, capacity: &Rational) -> Vec<TerminalMetadata> {
    if total.is_zero() {
        return Vec::new();
    }
    let mut remaining = total.clone();
    let mut rates = Vec::new();
    while remaining > Rational::zero() {
        let rate = if remaining > *capacity {
            capacity.clone()
        } else {
            remaining.clone()
        };
        remaining = remaining - &rate;
        rates.push(rate);
    }
    let input_count = rates.len();
    rates
        .into_iter()
        .enumerate()
        .map(|(index, rate)| TerminalMetadata {
            id: if input_count == 1 {
                "automatic-input".to_owned()
            } else {
                format!("automatic-input-{}", index + 1)
            },
            name: if input_count == 1 {
                "Automatic supply".to_owned()
            } else {
                format!("Automatic supply {}", index + 1)
            },
            rate,
        })
        .collect()
}

fn parse_terminals(
    requests: &[EndpointRequest],
    side: &'static str,
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

    fn request(inputs: &[&str], outputs: &[&str], capacity: &str) -> AppSolveRequest {
        AppSolveRequest {
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
    fn automatic_input_is_partitioned_when_the_sum_exceeds_capacity() {
        let prepared = request(&[], &["40", "80"], "119").prepare().unwrap();
        assert_eq!(prepared.inputs.len(), 2);
        assert_eq!(prepared.inputs[0].name, "Automatic supply 1");
        assert_eq!(prepared.inputs[1].name, "Automatic supply 2");
        assert_eq!(
            prepared.problem.inputs,
            vec![Rational::from(119_u32), Rational::from(1_u32)]
        );
        assert_eq!(
            prepared.problem.inputs.iter().sum::<Rational>(),
            Rational::from(120_u32)
        );
    }

    #[test]
    fn fractional_output_sum_uses_exact_arithmetic() {
        let prepared = request(&[], &["1/3", "0.1"], "13/30").prepare().unwrap();
        assert_eq!(prepared.problem.inputs, vec!["13/30".parse().unwrap()]);
    }

    #[test]
    fn explicit_surplus_is_preserved_for_core_discard_semantics() {
        let prepared = request(&["3", "2"], &["4"], "3").prepare().unwrap();
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
