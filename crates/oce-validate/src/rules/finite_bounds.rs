//! Non-finite numeric attribute refusal for Real connectors and boundary declarations.
//!
//! A `NaN` bound admits no value, because every comparison with `NaN` is false. A boundary input
//! carrying one used to load cleanly and then refuse every frame with `InputDomain`, so the
//! mistyped attribute surfaced only at run time and without its cause. Infinite bounds and
//! nominals are refused alongside it: an infinite bound is equivalent to no bound and should be
//! omitted, and a non-finite nominal has no meaning, so either is a malformed declaration. This
//! is about declared attributes only; it sets no policy for non-finite input values. Integer
//! bounds are `i64` and always finite; Boolean, String and Enum connectors carry no numeric
//! attributes.

use std::sync::Arc;

use oce_diag::{DiagCode, Diagnostic};
use oce_model::{Attrs, ModelGraph, RealAttrs};

use super::subject_of;

/// Report every non-finite `min`, `max` or `nominal` on a Real connector, a root boundary-input
/// declaration, or a root boundary-output declaration as a [`DiagCode::NonFiniteBound`] error.
///
/// One diagnostic per offending attribute, subject = the connector's or declaration's authored
/// identity. Reads attributes only; total and panic-free on any graph, including one whose
/// `Attrs` variant disagrees with its value type (a non-Real variant carries no `f64`).
pub(crate) fn check_finite_bounds(model: &ModelGraph, diags: &mut Vec<Diagnostic>) {
    for connector in &model.connectors {
        report(&connector.attrs, "connector", &subject_of(connector), diags);
    }
    for input in &model.boundary_inputs {
        report(&input.attrs, "boundary input", &input.iri, diags);
    }
    for output in &model.boundary_outputs {
        report(&output.attrs, "boundary output", &output.iri, diags);
    }
}

fn report(attrs: &Attrs, owner: &str, subject: &Arc<str>, diags: &mut Vec<Diagnostic>) {
    let Attrs::Real(RealAttrs {
        min, max, nominal, ..
    }) = attrs
    else {
        return;
    };
    for (name, value) in [("min", min), ("max", max), ("nominal", nominal)] {
        if let Some(value) = value.filter(|value| !value.is_finite()) {
            diags.push(
                Diagnostic::error(
                    DiagCode::NonFiniteBound,
                    format!(
                        "S231:{name} on a Real {owner} is {value}; a numeric attribute must be \
                         finite"
                    ),
                )
                .with_subject(Arc::clone(subject)),
            );
        }
    }
}
