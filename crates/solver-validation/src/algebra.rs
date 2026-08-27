use num::{BigInt, BigRational, Integer, One, Signed, Zero};
use thiserror::Error;

/// Failure returned by the independent exact linear-system solver.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum LinearSolveError {
    #[error("coefficient row {row} has {actual} columns, expected {expected}")]
    Ragged {
        row: usize,
        expected: usize,
        actual: usize,
    },
    #[error("the system has {rows} coefficient rows but {constants} constants")]
    ConstantCount { rows: usize, constants: usize },
    #[error("the exact system is inconsistent")]
    Inconsistent,
    #[error("the exact system is not unique: rank {rank} for {variables} variables")]
    NonUnique { rank: usize, variables: usize },
    #[error("fraction-free elimination encountered a non-exact division")]
    NonExactDivision,
}

/// Solve a rectangular rational system using fraction-free elimination.
///
/// Each equation is first cleared to a primitive integer row. Elimination then
/// uses the Bareiss recurrence, so no division occurs until a previous pivot is
/// known to divide the numerator exactly. The routine distinguishes inconsistent,
/// underdetermined, and uniquely solvable systems. It never uses floating point.
///
/// # Errors
///
/// Returns a structural error for ragged inputs, [`LinearSolveError::Inconsistent`]
/// for a contradictory system, or [`LinearSolveError::NonUnique`] when the
/// coefficient rank is smaller than the variable count.
pub fn solve_fraction_free(
    coefficients: &[Vec<BigRational>],
    constants: &[BigRational],
) -> Result<Vec<BigRational>, LinearSolveError> {
    if coefficients.len() != constants.len() {
        return Err(LinearSolveError::ConstantCount {
            rows: coefficients.len(),
            constants: constants.len(),
        });
    }

    let variables = coefficients.first().map_or(0, Vec::len);
    for (row, values) in coefficients.iter().enumerate() {
        if values.len() != variables {
            return Err(LinearSolveError::Ragged {
                row,
                expected: variables,
                actual: values.len(),
            });
        }
    }

    let mut matrix = coefficients
        .iter()
        .zip(constants)
        .map(|(row, constant)| integer_row(row, constant))
        .collect::<Vec<_>>();

    if variables == 0 {
        return if matrix.iter().all(|row| row[0].is_zero()) {
            Ok(Vec::new())
        } else {
            Err(LinearSolveError::Inconsistent)
        };
    }

    let mut pivot_columns = Vec::with_capacity(variables.min(matrix.len()));
    let mut pivot_row = 0;
    let mut previous_pivot = BigInt::one();

    for column in 0..variables {
        let Some(found) = (pivot_row..matrix.len()).find(|&row| !matrix[row][column].is_zero())
        else {
            continue;
        };
        matrix.swap(pivot_row, found);
        let pivot = matrix[pivot_row][column].clone();

        for row in (pivot_row + 1)..matrix.len() {
            let eliminated = matrix[row][column].clone();
            let pivot_values = matrix[pivot_row][(column + 1)..=variables].to_vec();
            for (target, pivot_value) in matrix[row][(column + 1)..=variables]
                .iter_mut()
                .zip(pivot_values)
            {
                let numerator = &*target * &pivot - &eliminated * pivot_value;
                let (quotient, remainder) = numerator.div_rem(&previous_pivot);
                if !remainder.is_zero() {
                    return Err(LinearSolveError::NonExactDivision);
                }
                *target = quotient;
            }
            matrix[row][column] = BigInt::zero();
        }

        pivot_columns.push(column);
        previous_pivot = pivot;
        pivot_row += 1;
        if pivot_row == matrix.len() {
            break;
        }
    }

    for row in &matrix {
        let all_zero = row[..variables].iter().all(Zero::is_zero);
        if all_zero && !row[variables].is_zero() {
            return Err(LinearSolveError::Inconsistent);
        }
    }

    let rank = pivot_columns.len();
    if rank < variables {
        return Err(LinearSolveError::NonUnique { rank, variables });
    }

    let mut solution = vec![BigRational::zero(); variables];
    for row in (0..rank).rev() {
        let column = pivot_columns[row];
        let mut value = BigRational::from_integer(matrix[row][variables].clone());
        for (coefficient, known) in matrix[row][(column + 1)..variables]
            .iter()
            .zip(&solution[(column + 1)..])
        {
            value -= BigRational::from_integer(coefficient.clone()) * known;
        }
        solution[column] = value / BigRational::from_integer(matrix[row][column].clone());
    }

    Ok(solution)
}

fn integer_row(coefficients: &[BigRational], constant: &BigRational) -> Vec<BigInt> {
    let denominator_lcm = coefficients
        .iter()
        .chain(std::iter::once(constant))
        .map(BigRational::denom)
        .fold(BigInt::one(), |lcm, denominator| lcm.lcm(denominator));

    let mut row = coefficients
        .iter()
        .chain(std::iter::once(constant))
        .map(|value| value.numer() * (&denominator_lcm / value.denom()))
        .collect::<Vec<_>>();
    let divisor = row
        .iter()
        .filter(|value| !value.is_zero())
        .fold(BigInt::zero(), |gcd, value| {
            if gcd.is_zero() {
                value.abs()
            } else {
                gcd.gcd(&value.abs())
            }
        });
    if !divisor.is_zero() && !divisor.is_one() {
        for value in &mut row {
            *value /= &divisor;
        }
    }
    if row
        .iter()
        .find(|value| !value.is_zero())
        .is_some_and(Signed::is_negative)
    {
        for value in &mut row {
            *value = -std::mem::take(value);
        }
    }
    row
}

#[cfg(test)]
mod tests {
    use super::*;

    fn integer(value: i64) -> BigRational {
        BigRational::from_integer(value.into())
    }

    #[test]
    fn solves_rectangular_unique_system() {
        let coefficients = vec![
            vec![integer(1), integer(1)],
            vec![integer(2), integer(-1)],
            vec![integer(3), integer(0)],
        ];
        let constants = vec![integer(5), integer(4), integer(9)];
        assert_eq!(
            solve_fraction_free(&coefficients, &constants).unwrap(),
            vec![integer(3), integer(2)]
        );
    }

    #[test]
    fn clears_rational_denominators_exactly() {
        let coefficients = vec![
            vec![
                BigRational::new(1.into(), 2.into()),
                BigRational::new(1.into(), 3.into()),
            ],
            vec![
                BigRational::new(1.into(), 7.into()),
                BigRational::new((-1).into(), 5.into()),
            ],
        ];
        let expected = vec![
            BigRational::new(10.into(), 3.into()),
            BigRational::new((-7).into(), 4.into()),
        ];
        let constants = coefficients
            .iter()
            .map(|row| {
                row.iter()
                    .zip(&expected)
                    .fold(BigRational::zero(), |sum, (left, right)| sum + left * right)
            })
            .collect::<Vec<_>>();
        assert_eq!(
            solve_fraction_free(&coefficients, &constants).unwrap(),
            expected
        );
    }

    #[test]
    fn distinguishes_inconsistent_and_nonunique_systems() {
        let singular = vec![vec![integer(1), integer(1)], vec![integer(2), integer(2)]];
        assert_eq!(
            solve_fraction_free(&singular, &[integer(1), integer(3)]),
            Err(LinearSolveError::Inconsistent)
        );
        assert_eq!(
            solve_fraction_free(&singular, &[integer(1), integer(2)]),
            Err(LinearSolveError::NonUnique {
                rank: 1,
                variables: 2
            })
        );
    }

    #[test]
    fn handles_very_large_coefficients_without_approximation() {
        let huge = BigInt::from(10_u8).pow(240) + BigInt::from(37_u8);
        let coefficients = vec![
            vec![BigRational::from_integer(huge.clone()), BigRational::one()],
            vec![BigRational::one(), BigRational::from_integer(huge.clone())],
        ];
        let expected = vec![
            BigRational::new(BigInt::from(10_u8).pow(180) + 3_u8, 17.into()),
            BigRational::new(-(BigInt::from(10_u8).pow(160) + 5_u8), 19.into()),
        ];
        let constants = coefficients
            .iter()
            .map(|row| {
                row.iter()
                    .zip(&expected)
                    .fold(BigRational::zero(), |sum, (left, right)| sum + left * right)
            })
            .collect::<Vec<_>>();
        assert_eq!(
            solve_fraction_free(&coefficients, &constants).unwrap(),
            expected
        );
    }

    #[test]
    fn agrees_with_exact_substitution_for_all_small_invertible_two_by_two_systems() {
        let expected = vec![
            BigRational::new(2.into(), 3.into()),
            BigRational::new((-5).into(), 7.into()),
        ];
        for a in -3..=3 {
            for b in -3..=3 {
                for c in -3..=3 {
                    for d in -3..=3 {
                        if a * d == b * c {
                            continue;
                        }
                        let coefficients =
                            vec![vec![integer(a), integer(b)], vec![integer(c), integer(d)]];
                        let constants = coefficients
                            .iter()
                            .map(|row| {
                                row.iter()
                                    .zip(&expected)
                                    .map(|(left, right)| left * right)
                                    .sum()
                            })
                            .collect::<Vec<BigRational>>();
                        assert_eq!(
                            solve_fraction_free(&coefficients, &constants).unwrap(),
                            expected,
                            "failed for [{a}, {b}; {c}, {d}]"
                        );
                    }
                }
            }
        }
    }
}
