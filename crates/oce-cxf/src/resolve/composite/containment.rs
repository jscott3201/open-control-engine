//! Depth-first `S231:containsBlock` flattening: the leaf order, each leaf's inherited scope, and
//! the containment refusals (nesting depth, cycles, and shared children).
//!
//! A CDL component instance has exactly **one** parent. The walk therefore visits every active
//! composite and every leaf at most once: a child already reached — listed twice by one
//! composite, or by two composites — is refused with [`DiagCode::SharedInstance`] and not
//! expanded again. Before this rule a composite listing `[C, C]` at every level doubled the walk
//! per level, so a few kilobytes of document drove exponential lowering time and memory. With it
//! the walk is linear in the number of `containsBlock` references.
//!
//! Each composite's own declaration chain is evaluated once, into one shared
//! [`ScopeFrame`] that every child composite and leaf references through an `Arc`, so inherited
//! scope memory is linear in the declarations plus the leaves (not their product).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use oce_diag::{DiagCode, Diagnostic};

use crate::dto::Node;
use crate::scope_chain::ScopeFrame;

use super::super::composite_rules::CONTAINS_CYCLE;
use super::super::declaration_scope::{Pass, evaluate_declarations};
use super::super::specialize::Specialization;
use super::is_runtime_composite;

/// Maximum supported composite depth during `containsBlock` lowering.
///
/// The outermost composite is depth one. Real inputs measured at depth three or less; this cap
/// bounds the recursive lowering walk well below the measured stack-exhaustion threshold.
pub(super) const MAX_COMPOSITE_NESTING_DEPTH: usize = 64;

/// The mutable state of one document's containment walk.
pub(super) struct Containment<'a, 'd> {
    by_id: &'a HashMap<&'a str, &'a Node>,
    specialization: &'a Specialization,
    diags: &'d mut Vec<Diagnostic>,
    /// O(1) membership for the current traversal spine (cycle detection).
    stack: HashSet<String>,
    /// The traversal spine in path order, so a detected cycle names every participant.
    path: Vec<String>,
    /// Every composite entered and every leaf listed so far — the single-parent check.
    reached: HashSet<String>,
    /// Active leaves in depth-first `containsBlock` order.
    pub(super) leaf_order: Vec<String>,
    /// Each leaf's inherited scope: a shared reference to its parent composite's frame.
    pub(super) inherited_scope: HashMap<String, Arc<ScopeFrame>>,
    /// Composite ids whose own declaration chains this walk evaluated.
    pub(super) evaluated_chains: HashSet<String>,
}

impl<'a, 'd> Containment<'a, 'd> {
    /// A fresh walk over `by_id` under the completed `specialization`.
    pub(super) fn new(
        by_id: &'a HashMap<&'a str, &'a Node>,
        specialization: &'a Specialization,
        diags: &'d mut Vec<Diagnostic>,
    ) -> Self {
        Self {
            by_id,
            specialization,
            diags,
            stack: HashSet::new(),
            path: Vec::new(),
            reached: HashSet::new(),
            leaf_order: Vec::new(),
            inherited_scope: HashMap::new(),
            evaluated_chains: HashSet::new(),
        }
    }

    /// Flatten `root` depth-first. A `true` result stops pre-lowering after the first depth
    /// refusal, before orientation indexes the hostile graph.
    pub(super) fn walk_root(&mut self, root: &str) -> bool {
        self.collect_leaves(root, 1, &ScopeFrame::root())
    }

    /// Depth-first `containsBlock` flattening of one composite. `stack` and `path` are pushed and
    /// popped together, so they always hold the same ids.
    fn collect_leaves(
        &mut self,
        composite_id: &str,
        depth: usize,
        parent_scope: &Arc<ScopeFrame>,
    ) -> bool {
        if depth > MAX_COMPOSITE_NESTING_DEPTH {
            self.diags.push(
                Diagnostic::error(
                    DiagCode::MalformedDocument,
                    format!(
                        "composite/nesting-too-deep: containsBlock nesting exceeds the supported \
                         depth ({MAX_COMPOSITE_NESTING_DEPTH})"
                    ),
                )
                .with_subject(composite_id.to_owned()),
            );
            return true;
        }
        if self.stack.contains(composite_id) {
            self.report_cycle(composite_id);
            return false;
        }
        if !self.reached.insert(composite_id.to_owned()) {
            self.report_shared(composite_id);
            return false;
        }
        self.stack.insert(composite_id.to_owned());
        self.path.push(composite_id.to_owned());
        let exceeded = self.expand(composite_id, depth, parent_scope);
        self.stack.remove(composite_id);
        self.path.pop();
        exceeded
    }

    /// Evaluate one entered composite's own chain and walk its active children in array order.
    fn expand(&mut self, composite_id: &str, depth: usize, parent_scope: &Arc<ScopeFrame>) -> bool {
        let by_id = self.by_id;
        let Some(composite) = by_id.get(composite_id).copied() else {
            self.diags.push(
                Diagnostic::error(DiagCode::UnresolvedReference, "composite node not found")
                    .with_subject(composite_id.to_owned()),
            );
            return false;
        };
        self.evaluated_chains.insert(composite_id.to_owned());
        let scope = evaluate_declarations(
            composite,
            &[],
            parent_scope,
            by_id,
            Pass::Lowering {
                specialization: self.specialization,
                diags: self.diags,
            },
        )
        .scope;
        for child in composite.contains_block.iter().map(|r| r.id.as_str()) {
            if self.specialization.is_inactive(child) {
                continue;
            }
            let Some(node) = by_id.get(child).copied() else {
                self.diags.push(
                    Diagnostic::error(
                        DiagCode::UnresolvedReference,
                        "containsBlock child not found",
                    )
                    .with_subject(child.to_owned()),
                );
                continue;
            };
            if is_runtime_composite(node) {
                if self.collect_leaves(child, depth + 1, &scope) {
                    return true;
                }
            } else if self.reached.insert(child.to_owned()) {
                self.leaf_order.push(child.to_owned());
                self.inherited_scope
                    .insert(child.to_owned(), Arc::clone(&scope));
            } else {
                self.report_shared(child);
            }
        }
        false
    }

    /// Report a `containsBlock` cycle re-entering `composite_id`: the participants from the
    /// re-entered id's position on the spine onward, closing with the re-entered id. Traversal
    /// follows `containsBlock` document order, so the list is deterministic. (`position` cannot
    /// miss — `stack` membership implies `path` membership — but falls back to the whole spine.)
    fn report_cycle(&mut self, composite_id: &str) {
        let start = self
            .path
            .iter()
            .position(|id| id == composite_id)
            .unwrap_or_default();
        let mut participants: Vec<&str> = self.path[start..].iter().map(String::as_str).collect();
        participants.push(composite_id);
        self.diags.push(
            Diagnostic::error(
                CONTAINS_CYCLE.code,
                CONTAINS_CYCLE.message(format!(
                    "cycle in nested composite containsBlock graph: {}",
                    participants.join(" -> ")
                )),
            )
            .with_subject(composite_id.to_owned()),
        );
    }

    /// Report a second reach of an already-reached composite or leaf, naming the composite whose
    /// listing reached it again (the top of the spine).
    fn report_shared(&mut self, child: &str) {
        let parent = self.path.last().map_or("", String::as_str);
        self.diags.push(
            Diagnostic::error(
                DiagCode::SharedInstance,
                format!(
                    "containsBlock child {child} is reached a second time (listed again by \
                     {parent}); a CDL component instance has exactly one parent, so the second \
                     reach is refused rather than expanded again"
                ),
            )
            .with_subject(child.to_owned()),
        );
    }
}
