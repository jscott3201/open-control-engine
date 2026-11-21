//! What [`refuse`] reports to its caller, which ends the resolve on `true`.

use oce_diag::{DiagCode, Diagnostic};

use super::refuse;
use crate::dto::CxfDocument;
use crate::resolve::specialize::Specialization;
use oce_diag::DiagnosticList;

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

/// Whether `document` holds an active extension block, as `refuse` reports it on `list`.
fn reported(document: &CxfDocument, list: &mut DiagnosticList) -> bool {
    refuse(document, &Specialization::default(), list)
}

/// A list that has ended: one diagnostic too large for any list was raised on it.
fn ended_list() -> DiagnosticList {
    let mut list = DiagnosticList::new();
    list.push(Diagnostic::error(
        DiagCode::MalformedDocument,
        "m".repeat(8 * 1024 * 1024 + 1),
    ));
    assert_eq!((list.len(), list.raised()), (1, 1));
    list
}

#[test]
fn an_extension_block_is_reported_whether_or_not_the_list_has_room_for_its_refusal() {
    let mut open = DiagnosticList::new();
    assert!(reported(&document(true), &mut open));
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].code, DiagCode::ExtensionBlockUnsupported);

    // A list that has ended lists nothing more, so its length does not move. The refusal is
    // still raised and counted, and the caller is still told to end the resolve.
    let mut ended = ended_list();
    assert!(reported(&document(true), &mut ended));
    assert_eq!((ended.len(), ended.raised()), (1, 2));
}

#[test]
fn a_document_without_an_extension_block_is_not_reported_on_an_open_or_an_ended_list() {
    let mut open = DiagnosticList::new();
    assert!(!reported(&document(false), &mut open));
    assert!(open.is_empty());

    let mut ended = ended_list();
    assert!(!reported(&document(false), &mut ended));
    assert_eq!(ended.raised(), 1);
}
