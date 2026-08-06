//! Document-wide subset checks for active array-valued connectors and block instances.
//!
//! The scan is document-wide by ruling: every active node's `S231:hasInput`/`S231:hasOutput`
//! references classify connectors and every active node's `S231:containsBlock` references
//! classify instances, whether or not the referencing node is reachable from the top-level
//! root. A node referenced as both receives both rejections. A `S231:hasParameter` listing
//! does not exempt a node — an array marker on anything wired as a connector or instance is a
//! silent-scalar-collapse hazard and rejects; preserved array parameters are never referenced
//! as connectors or instances, so they never reach these rules.
//!
//! An active **derivation-shaped** node's `S231:hasInstance` members are a connector source
//! too (`_spec/19` R19-11) — reference-based and class-independent exactly like the existing
//! sources, because this scan runs before class resolution and gating the new source on
//! member classification would make a class-independent guard class-dependent. The new source
//! matches the existing ones on reachability and is **narrower on shape**: an orphan node (a
//! node no `containsBlock` edge points at) and a runtime composite contribute nothing, where
//! the existing sources take any active node's list at all. An array-marked member whose kind
//! would have been *parameter* rejects here too — a `hasInstance` listing exempts nothing,
//! mirroring the `hasParameter` ruling above.

use std::collections::HashSet;

use oce_diag::{Diagnostic, Severity};

use crate::dto::{CxfDocument, Node};

use super::composite_rules::{ARRAY_CONNECTOR, ARRAY_INSTANCE};
use super::instance_interface::{contains_block_referents, is_derivation_shaped};
use super::specialize::Specialization;
use super::value_types::term_of;

/// Reject active array-valued connector and block-instance nodes before composite lowering.
///
/// A node is array-marked when it carries `S231:isArray: true` or any `S231:sizeOfDimensions`.
/// Marker keys match on the term after the last `:`, `#`, or `/` — the same convention as the
/// banned-Modelica-key rule — so absolute-IRI and re-prefixed spellings reject too. Nodes
/// removed by specialization are invisible to this check, as are markers on inactive
/// references — a pruned derivation-shaped instance's members are not scanned, the same
/// existing filter and not an exception to it.
pub(super) fn reject_unsupported(
    doc: &CxfDocument,
    specialization: &Specialization,
    diags: &mut Vec<Diagnostic>,
) {
    let contains_referents = contains_block_referents(doc);
    let mut connector_ids = HashSet::new();
    let mut instance_ids = HashSet::new();

    for parent in &doc.graph {
        if specialization.is_inactive(&parent.id) {
            continue;
        }
        connector_ids.extend(
            parent
                .has_input
                .iter()
                .chain(parent.has_output.iter())
                .map(|reference| reference.id.as_str())
                .filter(|id| !specialization.is_inactive(id)),
        );
        if is_derivation_shaped(parent, &contains_referents) {
            connector_ids.extend(
                parent
                    .has_instance
                    .iter()
                    .map(|reference| reference.id.as_str())
                    .filter(|id| !specialization.is_inactive(id)),
            );
        }
        instance_ids.extend(
            parent
                .contains_block
                .iter()
                .map(|reference| reference.id.as_str())
                .filter(|id| !specialization.is_inactive(id)),
        );
    }

    for node in &doc.graph {
        if specialization.is_inactive(&node.id) || !is_array_marked(node) {
            continue;
        }
        if connector_ids.contains(node.id.as_str()) {
            diags.push(
                Diagnostic::new(
                    Severity::Error,
                    ARRAY_CONNECTOR.code,
                    ARRAY_CONNECTOR.message(
                        "array-valued connector nodes are not supported; flatten the array to \
                         one connector per element",
                    ),
                )
                .with_subject(node.id.clone()),
            );
        }
        if instance_ids.contains(node.id.as_str()) {
            diags.push(
                Diagnostic::new(
                    Severity::Error,
                    ARRAY_INSTANCE.code,
                    ARRAY_INSTANCE.message(
                        "array-valued block-instance nodes are not supported; flatten the array \
                         to one instance per element",
                    ),
                )
                .with_subject(node.id.clone()),
            );
        }
    }
}

/// Whether a node carries an array marker: modeled `S231:isArray: true` / `S231:sizeOfDimensions`,
/// or either key in any other spelling (matched by trailing term) in the pass-through map.
/// `isArray` counts only when literally `true`, mirroring the modeled `Option<bool>` field;
/// `sizeOfDimensions` counts by presence, mirroring the modeled field's `is_some()`.
fn is_array_marked(node: &Node) -> bool {
    if node.is_array == Some(true) || node.size_dims.is_some() {
        return true;
    }
    node.other.iter().any(|(key, value)| match term_of(key) {
        "isArray" => value == &serde_json::Value::Bool(true),
        "sizeOfDimensions" => true,
        _ => false,
    })
}
