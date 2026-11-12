use oce_api::ValueType;

use super::{Untyped, typed};
use crate::number::Literal;
use crate::receipt::ReceiptValue;

/// Why a value was refused, or `None` when it was typed.
fn refusal(value: Result<ReceiptValue, Untyped>) -> Option<Untyped> {
    value.err()
}

fn real_bits(value: Result<ReceiptValue, Untyped>) -> u64 {
    match value {
        Ok(ReceiptValue::Real(x)) => x.to_bits(),
        other => panic!("not a Real: {other:?}"),
    }
}

#[test]
fn a_real_takes_a_real_token_with_its_bits_and_its_sign() {
    for x in [0.0, -0.0, 1.5, -1e308, 5e-324, f64::MIN_POSITIVE] {
        assert_eq!(
            real_bits(typed(Literal::Real(x), ValueType::Real)),
            x.to_bits()
        );
    }
}

#[test]
fn the_integer_token_minus_zero_lands_on_a_real_as_positive_zero() {
    // `-0` is read as the integer zero; on a Real it is `+0.0`, never `-0.0`.
    assert_eq!(
        real_bits(typed(Literal::Integer(0), ValueType::Real)),
        0.0_f64.to_bits()
    );
    assert_ne!(
        real_bits(typed(Literal::Integer(0), ValueType::Real)),
        (-0.0_f64).to_bits()
    );
}

#[test]
fn an_integer_token_lands_on_a_real_only_when_a_binary64_holds_it_exactly() {
    let two_53: i64 = 1 << 53;
    for exact in [
        1,
        -1,
        two_53,
        -two_53,
        two_53 + 2,
        i64::MIN,
        i64::MAX - 1023,
    ] {
        #[expect(
            clippy::cast_precision_loss,
            reason = "the oracle of an exact conversion"
        )]
        let expected = exact as f64;
        assert_eq!(
            real_bits(typed(Literal::Integer(exact), ValueType::Real)),
            expected.to_bits(),
            "{exact}"
        );
    }
    for inexact in [two_53 + 1, -two_53 - 1, two_53 + 3, i64::MAX, i64::MAX - 1] {
        assert_eq!(
            refusal(typed(Literal::Integer(inexact), ValueType::Real)),
            Some(Untyped::NotRepresentable),
            "{inexact}"
        );
    }
}

#[test]
fn an_integer_takes_an_integer_token_only() {
    for value in [0, -1, i64::MIN, i64::MAX] {
        assert!(matches!(
            typed(Literal::Integer(value), ValueType::Integer),
            Ok(ReceiptValue::Integer(v)) if v == value
        ));
    }
    // `1.0` is a real, never coerced to 1; nor is `-0.0` or a boolean.
    for refused in [
        Literal::Real(1.0),
        Literal::Real(-0.0),
        Literal::Boolean(true),
    ] {
        assert_eq!(
            refusal(typed(refused, ValueType::Integer)),
            Some(Untyped::Mismatch),
            "{refused:?}"
        );
    }
}

#[test]
fn a_boolean_takes_true_or_false_and_never_a_number() {
    for value in [true, false] {
        assert!(matches!(
            typed(Literal::Boolean(value), ValueType::Boolean),
            Ok(ReceiptValue::Boolean(v)) if v == value
        ));
    }
    for refused in [Literal::Integer(0), Literal::Integer(1), Literal::Real(1.0)] {
        assert_eq!(
            refusal(typed(refused, ValueType::Boolean)),
            Some(Untyped::Mismatch),
            "{refused:?}"
        );
    }
    assert_eq!(
        refusal(typed(Literal::Boolean(true), ValueType::Real)),
        Some(Untyped::Mismatch)
    );
}

#[test]
fn a_string_or_an_enum_takes_nothing_a_vector_can_write() {
    let enumeration = oce_api::enumeration("CDL.Types.SimpleController")
        .and_then(|class| class.id)
        .expect("the facade knows the CDL enumeration");
    for value_type in [ValueType::String, ValueType::Enum(enumeration)] {
        for literal in [
            Literal::Boolean(true),
            Literal::Integer(1),
            Literal::Real(1.0),
        ] {
            assert_eq!(
                refusal(typed(literal, value_type)),
                Some(Untyped::Mismatch),
                "{literal:?} on {value_type:?}"
            );
        }
    }
}
