//! Exact integer linear inequalities used for physical-flow bounds.
//!
//! The representation deliberately keeps strictness explicit. In particular,
//! physical positivity is `-x < 0`, not the weaker `-x <= 0`. Evaluation only
//! reports a contradiction after every variable in the row has an exact known
//! value. An unresolved row is never used to prune a search branch.

use std::collections::BTreeMap;

use num::{BigInt, Integer, One, Signed, Zero};
use solver_api::Rational;

use crate::topology::FlowVarId;

/// Whether an exact integer row uses `<` or `<=`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InequalityRelation {
    /// The left-hand side must be strictly less than the right-hand side.
    LessThan,
    /// The left-hand side may equal the right-hand side.
    LessThanOrEqual,
}

/// A primitive deterministic integer inequality `sum(a_i*x_i) relation rhs`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExactInequality {
    terms: BTreeMap<FlowVarId, BigInt>,
    rhs: BigInt,
    relation: InequalityRelation,
}

impl ExactInequality {
    /// Combines duplicate variables, removes zero coefficients, and divides the
    /// complete row by its positive integer GCD.
    #[must_use]
    pub fn new(
        terms: impl IntoIterator<Item = (FlowVarId, BigInt)>,
        rhs: BigInt,
        relation: InequalityRelation,
    ) -> Self {
        let mut combined = BTreeMap::<FlowVarId, BigInt>::new();
        for (variable, coefficient) in terms {
            *combined.entry(variable).or_default() += coefficient;
        }
        combined.retain(|_, coefficient| !coefficient.is_zero());

        // Dividing both sides by a positive integer preserves both inequality
        // direction and strictness. We must not flip the row's sign.
        let divisor = combined
            .values()
            .fold(rhs.abs(), |gcd, coefficient| gcd.gcd(&coefficient.abs()));
        let divisor = if divisor.is_zero() {
            BigInt::one()
        } else {
            divisor
        };
        for coefficient in combined.values_mut() {
            *coefficient /= &divisor;
        }

        Self {
            terms: combined,
            rhs: rhs / divisor,
            relation,
        }
    }

    /// Borrows the deterministically ordered nonzero coefficients.
    #[must_use]
    pub const fn terms(&self) -> &BTreeMap<FlowVarId, BigInt> {
        &self.terms
    }

    /// Borrows the integer right-hand side.
    #[must_use]
    pub const fn rhs(&self) -> &BigInt {
        &self.rhs
    }

    /// Returns this row's exact comparison relation.
    #[must_use]
    pub const fn relation(&self) -> InequalityRelation {
        self.relation
    }

    /// Evaluates the row when every referenced variable is known exactly.
    ///
    /// Returning [`InequalityEvaluation::Violated`] is a mathematical
    /// impossibility proof: substituting exact equalities into an exact required
    /// inequality leaves a false closed statement. Missing facts return
    /// `Undetermined` and can never eliminate a branch.
    #[must_use]
    pub fn evaluate_known(&self, known: &BTreeMap<FlowVarId, Rational>) -> InequalityEvaluation {
        let mut lhs = Rational::zero();
        for (variable, coefficient) in &self.terms {
            let Some(value) = known.get(variable) else {
                return InequalityEvaluation::Undetermined;
            };
            lhs = &lhs + &(&Rational::from_integer(coefficient.clone()) * value);
        }
        let rhs = Rational::from_integer(self.rhs.clone());
        let satisfied = match self.relation {
            InequalityRelation::LessThan => lhs < rhs,
            InequalityRelation::LessThanOrEqual => lhs <= rhs,
        };
        if satisfied {
            InequalityEvaluation::Satisfied
        } else {
            InequalityEvaluation::Violated
        }
    }
}

/// Result of substituting exact known values into an inequality.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InequalityEvaluation {
    /// Every variable was known and the inequality is true.
    Satisfied,
    /// Every variable was known and the inequality is false.
    Violated,
    /// At least one variable remains unresolved.
    Undetermined,
}

/// The mandatory exact bounds for one represented physical flow variable.
///
/// The returned rows encode `-x < 0` and `denominator(B)*x <= numerator(B)`.
/// No floating-point conversion participates in their construction.
#[must_use]
pub fn physical_flow_constraints(variable: FlowVarId, capacity: &Rational) -> [ExactInequality; 2] {
    let positivity = ExactInequality::new(
        [(variable, BigInt::from(-1))],
        BigInt::zero(),
        InequalityRelation::LessThan,
    );
    let capacity = ExactInequality::new(
        [(variable, capacity.denominator().clone())],
        capacity.numerator().clone(),
        InequalityRelation::LessThanOrEqual,
    );
    [positivity, capacity]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rational(value: &str) -> Rational {
        value.parse().unwrap()
    }

    fn known(variable: FlowVarId, value: &str) -> BTreeMap<FlowVarId, Rational> {
        [(variable, rational(value))].into_iter().collect()
    }

    #[test]
    fn physical_bounds_accept_exact_capacity_and_small_positive_values() {
        let variable = FlowVarId(7);
        let [positive, capacity] = physical_flow_constraints(variable, &rational("5/2"));
        assert_eq!(
            positive.evaluate_known(&known(variable, "1/1000000")),
            InequalityEvaluation::Satisfied
        );
        assert_eq!(
            capacity.evaluate_known(&known(variable, "5/2")),
            InequalityEvaluation::Satisfied
        );
    }

    #[test]
    fn physical_bounds_reject_zero_negative_and_over_capacity_exactly() {
        let variable = FlowVarId(3);
        let [positive, capacity] = physical_flow_constraints(variable, &rational("5/2"));
        assert_eq!(
            positive.evaluate_known(&known(variable, "0")),
            InequalityEvaluation::Violated
        );
        assert_eq!(
            positive.evaluate_known(&known(variable, "-1/9")),
            InequalityEvaluation::Violated
        );
        assert_eq!(
            capacity.evaluate_known(&known(
                variable,
                "25000000000000000001/10000000000000000000"
            )),
            InequalityEvaluation::Violated
        );
    }

    #[test]
    fn missing_exact_fact_never_becomes_a_prune() {
        let variable = FlowVarId(1);
        for constraint in physical_flow_constraints(variable, &Rational::from(4_u8)) {
            assert_eq!(
                constraint.evaluate_known(&BTreeMap::new()),
                InequalityEvaluation::Undetermined
            );
        }
    }

    #[test]
    fn canonicalization_combines_terms_and_uses_only_positive_gcd_scaling() {
        let variable = FlowVarId(9);
        let row = ExactInequality::new(
            [(variable, BigInt::from(-12)), (variable, BigInt::from(4))],
            BigInt::from(-16),
            InequalityRelation::LessThanOrEqual,
        );
        assert_eq!(
            row.terms(),
            &[(variable, BigInt::from(-1))].into_iter().collect()
        );
        assert_eq!(row.rhs(), &BigInt::from(-2));
        assert_eq!(row.relation(), InequalityRelation::LessThanOrEqual);
    }

    #[test]
    fn multi_variable_rows_use_exact_rational_substitution() {
        let left = FlowVarId(0);
        let right = FlowVarId(1);
        let row = ExactInequality::new(
            [(left, BigInt::from(3)), (right, BigInt::from(-2))],
            BigInt::from(1),
            InequalityRelation::LessThan,
        );
        let values = [(left, rational("1/3")), (right, rational("1/2"))]
            .into_iter()
            .collect();
        assert_eq!(row.evaluate_known(&values), InequalityEvaluation::Satisfied);
    }
}
