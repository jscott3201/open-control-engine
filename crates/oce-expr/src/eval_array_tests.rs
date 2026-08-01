//! Unit tests for 1-D array literals and ranges, mirroring `eval_array.rs`. Real elements are
//! compared **bit-exactly** (`to_bits`) — these are ground folds, and the -0.0/NaN element bit
//! policy is part of the determinism contract. The Real-range oracle vectors were precomputed
//! offline with the closed form `start + k * step` and are pinned as literal bit patterns.

use oce_model::{Value, ValueType};

use super::{ArrayValue, EvalResult, ExprAst, ExprError, Scope, Shape, eval, eval_str, parse};

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
}

/// Evaluate with an empty scope.
fn run(s: &str) -> Result<EvalResult, ExprError> {
    eval_str(s, &TestScope::new(&[]))
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

// --- Brace literals -----------------------------------------------------------------------

#[test]
fn literal_folds_expression_elements_left_to_right() {
    assert_integer_elements(&array("{1 + 1, 2 * 3, -4}"), &[2, 6, -4]);
    assert_integer_elements(&array("{min(2, 3), abs(-7)}"), &[2, 7]);
}

#[test]
fn all_integer_literal_preserves_integer_type() {
    let a = array("{1, 2, 3}");
    assert_integer_elements(&a, &[1, 2, 3]);
    assert_eq!(a.shape(), Shape::D1(3));
    assert!(!a.is_empty());
}

#[test]
fn any_real_element_promotes_the_whole_literal_to_real() {
    assert_real_element_bits(&array("{1, 2.5}"), &[1.0f64.to_bits(), 2.5f64.to_bits()]);
    // A single Real in the last position still promotes the earlier Integer elements.
    assert_real_element_bits(
        &array("{1, 2, 3.0}"),
        &[1.0f64.to_bits(), 2.0f64.to_bits(), 3.0f64.to_bits()],
    );
}

#[test]
fn boolean_literal_forms_a_boolean_array() {
    let a = array("{true, false, 1 < 2}");
    assert_eq!(a.elem_type(), ValueType::Boolean);
    assert_eq!(a.len(), 3);
    let expected = [true, false, true];
    for (got, want) in a.as_slice().iter().zip(expected) {
        assert!(got.bit_eq(&Value::Boolean(want)), "got {got:?}");
    }
}

#[test]
fn empty_brace_literal_is_a_typed_error_not_a_fabricated_type() {
    assert_eq!(run("{}").unwrap_err(), ExprError::EmptyArray);
}

#[test]
fn nested_array_elements_are_type_errors_pending_two_d() {
    assert!(matches!(
        run("{{1, 2}, {3, 4}}"),
        Err(ExprError::TypeError { .. })
    ));
    assert!(matches!(
        run("{1, {2, 3}}"),
        Err(ExprError::TypeError { .. })
    ));
    // A range element parses inside a literal but evaluates to an array — same rejection.
    assert!(matches!(run("{1:2}"), Err(ExprError::TypeError { .. })));
    assert!(matches!(
        run("{1, 2:3, 4}"),
        Err(ExprError::TypeError { .. })
    ));
}

#[test]
fn string_enum_and_mixed_literal_elements_are_type_errors() {
    assert!(matches!(
        run("{\"a\", \"b\"}"),
        Err(ExprError::TypeError { .. })
    ));
    assert!(matches!(run("{1, true}"), Err(ExprError::TypeError { .. })));
    assert!(matches!(
        run("{true, 1.0}"),
        Err(ExprError::TypeError { .. })
    ));
    assert!(matches!(
        run("{1, \"a\"}"),
        Err(ExprError::TypeError { .. })
    ));
}

// --- Ranges: counts, promotion, direction --------------------------------------------------

#[test]
fn two_operand_range_defaults_to_a_step_of_one() {
    assert_integer_elements(&array("1:4"), &[1, 2, 3, 4]);
    assert_integer_elements(&array("3:3"), &[3]);
    assert_integer_elements(&array("-2:2"), &[-2, -1, 0, 1, 2]);
}

#[test]
fn any_real_range_operand_promotes_the_range_to_real() {
    assert_real_element_bits(
        &array("1:0.5:2"),
        &[1.0f64.to_bits(), 1.5f64.to_bits(), 2.0f64.to_bits()],
    );
    // Integer endpoints, fractional span: 1:2.5 stops at 2.0 (the inclusive bound is never
    // stepped past).
    assert_real_element_bits(&array("1:2.5"), &[1.0f64.to_bits(), 2.0f64.to_bits()]);
}

#[test]
fn real_range_elements_match_hand_computed_closed_form_oracles() {
    // Bits precomputed offline as `start + (k as f64) * step` and pinned literally. These
    // vectors discriminate the closed form from an accumulating `acc += step` loop, which
    // drifts one ulp low on 0:0.1:1 from k = 6 onward and one ulp high on 1:0.1:1.3 at k = 2.
    // The 1:0.1:1.75 vector additionally discriminates FMA: a fused
    // `(k as f64).mul_add(step, start)` (the rewrite clippy's suboptimal_flops suggests —
    // forbidden here, its single rounding is not IEEE-reproducible across targets) agrees with
    // the two-rounding closed form on every other pinned element but diverges at k = 7:
    // fused 0x3FFB333333333333 (1.7) vs closed 0x3FFB333333333334 (1.7000000000000002).
    assert_real_element_bits(
        &array("0:0.1:1"), // 11 elements — a naive `while acc <= stop` loop is off by one
        &[
            0x0000000000000000, // 0.0
            0x3FB999999999999A, // 0.1
            0x3FC999999999999A, // 0.2
            0x3FD3333333333334, // 0.30000000000000004
            0x3FD999999999999A, // 0.4
            0x3FE0000000000000, // 0.5
            0x3FE3333333333334, // 0.6000000000000001
            0x3FE6666666666667, // 0.7000000000000001
            0x3FE999999999999A, // 0.8
            0x3FECCCCCCCCCCCCD, // 0.9
            0x3FF0000000000000, // 1.0
        ],
    );
    assert_real_element_bits(
        &array("0:0.2:1"),
        &[
            0x0000000000000000, // 0.0
            0x3FC999999999999A, // 0.2
            0x3FD999999999999A, // 0.4
            0x3FE3333333333334, // 0.6000000000000001
            0x3FE999999999999A, // 0.8
            0x3FF0000000000000, // 1.0
        ],
    );
    assert_real_element_bits(
        &array("1:0.1:1.3"),
        &[
            0x3FF0000000000000, // 1.0
            0x3FF199999999999A, // 1.1
            0x3FF3333333333333, // 1.2
            0x3FF4CCCCCCCCCCCD, // 1.3
        ],
    );
    assert_real_element_bits(
        // 8 elements: (1.75 - 1)/0.1 = 7.499999999999999, floor + 1 = 8. A stop of 1.7 would
        // NOT reach the FMA-discriminating k = 7 — its f64 count is 7 (see the comment above).
        &array("1:0.1:1.75"),
        &[
            0x3FF0000000000000, // 1.0
            0x3FF199999999999A, // 1.1
            0x3FF3333333333333, // 1.2
            0x3FF4CCCCCCCCCCCD, // 1.3
            0x3FF6666666666666, // 1.4
            0x3FF8000000000000, // 1.5
            0x3FF999999999999A, // 1.6
            0x3FFB333333333334, // 1.7000000000000002 — mul_add would give 0x3FFB333333333333
        ],
    );
    assert_real_element_bits(
        &array("0:0.5:2"),
        &[
            0x0000000000000000, // 0.0
            0x3FE0000000000000, // 0.5
            0x3FF0000000000000, // 1.0
            0x3FF8000000000000, // 1.5
            0x4000000000000000, // 2.0
        ],
    );
}

#[test]
fn empty_ranges_are_legal_typed_empty_arrays() {
    let a = array("5:3"); // stop < start with the default positive step
    assert_eq!(a.elem_type(), ValueType::Integer);
    assert_eq!(a.shape(), Shape::D1(0));
    assert!(a.is_empty());

    let b = array("3:-1:5"); // ascending span against a descending step
    assert_eq!(b.elem_type(), ValueType::Integer);
    assert!(b.is_empty());

    let c = array("1.0:0.5:0.0"); // Real empty keeps the promoted element type
    assert_eq!(c.elem_type(), ValueType::Real);
    assert!(c.is_empty());
}

#[test]
fn descending_range_with_negative_step_counts_down() {
    assert_integer_elements(&array("5:-1:3"), &[5, 4, 3]);
    assert_real_element_bits(
        &array("2.0:-0.5:1.0"),
        &[2.0f64.to_bits(), 1.5f64.to_bits(), 1.0f64.to_bits()],
    );
}

#[test]
fn zero_step_is_a_domain_error() {
    assert_eq!(
        run("1:0:5").unwrap_err(),
        ExprError::DomainError("range step is zero")
    );
    assert_eq!(
        run("1.0:0.0:5.0").unwrap_err(),
        ExprError::DomainError("range step is zero")
    );
    // -0.0 == 0.0, so a negative-zero Real step is the same domain error.
    let scope = TestScope::new(&[("s", Value::Real(-0.0))]);
    assert_eq!(
        eval_str("1.0:s:5.0", &scope).unwrap_err(),
        ExprError::DomainError("range step is zero")
    );
}

// --- Resource cap and overflow safety ------------------------------------------------------

#[test]
fn full_width_integer_endpoints_reject_without_overflow_or_allocation() {
    // stop - start == 2^64 - 1 overflows i64; the i128 count math must not. A 2^64-element
    // Vec is unallocatable, so these tests completing at all proves the reject path allocates
    // nothing.
    let scope = TestScope::new(&[
        ("lo", Value::Integer(i64::MIN)),
        ("hi", Value::Integer(i64::MAX)),
    ]);
    assert_eq!(
        eval_str("lo:hi", &scope).unwrap_err(),
        ExprError::ArrayTooLarge {
            count: 1u128 << 64,
            max: 1 << 20,
        }
    );
    // Descending full width: the negated span divides by the negative step.
    assert_eq!(
        eval_str("hi:-1:lo", &scope).unwrap_err(),
        ExprError::ArrayTooLarge {
            count: 1u128 << 64,
            max: 1 << 20,
        }
    );
}

#[test]
fn range_elements_at_the_integer_edges_do_not_overflow() {
    let scope = TestScope::new(&[
        ("nearHi", Value::Integer(i64::MAX - 2)),
        ("hi", Value::Integer(i64::MAX)),
        ("lo", Value::Integer(i64::MIN)),
    ]);
    assert_integer_elements(
        &array_in("nearHi:hi", &scope),
        &[i64::MAX - 2, i64::MAX - 1, i64::MAX],
    );
    assert_integer_elements(
        &array_in("lo:lo", &scope), // single-element range at the far edge
        &[i64::MIN],
    );
}

#[test]
fn tiny_real_step_reports_array_too_large_without_panicking() {
    // ~1e300 requested elements: far beyond u128, so the reported count saturates. The count
    // stays in the f64 domain until after the cap check — `floor() as i64` would saturate at
    // i64::MAX and the closed-form `+ 1` would overflow in debug builds.
    assert_eq!(
        run("0:1e-300:1").unwrap_err(),
        ExprError::ArrayTooLarge {
            count: u128::MAX,
            max: 1 << 20,
        }
    );
}

#[test]
fn element_cap_boundary_is_exact() {
    // 1 << 20 elements: exactly at the cap, accepted.
    let at_cap = array("1:1048576");
    assert_eq!(at_cap.len(), 1 << 20);
    assert!(at_cap.as_slice()[0].bit_eq(&Value::Integer(1)));
    assert!(at_cap.as_slice()[(1 << 20) - 1].bit_eq(&Value::Integer(1_048_576)));
    drop(at_cap); // release before the Real-path array below halves peak test memory
    // One element past the cap: rejected with the exact count.
    assert_eq!(
        run("1:1048577").unwrap_err(),
        ExprError::ArrayTooLarge {
            count: 1_048_577,
            max: 1 << 20,
        }
    );
    // The Real path checks the same boundary.
    assert_eq!(array("1.0:1048576.0").len(), 1 << 20);
    assert_eq!(
        run("1.0:1048577.0").unwrap_err(),
        ExprError::ArrayTooLarge {
            count: 1_048_577,
            max: 1 << 20,
        }
    );
}

#[test]
fn oversized_literal_ast_is_rejected_before_evaluating_elements() {
    // Binding text can't practically spell a literal this large (its element count equals its
    // AST node count), so build the AST directly to pin that the literal path checks the cap
    // before allocating its value Vec — uniformly with the range paths.
    let elems = vec![ExprAst::Int(0); (1 << 20) + 1];
    assert_eq!(
        eval(&ExprAst::ArrayLit(elems), &TestScope::new(&[])).unwrap_err(),
        ExprError::ArrayTooLarge {
            count: (1 << 20) + 1,
            max: 1 << 20,
        }
    );
}

// --- Operand typing and scalar-position rejection ------------------------------------------

#[test]
fn non_numeric_range_operands_are_type_errors() {
    assert!(matches!(
        run("true:false"),
        Err(ExprError::TypeError { .. })
    ));
    assert!(matches!(
        run("\"a\":\"b\""),
        Err(ExprError::TypeError { .. })
    ));
    assert!(matches!(run("1:true"), Err(ExprError::TypeError { .. })));
    assert!(matches!(run("1:2:false"), Err(ExprError::TypeError { .. })));
    // An array-valued operand is rejected the same way (ranges of ranges are not a thing).
    assert!(matches!(run("{1, 2}:3"), Err(ExprError::TypeError { .. })));
}

#[test]
fn array_in_scalar_operand_position_is_a_type_error() {
    assert!(matches!(
        run("{1, 2} + 1"),
        Err(ExprError::TypeError { .. })
    ));
    assert!(matches!(
        run("1 + {1, 2}"),
        Err(ExprError::TypeError { .. })
    ));
    assert!(matches!(
        run("abs({1, 2})"),
        Err(ExprError::TypeError { .. })
    ));
    assert!(matches!(run("-{1, 2}"), Err(ExprError::TypeError { .. })));
    assert!(matches!(
        run("not {true, false}"),
        Err(ExprError::TypeError { .. })
    ));
    assert!(matches!(
        run("1.0 / {1, 2}"),
        Err(ExprError::TypeError { .. })
    ));
}

// --- Grammar: precedence, contexts, and identifiers ----------------------------------------

#[test]
fn unary_minus_binds_inside_range_operands() {
    // `-1:2` is `(-1):2`, not `-(1:2)`.
    assert_integer_elements(&array("-1:2"), &[-1, 0, 1, 2]);
    assert_integer_elements(&array("-1:-1:-3"), &[-1, -2, -3]);
    // The colon is looser than arithmetic: `1:n-1` spans the whole subtraction.
    let scope = TestScope::new(&[("n", Value::Integer(4))]);
    assert_integer_elements(&array_in("1:n-1", &scope), &[1, 2, 3]);
}

#[test]
fn range_parses_in_parentheses_and_call_arguments() {
    assert_integer_elements(&array("(1:3)"), &[1, 2, 3]);
    // The grammar threads ranges through argument lists (`sum(1:3)` consumes one as its
    // argument); the scalar built-ins reject the array at evaluation — a TypeError, not a
    // parse error.
    assert!(parse("min(1:3, 2)").is_ok());
    assert!(matches!(
        run("min(1:3, 2)"),
        Err(ExprError::TypeError { .. })
    ));
}

#[test]
fn identifier_bound_to_an_array_reads_back_bit_identical() {
    let nan = Value::Real(f64::from_bits(0xfff8_0000_0000_0000));
    let source = array_in("{1.0, a}", &TestScope::new(&[("a", nan)]));
    let scope = TestScope::new(&[]).bind_array("arr", source);
    let read = array_in("arr", &scope);
    // The literal already canonicalized the NaN element, and ArrayValue's sole constructor
    // canonicalizes every Real — so the read-path re-canonicalization is structural
    // belt-and-braces, not behavior this assertion can observe (a plain clone would pass too).
    // What IS under test: the identifier read returns the stored array bit-identically.
    assert_real_element_bits(&read, &[1.0f64.to_bits(), 0x7FF8_0000_0000_0000]);
    // An array-valued identifier in scalar operand position is a TypeError, like any array.
    assert!(matches!(
        eval_str("arr + 1", &scope),
        Err(ExprError::TypeError { .. })
    ));
}

// --- Determinism goldens -------------------------------------------------------------------

#[test]
fn negative_zero_and_nan_element_bits_are_pinned() {
    // Literals preserve -0.0 (only NaN is canonicalized).
    assert_real_element_bits(
        &array("{0.0, -0.0}"),
        &[0.0f64.to_bits(), (-0.0f64).to_bits()],
    );
    // A NaN element (bound here via the scope; pure binding text reaches NaN too, e.g.
    // `0.0/0.0`) canonicalizes to the positive quiet NaN, in a plain literal and through
    // Integer→Real promotion alike.
    let nan = Value::Real(f64::from_bits(0xfff8_0000_0000_0000));
    assert_real_element_bits(
        &array_in("{a}", &TestScope::new(&[("a", nan.clone())])),
        &[0x7FF8_0000_0000_0000],
    );
    assert_real_element_bits(
        &array_in("{1, a}", &TestScope::new(&[("a", nan.clone())])),
        &[1.0f64.to_bits(), 0x7FF8_0000_0000_0000],
    );
    // Range elements come from the closed form, so k == 0 is `start + 0.0`: a -0.0 start
    // surfaces as +0.0 (IEEE addition normalizes the sign).
    assert_real_element_bits(&array("-0.0:1.0"), &[0.0f64.to_bits(), 1.0f64.to_bits()]);
    // A NaN range operand defeats the count comparison: the result is a legal empty Real
    // array — the deliberate total-function policy — never a panic and never a NaN-length
    // allocation. NaN is reachable from pure binding text, not just a scope value:
    let folded_nan = array("0.0/0.0:1.0"); // `/` is IEEE, so 0.0/0.0 folds to a NaN start
    assert_eq!(folded_nan.elem_type(), ValueType::Real);
    assert!(folded_nan.is_empty());
    let scope = TestScope::new(&[("a", nan)]);
    let empty = array_in("a:1.0", &scope);
    assert_eq!(empty.elem_type(), ValueType::Real);
    assert!(empty.is_empty());
}

#[test]
fn repeated_evaluation_is_bit_identical() {
    let first = array("0:0.1:1");
    let second = array("0:0.1:1");
    assert_eq!(first.len(), second.len());
    for (a, b) in first.as_slice().iter().zip(second.as_slice()) {
        assert!(a.bit_eq(b), "{a:?} != {b:?}");
    }
}

// --- Malformed input: typed parse errors, never a panic ------------------------------------

#[test]
fn malformed_array_syntax_is_a_parse_error_never_a_panic() {
    let cases = [
        "{",
        "}",
        "[",
        "]",
        "[1, 2]",
        "[a,b;c,d]",
        "{1,",
        "{,1}",
        "{1;2}",
        "{1 2}",
        "{1,}",
        "{{}",
        "{}}",
        "1:",
        ":",
        ":1",
        "1:2:",
        "1:2:3:4",
        "{:}",
        "1:}",
        "a[", // an unclosed subscript is still malformed — `a[1]` itself parses now
        "(1:3",
        "{1}{2}",
    ];
    for s in cases {
        assert!(
            matches!(run(s), Err(ExprError::Parse(_))),
            "{s:?} should be a parse error, got {:?}",
            run(s)
        );
    }
}
