//! Parser resource-boundary and ingest-totality tests.

use oce_model::Value;

use crate::fuel::{MAX_EVAL_FUEL, metered_with, remaining};
use crate::{
    BinOp, EvalResult, ExprAst, ExprError, MAX_EXPR_NODES, MAX_NESTING_DEPTH, Scope, eval,
    eval_finite, nesting_depth, parse,
};

const MAX_PAREN_DELIMITERS: usize = 31;
const MAX_BRACE_DELIMITERS: usize = 31;
const MAX_CALL_DELIMITERS: usize = 31;
const MAX_UNARY_SIGNS: usize = 62;
const MAX_INDEX_DELIMITERS: usize = 63;

struct EmptyScope;

impl Scope for EmptyScope {
    fn lookup(&self, _: &str) -> Option<&EvalResult> {
        None
    }
}

fn nested(open: &str, close: &str, count: usize) -> String {
    format!("{}1{}", open.repeat(count), close.repeat(count))
}

fn assert_depth_rejection(text: &str) {
    assert_eq!(
        parse(text).unwrap_err(),
        ExprError::NestingTooDeep {
            limit: MAX_NESTING_DEPTH
        }
    );
}

#[test]
fn parenthesis_parse_entries_accept_the_boundary_and_reject_one_past() {
    // Each delimiter adds two guarded entries (parse_range + parse_unary); the outer expression
    // consumes two too, so 31 delimiters consume all 64 allowed entries.
    parse(&nested("(", ")", MAX_PAREN_DELIMITERS)).unwrap();
    assert_depth_rejection(&nested("(", ")", MAX_PAREN_DELIMITERS + 1));
}

#[test]
fn brace_parse_entries_accept_the_boundary_and_reject_one_past() {
    // Like parentheses, each brace level adds two guarded entries while AST depth grows by one.
    parse(&nested("{", "}", MAX_BRACE_DELIMITERS)).unwrap();
    assert_depth_rejection(&nested("{", "}", MAX_BRACE_DELIMITERS + 1));
}

#[test]
fn call_parse_entries_accept_the_boundary_and_reject_one_past() {
    // Each arity-one call adds two guarded entries while AST depth grows by one.
    parse(&nested("abs(", ")", MAX_CALL_DELIMITERS)).unwrap();
    assert_depth_rejection(&nested("abs(", ")", MAX_CALL_DELIMITERS + 1));
}

#[test]
fn unary_parse_entries_accept_the_boundary_and_reject_one_past() {
    // A sign adds one guarded parse_unary entry and one AST node; parse_range and the final
    // operand's parse_unary consume two more entries, so 62 signs consume all 64 allowed entries
    // while producing AST depth 63.
    parse(&format!("{}1", "- ".repeat(MAX_UNARY_SIGNS))).unwrap();
    assert_depth_rejection(&format!("{}1", "- ".repeat(MAX_UNARY_SIGNS + 1)));
}

#[test]
fn postfix_index_depth_accepts_the_boundary_and_rejects_one_past() {
    // Postfix parsing is iterative and adds no guarded entries per subscript. The post-parse AST
    // depth check owns this boundary: identifier depth 1 plus 63 Index nodes reaches depth 64.
    parse(&format!("A{}", "[1]".repeat(MAX_INDEX_DELIMITERS))).unwrap();
    assert_depth_rejection(&format!("A{}", "[1]".repeat(MAX_INDEX_DELIMITERS + 1)));
}

#[test]
fn large_shallow_array_remains_accepted() {
    let text = format!(
        "{{{}}}",
        (0..1000)
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
            .join(",")
    );
    let ast = parse(&text).unwrap();
    assert!(matches!(eval(&ast, &EmptyScope), Ok(EvalResult::Array(_))));
}

#[test]
fn large_left_leaning_expression_is_rejected_during_construction() {
    let text = std::iter::repeat_n("1", 30_000)
        .collect::<Vec<_>>()
        .join("+");
    let error = std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(move || parse(&text).unwrap_err())
        .unwrap()
        .join()
        .unwrap();
    assert_eq!(
        error,
        ExprError::ExpressionTooLarge {
            limit: MAX_EXPR_NODES
        }
    );
}

#[test]
fn deeply_parenthesized_input_returns_a_typed_error_on_a_small_stack() {
    let text = nested("(", ")", 5000);
    let error = std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(move || parse(&text).unwrap_err())
        .unwrap()
        .join()
        .unwrap();
    assert_eq!(
        error,
        ExprError::NestingTooDeep {
            limit: MAX_NESTING_DEPTH
        }
    );
}

// --- Long left-associative chains (issue #371) ----------------------------------------------

/// The most terms one chain can hold: `n` leaves and `n - 1` operators is `2n - 1` nodes.
const MAX_CHAIN_TERMS: usize = MAX_EXPR_NODES.div_ceil(2);

fn chain(term: &str, op: &str, terms: usize) -> String {
    std::iter::repeat_n(term, terms)
        .collect::<Vec<_>>()
        .join(op)
}

/// Run `f` on a 2 MiB thread — the smallest stack the depth limits are sized for.
fn on_small_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap()
}

#[track_caller]
fn assert_value(actual: Value, expected: Value) {
    assert!(actual.bit_eq(&expected), "{actual:?} != {expected:?}");
}

fn scalar(text: &str) -> Value {
    match eval(&parse(text).unwrap(), &EmptyScope) {
        Ok(EvalResult::Scalar(value)) => value,
        other => panic!("{text}: expected a scalar, got {other:?}"),
    }
}

/// Evaluate `text` on the full per-evaluation budget, returning the result and the units spent.
/// Unwrap a scalar evaluation result.
fn scalar_of(result: Result<EvalResult, ExprError>) -> Value {
    match result {
        Ok(EvalResult::Scalar(value)) => value,
        other => panic!("expected a scalar, got {other:?}"),
    }
}

fn metered(text: &str) -> (Result<EvalResult, ExprError>, u64) {
    let ast = parse(text).unwrap();
    metered_with(MAX_EVAL_FUEL, || {
        let result = crate::eval::eval(&ast, &EmptyScope);
        (result, MAX_EVAL_FUEL - remaining().unwrap())
    })
}

#[test]
fn a_chain_longer_than_the_nesting_limit_parses_and_evaluates() {
    for terms in [MAX_NESTING_DEPTH, MAX_NESTING_DEPTH + 1, 100, 1000] {
        let ast = parse(&chain("1", "+", terms)).unwrap();
        assert_eq!(nesting_depth(&ast), 2, "{terms} terms");
        assert_value(
            scalar(&chain("1", "+", terms)),
            Value::Integer(i64::try_from(terms).unwrap()),
        );
    }
}

#[test]
fn every_chaining_operator_keeps_left_associative_semantics() {
    // Left association is observable for `-` and `/`: 1-1-…-1 (100 terms) is 1-99.
    assert_value(scalar(&chain("1", "-", 100)), Value::Integer(-98));
    assert_value(scalar(&chain("2", "*", 62)), Value::Integer(1 << 62));
    let Value::Real(halved) = scalar(&format!("1.0/{}", chain("2", "/", 70))) else {
        panic!("Real");
    };
    // 1/2/2/… is 2^-70 exactly: (1/2)/2 associates left; 1/(2/(2/…)) would be 1/2 or 1.
    assert_eq!(halved.to_bits(), 2f64.powi(-70).to_bits());
    assert_value(scalar(&chain("true", " and ", 200)), Value::Boolean(true));
    assert_value(
        scalar(&format!("{} and false", chain("true", " and ", 200))),
        Value::Boolean(false),
    );
    assert_value(
        scalar(&format!("{} or true", chain("false", " or ", 200))),
        Value::Boolean(true),
    );
    // A mixed additive chain: 10 + 1 - 2 + 1 - 2 … (99 operator pairs).
    let mixed = format!("10{}", " + 1 - 2".repeat(99));
    assert_value(scalar(&mixed), Value::Integer(10 - 99));
}

#[test]
fn the_longest_chain_the_node_cap_admits_is_accepted_and_one_more_term_is_refused() {
    let longest = chain("1", "+", MAX_CHAIN_TERMS);
    let over = chain("1", "+", MAX_CHAIN_TERMS + 1);
    let (value, depth, rendered, refusal) = on_small_stack(move || {
        // Parse, evaluate, clone, render and drop the maximal tree on the smallest stack.
        let ast = parse(&longest).unwrap();
        let copy = ast.clone();
        let rendered = format!("{copy:?}").len();
        let value = eval(&ast, &EmptyScope).unwrap();
        (
            value,
            nesting_depth(&ast),
            rendered,
            parse(&over).unwrap_err(),
        )
    });
    assert_value(
        scalar_of(Ok(value)),
        Value::Integer(i64::try_from(MAX_CHAIN_TERMS).unwrap()),
    );
    assert_eq!(depth, 2);
    assert!(rendered > MAX_CHAIN_TERMS);
    assert_eq!(
        refusal,
        ExprError::ExpressionTooLarge {
            limit: MAX_EXPR_NODES
        }
    );
}

#[test]
fn a_hundred_thousand_term_chain_is_refused_by_size_without_exhausting_the_stack() {
    let text = chain("x", "+", 100_000);
    let error = on_small_stack(move || parse(&text).unwrap_err());
    assert_eq!(
        error,
        ExprError::ExpressionTooLarge {
            limit: MAX_EXPR_NODES
        }
    );
}

/// A hand-built left-deep `1+1+…` chain of `terms` terms (no parser node cap applies).
fn hand_built_chain(terms: usize) -> ExprAst {
    let mut ast = ExprAst::Int(1);
    for _ in 1..terms {
        ast = ExprAst::Binary(BinOp::Add, Box::new(ast), Box::new(ExprAst::Int(1)));
    }
    ast
}

/// Take a left-deep chain apart iteratively: the derived `Drop` recurses, and a hand-built tree
/// this deep is the host's to dispose of (see [`ExprAst`]).
fn dismantle(mut ast: ExprAst) {
    while let ExprAst::Binary(_, left, _) = ast {
        ast = *left;
    }
}

#[test]
fn a_hundred_thousand_term_hand_built_chain_evaluates_iteratively_on_a_small_stack() {
    let (value, depth) = on_small_stack(|| {
        let ast = hand_built_chain(100_000);
        let result = (eval(&ast, &EmptyScope), nesting_depth(&ast));
        dismantle(ast);
        result
    });
    assert_value(scalar_of(value), Value::Integer(100_000));
    assert_eq!(depth, 2);
}

#[test]
fn true_nesting_still_counts_against_the_limit() {
    // Right-nested chains are real nesting: each operand is a subtree one level deeper.
    let mut right_deep = ExprAst::Int(1);
    for _ in 0..100 {
        right_deep = ExprAst::Binary(BinOp::Add, Box::new(ExprAst::Int(1)), Box::new(right_deep));
    }
    assert_eq!(nesting_depth(&right_deep), 101);
    assert_eq!(
        eval(&right_deep, &EmptyScope).unwrap_err(),
        ExprError::NestingTooDeep {
            limit: MAX_NESTING_DEPTH
        }
    );
    // In text, right nesting needs parentheses, and the parser's own guard bounds them.
    let text = format!("{}1{}", "1+(".repeat(40), ")".repeat(40));
    assert_depth_rejection(&text);
    // A long chain inside each of the deepest admitted parentheses is still fine.
    let inner = chain("1", "+", 500);
    let text = format!(
        "{}{inner}{}",
        "(".repeat(MAX_PAREN_DELIMITERS),
        ")".repeat(MAX_PAREN_DELIMITERS)
    );
    assert_value(scalar(&text), Value::Integer(500));
    // Mixed precedence: a product chain as every right operand of a sum chain.
    let text = chain(&chain("2", "*", 3), "+", 300);
    let ast = parse(&text).unwrap();
    assert_eq!(nesting_depth(&ast), 3);
    assert_value(scalar(&text), Value::Integer(8 * 300));
}

#[test]
fn chain_evaluation_spends_the_same_fuel_as_before() {
    // One unit per node: n terms are n leaves plus n - 1 operators.
    for terms in [1_usize, 2, 3, 64, 65, 1000, MAX_CHAIN_TERMS] {
        let (result, spent) = metered(&chain("1", "+", terms));
        result.unwrap();
        assert_eq!(
            spent,
            u64::try_from(2 * terms - 1).unwrap(),
            "{terms} terms"
        );
    }
    // A shared budget one unit short refuses; exactly enough succeeds.
    let text = chain("1", "+", 1000);
    let ast = parse(&text).unwrap();
    assert!(crate::with_eval_budget(1999, || eval(&ast, &EmptyScope)).is_ok());
    assert_eq!(
        crate::with_eval_budget(1998, || eval(&ast, &EmptyScope)).unwrap_err(),
        ExprError::ExpressionTooExpensive { limit: 1998 }
    );
}

#[test]
fn chain_errors_surface_in_source_order() {
    struct Undefined;
    impl Scope for Undefined {
        fn lookup(&self, _: &str) -> Option<&EvalResult> {
            None
        }
    }
    let run = |text: &str| eval(&parse(text).unwrap(), &Undefined).unwrap_err();
    // The leftmost failure wins, exactly as the recursive walk reported it.
    assert_eq!(
        run(&format!("true + 1{}+ nosuch", " + 1".repeat(100))),
        ExprError::TypeError {
            expected: "Real or Integer",
            found: "Boolean"
        }
    );
    assert_eq!(
        run(&format!("1{} + nosuch + true", " + 1".repeat(100))),
        ExprError::UnknownIdent("nosuch".into())
    );
    assert!(matches!(
        run(&format!("9223372036854775807{}", " + 1".repeat(100))),
        ExprError::DomainError(_)
    ));
}

#[test]
fn strict_evaluation_refuses_a_non_finite_intermediate_inside_a_chain() {
    // (1e308 * 10) is +inf at an inner spine node; the IEEE result of the whole chain is NaN.
    let text = format!("1e308 * 10 * 0{}", " * 1".repeat(100));
    let ast = parse(&text).unwrap();
    assert_eq!(
        eval_finite(&ast, &EmptyScope).unwrap_err(),
        ExprError::NonFinite { value: "inf" }
    );
    let Ok(EvalResult::Scalar(Value::Real(lenient))) = eval(&ast, &EmptyScope) else {
        panic!("lenient evaluation keeps IEEE propagation");
    };
    assert!(lenient.is_nan());
}

#[test]
fn chain_evaluation_is_deterministic() {
    let text = format!("0.1{}", " + 0.2 - 0.05 * 3 / 7".repeat(400));
    let first = scalar(&text);
    for _ in 0..4 {
        assert!(scalar(&text).bit_eq(&first));
    }
    // The bits are those of the explicit left fold.
    let mut fold = 0.1_f64;
    for _ in 0..400 {
        fold = fold + 0.2 - 0.05 * 3.0 / 7.0;
    }
    assert!(first.bit_eq(&Value::Real(fold)));
}

// --- The i64::MIN literal (issue #371) ----------------------------------------------------

#[test]
fn minus_two_to_the_sixty_three_is_the_integer_minimum() {
    for text in [
        "-9223372036854775808",
        "- 9223372036854775808",
        "-0009223372036854775808",
        "(-9223372036854775808)",
    ] {
        assert!(scalar(text).bit_eq(&Value::Integer(i64::MIN)), "{text}");
    }
    assert!(matches!(
        parse("-9223372036854775808").unwrap(),
        ExprAst::Int(i64::MIN)
    ));
    assert_value(
        scalar("-9223372036854775808 + 1"),
        Value::Integer(i64::MIN + 1),
    );
    assert_value(scalar("-9223372036854775808 * 1"), Value::Integer(i64::MIN));
    assert_value(
        scalar("{-9223372036854775808, 0}[1]"),
        Value::Integer(i64::MIN),
    );
    assert_value(
        scalar("-9223372036854775807 - 1 == -9223372036854775808"),
        Value::Boolean(true),
    );
    assert_value(
        scalar("-9223372036854775808 < -9223372036854775807"),
        Value::Boolean(true),
    );
    // A Real spelling was always legal and stays a Real.
    assert_value(
        scalar("-9223372036854775808.0"),
        Value::Real(-9.223_372_036_854_776e18),
    );
}

#[test]
fn two_to_the_sixty_three_is_refused_in_every_other_position() {
    let out_of_range =
        |text: &str| ExprError::Parse(format!("integer literal out of range {text:?}"));
    for (text, literal) in [
        ("9223372036854775808", "9223372036854775808"),
        ("+9223372036854775808", "9223372036854775808"),
        ("0 - 9223372036854775808", "9223372036854775808"),
        ("2 * 9223372036854775808", "9223372036854775808"),
        ("-+9223372036854775808", "9223372036854775808"),
        ("-9223372036854775808[1]", "9223372036854775808"),
        ("-09223372036854775808[1]", "09223372036854775808"),
        ("{9223372036854775808}", "9223372036854775808"),
        ("-9223372036854775809", "9223372036854775809"),
        ("-18446744073709551616", "18446744073709551616"),
    ] {
        assert_eq!(parse(text).unwrap_err(), out_of_range(literal), "{text}");
    }
    // Negating the minimum again, or stepping past it, overflows at evaluation.
    for text in [
        "--9223372036854775808",
        "-(-9223372036854775808)",
        "-9223372036854775808 - 1",
        "2 * -9223372036854775808",
        "-9223372036854775808 * -1",
    ] {
        assert!(
            matches!(
                eval(&parse(text).unwrap(), &EmptyScope),
                Err(ExprError::DomainError(_))
            ),
            "{text}"
        );
    }
}

#[test]
fn the_folded_minimum_costs_one_node() {
    let (result, spent) = metered("-9223372036854775808");
    assert_value(scalar_of(result), Value::Integer(i64::MIN));
    assert_eq!(spent, 1);
    let (_, spent) = metered("-9223372036854775807");
    assert_eq!(
        spent, 2,
        "an ordinary negative literal is a sign plus a literal"
    );
}
