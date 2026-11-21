//! What the specialize pass withholds and what lowering releases of it: tagged findings held as
//! references into the document, built into diagnostics only at release (GitHub issue #695).

use std::collections::HashSet;

use oce_diag::{DiagCode, Diagnostic};

use super::declaration_scope::{TaggedFinding, WithheldFindings};
use crate::diagnostic_list::DiagnosticList;

/// The finding that `iri` re-binds `k`, first declared at `first`.
fn rebinding<'a>(iri: &'a str, first: &'a str) -> TaggedFinding<'a> {
    TaggedFinding::Duplicate {
        iri,
        name: "k",
        first,
    }
}

/// The diagnostic of [`rebinding`], stated by hand.
fn rebinding_diagnostic(iri: &str, first: &str) -> Diagnostic {
    Diagnostic::error(
        DiagCode::MalformedDocument,
        format!(
            "composite/duplicate-declaration: own declaration {iri} re-binds local name `k` \
             first declared at {first}"
        ),
    )
    .with_subject(iri)
}

/// What `withheld` releases into an empty list when lowering evaluated the chains `evaluated`.
fn released(withheld: WithheldFindings<'_>, evaluated: &[&str]) -> Vec<Diagnostic> {
    let evaluated: HashSet<String> = evaluated.iter().map(|chain| (*chain).to_owned()).collect();
    let mut diags = DiagnosticList::new();
    withheld.emit_unvisited(&evaluated, &mut diags);
    diags.into_iter().collect()
}

#[test]
fn withheld_findings_release_exactly_the_chains_lowering_did_not_evaluate_in_recording_order() {
    let record = || {
        let mut withheld = WithheldFindings::default();
        withheld.record(
            "urn:a",
            vec![
                rebinding("urn:a.x.k", "urn:a.k"),
                rebinding("urn:a.y.k", "urn:a.k"),
            ],
        );
        withheld.record("urn:none", Vec::new());
        withheld.record("urn:b", vec![rebinding("urn:b.x.k", "urn:b.k")]);
        withheld.record(
            "urn:c",
            vec![TaggedFinding::Cycle {
                participants: vec!["urn:c.p", "urn:c.q"],
            }],
        );
        withheld
    };
    let a = [
        rebinding_diagnostic("urn:a.x.k", "urn:a.k"),
        rebinding_diagnostic("urn:a.y.k", "urn:a.k"),
    ];
    let b = rebinding_diagnostic("urn:b.x.k", "urn:b.k");
    let c = Diagnostic::error(
        DiagCode::MalformedDocument,
        "composite/declaration-cycle: cycle in the block's own declaration references: \
         urn:c.p -> urn:c.q -> urn:c.p",
    )
    .with_subject("urn:c.p");
    assert_eq!(
        released(record(), &[]),
        [a[0].clone(), a[1].clone(), b.clone(), c.clone()]
    );
    // A chain lowering evaluated is passed over whole, wherever it was recorded.
    assert_eq!(released(record(), &["urn:a"]), [b.clone(), c.clone()]);
    assert_eq!(
        released(record(), &["urn:b"]),
        [a[0].clone(), a[1].clone(), c.clone()]
    );
    assert_eq!(
        released(record(), &["urn:a", "urn:b", "urn:c"]),
        Vec::<Diagnostic>::new()
    );
}

#[test]
fn a_withheld_chain_of_long_findings_takes_nothing_from_a_chain_released_after_it() {
    // The first chain re-binds a name first declared under a megabyte of IRI, forty times. What
    // is withheld for it is references, so when lowering has evaluated that chain itself, the
    // second chain's findings are released whole into a list with all its room.
    let long = format!("urn:long:{}.k", "x".repeat(1 << 20));
    let again: Vec<String> = (0..40).map(|i| format!("urn:d{i}.k")).collect();
    let mut withheld = WithheldFindings::default();
    withheld.record(
        "urn:root",
        again.iter().map(|iri| rebinding(iri, &long)).collect(),
    );
    withheld.record(
        "urn:c",
        vec![
            rebinding("urn:c.x.k", "urn:c.k"),
            rebinding("urn:c.y.k", "urn:c.k"),
        ],
    );
    assert_eq!(
        released(withheld, &["urn:root"]),
        [
            rebinding_diagnostic("urn:c.x.k", "urn:c.k"),
            rebinding_diagnostic("urn:c.y.k", "urn:c.k"),
        ]
    );
}

#[test]
fn a_released_finding_is_built_only_while_the_list_has_room_and_is_counted_past_it() {
    // Seven megabyte findings fit an empty list; the eighth ends it, and the rest are counted.
    let long = format!("urn:long:{}.k", "x".repeat(1 << 20));
    let again: Vec<String> = (0..40).map(|i| format!("urn:d{i}.k")).collect();
    let mut withheld = WithheldFindings::default();
    withheld.record(
        "urn:root",
        again.iter().map(|iri| rebinding(iri, &long)).collect(),
    );
    let released = released(withheld, &[]);
    assert_eq!(released.len(), 8);
    for (finding, iri) in released[..7].iter().zip(&again) {
        assert_eq!(finding, &rebinding_diagnostic(iri, &long));
    }
    assert_eq!(released[7].code, DiagCode::ResourceLimitExceeded);
    assert!(
        released[7].message.starts_with(
            "33 more diagnostic(s) are not listed, the first of them `malformed-document` on \
             `urn:d7.k`: "
        ),
        "{}",
        released[7].message
    );
}
