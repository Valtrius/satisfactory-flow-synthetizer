use super::DisplayRate;
use num::{Integer, Signed, ToPrimitive, Zero};
use solver_api::Rational;

pub(super) fn format_rate(value: &Rational) -> DisplayRate {
    DisplayRate {
        exact: value.to_string(),
        decimal: decimal_string(value, 6),
    }
}

fn decimal_string(value: &Rational, precision: usize) -> String {
    let negative = value.is_negative();
    let numerator = value.numerator().abs();
    let denominator = value.denominator();
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
