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
//! `expand_array_param`'s sibling-name collision set, which is built once per instance from
//! `param_iris` itself (issue #731); that set is lookup-only and never iterated into a model id or
//! a vector order.
//!
//! Once an instance's table is complete, an Integer bound to a parameter its class types Real is
//! held as that Real, or refused above 2^53 in magnitude (`real_parameters`, issue #683). The
//! sibling scope keeps the grounded value.
//!
//! Bounded on hostile input: each instance reads its enclosing scope through the shared,
//! hash-indexed [`ScopeFrame`] its parent composite built (never a per-instance copy), siblings
//! accumulate in a hash-indexed region, and every preserved array parameter in the document draws
//! on one [`ArrayElementBudget`]. A value the instance reads more than once (a node it lists
//! again, a broadcast array's one value) is grounded once ([`InstanceReads`], issues #711 and
//! #736), every string value a table holds is counted against one [`StringParameterBudget`] for
//! the document, and each diagnostic is raised, so none is built once the list has ended.

use std::collections::HashMap;
use std::sync::Arc;

use oce_diag::{DiagCode, Diagnostic};
use oce_expr::EvalResult;
use oce_model::{BlockId, BlockInstance, ParamTable, Value};

use crate::arrays::{ArrayElementBudget, expand_array_param};
use crate::dto::Node;
use crate::parameter_reads::{InstanceReads, StringParameterBudget};
use crate::scope_chain::{IndexedEntries, ParamScope, ScopeFrame};
use oce_diag::DiagnosticList;

use super::instance_interface::InstanceDerivation;
use super::local_name;
use super::parameter_attrs::CheckedParameters;
use super::real_parameters::hold_real_parameters_as_reals;
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
    /// The enclosing composite scope this instance grounds under (issue #239), shared with every
    /// sibling leaf of the same composite.
    pub(super) inherited_scope: Arc<ScopeFrame>,
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
    checked: &mut CheckedParameters,
    diags: &mut DiagnosticList,
) {
    let mut array_budget = ArrayElementBudget::per_document();
    let mut string_budget = StringParameterBudget::per_document();
    for inst in insts {
        let mut table: Vec<(Arc<str>, Value)> = Vec::new();
        // The parameter node each table entry came from, in step with `table`.
        let mut sources: Vec<&str> = Vec::new();
        // The instance's own grounded members; the inherited scope stays in its shared frame.
        let mut siblings = IndexedEntries::default();
        let enclosing: &ScopeFrame = &inst.inherited_scope;
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
        let mut reads = InstanceReads::of(&param_iris);
        for &piri in &param_iris {
            let Some(pnode) = by_id.get(piri).copied() else {
                diags.raise(|| {
                    Diagnostic::error(DiagCode::UnresolvedReference, "parameter node not found")
                        .with_subject(piri.to_owned())
                });
                continue;
            };
            checked.refuse_malformed(pnode, piri, diags);
            let Some(cxf_val) = &pnode.value else {
                diags.raise(|| {
                    Diagnostic::error(
                        DiagCode::GroundingFailed,
                        "parameter has no value (Ground mode)",
                    )
                    .with_subject(piri.to_owned())
                });
                continue;
            };
            validate_g36_parameter_value(
                pnode,
                cxf_val,
                &ParamScope::with_enclosing(enclosing, &siblings),
                diags,
            );
            if pnode.is_array == Some(true) {
                // A preserved array parameter expands to per-element scalar entries (doc 04
                // §3.6.1). Both CXF encodings (this, and pre-flattened k_1/k_2 scalars)
                // converge here.
                expand_array_param(
                    piri,
                    &inst.node.id,
                    pnode,
                    cxf_val,
                    enclosing,
                    &mut reads,
                    &mut table,
                    &mut siblings,
                    &mut array_budget,
                    &mut string_budget,
                    diags,
                );
            } else {
                // Scalar parameter: grounded once however often the instance lists it.
                let scope = ParamScope::with_enclosing(enclosing, &siblings);
                let repeated = reads.groundings.is_repeated(piri);
                match &*reads.groundings.ground(cxf_val, &scope, repeated) {
                    Ok(v) => match string_budget.take(v) {
                        Ok(()) => {
                            let name: Arc<str> = Arc::from(local_name(piri));
                            siblings.push(Arc::clone(&name), EvalResult::Scalar(v.clone()));
                            table.push((name, v.clone()));
                        }
                        Err(refusal) => diags.raise(|| refusal.diagnostic(piri)),
                    },
                    Err(e) => {
                        diags.raise(|| crate::ground::ground_failure(e, piri, &inst.node.id));
                    }
                }
            }
            sources.resize(table.len(), piri);
        }
        let block = &mut blocks[inst.id.0 as usize];
        hold_real_parameters_as_reals(&block.class_iri, &inst.node.id, &mut table, &sources, diags);
        block.params = ParamTable { values: table };
    }
}
