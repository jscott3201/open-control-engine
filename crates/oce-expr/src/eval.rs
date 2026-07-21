//! Tree-walking evaluator for the scalar CDL binding subset (`02` §7).
//!
//! Pure and total: every path returns `Ok` or a typed [`ExprError`] — never a panic, never an
//! `unwrap` on a value. Integer arithmetic is checked (overflow → [`ExprError::DomainError`]
//! rather than a debug-mode panic or a release-mode silent wrap). CDL numeric promotion (§7.1):
//! `Integer op Integer → Integer` for `+ - *`; `/` is always `Real`; any `Real` operand promotes.
//!
//! [`eval_node`] is the scalar-or-array entry point: array constructs route to the
//! [`crate::eval_array`] module, array-shaped built-in calls to
//! [`crate::eval_array_builtins`], everything else through [`eval_scalar`]. In scalar operand
//! position (operators, scalar built-in arguments) an array is a typed
//! [`ExprError::TypeError`] — element-wise operators are not in the subset.

use oce_model::{
    Value,
    determinism::{canonicalize_real, det_max, det_min},
};

use crate::{BinOp, Builtin, BuiltinConst, EvalResult, ExprAst, ExprError, Scope, UnOp};

/// A numeric operand — Integer or Real — extracted from a [`Value`] for arithmetic.
#[derive(Clone, Copy)]
enum Num {
    /// An integer operand.
    I(i64),
    /// A real operand.
    R(f64),
}

impl Num {
    fn as_f64(self) -> f64 {
        match self {
            Num::I(v) => v as f64,
            Num::R(v) => v,
        }
    }
}

/// The static type name of a [`Value`], for [`ExprError::TypeError`] reporting.
pub(crate) fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Real(_) => "Real",
        Value::Integer(_) => "Integer",
        Value::Boolean(_) => "Boolean",
        Value::String(_) => "String",
        Value::Enum { .. } => "Enumeration",
    }
}

/// Extract a numeric operand, or a `TypeError` expecting a number.
fn num_of(v: &Value) -> Result<Num, ExprError> {
    match v {
        Value::Real(r) => Ok(Num::R(*r)),
        Value::Integer(i) => Ok(Num::I(*i)),
        other => Err(ExprError::TypeError {
            expected: "Real or Integer",
            found: type_name(other),
        }),
    }
}

/// Extract a boolean operand, or a `TypeError` expecting a Boolean.
fn bool_of(v: &Value) -> Result<bool, ExprError> {
    match v {
        Value::Boolean(b) => Ok(*b),
        other => Err(ExprError::TypeError {
            expected: "Boolean",
            found: type_name(other),
        }),
    }
}

/// Ground value of a named CDL constant (Buildings `CDL.Constants`; see [`BuiltinConst`]).
fn const_value(c: BuiltinConst) -> f64 {
    match c {
        // 2·asin(1.0) is π to f64; use the library constant for bit-exactness.
        BuiltinConst::Pi => std::f64::consts::PI,
        BuiltinConst::Eps => 1e-15,
        BuiltinConst::Small => 1e-37,
        BuiltinConst::Inf => f64::MAX,
    }
}

/// The single Real canonicalization choke point: every `Real` this crate produces — scalar or
/// array element — passes through here (NaN → the canonical bit pattern; `-0.0` preserved).
pub(crate) fn real(y: f64) -> Value {
    Value::Real(canonicalize_real(y))
}

/// Re-canonicalize a value read back from a [`Scope`] — the scope owner may have stored
/// arbitrary NaN bits.
pub(crate) fn canonicalize_value(v: &Value) -> Value {
    match v {
        Value::Real(r) => real(*r),
        other => other.clone(),
    }
}

/// Evaluate `ast` to an [`EvalResult`].
pub(crate) fn eval(ast: &ExprAst, scope: &dyn Scope) -> Result<EvalResult, ExprError> {
    eval_node(ast, scope)
}

/// Evaluate `ast` to a scalar-or-array [`EvalResult`]. Array constructs route to
/// [`crate::eval_array`]; a built-in call may return either shape ([`eval_call`]); an
/// identifier yields whatever shape the scope holds (arrays are cloned and re-canonicalized,
/// like the scalar read); everything else is scalar.
pub(crate) fn eval_node(ast: &ExprAst, scope: &dyn Scope) -> Result<EvalResult, ExprError> {
    match ast {
        ExprAst::ArrayLit(elems) => crate::eval_array::eval_array_literal(elems, scope),
        ExprAst::Range { start, step, stop } => {
            crate::eval_array::eval_range(start, step.as_deref(), stop, scope)
        }
        ExprAst::Call(b, args) => eval_call(*b, args, scope),
        ExprAst::Ident(name) => match scope.lookup(name) {
            Some(EvalResult::Scalar(v)) => Ok(EvalResult::Scalar(canonicalize_value(v))),
            Some(EvalResult::Array(a)) => Ok(EvalResult::Array(a.canonicalized_clone())),
            None => Err(ExprError::UnknownIdent(name.to_string())),
        },
        other => Ok(EvalResult::Scalar(eval_scalar(other, scope)?)),
    }
}

/// Unwrap a scalar result; an array in scalar position is a typed error (element-wise
/// operators are not in the subset).
fn expect_scalar(r: EvalResult) -> Result<Value, ExprError> {
    match r {
        EvalResult::Scalar(v) => Ok(v),
        EvalResult::Array(_) => Err(ExprError::TypeError {
            expected: "a scalar operand",
            found: "array",
        }),
    }
}

/// Evaluate `ast` to a ground scalar [`Value`]. Total over every AST form: constructs that can
/// produce arrays (identifier, literal, range, built-in call) go through [`eval_node`] and
/// reject an array result with a typed error.
pub(crate) fn eval_scalar(ast: &ExprAst, scope: &dyn Scope) -> Result<Value, ExprError> {
    match ast {
        ExprAst::Real(r) => Ok(real(*r)),
        ExprAst::Int(i) => Ok(Value::Integer(*i)),
        ExprAst::Bool(b) => Ok(Value::Boolean(*b)),
        ExprAst::Str(s) => Ok(Value::String(s.clone())),
        ExprAst::Const(c) => Ok(real(const_value(*c))),
        ExprAst::EnumRef(name) => eval_enum_ref(name, scope),
        ExprAst::Unary(op, e) => eval_unary(*op, &eval_scalar(e, scope)?),
        ExprAst::Binary(op, a, b) => {
            eval_binary(*op, &eval_scalar(a, scope)?, &eval_scalar(b, scope)?)
        }
        ExprAst::Call(..) | ExprAst::Ident(_) | ExprAst::ArrayLit(_) | ExprAst::Range { .. } => {
            expect_scalar(eval_node(ast, scope)?)
        }
    }
}

fn eval_enum_ref(name: &str, scope: &dyn Scope) -> Result<Value, ExprError> {
    let (class_name, literal) = name
        .rsplit_once('.')
        .ok_or_else(|| ExprError::UnknownIdent(name.to_owned()))?;
    let class = scope
        .enum_class(class_name)
        .ok_or_else(|| ExprError::UnknownIdent(class_name.to_owned()))?;
    let ordinal = scope
        .enum_ordinal(class, literal)
        .ok_or_else(|| ExprError::UnknownIdent(name.to_owned()))?;
    Ok(Value::Enum { class, ordinal })
}

fn eval_unary(op: UnOp, v: &Value) -> Result<Value, ExprError> {
    match op {
        UnOp::Neg => match num_of(v)? {
            Num::I(i) => Ok(Value::Integer(
                i.checked_neg()
                    .ok_or(ExprError::DomainError("integer overflow in unary minus"))?,
            )),
            Num::R(r) => Ok(real(-r)),
        },
        UnOp::Not => Ok(Value::Boolean(!bool_of(v)?)),
    }
}

fn eval_binary(op: BinOp, a: &Value, b: &Value) -> Result<Value, ExprError> {
    match op {
        BinOp::Add | BinOp::Sub | BinOp::Mul => arith(op, num_of(a)?, num_of(b)?),
        // The `/` operator always yields Real (§6.1); IEEE semantics for a zero divisor (the
        // `div`/`mod`/`rem` built-ins are the ones that raise DivisionByZero).
        BinOp::Div => Ok(real(num_of(a)?.as_f64() / num_of(b)?.as_f64())),
        BinOp::Gt | BinOp::Ge | BinOp::Lt | BinOp::Le => {
            Ok(Value::Boolean(compare_rel(op, num_of(a)?, num_of(b)?)))
        }
        BinOp::Eq | BinOp::Ne => {
            let equal = values_equal(a, b)?;
            Ok(Value::Boolean(if op == BinOp::Eq { equal } else { !equal }))
        }
        BinOp::And => Ok(Value::Boolean(bool_of(a)? && bool_of(b)?)),
        BinOp::Or => Ok(Value::Boolean(bool_of(a)? || bool_of(b)?)),
    }
}

/// Relational comparison (`> >= < <=`). Integer/Integer compares **exactly** as `i64`; any Real
/// operand promotes both to `f64` (an `i64 as f64` of both operands would lose precision above
/// 2^53, silently mis-ordering large integers; the large-integer regression pins this).
fn compare_rel(op: BinOp, a: Num, b: Num) -> bool {
    match (a, b) {
        (Num::I(x), Num::I(y)) => match op {
            BinOp::Gt => x > y,
            BinOp::Ge => x >= y,
            BinOp::Lt => x < y,
            _ => x <= y,
        },
        _ => {
            let (x, y) = (a.as_f64(), b.as_f64());
            match op {
                BinOp::Gt => x > y,
                BinOp::Ge => x >= y,
                BinOp::Lt => x < y,
                _ => x <= y,
            }
        }
    }
}

/// `+ - *` with CDL promotion: `Integer op Integer → Integer` (checked); any Real → Real.
fn arith(op: BinOp, a: Num, b: Num) -> Result<Value, ExprError> {
    match (a, b) {
        (Num::I(x), Num::I(y)) => {
            let r = match op {
                BinOp::Add => x.checked_add(y),
                BinOp::Sub => x.checked_sub(y),
                _ => x.checked_mul(y),
            };
            Ok(Value::Integer(
                r.ok_or(ExprError::DomainError("integer overflow"))?,
            ))
        }
        _ => {
            let (x, y) = (a.as_f64(), b.as_f64());
            Ok(real(match op {
                BinOp::Add => x + y,
                BinOp::Sub => x - y,
                _ => x * y,
            }))
        }
    }
}

/// Equality for `==`/`<>`. Integer/Integer and String/String compare exactly; Booleans compare
/// directly; a mixed numeric pair (Real with Integer or Real) compares by promoted `f64` value;
/// any other pairing is a `TypeError`. Exact float comparison is intended — these are ground
/// binding values, not measured signals. Enumeration equality lands with enum-reference grounding.
#[allow(clippy::float_cmp)]
fn values_equal(a: &Value, b: &Value) -> Result<bool, ExprError> {
    match (a, b) {
        (Value::Integer(x), Value::Integer(y)) => Ok(x == y),
        (Value::Boolean(x), Value::Boolean(y)) => Ok(x == y),
        (Value::String(x), Value::String(y)) => Ok(x == y),
        (
            Value::Enum {
                class: cx,
                ordinal: ox,
            },
            Value::Enum {
                class: cy,
                ordinal: oy,
            },
        ) => Ok(cx == cy && ox == oy),
        (Value::Real(_) | Value::Integer(_), Value::Real(_) | Value::Integer(_)) => {
            Ok(num_of(a)?.as_f64() == num_of(b)?.as_f64())
        }
        _ => Err(ExprError::TypeError {
            expected: "two numbers, two Booleans, two Strings, or two Enums",
            found: type_name(a),
        }),
    }
}

/// Evaluate a built-in call, dispatching on the built-in's shape. Array-shaped built-ins
/// receive their arguments as scalar-or-array [`EvalResult`]s (via [`eval_node`]) and route to
/// [`crate::eval_array_builtins`]; scalar built-ins keep the scalar-only argument path, so an
/// array argument to one is the same typed [`ExprError::TypeError`] as before. Both paths
/// evaluate arguments left to right.
fn eval_call(b: Builtin, args: &[ExprAst], scope: &dyn Scope) -> Result<EvalResult, ExprError> {
    match b {
        Builtin::Sum
        | Builtin::Size
        | Builtin::Fill
        | Builtin::Cat
        | Builtin::MinArr
        | Builtin::MaxArr => {
            let vals: Vec<EvalResult> = args
                .iter()
                .map(|a| eval_node(a, scope))
                .collect::<Result<_, _>>()?;
            crate::eval_array_builtins::eval_array_builtin(b, &vals)
        }
        _ => {
            let vals: Vec<Value> = args
                .iter()
                .map(|a| eval_scalar(a, scope))
                .collect::<Result<_, _>>()?;
            eval_scalar_call(b, &vals).map(EvalResult::Scalar)
        }
    }
}

/// Evaluate a scalar built-in over already-evaluated scalar arguments. Arity is already checked
/// by the parser; the slice patterns below keep evaluation panic-free even so (a mismatch
/// yields a typed error, never an index panic).
fn eval_scalar_call(b: Builtin, vals: &[Value]) -> Result<Value, ExprError> {
    match (b, vals) {
        (Builtin::Abs, [v]) => builtin_abs(v),
        (Builtin::Sign, [v]) => Ok(Value::Integer(builtin_sign(num_of(v)?))),
        (Builtin::Sqrt, [v]) => builtin_sqrt(num_of(v)?),
        (Builtin::Floor, [v]) => Ok(real(num_of(v)?.as_f64().floor())),
        (Builtin::Ceil, [v]) => Ok(real(num_of(v)?.as_f64().ceil())),
        (Builtin::Integer, [v]) => Ok(Value::Integer(builtin_integer(num_of(v)?))),
        (Builtin::Div, [x, y]) => builtin_div(num_of(x)?, num_of(y)?),
        (Builtin::Mod, [x, y]) => builtin_mod(num_of(x)?, num_of(y)?),
        (Builtin::Rem, [x, y]) => builtin_rem(num_of(x)?, num_of(y)?),
        (Builtin::MinScalar, [x, y]) => Ok(min_max(num_of(x)?, num_of(y)?, true)),
        (Builtin::MaxScalar, [x, y]) => Ok(min_max(num_of(x)?, num_of(y)?, false)),
        _ => Err(ExprError::DomainError(
            "built-in called with the wrong arity",
        )),
    }
}

fn builtin_abs(v: &Value) -> Result<Value, ExprError> {
    match num_of(v)? {
        Num::I(i) => Ok(Value::Integer(
            i.checked_abs()
                .ok_or(ExprError::DomainError("integer overflow in abs"))?,
        )),
        Num::R(r) => Ok(real(r.abs())),
    }
}

/// `sign(v)` is **always Integer** (R10.3a) and `sign(0) == 0` — note `f64::signum(0.0) == 1.0`,
/// so the zero case is handled explicitly rather than via `signum`.
fn builtin_sign(n: Num) -> i64 {
    let f = n.as_f64();
    if f > 0.0 {
        1
    } else if f < 0.0 {
        -1
    } else {
        0
    }
}

fn builtin_sqrt(n: Num) -> Result<Value, ExprError> {
    let f = n.as_f64();
    if f < 0.0 {
        Err(ExprError::DomainError("sqrt of a negative value"))
    } else {
        Ok(real(f.sqrt()))
    }
}

/// `integer(x)` = `floor(x)` cast to Integer (R10.1). `as i64` saturates (never panics) on
/// out-of-range or NaN.
fn builtin_integer(n: Num) -> i64 {
    match n {
        Num::I(i) => i,
        Num::R(r) => r.floor() as i64,
    }
}

/// `div(x, y)` — truncate toward zero; Integer iff both operands Integer (R10.1/R10.3b).
fn builtin_div(x: Num, y: Num) -> Result<Value, ExprError> {
    match (x, y) {
        (Num::I(a), Num::I(b)) => {
            if b == 0 {
                return Err(ExprError::DivisionByZero);
            }
            a.checked_div(b)
                .map(Value::Integer)
                .ok_or(ExprError::DomainError("integer overflow in div"))
        }
        _ => {
            let (a, b) = (x.as_f64(), y.as_f64());
            if b == 0.0 {
                return Err(ExprError::DivisionByZero);
            }
            Ok(real((a / b).trunc()))
        }
    }
}

/// `mod(x, y)` = `x - floor(x/y)*y`, result has the sign of `y` (R10.2).
fn builtin_mod(x: Num, y: Num) -> Result<Value, ExprError> {
    match (x, y) {
        (Num::I(a), Num::I(b)) => {
            if b == 0 {
                return Err(ExprError::DivisionByZero);
            }
            // The intermediate `q*b` can overflow `i64` even when the true `mod` fits (e.g.
            // `mod(i64::MIN, 3)`), so compute in `i128`; `|result| < |b| <= i64::MAX` always
            // fits back into `i64`.
            let (a, b) = (i128::from(a), i128::from(b));
            let q = ifloordiv_i128(a, b);
            Ok(Value::Integer((a - q * b) as i64))
        }
        _ => {
            let (a, b) = (x.as_f64(), y.as_f64());
            if b == 0.0 {
                return Err(ExprError::DivisionByZero);
            }
            Ok(real(a - (a / b).floor() * b))
        }
    }
}

/// `rem(x, y)` = `x - div(x,y)*y`, result has the sign of `x`, with `div(x,y)*y + rem == x`
/// (R10.2). For integers this is Rust's `%` (dividend-signed remainder).
fn builtin_rem(x: Num, y: Num) -> Result<Value, ExprError> {
    match (x, y) {
        (Num::I(a), Num::I(b)) => {
            if b == 0 {
                return Err(ExprError::DivisionByZero);
            }
            a.checked_rem(b)
                .map(Value::Integer)
                .ok_or(ExprError::DomainError("integer overflow in rem"))
        }
        _ => {
            let (a, b) = (x.as_f64(), y.as_f64());
            if b == 0.0 {
                return Err(ExprError::DivisionByZero);
            }
            Ok(real(a - (a / b).trunc() * b))
        }
    }
}

/// Integer floor division `floor(x / y)` in `i128` (caller guarantees `y != 0`). Differs from
/// the `/` truncate-toward-zero quotient when the operands have opposite signs and the division
/// is inexact. `i128` avoids any intermediate overflow for operands derived from `i64` values,
/// including a full-width range span such as `i64::MAX - i64::MIN`.
pub(crate) fn ifloordiv_i128(x: i128, y: i128) -> i128 {
    let q = x / y;
    let r = x % y;
    if r != 0 && ((r < 0) != (y < 0)) {
        q - 1
    } else {
        q
    }
}

/// Scalar `min`/`max`: Integer iff both Integer, else Real (promoted). `want_min` selects min.
///
/// A single NaN operand (only reachable via a scope identifier already bound to `Real(NaN)`) is
/// dropped; a NaN-only Real result is canonicalized for bit-stable determinism. The same Real-output
/// canonicalization applies to NaN flowing through arithmetic and `floor`/`ceil`; ground binding
/// values are not normally NaN.
fn min_max(x: Num, y: Num, want_min: bool) -> Value {
    match (x, y) {
        (Num::I(a), Num::I(b)) => Value::Integer(if want_min { a.min(b) } else { a.max(b) }),
        _ => {
            let (a, b) = (x.as_f64(), y.as_f64());
            if want_min {
                real(det_min(a, b))
            } else {
                real(det_max(a, b))
            }
        }
    }
}
