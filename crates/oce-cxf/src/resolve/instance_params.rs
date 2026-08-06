//! Ground-mode instance parameter grounding (resolver Step 7).
//!
//! Parameters ground in `hasParameter`/`hasConstant` array order, followed by the instance's
//! classified `hasInstance` parameter members in class-signature order
//! ([`InstanceDerivation::param_appendix`], R19-13 — appending last leaves existing goldens
//! unmoved, and duplicate names inside one instance were refused at derivation). A later
//! binding may reference an earlier one via the incrementally-built
//! [`ParamScope`]. VALUE references resolve on the split view — the inherited (enclosing)
//! region wins over a same-named sibling member (issue #239) — while array DIMENSION parsing
//! keeps the undivided latest-wins view (see `arrays.rs`). The appended members enter
//! `expand_array_param`'s sibling-name collision set, which is built from `param_iris` itself;
//! that set is lookup-only and never iterated into a model id or a vector order.

use std::collections::HashMap;
use std::sync::Arc;

use oce_diag::{DiagCode, Diagnostic};
use oce_expr::EvalResult;
use oce_model::{BlockId, BlockInstance, ParamTable, Value};

use crate::arrays::expand_array_param;
use crate::dto::Node;
use crate::ground::{ParamScope, ground_value};

use super::instance_interface::InstanceDerivation;
use super::local_name;
use super::specialize::validate_g36_parameter_value;

/// One resolved instance's wiring state, carried from Step 3 through Steps 5-8.
pub(super) struct Inst<'a> {
    /// The dense block id assigned in `containsBlock` order.
    pub(super) id: BlockId,
    /// The instance's `@graph` node.
    pub(super) node: &'a Node,
    /// Active authored `hasInput` references in array order (empty on the `hasInstance`
    /// dialect — the derived vectors live on the side, keyed by instance IRI).
    pub(super) input_iris: Vec<&'a str>,
    /// Active authored `hasOutput` references in array order.
    pub(super) output_iris: Vec<&'a str>,
    /// The enclosing composite scope this instance grounds under (issue #239).
    pub(super) inherited_scope: Vec<(Arc<str>, EvalResult)>,
}

/// Ground every instance's parameter table (Ground mode). A member with no `S231:value` is
/// `GroundingFailed`; a reference with no node at all is `UnresolvedReference` — the latter
/// arm is unreachable for `hasInstance` members, which the derivation refuses before this
/// loop runs (R19-3).
pub(super) fn ground_instance_params(
    insts: &[Inst<'_>],
    derivation: &InstanceDerivation,
    by_id: &HashMap<&str, &Node>,
    blocks: &mut [BlockInstance],
    diags: &mut Vec<Diagnostic>,
) {
    for inst in insts {
        let mut table: Vec<(Arc<str>, Value)> = Vec::new();
        let mut scope_entries: Vec<(Arc<str>, EvalResult)> = inst.inherited_scope.clone();
        // The enclosing/sibling split for value lookups: every inherited entry is enclosing.
        let split = inst.inherited_scope.len();
        // Collected (not lazily iterated) so the array branch can build the sibling-name set
        // for its collision check. Order = hasParameter array order, then hasConstant array
        // order, then the derived member appendix in class-signature order.
        let param_iris: Vec<&str> = inst
            .node
            .has_parameter
            .iter()
            .chain(inst.node.has_constant.iter())
            .map(|r| r.id.as_str())
            .chain(
                derivation
                    .param_appendix(&inst.node.id)
                    .iter()
                    .map(String::as_str),
            )
            .collect();
        for &piri in &param_iris {
            let Some(pnode) = by_id.get(piri).copied() else {
                diags.push(
                    Diagnostic::error(DiagCode::UnresolvedReference, "parameter node not found")
                        .with_subject(piri.to_owned()),
                );
                continue;
            };
            let Some(cxf_val) = &pnode.value else {
                diags.push(
                    Diagnostic::error(
                        DiagCode::GroundingFailed,
                        "parameter has no value (Ground mode)",
                    )
                    .with_subject(piri.to_owned()),
                );
                continue;
            };
            validate_g36_parameter_value(
                pnode,
                cxf_val,
                &ParamScope::with_enclosing(&scope_entries, split),
                diags,
            );
            if pnode.is_array == Some(true) {
                // A preserved array parameter expands to per-element scalar entries (doc 04
                // §3.6.1). Both CXF encodings (this, and pre-flattened k_1/k_2 scalars)
                // converge here.
                expand_array_param(
                    piri,
                    pnode,
                    cxf_val,
                    split,
                    &param_iris,
                    &mut table,
                    &mut scope_entries,
                    diags,
                );
            } else {
                // Scalar parameter.
                let name: Arc<str> = Arc::from(local_name(piri));
                match ground_value(cxf_val, &ParamScope::with_enclosing(&scope_entries, split)) {
                    Ok(v) => {
                        scope_entries.push((Arc::clone(&name), EvalResult::Scalar(v.clone())));
                        table.push((name, v));
                    }
                    Err(e) => diags.push(
                        Diagnostic::error(DiagCode::GroundingFailed, e.to_string())
                            .with_subject(piri.to_owned()),
                    ),
                }
            }
        }
        blocks[inst.id.0 as usize].params = ParamTable { values: table };
    }
}
