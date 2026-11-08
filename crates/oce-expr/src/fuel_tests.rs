//! Deterministic work-budget tests (`fuel.rs`): exact costs, the budget boundary, hostile
//! quadratic shapes, nesting, and the idle state after a refusal.

use oce_model::Value;

use crate::fuel::{MAX_EVAL_FUEL, metered_with, remaining};
use crate::{EvalResult, ExprError, Scope, eval_str, parse, with_eval_budget};

struct NoScope;

impl Scope for NoScope {
    fn lookup(&self, _: &str) -> Option<&EvalResult> {
        None
    }
}

/// Evaluate `text` on `budget` units, returning the result and the units it spent.
fn run(text: &str, budget: u64) -> (Result<EvalResult, ExprError>, u64) {
    let ast = parse(text).expect("test expression parses");
    metered_with(budget, || {
        let result = crate::eval::eval(&ast, &NoScope);
        let left = remaining().expect("inside a metered evaluation");
        (result, budget - left)
    })
}

/// The exact cost of `text` on the full budget (it must succeed).
fn cost(text: &str) -> u64 {
    let (result, spent) = run(text, MAX_EVAL_FUEL);
    result.unwrap_or_else(|e| panic!("{text} must evaluate: {e}"));
    spent
}

#[test]
fn costs_count_nodes_and_array_elements_exactly() {
    // Golden costs: one unit per AST node evaluated plus one per array element constructed,
    // read or reduced. Pinned so a silent change to the accounting is a reviewed diff.
    assert_eq!(cost("1"), 1);
    assert_eq!(cost("1 + 2 * 3"), 5);
    assert_eq!(cost("{1, 2, 3}"), 1 + 3 + 3);
    assert_eq!(cost("sum({1, 2, 3})"), 1 + (1 + 3 + 3) + 3);
    assert_eq!(cost("sum(1:1000)"), cost("sum(1:1)") + 2 * 999);
    assert_eq!(
        cost("fill(0, 4096)"),
        cost("fill(0, 1)") + 4095,
        "fill charges one unit per element constructed"
    );
}

#[test]
fn costs_are_identical_on_repeated_evaluation() {
    let text = "sum({i * 2 for i in 1:5000}) + max(fill(1.5, 300))";
    let first = cost(text);
    for _ in 0..8 {
        assert_eq!(cost(text), first);
    }
}

#[test]
fn budget_boundary_admits_the_exact_cost_and_refuses_one_unit_less() {
    let text = "sum({sum(1:64) for i in 1:64})";
    let exact = cost(text);
    let (at_limit, spent) = run(text, exact);
    assert!(at_limit.is_ok(), "exactly the cost is accepted");
    assert_eq!(spent, exact);
    let (below, _) = run(text, exact - 1);
    assert_eq!(
        below.unwrap_err(),
        ExprError::ExpressionTooExpensive { limit: exact - 1 },
        "the refusal names the budget that actually ran out"
    );
}

#[test]
fn quadratic_comprehension_is_refused_with_the_typed_error() {
    // The issue #365 shape: 2^14 × 2^14 element work from a 39-byte binding (~2^29 units).
    let err = eval_str("sum({sum(1:16384) for i in 1:16384})", &NoScope).unwrap_err();
    assert_eq!(
        err,
        ExprError::ExpressionTooExpensive {
            limit: MAX_EVAL_FUEL
        }
    );
    // The 2^20 variant (once extrapolated to ~23 minutes) refuses the same way, and the work
    // done before refusing is bounded by the budget itself: the refusal is reached after at
    // most MAX_EVAL_FUEL charged units.
    let err = eval_str("sum({sum(1:1048576) for i in 1:1048576})", &NoScope).unwrap_err();
    assert!(matches!(err, ExprError::ExpressionTooExpensive { .. }));
    assert_eq!(
        err.to_string(),
        "expression too expensive: evaluation exceeds the supported work budget (16777216 units)"
    );
}

#[test]
fn largest_single_array_construct_fits_with_headroom() {
    // A maximal 2^20-element array built and reduced is admissible.
    let spent = cost("sum(fill(1, 1048576))");
    assert!(spent < MAX_EVAL_FUEL / 4, "spent {spent}");
    let (result, _) = run("sum(fill(1, 1048576))", MAX_EVAL_FUEL);
    assert!(matches!(
        result.unwrap(),
        EvalResult::Scalar(Value::Integer(1_048_576))
    ));
}

#[test]
fn each_public_evaluation_gets_a_fresh_budget_and_leaves_no_meter_behind() {
    // Repeated public evaluations each just under a third of the budget all succeed: the
    // budget is per evaluation, not cumulative across calls on the thread.
    for _ in 0..4 {
        assert!(eval_str("sum({sum(1:2048) for i in 1:1300})", &NoScope).is_ok());
    }
    assert_eq!(
        remaining(),
        None,
        "no metered evaluation is active between calls"
    );
    // A refusal also leaves the thread idle, so the next evaluation starts full.
    assert!(eval_str("sum({sum(1:16384) for i in 1:16384})", &NoScope).is_err());
    assert_eq!(remaining(), None);
    assert!(eval_str("1 + 1", &NoScope).is_ok());
}

/// A scope whose lookup itself runs a nested public evaluation and counts its successes.
struct EvaluatingScope {
    value: EvalResult,
    nested_ok: std::cell::Cell<u32>,
}

impl Scope for EvaluatingScope {
    fn lookup(&self, _: &str) -> Option<&EvalResult> {
        if eval_str("sum(1:100000)", &NoScope).is_ok() {
            self.nested_ok.set(self.nested_ok.get() + 1);
        }
        Some(&self.value)
    }
}

#[test]
fn nested_evaluation_draws_on_the_enclosing_budget() {
    // Each nested `sum(1:100000)` costs about 2e5 units. With fresh nested budgets all 200
    // would succeed; drawing on the one enclosing budget, only the first ~83 fit.
    let nested_cost = cost("sum(1:100000)");
    let scope = EvaluatingScope {
        value: EvalResult::Scalar(Value::Integer(1)),
        nested_ok: std::cell::Cell::new(0),
    };
    let _ = eval_str("sum({x for i in 1:200})", &scope);
    let ok = u64::from(scope.nested_ok.get());
    assert!(ok > 0 && ok < 200, "nested successes {ok}");
    assert!(ok <= MAX_EVAL_FUEL / nested_cost, "nested successes {ok}");
    assert_eq!(remaining(), None);
}

/// A scope whose lookup panics mid-evaluation.
struct PanickingScope;

impl Scope for PanickingScope {
    fn lookup(&self, _: &str) -> Option<&EvalResult> {
        panic!("scope failure during evaluation")
    }
}

#[test]
fn a_panicking_scope_unwinds_the_meter_back_to_idle() {
    let unwound = std::panic::catch_unwind(|| eval_str("sum({x for i in 1:10})", &PanickingScope));
    assert!(unwound.is_err(), "the scope panic propagates");
    assert_eq!(
        remaining(),
        None,
        "the frame guard reset the meter on unwind"
    );
    // Inside a shared budget the unwind restores the enclosing frame minus what was spent.
    let left = with_eval_budget(1_000, || {
        let unwound = std::panic::catch_unwind(|| eval_str("1 + x", &PanickingScope));
        assert!(unwound.is_err());
        remaining()
    });
    assert_eq!(
        left,
        Some(1_000 - 3),
        "the Binary, the literal and the identifier were charged before the lookup panicked"
    );
    assert_eq!(remaining(), None);
    assert!(
        eval_str("1 + 1", &NoScope).is_ok(),
        "the next evaluation meters normally"
    );
}

#[test]
fn a_shared_budget_bounds_the_total_work_of_many_evaluations() {
    let one = cost("sum(1:1000)");
    // Exactly five evaluations fit a budget of five costs; the sixth refuses at once, naming the
    // shared budget, and every later one refuses at its first unit.
    let outcomes = with_eval_budget(5 * one, || {
        (0..8)
            .map(|_| eval_str("sum(1:1000)", &NoScope).map(|_| ()))
            .collect::<Vec<_>>()
    });
    assert!(outcomes[..5].iter().all(Result::is_ok), "{outcomes:?}");
    for refused in &outcomes[5..] {
        assert_eq!(
            refused,
            &Err(ExprError::ExpressionTooExpensive { limit: 5 * one })
        );
    }
    // Under a generous shared budget a single evaluation is still capped per evaluation.
    let err = with_eval_budget(u64::MAX, || {
        eval_str("sum({sum(1:16384) for i in 1:16384})", &NoScope).unwrap_err()
    });
    assert_eq!(
        err,
        ExprError::ExpressionTooExpensive {
            limit: MAX_EVAL_FUEL
        }
    );
    assert_eq!(remaining(), None);
}

#[test]
fn the_document_budget_boundary_binding_costs_exactly_two_to_the_twenty_one() {
    // `oce-cxf`'s document-budget boundary test fills its 2^26-unit budget with exactly 32 of
    // these bindings: four nodes plus 2^20 - 2 elements built and reduced.
    assert_eq!(cost("sum(fill(1, 1048574))"), 1 << 21);
}
