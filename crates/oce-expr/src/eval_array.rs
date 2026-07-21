//! Array construction and evaluation: 1-D brace literals and `a:b` / `a:step:b` ranges.
//!
//! Everything array-shaped that *evaluates* lives here (the scalar evaluator stays in
//! [`mod@crate::eval`]): literal element folding and type resolution, range counting and element
//! generation, and the [`ArrayValue`] constructor that enforces the shape/type/canonicalization
//! invariants. Two rules keep results bit-deterministic and panic-free:
//!
//! - **Closed-form elements.** Range element `k` is `start + k * step` (Integer in `i128`,
//!   Real in `f64`) — never an accumulating `acc += step` loop, so element bits depend only on
//!   the operands and `k`, and every Real element passes [`crate::eval::real`].
//! - **Count before allocation.** The element count is computed in `i128` (Integer) or `f64`
//!   (Real) — domains the count math cannot overflow — and checked against
//!   [`MAX_ARRAY_ELEMENTS`] *before* any `Vec` is sized. `floor() as i64` would saturate at
//!   `i64::MAX` and a later `+ 1` would overflow in debug builds; the count therefore stays in
//!   the wide domain until after the cap check.

use oce_model::{Value, ValueType, determinism::canonicalize_real};

use crate::{ArrayValue, EvalResult, ExprAst, ExprError, Scope, Shape, eval};

/// Maximum number of elements one array construct may produce — a DoS / OOM-abort guard.
/// The element count is purely input-derived from untrusted binding text; without a ceiling a
/// single `1:1000000000` (or a tiny Real step such as `0:1e-300:1`) would drive a
/// multi-gigabyte `Vec::with_capacity` that aborts the (embeddable, safety-critical) process
/// *uncatchably*, defeating the panic-free typed-`ExprError` contract. Checked **before** any
/// allocation. `1 << 20` mirrors the identical cap in `oce-cxf` (`arrays.rs`) and is far beyond
/// any realistic equipment-scale parameter array; revisit both together if a legitimate model
/// ever approaches it.
pub(crate) const MAX_ARRAY_ELEMENTS: usize = 1 << 20;

impl ArrayValue {
    /// Build a 1-D array from already-evaluated elements, enforcing every [`ArrayValue`]
    /// invariant: the element cap, per-element agreement with `elem_type`, and Real NaN
    /// canonicalization (the same [`canonicalize_real`] choke point every scalar Real passes;
    /// `-0.0` is preserved). Callers hand in elements that already satisfy the type and
    /// canonicalization invariants — the re-checks here make the invariants structural rather
    /// than by-convention, at the cost of one linear pass. Errors are typed
    /// ([`ExprError::ArrayTooLarge`] / [`ExprError::TypeError`]), never a panic.
    pub(crate) fn vector(elem_type: ValueType, mut data: Vec<Value>) -> Result<Self, ExprError> {
        if data.len() > MAX_ARRAY_ELEMENTS {
            return Err(ExprError::ArrayTooLarge {
                count: data.len() as u128,
                max: MAX_ARRAY_ELEMENTS,
            });
        }
        for v in &mut data {
            if let Value::Real(r) = v {
                *r = canonicalize_real(*r);
            }
            if v.value_type() != elem_type {
                return Err(ExprError::TypeError {
                    expected: "a uniform array element type",
                    found: eval::type_name(v),
                });
            }
        }
        Ok(Self {
            elem_type,
            shape: Shape::D1(data.len()),
            data,
        })
    }

    /// Clone for an identifier read, re-canonicalizing every Real element — the array analog
    /// of the scalar scope-read canonicalization (a [`Scope`] implementor may hold values with
    /// arbitrary NaN bits). Canonicalization never changes an element's type or the length, so
    /// the clone satisfies the same invariants.
    pub(crate) fn canonicalized_clone(&self) -> Self {
        Self {
            elem_type: self.elem_type,
            shape: self.shape,
            data: self.data.iter().map(eval::canonicalize_value).collect(),
        }
    }
}

/// Evaluate a `{a, b, c}` literal: elements left-to-right, then resolve the element type.
///
/// Typing (§7.1 promotion, element-wise): all-Integer stays Integer; any Real promotes every
/// Integer element to Real (canonicalized); homogeneous Boolean is legal. String or Enum
/// elements, and Boolean/numeric mixing, are [`ExprError::TypeError`]. An element that itself
/// evaluates to an array (nested literal, or a range element) is a [`ExprError::TypeError`] —
/// 2-D/nested arrays are deferred. `{}` is [`ExprError::EmptyArray`]: its element type is
/// unknowable, and fabricating `Real[0]` would risk silent mistyping downstream.
pub(crate) fn eval_array_literal(
    elems: &[ExprAst],
    scope: &dyn Scope,
) -> Result<EvalResult, ExprError> {
    if elems.is_empty() {
        return Err(ExprError::EmptyArray);
    }
    // Cap before ANY allocation, uniformly with the range paths. A literal's element count
    // equals its AST node count (already parsed), so this cannot amplify input size the way a
    // range can — the check keeps the cap rule uniform rather than guarding a live hazard.
    if elems.len() > MAX_ARRAY_ELEMENTS {
        return Err(ExprError::ArrayTooLarge {
            count: elems.len() as u128,
            max: MAX_ARRAY_ELEMENTS,
        });
    }
    let mut values = Vec::with_capacity(elems.len());
    for e in elems {
        match eval::eval_node(e, scope)? {
            EvalResult::Scalar(v) => values.push(v),
            EvalResult::Array(_) => {
                return Err(ExprError::TypeError {
                    expected: "a scalar array element (2-D/nested arrays are deferred)",
                    found: "array",
                });
            }
        }
    }
    let mut has_real = false;
    let mut has_int = false;
    let mut has_bool = false;
    for v in &values {
        match v {
            Value::Real(_) => has_real = true,
            Value::Integer(_) => has_int = true,
            Value::Boolean(_) => has_bool = true,
            other => {
                return Err(ExprError::TypeError {
                    expected: "Real, Integer, or Boolean array elements",
                    found: eval::type_name(other),
                });
            }
        }
    }
    if has_bool && (has_real || has_int) {
        return Err(ExprError::TypeError {
            expected: "array elements of one type",
            found: "a Boolean/numeric mix",
        });
    }
    let (elem_type, data) = if has_bool {
        (ValueType::Boolean, values)
    } else if has_real {
        let promoted = values
            .into_iter()
            .map(|v| match v {
                Value::Integer(i) => eval::real(i as f64),
                Value::Real(r) => eval::real(r),
                other => other, // unreachable after the scan above; kept total, not a panic
            })
            .collect();
        (ValueType::Real, promoted)
    } else {
        (ValueType::Integer, values)
    };
    Ok(EvalResult::Array(ArrayValue::vector(elem_type, data)?))
}

/// Evaluate a range: operands in source order (`start`, `step` if present, `stop`), each a
/// numeric scalar. All-Integer operands build an Integer range; any Real operand promotes the
/// whole range to Real (`a:b` is `a:1:b` in both). Non-numeric or array operands are
/// [`ExprError::TypeError`]; a zero step is [`ExprError::DomainError`]; empty ranges are legal.
pub(crate) fn eval_range(
    start: &ExprAst,
    step: Option<&ExprAst>,
    stop: &ExprAst,
    scope: &dyn Scope,
) -> Result<EvalResult, ExprError> {
    let start_v = range_operand(start, scope)?;
    let step_v = step.map(|s| range_operand(s, scope)).transpose()?;
    let stop_v = range_operand(stop, scope)?;
    match (&start_v, &step_v, &stop_v) {
        (Value::Integer(a), Some(Value::Integer(s)), Value::Integer(b)) => {
            integer_range(*a, *s, *b)
        }
        (Value::Integer(a), None, Value::Integer(b)) => integer_range(*a, 1, *b),
        _ => {
            // At least one Real operand: promote everything to f64 (the same `i64 as f64`
            // promotion the scalar operators use).
            let a = as_range_f64(&start_v);
            let s = step_v.as_ref().map_or(1.0, as_range_f64);
            let b = as_range_f64(&stop_v);
            real_range(a, s, b)
        }
    }
}

/// Evaluate one range operand to a numeric scalar, or a typed error.
fn range_operand(ast: &ExprAst, scope: &dyn Scope) -> Result<Value, ExprError> {
    let v = eval::eval_scalar(ast, scope)?;
    match v {
        Value::Real(_) | Value::Integer(_) => Ok(v),
        other => Err(ExprError::TypeError {
            expected: "Real or Integer range operands",
            found: eval::type_name(&other),
        }),
    }
}

/// Numeric range operand as `f64` (callers guarantee Real or Integer).
fn as_range_f64(v: &Value) -> f64 {
    match v {
        Value::Real(r) => *r,
        Value::Integer(i) => *i as f64,
        // Unreachable: `range_operand` admits only numerics. A total fallback keeps this
        // panic-free; 0.0 would surface as a wrong-but-typed result, never an abort.
        _ => 0.0,
    }
}

/// `start:step:stop` over Integer operands. Count = `max(0, floor((stop - start) / step) + 1)`,
/// computed in `i128` — the span `stop - start` can overflow `i64` (e.g. `i64::MIN:i64::MAX`).
/// Elements come from the closed form `start + k * step` in `i128`; every element lies between
/// `start` and `stop` inclusive, so the narrowing back to `i64` cannot fail (the `try_from` is
/// a typed belt-and-braces check, not a reachable error).
fn integer_range(start: i64, step: i64, stop: i64) -> Result<EvalResult, ExprError> {
    if step == 0 {
        return Err(ExprError::DomainError("range step is zero"));
    }
    let span = i128::from(stop) - i128::from(start);
    let count = (eval::ifloordiv_i128(span, i128::from(step)) + 1).max(0);
    if count > MAX_ARRAY_ELEMENTS as i128 {
        return Err(ExprError::ArrayTooLarge {
            count: count as u128, // non-negative, so the cast is exact
            max: MAX_ARRAY_ELEMENTS,
        });
    }
    let count = count as usize; // 0 ..= MAX_ARRAY_ELEMENTS after the checks above
    let mut data = Vec::with_capacity(count);
    for k in 0..count {
        let elem = i128::from(start) + (k as i128) * i128::from(step);
        let elem = i64::try_from(elem)
            .map_err(|_| ExprError::DomainError("range element overflows Integer"))?;
        data.push(Value::Integer(elem));
    }
    Ok(EvalResult::Array(ArrayValue::vector(
        ValueType::Integer,
        data,
    )?))
}

/// `start:step:stop` with at least one Real operand. Count from the closed-form `f64` formula
/// `floor((stop - start) / step) + 1`, kept in `f64` until after the cap check (see the module
/// header for why). Elements come from the closed form `start + k * step`, canonicalized —
/// note `k == 0` yields `start + 0.0`, which normalizes a `-0.0` start to `+0.0` (IEEE
/// addition); the determinism goldens pin this. A NaN operand — reachable from pure binding
/// text (`0.0/0.0:1.0` folds to a NaN start), not just from a scope value — fails the
/// `count >= 1` test and yields a legal empty Real array, the same as a span that opposes the
/// step's direction: the deliberate total-function policy, pinned by the goldens.
fn real_range(start: f64, step: f64, stop: f64) -> Result<EvalResult, ExprError> {
    if step == 0.0 {
        return Err(ExprError::DomainError("range step is zero"));
    }
    let count_f = ((stop - start) / step).floor() + 1.0;
    if count_f.is_nan() || count_f < 1.0 {
        return Ok(EvalResult::Array(ArrayValue::vector(
            ValueType::Real,
            Vec::new(),
        )?));
    }
    if count_f > MAX_ARRAY_ELEMENTS as f64 {
        // `as u128` saturates deterministically (never panics); a count beyond u128::MAX
        // (e.g. `0:1e-300:1` asks for ~1e300 elements) reports u128::MAX.
        return Err(ExprError::ArrayTooLarge {
            count: count_f as u128,
            max: MAX_ARRAY_ELEMENTS,
        });
    }
    let count = count_f as usize; // integer-valued and <= 2^20, so the cast is exact
    let mut data = Vec::with_capacity(count);
    for k in 0..count {
        data.push(eval::real(start + (k as f64) * step));
    }
    Ok(EvalResult::Array(ArrayValue::vector(
        ValueType::Real,
        data,
    )?))
}
