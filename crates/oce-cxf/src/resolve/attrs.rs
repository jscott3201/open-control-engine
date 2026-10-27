//! Connector §7.4.1 attribute parsing.

use std::sync::Arc;

use oce_diag::{DiagCode, Diagnostic};
use oce_model::{Attrs, IntAttrs, RealAttrs, Value, ValueType};

use crate::dto::{CxfValue, Node};
use crate::ground::ground_value;
use crate::scope_chain::ParamScope;

/// Parse a connector node's declared §7.4.1 attributes
/// (`unit`/`quantity`/`displayUnit`/`min`/`max`/`nominal`) into the `Attrs` variant matching its
/// [`ValueType`] (doc 02 §3.3). `Real` carries unit/quantity/displayUnit, numeric bounds and
/// `nominal`; `Integer` carries only integer bounds; `Boolean`/`String`/`Enum` carry none (the
/// type system forbids it), so a declared one is a `malformed-document` error, never silently
/// dropped.
///
/// `nominal` grounds like a bound (issue #379): it reaches the deep gate, which refuses a
/// non-finite one (`non-finite-bound`), and export round-trips it. It is still excluded from
/// §7.10 unification (R13.4) and never read on the tick. `unbounded` is not parsed.
pub(super) fn connector_attrs(node: &Node, vt: ValueType, diags: &mut Vec<Diagnostic>) -> Attrs {
    let subject = node.id.as_str();
    match vt {
        ValueType::Real => Attrs::Real(RealAttrs {
            quantity: term_attr(node.quantity.as_ref(), "quantity", subject, diags),
            unit: term_attr(node.unit.as_ref(), "unit", subject, diags),
            display_unit: term_attr(node.display_unit.as_ref(), "displayUnit", subject, diags),
            min: real_connector_number(node.min.as_ref(), "min", subject, diags),
            max: real_connector_number(node.max.as_ref(), "max", subject, diags),
            nominal: real_connector_number(node.nominal.as_ref(), "nominal", subject, diags),
            unbounded: None,
        }),
        ValueType::Integer => {
            refuse_nominal(node, "an Integer", subject, diags);
            Attrs::Integer(IntAttrs {
                min: int_connector_bound(node.min.as_ref(), "min", subject, diags),
                max: int_connector_bound(node.max.as_ref(), "max", subject, diags),
            })
        }
        // Boolean / String / Enumeration carry no §7.4.1 attributes (CDL §7.4.1.3–.5). A declared
        // attribute on such a connector is a malformed model — surface it, never silently drop it.
        other => {
            if node.unit.is_some()
                || node.quantity.is_some()
                || node.display_unit.is_some()
                || node.min.is_some()
                || node.max.is_some()
            {
                diags.push(
                    Diagnostic::error(
                        DiagCode::MalformedDocument,
                        format!(
                            "§7.4.1 attribute (unit/quantity/displayUnit/min/max) declared on a \
                             {other:?} connector, which permits none"
                        ),
                    )
                    .with_subject(subject.to_owned()),
                );
            }
            refuse_nominal(node, &format!("a {other:?}"), subject, diags);
            Attrs::default_for(other)
        }
    }
}

/// Refuse a declared `S231:nominal` on a non-Real connector: CDL gives only Real a nominal
/// (§7.4.1), so it is a `malformed-document` error rather than an attribute silently dropped.
fn refuse_nominal(node: &Node, kind: &str, subject: &str, diags: &mut Vec<Diagnostic>) {
    if node.nominal.is_some() {
        diags.push(
            Diagnostic::error(
                DiagCode::MalformedDocument,
                format!("§7.4.1 attribute nominal declared on {kind} connector; only Real has one"),
            )
            .with_subject(subject.to_owned()),
        );
    }
}

fn term_attr(
    t: Option<&crate::dto::TermAttr>,
    which: &str,
    subject: &str,
    diags: &mut Vec<Diagnostic>,
) -> Option<Arc<str>> {
    match t?.as_term() {
        Some(s) => Some(Arc::from(s)),
        None => {
            diags.push(
                Diagnostic::error(
                    DiagCode::MalformedDocument,
                    format!(
                        "S231:{which} is not a string, typed literal, or IRI node — malformed term"
                    ),
                )
                .with_subject(subject.to_owned()),
            );
            None
        }
    }
}

/// Ground one numeric Real attribute — a `min`/`max` bound or the `nominal` — against an empty
/// scope. A failure names the attribute's role: a bound for `min`/`max`, and for `nominal` (which
/// bounds nothing) the attribute itself.
fn real_connector_number(
    v: Option<&CxfValue>,
    which: &str,
    subject: &str,
    diags: &mut Vec<Diagnostic>,
) -> Option<f64> {
    let v = v?;
    let role = if which == "nominal" {
        "attribute"
    } else {
        "bound"
    };
    match ground_value(v, &ParamScope::empty()) {
        Ok(Value::Real(r)) => Some(r),
        Ok(Value::Integer(i)) => Some(i as f64),
        Ok(_) => {
            diags.push(
                Diagnostic::error(
                    DiagCode::MalformedDocument,
                    format!("S231:{which} on a Real connector did not ground to a number"),
                )
                .with_subject(subject.to_owned()),
            );
            None
        }
        Err(e) => {
            diags.push(
                Diagnostic::error(
                    DiagCode::GroundingFailed,
                    format!("S231:{which} connector {role} failed to ground: {e}"),
                )
                .with_subject(subject.to_owned()),
            );
            None
        }
    }
}

fn int_connector_bound(
    v: Option<&CxfValue>,
    which: &str,
    subject: &str,
    diags: &mut Vec<Diagnostic>,
) -> Option<i64> {
    let v = v?;
    match ground_value(v, &ParamScope::empty()) {
        Ok(Value::Integer(i)) => Some(i),
        Ok(_) => {
            diags.push(
                Diagnostic::error(
                    DiagCode::MalformedDocument,
                    format!("S231:{which} on an Integer connector did not ground to an integer"),
                )
                .with_subject(subject.to_owned()),
            );
            None
        }
        Err(e) => {
            diags.push(
                Diagnostic::error(
                    DiagCode::GroundingFailed,
                    format!("S231:{which} connector bound failed to ground: {e}"),
                )
                .with_subject(subject.to_owned()),
            );
            None
        }
    }
}
