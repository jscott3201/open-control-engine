//! The budget on the label bytes a document's connectors carry once §7.10 unification has
//! propagated them (issue #815, owner ruling of 2026-11-23).
//!
//! Unification gives every unset member of a cluster the one `unit` or `quantity` its members
//! agree on, so a label one output spells reaches every input it drives and every declared output
//! it feeds. The graph shares one allocation, but everything a load hands on holds or writes a
//! label for each connector: the inventory's and the projection's points, the point list, the
//! state manifest and the export's port nodes. So a long label spelled once on an output driving
//! many inputs cost a copy for each. The budget counts what each connector ends up carrying.
//!
//! **What counts:** the `unit`, `quantity` and `displayUnit` of every connector and every declared
//! boundary output whose attributes are `Real`, after propagation, each connector counted for
//! itself: a label written once and propagated to `n` inputs counts `n + 1` times, exactly as the
//! same label written on each of them. **What does not:** no other attribute set carries text
//! (`Integer`, `Boolean`, `String` and enumeration sets hold none), `displayUnit` is never
//! propagated (it is counted where it is written), boundary-input declarations are not unified,
//! and the semantic layer's `description` and `controlledDevice` are never set from a document
//! today. The sum does not depend on order, so neither does the outcome.

use oce_diag::{DiagCode, Diagnostic, DiagnosticList};
use oce_model::{Attrs, ModelGraph};

/// The most label bytes a document's connectors and declared outputs may carry after
/// unification: 8 MiB (8,388,608). The largest tracked or `open-control-library` model carries
/// 274 (`cooling_only_controller.jsonld`).
pub(crate) const MAX_DOCUMENT_LABEL_BYTES: usize = 8 * 1024 * 1024;

/// The label bytes one attribute set carries.
fn label_bytes(attrs: &Attrs) -> usize {
    attrs.as_real().map_or(0, |real| {
        [&real.unit, &real.quantity, &real.display_unit]
            .into_iter()
            .flatten()
            .map(|label| label.len())
            .fold(0, usize::saturating_add)
    })
}

/// The label bytes every connector and declared boundary output of `model` carries.
pub(crate) fn document_label_bytes(model: &ModelGraph) -> usize {
    model
        .connectors
        .iter()
        .map(|connector| label_bytes(&connector.attrs))
        .chain(
            model
                .boundary_outputs
                .iter()
                .map(|output| label_bytes(&output.attrs)),
        )
        .fold(0, usize::saturating_add)
}

/// Refuse a document whose connectors carry more than [`MAX_DOCUMENT_LABEL_BYTES`] of labels,
/// as one `resource-limit-exceeded` error naming the total. It names no subject: the total is the
/// document's, and no connector is the one that crosses it.
pub(crate) fn check_label_budget(model: &ModelGraph, diags: &mut DiagnosticList) {
    let total = document_label_bytes(model);
    if total > MAX_DOCUMENT_LABEL_BYTES {
        diags.push(Diagnostic::error(
            DiagCode::ResourceLimitExceeded,
            format!(
                "the connectors' unit, quantity and displayUnit labels take {total} bytes once \
                 §7.10 unification has propagated them, more than the \
                 {MAX_DOCUMENT_LABEL_BYTES} bytes a document may carry"
            ),
        ));
    }
}
