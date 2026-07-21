//! 1-D array indexing: evaluating a postfix subscript `A[i]` to the addressed element.
//!
//! Array *construction* lives in [`mod@crate::eval_array`] and the array-shaped built-ins in
//! [`mod@crate::eval_array_builtins`]; this module evaluates the [`crate::ExprAst::Index`]
//! nodes the parser produces. Three rules keep the read total, wrap-free, and
//! bit-deterministic:
//!
//! - **Integer subscripts only.** A subscript must evaluate to an Integer scalar; Real,
//!   Boolean, String, and Enumeration subscripts are [`ExprError::NonIntegerIndex`] — `A[1.0]` is
//!   rejected, never floored. Routing a subscript through the saturating `integer()` built-in
//!   coercion would turn `A[0.999999…]` (a rounding artifact) into a silent read of the wrong
//!   element; on a control engine a wrong element is a physical hazard, so the type gate is
//!   absolute.
//! - **Bounds before conversion.** CDL/Modelica indexing is 1-based: the check is
//!   `i < 1 || i > len` on the `i64` subscript itself, with `len` widened to `i64` (the
//!   [`crate::eval_array::MAX_ARRAY_ELEMENTS`] cap keeps `len` ≤ 2^20, so the widening is
//!   exact). Only after the check is the 0-based offset computed, making `i64::MAX` and
//!   `i64::MIN` subscripts safe by construction — they fail the comparison and report
//!   [`ExprError::IndexOutOfBounds`], never wrap through an `as usize` cast. On an empty
//!   array every subscript is out of bounds (`size` reports 0).
//! - **Canonical elements pass through.** Every stored element already went through the
//!   crate's canonicalization choke points ([`crate::ArrayValue::vector`] at construction,
//!   the identifier read's re-canonicalizing clone), so the read returns a clone of the
//!   element bits as-is — `-0.0` and canonical-NaN elements read back bit-identical.
//!
//! A multi-subscript `A[i, j]` parses but is rejected here with a typed
//! [`ExprError::DomainError`] naming the deferral; a chained `A[1][2]` needs no special case —
//! the first subscript yields a scalar and the second application reports the scalar-base
//! [`ExprError::TypeError`].

use oce_model::Value;

use crate::{EvalResult, ExprAst, ExprError, Scope, eval};

/// Evaluate `base[indices…]`: the base expression to an array, the single subscript to an
/// Integer scalar, then the 1-based bounds-checked element read described in the module
/// header. Errors are typed ([`ExprError::TypeError`] for a scalar base,
/// [`ExprError::DomainError`] for the multi-subscript deferral,
/// [`ExprError::NonIntegerIndex`] / [`ExprError::IndexOutOfBounds`] for the subscript),
/// never a panic.
pub(crate) fn eval_index(
    base: &ExprAst,
    indices: &[ExprAst],
    scope: &dyn Scope,
) -> Result<EvalResult, ExprError> {
    let base_v = eval::eval_node(base, scope)?;
    let array = match &base_v {
        EvalResult::Array(a) => a,
        EvalResult::Scalar(v) => {
            return Err(ExprError::TypeError {
                expected: "an array to index",
                found: eval::type_name(v),
            });
        }
    };
    // The count gate runs before any subscript evaluates, so `A[i, j]` names the deferral
    // even when `i`/`j` are unbound.
    let [index] = indices else {
        return Err(ExprError::DomainError(
            "arrays are 1-D, so a subscript takes exactly one index \
             (multi-dimensional indexing is deferred)",
        ));
    };
    // The scalar path already rejects an array-valued subscript with the established
    // array-in-scalar-position TypeError; any other non-Integer scalar is NonIntegerIndex.
    let i = match eval::eval_scalar(index, scope)? {
        Value::Integer(i) => i,
        other => {
            return Err(ExprError::NonIntegerIndex {
                found: eval::type_name(&other),
            });
        }
    };
    let size = array.len();
    // Bounds on the i64 itself, before any usize conversion (see the module header).
    if i < 1 || i > size as i64 {
        return Err(ExprError::IndexOutOfBounds { index: i, size });
    }
    // 1 ≤ i ≤ size ≤ 2^20 after the check, so `i - 1` is an exact 0-based offset. The
    // checked `try_from` + `get` keep the read total even if the bounds check ever
    // regresses — the fallback is a typed error, never an index panic.
    usize::try_from(i - 1)
        .ok()
        .and_then(|offset| array.as_slice().get(offset))
        .map(|elem| EvalResult::Scalar(elem.clone()))
        .ok_or(ExprError::DomainError(
            "array subscript escaped its bounds check",
        ))
}
