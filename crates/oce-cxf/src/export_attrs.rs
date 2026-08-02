//! Classified connector attributes and their canonical CXF emission.

use std::sync::Arc;

use crate::dto::{CxfValue, Node, TermAttr};

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
    },
    /// Integer connector bounds.
    Integer { min: Option<i64>, max: Option<i64> },
    /// No attributes are emitted.
    None,
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
