//! The refusal of CDL extension blocks (OBC §7.13 and §8.7; GitHub issue #537).
//!
//! An extension block is a block whose behaviour is a Functional Mockup Unit (FMU), which its CXF
//! form names with `S231:hasFmuPath`. The engine has no FMI import, and FMI is out of scope before
//! 1.0, so a document that contains one cannot run as written and is refused at load with
//! [`DiagCode::ExtensionBlockUnsupported`]: one error per offending node, subject the node's
//! `@id`.
//!
//! A node **is an extension block** when either holds:
//!
//! - one of its `@type` values, at any position in the list, is the S231 class `ExtensionBlock`
//!   (OBC §8.7), read the way a modeled property key is read: the `@context`-expanded type is
//!   `http://data.ashrae.org/S231#ExtensionBlock` (written as that IRI, as `S231:ExtensionBlock`
//!   under the published binding, through another prefix bound to the namespace, or as a
//!   `@context` term that expands to it), or it is the compact `S231:ExtensionBlock` of a document
//!   that binds no `S231`. A class of that name in any other namespace (`ex:ExtensionBlock`), and
//!   a bare `ExtensionBlock` that no `@context` term expands, is an ordinary class: unknown as a
//!   leaf (`class-not-found`) and lowered as a composite when it lists children, like a class of
//!   any other name. The `https:` spellings of the namespace never reach here: the expansion pass
//!   refuses them as `vocabulary-namespace-mismatch`, as it does a modeled key in them.
//! - it carries `S231:hasFmuPath`, whatever its type. The property's domain is `ExtensionBlock`
//!   (OBC §8.7), so a node that names an FMU claims an FMU's behaviour. Every key spelling of the
//!   property reaches the one modeled field, [`crate::dto::Node::fmu_path`].
//!
//! A node whose `@type` names an extension block of the same document (an instance carried
//! beside its class's own definition) is refused too, under the same code.
//!
//! The scan covers every active `@graph` node, wherever it sits: the root, a child of the root,
//! a child of a nested composite, an extension block that lists `containsBlock` children of its
//! own, and a node no `containsBlock` edge reaches. A node pruned by a false conditional guard is
//! not part of the schedule and is not refused, the same tolerance an inactive node gets for a
//! banned Modelica key or an array marker. Only the node's own activity counts: a class
//! definition carried in the document is refused even when every instance of it is pruned.
//!
//! It runs after specialization and before composite lowering, and a refusal ends the resolve
//! there. Lowering would otherwise read an extension block that lists children as a composite
//! and run those children in place of the FMU, choose an extension-block root as the top
//! composite, or report the block as an unknown class (`class-not-found`). The refusal replaces
//! all three, and it is the last thing reported for the whole document: nothing composite
//! lowering or a later stage would find, for any node, is reported beside it. Findings made
//! before it (the specialize pass's) still are.
//!
//! Messages are fixed text, so a refusal copies nothing from the document but the subject.

use std::collections::HashSet;

use oce_diag::{DiagCode, Diagnostic};

use crate::dto::{CxfDocument, Node};
use crate::export::S231_CONTEXT_IRI;

use super::specialize::Specialization;

/// The term of the CXF extension-block class in the S231 namespace (OBC §8.7).
pub(crate) const EXTENSION_BLOCK_TERM: &str = "ExtensionBlock";

/// Whether an expanded `@type` value is the S231 class `ExtensionBlock`: its IRI, or the compact
/// form that survives expansion only in a document that binds no `S231` (a document binding
/// `S231` anywhere else was refused before this pass).
fn is_s231_extension_block(type_iri: &str) -> bool {
    let term = type_iri
        .strip_prefix(S231_CONTEXT_IRI)
        .or_else(|| type_iri.strip_prefix("S231:"));
    term == Some(EXTENSION_BLOCK_TERM)
}

/// The end of every refusal message: why the block cannot be loaded.
const UNSUPPORTED: &str = "its behaviour is a Functional Mockup Unit, and the engine does not \
                           execute FMUs (FMI is out of scope before 1.0)";

/// Why a node is refused.
#[derive(Clone, Copy)]
enum Marker {
    /// Typed `ExtensionBlock`, with or without `S231:hasFmuPath`.
    Typed { fmu_path: bool },
    /// Not typed `ExtensionBlock`, but carries `S231:hasFmuPath`.
    FmuPath,
    /// Its `@type` names a node of this document that is an extension block.
    Instance,
}

impl Marker {
    fn message(self) -> String {
        let why = match self {
            Marker::Typed { fmu_path: true } => {
                "extension block (typed S231:ExtensionBlock, with S231:hasFmuPath)"
            }
            Marker::Typed { fmu_path: false } => {
                "extension block (typed S231:ExtensionBlock, without S231:hasFmuPath)"
            }
            Marker::FmuPath => {
                "extension block (it carries S231:hasFmuPath, whose domain is \
                 S231:ExtensionBlock)"
            }
            Marker::Instance => {
                "instance of an extension block (its @type names a node of this document that \
                 is one)"
            }
        };
        format!("{why}: {UNSUPPORTED}")
    }
}

/// The node's own extension-block marker, if it has one.
fn own_marker(node: &Node) -> Option<Marker> {
    let typed = node
        .r#type
        .as_ref()
        .is_some_and(|types| types.iter().any(|t| is_s231_extension_block(t)));
    let fmu_path = node.fmu_path.is_some();
    match (typed, fmu_path) {
        (true, fmu_path) => Some(Marker::Typed { fmu_path }),
        (false, true) => Some(Marker::FmuPath),
        (false, false) => None,
    }
}

/// Refuse every active extension block of `doc` (see the module docs), in `@graph` order. Returns
/// whether it refused any, in which case the caller ends the resolve.
pub(super) fn refuse(
    doc: &CxfDocument,
    specialization: &Specialization,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    let markers: Vec<Option<Marker>> = doc.graph.iter().map(own_marker).collect();
    // Lookup-only: every node that is an extension block in its own right, active or not.
    let classes: HashSet<&str> = doc
        .graph
        .iter()
        .zip(&markers)
        .filter(|(_, marker)| marker.is_some())
        .map(|(node, _)| node.id.as_str())
        .collect();
    let before = diags.len();
    for (node, marker) in doc.graph.iter().zip(markers) {
        if specialization.is_inactive(&node.id) {
            continue;
        }
        let marker = marker.or_else(|| {
            node.r#type
                .as_ref()
                .is_some_and(|types| types.iter().any(|t| classes.contains(t.as_str())))
                .then_some(Marker::Instance)
        });
        if let Some(marker) = marker {
            diags.push(
                Diagnostic::error(DiagCode::ExtensionBlockUnsupported, marker.message())
                    .with_subject(node.id.clone()),
            );
        }
    }
    diags.len() > before
}
