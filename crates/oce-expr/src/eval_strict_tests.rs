//! Strict-finite evaluation ([`crate::eval_str_finite`], issue #362): every node that produces a
//! non-finite Real refuses, no built-in can absorb one, finite extremes (including the
//! `Modelica.Constants.inf` alias, `f64::MAX`) stay legal, and the default [`crate::eval_str`]
//! keeps its IEEE propagation semantics unchanged.

use std::sync::Arc;

use oce_model::Value;

use crate::{EvalResult, ExprError, Scope, eval_str, eval_str_finite};

struct Pairs(Vec<(Arc<str>, EvalResult)>);

impl Pairs {
    fn new(pairs: &[(&str, Value)]) -> Self {
        Self(
            pairs
                .iter()
                .map(|(name, value)| (Arc::from(*name), EvalResult::Scalar(value.clone())))
                .collect(),
        )
    }
}

impl Scope for Pairs {
    fn lookup(&self, name: &str) -> Option<&EvalResult> {
        self.0
            .iter()
            .find(|(entry, _)| entry.as_ref() == name)
            .map(|(_, value)| value)
    }
}

fn nan_scope() -> Pairs {
    Pairs::new(&[
        ("nanK", Value::Real(f64::NAN)),
        ("infK", Value::Real(f64::INFINITY)),
        ("ok", Value::Real(1.5)),
    ])
}

fn refused(text: &str) -> ExprError {
    eval_str_finite(text, &nan_scope()).expect_err(&format!(
        "`{text}` must refuse under strict-finite evaluation"
    ))
}

#[test]
fn every_absorbing_construct_refuses_its_non_finite_intermediate() {
    for (text, value) in [
        // Producers.
        ("1/0", "inf"),
        ("-1/0", "-inf"),
        ("0/0", "NaN"),
        ("1e400", "inf"),
        // The literal `1e400` overflows before the unary minus applies.
        ("-1e400", "inf"),
        ("1e308*10", "inf"),
        ("inf*2", "inf"),
        ("Modelica.Constants.inf + Modelica.Constants.inf", "inf"),
        ("-inf*2", "-inf"),
        ("nanK", "NaN"),
        ("infK", "inf"),
        // Absorbers: the documented reviewer repros and their relatives.
        ("integer(nanK)", "NaN"),
        ("integer(1/0)", "inf"),
        ("sign(0/0)", "NaN"),
        ("min(1/0, 3.0)", "inf"),
        ("max(nanK, 2.0)", "NaN"),
        ("abs(-1/0)", "-inf"),
        ("floor(1e400)", "inf"),
        ("nanK > 0", "NaN"),
        ("nanK == nanK", "NaN"),
        ("(1/0) - (1/0)", "inf"),
        ("ok * 0 + 1/0 * 0", "inf"),
        // Arrays and reductions: an element, a reduction overflow, a comprehension body.
        ("sum({1e308, 1e308})", "inf"),
        ("{1.0, 1/0}", "inf"),
        ("max({nanK, 1.0})", "NaN"),
        ("{integer(1/i) for i in 0:1}", "inf"),
        ("fill(1/0, 2)", "inf"),
    ] {
        let error = refused(text);
        assert_eq!(error, ExprError::NonFinite { value }, "`{text}`");
    }
}

#[test]
fn finite_extremes_and_the_inf_alias_stay_legal_and_bit_exact() {
    for (text, expected) in [
        ("Modelica.Constants.inf", f64::MAX),
        ("-Modelica.Constants.inf", f64::MIN),
        ("inf", f64::MAX),
        ("1.7976931348623157e308", f64::MAX),
        ("2.2250738585072014e-308", f64::MIN_POSITIVE),
        ("5e-324", f64::from_bits(1)),
        ("-0.0", -0.0),
        ("1e308 * 1", 1e308),
        ("min(inf, 3.0)", 3.0),
        ("ok", 1.5),
    ] {
        let result = eval_str_finite(text, &nan_scope())
            .unwrap_or_else(|error| panic!("`{text}` must stay legal: {error:?}"));
        let EvalResult::Scalar(value) = result else {
            panic!("`{text}` is scalar")
        };
        assert!(value.bit_eq(&Value::Real(expected)), "`{text}`: {value:?}");
    }
    // Non-Real results are untouched by the strictness.
    assert!(matches!(
        eval_str_finite("ok > 1", &nan_scope()),
        Ok(EvalResult::Scalar(Value::Boolean(true)))
    ));
    assert!(matches!(
        eval_str_finite("integer(ok)", &nan_scope()),
        Ok(EvalResult::Scalar(Value::Integer(1)))
    ));
}

#[test]
fn default_evaluation_keeps_ieee_propagation_unchanged() {
    // The strict mode is opt-in: the ordinary entry point still yields the documented values.
    for (text, check) in [
        ("1/0", f64::is_infinite as fn(f64) -> bool),
        ("0/0", f64::is_nan),
        ("1e400", f64::is_infinite),
    ] {
        let Ok(EvalResult::Scalar(Value::Real(v))) = eval_str(text, &nan_scope()) else {
            panic!("`{text}` must evaluate under the default mode")
        };
        assert!(check(v), "`{text}`: {v}");
    }
    // `integer()` of a non-finite argument is outside the Integer range in every mode (#374).
    assert_eq!(
        eval_str("integer(1/0)", &nan_scope()).unwrap_err(),
        ExprError::DomainError("integer() argument is outside the Integer range [-2^63, 2^63)")
    );
    assert!(matches!(
        eval_str("nanK > 0", &nan_scope()),
        Ok(EvalResult::Scalar(Value::Boolean(false)))
    ));
}

#[test]
fn strict_refusal_is_deterministic() {
    for text in ["integer(1/0)", "sum({1e308, 1e308})", "nanK > 0"] {
        assert_eq!(refused(text), refused(text), "`{text}`");
    }
}

#[test]
fn inf_alias_arithmetic_is_ordinary_finite_arithmetic() {
    // Issue #374: the alias is f64::MAX, so these are finite results, not Modelica's IEEE NaN/inf.
    // Both entry points agree, and every spelling of the alias behaves the same.
    for alias in ["inf", "Modelica.Constants.inf"] {
        for (template, expected) in [
            ("A/A", 1.0),
            ("A-A", 0.0),
            ("A+1", f64::MAX),
            ("A-1", f64::MAX),
            ("A*1", f64::MAX),
            ("A*0.5", f64::MAX * 0.5),
            ("A/2", f64::MAX / 2.0),
            ("-A", f64::MIN),
            ("-A + A", 0.0),
            ("A*0", 0.0),
            ("max(A, 1e308)", f64::MAX),
            ("min(-A, -1e308)", f64::MIN),
            ("abs(-A)", f64::MAX),
        ] {
            let text = template.replace('A', alias);
            for (mode, result) in [
                ("strict", eval_str_finite(&text, &nan_scope())),
                ("default", eval_str(&text, &nan_scope())),
            ] {
                let Ok(EvalResult::Scalar(value)) = result else {
                    panic!("`{text}` ({mode}) must evaluate: {result:?}")
                };
                assert!(
                    value.bit_eq(&Value::Real(expected)),
                    "`{text}` ({mode}): {value:?}"
                );
            }
        }
        // Comparison and pass-through behave as "largest Real".
        for (template, expected) in [
            ("A > 1e308", true),
            ("A == A", true),
            ("-A < -1e308", true),
            ("A >= 1.7976931348623157e308", true),
        ] {
            let text = template.replace('A', alias);
            assert!(
                matches!(
                    eval_str_finite(&text, &nan_scope()),
                    Ok(EvalResult::Scalar(Value::Boolean(b))) if b == expected
                ),
                "`{text}`"
            );
        }
        // Overflowing it is refused at load; truncating it to an Integer is refused everywhere.
        for (template, expected) in [
            ("A*2", ExprError::NonFinite { value: "inf" }),
            ("A+A", ExprError::NonFinite { value: "inf" }),
            ("-A-A", ExprError::NonFinite { value: "-inf" }),
            ("A/1e-300", ExprError::NonFinite { value: "inf" }),
            (
                "integer(A)",
                ExprError::DomainError(
                    "integer() argument is outside the Integer range [-2^63, 2^63)",
                ),
            ),
        ] {
            let text = template.replace('A', alias);
            assert_eq!(refused(&text), expected, "`{text}`");
        }
    }
}
