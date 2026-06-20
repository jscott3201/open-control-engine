#![forbid(unsafe_code)]
//! `oce-expr` — the CDL §7.7.2 binding-expression parser/evaluator for the Open Control
//! Engine.
//!
//! Parameter/constant *bindings* may carry a restricted, closed-world expression language
//! (literals, identifier references, arithmetic/relational/boolean operators, the §7.7.2
//! built-in function set, and — later — array literals/comprehensions). `oce-expr` parses
//! opaque CDL binding text into an [`ExprAst`] and evaluates it against a [`Scope`] to a ground
//! value. It is **Group A** (no store, no database), pure, total, and never reads the clock,
//! connectors, or computation-affecting attributes (R11).
//!
//! # Scalar subset
//!
//! This crate implements the scalar grounding subset that `oce-flatten` needs to fold
//! parameter/constant bindings (`02-type-system-and-values.md` §6–7):
//!
//! - **Literals** — `Real`, `Integer`, `Boolean`, `String`.
//! - **Constants** — `pi`/`eps`/`small`/`inf` (bare or `…Constants.<name>` qualified), [`BuiltinConst`].
//! - **Identifier references** — resolved against a [`Scope`] (other in-scope params/constants).
//! - **Operators** — unary `-`/`not`; binary `+ - * /`, relational `> >= < <= == <>`, `and`/`or`.
//! - **Scalar built-ins** — `abs sign sqrt div mod rem floor ceil integer min max` (2-arg
//!   `min`/`max`), with the exact CDL numeric promotion and R10.x semantics.
//!
//! Arrays (`{…}` literals, comprehensions, `A[i]` indexing, `a:b` ranges) and the array-shaped
//! built-ins (`sum`/`cat`/`fill`/`size`, the array forms of `min`/`max`), plus enumeration references
//! and the `Modelica.Math.*` alias whitelist are deferred. They appear in `02` §7.4 and are reserved
//! here via `#[non_exhaustive]` so adding them is not a breaking change; encountering them in a
//! binding today is a typed error, never a panic.
//!
//! The public surface below (`parse`/`eval`/`eval_str`, [`ExprAst`], [`Scope`], [`EvalResult`],
//! [`ExprError`]) is the **stable contract** `oce-flatten` binds to.

use std::sync::Arc;

use oce_model::{EnumClassId, Value};

mod eval;
mod parse;
#[cfg(test)]
mod tests;

/// A parsed binding expression. Unknown functions are **not representable** — they are
/// rejected during parse/resolve (R9). Array constructs (`02` §7.4: `EnumRef`, `ArrayLit`,
/// `Comprehension`, `Index`, `Range`) are reserved via `#[non_exhaustive]`.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum ExprAst {
    /// A ground real literal.
    Real(f64),
    /// A ground integer literal.
    Int(i64),
    /// A ground boolean literal.
    Bool(bool),
    /// A ground string literal (metadata only — never a tick signal, §7.8).
    Str(Arc<str>),
    /// A reference to another in-scope parameter/constant, resolved during propagation.
    Ident(Arc<str>),
    /// A named CDL constant (`pi`/`eps`/`small`/`inf`), folded to a `Real` at evaluation.
    Const(BuiltinConst),
    /// A unary operator applied to one operand.
    Unary(UnOp, Box<ExprAst>),
    /// A binary operator applied to two operands.
    Binary(BinOp, Box<ExprAst>, Box<ExprAst>),
    /// A call to one of the closed §7.7.2 built-in functions (R9).
    Call(Builtin, Vec<ExprAst>),
}

/// A named CDL constant from `CDL.Constants` (§6.1). Each folds to a ground `Real`.
///
/// Values track the Buildings `Buildings.Controls.OBC.CDL.Constants` module (the conformance
/// oracle), which uses round decimal literals — **not** `ModelicaServices.Machine.*`. The exact
/// bit-values are **owner-ratifiable, pending an `oce-conformance` cross-check** against the
/// library source (see `_spec/11-m1-cxf-plan.md`); `eps` in particular is the CDL comparison
/// tolerance, whose value (the Buildings literal `1e-15` vs the Modelica machine epsilon
/// `≈2.22e-16`) must be confirmed before it is locked.
///
/// Note: this `inf` is the **Modelica/CDL constant** (`1e60`, a large *finite* real), distinct
/// from an IEEE `±∞` literal. The spelled-out `-Inf`/`+Inf` Real literals of §6.1 are a CXF
/// typed-value serialization concern parsed by `oce-cxf`, not part of this expression grammar.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum BuiltinConst {
    /// `pi` — the ratio of a circle's circumference to its diameter (`2·asin(1)`, == `f64` `PI`).
    Pi,
    /// `eps` — the CDL comparison tolerance (Buildings literal `1e-15`; value owner-ratifiable).
    Eps,
    /// `small` — smallest number such that `small` and `-small` are representable (`1e-60`).
    Small,
    /// `inf` — the large finite real the library treats as "infinity" (`1e60`); not IEEE `∞`.
    Inf,
}

/// A unary operator (§6.1).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum UnOp {
    /// Arithmetic negation (`-x`); type-preserving over the numeric types.
    Neg,
    /// Logical negation (`not b`); Boolean only.
    Not,
}

/// A binary operator (§6.1). Arithmetic, relational, and boolean operators share one enum; the
/// evaluator dispatches result typing per CDL promotion (§7.1).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum BinOp {
    /// `+` — `Integer+Integer → Integer`; any `Real` operand promotes to `Real`.
    Add,
    /// `-` — same promotion as `+`.
    Sub,
    /// `*` — same promotion as `+`.
    Mul,
    /// `/` — **always** yields `Real` (`3/2 == 1.5`, never `1`).
    Div,
    /// `>` — numeric comparison → `Boolean`.
    Gt,
    /// `>=` — numeric comparison → `Boolean`.
    Ge,
    /// `<` — numeric comparison → `Boolean`.
    Lt,
    /// `<=` — numeric comparison → `Boolean`.
    Le,
    /// `==` — equality over numeric, boolean, or string operands → `Boolean`.
    Eq,
    /// `<>` — inequality (CDL/Modelica "not equal"), same operand types as `==` → `Boolean`.
    Ne,
    /// `and` — Boolean operands only → `Boolean`.
    And,
    /// `or` — Boolean operands only → `Boolean`.
    Or,
}

/// The closed §7.7.2 built-in function set implemented for the **scalar** subset (R9: anything
/// outside this set, and the deferred array-shaped built-ins, is rejected).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum Builtin {
    /// `abs(v)` — absolute value (type-preserving).
    Abs,
    /// `sign(v)` — `-1`/`0`/`1`, **always Integer** result (R10.3a).
    Sign,
    /// `sqrt(v)` — requires `v ≥ 0` else a domain error (R10.4); Real result.
    Sqrt,
    /// `div(x, y)` — quotient truncated toward zero (R10.1); Integer iff both Integer.
    Div,
    /// `mod(x, y)` — `x - floor(x/y)*y`, sign of `y` (R10.2).
    Mod,
    /// `rem(x, y)` — remainder, sign of `x`, with `div(x,y)*y + rem(x,y) == x` (R10.2).
    Rem,
    /// `floor(x)` — largest integer ≤ `x`, returned as Real (R10.3).
    Floor,
    /// `ceil(x)` — smallest integer ≥ `x`, returned as Real (R10.3).
    Ceil,
    /// `integer(x)` — `floor` cast to Integer (R10.1/R10.3).
    Integer,
    /// `min(x, y)` — minimum of two scalars; promoted numeric type.
    MinScalar,
    /// `max(x, y)` — maximum of two scalars; promoted numeric type.
    MaxScalar,
}

/// A fully-evaluated binding result. Arrays (an `EvalResult::Array` variant in `02` §7.4) are
/// reserved here via `#[non_exhaustive]`.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum EvalResult {
    /// A scalar ground value.
    Scalar(Value),
}

/// Resolves identifiers to ground values during evaluation. Implemented over the in-scope
/// parameter/constant table that `oce-flatten` supplies. Pure and total: never reads
/// attributes, connectors, or time (R11).
///
/// `enum_class`/`enum_ordinal` back enumeration-reference grounding (`02` §7.4). The scalar subset
/// never calls them; default implementations return `None`, so scalar-only scopes need only provide
/// [`Scope::lookup`]. They gain real bodies when enum references are implemented.
pub trait Scope {
    /// Look up a parameter/constant by name, borrowing its already-evaluated value.
    fn lookup(&self, name: &str) -> Option<&EvalResult>;

    /// Resolve a qualified enumeration class name (e.g. `CDL.Types.SimpleController`) to its id.
    fn enum_class(&self, _qualified: &str) -> Option<EnumClassId> {
        None
    }

    /// Resolve an enumeration literal within a class to its 1-based ordinal.
    fn enum_ordinal(&self, _class: EnumClassId, _literal: &str) -> Option<u32> {
        None
    }
}

/// A typed expression error (never a panic; R10/R11). Array-shaped variants (`IndexOutOfBounds`,
/// `ShapeMismatch`, …) from `02` §7.4 are reserved via `#[non_exhaustive]`.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
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
    /// A domain violation, e.g. `sqrt` of a negative value (R10.4) or integer overflow.
    #[error("domain error: {0}")]
    DomainError(&'static str),
    /// Division by zero in `div`/`mod`/`rem`.
    #[error("division by zero")]
    DivisionByZero,
    /// An operator or built-in was applied to an operand of the wrong type (no implicit coercion).
    #[error("type error: expected {expected}, found {found}")]
    TypeError {
        /// The operand type the operator/built-in required.
        expected: &'static str,
        /// The operand type actually supplied.
        found: &'static str,
    },
}

/// Parse opaque CDL binding text into an [`ExprAst`], rejecting out-of-subset constructs (R9).
///
/// # Errors
/// Returns [`ExprError::Parse`] on malformed input and [`ExprError::UnsupportedFunction`] for a
/// function outside the §7.7.2 scalar set (array constructs are reported as parse errors until
/// implemented).
pub fn parse(text: &str) -> Result<ExprAst, ExprError> {
    parse::parse(text)
}

/// Evaluate an [`ExprAst`] to a ground value against a [`Scope`]. Total: never panics, always
/// returns `Ok` or a typed [`ExprError`]; deterministic.
///
/// # Errors
/// Returns a typed [`ExprError`] on any evaluation failure (unknown identifier, type mismatch,
/// domain violation, or division by zero).
pub fn eval(ast: &ExprAst, scope: &dyn Scope) -> Result<EvalResult, ExprError> {
    eval::eval(ast, scope)
}

/// Convenience for the flattener: [`parse()`] then [`eval()`] in one shot.
///
/// # Errors
/// Returns a typed [`ExprError`] on parse or evaluation failure.
pub fn eval_str(text: &str, scope: &dyn Scope) -> Result<EvalResult, ExprError> {
    eval(&parse(text)?, scope)
}
