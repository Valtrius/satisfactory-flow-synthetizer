//! Exact concrete component-application identities.
//!
//! An application proof is deliberately narrower than a symbolic component
//! contract. It applies only to one physical boundary relation, one exact
//! boundary-rate vector (modulo a single common homogeneous scale), and the
//! capacity scaled by that same factor.

use num::{BigInt, BigRational, Integer, One, Signed, Zero};
use solver_api::{NodeProfile, Rational};
use thiserror::Error;

use crate::components::{
    BoundarySignature, Component, ComponentApplication, ComponentApplicationError,
    ComponentCanonicalKey, ComponentCost, MatrixError,
};

/// Invalid concrete component optimization request.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ComponentApplicationRequestError {
    /// Both sides of a component boundary must be present.
    #[error("component applications require at least one boundary input and output")]
    EmptyBoundary,
    /// Exact input-vector dimension disagrees with the physical signature.
    #[error("expected {expected} boundary inputs, got {actual}")]
    InputCount { expected: usize, actual: usize },
    /// Exact output-vector dimension disagrees with the physical signature.
    #[error("expected {expected} boundary outputs, got {actual}")]
    OutputCount { expected: usize, actual: usize },
    /// One exact boundary value is not strictly positive.
    #[error("component {side} boundary {index} must be positive, got {value}")]
    NonPositiveBoundary {
        side: &'static str,
        index: usize,
        value: Rational,
    },
    /// Capacity itself is not strictly positive.
    #[error("component application capacity must be positive, got {0}")]
    NonPositiveCapacity(Rational),
    /// One boundary belt already exceeds the mandatory capacity.
    #[error("component {side} boundary {index} exceeds capacity {capacity}: {value}")]
    BoundaryCapacityExceeded {
        side: &'static str,
        index: usize,
        value: Box<Rational>,
        capacity: Box<Rational>,
    },
    /// A frozen component cannot create or discard flow internally.
    #[error("component boundary totals differ: inputs {inputs}, outputs {outputs}")]
    Conservation {
        inputs: Box<Rational>,
        outputs: Box<Rational>,
    },
    /// Boundary permutation failed an internal exact dimension check.
    #[error(transparent)]
    Permutation(#[from] MatrixError),
}

/// Scale-invariant concrete work identity for one application-optimal proof.
///
/// Coordinates and the feedback bitmap are jointly canonical. `inputs`,
/// `outputs`, and `max_link_rate` have all been divided by one common positive
/// scale derived from the complete `inputs + outputs` tuple. Thus common global
/// rescaling reuses a proof, while independent per-coordinate scaling and a
/// changed capacity ratio cannot collide.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComponentApplicationKey {
    boundary: BoundarySignature,
    inputs: Vec<Rational>,
    outputs: Vec<Rational>,
    max_link_rate: Rational,
}

impl ComponentApplicationKey {
    /// Borrows the canonical physical boundary relation.
    #[must_use]
    pub const fn boundary(&self) -> &BoundarySignature {
        &self.boundary
    }

    /// Borrows common-scale-normalized exact boundary inputs.
    #[must_use]
    pub fn inputs(&self) -> &[Rational] {
        &self.inputs
    }

    /// Borrows common-scale-normalized exact boundary outputs.
    #[must_use]
    pub fn outputs(&self) -> &[Rational] {
        &self.outputs
    }

    /// Borrows capacity divided by the exact same common scale.
    #[must_use]
    pub const fn max_link_rate(&self) -> &Rational {
        &self.max_link_rate
    }
}

/// Validated request together with the scale removed to obtain its work key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentApplicationRequest {
    key: ComponentApplicationKey,
    common_scale: Rational,
}

impl ComponentApplicationRequest {
    /// Validates and canonicalizes a concrete globally normalized boundary.
    ///
    /// Boundary labels are quotiented jointly with the direct-feedback bitmap.
    /// Only after choosing a physical canonical orientation is one common
    /// LCM/GCD scale removed from the entire input/output tuple; capacity is
    /// divided by that exact scale as well.
    ///
    /// # Errors
    ///
    /// Rejects dimensions, nonpositive values/capacity, boundary values above
    /// capacity, and unequal boundary totals.
    pub fn new(
        boundary: &BoundarySignature,
        inputs: &[Rational],
        outputs: &[Rational],
        max_link_rate: &Rational,
    ) -> Result<Self, ComponentApplicationRequestError> {
        validate_request(boundary, inputs, outputs, max_link_rate)?;
        let mut selected = None;
        for input_order in permutations(boundary.input_count()) {
            for output_order in permutations(boundary.output_count()) {
                let candidate_boundary = boundary.permute(&input_order, &output_order)?;
                let candidate_inputs = input_order
                    .iter()
                    .map(|&index| inputs[index].clone())
                    .collect::<Vec<_>>();
                let candidate_outputs = output_order
                    .iter()
                    .map(|&index| outputs[index].clone())
                    .collect::<Vec<_>>();
                let candidate = (candidate_boundary, candidate_inputs, candidate_outputs);
                if selected.as_ref().is_none_or(|current| candidate < *current) {
                    selected = Some(candidate);
                }
            }
        }
        let (boundary, inputs, outputs) =
            selected.ok_or(ComponentApplicationRequestError::EmptyBoundary)?;
        let common_scale = common_rate_scale(&inputs, &outputs);
        let normalized_inputs = inputs.iter().map(|value| value / &common_scale).collect();
        let normalized_outputs = outputs.iter().map(|value| value / &common_scale).collect();
        let normalized_capacity = max_link_rate / &common_scale;
        Ok(Self {
            key: ComponentApplicationKey {
                boundary,
                inputs: normalized_inputs,
                outputs: normalized_outputs,
                max_link_rate: normalized_capacity,
            },
            common_scale,
        })
    }

    /// Borrows the exact scale-invariant work identity.
    #[must_use]
    pub const fn key(&self) -> &ComponentApplicationKey {
        &self.key
    }

    /// Returns the one positive scale removed from every boundary rate and B.
    #[must_use]
    pub const fn common_scale(&self) -> &Rational {
        &self.common_scale
    }
}

fn validate_request(
    boundary: &BoundarySignature,
    inputs: &[Rational],
    outputs: &[Rational],
    capacity: &Rational,
) -> Result<(), ComponentApplicationRequestError> {
    if boundary.input_count() == 0 || boundary.output_count() == 0 {
        return Err(ComponentApplicationRequestError::EmptyBoundary);
    }
    if inputs.len() != boundary.input_count() {
        return Err(ComponentApplicationRequestError::InputCount {
            expected: boundary.input_count(),
            actual: inputs.len(),
        });
    }
    if outputs.len() != boundary.output_count() {
        return Err(ComponentApplicationRequestError::OutputCount {
            expected: boundary.output_count(),
            actual: outputs.len(),
        });
    }
    if !capacity.is_positive() {
        return Err(ComponentApplicationRequestError::NonPositiveCapacity(
            capacity.clone(),
        ));
    }
    for (side, values) in [("input", inputs), ("output", outputs)] {
        for (index, value) in values.iter().enumerate() {
            if !value.is_positive() {
                return Err(ComponentApplicationRequestError::NonPositiveBoundary {
                    side,
                    index,
                    value: value.clone(),
                });
            }
            if value > capacity {
                return Err(ComponentApplicationRequestError::BoundaryCapacityExceeded {
                    side,
                    index,
                    value: Box::new(value.clone()),
                    capacity: Box::new(capacity.clone()),
                });
            }
        }
    }
    let input_total = inputs.iter().sum::<Rational>();
    let output_total = outputs.iter().sum::<Rational>();
    if input_total != output_total {
        return Err(ComponentApplicationRequestError::Conservation {
            inputs: Box::new(input_total),
            outputs: Box::new(output_total),
        });
    }
    Ok(())
}

fn common_rate_scale(inputs: &[Rational], outputs: &[Rational]) -> Rational {
    let rates = inputs.iter().chain(outputs).collect::<Vec<_>>();
    let denominator_lcm = rates
        .iter()
        .map(|rate| rate.denominator())
        .fold(BigInt::one(), |lcm, denominator| lcm.lcm(denominator));
    let numerator_gcd = rates
        .iter()
        .map(|rate| {
            let multiplier = &denominator_lcm / rate.denominator();
            (rate.numerator() * multiplier).abs()
        })
        .fold(BigInt::zero(), |gcd, value| gcd.gcd(&value));
    debug_assert!(numerator_gcd.is_positive());
    Rational::from(BigRational::new(numerator_gcd, denominator_lcm))
}

fn permutations(size: usize) -> Vec<Vec<usize>> {
    fn extend(
        size: usize,
        prefix: &mut Vec<usize>,
        used: &mut [bool],
        output: &mut Vec<Vec<usize>>,
    ) {
        if prefix.len() == size {
            output.push(prefix.clone());
            return;
        }
        for value in 0..size {
            if used[value] {
                continue;
            }
            used[value] = true;
            prefix.push(value);
            extend(size, prefix, used, output);
            prefix.pop();
            used[value] = false;
        }
    }
    let mut output = Vec::new();
    extend(size, &mut Vec::new(), &mut vec![false; size], &mut output);
    output
}

/// Exact mapping proving that one symbolic component realizes a concrete key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentApplicationMatch {
    /// Request input coordinate -> component canonical input coordinate.
    pub component_input_order: Vec<usize>,
    /// Request output coordinate -> component canonical output coordinate.
    pub component_output_order: Vec<usize>,
    /// Exact evaluated component flows in component canonical coordinates.
    pub application: ComponentApplication,
}

/// Finds the deterministic smallest boundary isomorphism under which a
/// certified component realizes this exact application and capacity.
///
/// # Errors
///
/// Returns only internal contract errors. Ordinary domain, output, or capacity
/// mismatches return `Ok(None)` and retain the candidate elsewhere.
pub fn match_component_application(
    component: &Component,
    key: &ComponentApplicationKey,
) -> Result<Option<ComponentApplicationMatch>, ComponentApplicationError> {
    if component.boundary().input_count() != key.inputs.len()
        || component.boundary().output_count() != key.outputs.len()
    {
        return Ok(None);
    }
    for input_order in permutations(key.inputs.len()) {
        for output_order in permutations(key.outputs.len()) {
            let Ok(boundary) = component.boundary().permute(&input_order, &output_order) else {
                continue;
            };
            if boundary != key.boundary {
                continue;
            }
            let mut component_inputs = vec![Rational::zero(); input_order.len()];
            for (request, &component_coordinate) in input_order.iter().enumerate() {
                component_inputs[component_coordinate] = key.inputs[request].clone();
            }
            let application = match component.evaluate(&component_inputs, &key.max_link_rate) {
                Ok(application) => application,
                Err(
                    ComponentApplicationError::OutsideDomain
                    | ComponentApplicationError::BoundaryCapacityExceeded { .. }
                    | ComponentApplicationError::InternalCapacityExceeded { .. },
                ) => continue,
                Err(error) => return Err(error),
            };
            let oriented_outputs = output_order
                .iter()
                .map(|&component_coordinate| application.outputs[component_coordinate].clone())
                .collect::<Vec<_>>();
            if oriented_outputs == key.outputs {
                return Ok(Some(ComponentApplicationMatch {
                    component_input_order: input_order,
                    component_output_order: output_order,
                    application,
                }));
            }
        }
    }
    Ok(None)
}

/// One completely exhausted fixed-profile obligation in an application proof.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentProfileProof {
    /// Physical node count of the obligation.
    pub node_count: u32,
    /// Fixed number of internal physical links.
    pub internal_link_count: u32,
    /// Exact node-type inventory exhausted.
    pub profile: NodeProfile,
    /// Complete labeled physical bijections checked before canonical dedup.
    pub physical_bijections_checked: u64,
    /// Distinct canonical complete physical topologies checked.
    pub canonical_topologies_checked: u64,
}

/// Finite proof ledger for one concrete application optimum.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplicationOptimalityProof {
    /// Exact concrete application scope. No symbolic generalization is implied.
    pub key: ComponentApplicationKey,
    /// Canonical winning certified component.
    pub winner: ComponentCanonicalKey,
    /// Proven lexicographic component cost.
    pub cost: ComponentCost,
    /// Every lower/equal obligation exhausted in deterministic order.
    pub exhausted_profiles: Vec<ComponentProfileProof>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rational(value: &str) -> Rational {
        value.parse().unwrap()
    }

    fn signature(bits: &[bool]) -> BoundarySignature {
        BoundarySignature::from_feedback_relation(2, 2, bits.to_vec()).unwrap()
    }

    #[test]
    fn one_common_scale_includes_every_boundary_rate_and_capacity() {
        let base = ComponentApplicationRequest::new(
            &signature(&[true, false, false, true]),
            &[rational("2"), rational("3")],
            &[rational("1"), rational("4")],
            &rational("6"),
        )
        .unwrap();
        let scaled = ComponentApplicationRequest::new(
            &signature(&[true, false, false, true]),
            &[rational("20"), rational("30")],
            &[rational("10"), rational("40")],
            &rational("60"),
        )
        .unwrap();
        assert_eq!(base.key(), scaled.key());
        assert_eq!(base.common_scale(), &rational("1"));
        assert_eq!(scaled.common_scale(), &rational("10"));
        assert_eq!(scaled.key().max_link_rate(), &rational("6"));
    }

    #[test]
    fn changed_capacity_or_independent_coordinate_scale_cannot_alias() {
        let base = ComponentApplicationRequest::new(
            &signature(&[false, false, false, false]),
            &[rational("2"), rational("3")],
            &[rational("1"), rational("4")],
            &rational("6"),
        )
        .unwrap();
        let capacity = ComponentApplicationRequest::new(
            &signature(&[false, false, false, false]),
            &[rational("2"), rational("3")],
            &[rational("1"), rational("4")],
            &rational("7"),
        )
        .unwrap();
        let independent = ComponentApplicationRequest::new(
            &signature(&[false, false, false, false]),
            &[rational("4"), rational("3")],
            &[rational("2"), rational("5")],
            &rational("8"),
        )
        .unwrap();
        assert_ne!(base.key(), capacity.key());
        assert_ne!(base.key(), independent.key());
    }

    #[test]
    fn boundary_rates_and_feedback_relation_are_canonicalized_jointly() {
        let left = ComponentApplicationRequest::new(
            &signature(&[true, false, false, true]),
            &[rational("2"), rational("3")],
            &[rational("1"), rational("4")],
            &rational("6"),
        )
        .unwrap();
        let right = ComponentApplicationRequest::new(
            &signature(&[true, false, false, true]),
            &[rational("3"), rational("2")],
            &[rational("4"), rational("1")],
            &rational("6"),
        )
        .unwrap();
        assert_eq!(left.key(), right.key());

        let different_relation = ComponentApplicationRequest::new(
            &signature(&[true, true, false, false]),
            &[rational("2"), rational("3")],
            &[rational("1"), rational("4")],
            &rational("6"),
        )
        .unwrap();
        assert_ne!(left.key(), different_relation.key());
    }

    #[test]
    fn huge_fractional_values_normalize_without_machine_integer_decisions() {
        let huge = "1234567890123456789012345678901234567890";
        let input_a = rational(&format!("{huge}/97"));
        let input_b = rational(&format!("{huge}/89"));
        let total = &input_a + &input_b;
        let request = ComponentApplicationRequest::new(
            &BoundarySignature::from_feedback_relation(2, 1, vec![false, false]).unwrap(),
            &[input_a, input_b],
            std::slice::from_ref(&total),
            &total,
        )
        .unwrap();
        assert!(request.key().inputs().iter().all(Rational::is_positive));
        assert_eq!(
            request.key().inputs().iter().sum::<Rational>(),
            request.key().outputs().iter().sum::<Rational>()
        );
    }

    #[test]
    fn rejects_nonphysical_concrete_requests_before_work_deduplication() {
        let boundary = BoundarySignature::from_feedback_relation(1, 1, vec![false]).unwrap();
        assert!(matches!(
            ComponentApplicationRequest::new(
                &boundary,
                &[rational("2")],
                &[rational("1")],
                &rational("2"),
            ),
            Err(ComponentApplicationRequestError::Conservation { .. })
        ));
        assert!(matches!(
            ComponentApplicationRequest::new(
                &boundary,
                &[rational("2")],
                &[rational("2")],
                &rational("1"),
            ),
            Err(ComponentApplicationRequestError::BoundaryCapacityExceeded { .. })
        ));
    }
}
