//! What [`refuse`] reports to its caller, which ends the resolve on `true`.

use oce_diag::{DiagCode, Diagnostic};

use super::refuse;
use crate::diagnostic_list::DiagnosticList;
use crate::dto::CxfDocument;
use crate::resolve::specialize::Specialization;

/// A document of one node typed as an extension block, and one that is no extension block.
fn document(extension_block: bool) -> CxfDocument {
    let class = if extension_block {
        "S231:ExtensionBlock"
    } else {
        "S231:Block"
    };
    serde_json::from_value(serde_json::json!({
        "@context": {},
        "@graph": [ { "@id": "urn:n", "@type": class } ]
    }))
    .expect("the document parses")
}

#[test]
fn an_extension_block_is_reported_whether_or_not_the_list_has_room_for_its_refusal() {
    let mut open = DiagnosticList::new();
    assert!(refuse(
        &document(true),
        &Specialization::default(),
        &mut open
    ));
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].code, DiagCode::ExtensionBlockUnsupported);

    // A list that has ended lists nothing more, so its length does not move. The refusal is
    // still raised and counted, and the caller is still told to end the resolve.
    let mut ended = DiagnosticList::new();
    ended.push(Diagnostic::error(DiagCode::MalformedDocument, "earlier"));
    ended.leave_unlisted();
    let (length, raised) = (ended.len(), ended.raised());
    assert!(refuse(
        &document(true),
        &Specialization::default(),
        &mut ended
    ));
    assert_eq!(ended.len(), length);
    assert_eq!(ended.raised(), raised + 1);
}

#[test]
fn a_document_without_an_extension_block_is_not_reported_on_an_open_or_an_ended_list() {
    let mut open = DiagnosticList::new();
    assert!(!refuse(
        &document(false),
        &Specialization::default(),
        &mut open
    ));
    assert!(open.is_empty());

    let mut ended = DiagnosticList::new();
    ended.leave_unlisted();
    assert!(!refuse(
        &document(false),
        &Specialization::default(),
        &mut ended
    ));
    assert_eq!(ended.raised(), 1);
}
