use std::{
    fmt,
    iter::Sum,
    ops::{Add, Div, Mul, Neg, Sub},
    str::FromStr,
};

use num::{BigInt, BigRational, One, Signed, Zero};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use thiserror::Error;

/// An arbitrary-precision rational number in canonical reduced form.
///
/// `Rational` parses integers, finite decimals, and fractions without using floating point.
/// Its serialized representation is always a JSON string containing the canonical integer or
/// fraction, such as `"12"` or `"1/3"`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Rational(BigRational);

/// A syntax or domain error returned while constructing an exact [`Rational`].
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum RationalParseError {
    /// The supplied text contains no number.
    #[error("rate is required")]
    Empty,
    /// A fraction contains more than one slash.
    #[error("fraction must contain exactly one slash")]
    MultipleSlashes,
    /// A fraction has no numerator.
    #[error("fraction is missing its numerator")]
    MissingNumerator,
    /// A fraction has no denominator.
    #[error("fraction is missing its denominator")]
    MissingDenominator,
    /// The fraction denominator is zero.
    #[error("fraction denominator cannot be zero")]
    ZeroDenominator,
    /// The value contains more than one decimal point.
    #[error("rate contains more than one decimal point")]
    MultipleDecimalPoints,
    /// A decimal contains a character other than a sign, digit, or its one decimal point.
    #[error("'{0}' is not an integer, decimal, or fraction")]
    InvalidNumber(String),
}

impl Rational {
    /// Constructs and reduces `numerator / denominator`.
    ///
    /// # Errors
    ///
    /// Returns [`RationalParseError::ZeroDenominator`] when `denominator` is zero.
    pub fn new(
        numerator: impl Into<BigInt>,
        denominator: impl Into<BigInt>,
    ) -> Result<Self, RationalParseError> {
        let numerator = numerator.into();
        let denominator = denominator.into();
        if denominator.is_zero() {
            return Err(RationalParseError::ZeroDenominator);
        }
        Ok(Self(BigRational::new(numerator, denominator)))
    }

    /// Constructs an exact integer.
    #[must_use]
    pub fn from_integer(value: impl Into<BigInt>) -> Self {
        Self(BigRational::from_integer(value.into()))
    }

    /// Returns exact zero.
    #[must_use]
    pub fn zero() -> Self {
        Self(BigRational::zero())
    }

    /// Returns exact one.
    #[must_use]
    pub fn one() -> Self {
        Self(BigRational::one())
    }

    /// Borrows the canonical numerator.
    #[must_use]
    pub fn numerator(&self) -> &BigInt {
        self.0.numer()
    }

    /// Borrows the positive canonical denominator.
    #[must_use]
    pub fn denominator(&self) -> &BigInt {
        self.0.denom()
    }

    /// Borrows the underlying arbitrary-precision ratio.
    #[must_use]
    pub const fn as_big_rational(&self) -> &BigRational {
        &self.0
    }

    /// Consumes this value and returns its underlying arbitrary-precision ratio.
    #[must_use]
    pub fn into_big_rational(self) -> BigRational {
        self.0
    }

    /// Returns `true` when this value is exactly zero.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.0.is_zero()
    }

    /// Returns `true` when this value is strictly positive.
    #[must_use]
    pub fn is_positive(&self) -> bool {
        self.0.is_positive()
    }

    /// Returns `true` when this value is strictly negative.
    #[must_use]
    pub fn is_negative(&self) -> bool {
        self.0.is_negative()
    }

    /// Returns the exact absolute value.
    #[must_use]
    pub fn abs(&self) -> Self {
        Self(self.0.abs())
    }

    /// Returns the exact reciprocal, or `None` for zero.
    #[must_use]
    pub fn reciprocal(&self) -> Option<Self> {
        (!self.is_zero()).then(|| Self(self.0.recip()))
    }

    /// Divides exactly, returning `None` when `divisor` is zero.
    #[must_use]
    pub fn checked_div(&self, divisor: &Self) -> Option<Self> {
        divisor.reciprocal().map(|reciprocal| self * &reciprocal)
    }
}

impl Default for Rational {
    fn default() -> Self {
        Self::zero()
    }
}

impl fmt::Display for Rational {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.denominator().is_one() {
            self.numerator().fmt(formatter)
        } else {
            write!(formatter, "{}/{}", self.numerator(), self.denominator())
        }
    }
}

impl FromStr for Rational {
    type Err = RationalParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let value = value.trim();
        if value.is_empty() {
            return Err(RationalParseError::Empty);
        }

        if let Some((numerator, denominator)) = value.split_once('/') {
            if denominator.contains('/') {
                return Err(RationalParseError::MultipleSlashes);
            }
            let numerator = numerator.trim();
            let denominator = denominator.trim();
            if numerator.is_empty() {
                return Err(RationalParseError::MissingNumerator);
            }
            if denominator.is_empty() {
                return Err(RationalParseError::MissingDenominator);
            }
            let numerator = parse_signed_integer(numerator, value)?;
            let denominator = parse_signed_integer(denominator, value)?;
            return Self::new(numerator, denominator);
        }

        parse_decimal(value)
    }
}

fn parse_signed_integer(value: &str, original: &str) -> Result<BigInt, RationalParseError> {
    value
        .parse::<BigInt>()
        .map_err(|_| RationalParseError::InvalidNumber(original.to_owned()))
}

fn parse_decimal(value: &str) -> Result<Rational, RationalParseError> {
    let (negative, unsigned) = match value.as_bytes().first() {
        Some(b'-') => (true, &value[1..]),
        Some(b'+') => (false, &value[1..]),
        _ => (false, value),
    };
    if unsigned.is_empty() {
        return Err(RationalParseError::InvalidNumber(value.to_owned()));
    }

    let (whole, fraction) = if let Some((whole, fraction)) = unsigned.split_once('.') {
        if fraction.contains('.') {
            return Err(RationalParseError::MultipleDecimalPoints);
        }
        (whole, Some(fraction))
    } else {
        (unsigned, None)
    };
    if whole.is_empty() && fraction.is_none_or(str::is_empty) {
        return Err(RationalParseError::InvalidNumber(value.to_owned()));
    }
    if !whole.chars().all(|character| character.is_ascii_digit())
        || fraction
            .is_some_and(|digits| !digits.chars().all(|character| character.is_ascii_digit()))
    {
        return Err(RationalParseError::InvalidNumber(value.to_owned()));
    }

    let fraction = fraction.unwrap_or_default();
    let digits = format!("{}{}", if whole.is_empty() { "0" } else { whole }, fraction);
    let mut numerator = parse_signed_integer(&digits, value)?;
    if negative {
        numerator = -numerator;
    }
    let exponent = u32::try_from(fraction.len())
        .map_err(|_| RationalParseError::InvalidNumber(value.to_owned()))?;
    let denominator = BigInt::from(10_u8).pow(exponent);
    Rational::new(numerator, denominator)
}

impl Serialize for Rational {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Rational {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(D::Error::custom)
    }
}

impl From<BigRational> for Rational {
    fn from(value: BigRational) -> Self {
        Self(value)
    }
}

impl From<Rational> for BigRational {
    fn from(value: Rational) -> Self {
        value.0
    }
}

impl From<BigInt> for Rational {
    fn from(value: BigInt) -> Self {
        Self::from_integer(value)
    }
}

macro_rules! impl_integer_from {
    ($($integer:ty),+ $(,)?) => {
        $(
            impl From<$integer> for Rational {
                fn from(value: $integer) -> Self {
                    Self::from_integer(value)
                }
            }
        )+
    };
}

impl_integer_from!(
    i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize
);

macro_rules! impl_binary_operator {
    ($trait:ident, $method:ident, $operator:tt) => {
        impl $trait for Rational {
            type Output = Rational;

            fn $method(self, rhs: Self) -> Self::Output {
                Rational(self.0 $operator rhs.0)
            }
        }

        impl $trait<&Rational> for Rational {
            type Output = Rational;

            fn $method(self, rhs: &Rational) -> Self::Output {
                Rational(self.0 $operator &rhs.0)
            }
        }

        impl $trait<Rational> for &Rational {
            type Output = Rational;

            fn $method(self, rhs: Rational) -> Self::Output {
                Rational(&self.0 $operator rhs.0)
            }
        }

        impl $trait<&Rational> for &Rational {
            type Output = Rational;

            fn $method(self, rhs: &Rational) -> Self::Output {
                Rational(&self.0 $operator &rhs.0)
            }
        }
    };
}

impl_binary_operator!(Add, add, +);
impl_binary_operator!(Sub, sub, -);
impl_binary_operator!(Mul, mul, *);
impl_binary_operator!(Div, div, /);

impl Neg for Rational {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self(-self.0)
    }
}

impl Neg for &Rational {
    type Output = Rational;

    fn neg(self) -> Self::Output {
        Rational(-&self.0)
    }
}

impl Sum for Rational {
    fn sum<I: Iterator<Item = Self>>(values: I) -> Self {
        values.fold(Self::zero(), |sum, value| sum + value)
    }
}

impl<'a> Sum<&'a Rational> for Rational {
    fn sum<I: Iterator<Item = &'a Rational>>(values: I) -> Self {
        values.fold(Self::zero(), |sum, value| sum + value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_canonicalizes_supported_syntax() {
        let cases = [
            ("50", "50"),
            ("1/3", "1/3"),
            ("12.5", "25/2"),
            (".25", "1/4"),
            ("12.", "12"),
            ("-6/-8", "3/4"),
            ("+0007.500", "15/2"),
        ];
        for (source, canonical) in cases {
            assert_eq!(source.parse::<Rational>().unwrap().to_string(), canonical);
        }
    }

    #[test]
    fn rejects_malformed_or_undefined_values() {
        for value in ["", ".", "1/0", "/2", "2/", "1/2/3", "1.2.3", "NaN"] {
            assert!(value.parse::<Rational>().is_err(), "accepted {value}");
        }
    }

    #[test]
    fn serde_uses_only_canonical_strings() {
        let value = "12.50".parse::<Rational>().unwrap();
        assert_eq!(serde_json::to_string(&value).unwrap(), "\"25/2\"");
        assert_eq!(
            serde_json::from_str::<Rational>("\"12.50\"").unwrap(),
            value
        );
        assert!(serde_json::from_str::<Rational>("12.5").is_err());
    }

    #[test]
    fn canonical_display_round_trips_for_many_ratios() {
        for numerator in -40..=40 {
            for denominator in -20..=20 {
                if denominator == 0 {
                    continue;
                }
                let value = Rational::new(numerator, denominator).unwrap();
                assert_eq!(value.to_string().parse::<Rational>().unwrap(), value);
                assert!(value.denominator().is_positive());
            }
        }
    }

    #[test]
    fn exact_arithmetic_obeys_ring_identities_on_small_ratios() {
        let values = (-5..=5)
            .flat_map(|numerator| {
                (1..=5).map(move |denominator| Rational::new(numerator, denominator).unwrap())
            })
            .collect::<Vec<_>>();
        for left in &values {
            for right in &values {
                assert_eq!((left + right) - right, left.clone());
                assert_eq!(left * right, right * left);
                if !right.is_zero() {
                    assert_eq!(left.checked_div(right).unwrap() * right, left.clone());
                }
            }
        }
    }
}
