//! Unit tests for the array-shaped built-ins, mirroring `eval_array_builtins.rs`. Real results
//! are compared **bit-exactly** (`to_bits`/`bit_eq`) — fold order, the empty-sum identity, and
//! the NaN/signed-zero reduction policy are part of the determinism contract, so every golden
//! pins exact bits, not approximate values.

use oce_model::{Value, ValueType};

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
}

impl Scope for TestScope {
    fn lookup(&self, name: &str) -> Option<&EvalResult> {
        self.vars.iter().find(|(k, _)| k == name).map(|(_, v)| v)
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

/// Evaluate with an empty scope, panicking the test unless the result is an array.
#[track_caller]
fn array(s: &str) -> ArrayValue {
    array_in(s, &TestScope::new(&[]))
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

#[track_caller]
fn assert_integer_elements(a: &ArrayValue, expected: &[i64]) {
    assert_eq!(a.len(), expected.len(), "length of {a:?}");
    assert_eq!(a.elem_type(), ValueType::Integer);
    for (got, want) in a.as_slice().iter().zip(expected) {
        assert!(
            got.bit_eq(&Value::Integer(*want)),
            "expected Integer({want}), got {got:?} in {a:?}"
        );
    }
}

#[track_caller]
fn assert_real_element_bits(a: &ArrayValue, expected_bits: &[u64]) {
    assert_eq!(a.len(), expected_bits.len(), "length of {a:?}");
    assert_eq!(a.elem_type(), ValueType::Real);
    for (i, (got, want)) in a.as_slice().iter().zip(expected_bits).enumerate() {
        let Value::Real(r) = got else {
            panic!("element {i} of {a:?} is not Real");
        };
        assert_eq!(
            r.to_bits(),
            *want,
            "element {i}: got {r:?} (0x{:016X}), want 0x{want:016X}",
            r.to_bits()
        );
    }
}

#[track_caller]
fn assert_boolean_elements(a: &ArrayValue, expected: &[bool]) {
    assert_eq!(a.len(), expected.len(), "length of {a:?}");
    assert_eq!(a.elem_type(), ValueType::Boolean);
    for (got, want) in a.as_slice().iter().zip(expected) {
        assert!(got.bit_eq(&Value::Boolean(*want)), "got {got:?} in {a:?}");
    }
}

// --- sum ------------------------------------------------------------------------------------

#[test]
fn sum_reduces_integer_arrays_exactly() {
    assert!(scalar("sum(1:4)").bit_eq(&Value::Integer(10)));
    assert!(scalar("sum({1, 2, 3})").bit_eq(&Value::Integer(6)));
    assert!(scalar("sum({2, -5, 3})").bit_eq(&Value::Integer(0)));
    assert!(scalar("sum(fill(7, 3))").bit_eq(&Value::Integer(21)));
}

#[test]
fn sum_folds_reals_left_to_right_not_reassociated() {
    // ORDER-DISCRIMINATING golden: the left-to-right fold (0.1 + 0.2) + 0.3 is
    // 0.6000000000000001 (0x3FE3333333333334); a right-associated 0.1 + (0.2 + 0.3) would be
    // exactly 0.6 (0x3FE3333333333333). Pinning the left bits pins the fold order.
    assert_scalar_real_bits(scalar("sum({0.1, 0.2, 0.3})"), 0x3FE3333333333334);
    // The fold composes with range construction: summing the 11 golden-pinned elements of
    // 0:0.1:1 (see `eval_array_tests`) left to right gives 5.500000000000001.
    assert_scalar_real_bits(scalar("sum(0:0.1:1)"), 0x4016000000000001);
}

#[test]
fn sum_of_empty_arrays_is_the_element_type_identity() {
    // Owner-ratified, provisional under R10.6: the empty sum is the typed identity, exact to
    // the bit — Integer(0) for Integer arrays, Real(+0.0) for Real arrays.
    assert!(scalar("sum(5:3)").bit_eq(&Value::Integer(0)));
    assert!(scalar("sum(fill(1, 0))").bit_eq(&Value::Integer(0)));
    assert_scalar_real_bits(scalar("sum(1.0:0.5:0.0)"), 0.0f64.to_bits());
    assert_scalar_real_bits(scalar("sum(fill(1.5, 0))"), 0.0f64.to_bits());
}

#[test]
fn sum_identity_seed_normalizes_a_leading_negative_zero() {
    // The Real fold is seeded from the +0.0 identity, so IEEE `0.0 + -0.0 == +0.0` turns an
    // all-negative-zero array into +0.0. Pinned as contract (an element-seeded fold would
    // return -0.0 here).
    assert_scalar_real_bits(scalar("sum({-0.0})"), 0.0f64.to_bits());
    assert_scalar_real_bits(scalar("sum({-0.0, -0.0})"), 0.0f64.to_bits());
}

#[test]
fn sum_integer_overflow_is_a_domain_error_not_a_panic() {
    let scope = TestScope::new(&[("hi", Value::Integer(i64::MAX))]);
    // i64::MAX + 1 overflows only at the final i128 → i64 narrowing; the fold itself is safe.
    assert_eq!(
        eval_str("sum(cat(1, fill(hi, 1), fill(1, 1)))", &scope).unwrap_err(),
        ExprError::DomainError("integer overflow in sum")
    );
    // Three copies of i64::MAX: far outside i64 yet nowhere near the i128 accumulator's range.
    assert_eq!(
        eval_str("sum(fill(hi, 3))", &scope).unwrap_err(),
        ExprError::DomainError("integer overflow in sum")
    );
    // A cancelling fold lands back inside i64 and succeeds — the accumulator is what widens.
    let lo_hi = TestScope::new(&[
        ("hi", Value::Integer(i64::MAX)),
        ("lo", Value::Integer(i64::MIN)),
    ]);
    assert!(scalar_in("sum(cat(1, fill(hi, 1), fill(lo, 1)))", &lo_hi).bit_eq(&Value::Integer(-1)));
}

#[test]
fn sum_rejects_boolean_arrays_and_scalar_arguments() {
    assert!(matches!(
        run("sum(fill(true, 3))"),
        Err(ExprError::TypeError { .. })
    ));
    assert!(matches!(
        run("sum({true, false})"),
        Err(ExprError::TypeError { .. })
    ));
    assert!(matches!(run("sum(1)"), Err(ExprError::TypeError { .. })));
    assert!(matches!(run("sum(1.5)"), Err(ExprError::TypeError { .. })));
}

// --- min / max (one-argument array forms) ---------------------------------------------------

#[test]
fn min_and_max_reduce_single_elements_to_themselves() {
    assert!(scalar("min({7})").bit_eq(&Value::Integer(7)));
    assert!(scalar("max({-7})").bit_eq(&Value::Integer(-7)));
    assert_scalar_real_bits(scalar("min({3.5})"), 3.5f64.to_bits());
    assert_scalar_real_bits(scalar("max({3.5})"), 3.5f64.to_bits());
}

#[test]
fn min_and_max_fold_with_det_min_det_max_nan_policy() {
    // The Real fold inherits det_min/det_max: a single NaN operand is dropped, a NaN-only
    // fold canonicalizes. (The literal already canonicalized the scope NaN's sign bit.)
    let nan = Value::Real(f64::from_bits(0xfff8_0000_0000_0000));
    let scope = TestScope::new(&[("a", nan), ("b", Value::Real(2.0)), ("c", Value::Real(3.0))]);
    assert_scalar_real_bits(scalar_in("min({a, b})", &scope), 2.0f64.to_bits());
    assert_scalar_real_bits(scalar_in("min({b, a})", &scope), 2.0f64.to_bits());
    assert_scalar_real_bits(scalar_in("max({a, b})", &scope), 2.0f64.to_bits());
    assert_scalar_real_bits(scalar_in("max({b, a})", &scope), 2.0f64.to_bits());
    // NaN in the middle of a longer fold still drops out.
    assert_scalar_real_bits(scalar_in("min({c, a, b})", &scope), 2.0f64.to_bits());
    assert_scalar_real_bits(scalar_in("max({b, a, c})", &scope), 3.0f64.to_bits());
    // All-NaN reduces to the canonical positive quiet NaN.
    assert_scalar_real_bits(scalar_in("min({a, a})", &scope), 0x7FF8_0000_0000_0000);
    assert_scalar_real_bits(scalar_in("max({a})", &scope), 0x7FF8_0000_0000_0000);
}

#[test]
fn min_and_max_pin_signed_zero_ordering() {
    // det_min orders -0.0 below +0.0; det_max orders +0.0 above -0.0 — in either operand order.
    assert_scalar_real_bits(scalar("min({0.0, -0.0})"), (-0.0f64).to_bits());
    assert_scalar_real_bits(scalar("min({-0.0, 0.0})"), (-0.0f64).to_bits());
    assert_scalar_real_bits(scalar("max({0.0, -0.0})"), 0.0f64.to_bits());
    assert_scalar_real_bits(scalar("max({-0.0, 0.0})"), 0.0f64.to_bits());
}

#[test]
fn min_and_max_of_empty_arrays_are_empty_array_errors() {
    assert_eq!(run("min(5:3)").unwrap_err(), ExprError::EmptyArray);
    assert_eq!(run("max(5:3)").unwrap_err(), ExprError::EmptyArray);
    assert_eq!(run("min(1.0:0.5:0.0)").unwrap_err(), ExprError::EmptyArray);
    assert_eq!(run("max(fill(1, 0))").unwrap_err(), ExprError::EmptyArray);
}

#[test]
fn min_and_max_are_exact_at_integer_edges() {
    // Above 2^53 an f64 fold would collapse adjacent values; the Integer fold must stay exact.
    let scope = TestScope::new(&[
        ("lo", Value::Integer(i64::MIN)),
        ("hi", Value::Integer(i64::MAX)),
        ("nearHi", Value::Integer(i64::MAX - 1)),
    ]);
    assert!(scalar_in("min({hi, lo})", &scope).bit_eq(&Value::Integer(i64::MIN)));
    assert!(scalar_in("max({lo, hi})", &scope).bit_eq(&Value::Integer(i64::MAX)));
    assert!(scalar_in("min({hi, nearHi})", &scope).bit_eq(&Value::Integer(i64::MAX - 1)));
    assert!(scalar_in("max({nearHi, hi})", &scope).bit_eq(&Value::Integer(i64::MAX)));
}

#[test]
fn min_and_max_reject_boolean_arrays_and_scalar_arguments() {
    assert!(matches!(
        run("min(fill(true, 2))"),
        Err(ExprError::TypeError { .. })
    ));
    assert!(matches!(
        run("max({true, false})"),
        Err(ExprError::TypeError { .. })
    ));
    // The type check runs before the emptiness check: an empty Boolean array is a TypeError.
    assert!(matches!(
        run("min(fill(true, 0))"),
        Err(ExprError::TypeError { .. })
    ));
    // A scalar argument to the one-argument form is a shape problem, reported as a TypeError.
    assert!(matches!(run("min(3)"), Err(ExprError::TypeError { .. })));
    assert!(matches!(run("max(3.5)"), Err(ExprError::TypeError { .. })));
}

// --- fill -----------------------------------------------------------------------------------

#[test]
fn fill_replicates_scalars_with_the_value_type() {
    assert_integer_elements(&array("fill(7, 2)"), &[7, 7]);
    assert_boolean_elements(&array("fill(false, 3)"), &[false, false, false]);
    assert_real_element_bits(
        &array("fill(1.5, 3)"),
        &[1.5f64.to_bits(), 1.5f64.to_bits(), 1.5f64.to_bits()],
    );
    // The fill value can be any expression, including a fold of in-scope bindings.
    let scope = TestScope::new(&[("n", Value::Integer(3))]);
    assert_integer_elements(&array_in("fill(n * 2, n)", &scope), &[6, 6, 6]);
}

#[test]
fn fill_canonicalizes_real_values_including_nan() {
    // A scope-held negative NaN replicates as the canonical positive quiet NaN.
    let nan = Value::Real(f64::from_bits(0xfff8_0000_0000_0000));
    let scope = TestScope::new(&[("a", nan)]);
    assert_real_element_bits(
        &array_in("fill(a, 2)", &scope),
        &[0x7FF8_0000_0000_0000, 0x7FF8_0000_0000_0000],
    );
    // -0.0 is preserved (only NaN is canonicalized) — same policy as array literals.
    assert_real_element_bits(
        &array("fill(-0.0, 2)"),
        &[(-0.0f64).to_bits(), (-0.0f64).to_bits()],
    );
}

#[test]
fn fill_of_zero_elements_is_a_typed_empty_array() {
    let ints = array("fill(1, 0)");
    assert!(ints.is_empty());
    assert_eq!(ints.elem_type(), ValueType::Integer);
    let reals = array("fill(1.5, 0)");
    assert!(reals.is_empty());
    assert_eq!(reals.elem_type(), ValueType::Real);
    let bools = array("fill(true, 0)");
    assert!(bools.is_empty());
    assert_eq!(bools.elem_type(), ValueType::Boolean);
}

#[test]
fn negative_fill_counts_are_domain_errors_not_wraps() {
    // The wrap trap: `-1 as usize` is ~1.8e19, so a cast-based guard would misreport a
    // negative count as ArrayTooLarge (or worse, allocate). The distinct DomainError proves
    // the checked usize::try_from conversion runs first.
    assert_eq!(
        run("fill(1, -1)").unwrap_err(),
        ExprError::DomainError("negative array size")
    );
    let scope = TestScope::new(&[("lo", Value::Integer(i64::MIN))]);
    assert_eq!(
        eval_str("fill(1, lo)", &scope).unwrap_err(),
        ExprError::DomainError("negative array size")
    );
}

#[test]
fn fill_counts_beyond_the_cap_are_rejected_before_allocation() {
    assert_eq!(
        run("fill(1, 1048577)").unwrap_err(),
        ExprError::ArrayTooLarge {
            count: (1 << 20) + 1,
            max: 1 << 20,
        }
    );
    // An i64::MAX-element Vec is unallocatable; this test completing proves the reject path
    // allocates nothing.
    let scope = TestScope::new(&[("hi", Value::Integer(i64::MAX))]);
    assert_eq!(
        eval_str("fill(1, hi)", &scope).unwrap_err(),
        ExprError::ArrayTooLarge {
            count: i64::MAX as u128,
            max: 1 << 20,
        }
    );
    // Exactly at the cap is legal.
    assert_eq!(array("fill(0, 1048576)").len(), 1 << 20);
}

#[test]
fn fill_rejects_non_integer_counts_and_non_scalar_operands() {
    assert!(matches!(
        run("fill(1, 2.0)"), // Real count
        Err(ExprError::TypeError { .. })
    ));
    assert!(matches!(
        run("fill(\"a\", 2)"), // String fill value (same policy as literal elements)
        Err(ExprError::TypeError { .. })
    ));
    assert!(matches!(
        run("fill({1}, 2)"), // array fill value (2-D result is deferred)
        Err(ExprError::TypeError { .. })
    ));
    assert!(matches!(
        run("fill(1, {2})"), // array count
        Err(ExprError::TypeError { .. })
    ));
}

#[test]
fn fill_with_more_than_one_extent_is_a_deferred_parse_error() {
    let Err(ExprError::Parse(msg)) = parse("fill(1, 2, 3)") else {
        panic!("fill(1, 2, 3) should be a parse error");
    };
    assert!(
        msg.contains("deferred"),
        "message should name the deferral: {msg}"
    );
    assert!(matches!(parse("fill(1)"), Err(ExprError::Parse(_))));
    assert!(matches!(parse("fill()"), Err(ExprError::Parse(_))));
}

// --- size -----------------------------------------------------------------------------------

#[test]
fn size_returns_the_shape_vector_for_one_d_arrays() {
    assert_integer_elements(&array("size(fill(1, 4))"), &[4]);
    assert_integer_elements(&array("size({1.5, 2.5})"), &[2]);
    assert_integer_elements(&array("size(fill(true, 2))"), &[2]);
    // An empty array has shape {0} — size is total over legal empties.
    assert_integer_elements(&array("size(5:3)"), &[0]);
}

#[test]
fn size_with_a_dimension_index_returns_a_scalar_extent() {
    assert!(scalar("size(fill(1, 4), 1)").bit_eq(&Value::Integer(4)));
    assert!(scalar("size(5:3, 1)").bit_eq(&Value::Integer(0)));
}

#[test]
fn size_dimension_indexes_below_one_are_shape_mismatches() {
    assert_eq!(
        run("size(fill(1, 4), 0)").unwrap_err(),
        ExprError::ShapeMismatch("array dimension index must be at least 1")
    );
    assert_eq!(
        run("size(fill(1, 4), -1)").unwrap_err(),
        ExprError::ShapeMismatch("array dimension index must be at least 1")
    );
}

#[test]
fn size_dimension_two_names_the_two_d_deferral() {
    assert_eq!(
        run("size(fill(1, 4), 2)").unwrap_err(),
        ExprError::ShapeMismatch("only dimension 1 exists until 2-D arrays land")
    );
    assert_eq!(
        run("size(fill(1, 4), 99)").unwrap_err(),
        ExprError::ShapeMismatch("only dimension 1 exists until 2-D arrays land")
    );
}

#[test]
fn size_rejects_scalar_arguments_and_non_integer_indexes() {
    assert!(matches!(run("size(1)"), Err(ExprError::TypeError { .. })));
    assert!(matches!(
        run("size(fill(1, 2), 1.0)"), // Real dimension index
        Err(ExprError::TypeError { .. })
    ));
    assert!(matches!(
        run("size(fill(1, 2), {1})"), // array dimension index
        Err(ExprError::TypeError { .. })
    ));
    assert!(matches!(parse("size()"), Err(ExprError::Parse(_))));
    assert!(matches!(parse("size(a, 1, 2)"), Err(ExprError::Parse(_))));
}

// --- cat ------------------------------------------------------------------------------------

#[test]
fn cat_concatenates_arrays_of_matching_element_type() {
    assert_integer_elements(&array("cat(1, {1, 2}, {3})"), &[1, 2, 3]);
    assert_integer_elements(&array("cat(1, {1}, {2}, {3})"), &[1, 2, 3]);
    assert_integer_elements(&array("cat(1, 1:3, 4:5)"), &[1, 2, 3, 4, 5]);
    assert_real_element_bits(
        &array("cat(1, {1.0}, {2.5, 3.5})"),
        &[1.0f64.to_bits(), 2.5f64.to_bits(), 3.5f64.to_bits()],
    );
    assert_boolean_elements(
        &array("cat(1, fill(true, 1), fill(false, 2))"),
        &[true, false, false],
    );
}

#[test]
fn cat_with_empty_operands_keeps_type_and_order() {
    assert_integer_elements(&array("cat(1, 5:3, 1:3)"), &[1, 2, 3]);
    assert_integer_elements(&array("cat(1, 1:3, 5:3)"), &[1, 2, 3]);
    let all_empty = array("cat(1, 5:3, 5:3)");
    assert!(all_empty.is_empty());
    assert_eq!(all_empty.elem_type(), ValueType::Integer);
    assert_real_element_bits(
        &array("cat(1, fill(1.5, 0), fill(2.5, 2))"),
        &[2.5f64.to_bits(), 2.5f64.to_bits()],
    );
}

#[test]
fn cat_element_type_mismatch_is_a_type_error() {
    // No Integer→Real promotion across cat operands (unlike inside one array literal).
    assert!(matches!(
        run("cat(1, {1}, {2.0})"),
        Err(ExprError::TypeError { .. })
    ));
    assert!(matches!(
        run("cat(1, {1.0}, fill(true, 1))"),
        Err(ExprError::TypeError { .. })
    ));
    // An empty operand still carries its type and still has to match.
    assert!(matches!(
        run("cat(1, fill(1.5, 0), {1})"),
        Err(ExprError::TypeError { .. })
    ));
}

#[test]
fn cat_result_cap_applies_to_the_summed_length_before_allocation() {
    // Each operand is individually at or under the cap; their sum is one element over. The
    // reject must fire on the summed length before the result Vec is allocated.
    assert_eq!(
        run("cat(1, 1:1048576, 1:1)").unwrap_err(),
        ExprError::ArrayTooLarge {
            count: (1 << 20) + 1,
            max: 1 << 20,
        }
    );
    // Exactly at the cap is legal.
    assert_eq!(array("cat(1, 1:1048575, 1:1)").len(), 1 << 20);
}

#[test]
fn cat_dimension_errors_are_typed() {
    assert_eq!(
        run("cat(2, {1}, {2})").unwrap_err(),
        ExprError::ShapeMismatch(
            "concatenation along dimension 2 or higher needs 2-D arrays, which are deferred"
        )
    );
    assert_eq!(
        run("cat(0, {1}, {2})").unwrap_err(),
        ExprError::ShapeMismatch("concatenation dimension must be at least 1")
    );
    assert_eq!(
        run("cat(-1, {1}, {2})").unwrap_err(),
        ExprError::ShapeMismatch("concatenation dimension must be at least 1")
    );
    assert!(matches!(
        run("cat(1.0, {1}, {2})"), // Real dimension
        Err(ExprError::TypeError { .. })
    ));
    assert!(matches!(
        run("cat({1}, {2}, {3})"), // array dimension
        Err(ExprError::TypeError { .. })
    ));
    assert!(matches!(
        run("cat(1, 5, {1})"), // scalar operand
        Err(ExprError::TypeError { .. })
    ));
}

#[test]
fn cat_needs_a_dimension_and_at_least_two_operands() {
    assert!(matches!(parse("cat(1, {1})"), Err(ExprError::Parse(_))));
    assert!(matches!(parse("cat(1)"), Err(ExprError::Parse(_))));
    assert!(matches!(parse("cat()"), Err(ExprError::Parse(_))));
}

// --- Closed world and parse flips -----------------------------------------------------------

#[test]
fn unlisted_array_functions_stay_unsupported() {
    assert_eq!(
        run("product({1, 2})").unwrap_err(),
        ExprError::UnsupportedFunction("product".to_string())
    );
    assert_eq!(
        run("transpose({1, 2})").unwrap_err(),
        ExprError::UnsupportedFunction("transpose".to_string())
    );
    assert_eq!(
        run("zeros(3)").unwrap_err(),
        ExprError::UnsupportedFunction("zeros".to_string())
    );
}

#[test]
fn one_argument_reductions_parse_and_fail_only_at_evaluation() {
    // Formerly parse-time deferral rejections; now the parse succeeds and an unbound
    // identifier is the (evaluation-time) error.
    assert!(parse("sum(x)").is_ok());
    assert!(parse("min(a)").is_ok());
    assert!(parse("max(a)").is_ok());
    assert_eq!(
        run("sum(x)").unwrap_err(),
        ExprError::UnknownIdent("x".to_string())
    );
    // Zero- and three-argument min/max stay arity errors at parse.
    assert!(matches!(parse("min()"), Err(ExprError::Parse(_))));
    assert!(matches!(parse("max(1, 2, 3)"), Err(ExprError::Parse(_))));
    assert!(matches!(parse("sum()"), Err(ExprError::Parse(_))));
    assert!(matches!(parse("sum({1}, {2})"), Err(ExprError::Parse(_))));
}

#[test]
fn scalar_builtins_still_reject_array_arguments() {
    // The eval_call split must not loosen the scalar built-ins: an array argument stays a
    // TypeError, and two-argument min/max stay scalar-only.
    assert!(matches!(
        run("abs({1, 2})"),
        Err(ExprError::TypeError { .. })
    ));
    assert!(matches!(
        run("min(1:3, 2)"),
        Err(ExprError::TypeError { .. })
    ));
    assert!(matches!(
        run("sqrt(fill(4.0, 1))"),
        Err(ExprError::TypeError { .. })
    ));
}

// --- Determinism goldens --------------------------------------------------------------------

#[test]
fn repeated_array_builtin_evaluation_is_bit_identical() {
    let nan = Value::Real(f64::from_bits(0xfff8_0000_0000_0000));
    let scope = TestScope::new(&[("a", nan)]);
    for expr in [
        "sum({0.1, 0.2, 0.3})",
        "min({-0.0, 0.0})",
        "max(cat(1, 0:0.1:0.5, fill(0.7, 2)))",
    ] {
        let first = scalar_in(expr, &scope);
        let second = scalar_in(expr, &scope);
        assert!(first.bit_eq(&second), "{expr}: {first:?} != {second:?}");
    }
    let first = array_in("cat(1, 0:0.1:0.5, fill(a, 2))", &scope);
    let second = array_in("cat(1, 0:0.1:0.5, fill(a, 2))", &scope);
    assert_eq!(first.len(), second.len());
    for (x, y) in first.as_slice().iter().zip(second.as_slice()) {
        assert!(x.bit_eq(y), "{x:?} != {y:?}");
    }
}

#[test]
fn array_builtins_compose_with_scalar_expressions() {
    // Reductions are scalars, so they participate in ordinary arithmetic and comparisons.
    assert!(scalar("sum(1:4) + min({2, 8})").bit_eq(&Value::Integer(12)));
    assert!(scalar("size(fill(1, 4), 1) == 4").bit_eq(&Value::Boolean(true)));
    assert!(scalar("max({1.0, 2.0}) / 4.0").bit_eq(&Value::Real(0.5)));
    // And array results feed back into array-shaped positions.
    assert_integer_elements(&array("size(cat(1, fill(1, 2), 1:3))"), &[5]);
    assert!(scalar("sum(size(fill(1, 4)))").bit_eq(&Value::Integer(4)));
}
