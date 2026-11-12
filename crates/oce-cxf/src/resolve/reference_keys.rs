//! Refusal of a reference object that carries a key beside its `@id` (GitHub issue #588).
//!
//! Every structural edge (`S231:hasInput`, `hasOutput`, `hasParameter`, `hasConstant`,
//! `containsBlock`, `hasInstance`, `isConnectedTo`) and `S231:isOfDataType` is read as the `@id`
//! of each of its reference objects, and as nothing else. Any other key was kept in
//! [`IriRef::other`](crate::IriRef) for round-trip and never read, so it was dropped without a
//! word. `"S231:hasParameter": {"@id": k, "S231:value": 99}` loaded with the node's own value, and
//! a `containsBlock` entry `{"@id": leaf, "@type": "S231:ExtensionBlock", "S231:hasFmuPath": …}`
//! loaded and ran the leaf. In JSON-LD such an object is an embedded node object: it states more
//! about the node it names, which flattened CXF states on that node's own `@graph` entry instead.
//! The engine reads none of it, so it refuses the document as `non-subset-construct` rather than
//! load a reading the author did not write.
//!
//! The message names the property, the reference's canonical `@id` (whole) and up to four of the
//! keys, each cut to an excerpt ([`crate::excerpt`]), then counts the rest.
//!
//! The check runs on the expanded document (every `@id` canonical) with the `@id` index, before
//! anything reads the graph. A `@context` inside a reference never reaches it: the expansion pass
//! refuses a scoped context first, in its `context_scope` check.

use std::sync::Arc;

use oce_diag::{DiagCode, Diagnostic};

use crate::dto::{CxfDocument, IriRef};
use crate::excerpt::named_keys;

/// Refuse every reference object of `doc` that carries a key beside its `@id`: one error per
/// reference, subject the node that holds it (`connector at @graph[N]` for a node without an
/// `@id`, as the index step names it), in `@graph` order and then property order.
pub(super) fn refuse(doc: &CxfDocument, diags: &mut Vec<Diagnostic>) {
    for (index, node) in doc.graph.iter().enumerate() {
        let mut subject: Option<Arc<str>> = None;
        let mut refuse_one = |slot: &str, reference: &IriRef| {
            let subject = Arc::clone(subject.get_or_insert_with(|| {
                if node.id.is_empty() {
                    Arc::from(format!("connector at @graph[{index}]"))
                } else {
                    Arc::from(node.id.as_str())
                }
            }));
            diags.push(
                Diagnostic::error(
                    DiagCode::NonSubsetConstruct,
                    format!(
                        "{slot} reference `{}` on node `{subject}` carries {} beside its `@id`; a \
                         reference is read as its `@id` alone, so the engine would drop it \
                         without effect: state it on the referenced node instead",
                        reference.id,
                        named_keys(&reference.other),
                    ),
                )
                .with_subject(subject),
            );
        };
        for (slot, references) in [
            ("S231:hasInput", node.has_input.as_slice()),
            ("S231:hasOutput", node.has_output.as_slice()),
            ("S231:hasParameter", node.has_parameter.as_slice()),
            ("S231:hasConstant", node.has_constant.as_slice()),
            ("S231:containsBlock", node.contains_block.as_slice()),
            ("S231:hasInstance", node.has_instance.as_slice()),
            ("S231:isConnectedTo", node.is_connected_to.as_slice()),
            ("S231:isOfDataType", node.is_of_data_type.as_slice()),
        ] {
            for reference in references.iter().filter(|r| !r.other.is_empty()) {
                refuse_one(slot, reference);
            }
        }
    }
}
