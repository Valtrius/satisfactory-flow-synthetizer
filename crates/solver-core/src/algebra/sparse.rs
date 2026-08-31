//! Sparse, rollback-capable exact linear algebra for production propagation.
//!
//! Rows represent equations of the form `sum(a_i * x_i) = rhs`. Coefficients
//! and right-hand sides are arbitrary-precision integers. Analysis substitutes
//! exact known rationals, clears their denominators, and then uses independent
//! fraction-free elimination. Floating point never enters this module.

use std::{
    collections::{BTreeMap, BTreeSet},
    time::{Duration, Instant},
};

use num::{BigInt, BigRational, Integer, One, Signed, Zero};
use thiserror::Error;

use crate::topology::FlowVarId;

/// One canonical primitive integer equation.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SparseRow {
    coefficients: BTreeMap<FlowVarId, BigInt>,
    rhs: BigInt,
}

impl SparseRow {
    /// Builds and normalizes an integer equation.
    ///
    /// Duplicate terms are summed, zero terms are removed, the whole row is
    /// divided by the gcd of its entries, and its first nonzero entry is made
    /// positive. These operations multiply the equation by a nonzero rational,
    /// so they preserve exactly the same solution set.
    pub fn new(terms: impl IntoIterator<Item = (FlowVarId, BigInt)>, rhs: BigInt) -> Self {
        let mut coefficients = BTreeMap::<FlowVarId, BigInt>::new();
        for (variable, coefficient) in terms {
            *coefficients.entry(variable).or_default() += coefficient;
        }
        Self::from_parts(coefficients, rhs)
    }

    fn from_parts(mut coefficients: BTreeMap<FlowVarId, BigInt>, mut rhs: BigInt) -> Self {
        coefficients.retain(|_, coefficient| !coefficient.is_zero());

        let divisor = coefficients
            .values()
            .chain(std::iter::once(&rhs))
            .filter(|value| !value.is_zero())
            .fold(BigInt::zero(), |gcd, value| {
                if gcd.is_zero() {
                    value.abs()
                } else {
                    gcd.gcd(&value.abs())
                }
            });
        if !divisor.is_zero() && !divisor.is_one() {
            for coefficient in coefficients.values_mut() {
                *coefficient /= &divisor;
            }
            rhs /= &divisor;
        }

        let leading_is_negative = coefficients.values().next().unwrap_or(&rhs).is_negative();
        if leading_is_negative {
            for coefficient in coefficients.values_mut() {
                *coefficient = -std::mem::take(coefficient);
            }
            rhs = -rhs;
        }

        Self { coefficients, rhs }
    }

    /// Returns the nonzero coefficients in increasing [`FlowVarId`] order.
    #[must_use]
    pub const fn coefficients(&self) -> &BTreeMap<FlowVarId, BigInt> {
        &self.coefficients
    }

    /// Returns the integer right-hand side.
    #[must_use]
    pub const fn rhs(&self) -> &BigInt {
        &self.rhs
    }

    /// Returns whether this row is the identity `0 = 0`.
    #[must_use]
    pub fn is_tautology(&self) -> bool {
        self.coefficients.is_empty() && self.rhs.is_zero()
    }

    /// Returns whether this row is an explicit contradiction `0 = c`, `c != 0`.
    #[must_use]
    pub fn is_contradiction(&self) -> bool {
        self.coefficients.is_empty() && !self.rhs.is_zero()
    }

    /// Substitutes exact known values and returns another primitive integer row.
    ///
    /// Moving every known term to the right preserves the equation. Multiplying
    /// by the resulting right-hand-side denominator then preserves it because
    /// that denominator is strictly positive. Primitive normalization is also
    /// equivalence-preserving, as documented by [`SparseRow::new`].
    #[must_use]
    pub fn substitute(&self, known: &BTreeMap<FlowVarId, BigRational>) -> Self {
        let mut coefficients = BTreeMap::new();
        let mut rhs = BigRational::from_integer(self.rhs.clone());
        for (variable, coefficient) in &self.coefficients {
            if let Some(value) = known.get(variable) {
                rhs -= BigRational::from_integer(coefficient.clone()) * value;
            } else {
                coefficients.insert(*variable, coefficient.clone());
            }
        }

        let denominator = rhs.denom().clone();
        for coefficient in coefficients.values_mut() {
            *coefficient *= &denominator;
        }
        Self::from_parts(coefficients, rhs.numer().clone())
    }
}

/// Opaque rollback point for rows inserted after it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowCheckpoint {
    row_len: usize,
}

/// Result of inserting one normalized equation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowInsertion {
    /// A non-tautological row was retained, including an explicit contradiction.
    Added,
    /// The normalized equation was `0 = 0` and therefore imposed no constraint.
    Tautology,
}

/// Exact consistency of the analyzed equation system.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Consistency {
    /// The equations have at least one exact rational solution.
    Consistent,
    /// The equations imply an exact contradiction.
    Inconsistent,
}

/// A value fixed in every solution of the analyzed system.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnownValueDeduction {
    pub variable: FlowVarId,
    pub value: BigRational,
}

/// A homogeneous multiplicative equality fixed in every solution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RatioDeduction {
    /// Smaller variable identifier, used for deterministic orientation.
    pub lhs: FlowVarId,
    /// Larger variable identifier.
    pub rhs: FlowVarId,
    /// Exact factor in `lhs = factor * rhs`.
    pub factor: BigRational,
}

/// Exact rank and deductions obtained from an equivalent reduced row basis.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SparseAnalysis {
    pub consistency: Consistency,
    pub coefficient_rank: usize,
    pub augmented_rank: usize,
    pub variable_count: usize,
    pub pivot_variables: Vec<FlowVarId>,
    pub free_variables: Vec<FlowVarId>,
    pub known_values: Vec<KnownValueDeduction>,
    pub homogeneous_ratios: Vec<RatioDeduction>,
}

/// Diagnostic timings and matrix shape for one sparse analysis.
///
/// Production callers use [`SparseSystem::analyze_over`], which does not read
/// the clock. The hotspot recorder selects this profile only for benchmark runs.
#[derive(Clone, Debug, Default)]
pub(crate) struct SparseProfile {
    pub input_rows: usize,
    pub active_rows: usize,
    pub variable_count: usize,
    pub nonzero_terms: usize,
    pub preparation: Duration,
    pub variable_collection: Duration,
    pub substitution_normalization: Duration,
    pub tautology_filter: Duration,
    pub sorting: Duration,
    pub working_row_conversion: Duration,
    pub forward: Duration,
    pub back_reduction: Duration,
    pub deductions: Duration,
    pub total: Duration,
}

impl SparseAnalysis {
    /// Returns whether all requested variables have one exact solution.
    #[must_use]
    pub fn is_unique(&self) -> bool {
        self.consistency == Consistency::Consistent && self.coefficient_rank == self.variable_count
    }
}

/// Internal failure of production fraction-free elimination.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum SparseAlgebraError {
    /// Bareiss division should be exact for an integer matrix. A remainder means
    /// an implementation invariant was violated, not that the branch is unsat.
    #[error("Bareiss division was not exact at row {row}, pivot variable {pivot:?}")]
    NonExactBareissDivision { row: usize, pivot: FlowVarId },
}

/// Rollback-capable collection of sparse integer equations.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SparseSystem {
    rows: Vec<SparseRow>,
}

impl SparseSystem {
    /// Creates an empty equation system.
    #[must_use]
    pub const fn new() -> Self {
        Self { rows: Vec::new() }
    }

    /// Returns retained rows in insertion order.
    #[must_use]
    pub fn rows(&self) -> &[SparseRow] {
        &self.rows
    }

    /// Records a rollback point before speculative row insertion.
    #[must_use]
    pub fn checkpoint(&self) -> RowCheckpoint {
        RowCheckpoint {
            row_len: self.rows.len(),
        }
    }

    /// Removes every row inserted after `checkpoint`.
    ///
    /// A checkpoint from a different or already-shorter system is a programming
    /// error. Its opaque representation prevents callers from constructing one.
    ///
    /// # Panics
    ///
    /// Panics if the checkpoint refers to a longer row prefix than the current
    /// system.
    pub fn rollback(&mut self, checkpoint: RowCheckpoint) {
        assert!(
            checkpoint.row_len <= self.rows.len(),
            "row checkpoint must refer to an extant prefix"
        );
        self.rows.truncate(checkpoint.row_len);
    }

    /// Normalizes and inserts one equation.
    ///
    /// `0 = 0` is discarded. `0 = c`, `c != 0`, is retained so analysis cannot
    /// accidentally turn a contradiction into a tautology.
    pub fn insert(&mut self, row: SparseRow) -> RowInsertion {
        if row.is_tautology() {
            RowInsertion::Tautology
        } else {
            self.rows.push(row);
            RowInsertion::Added
        }
    }

    /// Analyzes variables present in the retained rows after known substitution.
    ///
    /// # Errors
    ///
    /// Returns [`SparseAlgebraError::NonExactBareissDivision`] only if the
    /// fraction-free implementation violates its integer divisibility invariant.
    pub fn analyze(
        &self,
        known: &BTreeMap<FlowVarId, BigRational>,
    ) -> Result<SparseAnalysis, SparseAlgebraError> {
        self.analyze_over(std::iter::empty(), known)
    }

    /// Analyzes the rows over an explicit variable set.
    ///
    /// Variables found in rows are always included. Extra requested variables
    /// remain visible as zero columns and are therefore reported as free. Known
    /// variables are substituted and removed from the unknown-variable count.
    ///
    /// # Errors
    ///
    /// Returns [`SparseAlgebraError::NonExactBareissDivision`] only for an
    /// internal exact-division failure.
    pub fn analyze_over(
        &self,
        variables: impl IntoIterator<Item = FlowVarId>,
        known: &BTreeMap<FlowVarId, BigRational>,
    ) -> Result<SparseAnalysis, SparseAlgebraError> {
        let (rows, variables) = self.prepare_analysis(variables, known);
        analyze_integer_rows(rows, &variables)
    }

    pub(crate) fn analyze_over_profiled(
        &self,
        variables: impl IntoIterator<Item = FlowVarId>,
        known: &BTreeMap<FlowVarId, BigRational>,
    ) -> Result<(SparseAnalysis, SparseProfile), SparseAlgebraError> {
        let total_started = Instant::now();
        let preparation_started = Instant::now();
        let input_rows = self.rows.len();
        let variable_started = Instant::now();
        let mut variable_set = variables.into_iter().collect::<BTreeSet<_>>();
        for row in &self.rows {
            variable_set.extend(row.coefficients.keys().copied());
        }
        for variable in known.keys() {
            variable_set.remove(variable);
        }
        let variables = variable_set.into_iter().collect::<Vec<_>>();
        let variable_collection = variable_started.elapsed();

        let substitution_started = Instant::now();
        let mut rows = self
            .rows
            .iter()
            .map(|row| row.substitute(known))
            .collect::<Vec<_>>();
        let substitution_normalization = substitution_started.elapsed();
        let filter_started = Instant::now();
        rows.retain(|row| !row.is_tautology());
        let tautology_filter = filter_started.elapsed();
        let sorting_started = Instant::now();
        rows.sort();
        let sorting = sorting_started.elapsed();
        let mut profile = SparseProfile {
            input_rows,
            active_rows: rows.len(),
            variable_count: variables.len(),
            nonzero_terms: rows.iter().map(|row| row.coefficients.len()).sum(),
            preparation: preparation_started.elapsed(),
            variable_collection,
            substitution_normalization,
            tautology_filter,
            sorting,
            ..SparseProfile::default()
        };
        let analysis = analyze_integer_rows_profiled(rows, &variables, &mut profile)?;
        profile.total = total_started.elapsed();
        Ok((analysis, profile))
    }

    fn prepare_analysis(
        &self,
        variables: impl IntoIterator<Item = FlowVarId>,
        known: &BTreeMap<FlowVarId, BigRational>,
    ) -> (Vec<SparseRow>, Vec<FlowVarId>) {
        let mut variable_set = variables.into_iter().collect::<BTreeSet<_>>();
        for row in &self.rows {
            variable_set.extend(row.coefficients.keys().copied());
        }
        for variable in known.keys() {
            variable_set.remove(variable);
        }
        let variables = variable_set.into_iter().collect::<Vec<_>>();

        let mut rows = self
            .rows
            .iter()
            .map(|row| row.substitute(known))
            .filter(|row| !row.is_tautology())
            .collect::<Vec<_>>();
        rows.sort();
        (rows, variables)
    }
}

#[derive(Clone, Debug)]
struct WorkingRow {
    coefficients: BTreeMap<FlowVarId, BigInt>,
    rhs: BigInt,
}

impl From<SparseRow> for WorkingRow {
    fn from(row: SparseRow) -> Self {
        Self {
            coefficients: row.coefficients,
            rhs: row.rhs,
        }
    }
}

fn analyze_integer_rows(
    rows: Vec<SparseRow>,
    variables: &[FlowVarId],
) -> Result<SparseAnalysis, SparseAlgebraError> {
    let mut rows = rows.into_iter().map(WorkingRow::from).collect::<Vec<_>>();
    let pivot_variables = bareiss_forward(&mut rows, variables)?;
    Ok(finish_analysis(rows, variables, pivot_variables))
}

fn analyze_integer_rows_profiled(
    rows: Vec<SparseRow>,
    variables: &[FlowVarId],
    profile: &mut SparseProfile,
) -> Result<SparseAnalysis, SparseAlgebraError> {
    let started = Instant::now();
    let mut rows = rows.into_iter().map(WorkingRow::from).collect::<Vec<_>>();
    profile.working_row_conversion = started.elapsed();
    let started = Instant::now();
    let pivot_variables = bareiss_forward(&mut rows, variables)?;
    profile.forward = started.elapsed();
    Ok(finish_analysis_profiled(
        rows,
        variables,
        pivot_variables,
        profile,
    ))
}

fn finish_analysis(
    mut rows: Vec<WorkingRow>,
    variables: &[FlowVarId],
    pivot_variables: Vec<FlowVarId>,
) -> SparseAnalysis {
    let coefficient_rank = pivot_variables.len();
    let inconsistent = rows
        .iter()
        .any(|row| row.coefficients.is_empty() && !row.rhs.is_zero());
    let consistency = if inconsistent {
        Consistency::Inconsistent
    } else {
        Consistency::Consistent
    };
    let augmented_rank = coefficient_rank + usize::from(inconsistent);
    let pivot_set = pivot_variables.iter().copied().collect::<BTreeSet<_>>();
    let free_variables = variables
        .iter()
        .filter(|variable| !pivot_set.contains(variable))
        .copied()
        .collect::<Vec<_>>();

    if inconsistent {
        return SparseAnalysis {
            consistency,
            coefficient_rank,
            augmented_rank,
            variable_count: variables.len(),
            pivot_variables,
            free_variables,
            known_values: Vec::new(),
            homogeneous_ratios: Vec::new(),
        };
    }

    fraction_free_back_reduce(&mut rows, &pivot_variables);
    let (known_values, homogeneous_ratios) = deductions(&rows, &pivot_variables);
    SparseAnalysis {
        consistency,
        coefficient_rank,
        augmented_rank,
        variable_count: variables.len(),
        pivot_variables,
        free_variables,
        known_values,
        homogeneous_ratios,
    }
}

fn finish_analysis_profiled(
    mut rows: Vec<WorkingRow>,
    variables: &[FlowVarId],
    pivot_variables: Vec<FlowVarId>,
    profile: &mut SparseProfile,
) -> SparseAnalysis {
    let coefficient_rank = pivot_variables.len();
    let inconsistent = rows
        .iter()
        .any(|row| row.coefficients.is_empty() && !row.rhs.is_zero());
    let consistency = if inconsistent {
        Consistency::Inconsistent
    } else {
        Consistency::Consistent
    };
    let augmented_rank = coefficient_rank + usize::from(inconsistent);
    let pivot_set = pivot_variables.iter().copied().collect::<BTreeSet<_>>();
    let free_variables = variables
        .iter()
        .filter(|variable| !pivot_set.contains(variable))
        .copied()
        .collect::<Vec<_>>();

    if inconsistent {
        return SparseAnalysis {
            consistency,
            coefficient_rank,
            augmented_rank,
            variable_count: variables.len(),
            pivot_variables,
            free_variables,
            known_values: Vec::new(),
            homogeneous_ratios: Vec::new(),
        };
    }

    let started = Instant::now();
    fraction_free_back_reduce(&mut rows, &pivot_variables);
    profile.back_reduction = started.elapsed();
    let started = Instant::now();
    let (known_values, homogeneous_ratios) = deductions(&rows, &pivot_variables);
    profile.deductions = started.elapsed();
    SparseAnalysis {
        consistency,
        coefficient_rank,
        augmented_rank,
        variable_count: variables.len(),
        pivot_variables,
        free_variables,
        known_values,
        homogeneous_ratios,
    }
}

fn bareiss_forward(
    rows: &mut [WorkingRow],
    variables: &[FlowVarId],
) -> Result<Vec<FlowVarId>, SparseAlgebraError> {
    let mut pivot_variables = Vec::with_capacity(variables.len().min(rows.len()));
    let mut pivot_row = 0;
    let mut previous_pivot = BigInt::one();

    for &variable in variables {
        let Some(found) = (pivot_row..rows.len())
            .find(|&row| coefficient(&rows[row], variable).is_some_and(|value| !value.is_zero()))
        else {
            continue;
        };
        rows.swap(pivot_row, found);
        let pivot = rows[pivot_row]
            .coefficients
            .get(&variable)
            .expect("selected pivot is nonzero")
            .clone();
        let pivot_source = rows[pivot_row].clone();

        for (row_index, target) in rows.iter_mut().enumerate().skip(pivot_row + 1) {
            bareiss_eliminate(
                target,
                &pivot_source,
                variable,
                &pivot,
                &previous_pivot,
                row_index,
            )?;
        }

        pivot_variables.push(variable);
        previous_pivot = pivot;
        pivot_row += 1;
        if pivot_row == rows.len() {
            break;
        }
    }
    Ok(pivot_variables)
}

fn coefficient(row: &WorkingRow, variable: FlowVarId) -> Option<&BigInt> {
    row.coefficients.get(&variable)
}

fn bareiss_eliminate(
    target: &mut WorkingRow,
    pivot_row: &WorkingRow,
    pivot_variable: FlowVarId,
    pivot: &BigInt,
    previous_pivot: &BigInt,
    row_index: usize,
) -> Result<(), SparseAlgebraError> {
    let eliminated = target
        .coefficients
        .get(&pivot_variable)
        .cloned()
        .unwrap_or_default();
    let active_variables = target
        .coefficients
        .range((
            std::ops::Bound::Excluded(pivot_variable),
            std::ops::Bound::Unbounded,
        ))
        .map(|(&variable, _)| variable)
        .chain(
            pivot_row
                .coefficients
                .range((
                    std::ops::Bound::Excluded(pivot_variable),
                    std::ops::Bound::Unbounded,
                ))
                .map(|(&variable, _)| variable),
        )
        .collect::<BTreeSet<_>>();
    let mut coefficients = BTreeMap::new();
    for variable in active_variables {
        let numerator = target
            .coefficients
            .get(&variable)
            .cloned()
            .unwrap_or_default()
            * pivot
            - &eliminated
                * pivot_row
                    .coefficients
                    .get(&variable)
                    .cloned()
                    .unwrap_or_default();
        let value = exact_bareiss_quotient(&numerator, previous_pivot, row_index, pivot_variable)?;
        if !value.is_zero() {
            coefficients.insert(variable, value);
        }
    }
    let rhs_numerator = &target.rhs * pivot - &eliminated * &pivot_row.rhs;
    let rhs = exact_bareiss_quotient(&rhs_numerator, previous_pivot, row_index, pivot_variable)?;
    target.coefficients = coefficients;
    target.rhs = rhs;
    Ok(())
}

fn exact_bareiss_quotient(
    numerator: &BigInt,
    denominator: &BigInt,
    row: usize,
    pivot: FlowVarId,
) -> Result<BigInt, SparseAlgebraError> {
    let (quotient, remainder) = numerator.div_rem(denominator);
    if remainder.is_zero() {
        Ok(quotient)
    } else {
        Err(SparseAlgebraError::NonExactBareissDivision { row, pivot })
    }
}

fn fraction_free_back_reduce(rows: &mut [WorkingRow], pivots: &[FlowVarId]) {
    for pivot_index in (0..pivots.len()).rev() {
        let pivot_variable = pivots[pivot_index];
        let pivot_row = rows[pivot_index].clone();
        let pivot = pivot_row
            .coefficients
            .get(&pivot_variable)
            .expect("echelon pivot remains nonzero")
            .clone();
        for target in &mut rows[..pivot_index] {
            let Some(eliminated) = target.coefficients.get(&pivot_variable).cloned() else {
                continue;
            };
            if eliminated.is_zero() {
                continue;
            }
            combine_rows(target, &pivot_row, &pivot, &eliminated);
        }
    }
}

fn combine_rows(
    target: &mut WorkingRow,
    pivot_row: &WorkingRow,
    pivot: &BigInt,
    eliminated: &BigInt,
) {
    let variables = target
        .coefficients
        .keys()
        .chain(pivot_row.coefficients.keys())
        .copied()
        .collect::<BTreeSet<_>>();
    let coefficients = variables
        .into_iter()
        .filter_map(|variable| {
            let value = target
                .coefficients
                .get(&variable)
                .cloned()
                .unwrap_or_default()
                * pivot
                - eliminated
                    * pivot_row
                        .coefficients
                        .get(&variable)
                        .cloned()
                        .unwrap_or_default();
            (!value.is_zero()).then_some((variable, value))
        })
        .collect::<BTreeMap<_, _>>();
    let rhs = &target.rhs * pivot - eliminated * &pivot_row.rhs;
    let normalized = SparseRow::from_parts(coefficients, rhs);
    target.coefficients = normalized.coefficients;
    target.rhs = normalized.rhs;
}

fn deductions(
    rows: &[WorkingRow],
    pivots: &[FlowVarId],
) -> (Vec<KnownValueDeduction>, Vec<RatioDeduction>) {
    let mut known_values = Vec::new();
    let mut ratios = BTreeMap::<(FlowVarId, FlowVarId), BigRational>::new();

    for (row, &pivot_variable) in rows.iter().zip(pivots) {
        let pivot = row
            .coefficients
            .get(&pivot_variable)
            .expect("back-reduced pivot remains nonzero");
        let other_terms = row
            .coefficients
            .iter()
            .filter(|(variable, _)| **variable != pivot_variable)
            .collect::<Vec<_>>();

        if other_terms.is_empty() {
            // The equivalent row is `a*x = b`, with `a != 0`. Division in the
            // rational field proves that every solution has exactly x = b/a.
            known_values.push(KnownValueDeduction {
                variable: pivot_variable,
                value: BigRational::new(row.rhs.clone(), pivot.clone()),
            });
        } else if row.rhs.is_zero() && other_terms.len() == 1 {
            // The equivalent row is `a*x + b*y = 0`, with both coefficients
            // nonzero. Rearrangement proves x = (-b/a)*y. No relation is emitted
            // from a row with a constant or a third variable.
            let (&other_variable, other) = other_terms[0];
            let pivot_factor = BigRational::new(-other.clone(), pivot.clone());
            let (lhs, rhs, factor) = if pivot_variable < other_variable {
                (pivot_variable, other_variable, pivot_factor)
            } else {
                (
                    other_variable,
                    pivot_variable,
                    BigRational::one() / pivot_factor,
                )
            };
            ratios.insert((lhs, rhs), factor);
        }
    }

    known_values.sort_by_key(|deduction| deduction.variable);
    let homogeneous_ratios = ratios
        .into_iter()
        .map(|((lhs, rhs), factor)| RatioDeduction { lhs, rhs, factor })
        .collect();
    (known_values, homogeneous_ratios)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn var(id: u32) -> FlowVarId {
        FlowVarId(id)
    }

    fn integer(value: i64) -> BigRational {
        BigRational::from_integer(value.into())
    }

    fn row<const N: usize>(terms: [(u32, i64); N], rhs: i64) -> SparseRow {
        SparseRow::new(
            terms.map(|(variable, coefficient)| (var(variable), coefficient.into())),
            rhs.into(),
        )
    }

    #[test]
    fn row_normalization_is_primitive_and_deterministic() {
        let normalized = SparseRow::new(
            [
                (var(2), BigInt::from(-6)),
                (var(1), BigInt::from(-4)),
                (var(2), BigInt::from(2)),
                (var(3), BigInt::zero()),
            ],
            BigInt::from(-8),
        );
        assert_eq!(normalized, row([(1, 1), (2, 1)], 2));
        assert_eq!(
            normalized
                .coefficients()
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            vec![var(1), var(2)]
        );
    }

    #[test]
    fn insertion_and_rollback_preserve_explicit_contradictions() {
        let mut system = SparseSystem::new();
        assert_eq!(system.insert(row([], 0)), RowInsertion::Tautology);
        let checkpoint = system.checkpoint();
        assert_eq!(system.insert(row([], -9)), RowInsertion::Added);
        assert!(system.rows()[0].is_contradiction());
        assert_eq!(
            system.analyze(&BTreeMap::new()).unwrap().consistency,
            Consistency::Inconsistent
        );
        system.rollback(checkpoint);
        assert!(system.rows().is_empty());
    }

    #[test]
    fn substitution_clears_denominators_back_to_a_primitive_integer_row() {
        let original = row([(0, 2), (1, 3)], 7);
        let known = BTreeMap::from([(var(1), BigRational::new(1.into(), 2.into()))]);
        let substituted = original.substitute(&known);
        assert_eq!(substituted, row([(0, 4)], 11));

        for numerator in -5..=5 {
            let x = BigRational::new(numerator.into(), 7.into());
            let original_lhs = integer(2) * &x + integer(3) * &known[&var(1)];
            let reduced_lhs = integer(4) * x;
            assert_eq!(original_lhs == integer(7), reduced_lhs == integer(11));
        }
    }

    #[test]
    fn unique_system_reports_rank_and_values_after_back_reduction() {
        let mut system = SparseSystem::new();
        system.insert(row([(0, 1), (1, 1)], 3));
        system.insert(row([(0, 1), (1, -1)], 1));
        let analysis = system.analyze(&BTreeMap::new()).unwrap();
        assert_eq!(analysis.consistency, Consistency::Consistent);
        assert_eq!(analysis.coefficient_rank, 2);
        assert_eq!(analysis.augmented_rank, 2);
        assert!(analysis.is_unique());
        assert_eq!(
            analysis.known_values,
            vec![
                KnownValueDeduction {
                    variable: var(0),
                    value: integer(2),
                },
                KnownValueDeduction {
                    variable: var(1),
                    value: integer(1),
                },
            ]
        );
    }

    #[test]
    fn profiled_analysis_preserves_results_and_records_matrix_shape() {
        let mut system = SparseSystem::new();
        system.insert(row([(0, 1), (1, 1)], 3));
        system.insert(row([(0, 1), (1, -1)], 1));
        let variables = [var(0), var(1)];
        let plain = system.analyze_over(variables, &BTreeMap::new()).unwrap();
        let (profiled, profile) = system
            .analyze_over_profiled(variables, &BTreeMap::new())
            .unwrap();

        assert_eq!(profiled, plain);
        assert_eq!(profile.input_rows, 2);
        assert_eq!(profile.active_rows, 2);
        assert_eq!(profile.variable_count, 2);
        assert_eq!(profile.nonzero_terms, 4);

        let mut ratio_system = SparseSystem::new();
        ratio_system.insert(row([(0, 1), (1, -2)], 0));
        let mut inconsistent_system = ratio_system.clone();
        inconsistent_system.insert(row([(0, 1), (1, -2)], 1));
        for system in [SparseSystem::new(), ratio_system, inconsistent_system] {
            let plain = system.analyze_over(variables, &BTreeMap::new()).unwrap();
            let (profiled, _) = system
                .analyze_over_profiled(variables, &BTreeMap::new())
                .unwrap();
            assert_eq!(profiled, plain);
        }
    }

    #[test]
    fn back_reduction_proves_only_exact_homogeneous_two_variable_ratios() {
        let mut system = SparseSystem::new();
        system.insert(row([(0, 1), (1, 1), (2, 1)], 0));
        system.insert(row([(1, 1), (2, -1)], 0));
        let analysis = system
            .analyze_over([var(0), var(1), var(2)], &BTreeMap::new())
            .unwrap();
        assert_eq!(analysis.free_variables, vec![var(2)]);
        assert_eq!(
            analysis.homogeneous_ratios,
            vec![
                RatioDeduction {
                    lhs: var(0),
                    rhs: var(2),
                    factor: integer(-2),
                },
                RatioDeduction {
                    lhs: var(1),
                    rhs: var(2),
                    factor: integer(1),
                },
            ]
        );

        let mut affine = SparseSystem::new();
        affine.insert(row([(0, 1), (1, -1)], 1));
        assert!(
            affine
                .analyze(&BTreeMap::new())
                .unwrap()
                .homogeneous_ratios
                .is_empty()
        );
    }

    #[test]
    fn singular_overdetermined_and_inconsistent_systems_are_distinguished() {
        let mut singular = SparseSystem::new();
        singular.insert(row([(0, 1), (1, 1)], 3));
        singular.insert(row([(0, 2), (1, 2)], 6));
        let singular_analysis = singular
            .analyze_over([var(0), var(1), var(2)], &BTreeMap::new())
            .unwrap();
        assert_eq!(singular_analysis.consistency, Consistency::Consistent);
        assert_eq!(singular_analysis.coefficient_rank, 1);
        assert_eq!(singular_analysis.variable_count, 3);
        assert_eq!(singular_analysis.free_variables, vec![var(1), var(2)]);

        singular.insert(row([(0, 1), (1, -1)], 1));
        let overdetermined = singular
            .analyze_over([var(0), var(1)], &BTreeMap::new())
            .unwrap();
        assert!(overdetermined.is_unique());
        assert_eq!(overdetermined.known_values.len(), 2);

        singular.insert(row([(0, 2), (1, 2)], 7));
        let inconsistent = singular.analyze(&BTreeMap::new()).unwrap();
        assert_eq!(inconsistent.consistency, Consistency::Inconsistent);
        assert_eq!(
            inconsistent.augmented_rank,
            inconsistent.coefficient_rank + 1
        );
        assert!(inconsistent.known_values.is_empty());
        assert!(inconsistent.homogeneous_ratios.is_empty());
    }

    #[test]
    fn very_large_coefficients_remain_exact() {
        let huge = BigInt::from(10_u8).pow(220) + BigInt::from(37_u8);
        let mut system = SparseSystem::new();
        system.insert(SparseRow::new(
            [(var(0), huge.clone()), (var(1), BigInt::one())],
            BigInt::one(),
        ));
        system.insert(SparseRow::new(
            [(var(0), BigInt::one()), (var(1), huge.clone())],
            BigInt::from(2),
        ));
        let analysis = system.analyze(&BTreeMap::new()).unwrap();
        let determinant: BigInt = &huge * &huge - 1;
        assert_eq!(
            analysis.known_values,
            vec![
                KnownValueDeduction {
                    variable: var(0),
                    value: BigRational::new(&huge - 2, determinant.clone()),
                },
                KnownValueDeduction {
                    variable: var(1),
                    value: BigRational::new(&huge * 2 - 1, determinant),
                },
            ]
        );
    }

    #[test]
    fn row_order_does_not_change_analysis() {
        let rows = [
            row([(0, 1), (1, 2), (2, -1)], 4),
            row([(0, 2), (1, -1)], 3),
            row([(1, 1), (2, 1)], 5),
        ];
        let mut forward = SparseSystem::new();
        let mut reverse = SparseSystem::new();
        for equation in &rows {
            forward.insert(equation.clone());
        }
        for equation in rows.iter().rev() {
            reverse.insert(equation.clone());
        }
        assert_eq!(
            forward.analyze(&BTreeMap::new()).unwrap(),
            reverse.analyze(&BTreeMap::new()).unwrap()
        );
    }

    #[test]
    fn exhaustive_small_matrices_match_direct_rational_gaussian_elimination() {
        let variables = [var(0), var(1)];
        for a in -2..=2 {
            for b in -2..=2 {
                for c in -2..=2 {
                    for d in -2..=2 {
                        for first_rhs in -1..=1 {
                            for second_rhs in -1..=1 {
                                let matrix = [[a, b], [c, d]];
                                let rhs = [first_rhs, second_rhs];
                                let direct = direct_rational_analysis(matrix, rhs);
                                let mut system = SparseSystem::new();
                                system.insert(row([(0, a), (1, b)], first_rhs));
                                system.insert(row([(0, c), (1, d)], second_rhs));
                                let sparse =
                                    system.analyze_over(variables, &BTreeMap::new()).unwrap();
                                assert_eq!(
                                    sparse.consistency, direct.consistency,
                                    "{matrix:?} = {rhs:?}"
                                );
                                assert_eq!(
                                    sparse.coefficient_rank, direct.rank,
                                    "{matrix:?} = {rhs:?}"
                                );
                                if let Some(solution) = direct.unique_solution {
                                    let values = sparse
                                        .known_values
                                        .iter()
                                        .map(|deduction| deduction.value.clone())
                                        .collect::<Vec<_>>();
                                    assert_eq!(values, solution, "{matrix:?} = {rhs:?}");
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    struct DirectAnalysis {
        consistency: Consistency,
        rank: usize,
        unique_solution: Option<Vec<BigRational>>,
    }

    fn direct_rational_analysis(matrix: [[i64; 2]; 2], rhs: [i64; 2]) -> DirectAnalysis {
        let mut rows = matrix
            .into_iter()
            .zip(rhs)
            .map(|(coefficients, rhs)| {
                [
                    integer(coefficients[0]),
                    integer(coefficients[1]),
                    integer(rhs),
                ]
            })
            .collect::<Vec<_>>();
        let mut pivot_columns = Vec::new();
        let mut pivot_row = 0;
        for column in 0..2 {
            let Some(found) = (pivot_row..rows.len()).find(|&row| !rows[row][column].is_zero())
            else {
                continue;
            };
            rows.swap(pivot_row, found);
            let pivot = rows[pivot_row][column].clone();
            for value in &mut rows[pivot_row][column..] {
                *value /= &pivot;
            }
            let source = rows[pivot_row].clone();
            for (row_index, target) in rows.iter_mut().enumerate() {
                if row_index == pivot_row {
                    continue;
                }
                let factor = target[column].clone();
                for index in column..=2 {
                    target[index] -= &factor * &source[index];
                }
            }
            pivot_columns.push(column);
            pivot_row += 1;
        }
        let inconsistent = rows
            .iter()
            .any(|row| row[..2].iter().all(Zero::is_zero) && !row[2].is_zero());
        let consistency = if inconsistent {
            Consistency::Inconsistent
        } else {
            Consistency::Consistent
        };
        let unique_solution = (!inconsistent && pivot_columns.len() == 2).then(|| {
            let mut solution = vec![BigRational::zero(); 2];
            for (row, &column) in pivot_columns.iter().enumerate() {
                solution[column] = rows[row][2].clone();
            }
            solution
        });
        DirectAnalysis {
            consistency,
            rank: pivot_columns.len(),
            unique_solution,
        }
    }
}
