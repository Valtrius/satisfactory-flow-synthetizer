//! One-sided exact lower bounds and fixed-profile impossibility proofs.
//!
//! Every function in this module is deliberately allowed to return no useful
//! proof. Search may prune only when a returned certificate follows from a
//! necessary condition shared by every physical completion.

use std::collections::BTreeMap;

use num::{BigInt, Integer, One, ToPrimitive, Zero};
use solver_api::{NodeProfile, Rational};
use thiserror::Error;

use crate::{problem::NormalizedProblem, profile::ProfileLinkAccounting};

const MAX_SUBSET_STATES: usize = 100_000;

/// Baseline independent node-count lower bounds combined only by `max`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaselineLowerBounds {
    /// Minimum number of strictly-positive capacity-bounded discard lines.
    pub minimum_discard_links: u32,
    /// Nodes forced by a net terminal branch deficit.
    pub splitter_branch_deficit: u32,
    /// Nodes forced by a net terminal merge deficit.
    pub merger_deficit: u32,
    /// Nodes forced jointly by exact source grain and port balance.
    pub arithmetic_nodes: u32,
    /// Sound combined starting point, never a sum of dependent arguments.
    pub combined_nodes: u32,
}

/// Checked-count failure while deriving a proof-only bound.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum LowerBoundError {
    #[error("terminal count cannot fit the public u32 proof type")]
    TerminalCountOverflow,
    #[error("minimum discard-link count cannot fit the public u32 proof type")]
    DiscardCountOverflow,
    #[error("exact arithmetic node lower bound cannot fit the public u32 proof type")]
    ArithmeticNodeCountOverflow,
}

/// A necessary fixed-profile condition that failed exactly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProfileImpossibilityProof {
    /// Selected splitters cannot create enough net producer branches.
    SplitterBranchDeficit { required: u32, available: u32 },
    /// Selected mergers cannot consume enough net producer branches.
    MergerDeficit { required: u32, available: u32 },
    /// With no mergers, one output cannot lie on any legal acyclic splitter path.
    AcyclicSplitterDepth { output_index: u32 },
    /// With no splitters, one output is not a sum of any subset of input lines.
    AcyclicMergerSubset { output_index: u32 },
    /// A pure-merger output needs more branch reductions than the profile owns.
    AcyclicMergerDepth {
        output_index: u32,
        required_reductions: u32,
        available_reductions: u32,
    },
    /// The profile's splitter product is too small for the endpoint source grain.
    SplitterDeterminantMagnitude {
        required_denominator: BigInt,
        available_product: BigInt,
    },
}

/// Computes the mandatory v1 terminal-count baseline without floating point.
///
/// # Soundness
///
/// A 2-way splitter increases the live branch count by at most one and a
/// 3-way splitter by at most two; mergers decrease it by the same respective
/// amounts. Positive surplus needs at least `ceil(surplus/B)` discard branches.
///
/// The arithmetic bound is valid for cyclic as well as acyclic networks. View
/// one unit of material as an absorbing flow chain: mergers have one
/// deterministic successor and a splitter of arity `d` chooses each successor
/// with weight `1/d`. For the transient internal matrix `Q`, clearing one row
/// by each splitter arity gives an integer determinant
///
/// `Delta = (product d) * det(I - Q)`.
///
/// `I-Q` is a nonsingular M-matrix. Exact elimination subtracts nonnegative
/// Schur-complement terms from its unit diagonal, so
/// `0 < det(I-Q) <= 1`; consequently `0 < Delta <= product d`. Cramer's rule
/// makes every source-to-boundary coefficient's reduced denominator divide
/// `Delta`. After clearing external denominators, let `g` be the GCD of all
/// input rates and `h` the GCD after adding outputs and total discard. Then
/// `q=g/h` divides `Delta` (Bezout applied to the normalized endpoints), hence
/// every feasible profile must satisfy `2^S2 * 3^S3 >= q`.
///
/// This source-grain argument uses the GCD of *all* inputs; it does not assume a
/// single source. It is combined jointly with exact port balance when deriving
/// `arithmetic_nodes`. Independent baseline bounds are combined only by `max`.
///
/// # Errors
///
/// Returns a checked-count error when the public proof counters cannot represent
/// the exact finite value.
pub fn baseline_lower_bounds(
    problem: &NormalizedProblem,
) -> Result<BaselineLowerBounds, LowerBoundError> {
    let inputs =
        u32::try_from(problem.inputs.len()).map_err(|_| LowerBoundError::TerminalCountOverflow)?;
    let outputs =
        u32::try_from(problem.outputs.len()).map_err(|_| LowerBoundError::TerminalCountOverflow)?;
    let minimum_discard_links = minimum_discard_links(&problem.surplus, &problem.max_link_rate)?;

    let minimum_consumers = outputs
        .checked_add(minimum_discard_links)
        .ok_or(LowerBoundError::TerminalCountOverflow)?;
    let splitter_branch_deficit = minimum_consumers.saturating_sub(inputs).div_ceil(2);

    // With zero surplus the discard count is exactly zero. With positive
    // surplus, arbitrarily fine positive rational discard partitions may move
    // the terminal branch delta toward zero, so claiming a merger bound from
    // line counts alone would be unsound.
    let merger_deficit = if problem.surplus.is_zero() {
        inputs.saturating_sub(outputs).div_ceil(2)
    } else {
        0
    };
    let required_denominator = required_source_denominator(problem);
    let arithmetic_nodes = arithmetic_node_lower_bound(
        &required_denominator,
        inputs,
        outputs,
        minimum_discard_links,
        !problem.surplus.is_zero(),
    )?;
    Ok(BaselineLowerBounds {
        minimum_discard_links,
        splitter_branch_deficit,
        merger_deficit,
        arithmetic_nodes,
        combined_nodes: splitter_branch_deficit
            .max(merger_deficit)
            .max(arithmetic_nodes),
    })
}

/// Returns the first exact necessary-condition failure for one accounted profile.
///
/// The arithmetic path/depth checks are restricted to pure splitter or pure
/// merger profiles. Such a positive-flow network cannot contain a live directed
/// cycle: a splitter-only cycle has multiplicative gain below one and no ingress,
/// while a merger-only cycle either has no egress or requires `x=x+p` for
/// positive `p`. Its output equations are therefore acyclic path division or
/// subset addition respectively.
#[must_use]
pub fn profile_impossibility(
    problem: &NormalizedProblem,
    profile: NodeProfile,
    accounting: ProfileLinkAccounting,
) -> Option<ProfileImpossibilityProof> {
    let input_count = u32::try_from(problem.inputs.len()).ok()?;
    let output_count = u32::try_from(problem.outputs.len()).ok()?;
    let consumer_terminals = output_count.checked_add(accounting.discard_link_count)?;
    let required_split = consumer_terminals.saturating_sub(input_count);
    let available_split = profile
        .splitter2
        .checked_add(profile.splitter3.checked_mul(2)?)?;
    if available_split < required_split {
        return Some(ProfileImpossibilityProof::SplitterBranchDeficit {
            required: required_split,
            available: available_split,
        });
    }
    let required_merge = input_count.saturating_sub(consumer_terminals);
    let available_merge = profile
        .merger2
        .checked_add(profile.merger3.checked_mul(2)?)?;
    if available_merge < required_merge {
        return Some(ProfileImpossibilityProof::MergerDeficit {
            required: required_merge,
            available: available_merge,
        });
    }

    if profile.merger2 == 0 && profile.merger3 == 0 {
        for (index, output) in problem.outputs.iter().enumerate() {
            if !problem.inputs.iter().any(|input| {
                splitter_path_can_produce(input, output, profile.splitter2, profile.splitter3)
            }) {
                return Some(ProfileImpossibilityProof::AcyclicSplitterDepth {
                    output_index: u32::try_from(index).ok()?,
                });
            }
        }
    }

    if profile.splitter2 == 0 && profile.splitter3 == 0 {
        let subset_cardinality = subset_sum_cardinalities(problem.inputs.as_slice())?;
        for (index, output) in problem.outputs.iter().enumerate() {
            let Some(&cardinality) = subset_cardinality.get(output) else {
                return Some(ProfileImpossibilityProof::AcyclicMergerSubset {
                    output_index: u32::try_from(index).ok()?,
                });
            };
            let required_reductions = cardinality.saturating_sub(1);
            if available_merge < required_reductions {
                return Some(ProfileImpossibilityProof::AcyclicMergerDepth {
                    output_index: u32::try_from(index).ok()?,
                    required_reductions,
                    available_reductions: available_merge,
                });
            }
        }
    }

    let required_denominator = required_source_denominator(problem);
    let available_product = splitter_arity_product(profile.splitter2, profile.splitter3);
    if available_product < required_denominator {
        return Some(ProfileImpossibilityProof::SplitterDeterminantMagnitude {
            required_denominator,
            available_product,
        });
    }
    None
}

fn required_source_denominator(problem: &NormalizedProblem) -> BigInt {
    let scale = problem
        .inputs
        .iter()
        .chain(problem.outputs.iter())
        .chain(std::iter::once(&problem.surplus))
        .map(Rational::denominator)
        .fold(BigInt::one(), |lcm, denominator| lcm.lcm(denominator));
    let integer_rate = |rate: &Rational| rate.numerator() * (&scale / rate.denominator());

    let input_gcd = problem
        .inputs
        .iter()
        .map(integer_rate)
        .fold(BigInt::zero(), |gcd, rate| gcd.gcd(&rate));
    debug_assert!(!input_gcd.is_zero());
    let endpoint_grain = problem
        .outputs
        .iter()
        .map(integer_rate)
        .chain(std::iter::once(integer_rate(&problem.surplus)))
        .fold(input_gcd.clone(), |gcd, rate| gcd.gcd(&rate));
    debug_assert!(!endpoint_grain.is_zero());
    input_gcd / endpoint_grain
}

fn arithmetic_node_lower_bound(
    required_denominator: &BigInt,
    input_count: u32,
    output_count: u32,
    minimum_discard_links: u32,
    has_surplus: bool,
) -> Result<u32, LowerBoundError> {
    let needs_feedback = !is_two_three_smooth(required_denominator);
    let terminal_delta = i64::from(output_count) - i64::from(input_count);
    let minimum_reduction = i64::from(needs_feedback);
    let required_branch_gain = if has_surplus {
        terminal_delta
            .checked_add(i64::from(minimum_discard_links))
            .and_then(|value| value.checked_add(minimum_reduction))
            .ok_or(LowerBoundError::ArithmeticNodeCountOverflow)?
    } else {
        terminal_delta
            .checked_add(minimum_reduction)
            .ok_or(LowerBoundError::ArithmeticNodeCountOverflow)?
    }
    .max(0);

    let arithmetic_threes = ceil_log(required_denominator, 3)?;
    let branch_threes = u32::try_from(required_branch_gain)
        .map_err(|_| LowerBoundError::ArithmeticNodeCountOverflow)?
        .div_ceil(2);
    let maximum_threes = arithmetic_threes.max(branch_threes);
    let mut power_of_three = BigInt::one();
    let mut best = None;
    for splitter3 in 0..=maximum_threes {
        let remaining_denominator = required_denominator.div_ceil(&power_of_three);
        let arithmetic_twos = ceil_log(&remaining_denominator, 2)?;
        let branch_gain_from_threes = i64::from(splitter3) * 2;
        let branch_twos = u32::try_from(
            required_branch_gain
                .checked_sub(branch_gain_from_threes)
                .ok_or(LowerBoundError::ArithmeticNodeCountOverflow)?
                .max(0),
        )
        .map_err(|_| LowerBoundError::ArithmeticNodeCountOverflow)?;
        let splitter2 = arithmetic_twos.max(branch_twos);
        let branch_gain = i64::from(splitter2) + branch_gain_from_threes;

        let mergers = if has_surplus {
            u32::from(needs_feedback)
        } else {
            let reduction = branch_gain
                .checked_sub(terminal_delta)
                .ok_or(LowerBoundError::ArithmeticNodeCountOverflow)?;
            debug_assert!(reduction >= minimum_reduction);
            u32::try_from(reduction)
                .map_err(|_| LowerBoundError::ArithmeticNodeCountOverflow)?
                .div_ceil(2)
        };
        let total = splitter2
            .checked_add(splitter3)
            .and_then(|value| value.checked_add(mergers))
            .ok_or(LowerBoundError::ArithmeticNodeCountOverflow)?;
        best = Some(best.map_or(total, |known: u32| known.min(total)));
        power_of_three *= 3_u8;
    }
    best.ok_or(LowerBoundError::ArithmeticNodeCountOverflow)
}

fn ceil_log(value: &BigInt, base: u8) -> Result<u32, LowerBoundError> {
    if value <= &BigInt::one() {
        return Ok(0);
    }
    let mut exponent = 0_u32;
    let mut power = BigInt::one();
    while power < *value {
        power *= base;
        exponent = exponent
            .checked_add(1)
            .ok_or(LowerBoundError::ArithmeticNodeCountOverflow)?;
    }
    Ok(exponent)
}

fn is_two_three_smooth(value: &BigInt) -> bool {
    let mut remaining = value.clone();
    while (&remaining % 2_u8).is_zero() {
        remaining /= 2_u8;
    }
    while (&remaining % 3_u8).is_zero() {
        remaining /= 3_u8;
    }
    remaining.is_one()
}

fn splitter_arity_product(splitter2: u32, splitter3: u32) -> BigInt {
    BigInt::from(2_u8).pow(splitter2) * BigInt::from(3_u8).pow(splitter3)
}

fn minimum_discard_links(surplus: &Rational, capacity: &Rational) -> Result<u32, LowerBoundError> {
    if surplus.is_zero() {
        return Ok(0);
    }
    let ratio = surplus.as_big_rational() / capacity.as_big_rational();
    ratio
        .numer()
        .div_ceil(ratio.denom())
        .to_u32()
        .ok_or(LowerBoundError::DiscardCountOverflow)
}

fn splitter_path_can_produce(
    input: &Rational,
    output: &Rational,
    splitter2: u32,
    splitter3: u32,
) -> bool {
    let Some(ratio) = input.checked_div(output) else {
        return false;
    };
    if ratio.denominator() != &BigInt::one() || ratio.numerator() <= &BigInt::from(0_u8) {
        return false;
    }
    let mut remaining = ratio.numerator().clone();
    let mut twos = 0_u32;
    let mut threes = 0_u32;
    while (&remaining % 2_u8) == BigInt::from(0_u8) {
        remaining /= 2_u8;
        let Some(next) = twos.checked_add(1) else {
            return false;
        };
        twos = next;
    }
    while (&remaining % 3_u8) == BigInt::from(0_u8) {
        remaining /= 3_u8;
        let Some(next) = threes.checked_add(1) else {
            return false;
        };
        threes = next;
    }
    remaining.is_one() && twos <= splitter2 && threes <= splitter3
}

fn subset_sum_cardinalities(inputs: &[Rational]) -> Option<BTreeMap<Rational, u32>> {
    let mut sums = BTreeMap::from([(Rational::zero(), 0_u32)]);
    for input in inputs {
        let additions = sums
            .iter()
            .map(|(sum, &count)| Some((sum + input, count.checked_add(1)?)))
            .collect::<Option<Vec<_>>>()?;
        for (sum, count) in additions {
            sums.entry(sum)
                .and_modify(|known| *known = (*known).min(count))
                .or_insert(count);
        }
        if sums.len() > MAX_SUBSET_STATES {
            // Resource exhaustion disables the optimization; it is never an
            // impossibility proof.
            return None;
        }
    }
    Some(sums)
}

#[cfg(test)]
mod tests {
    use solver_api::Problem;

    use super::*;
    use crate::{Preparation, prepare_problem, profile::profile_link_accounting};

    fn normalized(inputs: &[u32], outputs: &[u32], capacity: u32) -> NormalizedProblem {
        let problem = Problem {
            inputs: inputs.iter().copied().map(Rational::from).collect(),
            outputs: outputs.iter().copied().map(Rational::from).collect(),
            max_link_rate: Rational::from(capacity),
        };
        match prepare_problem(&problem).unwrap() {
            Preparation::Prepared(problem) => problem,
            Preparation::GloballyUnsat(_) => panic!("test problem must pass global checks"),
        }
    }

    fn accounting(problem: &NormalizedProblem, profile: NodeProfile) -> ProfileLinkAccounting {
        profile_link_accounting(
            profile,
            u32::try_from(problem.inputs.len()).unwrap(),
            u32::try_from(problem.outputs.len()).unwrap(),
            &problem.surplus,
            &problem.max_link_rate,
        )
        .unwrap()
        .unwrap()
    }

    #[test]
    fn independent_terminal_deficits_are_combined_by_max() {
        let split = normalized(&[4], &[1, 1, 1, 1], 4);
        assert_eq!(
            baseline_lower_bounds(&split).unwrap(),
            BaselineLowerBounds {
                minimum_discard_links: 0,
                splitter_branch_deficit: 2,
                merger_deficit: 0,
                arithmetic_nodes: 2,
                combined_nodes: 2,
            }
        );
        let merge = normalized(&[1, 1, 1, 1], &[4], 4);
        assert_eq!(baseline_lower_bounds(&merge).unwrap().combined_nodes, 2);
    }

    #[test]
    fn positive_surplus_discard_partitions_do_not_invent_a_merger_bound() {
        let problem = normalized(&[1, 1, 1, 1], &[1], 4);
        let bounds = baseline_lower_bounds(&problem).unwrap();
        assert_eq!(bounds.minimum_discard_links, 1);
        assert_eq!(bounds.merger_deficit, 0);
        assert_eq!(bounds.combined_nodes, 0);
    }

    #[test]
    fn cyclic_safe_source_grain_starts_216_case_at_seven_nodes() {
        let problem = normalized(&[216], &[66, 150], 1_200);
        let bounds = baseline_lower_bounds(&problem).unwrap();
        assert_eq!(required_source_denominator(&problem), BigInt::from(36));
        assert_eq!(bounds.arithmetic_nodes, 7);
        assert_eq!(bounds.combined_nodes, 7);
    }

    #[test]
    fn source_grain_uses_the_gcd_of_every_input() {
        let problem = normalized(&[72, 144], &[66, 150], 1_200);
        let bounds = baseline_lower_bounds(&problem).unwrap();
        assert_eq!(required_source_denominator(&problem), BigInt::from(12));
        assert_eq!(bounds.arithmetic_nodes, 5);
        assert_eq!(bounds.combined_nodes, 5);

        let permuted = normalized(&[144, 72], &[150, 66], 1_200);
        assert_eq!(baseline_lower_bounds(&permuted).unwrap(), bounds);
    }

    #[test]
    fn source_grain_bound_is_invariant_under_common_rate_scaling() {
        let base = normalized(&[216], &[66, 150], 1_200);
        let scaled = normalized(&[2_160], &[660, 1_500], 12_000);
        assert_eq!(
            baseline_lower_bounds(&base).unwrap(),
            baseline_lower_bounds(&scaled).unwrap()
        );
    }

    #[test]
    fn profile_bound_rejects_insufficient_cyclic_determinant_magnitude() {
        let problem = normalized(&[216], &[66, 150], 1_200);
        let profile = NodeProfile {
            splitter3: 1,
            merger2: 1,
            ..NodeProfile::default()
        };
        let accounting =
            profile_link_accounting(profile, 1, 2, &problem.surplus, &problem.max_link_rate)
                .unwrap()
                .unwrap();
        assert!(matches!(
            profile_impossibility(&problem, profile, accounting),
            Some(ProfileImpossibilityProof::SplitterDeterminantMagnitude {
                required_denominator,
                available_product,
            }) if required_denominator == BigInt::from(36)
                && available_product == BigInt::from(3)
        ));
    }

    #[test]
    fn pure_splitter_depth_rejects_non_two_three_denominators() {
        let problem = normalized(&[5], &[1], 5);
        let profile = NodeProfile {
            splitter2: 1,
            ..NodeProfile::default()
        };
        assert!(matches!(
            profile_impossibility(&problem, profile, accounting(&problem, profile)),
            Some(ProfileImpossibilityProof::AcyclicSplitterDepth { output_index: 0 })
        ));
        assert!(splitter_path_can_produce(
            &Rational::from(6),
            &Rational::one(),
            1,
            1
        ));
    }

    #[test]
    fn pure_merger_subset_proof_uses_exact_input_sums() {
        let problem = normalized(&[2, 3, 5], &[4], 6);
        let profile = NodeProfile {
            merger2: 1,
            ..NodeProfile::default()
        };
        assert!(matches!(
            profile_impossibility(&problem, profile, accounting(&problem, profile)),
            Some(ProfileImpossibilityProof::AcyclicMergerSubset { output_index: 0 })
        ));
    }
}
