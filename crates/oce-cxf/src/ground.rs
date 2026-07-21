//! Ground-mode value grounding (doc 04 §8): turn a parsed
//! [`CxfValue`] parameter/constant binding into a ground [`oce_model::Value`].
//!
//! Three input shapes (verified against `dto::CxfValue`):
//! - bare JSON literals (`Bool`/`Int`/`Float`) ground **literal-natural** to the matching `Value`
//!   variant. A bare `Int` feeding a Real-typed parameter with no `isOfDataType` is *not* re-typed
//!   here; the block constructor owns any accepted Int→Real parameter promotion.
//! - typed XSD literals (`{"@value","@type"}`) parse the lexical form per the datatype suffix.
//! - `Expr(string)` is handed to [`oce_expr::eval_str`] against a [`ParamScope`] of the instance's
//!   already-ground parameters (so a later binding may reference an earlier one).
//!
//! Every failure path is a typed [`GroundErr`] — **never a panic, never an `unwrap`** on
//! input-derived text. The resolver maps a `GroundErr` to a
//! [`oce_diag::DiagCode::GroundingFailed`] diagnostic.

use std::fmt;
use std::sync::Arc;

use oce_expr::{EvalResult, Scope};
use oce_model::{EnumClassId, Value, enum_class_id, enum_member_ordinal, g36_integer_constant};

use crate::dto::CxfValue;

/// A typed grounding failure (mapped by the resolver to `GroundingFailed`). Never panics.
#[derive(Debug)]
pub(crate) enum GroundErr {
    /// A typed literal's lexical form did not parse as its datatype (e.g. `i64` overflow, NaN-garbage).
    BadLiteral {
        /// The XSD datatype IRI as written.
        datatype: String,
        /// The lexical `@value` that failed to parse.
        lexical: String,
        /// The Rust type we tried to parse into (`"Real"`/`"Integer"`/`"Boolean"`).
        want: &'static str,
    },
    /// The `@type` XSD datatype IRI was not one this engine recognizes.
    UnknownDatatype(String),
    /// An `Expr` binding failed to evaluate to a ground value (unbound symbol, type error, …).
    Expr(oce_expr::ExprError),
    /// The binding did not ground to a scalar — an `Expr` that evaluated to an array (legal only
    /// on an `isArray` parameter, which the resolver expands in `arrays.rs` before reaching here)
    /// or a `List` (array) value on a non-array parameter node.
    NonScalar,
}

impl fmt::Display for GroundErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GroundErr::BadLiteral {
                datatype,
                lexical,
                want,
            } => write!(
                f,
                "typed literal {lexical:?} (datatype {datatype}) is not a valid {want}"
            ),
            GroundErr::UnknownDatatype(dt) => write!(f, "unknown XSD datatype {dt}"),
            GroundErr::Expr(e) => write!(f, "expression binding did not ground: {e}"),
            GroundErr::NonScalar => f.write_str("binding did not ground to a scalar value"),
        }
    }
}

/// The fragment of an XSD datatype IRI after the last `#` or `/` (e.g.
/// `http://www.w3.org/2001/XMLSchema#double` → `double`). Total; never panics.
fn datatype_suffix(datatype: &str) -> &str {
    datatype.rsplit(['#', '/']).next().unwrap_or(datatype)
}

/// Ground one [`CxfValue`] to an [`oce_model::Value`] against `scope` (only `Expr` consults it).
///
/// # Errors
/// Returns a [`GroundErr`] on any parse / evaluation failure — never panics.
pub(crate) fn ground_value(value: &CxfValue, scope: &dyn Scope) -> Result<Value, GroundErr> {
    match value {
        CxfValue::Bool(b) => Ok(Value::Boolean(*b)),
        CxfValue::Int(i) => Ok(Value::Integer(*i)),
        CxfValue::Float(f) => Ok(Value::Real(*f)),
        CxfValue::Typed {
            value, datatype, ..
        } => ground_typed(value, datatype),
        CxfValue::Expr(text) => {
            if let Some(value) = g36_integer_constant(text) {
                return Ok(Value::Integer(value));
            }
            match oce_expr::eval_str(text, scope) {
                // `EvalResult` is `#[non_exhaustive]`; the wildcard rejects every non-scalar
                // result — today an `Array` on a scalar parameter (array parameters never reach
                // this: the resolver expands them in `arrays.rs` first).
                Ok(EvalResult::Scalar(v)) => Ok(v),
                Ok(_) => Err(GroundErr::NonScalar),
                Err(e) => Err(GroundErr::Expr(e)),
            }
        }
        // A `List` is an array value. The resolver decodes an array-parameter's `List` per-element
        // BEFORE calling this — so a `List` reaching here is an array value on a
        // non-array (scalar) parameter node: a malformed binding, reported as a typed failure.
        CxfValue::List(_) => Err(GroundErr::NonScalar),
    }
}

/// Parse a typed XSD literal `{@value, @type}` to a `Value` by its datatype suffix.
fn ground_typed(lexical: &str, datatype: &str) -> Result<Value, GroundErr> {
    let bad = |want: &'static str| GroundErr::BadLiteral {
        datatype: datatype.to_owned(),
        lexical: lexical.to_owned(),
        want,
    };
    match datatype_suffix(datatype) {
        // `double`/`float`/`decimal` → Real. `f64::from_str` faithfully accepts `inf`/`nan` (and
        // signed forms); we ground them as-is (a faithful lowering). Rejecting non-finite parameter
        // values is a validation/semantics policy, not the resolver's job — see test below.
        "double" | "float" | "decimal" => lexical
            .parse::<f64>()
            .map(Value::Real)
            .map_err(|_| bad("Real")),
        // `integer`/`int`/`long` → Integer. Out-of-`i64` lexicals (overflow) are a typed error,
        // NOT a panic (hit the type-domain boundary on purpose).
        "integer" | "int" | "long" => lexical
            .parse::<i64>()
            .map(Value::Integer)
            .map_err(|_| bad("Integer")),
        "boolean" => lexical
            .parse::<bool>()
            .map(Value::Boolean)
            .map_err(|_| bad("Boolean")),
        _ => Err(GroundErr::UnknownDatatype(datatype.to_owned())),
    }
}

/// A read-only [`Scope`] over an instance's already-ground parameters, for grounding `Expr`
/// bindings that reference sibling parameters. Built incrementally by the resolver (a binding sees
/// only the parameters grounded *before* it, in declaration order). Pure and total (R11).
pub(crate) struct ParamScope<'a> {
    entries: &'a [(Arc<str>, EvalResult)],
}

impl<'a> ParamScope<'a> {
    /// A scope over the given (name, value) entries (latest-wins on a duplicate name).
    pub(crate) fn new(entries: &'a [(Arc<str>, EvalResult)]) -> Self {
        Self { entries }
    }
}

impl Scope for ParamScope<'_> {
    fn lookup(&self, name: &str) -> Option<&EvalResult> {
        // Reverse so a later binding of the same name shadows an earlier one. Lookup-only; the
        // iteration order here never affects model id/connection ordering (it returns a value, not order).
        self.entries
            .iter()
            .rev()
            .find(|(n, _)| n.as_ref() == name)
            .map(|(_, v)| v)
    }

    fn enum_class(&self, qualified: &str) -> Option<EnumClassId> {
        enum_class_id(qualified)
    }

    fn enum_ordinal(&self, class: EnumClassId, literal: &str) -> Option<u32> {
        enum_member_ordinal(class, literal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An empty scope for grounding literals (which never consult it).
    fn no_scope() -> ParamScope<'static> {
        ParamScope::new(&[])
    }

    #[test]
    fn bare_literals_ground_literal_natural() {
        assert!(
            ground_value(&CxfValue::Bool(true), &no_scope())
                .unwrap()
                .bit_eq(&Value::Boolean(true))
        );
        assert!(
            ground_value(&CxfValue::Int(42), &no_scope())
                .unwrap()
                .bit_eq(&Value::Integer(42))
        );
        assert!(
            ground_value(&CxfValue::Float(1.5), &no_scope())
                .unwrap()
                .bit_eq(&Value::Real(1.5))
        );
        // The minimal_loop del.y_start case: a bare Int grounds to Integer, NOT re-typed to Real.
        assert!(
            ground_value(&CxfValue::Int(0), &no_scope())
                .unwrap()
                .bit_eq(&Value::Integer(0))
        );
    }

    fn typed(value: &str, datatype: &str) -> CxfValue {
        CxfValue::Typed {
            value: value.to_owned(),
            datatype: datatype.to_owned(),
            extra: std::collections::BTreeMap::new(),
        }
    }

    #[test]
    fn typed_double_and_integer_and_boolean() {
        let xsd = |s: &str| format!("http://www.w3.org/2001/XMLSchema#{s}");
        assert!(
            ground_value(&typed("2.0", &xsd("double")), &no_scope())
                .unwrap()
                .bit_eq(&Value::Real(2.0))
        );
        assert!(
            ground_value(&typed("0.5", &xsd("double")), &no_scope())
                .unwrap()
                .bit_eq(&Value::Real(0.5))
        );
        assert!(
            ground_value(&typed("-7", &xsd("integer")), &no_scope())
                .unwrap()
                .bit_eq(&Value::Integer(-7))
        );
        assert!(
            ground_value(&typed("true", &xsd("boolean")), &no_scope())
                .unwrap()
                .bit_eq(&Value::Boolean(true))
        );
    }

    #[test]
    fn typed_integer_overflow_is_typed_error_not_panic() {
        // Type-domain boundary: a lexical past i64 must be GroundingFailed, not a panic.
        let r = ground_value(
            &typed(
                "99999999999999999999999",
                "http://www.w3.org/2001/XMLSchema#integer",
            ),
            &no_scope(),
        );
        assert!(matches!(
            r,
            Err(GroundErr::BadLiteral {
                want: "Integer",
                ..
            })
        ));
    }

    #[test]
    fn typed_double_garbage_is_typed_error() {
        let r = ground_value(
            &typed("not-a-number", "http://www.w3.org/2001/XMLSchema#double"),
            &no_scope(),
        );
        assert!(matches!(r, Err(GroundErr::BadLiteral { want: "Real", .. })));
    }

    #[test]
    fn typed_double_nan_literal_grounds_faithfully_to_nan_real() {
        // DECIDED + TESTED behavior (TESTING.md non-finite rule): a `#double` lexical of "NaN"
        // parses (f64::from_str accepts it) and grounds to Real(NaN) — a faithful lowering.
        // Rejecting non-finite parameter values is a validation/semantics policy, not the resolver's.
        let v = ground_value(
            &typed("NaN", "http://www.w3.org/2001/XMLSchema#double"),
            &no_scope(),
        )
        .unwrap();
        assert!(
            v.bit_eq(&Value::Real(f64::NAN)),
            "NaN literal grounds to Real(NaN) by bits"
        );
        // And infinity likewise (faithful).
        let inf = ground_value(
            &typed("inf", "http://www.w3.org/2001/XMLSchema#double"),
            &no_scope(),
        )
        .unwrap();
        assert!(inf.bit_eq(&Value::Real(f64::INFINITY)));
    }

    #[test]
    fn unknown_datatype_is_typed_error() {
        let r = ground_value(&typed("x", "http://example.org#widget"), &no_scope());
        assert!(matches!(r, Err(GroundErr::UnknownDatatype(_))));
    }

    #[test]
    fn expr_binding_resolves_sibling_then_fails_when_unbound() {
        // A scope with `a = 3` (Integer); `Expr("a + 1")` grounds to Integer(4).
        let entries = vec![(Arc::from("a"), EvalResult::Scalar(Value::Integer(3)))];
        let scope = ParamScope::new(&entries);
        let v = ground_value(&CxfValue::Expr("a + 1".to_owned()), &scope).unwrap();
        assert!(v.bit_eq(&Value::Integer(4)));
        // An unbound symbol is a typed grounding failure, never a panic.
        let r = ground_value(
            &CxfValue::Expr("undefined_param + 1".to_owned()),
            &no_scope(),
        );
        assert!(matches!(r, Err(GroundErr::Expr(_))));
    }

    #[test]
    fn expr_evaluating_to_array_on_scalar_binding_is_non_scalar() {
        // Regression guard for the `Ok(_)` wildcard above: now that `EvalResult::Array` is a live
        // variant, an array-evaluating Expr on a SCALAR (non-array) parameter must still be the
        // typed NonScalar rejection, never a silent acceptance or a panic.
        for text in ["{1, 2}", "1:3", "fill(1.0, 2)"] {
            let r = ground_value(&CxfValue::Expr(text.to_owned()), &no_scope());
            assert!(
                matches!(r, Err(GroundErr::NonScalar)),
                "{text:?} on a scalar binding must be NonScalar, got {r:?}"
            );
        }
    }

    #[test]
    fn expr_binding_resolves_cdl_enum_reference() {
        let v = ground_value(
            &CxfValue::Expr("Buildings.Controls.OBC.CDL.Types.ZeroTime.NY2017".to_owned()),
            &no_scope(),
        )
        .unwrap();
        assert!(v.bit_eq(&Value::Enum {
            class: EnumClassId::ZERO_TIME,
            ordinal: 11,
        }));

        let invalid = ground_value(
            &CxfValue::Expr("Buildings.Controls.OBC.CDL.Types.ZeroTime.NY2051".to_owned()),
            &no_scope(),
        );
        assert!(matches!(invalid, Err(GroundErr::Expr(_))));
    }

    #[test]
    fn expr_binding_resolves_g36_enum_reference() {
        let v = ground_value(
            &CxfValue::Expr(
                "Buildings.Controls.OBC.ASHRAE.G36.Types.VentilationStandard.California_Title_24"
                    .to_owned(),
            ),
            &no_scope(),
        )
        .unwrap();
        assert!(v.bit_eq(&Value::Enum {
            class: EnumClassId::G36_VENTILATION_STANDARD,
            ordinal: 2,
        }));

        let invalid = ground_value(
            &CxfValue::Expr(
                "Buildings.Controls.OBC.ASHRAE.G36.Types.VentilationStandard.Title_25".to_owned(),
            ),
            &no_scope(),
        );
        assert!(matches!(invalid, Err(GroundErr::Expr(_))));
    }

    #[test]
    fn expr_binding_resolves_g36_integer_constants() {
        let occupied = ground_value(
            &CxfValue::Expr(
                "Buildings.Controls.OBC.ASHRAE.G36.Types.OperationModes.occupied".to_owned(),
            ),
            &no_scope(),
        )
        .unwrap();
        assert!(occupied.bit_eq(&Value::Integer(1)));

        let cooling = ground_value(
            &CxfValue::Expr(
                "Buildings.Controls.OBC.ASHRAE.G36.Types.ZoneStates.cooling".to_owned(),
            ),
            &no_scope(),
        )
        .unwrap();
        assert!(cooling.bit_eq(&Value::Integer(3)));
    }

    #[test]
    fn datatype_suffix_handles_hash_slash_and_bare() {
        assert_eq!(
            datatype_suffix("http://www.w3.org/2001/XMLSchema#double"),
            "double"
        );
        assert_eq!(
            datatype_suffix("http://example.org/types/integer"),
            "integer"
        );
        assert_eq!(datatype_suffix("boolean"), "boolean");
    }
}
