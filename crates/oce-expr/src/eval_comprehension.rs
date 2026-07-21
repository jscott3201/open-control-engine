//! Array comprehension evaluation: `{e for i in r}` (and the same node when it arrives as
//! `sum`'s argument via the `sum(e for i in r)` reduction sugar).
//!
//! The parser produces [`crate::ExprAst::Comprehension`] nodes; this module evaluates them.
//! Four rules keep results bit-deterministic, panic-free, and scope-hygienic:
//!
//! - **Source order, always.** The iteration source evaluates once (any element type is a
//!   legal source; a scalar is a typed [`ExprError::TypeError`]), then the body evaluates
//!   once per element strictly in source order, left to right — never reversed, never
//!   reordered — so Real result bits depend only on the element bits and their order.
//! - **A cheap scope layer, not a rebuilt table.** Each iteration wraps the outer scope in an
//!   [`IterationScope`] — one name, one borrowed delegate — so the iterator SHADOWS an outer
//!   binding of the same name, every other name (and enum resolution) falls through, and the
//!   outer scope is never copied or mutated: the binding structurally cannot leak out.
//! - **Literal-identical collection.** Body results must be scalars (an array-valued body is
//!   a typed error naming the nested/2-D deferral, like a nested literal element) and are
//!   collected through [`crate::eval_array::array_from_scalars`] — the *same* element-type
//!   promotion and canonicalization path brace literals use, ending in the
//!   [`crate::ArrayValue::vector`] invariant re-checks. No new cap site is needed: the result
//!   length equals the source length, and the source is an `ArrayValue` already at or under
//!   the 2^20 cap by construction (`vector` re-checks regardless).
//! - **No fabricated types.** An empty iteration source never evaluates the body, so the
//!   result's element type is unknowable — the same bind as the `{}` literal, resolved the
//!   same way: [`ExprError::EmptyArray`], never a guessed-type empty array. This deliberately
//!   makes `sum` of an empty comprehension an error too (doc-02's empty → 0 row cannot say
//!   *which* zero — `Integer(0)` vs `Real(0.0)` — without a body type to read it from).
//!
//! Multi-iterator clauses parse (the reserved `iters` surface) but are rejected here with a
//! typed [`ExprError::DomainError`] naming the deferral.

use std::sync::Arc;

use oce_model::EnumClassId;

use crate::{EvalResult, ExprAst, ExprError, Scope, eval, eval_array};

/// One iterator binding layered over the outer scope — a cheap wrapper, not a rebuilt table.
/// [`Scope::lookup`] answers the iterator name from the stored element (shadowing any outer
/// binding of that name) and delegates every other name, and both enum-resolution hooks, to
/// the wrapped scope.
struct IterationScope<'a> {
    /// The iterator name this layer answers for.
    name: &'a str,
    /// The current element, pre-wrapped so `lookup` can hand out a borrow.
    value: EvalResult,
    /// The enclosing scope every other name resolves against.
    outer: &'a dyn Scope,
}

impl Scope for IterationScope<'_> {
    fn lookup(&self, name: &str) -> Option<&EvalResult> {
        if name == self.name {
            Some(&self.value)
        } else {
            self.outer.lookup(name)
        }
    }

    fn enum_class(&self, qualified: &str) -> Option<EnumClassId> {
        self.outer.enum_class(qualified)
    }

    fn enum_ordinal(&self, class: EnumClassId, literal: &str) -> Option<u32> {
        self.outer.enum_ordinal(class, literal)
    }
}

/// Evaluate a comprehension node: exactly one `i in r` clause (more is the multi-iterator
/// deferral), `r` to an array, then the body once per element in source order under an
/// [`IterationScope`], collected via [`crate::eval_array::array_from_scalars`]. Errors are
/// typed ([`ExprError::DomainError`] for the deferral, [`ExprError::TypeError`] for a scalar
/// source or an array-valued body, [`ExprError::EmptyArray`] for an empty source), never a
/// panic.
pub(crate) fn eval_comprehension(
    body: &ExprAst,
    iters: &[(Arc<str>, ExprAst)],
    scope: &dyn Scope,
) -> Result<EvalResult, ExprError> {
    // The clause-count gate runs before anything evaluates, so `{i+j for i in 1:2, j in 1:2}`
    // names the deferral even when the sources are unbound. A zero-clause node cannot come
    // from the parser; the same typed error keeps the hand-built-AST path total.
    let [(name, source)] = iters else {
        return Err(ExprError::DomainError(
            "a comprehension binds exactly one iterator \
             (multi-iterator comprehensions are deferred)",
        ));
    };
    let source = match eval::eval_node(source, scope)? {
        EvalResult::Array(a) => a,
        EvalResult::Scalar(v) => {
            return Err(ExprError::TypeError {
                expected: "an array iteration source",
                found: eval::type_name(&v),
            });
        }
    };
    if source.is_empty() {
        // POLICY: the body is NEVER evaluated over an empty source, so the result's element
        // type is unknowable — return the same EmptyArray error as the `{}` literal rather
        // than fabricate a typed empty array (see the module header).
        return Err(ExprError::EmptyArray);
    }
    // Result length == source length ≤ 2^20 (the source ArrayValue's construction cap), so no
    // new cap check is needed before this allocation; `array_from_scalars` ends in
    // `ArrayValue::vector`, which re-checks it structurally.
    let mut values = Vec::with_capacity(source.len());
    for elem in source.as_slice() {
        let bound = IterationScope {
            name: name.as_ref(),
            value: EvalResult::Scalar(elem.clone()),
            outer: scope,
        };
        match eval::eval_node(body, &bound)? {
            EvalResult::Scalar(v) => values.push(v),
            EvalResult::Array(_) => {
                return Err(ExprError::TypeError {
                    expected: "a scalar comprehension body (nested/2-D comprehensions are \
                               deferred)",
                    found: "array",
                });
            }
        }
    }
    Ok(EvalResult::Array(eval_array::array_from_scalars(values)?))
}
