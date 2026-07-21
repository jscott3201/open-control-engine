#![forbid(unsafe_code)]
//! `oce-expr` — the CDL §7.7.2 binding-expression parser/evaluator for the Open Control
//! Engine.
//!
//! Parameter/constant *bindings* may carry a restricted, closed-world expression language
//! (literals, identifier references, arithmetic/relational/boolean operators, the §7.7.2
//! built-in function set, 1-D array literals/ranges, and — later — comprehensions). `oce-expr`
//! parses
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
//! - **Constants** — `pi`/`eps`/`small` from Buildings `CDL.Constants`, plus the retained
//!   `inf` Modelica-alias compatibility constant (bare or `…Constants.<name>` qualified),
//!   [`BuiltinConst`].
//! - **Identifier references** — resolved against a [`Scope`] (other in-scope params/constants).
//! - **Operators** — unary `-`/`not`; binary `+ - * /`, relational `> >= < <= == <>`, `and`/`or`.
//! - **Scalar built-ins** — `abs sign sqrt div mod rem floor ceil integer min max` (2-arg
//!   `min`/`max`), with the exact CDL numeric promotion and R10.x semantics.
//!
//! # 1-D arrays
//!
//! Brace literals (`{a, b, c}`) and ranges (`a:b`, `a:step:b`) evaluate to an [`ArrayValue`]
//! carried by [`EvalResult::Array`]: homogeneous element type, flat storage, Real elements
//! NaN-canonicalized, element count capped at 2^20 (a larger construct is
//! [`ExprError::ArrayTooLarge`], checked before any allocation). An array in scalar operand
//! position is a [`ExprError::TypeError`] — element-wise operators are not in the subset.
//!
//! Still deferred: array comprehensions, `A[i]` indexing, 2-D/matrix constructors
//! (`[a, b; c, d]`), the array-shaped built-ins (`sum`/`cat`/`fill`/`size` and the array forms
//! of `min`/`max`), and the `Modelica.Math.*` alias whitelist. They appear in `02` §7.4 and are
//! reserved here via `#[non_exhaustive]` so adding them is not a breaking change; encountering
//! them in a binding today is a typed error, never a panic.
//!
//! The public surface below (`parse`/`eval`/`eval_str`, [`ExprAst`], [`Scope`], [`EvalResult`],
//! [`ExprError`]) is the **stable contract** `oce-flatten` binds to.

use std::sync::Arc;

use oce_model::{EnumClassId, Value, ValueType};

mod eval;
mod eval_array;
#[cfg(test)]
mod eval_array_tests;
mod parse;
#[cfg(test)]
mod tests;

/// A parsed binding expression. Unknown functions are **not representable** — they are
/// rejected during parse/resolve (R9). Comprehension and indexing constructs (`02` §7.4)
/// remain reserved via `#[non_exhaustive]`.
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
    /// A qualified enum reference such as `CDL.Types.ZeroTime.NY2017`.
    EnumRef(Arc<str>),
    /// A unary operator applied to one operand.
    Unary(UnOp, Box<ExprAst>),
    /// A binary operator applied to two operands.
    Binary(BinOp, Box<ExprAst>, Box<ExprAst>),
    /// A call to one of the closed §7.7.2 built-in functions (R9).
    Call(Builtin, Vec<ExprAst>),
    /// A 1-D brace array literal `{a, b, c}`. `{}` is representable and evaluates to
    /// [`ExprError::EmptyArray`] — its element type is unknowable.
    ArrayLit(Vec<ExprAst>),
    /// A range `start:stop` or `start:step:stop`; `a:b` is `a:1:b`. Elements run from `start`
    /// in `step` strides while they remain on the `stop` side of the sequence (an empty range
    /// is legal).
    Range {
        /// The first element (also the anchor of the closed-form element formula).
        start: Box<ExprAst>,
        /// The stride between consecutive elements; `None` means a step of one.
        step: Option<Box<ExprAst>>,
        /// The inclusive bound: the last element never steps past it.
        stop: Box<ExprAst>,
    },
}

/// A named fold-time constant from Buildings `CDL.Constants` (§6.1), plus the retained
/// `inf` compatibility alias. Each folds to a ground `Real`.
///
/// `pi`, `eps`, and `small` track the source-verified Buildings
/// `Buildings.Controls.OBC.CDL.Constants` module. `inf` is not defined by that package; Open Control
/// Engine retains it as a closed-world `Modelica.Constants.inf` alias for compatibility with
/// existing expression bindings, and pins it to the largest finite `f64`.
///
/// Note: this `inf` is distinct from an IEEE `±∞` literal. The spelled-out `-Inf`/`+Inf` Real
/// literals of §6.1 are a CXF typed-value serialization concern parsed by `oce-cxf`, not part of
/// this expression grammar.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum BuiltinConst {
    /// `pi` — the ratio of a circle's circumference to its diameter (`2·asin(1)`, == `f64` `PI`).
    Pi,
    /// `eps` — the Buildings CDL comparison tolerance (`1e-15`).
    Eps,
    /// `small` — the Buildings CDL representability floor (`1e-37`).
    Small,
    /// `inf` — retained `Modelica.Constants.inf` alias (`f64::MAX`); not IEEE `∞`.
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

/// A fully-evaluated binding result: a scalar or a 1-D array. Further result shapes remain
/// reserved via `#[non_exhaustive]`.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum EvalResult {
    /// A scalar ground value.
    Scalar(Value),
    /// A 1-D array of ground values (a `{…}` literal or an `a:b` range).
    Array(ArrayValue),
}

/// A rectangular array of ground scalar [`Value`]s, produced by evaluating a `{…}` literal or
/// an `a:b`/`a:step:b` range.
///
/// Fields are private so the invariants cannot be broken after construction:
///
/// - `data.len()` equals the element count of the shape (`n` for [`Shape::D1`]).
/// - Every element's [`Value::value_type`] equals the array's element type — arrays are
///   homogeneous; the promotion to `Real` already happened during evaluation.
/// - Every `Real` element has passed the crate's NaN canonicalization choke point
///   ([`oce_model::determinism::canonicalize_real`]), so element bits are deterministic.
///   `-0.0` is preserved (only NaN is canonicalized).
/// - The element count never exceeds the crate array cap (2^20 = 1,048,576 elements); a
///   larger construct is rejected with [`ExprError::ArrayTooLarge`] before any allocation.
///
/// Storage is a flat, row-major `Vec<Value>` — never nested. Construction is crate-internal;
/// consumers read through the accessors.
///
/// ```
/// use oce_expr::{EvalResult, Scope, eval_str};
/// use oce_model::{Value, ValueType};
///
/// struct Empty;
/// impl Scope for Empty {
///     fn lookup(&self, _: &str) -> Option<&EvalResult> {
///         None
///     }
/// }
///
/// // `2:2:8` steps by two through the inclusive bound: four Integer elements.
/// let EvalResult::Array(a) = eval_str("2:2:8", &Empty).unwrap() else {
///     panic!("a range evaluates to an array");
/// };
/// assert_eq!(a.elem_type(), ValueType::Integer);
/// assert_eq!(a.len(), 4);
/// assert!(a.as_slice()[3].bit_eq(&Value::Integer(8)));
/// ```
#[derive(Clone, Debug)]
pub struct ArrayValue {
    /// The homogeneous type of every element in `data`.
    elem_type: ValueType,
    /// The rectangular shape; `data.len()` equals its element count.
    shape: Shape,
    /// The elements, flat and row-major.
    data: Vec<Value>,
}

impl ArrayValue {
    /// The homogeneous element type of this array.
    #[must_use]
    pub fn elem_type(&self) -> ValueType {
        self.elem_type
    }

    /// The rectangular shape. Every array this crate produces today is [`Shape::D1`].
    #[must_use]
    pub fn shape(&self) -> Shape {
        self.shape
    }

    /// Total number of elements.
    #[must_use]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// `true` when the array has no elements — a legal outcome (e.g. the empty range `5:3`).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// The elements in flat row-major order.
    #[must_use]
    pub fn as_slice(&self) -> &[Value] {
        &self.data
    }
}

/// The rectangular shape of an [`ArrayValue`].
///
/// Only [`Shape::D1`] is produced today. [`Shape::D2`] is declared for the planned 2-D subset —
/// matrix constructors are still rejected at parse — so downstream matches can be written once.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum Shape {
    /// A vector of `n` elements.
    D1(usize),
    /// A `rows × cols` matrix, stored row-major. Never constructed yet.
    D2 {
        /// Number of rows.
        rows: usize,
        /// Number of columns.
        cols: usize,
    },
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
    /// An array construct would produce more elements than the engine will allocate — a
    /// resource-limit rejection, distinct from [`ExprError::DomainError`]. The count is checked
    /// before any allocation; `count` reports the requested element count (saturated at
    /// `u128::MAX` when even `u128` cannot hold it, e.g. `0:1e-300:1`).
    #[error("array too large: {count} elements exceed the supported maximum ({max})")]
    ArrayTooLarge {
        /// The element count the construct asked for.
        count: u128,
        /// The crate's element cap (2^20).
        max: usize,
    },
    /// An empty array construct whose element type is unknowable (`{}`); fabricating a typed
    /// empty array here would risk silent mistyping. Empty *ranges* are legal — their operands
    /// carry the type.
    #[error("empty array literal has no element type")]
    EmptyArray,
}

/// Parse opaque CDL binding text into an [`ExprAst`], rejecting out-of-subset constructs (R9).
///
/// # Errors
/// Returns [`ExprError::Parse`] on malformed input and [`ExprError::UnsupportedFunction`] for a
/// function outside the §7.7.2 scalar set (the still-deferred constructs — indexing,
/// comprehensions, matrix syntax, array built-ins — are reported as parse errors).
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
