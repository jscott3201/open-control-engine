//! Strict-finite evaluation support for load-time grounding (issue #362).
//!
//! [`FiniteScope`] wraps a caller scope and answers [`Scope::requires_finite`] with `true`; the
//! evaluator's two recursion entry points ([`crate::eval::eval_node`] and
//! [`crate::eval::eval_scalar`]) pass every node result through [`check_scalar`] /
//! [`check_result`], which refuse a non-finite Real with [`ExprError::NonFinite`] only under such a
//! scope. Under an ordinary scope both checks are the identity, so [`crate::eval()`] keeps its
//! documented IEEE propagation semantics bit-for-bit.

use oce_model::{EnumClassId, Value};

use crate::{EvalResult, ExprError, Scope};

/// A pass-through scope that marks the evaluation as strict-finite.
pub(crate) struct FiniteScope<'a> {
    /// The caller's scope every lookup resolves against.
    pub(crate) inner: &'a dyn Scope,
}

impl Scope for FiniteScope<'_> {
    fn lookup(&self, name: &str) -> Option<&EvalResult> {
        self.inner.lookup(name)
    }

    fn enum_class(&self, qualified: &str) -> Option<EnumClassId> {
        self.inner.enum_class(qualified)
    }

    fn enum_ordinal(&self, class: EnumClassId, literal: &str) -> Option<u32> {
        self.inner.enum_ordinal(class, literal)
    }

    fn requires_finite(&self) -> bool {
        true
    }
}

fn refuse(value: f64) -> ExprError {
    ExprError::NonFinite {
        value: if value.is_nan() {
            "NaN"
        } else if value > 0.0 {
            "inf"
        } else {
            "-inf"
        },
    }
}

/// Refuse a non-finite scalar Real when `scope` is strict; otherwise return it unchanged.
pub(crate) fn check_scalar(value: Value, scope: &dyn Scope) -> Result<Value, ExprError> {
    match value {
        Value::Real(v) if !v.is_finite() && scope.requires_finite() => Err(refuse(v)),
        other => Ok(other),
    }
}

/// Refuse a scalar or array result holding any non-finite Real when `scope` is strict.
pub(crate) fn check_result(result: EvalResult, scope: &dyn Scope) -> Result<EvalResult, ExprError> {
    if !scope.requires_finite() {
        return Ok(result);
    }
    match result {
        EvalResult::Scalar(value) => Ok(EvalResult::Scalar(check_scalar(value, scope)?)),
        EvalResult::Array(array) => {
            for element in array.as_slice() {
                if let Value::Real(v) = element
                    && !v.is_finite()
                {
                    return Err(refuse(*v));
                }
            }
            Ok(EvalResult::Array(array))
        }
    }
}
