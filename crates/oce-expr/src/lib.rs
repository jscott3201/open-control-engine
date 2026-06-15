#![forbid(unsafe_code)]
//! `oce-expr` — the CDL §7.7.2 binding-expression parser/evaluator for the Open Control
//! Engine.
//!
//! Parameter/constant *bindings* may carry a restricted, closed-world expression language
//! (literals, identifier references, arithmetic/relational/boolean operators, the §7.7.2
//! built-in function set, array literals/comprehensions). `oce-expr` parses opaque CDL binding
//! text into an [`ExprAst`] and evaluates it against a [`Scope`] to a ground value. It is
//! **Group A** (no store, no selene-db), pure, total, and never reads the clock, connectors,
//! or computation-affecting attributes (R11).
//!
//! Status: **M0 scaffold.** The grammar/evaluator land in M1; the public surface below is the
//! stable contract `oce-flatten` binds to.

use std::sync::Arc;

use oce_model::Value;

/// A parsed binding expression. Unknown functions are **not representable** — they are
/// rejected during parse/resolve (R9).
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum ExprAst {
    /// A ground real literal.
    Real(f64),
    /// A ground integer literal.
    Int(i64),
    /// A ground boolean literal.
    Bool(bool),
    /// A ground string literal (metadata only).
    Str(Arc<str>),
    /// A reference to another in-scope parameter/constant, resolved during propagation.
    Ident(Arc<str>),
    /// A call to one of the closed §7.7.2 built-in functions (R9).
    Call(Builtin, Vec<ExprAst>),
}

/// The closed §7.7.2 built-in function set (R9: anything outside this set is rejected).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum Builtin {
    /// `abs(v)` — absolute value (type-preserving).
    Abs,
    /// `sign(v)` — `-1`/`0`/`1`, **always Integer** result.
    Sign,
    /// `sqrt(v)` — requires `v ≥ 0` else a domain error.
    Sqrt,
    /// `div(x, y)` — quotient truncated toward zero.
    Div,
    /// `mod(x, y)` — `x - floor(x/y)*y` (sign of `y`).
    Mod,
    /// `rem(x, y)` — remainder (sign of `x`).
    Rem,
    /// `floor(x)` — largest integer ≤ `x`, returned as Real.
    Floor,
    /// `ceil(x)` — smallest integer ≥ `x`, returned as Real.
    Ceil,
    /// `integer(x)` — `floor` cast to Integer.
    Integer,
    /// `min`/`max`/`sum`/`cat`/`fill`/`size` and friends (resolved by arity at parse time).
    Aggregate,
}

/// A fully-evaluated binding result (ground value or array). Arrays land in M1.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum EvalResult {
    /// A scalar ground value.
    Scalar(Value),
}

/// Resolves identifiers to ground values during evaluation. Implemented over the in-scope
/// parameter/constant table that `oce-flatten` supplies. Pure and total: never reads
/// attributes, connectors, or time (R11).
pub trait Scope {
    /// Look up a parameter/constant by name.
    fn lookup(&self, name: &str) -> Option<EvalResult>;
}

/// A typed expression error (never a panic; R10/R11).
#[derive(Clone, Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ExprError {
    /// A parse failure with a human-readable detail.
    #[error("expression parse error: {0}")]
    Parse(String),
    /// A function outside the §7.7.2 table (and §7.3 whitelist) appeared in a binding (R9).
    #[error("unsupported function in binding: {0}")]
    UnsupportedFunction(String),
    /// A referenced identifier was not in scope.
    #[error("unknown identifier: {0}")]
    UnknownIdent(String),
    /// A domain violation, e.g. `sqrt` of a negative value (R10.4).
    #[error("domain error: {0}")]
    DomainError(&'static str),
    /// Division by zero in `div`/`mod`/`rem`.
    #[error("division by zero")]
    DivisionByZero,
}

/// Parse opaque CDL binding text into an [`ExprAst`], rejecting out-of-subset constructs (R9).
///
/// # Errors
/// Returns [`ExprError::Parse`] / [`ExprError::UnsupportedFunction`] for invalid input.
pub fn parse(_text: &str) -> Result<ExprAst, ExprError> {
    unimplemented!("oce-expr::parse — M0 scaffold (grammar lands in M1)")
}

/// Evaluate an [`ExprAst`] to a ground value against a [`Scope`]. Total: never panics, always
/// returns `Ok` or a typed [`ExprError`]; deterministic (R10.7 fixed summation order).
///
/// # Errors
/// Returns a typed [`ExprError`] on any evaluation failure.
pub fn eval(_ast: &ExprAst, _scope: &dyn Scope) -> Result<EvalResult, ExprError> {
    unimplemented!("oce-expr::eval — M0 scaffold (evaluator lands in M1)")
}

/// Convenience for the flattener: [`parse`] then [`eval`] in one shot.
///
/// # Errors
/// Returns a typed [`ExprError`] on parse or evaluation failure.
pub fn eval_str(text: &str, scope: &dyn Scope) -> Result<EvalResult, ExprError> {
    eval(&parse(text)?, scope)
}
