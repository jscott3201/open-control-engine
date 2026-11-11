//! Values typed by what they land on (vectors note, "Binding and running" step 3; decisions 7 and
//! 8).
//!
//! A set keeps each number's lexical kind and types nothing ([`crate::Literal`]); the binder
//! types every value by the declared type of the port or parameter it lands on, never by its
//! JSON token:
//!
//! - a **Real** takes any number. A real token keeps its bits (`-0.0` keeps its sign). An integer
//!   token is taken when it converts to binary64 exactly, so `-0` is the integer zero and lands
//!   as `+0.0`, and an integer past 2^53 that no binary64 holds is `value-not-representable`;
//! - an **Integer** takes an integer token only: `1.0` is a real and is refused, never coerced;
//! - a **Boolean** takes `true` or `false`, never `0` or `1`;
//! - a **String** or an **Enum** takes nothing a vector can write.
//!
//! Every other pairing is `value-type-mismatch`.

use oce_api::ValueType;

use crate::number::Literal;
use crate::receipt::ReceiptValue;

/// Why a value could not be typed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Untyped {
    /// The value's JSON kind is not one the declared type takes.
    Mismatch,
    /// An integer token on a Real that no binary64 holds exactly.
    NotRepresentable,
}

/// `literal` as a value of `value_type`, by the rules in the module documentation. The result is
/// a Real, an Integer or a Boolean: nothing a vector writes types as a String or an Enum.
pub(crate) fn typed(literal: Literal, value_type: ValueType) -> Result<ReceiptValue, Untyped> {
    match value_type {
        ValueType::Real => match literal {
            Literal::Real(value) => Ok(ReceiptValue::Real(value)),
            Literal::Integer(value) => exact_real(value)
                .map(ReceiptValue::Real)
                .ok_or(Untyped::NotRepresentable),
            Literal::Boolean(_) => Err(Untyped::Mismatch),
        },
        ValueType::Integer => match literal {
            Literal::Integer(value) => Ok(ReceiptValue::Integer(value)),
            Literal::Real(_) | Literal::Boolean(_) => Err(Untyped::Mismatch),
        },
        ValueType::Boolean => match literal {
            Literal::Boolean(value) => Ok(ReceiptValue::Boolean(value)),
            Literal::Real(_) | Literal::Integer(_) => Err(Untyped::Mismatch),
        },
        ValueType::String | ValueType::Enum(_) => Err(Untyped::Mismatch),
    }
}

/// The binary64 equal to `value`, when one is: every integer within ±2^53, and beyond it only
/// those the format holds exactly (2^53 + 2, 2^63 - 1024 and so on).
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "the conversion is checked: the binary64 is kept only when it reads back as the same integer"
)]
fn exact_real(value: i64) -> Option<f64> {
    let real = value as f64;
    // An `f64` of magnitude at most 2^63 converts to `i128` exactly, so the comparison is exact;
    // `i64::MAX as f64` is 2^63, which is not `i64::MAX`.
    (real as i128 == i128::from(value)).then_some(real)
}

/// A short name for a declared type, for refusal sentences.
pub(crate) fn type_name(value_type: ValueType) -> &'static str {
    match value_type {
        ValueType::Real => "a Real",
        ValueType::Integer => "an Integer",
        ValueType::Boolean => "a Boolean",
        ValueType::String => "a String",
        ValueType::Enum(_) => "an Enum",
    }
}

/// A short name for a literal's kind, for refusal sentences.
pub(crate) fn literal_name(literal: Literal) -> &'static str {
    match literal {
        Literal::Boolean(_) => "a boolean",
        Literal::Integer(_) => "an integer token",
        Literal::Real(_) => "a real",
    }
}

#[cfg(test)]
#[path = "typing_tests.rs"]
mod tests;
