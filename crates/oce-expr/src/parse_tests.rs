//! Parser resource-boundary and ingest-totality tests.

use crate::{EvalResult, ExprError, MAX_EXPR_NODES, MAX_NESTING_DEPTH, Scope, eval, parse};

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
fn left_leaning_depth_is_rejected_after_parse() {
    let text = std::iter::repeat_n("1", 100).collect::<Vec<_>>().join("+");
    assert_depth_rejection(&text);
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
