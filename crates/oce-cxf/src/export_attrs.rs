//! Classified connector attributes and their canonical CXF emission.

use std::sync::Arc;

use oce_diag::{DiagCode, Diagnostic};
use oce_model::Attrs;

use crate::dto::{CxfValue, Node, TermAttr};

/// The connector carries a non-default `unbounded` attribute (the importer hardcodes
/// `unbounded: None`, so any `Some` is outside the canonical export subset).
const MSG_ATTR_UNBOUNDED: &str = "export subset: connector carries a non-default §7.4.1 unbounded attribute, \
     which is outside the canonical (bare-scalar) export subset";
/// A Real `min`/`max` bound is non-finite (`NaN`/`INFINITY`/`NEG_INFINITY`).
/// `serde_json` serializes a non-finite `f64` as JSON `null`, which re-imports as `None`, silently
/// breaking the RT-2 render fixpoint.
const MSG_ATTR_NONFINITE_BOUND: &str = "export subset: connector carries a non-finite §7.4.1 min/max bound, \
     which is outside the canonical (bare-scalar) export subset";
/// A Real `nominal` is non-finite; refused for the same reason as [`MSG_ATTR_NONFINITE_BOUND`].
const MSG_ATTR_NONFINITE_NOMINAL: &str = "export subset: connector carries a non-finite §7.4.1 nominal, \
     which is outside the canonical (bare-scalar) export subset";

/// The classified §7.4.1 attributes for one emitted port.
#[derive(Clone, Debug)]
pub(super) enum PortAttrs {
    /// Real connector attributes representable by the canonical exporter.
    Real {
        unit: Option<Arc<str>>,
        quantity: Option<Arc<str>>,
        display_unit: Option<Arc<str>>,
        min: Option<f64>,
        max: Option<f64>,
        nominal: Option<f64>,
    },
    /// Integer connector bounds.
    Integer { min: Option<i64>, max: Option<i64> },
    /// No attributes are emitted.
    None,
}

/// Classify a type-matched connector's [`Attrs`] into the canonical emit subset.
pub(super) fn classify_attrs(
    attrs: &Attrs,
    subject: &str,
    diags: &mut Vec<Diagnostic>,
) -> PortAttrs {
    match attrs {
        Attrs::Real(attrs) => {
            // Imported since issue #379, so a finite nominal round-trips like a bound.
            let nominal = match attrs.nominal {
                Some(nominal) if !nominal.is_finite() => {
                    diags.push(reject(MSG_ATTR_NONFINITE_NOMINAL, subject));
                    None
                }
                other => other,
            };
            if attrs.unbounded.is_some() {
                diags.push(reject(MSG_ATTR_UNBOUNDED, subject));
            }
            let min = finite_real_bound(attrs.min, subject, diags);
            let max = finite_real_bound(attrs.max, subject, diags);
            PortAttrs::Real {
                unit: attrs.unit.clone(),
                quantity: attrs.quantity.clone(),
                display_unit: attrs.display_unit.clone(),
                min,
                max,
                nominal,
            }
        }
        Attrs::Integer(attrs) => PortAttrs::Integer {
            min: attrs.min,
            max: attrs.max,
        },
        Attrs::Boolean(_) | Attrs::String(_) | Attrs::Enum(_) => PortAttrs::None,
    }
}

/// Keep finite Real bounds and reject values that JSON would serialize as `null`.
fn finite_real_bound(
    value: Option<f64>,
    subject: &str,
    diags: &mut Vec<Diagnostic>,
) -> Option<f64> {
    match value {
        Some(bound) if bound.is_finite() => Some(bound),
        Some(_) => {
            diags.push(reject(MSG_ATTR_NONFINITE_BOUND, subject));
            None
        }
        None => None,
    }
}

fn reject(message: &str, subject: &str) -> Diagnostic {
    Diagnostic::error(DiagCode::ExportUnsupported, message).with_subject(subject.to_owned())
}

/// Emit a port node's classified attributes under the Bare-Scalar Canonical wire shape.
pub(super) fn emit_port_attrs(node: &mut Node, attrs: &PortAttrs) {
    match attrs {
        PortAttrs::Real {
            unit,
            quantity,
            display_unit,
            min,
            max,
            nominal,
        } => {
            if let Some(u) = unit {
                node.unit = Some(TermAttr::Bare(u.as_ref().to_owned()));
            }
            if let Some(q) = quantity {
                node.quantity = Some(TermAttr::Bare(q.as_ref().to_owned()));
            }
            if let Some(d) = display_unit {
                node.display_unit = Some(TermAttr::Bare(d.as_ref().to_owned()));
            }
            if let Some(m) = min {
                node.min = Some(CxfValue::Float(*m));
            }
            if let Some(m) = max {
                node.max = Some(CxfValue::Float(*m));
            }
            if let Some(n) = nominal {
                node.nominal = Some(CxfValue::Float(*n));
            }
        }
        PortAttrs::Integer { min, max } => {
            if let Some(m) = min {
                node.min = Some(CxfValue::Int(*m));
            }
            if let Some(m) = max {
                node.max = Some(CxfValue::Int(*m));
            }
        }
        PortAttrs::None => {}
    }
}
