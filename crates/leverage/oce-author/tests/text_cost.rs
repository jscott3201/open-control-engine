//! A text document at the statement cap whose every block carries a finding has every finding
//! placed (issue #482 review, ruling C: placing findings on the text indexes the subjects once;
//! the review measured 17.8 s for 64,000 blocks before the index, against 31 ms for `validate`).
//!
//! What the placement costs at that size is pinned in key comparisons by the unit tests of
//! `src/counted.rs` (issue #478), not here and not in elapsed time.

use oce_author::{MAX_STATEMENTS_PER_KIND, MAX_TEXT_BYTES, validate_text_author_side};

/// `n` blocks of an unregistered class: one `class-not-found` finding each.
fn document(n: usize) -> String {
    let mut text = String::from("sequence s\nderived_from none\n");
    for i in 0..n {
        text.push_str(&format!("block b{i} = CDL.Reals.Nope\n"));
    }
    text
}

#[test]
fn a_document_at_the_statement_cap_places_every_finding() {
    let n = MAX_STATEMENTS_PER_KIND;
    let text = document(n);
    assert!(text.len() <= MAX_TEXT_BYTES);
    let findings = validate_text_author_side(&text).expect("parses");
    assert_eq!(findings.len(), n);
    let last = findings.last().expect("findings");
    assert_eq!(last.subject, format!("b{}", n - 1));
    assert_eq!(
        last.span.map(|at| (at.line, at.column)),
        Some((u32::try_from(n + 2).expect("fits"), 7))
    );
}
