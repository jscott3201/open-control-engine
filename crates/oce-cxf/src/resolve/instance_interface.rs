//! Child-instance interface derivation from `S231:hasInstance` (`_spec/19`, scalar-only
//! slice 1) and the structural shape domain its pre-lowering consumers read.
//!
//! Three domains, defined once and never mixed (R19-2):
//!
//! - **derivation-shaped** — a `containsBlock` referent anywhere in the document that is not a
//!   runtime composite, declaring neither `hasInput` nor `hasOutput`, carrying a `hasInstance`
//!   list. A structural test, computable before lowering; reachability is NOT part of it. The
//!   two pre-lowering consumers read this set: the array scan over its active members and
//!   specialization over all of them ([`is_derivation_shaped`]).
//! - the **derivation domain** — the post-lowering instance population (`child_iris`: active
//!   leaves) restricted to nodes that declare neither port list and carry a list. Those, and
//!   only those, derive an interface. The post-lowering domain is `child_iris` and nothing
//!   else — an implementation MUST NOT cache a pre-lowering referent set and apply the
//!   [`is_runtime_composite`](super::composite::is_runtime_composite) predicate to it after
//!   lowering, because `lower` clears every non-root composite's `contains_block` and the
//!   predicate then answers false for a nested composite.
//! - the **comparison domain** — instances that declare `hasInput` or `hasOutput` AND carry a
//!   list; compared one-directional (list minus own) where the class resolves, never derived.
//!
//! `hasInstance` never feeds composite classification: `is_runtime_composite` stays
//! `!contains_block.is_empty() && !is_registered_leaf(node)`, so a node carrying `hasInstance`
//! with no `containsBlock` is a leaf (derivation-shaped when portless), never a runtime
//! composite — R20-9's `composite_chain` flag keeps its meaning unchanged, and `hasInstance` is
//! not a containment edge anywhere (R19-12).

use std::collections::HashSet;

use crate::dto::{CxfDocument, Node};

use super::composite::is_runtime_composite;

/// Every IRI referenced by any node's `containsBlock`, active or not — the reachability-blind
/// referent set the derivation-shaped test reads. Lookup-only.
pub(super) fn contains_block_referents(doc: &CxfDocument) -> HashSet<&str> {
    doc.graph
        .iter()
        .flat_map(|node| node.contains_block.iter().map(|r| r.id.as_str()))
        .collect()
}

/// The structural shape test (R19-2): a `containsBlock` referent that is not a runtime
/// composite, declares neither port list, and carries a `hasInstance` list. Valid ONLY
/// pre-lowering — see the module docs for why it must never be applied to the lowered graph.
pub(super) fn is_derivation_shaped(node: &Node, contains_referents: &HashSet<&str>) -> bool {
    contains_referents.contains(node.id.as_str())
        && !is_runtime_composite(node)
        && node.has_input.is_empty()
        && node.has_output.is_empty()
        && !node.has_instance.is_empty()
}
