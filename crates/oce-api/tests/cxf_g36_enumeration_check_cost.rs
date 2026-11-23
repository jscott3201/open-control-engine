//! What the G36 enumeration check costs a read when one parameter node with a long value is
//! listed by many instances, in requested bytes and never in elapsed time (issue #799;
//! `TESTING.md`: cost is asserted with deterministic measures).
//!
//! A parameter typed by a G36 enumeration has its value judged before grounding, once for each
//! instance that lists it, and a refusal quotes the value's whole text (`G36 enum literal `<text>`
//! is not fully qualified`). The check built that message for every listing, so a node with a
//! 64 KiB value listed by `N` instances built `N × 64 KiB` of messages, even once the read's
//! diagnostic list had ended at its 8 MiB and kept none of them. A refusal is now built only while
//! the list has room for it, so what the messages cost a read is held to the list's budget
//! whatever `N` is.
//!
//! The value is a string literal, which grounds once for the document (#749), so what is
//! measured is the check's own cost. The documents are the minimal loop with `N` more `Constant`
//! leaves that each list the node, a name their class does not declare: the facade refuses them,
//! so the reader (`import_cxf`, the facade's first step) is measured. Measured with
//! `allocation-counter`, whose counters are local to the measuring thread. The ceilings have
//! headroom over what was measured (in brackets, with what the same document cost before the
//! change).

use allocation_counter::{AllocationInfo, measure};
use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::{DiagCode, Diagnostic};

use crate::cxf_minted_array_cost::{MODEL, named_leaves};

/// The G36 enumeration the node is typed by.
const VENTILATION: &str = "Buildings.Controls.OBC.ASHRAE.G36.Types.VentilationStandard";

/// A CDL string literal of `bytes` bytes, quotes included: a value with no `.`, so the check
/// refuses it as a literal that is not fully qualified.
fn string_literal(bytes: usize) -> String {
    format!("\"{}\"", "x".repeat(bytes - 2))
}

/// The minimal loop with `leaves` more leaves, each listing the node `urn:g.e` typed by
/// [`VENTILATION`] whose value is the CDL text `text`.
fn listed_by(leaves: usize, text: &str) -> Vec<u8> {
    let node = format!(
        "{{ \"@id\": \"urn:g.e\", \"S231:isOfDataType\": {{ \"@id\": \
         \"http://example.org#{VENTILATION}\" }}, \"S231:value\": \"{}\" }}",
        text.replace('"', "\\\"")
    );
    named_leaves(
        leaves,
        |i| format!("urn:c{i}"),
        |_| vec!["urn:g.e".to_owned()],
        &[node],
    )
}

/// One read of `bytes`, measured: its diagnostics and what it requested.
fn refused(bytes: &[u8]) -> (Vec<Diagnostic>, AllocationInfo) {
    import_cxf(MODEL.as_bytes(), &ResolveOptions::default()).expect("the model reads");
    let mut result = None;
    let info = measure(|| result = Some(import_cxf(bytes, &ResolveOptions::default())));
    match result.expect("the read ran") {
        Err(CxfError::Validation(diagnostics)) => (diagnostics, info),
        Err(other) => panic!("not a validation refusal: {other}"),
        Ok(_) => panic!("the document read"),
    }
}

/// The refusal one listing raises for the value `text`, stated by hand.
fn refusal(text: &str) -> Diagnostic {
    Diagnostic::error(
        DiagCode::UnknownEnumLiteral,
        format!("G36 enum literal `{text}` is not fully qualified"),
    )
    .with_subject("urn:g.e".to_owned())
}

#[test]
fn a_long_value_many_instances_list_costs_its_refusals_at_most_the_list_budget() {
    // 256, 512 and 1,024 leaves list one node whose value is 64 KiB. The refusals fill the list's
    // 8 MiB, and the rest are counted without being built: from 1x to 4x the leaves, what the
    // read requests grows by each leaf's own share of the document, never by a copy of the value.
    // [measured: totals of 42,656,686 / 49,200,742 / 62,289,774, 25,563 bytes a further leaf,
    // peaks of 12.3 / 15.7 / 22.7 MB; before, totals of 67,844,910 / 124,765,414 / 238,607,342,
    // 222,346 bytes a further leaf]
    let text = string_literal(64 * 1024);
    let mut totals = Vec::new();
    for leaves in [256_u64, 512, 1_024] {
        let (diagnostics, info) = refused(&listed_by(leaves as usize, &text));
        let (closing, listed) = diagnostics.split_last().expect("a list");
        assert_eq!(closing.code, DiagCode::ResourceLimitExceeded, "{leaves}");
        // Each leaf raises the refusal and the list holds 127 of them; the closing error counts
        // the rest with whatever else the read raised after the list ended.
        let unlisted: u64 = closing
            .message
            .split(' ')
            .next()
            .and_then(|n| n.parse().ok())
            .expect("a count");
        assert!(unlisted >= leaves - 127, "{leaves}: {}", closing.message);
        assert!(
            closing.message.contains(
                " more diagnostic(s) are not listed, the first of them `unknown-enum-literal` \
                 on `urn:g.e`"
            ),
            "{leaves}: {}",
            closing.message
        );
        assert_eq!(listed.len(), 127, "{leaves}");
        assert!(listed.iter().all(|d| *d == refusal(&text)), "{leaves}");
        assert!(info.bytes_max <= 32 * 1024 * 1024, "{leaves}: {info:?}");
        totals.push(info.bytes_total);
    }
    // A further leaf costs well under the one copy of the value its refusal would quote.
    let per_leaf = (totals[2] - totals[0]) / (1_024 - 256);
    assert!(
        per_leaf <= 32 * 1024,
        "{per_leaf} bytes a further leaf: {totals:?}"
    );
    assert!(totals[1] - totals[0] <= 256 * 32 * 1024, "{totals:?}");
}

#[test]
fn every_listing_raises_the_same_refusal_while_the_list_has_room() {
    // Short values: each of 64 listings raises its refusal, naming the node, for a literal that
    // is not fully qualified and for one that is no member of the declared enumeration.
    let text = string_literal(16);
    let (diagnostics, _) = refused(&listed_by(64, &text));
    assert_eq!(
        diagnostics.iter().filter(|d| **d == refusal(&text)).count(),
        64,
        "{diagnostics:?}"
    );
    let unknown = format!("{VENTILATION}.notAMember");
    let (diagnostics, _) = refused(&listed_by(64, &unknown));
    let expected = Diagnostic::error(
        DiagCode::UnknownEnumLiteral,
        format!("unknown G36 enum literal `{unknown}`"),
    )
    .with_subject("urn:g.e".to_owned());
    assert_eq!(
        diagnostics.iter().filter(|d| **d == expected).count(),
        64,
        "{diagnostics:?}"
    );
}

/// The minimal loop with `leaves` more leaves, each listing the node `urn:g.e` of
/// [`listed_by`] with the value `text`, then the node `urn:g.t` whose value is `1.0`, typed by
/// `type_iri` when one is given.
fn listed_with_typed(leaves: usize, text: &str, type_iri: Option<&str>) -> Vec<u8> {
    let long = format!(
        "{{ \"@id\": \"urn:g.e\", \"S231:isOfDataType\": {{ \"@id\": \
         \"http://example.org#{VENTILATION}\" }}, \"S231:value\": \"{}\" }}",
        text.replace('"', "\\\"")
    );
    let typed = match type_iri {
        Some(iri) => format!(
            "{{ \"@id\": \"urn:g.t\", \"S231:isOfDataType\": {{ \"@id\": \"{iri}\" }}, \
             \"S231:value\": 1.0 }}"
        ),
        None => "{ \"@id\": \"urn:g.t\", \"S231:value\": 1.0 }".to_owned(),
    };
    named_leaves(
        leaves,
        |i| format!("urn:c{i}"),
        |_| vec!["urn:g.e".to_owned(), "urn:g.t".to_owned()],
        &[long, typed],
    )
}

#[test]
fn a_typing_iri_refusal_is_built_only_while_the_list_has_room() {
    // 256 and 512 leaves each list the long-valued node, whose refusals end the list after about
    // 127 leaves, and a node typed by an unknown CDL enumeration, whose refusal (283 bytes) names
    // every CDL enumeration. Against the same node untyped, a further leaf adds less than one
    // refusal: the typed node's refusal is built only while the list has room. [measured: the typed
    // node adds 127,747 bytes at 256 leaves and at 512, none a further leaf]
    let text = string_literal(64 * 1024);
    let unknown = "http://example.org#Buildings.Controls.OBC.CDL.Types.ZeroTim";
    let mut added = Vec::new();
    let mut message_bytes = 0;
    for leaves in [256, 512] {
        let (diagnostics, typed) = refused(&listed_with_typed(leaves, &text, Some(unknown)));
        message_bytes = diagnostics
            .iter()
            .find(|d| d.subject.as_deref() == Some("urn:g.t"))
            .expect("the typed node is refused")
            .message
            .len() as u64;
        let (_, untyped) = refused(&listed_with_typed(leaves, &text, None));
        added.push(typed.bytes_total - untyped.bytes_total);
    }
    let per_leaf = (added[1] - added[0]) / 256;
    assert!(
        per_leaf < message_bytes,
        "{per_leaf} bytes a further leaf: {added:?}"
    );
}
