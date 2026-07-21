//! The array-shaped §7.7.2 built-ins: `sum`, one-argument `min`/`max`, `fill`, `size`, and
//! `cat`.
//!
//! Array *construction* (literals, ranges) lives in [`mod@crate::eval_array`]; this module
//! evaluates the built-ins that consume or produce those arrays. Arguments arrive already
//! evaluated as scalar-or-array [`EvalResult`]s (the dispatch in `eval::eval_call` routes
//! here), so every function is a pure fold or rebuild over ground values. Three rules keep
//! results bit-deterministic and panic-free:
//!
//! - **Fixed fold order.** `sum`/`min`/`max` fold strictly left to right over the flat element
//!   slice — no reassociation, no FMA — so Real result bits depend only on the element bits
//!   and their order. Integer sums accumulate in `i128` (2^20 addends of `i64` cannot
//!   overflow it) and narrow once at the end; Real folds go through the [`crate::eval::real`]
//!   canonicalization choke point.
//! - **Count before allocation.** `fill` converts its count through `usize::try_from` (a
//!   negative `i64` must become a typed [`ExprError::DomainError`], never the ~1.8e19-element
//!   wrap of an `as usize` cast) and `cat` sums operand lengths in `u128`, both checked
//!   against [`MAX_ARRAY_ELEMENTS`] *before* any `Vec` is sized.
//! - **Typed errors only.** A scalar where an array is required (or vice versa), a
//!   non-numeric reduction, a non-Integer count/dimension, or a dimension other than 1 is a
//!   typed [`ExprError`] — never a panic and never an index expression that could become one.

use oce_model::{
    Value, ValueType,
    determinism::{det_max, det_min},
};

use crate::{ArrayValue, Builtin, EvalResult, ExprError, eval, eval_array::MAX_ARRAY_ELEMENTS};

/// Dispatch an array-shaped built-in over already-evaluated arguments. The parser has checked
/// arity; the slice patterns keep this total even so (a mismatch is a typed error, never an
/// index panic).
pub(crate) fn eval_array_builtin(b: Builtin, args: &[EvalResult]) -> Result<EvalResult, ExprError> {
    match (b, args) {
        (Builtin::Sum, [a]) => Ok(EvalResult::Scalar(sum_array(expect_array(a)?)?)),
        (Builtin::MinArr, [a]) => Ok(EvalResult::Scalar(fold_extremum(expect_array(a)?, true)?)),
        (Builtin::MaxArr, [a]) => Ok(EvalResult::Scalar(fold_extremum(expect_array(a)?, false)?)),
        (Builtin::Fill, [x, n]) => fill_array(x, n),
        (Builtin::Size, [a]) => size_vector(expect_array(a)?),
        (Builtin::Size, [a, i]) => Ok(EvalResult::Scalar(size_dimension(expect_array(a)?, i)?)),
        (Builtin::Cat, [k, operands @ ..]) if operands.len() >= 2 => cat_arrays(k, operands),
        _ => Err(ExprError::DomainError(
            "built-in called with the wrong arity",
        )),
    }
}

/// Unwrap an array argument; a scalar where an array is required is a typed error.
fn expect_array(r: &EvalResult) -> Result<&ArrayValue, ExprError> {
    match r {
        EvalResult::Array(a) => Ok(a),
        EvalResult::Scalar(v) => Err(ExprError::TypeError {
            expected: "an array argument",
            found: eval::type_name(v),
        }),
    }
}

/// The element-type name of an array, for [`ExprError::TypeError`] reporting.
fn elem_type_name(t: ValueType) -> &'static str {
    match t {
        ValueType::Real => "a Real array",
        ValueType::Integer => "an Integer array",
        ValueType::Boolean => "a Boolean array",
        ValueType::String => "a String array",
        ValueType::Enum(_) => "an Enumeration array",
    }
}

/// `sum(A)` over a numeric array: a fixed left-to-right fold from the element-type identity.
///
/// - **Integer** — accumulates in `i128` (which 2^20 addends of `i64` cannot overflow) and
///   narrows once at the end; a result outside `i64` is a typed
///   [`ExprError::DomainError`].
/// - **Real** — sequential `f64` additions seeded from `+0.0`, in slice order, never
///   reassociated and never fused (`(0.1 + 0.2) + 0.3` is `0.6000000000000001`, not the
///   right-associated `0.6` — the goldens pin the order). Seeding from the identity means an
///   all-`-0.0` array sums to `+0.0` (IEEE: `0.0 + -0.0 == +0.0`); the result passes the
///   [`crate::eval::real`] choke point.
/// - **Empty** — the fold never runs, so the result is the element-type identity:
///   `Integer(0)` or `Real(0.0)`. Owner-ratified and provisional under spec rule R10.6 — the
///   CDL spec does not pin the empty-sum value; revisit if R10.6 hardens.
/// - **Boolean** (or any non-numeric element type) is a typed [`ExprError::TypeError`].
fn sum_array(a: &ArrayValue) -> Result<Value, ExprError> {
    match a.elem_type() {
        ValueType::Integer => {
            let mut acc: i128 = 0;
            for v in a.as_slice() {
                if let Value::Integer(i) = v {
                    acc += i128::from(*i);
                }
            }
            i64::try_from(acc)
                .map(Value::Integer)
                .map_err(|_| ExprError::DomainError("integer overflow in sum"))
        }
        ValueType::Real => {
            let mut acc = 0.0_f64;
            for v in a.as_slice() {
                if let Value::Real(r) = v {
                    acc += *r;
                }
            }
            Ok(eval::real(acc))
        }
        other => Err(ExprError::TypeError {
            expected: "a numeric array",
            found: elem_type_name(other),
        }),
    }
}

/// One-argument `min(A)`/`max(A)` over a numeric array: a left-to-right fold seeded from the
/// first element (`want_min` selects min). Integer folds are exact `i64` comparisons; Real
/// folds go through [`det_min`]/[`det_max`] and inherit their policy — a single NaN operand is
/// dropped, `min(+0.0, -0.0) == -0.0`, `max(-0.0, +0.0) == +0.0` — with the result
/// canonicalized. An empty array has no extremum ([`ExprError::EmptyArray`]); a non-numeric
/// element type is a typed [`ExprError::TypeError`] (checked first, so an empty Boolean array
/// reports its type problem).
fn fold_extremum(a: &ArrayValue, want_min: bool) -> Result<Value, ExprError> {
    match a.elem_type() {
        ValueType::Integer => {
            let mut acc: Option<i64> = None;
            for v in a.as_slice() {
                if let Value::Integer(i) = v {
                    acc = Some(match acc {
                        None => *i,
                        Some(m) if want_min => m.min(*i),
                        Some(m) => m.max(*i),
                    });
                }
            }
            acc.map(Value::Integer).ok_or(ExprError::EmptyArray)
        }
        ValueType::Real => {
            let mut acc: Option<f64> = None;
            for v in a.as_slice() {
                if let Value::Real(r) = v {
                    acc = Some(match acc {
                        None => *r,
                        Some(m) if want_min => det_min(m, *r),
                        Some(m) => det_max(m, *r),
                    });
                }
            }
            acc.map(eval::real).ok_or(ExprError::EmptyArray)
        }
        other => Err(ExprError::TypeError {
            expected: "a numeric array",
            found: elem_type_name(other),
        }),
    }
}

/// `fill(x, n)`: an `n`-element array of copies of the scalar `x` (Real, Integer, or Boolean —
/// the same element policy as array literals; String/Enum/array `x` is a typed error). `n`
/// must be an Integer scalar. Guard order matters and is deliberate: `usize::try_from(n)`
/// **first**, so a negative count is a typed [`ExprError::DomainError`] (an `as usize` cast
/// would wrap `-1` to ~1.8e19 and misreport it as too-large); then the
/// [`MAX_ARRAY_ELEMENTS`] cap ([`ExprError::ArrayTooLarge`]); only then the allocation.
/// `n == 0` is a legal typed empty array carrying `x`'s type; Real elements are canonicalized
/// by the [`ArrayValue::vector`] constructor.
fn fill_array(x: &EvalResult, n: &EvalResult) -> Result<EvalResult, ExprError> {
    let x = match x {
        EvalResult::Scalar(v) => v,
        EvalResult::Array(_) => {
            return Err(ExprError::TypeError {
                expected: "a scalar fill value",
                found: "array",
            });
        }
    };
    match x {
        Value::Real(_) | Value::Integer(_) | Value::Boolean(_) => {}
        other => {
            return Err(ExprError::TypeError {
                expected: "a Real, Integer, or Boolean fill value",
                found: eval::type_name(other),
            });
        }
    }
    let n = integer_operand(n, "an Integer element count")?;
    let count = usize::try_from(n).map_err(|_| ExprError::DomainError("negative array size"))?;
    if count > MAX_ARRAY_ELEMENTS {
        return Err(ExprError::ArrayTooLarge {
            count: count as u128,
            max: MAX_ARRAY_ELEMENTS,
        });
    }
    Ok(EvalResult::Array(ArrayValue::vector(
        x.value_type(),
        vec![x.clone(); count],
    )?))
}

/// The array's length as an Integer [`Value`]. The cap keeps every length ≤ 2^20, so the
/// narrowing cannot fail; the `try_from` is a typed belt-and-braces check, not a reachable
/// error.
fn length_value(a: &ArrayValue) -> Result<Value, ExprError> {
    i64::try_from(a.len())
        .map(Value::Integer)
        .map_err(|_| ExprError::DomainError("array length overflows Integer"))
}

/// `size(A)`: the shape vector — a one-element Integer array `{n}` for the 1-D arrays this
/// crate produces (Modelica's `size` returns the vector of extents, one per dimension).
fn size_vector(a: &ArrayValue) -> Result<EvalResult, ExprError> {
    Ok(EvalResult::Array(ArrayValue::vector(
        ValueType::Integer,
        vec![length_value(a)?],
    )?))
}

/// `size(A, i)`: the scalar extent of dimension `i` (1-based). `i` must be an Integer scalar;
/// `i < 1` is a [`ExprError::ShapeMismatch`], and `i >= 2` is one naming the 2-D deferral —
/// arrays are 1-D today, so only dimension 1 exists. Dispatch is by match, never by indexing
/// a dimension table, so no value of `i` can panic or wrap.
fn size_dimension(a: &ArrayValue, i: &EvalResult) -> Result<Value, ExprError> {
    match integer_operand(i, "an Integer dimension index")? {
        1 => length_value(a),
        i if i < 1 => Err(ExprError::ShapeMismatch(
            "array dimension index must be at least 1",
        )),
        _ => Err(ExprError::ShapeMismatch(
            "only dimension 1 exists until 2-D arrays land",
        )),
    }
}

/// `cat(k, A, B, …)`: concatenate arrays along dimension `k`. `k` must be an Integer scalar
/// and, with every array 1-D, must be 1 (`k < 1` is a [`ExprError::ShapeMismatch`]; `k >= 2`
/// is one naming the 2-D deferral). Every operand must be an array of the same element type —
/// there is **no** Integer→Real promotion across operands, unlike inside an array literal
/// ([`ExprError::TypeError`] on mismatch, empty operands included: an empty array still
/// carries its type). The result length is the operand-length sum, computed in `u128` and
/// checked against [`MAX_ARRAY_ELEMENTS`] **before** any allocation — each operand already
/// passed the cap individually, but their sum can exceed it.
fn cat_arrays(k: &EvalResult, operands: &[EvalResult]) -> Result<EvalResult, ExprError> {
    match integer_operand(k, "an Integer concatenation dimension")? {
        1 => {}
        k if k < 1 => {
            return Err(ExprError::ShapeMismatch(
                "concatenation dimension must be at least 1",
            ));
        }
        _ => {
            return Err(ExprError::ShapeMismatch(
                "concatenation along dimension 2 or higher needs 2-D arrays, which are deferred",
            ));
        }
    }
    let mut arrays = Vec::with_capacity(operands.len());
    for r in operands {
        arrays.push(expect_array(r)?);
    }
    let Some(first) = arrays.first() else {
        // Unreachable: the dispatcher admits `cat` only with two or more operands.
        return Err(ExprError::DomainError(
            "built-in called with the wrong arity",
        ));
    };
    let elem_type = first.elem_type();
    for a in &arrays {
        if a.elem_type() != elem_type {
            return Err(ExprError::TypeError {
                expected: elem_type_name(elem_type),
                found: elem_type_name(a.elem_type()),
            });
        }
    }
    let total: u128 = arrays.iter().map(|a| a.len() as u128).sum();
    if total > MAX_ARRAY_ELEMENTS as u128 {
        return Err(ExprError::ArrayTooLarge {
            count: total,
            max: MAX_ARRAY_ELEMENTS,
        });
    }
    // total ≤ 2^20 after the cap check, so the narrowing is exact.
    let mut data = Vec::with_capacity(total as usize);
    for a in &arrays {
        data.extend_from_slice(a.as_slice());
    }
    Ok(EvalResult::Array(ArrayValue::vector(elem_type, data)?))
}

/// Unwrap an Integer scalar operand (a `fill` count, a `size` dimension, a `cat` dimension);
/// anything else — Real, Boolean, String, Enum, or an array — is a typed error naming
/// `expected`.
fn integer_operand(r: &EvalResult, expected: &'static str) -> Result<i64, ExprError> {
    match r {
        EvalResult::Scalar(Value::Integer(i)) => Ok(*i),
        EvalResult::Scalar(other) => Err(ExprError::TypeError {
            expected,
            found: eval::type_name(other),
        }),
        EvalResult::Array(_) => Err(ExprError::TypeError {
            expected,
            found: "array",
        }),
    }
}
