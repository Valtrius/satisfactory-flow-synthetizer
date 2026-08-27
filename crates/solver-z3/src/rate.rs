use num::{BigInt, BigRational, Integer, One, Signed, ToPrimitive, Zero};

use crate::model::DisplayRate;

/// Parse an integer, decimal, or fraction without losing precision.
///
/// # Errors
///
/// Returns a short validation message when the value is empty, too long, or not a valid number.
pub fn parse_rate(value: &str) -> Result<BigRational, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("rate is required".to_owned());
    }
    if value.len() > 128 {
        return Err("rate is too long".to_owned());
    }

    if let Some((numerator, denominator)) = value.split_once('/') {
        if denominator.contains('/') {
            return Err("fraction must contain one slash".to_owned());
        }
        let numerator = parse_integer(numerator.trim())?;
        let denominator = parse_integer(denominator.trim())?;
        if denominator.is_zero() {
            return Err("fraction denominator cannot be zero".to_owned());
        }
        return Ok(BigRational::new(numerator, denominator));
    }

    parse_decimal(value)
}

fn parse_integer(value: &str) -> Result<BigInt, String> {
    if value.is_empty() {
        return Err("fraction is missing a number".to_owned());
    }
    value
        .parse::<BigInt>()
        .map_err(|_| format!("'{value}' is not an integer"))
}

fn parse_decimal(value: &str) -> Result<BigRational, String> {
    let (negative, unsigned) = match value.as_bytes().first() {
        Some(b'-') => (true, &value[1..]),
        Some(b'+') => (false, &value[1..]),
        _ => (false, value),
    };
    if unsigned.is_empty() {
        return Err("rate is missing digits".to_owned());
    }

    let mut parts = unsigned.split('.');
    let whole = parts.next().unwrap_or_default();
    let fraction = parts.next();
    if parts.next().is_some() {
        return Err("rate contains more than one decimal point".to_owned());
    }
    if whole.is_empty() && fraction.is_none_or(str::is_empty) {
        return Err("rate is missing digits".to_owned());
    }
    if !whole.chars().all(|character| character.is_ascii_digit())
        || fraction
            .is_some_and(|digits| !digits.chars().all(|character| character.is_ascii_digit()))
    {
        return Err(format!("'{value}' is not a decimal or fraction"));
    }

    let fraction = fraction.unwrap_or_default();
    let digits = format!("{}{}", if whole.is_empty() { "0" } else { whole }, fraction);
    let mut numerator = digits
        .parse::<BigInt>()
        .map_err(|_| format!("'{value}' is not a decimal"))?;
    if negative {
        numerator = -numerator;
    }
    let exponent = u32::try_from(fraction.len()).map_err(|_| "rate is too long".to_owned())?;
    let denominator = BigInt::from(10_u8).pow(exponent);
    Ok(BigRational::new(numerator, denominator))
}

#[must_use]
pub fn format_rate(value: &BigRational) -> DisplayRate {
    DisplayRate {
        exact: exact_string(value),
        decimal: decimal_string(value, 6),
    }
}

fn exact_string(value: &BigRational) -> String {
    if value.denom().is_one() {
        value.numer().to_string()
    } else {
        format!("{}/{}", value.numer(), value.denom())
    }
}

fn decimal_string(value: &BigRational, precision: usize) -> String {
    let negative = value.is_negative();
    let numerator = value.numer().abs();
    let denominator = value.denom();
    let (whole, mut remainder) = numerator.div_rem(denominator);
    if remainder.is_zero() {
        return format!("{}{}", if negative { "-" } else { "" }, whole);
    }

    let mut digits = String::with_capacity(precision);
    for _ in 0..precision {
        remainder *= 10_u8;
        let (digit, next) = remainder.div_rem(denominator);
        digits.push(char::from(b'0' + digit.to_u8().unwrap_or(0)));
        remainder = next;
        if remainder.is_zero() {
            break;
        }
    }
    while digits.ends_with('0') {
        digits.pop();
    }
    format!("{}{}.{}", if negative { "-" } else { "" }, whole, digits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_decimals_and_fractions_exactly() {
        assert_eq!(
            parse_rate("12.5").unwrap(),
            BigRational::new(25.into(), 2.into())
        );
        assert_eq!(
            parse_rate("1/7").unwrap(),
            BigRational::new(1.into(), 7.into())
        );
        assert_eq!(
            parse_rate(".25").unwrap(),
            BigRational::new(1.into(), 4.into())
        );
    }

    #[test]
    fn rejects_bad_rates() {
        assert!(parse_rate("1/0").is_err());
        assert!(parse_rate("1.2.3").is_err());
        assert!(parse_rate("").is_err());
    }

    #[test]
    fn formats_rates_for_people_and_machines() {
        let rate = BigRational::new(1.into(), 3.into());
        let formatted = format_rate(&rate);
        assert_eq!(formatted.exact, "1/3");
        assert_eq!(formatted.decimal, "0.333333");
    }
}
