//! Ground-mode value grounding (doc 04 §8): turn a parsed
//! [`CxfValue`] parameter/constant binding into a ground [`oce_model::Value`].
//!
//! Three input shapes (verified against `dto::CxfValue`):
//! - bare JSON literals (`Bool`/`Int`/`Float`) ground **literal-natural** to the matching `Value`
//!   variant. A bare `Int` feeding a Real-typed parameter with no `isOfDataType` is *not* re-typed
//!   here; the block constructor owns any accepted Int→Real parameter promotion.
//! - typed XSD literals (`{"@value","@type"}`) parse the lexical form per the datatype suffix.
//! - `Expr(string)` is handed to [`oce_expr::eval_str`] against a [`Scope`] of the
//!   already-ground parameters visible to the binding. For a leaf *member* binding that is
//!   Step 7's incremental [`ParamScope`] — the enclosing scope chain plus earlier-declared
//!   sibling members, enclosing-first for values (see [`ParamScope`]). A composite's *own*
//!   declaration chain instead grounds as one mutual, order-independent scope in the
//!   resolver's declaration-scope mechanism (issue #240), which supplies its own `Scope` impl
//!   and hands the finished entries onward.
//!
//! Every failure path, including over-limit expression depth or size, is a typed [`GroundErr`] —
//! **never a panic, never an `unwrap`** on input-derived text. The resolver maps a `GroundErr` to a
//! [`oce_diag::DiagCode::GroundingFailed`] diagnostic, except [`GroundErr::NonFinite`], which
//! [`ground_param_value`] raises for a non-finite parameter value or expression intermediate and
//! which maps to [`oce_diag::DiagCode::NonFiniteParameter`] (issue #362).

use std::fmt;
use std::sync::Arc;

use oce_diag::Diagnostic;

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
    /// An array parameter's value did not fit its declared shape (element count, dimensionality,
    /// or a non-array result); the message is reported verbatim.
    ArrayShape(String),
    /// A parameter binding grounded to, or passed through, a non-finite Real (`NaN`, `+inf`,
    /// `-inf`): a non-finite literal, or any non-finite expression intermediate under the
    /// strict-finite evaluator (issue #362). Raised only by [`ground_param_value`]; mapped to
    /// [`oce_diag::DiagCode::NonFiniteParameter`] by [`GroundErr::diag_code`].
    NonFinite {
        /// The offending value, rendered (`NaN`, `inf` or `-inf`).
        value: &'static str,
        /// `true` when the value was an expression intermediate rather than the final value.
        intermediate: bool,
    },
}

impl GroundErr {
    /// The diagnostic code a resolver site reports this failure under: `non-finite-parameter`
    /// for [`GroundErr::NonFinite`], `grounding-failed` for everything else.
    pub(crate) fn diag_code(&self) -> oce_diag::DiagCode {
        match self {
            GroundErr::NonFinite { .. } => oce_diag::DiagCode::NonFiniteParameter,
            _ => oce_diag::DiagCode::GroundingFailed,
        }
    }
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
            GroundErr::ArrayShape(message) => f.write_str(message),
            GroundErr::NonFinite {
                value,
                intermediate: false,
            } => write!(f, "must be finite; got {value}"),
            GroundErr::NonFinite {
                value,
                intermediate: true,
            } => write!(
                f,
                "must be finite; its expression produced a non-finite intermediate ({value})"
            ),
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
    ground_with(value, scope, oce_expr::eval_str)
}

/// Ground a **parameter** binding under the engine-wide finiteness rule (issue #362).
///
/// Identical to [`ground_value`] except that an `Expr` is evaluated strict-finite
/// ([`oce_expr::eval_str_finite`] — no node may produce `NaN`/`±inf`, so `integer(1/0)`,
/// `sign(0/0)`, `min(1/0, 3)` or `p > 0` over a non-finite `p` cannot absorb one), and a grounded
/// non-finite Real literal is refused. Every resolver site that grounds a declared parameter —
/// leaf members, composite and root declarations, array elements — uses this entry; connector
/// attribute bounds keep [`ground_value`].
///
/// # Errors
/// Every [`ground_value`] error, plus [`GroundErr::NonFinite`]. Never panics.
pub(crate) fn ground_param_value(value: &CxfValue, scope: &dyn Scope) -> Result<Value, GroundErr> {
    match ground_with(value, scope, oce_expr::eval_str_finite) {
        Ok(Value::Real(v)) if !v.is_finite() => Err(GroundErr::NonFinite {
            value: non_finite_name(v),
            intermediate: false,
        }),
        Err(GroundErr::Expr(error)) => Err(expr_failure(error)),
        other => other,
    }
}

/// The diagnostic for a declared parameter that failed to ground.
///
/// A non-finite value or intermediate (issue #362) reports `non-finite-parameter` with the
/// **declaring instance** (`owner`) as subject and the parameter named in the message; every other
/// failure keeps `grounding-failed` on the parameter node, as before.
pub(crate) fn ground_failure(error: &GroundErr, parameter_iri: &str, owner: &str) -> Diagnostic {
    match error {
        GroundErr::NonFinite { .. } => Diagnostic::error(
            error.diag_code(),
            format!(
                "parameter `{}` on `{owner}` {error}",
                crate::resolve::local_name(parameter_iri)
            ),
        )
        .with_subject(owner.to_owned()),
        _ => Diagnostic::error(error.diag_code(), error.to_string())
            .with_subject(parameter_iri.to_owned()),
    }
}

/// Re-type a strict-evaluation [`oce_expr::ExprError::NonFinite`] as an intermediate
/// [`GroundErr::NonFinite`]; every other expression error stays [`GroundErr::Expr`].
pub(crate) fn expr_failure(error: oce_expr::ExprError) -> GroundErr {
    match error {
        oce_expr::ExprError::NonFinite { value } => GroundErr::NonFinite {
            value,
            intermediate: true,
        },
        other => GroundErr::Expr(other),
    }
}

fn non_finite_name(v: f64) -> &'static str {
    if v.is_nan() {
        "NaN"
    } else if v > 0.0 {
        "inf"
    } else {
        "-inf"
    }
}

type ExprEval = fn(&str, &dyn Scope) -> Result<EvalResult, oce_expr::ExprError>;

fn ground_with(value: &CxfValue, scope: &dyn Scope, eval: ExprEval) -> Result<Value, GroundErr> {
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
            match eval(text, scope) {
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
        // signed forms); `ground_value` grounds them as-is (a faithful lowering, still used for
        // connector attribute bounds). Parameter sites use `ground_param_value`, which refuses
        // them and every non-finite expression intermediate (issue #362) — see tests below.
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

/// A read-only [`Scope`] over an instance's already-ground parameters, for grounding a leaf
/// *member* `Expr` binding that references enclosing-scope or sibling parameters. Built
/// incrementally by the resolver's Step-7 member grounding (a member binding sees only the
/// members grounded *before* it, in declaration order — a member's forward reference still
/// fails). A composite's *own* declaration chain is a different level: it grounds as one
/// mutual, order-independent scope in the resolver's declaration-scope mechanism (issue #240)
/// and this type only carries the finished entries onward. Pure and total (R11).
///
/// Two views exist over one entry slice:
/// - [`ParamScope::new`] — the undivided latest-wins view: one reverse scan, so the most recently
///   grounded binding of a name wins. Array *dimension* expressions (`sizeOfDimensions`) resolve
///   on this view.
/// - [`ParamScope::with_enclosing`] — the split view Step 7 uses for member *value* expressions:
///   the enclosing region is searched first, so an enclosing binding wins over a same-named
///   sibling (issue #239); sibling-only names keep resolving against earlier-declared siblings.
///
/// Split corollary (owner-ruled): a name bound BOTH by a sibling member and the enclosing scope
/// has two readings inside one block — value expressions read the enclosing binding, dimension
/// expressions read the sibling *when the sibling is grounded earlier*: member array order still
/// decides the dimension reading (values are order-invariant under member order, dimensions are
/// not). That divergence is silent on the scalar path; on the array path an element-count
/// divergence refuses (`GroundingFailed`, both counts in the message), while a value divergence
/// with a matching count is silent, exactly like the scalar path.
pub(crate) struct ParamScope<'a> {
    entries: &'a [(Arc<str>, EvalResult)],
    /// Number of leading `entries` grounded by the enclosing scope chain; the rest are the
    /// instance's own already-ground members. `0` is the undivided view.
    enclosing: usize,
}

impl<'a> ParamScope<'a> {
    /// The undivided view over the given (name, value) entries: one reverse scan, latest-wins on
    /// a duplicate name. Dimension parsing uses this view; member value grounding uses
    /// [`ParamScope::with_enclosing`].
    pub(crate) fn new(entries: &'a [(Arc<str>, EvalResult)]) -> Self {
        Self {
            entries,
            enclosing: 0,
        }
    }

    /// The split view: `entries[..enclosing]` came from the enclosing scope chain and is searched
    /// first, so an enclosing binding wins over a same-named sibling in `entries[enclosing..]`.
    /// Each region is scanned in reverse (its most recently grounded binding wins). Total: an
    /// `enclosing` past the slice end is clamped at lookup, never a panic.
    pub(crate) fn with_enclosing(entries: &'a [(Arc<str>, EvalResult)], enclosing: usize) -> Self {
        Self { entries, enclosing }
    }
}

impl Scope for ParamScope<'_> {
    fn lookup(&self, name: &str) -> Option<&EvalResult> {
        // Enclosing region first, then siblings — an enclosing binding wins over a same-named
        // sibling (issue #239). Each region is scanned in reverse so its own most recently
        // grounded binding wins (innermost enclosing composite; latest earlier sibling). With
        // `enclosing == 0` this degenerates to one reverse scan over the whole slice — the
        // undivided latest-wins view `new` constructs. Lookup-only; the iteration order here
        // never affects model id/connection ordering (it returns a value, not order).
        let (enclosing, siblings) = self
            .entries
            .split_at(self.enclosing.min(self.entries.len()));
        enclosing
            .iter()
            .rev()
            .chain(siblings.iter().rev())
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
            ground_value(
                &typed("2.0", "http://www.w3.org/2001/XMLSchema#double"),
                &no_scope()
            )
            .unwrap()
            .bit_eq(&Value::Real(2.0))
        );
        assert!(
            ground_value(
                &typed("0.5", "http://www.w3.org/2001/XMLSchema#double"),
                &no_scope()
            )
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
        // Parameter sites refuse it through `ground_param_value` instead (issue #362).
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
    fn split_view_reads_enclosing_binding_over_same_named_sibling() {
        // Issue #239: on the split view the enclosing region wins on a name bound in both
        // regions, while a sibling-only name keeps resolving against the sibling region.
        let entries = vec![
            (Arc::from("a"), EvalResult::Scalar(Value::Integer(10))), // enclosing
            (Arc::from("a"), EvalResult::Scalar(Value::Integer(99))), // sibling
            (Arc::from("b"), EvalResult::Scalar(Value::Integer(7))),  // sibling-only
        ];
        let split = ParamScope::with_enclosing(&entries, 1);
        let both = ground_value(&CxfValue::Expr("a".to_owned()), &split).unwrap();
        assert!(
            both.bit_eq(&Value::Integer(10)),
            "the enclosing binding of `a` must win over the sibling, got {both:?}"
        );
        let sibling_only = ground_value(&CxfValue::Expr("b".to_owned()), &split).unwrap();
        assert!(
            sibling_only.bit_eq(&Value::Integer(7)),
            "a sibling-only name must still resolve, got {sibling_only:?}"
        );
    }

    #[test]
    fn innermost_enclosing_entry_wins_within_the_enclosing_region() {
        // The enclosing region's OWN scan direction: with two enclosing entries of one name —
        // outer composite first, inner composite last, chain order — the reverse scan must
        // return the innermost (last-pushed) binding. A one-entry enclosing region cannot
        // distinguish forward from reverse, so this is the case that pins the direction.
        let entries = vec![
            (Arc::from("a"), EvalResult::Scalar(Value::Integer(1))), // outer enclosing composite
            (Arc::from("a"), EvalResult::Scalar(Value::Integer(2))), // inner enclosing composite
            (Arc::from("a"), EvalResult::Scalar(Value::Integer(99))), // sibling
        ];
        let split = ParamScope::with_enclosing(&entries, 2);
        let v = ground_value(&CxfValue::Expr("a".to_owned()), &split).unwrap();
        assert!(
            v.bit_eq(&Value::Integer(2)),
            "the INNERMOST enclosing binding (last-pushed in the enclosing region) must win \
             over the outer composite's, got {v:?}"
        );
    }

    #[test]
    fn undivided_view_keeps_latest_wins_on_a_duplicate_name() {
        // `new` is the undivided view dimension parsing uses: one reverse scan, latest-wins.
        let entries = vec![
            (Arc::from("a"), EvalResult::Scalar(Value::Integer(10))),
            (Arc::from("a"), EvalResult::Scalar(Value::Integer(99))),
        ];
        let v = ground_value(&CxfValue::Expr("a".to_owned()), &ParamScope::new(&entries)).unwrap();
        assert!(
            v.bit_eq(&Value::Integer(99)),
            "the undivided view must stay latest-wins, got {v:?}"
        );
    }

    #[test]
    fn enclosing_split_past_the_slice_end_is_clamped_not_a_panic() {
        let entries = vec![(Arc::from("a"), EvalResult::Scalar(Value::Integer(3)))];
        let scope = ParamScope::with_enclosing(&entries, 5);
        let v = ground_value(&CxfValue::Expr("a".to_owned()), &scope).unwrap();
        assert!(v.bit_eq(&Value::Integer(3)));
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
    fn parameter_grounding_refuses_non_finite_literals_and_intermediates() {
        // Literal non-finite values: the final value is refused (not an intermediate).
        for (lexical, value) in [("NaN", "NaN"), ("INF", "inf"), ("-inf", "-inf")] {
            let r = ground_param_value(
                &typed(lexical, "http://www.w3.org/2001/XMLSchema#double"),
                &no_scope(),
            );
            assert!(
                matches!(r, Err(GroundErr::NonFinite { value: v, intermediate: false }) if v == value),
                "{lexical}: {r:?}"
            );
        }
        // Absorbed intermediates are refused at the node that produced them.
        for (text, value) in [
            ("integer(1/0)", "inf"),
            ("sign(0/0)", "NaN"),
            ("min(1/0, 3.0)", "inf"),
            ("(0/0) > 0", "NaN"),
            ("inf * 2", "inf"),
        ] {
            let r = ground_param_value(&CxfValue::Expr(text.to_owned()), &no_scope());
            assert!(
                matches!(r, Err(GroundErr::NonFinite { value: v, intermediate: true }) if v == value),
                "{text}: {r:?}"
            );
            assert_eq!(
                r.unwrap_err().diag_code(),
                oce_diag::DiagCode::NonFiniteParameter
            );
        }
        // The finite `inf` alias and finite extremes are untouched.
        let alias = ground_param_value(&CxfValue::Expr("inf".to_owned()), &no_scope()).unwrap();
        assert!(alias.bit_eq(&Value::Real(f64::MAX)));
        let tiny = ground_param_value(
            &typed("5e-324", "http://www.w3.org/2001/XMLSchema#double"),
            &no_scope(),
        )
        .unwrap();
        assert!(tiny.bit_eq(&Value::Real(f64::from_bits(1))));
        // Other failures keep `grounding-failed`.
        let unknown = ground_param_value(&CxfValue::Expr("nope".to_owned()), &no_scope());
        assert_eq!(
            unknown.unwrap_err().diag_code(),
            oce_diag::DiagCode::GroundingFailed
        );
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
