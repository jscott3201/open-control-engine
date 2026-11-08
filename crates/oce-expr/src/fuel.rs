//! Deterministic evaluation work budget ("fuel").
//!
//! Parse-time caps bound an expression's *size* ([`crate::MAX_EXPR_NODES`]) and every array's
//! *length* (2^20), but not the *work* an evaluation does: a 40-byte binding such as
//! `sum({sum(1:1048576) for i in 1:1048576})` would build and fold 2^40 elements. Work is
//! therefore metered in units charged for operations — never wall time, so the outcome is
//! identical on every host and every run:
//!
//! - one unit per AST node evaluated (comprehension bodies are charged once per iteration);
//! - one unit per element of every array constructed, read from a scope, or reduced.
//!
//! Exhausting a budget is the typed [`ExprError::ExpressionTooExpensive`], returned before the
//! charged work runs; its `limit` names the budget that ran out.
//!
//! Budgets nest as frames on a thread-local meter. Every public evaluation opens a frame of at
//! most [`MAX_EVAL_FUEL`] units; a frame opened inside another is capped at the enclosing frame's
//! remainder and its spending is deducted from the enclosing frame when it closes. Two callers
//! rely on this: a [`crate::Scope`] that itself evaluates draws on the evaluation that called it
//! (nesting cannot mint fresh fuel), and a host bounds the *total* work of many evaluations — one
//! document load, for instance — by wrapping them all in [`with_eval_budget`]. A drop guard
//! restores the enclosing frame (or the idle state) even if a caller-supplied scope panics.
//! Outside every frame (crate-internal unit tests that drive the evaluator directly) charging is
//! a no-op.

use std::cell::Cell;

use crate::ExprError;

/// Work units one public evaluation ([`crate::eval()`] / [`crate::eval_str`]) may spend.
///
/// `1 << 24` (16,777,216). The largest admissible single construct — a 2^20-element array
/// built, read and reduced — costs about 2^22 units, so the budget admits it with 4× headroom;
/// the most expensive expression in the tracked CXF corpus costs 77 units. Exhaustion bounds one
/// evaluation to tens of milliseconds of work. Inside [`with_eval_budget`] an evaluation is
/// further capped by what remains of that budget.
pub const MAX_EVAL_FUEL: u64 = 1 << 24;

/// The active frame: units remaining, and the budget to report if they run out.
#[derive(Clone, Copy)]
struct Frame {
    remaining: u64,
    limit: u64,
}

thread_local! {
    /// The innermost active frame, `None` when no metered evaluation is active.
    static ACTIVE: Cell<Option<Frame>> = const { Cell::new(None) };
}

/// Closes one frame: deducts its spending from the enclosing frame (or restores the idle state)
/// when dropped, including on unwind.
struct FrameGuard {
    outer: Option<Frame>,
    start: u64,
}

impl Drop for FrameGuard {
    fn drop(&mut self) {
        ACTIVE.with(|cell| {
            let left = cell.get().map_or(0, |frame| frame.remaining);
            let spent = self.start.saturating_sub(left);
            cell.set(self.outer.map(|outer| Frame {
                remaining: outer.remaining.saturating_sub(spent),
                limit: outer.limit,
            }));
        });
    }
}

/// Run `f` in a frame of [`MAX_EVAL_FUEL`] units (capped by any enclosing frame).
pub(crate) fn metered<R>(f: impl FnOnce() -> R) -> R {
    metered_with(MAX_EVAL_FUEL, f)
}

/// Run `f` in a frame of `budget` units. Inside an enclosing frame the new frame starts at the
/// smaller of `budget` and the enclosing remainder; when the enclosing remainder is the smaller,
/// an exhaustion reports the enclosing frame's limit, because that is the budget that ran out.
pub(crate) fn metered_with<R>(budget: u64, f: impl FnOnce() -> R) -> R {
    let outer = ACTIVE.with(Cell::get);
    let frame = match outer {
        Some(enclosing) if enclosing.remaining < budget => Frame {
            remaining: enclosing.remaining,
            limit: enclosing.limit,
        },
        _ => Frame {
            remaining: budget,
            limit: budget,
        },
    };
    ACTIVE.with(|cell| cell.set(Some(frame)));
    let _guard = FrameGuard {
        outer,
        start: frame.remaining,
    };
    f()
}

/// Run `f` with every evaluation inside it drawing on one shared budget of `budget` units.
///
/// Each evaluation inside still stops at [`MAX_EVAL_FUEL`] on its own; together they stop at
/// `budget`. Once it is spent, every later evaluation in `f` refuses at its first charged unit
/// with [`ExprError::ExpressionTooExpensive`] whose `limit` is `budget`, so the total work of `f`'s
/// evaluations is at most `budget` units, plus per refused evaluation only its parse and the work
/// before its first charge (bounded by `MAX_EXPR_NODES`). Deterministic (counts
/// operations, never time); nests like any frame (an enclosing budget still caps this one).
/// Never panics.
pub fn with_eval_budget<R>(budget: u64, f: impl FnOnce() -> R) -> R {
    metered_with(budget, f)
}

/// Spend `units` of the active frame, or refuse with [`ExprError::ExpressionTooExpensive`]
/// (spending nothing) when fewer remain.
pub(crate) fn charge(units: usize) -> Result<(), ExprError> {
    ACTIVE.with(|cell| match cell.get() {
        None => Ok(()),
        Some(frame) => {
            let units = u64::try_from(units).unwrap_or(u64::MAX);
            match frame.remaining.checked_sub(units) {
                Some(left) => {
                    cell.set(Some(Frame {
                        remaining: left,
                        limit: frame.limit,
                    }));
                    Ok(())
                }
                None => Err(ExprError::ExpressionTooExpensive { limit: frame.limit }),
            }
        }
    })
}

/// Units remaining in the innermost active frame (`None` when none is active). Test-only
/// observability for the deterministic cost assertions.
#[cfg(test)]
pub(crate) fn remaining() -> Option<u64> {
    ACTIVE.with(|cell| cell.get().map(|frame| frame.remaining))
}
