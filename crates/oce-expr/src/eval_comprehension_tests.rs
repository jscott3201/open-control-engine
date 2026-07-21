//! Unit tests for array comprehensions, mirroring `eval_comprehension.rs`. Real results are
//! compared **bit-exactly** (`to_bits`/`bit_eq`) — iteration order, the shared literal
//! promotion, and the empty-source policy are part of the determinism contract. The Real-body
//! goldens are pinned *per position*, so reversing the iteration order is a detectable
//! mutation, and the `sum` sugar golden's left-to-right fold bits differ from the reversed
//! fold's (0x3FE3333333333334 vs 0x3FE3333333333333).

use oce_model::{EnumClassId, Value, ValueType, enum_class_id, enum_member_ordinal};

use super::{ArrayValue, EvalResult, ExprError, Scope, eval_array, eval_str, parse};

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

// --- Bodies over Integer range sources ------------------------------------------------------

#[test]
fn integer_bodies_follow_the_iterator_in_source_order() {
    assert_integer_elements(&array("{i*2 for i in 1:3}"), &[2, 4, 6]);
    assert_integer_elements(&array("{i - 1 for i in 1:3}"), &[0, 1, 2]);
    // A single-element source iterates exactly once.
    assert_integer_elements(&array("{i*2 for i in 3:3}"), &[6]);
    // A descending source keeps ITS order — the body follows the source, never re-sorts.
    assert_integer_elements(&array("{i for i in 3:-1:1}"), &[3, 2, 1]);
}

#[test]
fn real_bodies_pin_closed_form_bits_per_position() {
    // Hand-computed closed forms, pinned PER POSITION so a reversed iteration order cannot
    // pass: i*0.5 is exact for i = 1..4.
    assert_real_element_bits(
        &array("{i*0.5 for i in 1:4}"),
        &[
            0x3FE0000000000000, // 1*0.5 = 0.5
            0x3FF0000000000000, // 2*0.5 = 1.0
            0x3FF8000000000000, // 3*0.5 = 1.5
            0x4000000000000000, // 4*0.5 = 2.0
        ],
    );
    // i*0.1 exercises inexact products: 3*0.1 rounds up to 0.30000000000000004.
    assert_real_element_bits(
        &array("{i*0.1 for i in 1:4}"),
        &[
            0x3FB999999999999A, // 1*0.1 = 0.1
            0x3FC999999999999A, // 2*0.1 = 0.2
            0x3FD3333333333334, // 3*0.1 = 0.30000000000000004
            0x3FD999999999999A, // 4*0.1 = 0.4
        ],
    );
}

#[test]
fn boolean_bodies_form_a_boolean_array() {
    assert_boolean_elements(&array("{i > 2 for i in 1:4}"), &[false, false, true, true]);
    assert_boolean_elements(&array("{i == 2 for i in 1:3}"), &[false, true, false]);
}

#[test]
fn constant_bodies_ignore_the_iterator() {
    assert_integer_elements(&array("{7 for i in 1:3}"), &[7, 7, 7]);
    assert_boolean_elements(&array("{true for i in 1:2}"), &[true, true]);
}

// --- Sources: literals, identifiers, Real ranges, Boolean arrays ----------------------------

#[test]
fn literal_and_identifier_sources_iterate_their_elements() {
    assert_integer_elements(&array("{x + 1 for x in {10, 20}}"), &[11, 21]);
    let scope = TestScope::new(&[]).bind_array("arr", array("7:9"));
    assert_integer_elements(&array_in("{v * 2 for v in arr}", &scope), &[14, 16, 18]);
}

#[test]
fn real_range_sources_iterate_promoted_elements() {
    // Source 0:0.5:1 is the pinned closed-form vector {0.0, 0.5, 1.0}; x + 0.25 is exact.
    assert_real_element_bits(
        &array("{x + 0.25 for x in 0:0.5:1}"),
        &[
            0x3FD0000000000000, // 0.0 + 0.25 = 0.25
            0x3FE8000000000000, // 0.5 + 0.25 = 0.75
            0x3FF4000000000000, // 1.0 + 0.25 = 1.25
        ],
    );
}

#[test]
fn boolean_sources_are_legal_iteration_sources() {
    // Any element type is legal as the SOURCE; here the body consumes Booleans.
    assert_boolean_elements(&array("{not b for b in {true, false}}"), &[false, true]);
}

#[test]
fn comprehension_sources_can_be_comprehensions() {
    // The source is itself a comprehension: {i*2 for i in 1:3} = {2, 4, 6}, then x + 1.
    assert_integer_elements(&array("{x + 1 for x in {i*2 for i in 1:3}}"), &[3, 5, 7]);
}

// --- Scope: shadowing, outer visibility, no leaking -----------------------------------------

#[test]
fn iterator_shadows_an_outer_binding_of_the_same_name() {
    let scope = TestScope::new(&[("i", Value::Integer(99))]);
    assert_integer_elements(&array_in("{i for i in 1:2}", &scope), &[1, 2]);
    // The shadow also covers a Real outer binding under an Integer iterator.
    let real_outer = TestScope::new(&[("i", Value::Real(0.5))]);
    assert_integer_elements(&array_in("{i for i in 1:2}", &real_outer), &[1, 2]);
}

#[test]
fn outer_bindings_stay_visible_in_the_body() {
    let scope = TestScope::new(&[("n", Value::Integer(10))]);
    assert_integer_elements(&array_in("{i + n for i in 1:2}", &scope), &[11, 12]);
    // An outer ARRAY binding is visible too — the body indexes it per iteration.
    let scope = TestScope::new(&[]).bind_array("arr", array("{10, 20, 30}"));
    assert_integer_elements(&array_in("{arr[i] for i in 1:3}", &scope), &[10, 20, 30]);
}

#[test]
fn iterator_binding_does_not_leak_into_the_outer_scope() {
    // Structurally guaranteed (the child scope is a wrapper the comprehension drops), but
    // pinned: after evaluating the comprehension, `i` alone still resolves to the OUTER 99.
    let scope = TestScope::new(&[("i", Value::Integer(99))]);
    assert_integer_elements(&array_in("{i for i in 1:2}", &scope), &[1, 2]);
    assert!(scalar_in("i", &scope).bit_eq(&Value::Integer(99)));
    // And against an empty outer scope the iterator name is simply unknown afterwards.
    let empty = TestScope::new(&[]);
    assert_integer_elements(&array_in("{j for j in 1:2}", &empty), &[1, 2]);
    assert_eq!(
        eval_str("j", &empty).unwrap_err(),
        ExprError::UnknownIdent("j".to_string())
    );
}

// --- The sum reduction sugar ----------------------------------------------------------------

#[test]
fn sum_sugar_reduces_left_to_right() {
    // Gauss, bit-exact in the i128-widened Integer fold.
    assert!(scalar("sum(i for i in 1:100)").bit_eq(&Value::Integer(5050)));
    // ORDER-DISCRIMINATING Real golden: the left-to-right fold 0.0 + 0.1 + 0.2
    // + 0.30000000000000004 lands on 0.6000000000000001 (0x3FE3333333333334); folding the
    // same elements in REVERSED source order gives exactly 0.6 (0x3FE3333333333333).
    assert_scalar_real_bits(scalar("sum(i*0.1 for i in 1:3)"), 0x3FE3333333333334);
}

#[test]
fn sum_sugar_promotes_real_bodies() {
    // Integer iterator, Real body: the collected array is Real, so the sum is Real.
    assert_scalar_real_bits(scalar("sum(i*0.5 for i in 1:3)"), 3.0f64.to_bits());
    // All-Integer body stays Integer through the same promotion rules.
    assert!(scalar("sum(i*2 for i in 1:3)").bit_eq(&Value::Integer(12)));
}

#[test]
fn sum_sugar_is_the_same_node_as_an_explicit_comprehension_argument() {
    // The sugar only re-shapes the parse: sum(e for i in r) == sum({e for i in r}).
    assert!(scalar("sum({i for i in 1:100})").bit_eq(&Value::Integer(5050)));
    assert_scalar_real_bits(scalar("sum({i*0.1 for i in 1:3})"), 0x3FE3333333333334);
}

#[test]
fn boolean_bodies_are_rejected_by_sums_numeric_gate() {
    // A Boolean body is a legal comprehension (pinned above) but not a legal sum operand:
    // the collected Boolean array hits sum's existing numeric gate, exact to the fields —
    // the sugar adds no coercion and no bypass around the built-in's type checks.
    assert_eq!(
        run("sum(i > 2 for i in 1:4)").unwrap_err(),
        ExprError::TypeError {
            expected: "a numeric array",
            found: "a Boolean array",
        }
    );
    // Identical through the explicit-argument spelling, pinning sugar/argument equivalence
    // on the error path too.
    assert_eq!(
        run("sum({i > 2 for i in 1:4})").unwrap_err(),
        ExprError::TypeError {
            expected: "a numeric array",
            found: "a Boolean array",
        }
    );
}

// --- Empty-source policy --------------------------------------------------------------------

#[test]
fn empty_iteration_sources_are_empty_array_errors_and_never_evaluate_the_body() {
    // POLICY (pinned): an empty-source comprehension is ExprError::EmptyArray, mirroring the
    // `{}` literal. The body never evaluates over an empty source, so the result's element
    // type is unknowable — fabricating a typed empty array would risk silent mistyping
    // downstream, exactly the hazard the `{}` policy exists to prevent. (Typed empties stay
    // legal where the type IS knowable: empty ranges and fill(x, 0) carry their operand's
    // type.)
    assert_eq!(run("{i for i in 1:0}").unwrap_err(), ExprError::EmptyArray);
    assert_eq!(run("{i for i in 5:3}").unwrap_err(), ExprError::EmptyArray);
    assert_eq!(
        run("{x for x in 1.0:0.5:0.0}").unwrap_err(),
        ExprError::EmptyArray
    );
    // The body is NEVER evaluated: an unbound body identifier would be UnknownIdent if it
    // ran, but the empty source short-circuits first.
    assert_eq!(
        run("{missing for i in 1:0}").unwrap_err(),
        ExprError::EmptyArray
    );
}

#[test]
fn sum_of_an_empty_comprehension_is_the_same_empty_array_error() {
    // POLICY (pinned): the sugar is pure syntax, so sum(e for i in 1:0) inherits the
    // EmptyArray error above. This DIVERGES from doc-02's comprehension-sum row (empty → 0)
    // deliberately: that row cannot say WHICH zero — Integer(0) vs Real(0.0) — without a
    // body type to read it from, and a guessed identity would mistype downstream folds.
    // Contrast: sum over an empty ARRAY (`sum(5:3)`) stays the typed identity, because an
    // array value carries its element type even when empty.
    assert_eq!(
        run("sum(i for i in 1:0)").unwrap_err(),
        ExprError::EmptyArray
    );
    assert_eq!(
        run("sum(i*0.5 for i in 5:3)").unwrap_err(),
        ExprError::EmptyArray
    );
    assert!(scalar("sum(5:3)").bit_eq(&Value::Integer(0)));
}

// --- Multi-iterator: parses, defers at evaluation -------------------------------------------

#[test]
fn multi_iterator_comprehensions_parse_then_defer_at_evaluation() {
    let deferral = "a comprehension binds exactly one iterator \
                    (multi-iterator comprehensions are deferred)";
    assert!(parse("{i+j for i in 1:2, j in 1:2}").is_ok());
    assert_eq!(
        run("{i+j for i in 1:2, j in 1:2}").unwrap_err(),
        ExprError::DomainError(deferral)
    );
    // The clause-count gate runs before any source evaluates: unbound sources still get the
    // deferral message, not UnknownIdent.
    assert_eq!(
        run("{i+j for i in a, j in b}").unwrap_err(),
        ExprError::DomainError(deferral)
    );
    // The sum sugar parses the same multi-clause surface and defers identically.
    assert!(parse("sum(i+j for i in 1:2, j in 1:2)").is_ok());
    assert_eq!(
        run("sum(i+j for i in 1:2, j in 1:2)").unwrap_err(),
        ExprError::DomainError(deferral)
    );
}

// --- `for` stays a plain identifier outside comprehensions ----------------------------------

#[test]
fn for_stays_a_plain_identifier_outside_comprehensions() {
    // The lexer emits `for` as an ordinary Name and the parser only treats it as a keyword
    // after a brace element or a sum argument — everywhere else it is an Ident, exactly as
    // before this grammar landed.
    let scope = TestScope::new(&[("for", Value::Integer(1))]);
    assert!(scalar_in("for + 1", &scope).bit_eq(&Value::Integer(2)));
    assert!(scalar_in("abs(for)", &scope).bit_eq(&Value::Integer(1)));
    assert_integer_elements(&array_in("{for}", &scope), &[1]);
    // Unbound, it fails at evaluation like any identifier — never at parse.
    assert_eq!(
        run("for").unwrap_err(),
        ExprError::UnknownIdent("for".to_string())
    );
    // `in` is contextual the same way.
    let scope = TestScope::new(&[("in", Value::Integer(4))]);
    assert!(scalar_in("in * 2", &scope).bit_eq(&Value::Integer(8)));
}

// --- Malformed comprehensions: typed parse errors, never a panic ----------------------------

#[test]
fn malformed_comprehensions_are_typed_parse_errors_never_panics() {
    let cases = [
        "{for i in 1:3}",       // no body: `for` parses as an Ident, the clause then misfits
        "{i for}",              // no iterator clause
        "{i for i}",            // missing 'in'
        "{i for i in}",         // missing iteration source
        "{i for 5 in 1:3}",     // non-identifier iterator
        "{i for true in 1:3}",  // keyword-literal iterator
        "{i for Foo.b in 1:3}", // qualified-name iterator
        "{i for pi in 1:3}",    // named-constant iterator (the body would read the constant)
        "{i for in in 1:3}",    // contextual keyword as iterator name
        "{i for i in 1:3",      // unterminated comprehension
        "{i for i in 1:3,",     // dangling clause comma
        "{i for i in 1:3, j}",  // second clause missing 'in'
        "{i for i in 1:3 j in 1:2}", // missing clause comma
        "sum(i for i in 1:3",   // unterminated sugar
        "sum(i for)",           // sugar with no clause
        "sum(1, i for i in 1:3)", // sugar in a non-first argument
    ];
    for s in cases {
        assert!(
            matches!(run(s), Err(ExprError::Parse(_))),
            "{s:?} should be a parse error, got {:?}",
            run(s)
        );
    }
}

#[test]
fn mixed_elements_and_for_clause_are_a_parse_error() {
    let Err(ExprError::Parse(msg)) = parse("{1, 2 for i in 1:3}") else {
        panic!("{{1, 2 for i in 1:3}} should be a parse error");
    };
    assert!(
        msg.contains("single body"),
        "message should name the single-body rule: {msg}"
    );
    assert!(matches!(
        parse("{1, 2, 3 for i in 1:3}"),
        Err(ExprError::Parse(_))
    ));
}

#[test]
fn reduction_sugar_outside_sum_is_a_parse_error_naming_sum() {
    for s in [
        "min(i for i in 1:3)",
        "max(i for i in 1:3)",
        "abs(i for i in 1:3)",
        "fill(x for x in 1:2, 3)",
    ] {
        let Err(ExprError::Parse(msg)) = parse(s) else {
            panic!("{s:?} should be a parse error");
        };
        assert!(
            msg.contains("'sum'"),
            "{s:?} should name the sum-only support: {msg}"
        );
    }
    // An unknown function with a for-argument reports the sugar scope, not the R9 rejection —
    // the `for` is structural, so the argument list never finishes parsing.
    assert!(matches!(
        parse("product(i for i in 1:3)"),
        Err(ExprError::Parse(_))
    ));
}

// --- Type gates -----------------------------------------------------------------------------

#[test]
fn scalar_iteration_sources_are_type_errors() {
    assert_eq!(
        run("{i for i in 5}").unwrap_err(),
        ExprError::TypeError {
            expected: "an array iteration source",
            found: "Integer",
        }
    );
    assert_eq!(
        run("{i for i in 1.5}").unwrap_err(),
        ExprError::TypeError {
            expected: "an array iteration source",
            found: "Real",
        }
    );
    assert!(matches!(
        run("{i for i in true}"),
        Err(ExprError::TypeError { .. })
    ));
    assert!(matches!(
        run("{i for i in \"a\"}"),
        Err(ExprError::TypeError { .. })
    ));
}

#[test]
fn array_valued_bodies_are_type_errors_naming_the_deferral() {
    let expected = ExprError::TypeError {
        expected: "a scalar comprehension body (nested/2-D comprehensions are deferred)",
        found: "array",
    };
    assert_eq!(run("{1:3 for i in 1:2}").unwrap_err(), expected);
    assert_eq!(run("{fill(1, 2) for i in 1:2}").unwrap_err(), expected);
    assert_eq!(run("{{1, 2} for i in 1:2}").unwrap_err(), expected);
    // A nested comprehension BODY is the same rejection.
    assert_eq!(
        run("{{j for j in 1:2} for i in 1:2}").unwrap_err(),
        expected
    );
}

#[test]
fn string_and_enum_bodies_are_type_errors() {
    // Homogeneous-but-illegal element types flow through the shared literal promotion and
    // are rejected there, same as `{"a", "b"}`.
    assert!(matches!(
        run("{\"a\" for i in 1:2}"),
        Err(ExprError::TypeError { .. })
    ));
    let scope = TestScope::new(&[]);
    assert!(matches!(
        eval_str(
            "{Buildings.Controls.OBC.CDL.Types.ZeroTime.NY2017 for i in 1:2}",
            &scope
        ),
        Err(ExprError::TypeError { .. })
    ));
}

#[test]
fn mixed_scalar_elements_are_type_errors_at_the_shared_promotion_seam() {
    // A grammar-reachable comprehension body cannot produce mixed element types: the body is
    // one fixed expression and the source is homogeneous (an ArrayValue invariant), so every
    // iteration yields the same result type. The mixed-type gate therefore lives in — and is
    // pinned at — the shared collection seam both literals and comprehensions build through.
    assert!(matches!(
        eval_array::array_from_scalars(vec![Value::Integer(1), Value::Boolean(true)]),
        Err(ExprError::TypeError { .. })
    ));
    assert!(matches!(
        eval_array::array_from_scalars(vec![Value::Real(1.0), Value::Boolean(true)]),
        Err(ExprError::TypeError { .. })
    ));
    // And the belt-and-braces empty gate (both callers pre-reject the empty case).
    assert_eq!(
        eval_array::array_from_scalars(Vec::new()).unwrap_err(),
        ExprError::EmptyArray
    );
}

// --- Body errors propagate mid-iteration ----------------------------------------------------

#[test]
fn body_errors_halt_iteration_with_the_typed_error_never_a_partial_array() {
    // A body raising its own typed error propagates cleanly out of the comprehension —
    // never a panic, never a truncated partial array — halting at the first failing element
    // in source order. Three probe shapes cover the failure position across the iteration:
    //
    // First element: sqrt(1 - 3) fails before any element is collected.
    assert_eq!(
        run("{sqrt(i - 3) for i in 1:3}").unwrap_err(),
        ExprError::DomainError("sqrt of a negative value")
    );
    // Mid-iteration: div(1, i - 2) succeeds at i = 1 and fails at i = 2 — one element was
    // already collected, and the error still surfaces instead of a shortened array.
    assert_eq!(
        run("{div(1, i - 2) for i in 1:3}").unwrap_err(),
        ExprError::DivisionByZero
    );
    // Last element: a[3] over a 2-element array fails only after every earlier element
    // succeeded, exact to the diagnostic fields — the fullest possible partial result is
    // still discarded in favor of the typed error.
    let scope = TestScope::new(&[]).bind_array("a", array("{10, 20}"));
    assert_eq!(
        eval_str("{a[i] for i in 1:3}", &scope).unwrap_err(),
        ExprError::IndexOutOfBounds { index: 3, size: 2 }
    );
    // The sum sugar propagates the same mid-iteration error — no identity fallback.
    assert_eq!(
        run("sum(div(1, i - 2) for i in 1:3)").unwrap_err(),
        ExprError::DivisionByZero
    );
}

// --- The element cap guards the iteration source --------------------------------------------

#[test]
fn the_cap_rejects_oversized_sources_before_the_comprehension_allocates() {
    // The comprehension itself needs no cap site: the result length equals the source
    // length, and an oversized source is already rejected — with the exact count — at range
    // construction, BEFORE the comprehension allocates or evaluates anything.
    assert_eq!(
        run("{i for i in 1:2000000}").unwrap_err(),
        ExprError::ArrayTooLarge {
            count: 2_000_000,
            max: 1 << 20,
        }
    );
    assert_eq!(
        run("sum(i for i in 1:2000000)").unwrap_err(),
        ExprError::ArrayTooLarge {
            count: 2_000_000,
            max: 1 << 20,
        }
    );
    // Exactly at the cap is legal end to end: source, per-element body, and result.
    assert_eq!(array("{0 for i in 1:1048576}").len(), 1 << 20);
}

// --- Composition ----------------------------------------------------------------------------

#[test]
fn comprehensions_compose_with_indexing_and_array_builtins() {
    assert!(scalar("{i*2 for i in 1:3}[2]").bit_eq(&Value::Integer(4)));
    assert!(scalar("size({i for i in 1:3}, 1)").bit_eq(&Value::Integer(3)));
    assert!(scalar("min({i*2 for i in 1:3})").bit_eq(&Value::Integer(2)));
    assert_integer_elements(&array("cat(1, {i for i in 1:2}, {9})"), &[1, 2, 9]);
    // A comprehension result is an ordinary array value: an identifier-free scalar context
    // still rejects it like any array.
    assert!(matches!(
        run("{i for i in 1:2} + 1"),
        Err(ExprError::TypeError { .. })
    ));
}

// --- Determinism goldens --------------------------------------------------------------------

#[test]
fn repeated_comprehension_evaluation_is_bit_identical() {
    let first = array("{i*0.1 for i in 1:4}");
    let second = array("{i*0.1 for i in 1:4}");
    assert_eq!(first.len(), second.len());
    for (a, b) in first.as_slice().iter().zip(second.as_slice()) {
        assert!(a.bit_eq(b), "{a:?} != {b:?}");
    }
    let first = scalar("sum(i*0.1 for i in 1:3)");
    let second = scalar("sum(i*0.1 for i in 1:3)");
    assert!(first.bit_eq(&second), "{first:?} != {second:?}");
}
