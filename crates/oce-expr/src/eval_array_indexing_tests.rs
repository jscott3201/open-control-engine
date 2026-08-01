//! Unit tests for 1-D array indexing, mirroring `eval_array_indexing.rs`. Real results are
//! compared **bit-exactly** (`to_bits`/`bit_eq`) — an indexed read must return the stored
//! element bits unchanged, so the `-0.0`/NaN pass-through goldens are part of the determinism
//! contract. The bounds and subscript-type suites assert exact error *fields*, not just
//! variants: the reported index/size pair is diagnostic surface the flattener relays.

use oce_model::{EnumClassId, Value, enum_class_id, enum_member_ordinal};

use super::{ArrayValue, EvalResult, ExprError, Scope, eval_str, parse};

/// A tiny linear-scan scope that can hold scalar *and* array bindings.
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

    fn bind_array(mut self, name: &str, a: ArrayValue) -> Self {
        self.vars.push((name.to_string(), EvalResult::Array(a)));
        self
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

/// Evaluate with an empty scope.
fn run(s: &str) -> Result<EvalResult, ExprError> {
    eval_str(s, &TestScope::new(&[]))
}

/// Evaluate with an empty scope, panicking the test unless the result is a scalar.
#[track_caller]
fn scalar(s: &str) -> Value {
    scalar_in(s, &TestScope::new(&[]))
}

/// Evaluate against `scope`, panicking the test unless the result is a scalar.
#[track_caller]
fn scalar_in(s: &str, scope: &TestScope) -> Value {
    match eval_str(s, scope) {
        Ok(EvalResult::Scalar(v)) => v,
        other => panic!("{s:?} should evaluate to a scalar, got {other:?}"),
    }
}

/// Evaluate against `scope`, panicking the test unless the result is an array.
#[track_caller]
fn array_in(s: &str, scope: &TestScope) -> ArrayValue {
    match eval_str(s, scope) {
        Ok(EvalResult::Array(a)) => a,
        other => panic!("{s:?} should evaluate to an array, got {other:?}"),
    }
}

#[track_caller]
fn assert_scalar_real_bits(v: Value, expected_bits: u64) {
    let Value::Real(r) = v else {
        panic!("expected a Real result, got {v:?}");
    };
    assert_eq!(
        r.to_bits(),
        expected_bits,
        "got {r:?} (0x{:016X}), want 0x{expected_bits:016X}",
        r.to_bits()
    );
}

// --- Reads: every element type and every array-producing base ------------------------------

#[test]
fn integer_literal_elements_read_back_one_based() {
    assert!(scalar("{1, 2, 3}[2]").bit_eq(&Value::Integer(2)));
    // Both boundary elements: subscript 1 is the first element, subscript len the last.
    assert!(scalar("{1, 2, 3}[1]").bit_eq(&Value::Integer(1)));
    assert!(scalar("{1, 2, 3}[3]").bit_eq(&Value::Integer(3)));
}

#[test]
fn range_elements_read_back_through_the_closed_form() {
    assert!(scalar("(1:5)[3]").bit_eq(&Value::Integer(3)));
    assert!(scalar("(2:2:8)[4]").bit_eq(&Value::Integer(8)));
    // A descending range keeps its construction order under indexing.
    assert!(scalar("(5:-1:3)[1]").bit_eq(&Value::Integer(5)));
    // The Real range golden: element k = 6 of 0:0.1:1 is 0.6000000000000001 — the same
    // closed-form bits `eval_array_tests` pins for the whole vector.
    assert_scalar_real_bits(scalar("(0:0.1:1)[7]"), 0x3FE3333333333334);
}

#[test]
fn boolean_array_elements_read_back() {
    assert!(scalar("{true, false}[1]").bit_eq(&Value::Boolean(true)));
    assert!(scalar("{true, false}[2]").bit_eq(&Value::Boolean(false)));
    assert!(scalar("fill(false, 3)[3]").bit_eq(&Value::Boolean(false)));
}

#[test]
fn real_element_bits_pass_through_unchanged() {
    // -0.0 is a stored element bit pattern (literals preserve it) and must read back
    // bit-identical — an indexed read never re-derives or normalizes the element.
    assert_scalar_real_bits(scalar("{0.0, -0.0}[2]"), (-0.0f64).to_bits());
    assert_scalar_real_bits(scalar("{-0.0, 0.0}[1]"), (-0.0f64).to_bits());
    // A NaN element was canonicalized when the array was BUILT (the literal's constructor);
    // the read returns those canonical bits as-is.
    let nan = Value::Real(f64::from_bits(0xfff8_0000_0000_0000));
    let scope = TestScope::new(&[("a", nan)]);
    assert_scalar_real_bits(scalar_in("{1.0, a}[2]", &scope), 0x7FF8_0000_0000_0000);
}

#[test]
fn scope_bound_array_identifiers_are_indexable() {
    let nan = Value::Real(f64::from_bits(0xfff8_0000_0000_0000));
    let stored = array_in("{1.5, a, -0.0}", &TestScope::new(&[("a", nan)]));
    let scope = TestScope::new(&[]).bind_array("arr", stored);
    assert_scalar_real_bits(scalar_in("arr[1]", &scope), 1.5f64.to_bits());
    assert_scalar_real_bits(scalar_in("arr[2]", &scope), 0x7FF8_0000_0000_0000);
    assert_scalar_real_bits(scalar_in("arr[3]", &scope), (-0.0f64).to_bits());
}

#[test]
fn built_in_results_are_indexable() {
    // size(A) returns an Integer array, so the shapes compose: size({…})[1] is the length.
    assert!(scalar("size({5, 6, 7})[1]").bit_eq(&Value::Integer(3)));
    assert_scalar_real_bits(scalar("fill(2.5, 3)[2]"), 2.5f64.to_bits());
    assert!(scalar("cat(1, {1}, {2})[2]").bit_eq(&Value::Integer(2)));
}

#[test]
fn subscript_expressions_fold_before_the_read() {
    assert!(scalar("{10, 20, 30}[1 + 1]").bit_eq(&Value::Integer(20)));
    assert!(scalar("{5, 6}[min(1, 2)]").bit_eq(&Value::Integer(5)));
    // `A[size(A, 1)]` is the canonical last-element idiom.
    let scope = TestScope::new(&[]).bind_array("arr", array_in("7:9", &TestScope::new(&[])));
    assert!(scalar_in("arr[size(arr, 1)]", &scope).bit_eq(&Value::Integer(9)));
    // An indexed read is a scalar, so it participates in ordinary arithmetic.
    assert!(scalar("{1, 2, 3}[2] + 10").bit_eq(&Value::Integer(12)));
}

// --- Bounds: 1-based, exact fields, wrap-free at the i64 edges ------------------------------

#[test]
fn subscript_zero_negative_and_past_the_end_are_out_of_bounds() {
    // CDL indexing is 1-based: 0 is NOT the first element — the classic off-by-one trap.
    assert_eq!(
        run("{1, 2, 3}[0]").unwrap_err(),
        ExprError::IndexOutOfBounds { index: 0, size: 3 }
    );
    assert_eq!(
        run("{1, 2, 3}[-1]").unwrap_err(),
        ExprError::IndexOutOfBounds { index: -1, size: 3 }
    );
    assert_eq!(
        run("{1, 2, 3}[4]").unwrap_err(),
        ExprError::IndexOutOfBounds { index: 4, size: 3 }
    );
}

#[test]
fn extreme_integer_subscripts_reject_without_wrapping() {
    // The wrap traps: a `(i - 1) as usize` computed BEFORE the bounds check would turn
    // i64::MIN into a huge offset (debug-overflow or wrap), and an `as usize` comparison
    // would misclassify on 32-bit targets. Bounds run on the i64 itself, so both extremes
    // report the exact out-of-bounds fields.
    assert_eq!(
        run("{7}[9223372036854775807]").unwrap_err(),
        ExprError::IndexOutOfBounds {
            index: i64::MAX,
            size: 1,
        }
    );
    // i64::MIN cannot be spelled as a literal (its magnitude overflows the Int token), so it
    // arrives through the scope like any computed subscript would.
    let scope = TestScope::new(&[("n", Value::Integer(i64::MIN))]);
    assert_eq!(
        eval_str("{7}[n]", &scope).unwrap_err(),
        ExprError::IndexOutOfBounds {
            index: i64::MIN,
            size: 1,
        }
    );
}

#[test]
fn every_subscript_of_an_empty_array_is_out_of_bounds() {
    // An empty array has no valid subscript; the error reports size 0, exact to the field.
    assert_eq!(
        run("(5:3)[1]").unwrap_err(),
        ExprError::IndexOutOfBounds { index: 1, size: 0 }
    );
    assert_eq!(
        run("(5:3)[0]").unwrap_err(),
        ExprError::IndexOutOfBounds { index: 0, size: 0 }
    );
    assert_eq!(
        run("fill(1, 0)[1]").unwrap_err(),
        ExprError::IndexOutOfBounds { index: 1, size: 0 }
    );
}

// --- Subscript typing: Integer only, no coercion --------------------------------------------

#[test]
fn non_integer_subscripts_are_rejected_without_coercion() {
    assert_eq!(
        run("{1, 2}[1.5]").unwrap_err(),
        ExprError::NonIntegerIndex { found: "Real" }
    );
    assert_eq!(
        run("{1, 2}[true]").unwrap_err(),
        ExprError::NonIntegerIndex { found: "Boolean" }
    );
    assert_eq!(
        run("{1, 2}[\"a\"]").unwrap_err(),
        ExprError::NonIntegerIndex { found: "String" }
    );
    // A whole-valued Real is STILL rejected — no silent coercion. Modelica subscripts are
    // Integer-typed, and flooring `1.0` would invite the 0.999999… rounding trap: a computed
    // subscript one ulp under its intended integer would silently read the wrong element
    // instead of failing loudly.
    assert_eq!(
        run("{1, 2}[1.0]").unwrap_err(),
        ExprError::NonIntegerIndex { found: "Real" }
    );
    // An Enumeration subscript is rejected like any other non-Integer scalar — an enum's
    // 1-based ordinal is NOT usable as an index without an explicit conversion.
    assert_eq!(
        run("{1, 2}[Buildings.Controls.OBC.CDL.Types.ZeroTime.NY2017]").unwrap_err(),
        ExprError::NonIntegerIndex {
            found: "Enumeration"
        }
    );
    // An array-valued subscript is the established array-in-scalar-position TypeError
    // (vector subscripts are a slicing feature, and slicing is out of subset).
    assert!(matches!(
        run("{1, 2}[{1}]"),
        Err(ExprError::TypeError { .. })
    ));
}

// --- Base typing and chains -----------------------------------------------------------------

#[test]
fn scalar_bases_are_type_errors_naming_the_found_type() {
    assert_eq!(
        run("5[1]").unwrap_err(),
        ExprError::TypeError {
            expected: "an array to index",
            found: "Integer",
        }
    );
    let scope = TestScope::new(&[("x", Value::Real(2.5))]);
    assert_eq!(
        eval_str("x[1]", &scope).unwrap_err(),
        ExprError::TypeError {
            expected: "an array to index",
            found: "Real",
        }
    );
    assert!(matches!(run("true[1]"), Err(ExprError::TypeError { .. })));
    assert!(matches!(run("\"s\"[1]"), Err(ExprError::TypeError { .. })));
}

#[test]
fn chained_indexing_hits_the_scalar_base_type_error() {
    // A[1] yields a scalar, so a second subscript group indexes a scalar — no special case,
    // the ordinary base check reports it.
    assert!(parse("a[1][2]").is_ok());
    assert_eq!(
        run("{1, 2}[1][1]").unwrap_err(),
        ExprError::TypeError {
            expected: "an array to index",
            found: "Integer",
        }
    );
}

#[test]
fn base_and_subscript_evaluation_errors_propagate() {
    assert_eq!(
        run("missing[1]").unwrap_err(),
        ExprError::UnknownIdent("missing".to_string())
    );
    assert_eq!(
        run("{1}[missing]").unwrap_err(),
        ExprError::UnknownIdent("missing".to_string())
    );
}

// --- Grammar: precedence, slicing/matrix/comprehension rejections ---------------------------

#[test]
fn subscripts_bind_tighter_than_the_unary_sign() {
    // `-A[i]` is `-(A[i])`: if the sign bound tighter it would negate the ARRAY — a
    // TypeError — so these succeeding at all pins the precedence, and the values pin it
    // twice.
    assert!(scalar("-{1, 2}[2]").bit_eq(&Value::Integer(-2)));
    assert!(scalar("-(1:3)[3]").bit_eq(&Value::Integer(-3)));
    assert_scalar_real_bits(scalar("-{1.5, 2.5}[1]"), (-1.5f64).to_bits());
}

#[test]
fn slicing_subscripts_are_a_typed_parse_error() {
    for s in ["a[1:2]", "a[1:2:3]", "a[(1:2)]", "{1, 2}[1:2]"] {
        let Err(ExprError::Parse(msg)) = parse(s) else {
            panic!("{s:?} should be a parse error");
        };
        assert!(
            msg.contains("slicing"),
            "{s:?} should name slicing as unsupported: {msg}"
        );
    }
}

#[test]
fn empty_subscript_brackets_are_a_parse_error() {
    let Err(ExprError::Parse(msg)) = parse("a[]") else {
        panic!("a[] should be a parse error");
    };
    assert!(msg.contains("empty subscript"), "got: {msg}");
    assert!(matches!(parse("{1, 2}[]"), Err(ExprError::Parse(_))));
}

#[test]
fn multiple_subscripts_parse_then_defer_at_evaluation() {
    assert!(parse("a[1, 2]").is_ok());
    let deferral = "arrays are 1-D, so a subscript takes exactly one index \
                    (multi-dimensional indexing is deferred)";
    assert_eq!(
        run("{1, 2}[1, 2]").unwrap_err(),
        ExprError::DomainError(deferral)
    );
    // The count gate runs before any subscript evaluates: unbound subscript identifiers
    // still get the deferral message, not UnknownIdent.
    let scope = TestScope::new(&[]).bind_array("arr", array_in("1:3", &TestScope::new(&[])));
    assert_eq!(
        eval_str("arr[i, j]", &scope).unwrap_err(),
        ExprError::DomainError(deferral)
    );
}

#[test]
fn matrix_constructor_rejection_is_byte_identical() {
    // The primary-position `[` rejection predates indexing and must not have shifted.
    assert_eq!(
        parse("[1, 2]").unwrap_err(),
        ExprError::Parse("matrix constructor [a,b;c,d] is not supported".to_string())
    );
    // The full matrix spelling never reaches the parser — `;` is outside the lexical subset
    // (pre-existing behavior, pinned at variant granularity).
    assert!(matches!(parse("[a,b;c,d]"), Err(ExprError::Parse(_))));
}

// --- Determinism goldens --------------------------------------------------------------------

#[test]
fn repeated_indexed_evaluation_is_bit_identical() {
    let nan = Value::Real(f64::from_bits(0xfff8_0000_0000_0000));
    let scope = TestScope::new(&[("a", nan)]);
    for expr in ["(0:0.1:1)[7]", "{1.0, a}[2]", "{-0.0, 0.0}[1]"] {
        let first = scalar_in(expr, &scope);
        let second = scalar_in(expr, &scope);
        assert!(first.bit_eq(&second), "{expr}: {first:?} != {second:?}");
    }
}
