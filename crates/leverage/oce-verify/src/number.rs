//! Numbers as a vector set writes them, and their one canonical spelling (vectors note,
//! "Canonical form"; decision 16).
//!
//! A loaded set remembers whether each number was an integer token (no fraction, no exponent) or
//! a real, because the two are typed differently when the set is bound to a sequence: an Integer
//! port takes an integer token only. The canonical form keeps that kind:
//!
//! - an integer token is written as a decimal integer, and `-0` (the integer zero) as `0`;
//! - a real is written with the shortest digits that read back to the same binary64, in plain
//!   decimal with at least one fractional digit when it is zero or its magnitude is at least
//!   `1e-4` and below `1e16`, and otherwise as `d[.ddd]e[-]N` with a lowercase `e`, no `+` and no
//!   leading zero in the exponent. `-0.0` is written `-0.0`.
//!
//! "Shortest" alone does not pick one string: `3e-324` through `7e-324` all read back to the
//! smallest subnormal. Of the shortest strings that read back, the one written is the one closest
//! to the exact binary value (`5e-324`: the value is 4.94…e-324).
//!
//! The layout is decided here, from the decimal exponent of those digits, so no formatter's
//! choice of threshold decides a hashed byte; the digits themselves are the standard library's
//! shortest round-trip digits (`{:e}`). The tests pin both halves: goldens at both thresholds and
//! at the subnormals where several strings read back, a comparison with Rust's own `{:?}`
//! spelling, which follows the same layout rule, and an exact-integer check that the digits are
//! the shortest that read back and the closest of those.

/// A JSON number, with the kind it was written as.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Number {
    /// An integer token: no fraction and no exponent.
    Integer(i64),
    /// Any other number token, read with correct rounding. Always finite in a loaded set.
    Real(f64),
}

impl Number {
    /// The number as a time in seconds. An integer token within
    /// [`crate::MAX_TIME_INTEGER`] converts exactly; negative zero reads as zero, so a time never
    /// carries a sign of its own.
    pub(crate) fn seconds(self) -> f64 {
        let value = match self {
            #[expect(
                clippy::cast_precision_loss,
                reason = "the reader bounds a time's integer token to 2^53, where the cast is exact"
            )]
            Self::Integer(value) => value as f64,
            Self::Real(value) => value,
        };
        // IEEE 754: -0.0 + 0.0 is +0.0, and every other value is unchanged.
        value + 0.0
    }

    /// The canonical spelling (see the module documentation).
    pub(crate) fn canonical(self) -> String {
        match self {
            Self::Integer(value) => value.to_string(),
            Self::Real(value) => canonical_real(value),
        }
    }
}

impl PartialEq for Number {
    /// Equal when the kind matches and, for a real, the bits do: `0.0` is not `-0.0`.
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Integer(a), Self::Integer(b)) => a == b,
            (Self::Real(a), Self::Real(b)) => a.to_bits() == b.to_bits(),
            _ => false,
        }
    }
}

impl Eq for Number {}

/// A value as a vector set writes it: an input constant, a step value, an `equals` literal or a
/// parameter value.
///
/// The lexical kind is kept and nothing is typed here: whether `1` may stand for a Real, or `1.0`
/// for an Integer, is decided when the set is bound to a sequence's ports and parameters.
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub enum Literal {
    /// `true` or `false`.
    Boolean(bool),
    /// An integer token: a number with no fraction and no exponent. `-0` reads as `0`.
    Integer(i64),
    /// Any other number token, read with correct rounding; finite, and `-0.0` keeps its sign.
    Real(f64),
}

impl PartialEq for Literal {
    /// Equal when the kind matches and, for a real, the bits do: `0.0` is not `-0.0`, and `1` is
    /// not `1.0`.
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Boolean(a), Self::Boolean(b)) => a == b,
            (Self::Integer(a), Self::Integer(b)) => a == b,
            (Self::Real(a), Self::Real(b)) => a.to_bits() == b.to_bits(),
            _ => false,
        }
    }
}

impl Eq for Literal {}

impl Literal {
    /// The canonical spelling: `true`, `false`, or the number's (see the module documentation).
    pub(crate) fn canonical(self) -> String {
        match self {
            Self::Boolean(value) => if value { "true" } else { "false" }.to_owned(),
            Self::Integer(value) => Number::Integer(value).canonical(),
            Self::Real(value) => Number::Real(value).canonical(),
        }
    }
}

impl From<Number> for Literal {
    fn from(number: Number) -> Self {
        match number {
            Number::Integer(value) => Self::Integer(value),
            Number::Real(value) => Self::Real(value),
        }
    }
}

/// The smallest decimal exponent written in plain decimal: `1e-4` is `0.0001`.
const DECIMAL_FROM_EXPONENT: i32 = -4;

/// The largest decimal exponent written in plain decimal: below `1e16`.
const DECIMAL_TO_EXPONENT: i32 = 15;

/// The canonical spelling of a real (see the module documentation). A non-finite value has no
/// JSON spelling and is written `null`; no loaded set holds one.
pub(crate) fn canonical_real(value: f64) -> String {
    if !value.is_finite() {
        return "null".to_owned();
    }
    if value == 0.0 {
        return if value.is_sign_negative() {
            "-0.0"
        } else {
            "0.0"
        }
        .to_owned();
    }
    let (digits, exponent) = shortest_digits(value.abs());
    let mut out = String::with_capacity(digits.len() + 8);
    if value < 0.0 {
        out.push('-');
    }
    if (DECIMAL_FROM_EXPONENT..=DECIMAL_TO_EXPONENT).contains(&exponent) {
        if exponent >= 0 {
            let whole = usize::try_from(exponent).unwrap_or(0) + 1;
            if digits.len() <= whole {
                out.push_str(&digits);
                out.extend(std::iter::repeat_n('0', whole - digits.len()));
                out.push_str(".0");
            } else {
                out.push_str(&digits[..whole]);
                out.push('.');
                out.push_str(&digits[whole..]);
            }
        } else {
            out.push_str("0.");
            let zeros = usize::try_from(-exponent - 1).unwrap_or(0);
            out.extend(std::iter::repeat_n('0', zeros));
            out.push_str(&digits);
        }
    } else {
        out.push_str(&digits[..1]);
        if digits.len() > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        out.push('e');
        out.push_str(&exponent.to_string());
    }
    out
}

/// The shortest decimal digits that read back to `magnitude` (finite, positive), the closest to
/// its exact value where several strings of that length read back, and the decimal exponent of
/// the first digit: `magnitude` is `d.ddd × 10^exponent`.
fn shortest_digits(magnitude: f64) -> (String, i32) {
    // `{:e}` writes the shortest round-trip digits as `d[.ddd]e[-]N`.
    let scientific = format!("{magnitude:e}");
    let (mantissa, exponent) = scientific
        .split_once('e')
        .unwrap_or((scientific.as_str(), "0"));
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let digits = if digits.is_empty() {
        "0".to_owned()
    } else {
        digits
    };
    (digits, exponent.parse().unwrap_or(0))
}

#[cfg(test)]
#[path = "number_tests.rs"]
mod tests;
