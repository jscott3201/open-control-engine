//! Unit tests for the scalar CDL binding evaluator. Real results are compared **bit-exactly**
//! (`Value::bit_eq`) — these are ground folds, not measured signals. The R10.x cases are the
//! ones cross-checked against the Buildings/OpenModelica oracle in `oce-conformance`.

use std::sync::Arc;

use oce_model::{EnumClassId, Value, enum_class_id, enum_member_ordinal};

use super::{EvalResult, ExprAst, ExprError, Scope, eval, eval_str, parse};

/// A tiny linear-scan scope for tests.
struct TestScope {
    vars: Vec<(String, EvalResult)>,
}

impl TestScope {
    fn new(pairs: &[(&str, Value)]) -> Self {
        Self {
            vars: pairs
                .iter()
                .map(|(k, v)| ((*k).to_string(), EvalResult::Scalar(v.clone())))
                .collect(),
        }
    }
}

impl Scope for TestScope {
    fn lookup(&self, name: &str) -> Option<&EvalResult> {
        self.vars.iter().find(|(k, _)| k == name).map(|(_, v)| v)
    }

    fn enum_class(&self, qualified: &str) -> Option<EnumClassId> {
        enum_class_id(qualified)
    }

    fn enum_ordinal(&self, class: EnumClassId, literal: &str) -> Option<u32> {
        enum_member_ordinal(class, literal)
    }
}

fn scalar(r: EvalResult) -> Value {
    match r {
        EvalResult::Scalar(v) => v,
    }
}

/// Evaluate with an empty scope.
fn run(s: &str) -> Result<Value, ExprError> {
    eval_str(s, &TestScope::new(&[])).map(scalar)
}

/// Evaluate with a scope, panicking the test on error.
fn run_in(s: &str, pairs: &[(&str, Value)]) -> Value {
    scalar(eval_str(s, &TestScope::new(pairs)).expect("evaluation should succeed"))
}

#[track_caller]
fn assert_real(s: &str, expected: f64) {
    let v = run(s).expect("should evaluate");
    assert!(
        v.bit_eq(&Value::Real(expected)),
        "{s:?} = {v:?}, expected Real({expected})"
    );
}

#[track_caller]
fn assert_real_bits(value: Value, expected_bits: u64) {
    let Value::Real(got) = value else {
        panic!("expected Real output, got {value:?}");
    };
    assert_eq!(got.to_bits(), expected_bits, "got {got:?}");
}

#[track_caller]
fn assert_int(s: &str, expected: i64) {
    let v = run(s).expect("should evaluate");
    assert!(
        v.bit_eq(&Value::Integer(expected)),
        "{s:?} = {v:?}, expected Integer({expected})"
    );
}

#[track_caller]
fn assert_bool(s: &str, expected: bool) {
    let v = run(s).expect("should evaluate");
    assert!(
        v.bit_eq(&Value::Boolean(expected)),
        "{s:?} = {v:?}, expected Boolean({expected})"
    );
}

// --- Literals and the minimal binding folds -----------------------------------------------

#[test]
fn literal_folds() {
    assert_real("2.0", 2.0);
    assert_real("0.5", 0.5);
    assert_int("0", 0);
    assert_int("3", 3);
    assert_bool("true", true);
    assert_bool("false", false);
    assert_int("-1", -1);
    assert_real("1e-37", 1e-37);
    assert_real("1.5e3", 1500.0);
}

#[test]
fn string_literal_is_metadata() {
    let v = run("\"K\"").expect("should evaluate");
    assert!(v.bit_eq(&Value::String(Arc::from("K"))), "got {v:?}");
}

#[test]
fn enum_reference_folds_to_class_and_ordinal() {
    assert!(
        run("Buildings.Controls.OBC.CDL.Types.ZeroTime.NY2017")
            .unwrap()
            .bit_eq(&Value::Enum {
                class: EnumClassId::ZERO_TIME,
                ordinal: 11,
            })
    );
    assert_eq!(
        run("Buildings.Controls.OBC.CDL.Types.ZeroTime.Nope").unwrap_err(),
        ExprError::UnknownIdent("Buildings.Controls.OBC.CDL.Types.ZeroTime.Nope".to_owned())
    );
}

#[test]
fn ident_lookup_and_arithmetic() {
    // pRel - 25 with pRel = 50 → 25 (Integer).
    assert!(run_in("pRel - 25", &[("pRel", Value::Integer(50))]).bit_eq(&Value::Integer(25)));
    // A Real-valued scope identifier promotes the result.
    assert!(run_in("k * 2", &[("k", Value::Real(0.5))]).bit_eq(&Value::Real(1.0)));
}

// --- Numeric promotion (§7.1) -------------------------------------------------------------

#[test]
fn promotion_rules() {
    assert_int("3 + 2", 5); // Integer + Integer → Integer
    assert_real("3 / 2", 1.5); // / always Real (not 1)
    assert_real("1.0 + 2", 3.0); // any Real operand promotes
    assert_int("2 - 5", -3);
    assert_int("4 * 3", 12);
    assert_real("4.0 * 3", 12.0);
}

#[test]
fn operator_precedence_and_associativity() {
    assert_int("2 + 3 * 4", 14); // * binds tighter than +
    assert_int("1 - 2 - 3", -4); // additive is left-associative
    assert_int("-2 * 3", -6); // leading sign binds to the whole term: -(2*3)
    assert_int("(2 + 3) * 4", 20); // parens override
    assert_real("10 / 2 / 5", 1.0); // (10/2)/5, all Real
}

// --- Relational and boolean ---------------------------------------------------------------

#[test]
fn relational_and_boolean() {
    assert_bool("2 < 3", true);
    assert_bool("3.0 >= 3", true); // mixed numeric comparison
    assert_bool("2 == 2.0", true); // Integer/Real equality by value
    assert_bool("2 <> 3", true);
    assert_bool("true and false", false);
    assert_bool("true or false", true);
    assert_bool("not true", false);
    assert_bool("not (1 < 2)", false); // not binds a relation
    assert_bool("1 < 2 and 3 < 4", true);
}

// --- Built-ins: the R10.x oracle cases ----------------------------------------------------

#[test]
fn div_vs_integer_rounding() {
    assert_int("div(-7, 2)", -3); // truncate toward zero
    assert_int("integer(-7.0/2.0)", -4); // floor(-3.5)
    assert_int("div(7, 2)", 3);
    assert_real("div(7.5, 2.0)", 3.0); // Real operands → whole-valued Real (R10.3b)
}

#[test]
fn mod_and_rem_sign_conventions() {
    assert_int("mod(-7, 3)", 2); // sign of divisor
    assert_int("rem(-7, 3)", -1); // sign of dividend
    assert_int("mod(7, -3)", -2); // negative divisor
    assert_int("rem(7, -3)", 1);
    assert_real("mod(-7.0, 3.0)", 2.0);
    assert_real("rem(-7.0, 3.0)", -1.0);
    // div(x,y)*y + rem(x,y) == x must hold.
    assert_int("div(-7,3)*3 + rem(-7,3)", -7);
}

#[test]
fn ceil_floor_integer_types() {
    assert_real("floor(2.9)", 2.0);
    assert_real("ceil(2.1)", 3.0);
    assert_real("floor(-2.1)", -3.0);
    assert_int("integer(2.9)", 2);
    assert_real("ceil(2.0)", 2.0);
}

#[test]
fn sign_is_always_integer_and_zero_is_zero() {
    assert_int("sign(-3.0)", -1);
    assert_int("sign(2.5)", 1); // Real arg, Integer result
    assert_int("sign(0.0)", 0); // NOT 1.0 (f64::signum(0.0) == 1.0 trap)
    assert_int("sign(0)", 0);
    assert_int("sign(-4)", -1);
}

#[test]
fn abs_is_type_preserving() {
    assert_int("abs(-3)", 3);
    assert_real("abs(-2.5)", 2.5);
    assert_int("abs(4)", 4);
}

#[test]
fn min_max_scalar_promotion() {
    assert_int("min(2, 3)", 2);
    assert_int("max(2, 3)", 3);
    assert_real("min(2, 3.0)", 2.0); // mixed → Real
    assert_real("max(2.0, 3)", 3.0);
}

#[test]
fn scalar_min_max_pin_nan_and_signed_zero_policy() {
    let nan = Value::Real(f64::from_bits(0xfff8_0000_0000_0000));
    let pos = Value::Real(2.0);

    assert_real_bits(
        run_in("min(a, b)", &[("a", nan.clone()), ("b", pos.clone())]),
        2.0f64.to_bits(),
    );
    assert_real_bits(
        run_in("min(a, b)", &[("a", pos.clone()), ("b", nan.clone())]),
        2.0f64.to_bits(),
    );
    assert_real_bits(
        run_in("max(a, b)", &[("a", nan.clone()), ("b", pos.clone())]),
        2.0f64.to_bits(),
    );
    assert_real_bits(
        run_in("max(a, b)", &[("a", pos.clone()), ("b", nan.clone())]),
        2.0f64.to_bits(),
    );
    assert_real_bits(
        run_in(
            "min(a, b)",
            &[("a", nan.clone()), ("b", Value::Real(f64::NAN))],
        ),
        0x7ff8000000000000,
    );
    assert_real_bits(
        run_in("max(a, b)", &[("a", nan), ("b", Value::Real(f64::NAN))]),
        0x7ff8000000000000,
    );

    assert_real_bits(
        run_in(
            "min(a, b)",
            &[("a", Value::Real(-0.0)), ("b", Value::Real(0.0))],
        ),
        (-0.0f64).to_bits(),
    );
    assert_real_bits(
        run_in(
            "min(a, b)",
            &[("a", Value::Real(0.0)), ("b", Value::Real(-0.0))],
        ),
        (-0.0f64).to_bits(),
    );
    assert_real_bits(
        run_in(
            "max(a, b)",
            &[("a", Value::Real(-0.0)), ("b", Value::Real(0.0))],
        ),
        0.0f64.to_bits(),
    );
    assert_real_bits(
        run_in(
            "max(a, b)",
            &[("a", Value::Real(0.0)), ("b", Value::Real(-0.0))],
        ),
        0.0f64.to_bits(),
    );
}

#[test]
fn scalar_real_outputs_canonicalize_generated_and_propagated_nan_bits() {
    let pairs = [
        (
            "a + b",
            Value::Real(f64::INFINITY),
            Value::Real(f64::NEG_INFINITY),
        ),
        (
            "a - b",
            Value::Real(f64::INFINITY),
            Value::Real(f64::INFINITY),
        ),
        ("a * b", Value::Real(0.0), Value::Real(f64::INFINITY)),
        ("a / b", Value::Real(0.0), Value::Real(0.0)),
        (
            "a / b",
            Value::Real(f64::INFINITY),
            Value::Real(f64::INFINITY),
        ),
    ];
    for (expr, a, b) in pairs {
        assert_real_bits(run_in(expr, &[("a", a), ("b", b)]), 0x7ff8000000000000);
    }

    let negative_nan = Value::Real(f64::from_bits(0xfff8_0000_0000_0000));
    assert_real_bits(
        run_in("a", &[("a", negative_nan.clone())]),
        0x7ff8000000000000,
    );
    assert_real_bits(run_in("abs(a)", &[("a", negative_nan)]), 0x7ff8000000000000);
}

#[test]
fn sqrt_domain() {
    assert_real("sqrt(4.0)", 2.0);
    assert_real("sqrt(9)", 3.0);
    // `Value` has no `PartialEq` (it uses bit_eq), so compare the error directly.
    assert_eq!(
        run("sqrt(-1.0)").unwrap_err(),
        ExprError::DomainError("sqrt of a negative value")
    );
}

#[test]
fn division_by_zero_in_builtins() {
    assert_eq!(run("div(1, 0)").unwrap_err(), ExprError::DivisionByZero);
    assert_eq!(run("mod(1, 0)").unwrap_err(), ExprError::DivisionByZero);
    assert_eq!(run("rem(1, 0)").unwrap_err(), ExprError::DivisionByZero);
}

// --- Named constants ----------------------------------------------------------------------

#[test]
fn named_constants() {
    assert_real("pi", std::f64::consts::PI);
    assert_real("Modelica.Constants.pi", std::f64::consts::PI);
    assert_real("CDL.Constants.eps", 1e-15);
    assert_real("small", 1e-37);
    assert_real("CDL.Constants.small", 1e-37);
    assert_real("Buildings.Controls.OBC.CDL.Constants.small", 1e-37);
    assert_real("inf", f64::MAX);
    assert_real("CDL.Constants.inf", f64::MAX);
    assert_real("-inf", -f64::MAX);
    // A qualified name whose final package is not `Constants` is an enum reference candidate.
    assert_eq!(
        run("Foo.pi").unwrap_err(),
        ExprError::UnknownIdent("Foo".to_string())
    );
}

// --- Closed-world rejection (R9) ----------------------------------------------------------

#[test]
fn unsupported_functions_are_rejected() {
    assert_eq!(
        run("foo(1)").unwrap_err(),
        ExprError::UnsupportedFunction("foo".to_string())
    );
    assert_eq!(
        run("sin(1)").unwrap_err(),
        ExprError::UnsupportedFunction("sin".to_string())
    );
    assert_eq!(
        run("Modelica.Math.asin(1)").unwrap_err(),
        ExprError::UnsupportedFunction("Modelica.Math.asin".to_string())
    );
}

#[test]
fn unknown_identifier_is_rejected() {
    assert_eq!(
        run("missing").unwrap_err(),
        ExprError::UnknownIdent("missing".to_string())
    );
}

#[test]
fn type_errors_have_no_implicit_coercion() {
    assert!(matches!(run("true + 1"), Err(ExprError::TypeError { .. })));
    assert!(matches!(run("1 and 2"), Err(ExprError::TypeError { .. })));
    assert!(matches!(run("not 1"), Err(ExprError::TypeError { .. })));
    assert!(matches!(
        run("\"a\" == 1"),
        Err(ExprError::TypeError { .. })
    ));
}

// --- Deferred array constructs report a typed parse error (never a panic) ------------------

#[test]
fn array_constructs_are_deferred_not_panicking() {
    assert!(matches!(run("{1, 2}"), Err(ExprError::Parse(_))));
    assert!(matches!(run("1:3"), Err(ExprError::Parse(_))));
    assert!(matches!(run("sum(x)"), Err(ExprError::Parse(_))));
    assert!(matches!(run("min(a)"), Err(ExprError::Parse(_)))); // 1-arg array form
}

// --- Malformed input is a typed parse error -----------------------------------------------

#[test]
fn malformed_input_is_a_parse_error() {
    assert!(matches!(run(""), Err(ExprError::Parse(_))));
    assert!(matches!(run("   "), Err(ExprError::Parse(_))));
    assert!(matches!(run("1 2"), Err(ExprError::Parse(_)))); // trailing input
    assert!(matches!(run("a = 1"), Err(ExprError::Parse(_)))); // bare '='
    assert!(matches!(run("1e"), Err(ExprError::Parse(_)))); // empty exponent
    assert!(matches!(run("\"unterminated"), Err(ExprError::Parse(_))));
    assert!(matches!(run("abs(1, 2)"), Err(ExprError::Parse(_)))); // arity
    assert!(matches!(run("(1 + 2"), Err(ExprError::Parse(_)))); // unbalanced paren
    assert!(matches!(run("+"), Err(ExprError::Parse(_)))); // operator, no operand
}

#[test]
fn string_equality() {
    assert_bool("\"K\" == \"K\"", true);
    assert_bool("\"K\" == \"degC\"", false);
    assert_bool("\"K\" <> \"degC\"", true);
}

// --- Regression tests for arithmetic and parser edge cases ---------------------------------

#[test]
fn large_integer_comparison_is_exact() {
    // Above 2^53 these collapse to equal if compared via f64; Integer comparison must be exact.
    assert_bool("9007199254740993 == 9007199254740992", false);
    assert_bool("9007199254740993 > 9007199254740992", true);
    assert_bool("9223372036854775807 == 9223372036854775806", false); // i64::MAX vs MAX-1
    assert_bool("9223372036854775807 > 9223372036854775806", true);
    assert_bool("9007199254740993 <> 9007199254740992", true);
}

#[test]
fn unary_minus_after_a_binary_operator_parses() {
    assert_int("2 * -3", -6);
    assert_int("3 - -2", 5);
    assert_int("4 + -1", 3);
    assert_real("6.0 / -2.0", -3.0);
    assert_int("--3", 3); // double negation
    assert_int("-2 * 3", -6); // leading sign still binds: (-2)*3 == -(2*3)
}

#[test]
fn integer_overflow_is_a_domain_error_not_a_panic() {
    let max = &[("n", Value::Integer(i64::MAX))];
    let min = &[("n", Value::Integer(i64::MIN))];
    let dm = &[("n", Value::Integer(i64::MIN)), ("m", Value::Integer(-1))];
    assert!(matches!(
        eval_str("n + 1", &TestScope::new(max)).unwrap_err(),
        ExprError::DomainError(_)
    ));
    assert!(matches!(
        eval_str("-n", &TestScope::new(min)).unwrap_err(),
        ExprError::DomainError(_)
    ));
    assert!(matches!(
        eval_str("abs(n)", &TestScope::new(min)).unwrap_err(),
        ExprError::DomainError(_)
    ));
    assert!(matches!(
        eval_str("div(n, m)", &TestScope::new(dm)).unwrap_err(),
        ExprError::DomainError(_)
    ));
}

#[test]
fn mod_on_extreme_operands_does_not_spuriously_overflow() {
    // mod(i64::MIN, 3) fits (== 1); the i128 intermediate avoids a spurious overflow error.
    let expected_mod = (i128::from(i64::MIN).rem_euclid(3)) as i64; // 1 (rem_euclid == mod for +y)
    let m = run_in("mod(n, 3)", &[("n", Value::Integer(i64::MIN))]);
    assert!(m.bit_eq(&Value::Integer(expected_mod)), "got {m:?}");
    // rem keeps the dividend's sign (== Rust %).
    let r = run_in("rem(n, 3)", &[("n", Value::Integer(i64::MIN))]);
    assert!(r.bit_eq(&Value::Integer(i64::MIN % 3)), "got {r:?}");
}

#[test]
fn integer_cast_saturates_without_panic() {
    let hi = run_in("integer(x)", &[("x", Value::Real(1e300))]);
    assert!(hi.bit_eq(&Value::Integer(i64::MAX)), "got {hi:?}");
    let lo = run_in("integer(x)", &[("x", Value::Real(-1e300))]);
    assert!(lo.bit_eq(&Value::Integer(i64::MIN)), "got {lo:?}");
}

#[test]
fn real_div_rem_identity() {
    // div(x,y)*y + rem(x,y) == x must hold for Real operands too (R10.3b).
    assert_real("div(-7.5, 2.0)*2.0 + rem(-7.5, 2.0)", -7.5);
    assert_real("div(-7.5, 2.0)", -3.0);
    assert_real("rem(-7.5, 2.0)", -1.5);
}

// --- Determinism and contract stability ----------------------------------------------------

#[test]
fn evaluation_is_deterministic() {
    let scope = TestScope::new(&[("k", Value::Real(0.5))]);
    let a = scalar(eval_str("k * 2 + sqrt(9.0)", &scope).unwrap());
    let b = scalar(eval_str("k * 2 + sqrt(9.0)", &scope).unwrap());
    assert!(a.bit_eq(&b));
    assert!(a.bit_eq(&Value::Real(4.0)));
}

#[test]
fn public_signatures_are_stable() {
    // The contract `oce-flatten` binds to: these coercions fail to compile if a signature drifts.
    let _p: fn(&str) -> Result<ExprAst, ExprError> = parse;
    let _e: fn(&ExprAst, &dyn Scope) -> Result<EvalResult, ExprError> = eval;
    let _es: fn(&str, &dyn Scope) -> Result<EvalResult, ExprError> = eval_str;
}
