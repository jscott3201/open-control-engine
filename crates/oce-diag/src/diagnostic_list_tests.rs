//! The text budget of a [`DiagnosticList`]: what counts, where the list ends, and what the
//! closing error says.

use std::cell::Cell;
use std::sync::Arc;

use crate::{DiagCode, Diagnostic, Severity, has_errors};

use super::{DiagnosticList, MAX_DIAGNOSTIC_TEXT_BYTES};

/// The budget, restated: a change to the constant fails here until it is reviewed.
const BUDGET: usize = 8 * 1024 * 1024;

/// A warning whose message is `message_bytes` of `m`, with no subject.
fn warning(message_bytes: usize) -> Diagnostic {
    Diagnostic::warning(DiagCode::UnknownProperty, "m".repeat(message_bytes))
}

/// The closing error for `unlisted` diagnostics, the first of them described as `first`, stated
/// by hand.
fn closing(unlisted: u64, first: &str) -> Diagnostic {
    Diagnostic::error(
        DiagCode::ResourceLimitExceeded,
        format!(
            "{unlisted} more diagnostic(s) are not listed, the first of them {first}: a \
             diagnostic list holds at most 8388608 bytes of message and subject text"
        ),
    )
}

/// How the closing error describes a [`warning`] that ended a list.
const A_WARNING: &str = "`unknown-property` with no subject";

#[test]
fn the_budget_is_eight_mebibytes() {
    assert_eq!(MAX_DIAGNOSTIC_TEXT_BYTES, BUDGET);
}

#[test]
fn a_list_within_the_budget_holds_every_diagnostic_in_the_order_raised() {
    let mut list = DiagnosticList::new();
    assert!(list.is_empty());
    assert_eq!(list.raised(), 0);
    let raised = [
        Diagnostic::error(DiagCode::MalformedDocument, "first").with_subject("b"),
        Diagnostic::warning(DiagCode::UnknownProperty, "second"),
        Diagnostic::info(DiagCode::AnalogCoercedToReal, "third").with_subject("a"),
    ];
    list.push(raised[0].clone());
    list.raise(|| raised[1].clone());
    list.extend([raised[2].clone()]);
    assert_eq!(&*list, &raised);
    assert_eq!(list.raised(), 3);
    let (listed, closing) = list.into_parts();
    assert_eq!(listed, raised);
    assert_eq!(closing, None);
}

#[test]
fn text_that_fills_the_budget_exactly_is_listed_and_one_byte_more_ends_the_list() {
    // Message and subject both count: 1 MiB of subject and the rest in the message.
    let subject: Arc<str> = Arc::from("s".repeat(1 << 20));
    let at_budget = |message_bytes: usize| {
        let mut list = DiagnosticList::new();
        list.push(warning(message_bytes).with_subject(Arc::clone(&subject)));
        list
    };

    let exact = at_budget(BUDGET - (1 << 20));
    assert_eq!(exact.len(), 1);
    assert!(!has_errors(&exact), "a warning that fits stays a warning");
    assert_eq!(exact.raised(), 1);

    let over = at_budget(BUDGET - (1 << 20) + 1);
    let first = format!("`unknown-property` on `{}…`", "s".repeat(64));
    assert_eq!(&*over, &[closing(1, &first)]);
    assert!(has_errors(&over));
    assert!(!over.is_empty());
    assert_eq!(over.raised(), 1);
}

#[test]
fn a_subject_alone_can_fill_the_budget_and_one_byte_more_of_it_ends_the_list() {
    let mut exact = DiagnosticList::new();
    exact.push(warning(0).with_subject("s".repeat(BUDGET)));
    assert_eq!(exact.len(), 1);
    assert!(!has_errors(&exact));

    let mut over = DiagnosticList::new();
    over.push(warning(0).with_subject("s".repeat(BUDGET + 1)));
    assert_eq!(over.len(), 1);
    assert_eq!(over[0].code, DiagCode::ResourceLimitExceeded);
}

#[test]
fn the_budget_is_spent_by_every_listed_diagnostic_together() {
    let mut list = DiagnosticList::new();
    // Eight diagnostics of 1 MiB fill the budget; a ninth of one byte does not fit.
    for _ in 0..8 {
        list.push(warning(1 << 20));
    }
    assert_eq!(list.len(), 8);
    assert!(!has_errors(&list));
    list.push(warning(1));
    assert_eq!(list.len(), 9);
    assert_eq!(list[8], closing(1, A_WARNING));
    assert_eq!(list.raised(), 9);
}

#[test]
fn a_subject_is_counted_for_every_diagnostic_that_carries_it_shared_or_copied() {
    // Eight diagnostics on a 1 MiB subject fill the budget with no message at all, whether the
    // eight share one allocation of it or each holds a copy: a consumer that prints or copies
    // each diagnostic pays for the subject each time.
    let text = "s".repeat(1 << 20);
    let shared: Arc<str> = Arc::from(text.as_str());
    let first = format!("`unknown-property` on `{}…`", "s".repeat(64));
    for share in [true, false] {
        let subject = || -> Arc<str> {
            if share {
                Arc::clone(&shared)
            } else {
                Arc::from(text.as_str())
            }
        };
        let mut list = DiagnosticList::new();
        for _ in 0..8 {
            list.push(warning(0).with_subject(subject()));
        }
        assert_eq!(list.len(), 8, "shared: {share}");
        assert!(!has_errors(&list), "shared: {share}");
        // A ninth on the same subject does not fit, though a shared one would hold nothing more.
        list.push(warning(0).with_subject(subject()));
        assert_eq!(list[8], closing(1, &first), "shared: {share}");
    }
}

#[test]
fn once_the_list_has_ended_nothing_more_is_listed_even_when_it_would_fit() {
    let mut list = DiagnosticList::new();
    list.push(warning(BUDGET));
    list.push(warning(1));
    assert_eq!(list.len(), 2);
    // An empty message costs nothing, and the list has ended all the same.
    list.push(warning(0));
    list.extend([warning(0), warning(0)]);
    assert_eq!(list.len(), 2);
    assert_eq!(list[1], closing(4, A_WARNING));
    assert_eq!(list.raised(), 5);
    let (listed, last) = list.into_parts();
    assert_eq!(listed, [warning(BUDGET)]);
    assert_eq!(last, Some(closing(4, A_WARNING)));
}

#[test]
fn a_diagnostic_larger_than_the_whole_budget_leaves_the_closing_error_that_names_it() {
    let mut list = DiagnosticList::new();
    list.push(
        Diagnostic::error(DiagCode::GroundingFailed, "m".repeat(BUDGET + 1))
            .with_subject("http://example.org#M.con.k"),
    );
    let first = "`grounding-failed` on `http://example.org#M.con.k`";
    assert_eq!(&*list, &[closing(1, first)]);
    assert_eq!(list.raised(), 1);
    // The list is not empty and has failed, so no caller reads it as a clean result.
    assert!(!list.is_empty());
    assert!(has_errors(&list));
    assert_eq!(list.into_parts(), (Vec::new(), Some(closing(1, first))));
}

#[test]
fn the_closing_error_names_the_first_diagnostic_left_out_and_no_later_one() {
    let mut list = DiagnosticList::new();
    list.push(warning(BUDGET));
    list.push(Diagnostic::error(DiagCode::RelativeIri, "x").with_subject("urn:first"));
    list.push(Diagnostic::error(DiagCode::DuplicateId, "x").with_subject("urn:second"));
    list.raise(|| Diagnostic::error(DiagCode::SharedInstance, "x").with_subject("urn:third"));
    assert_eq!(list[1], closing(3, "`relative-iri` on `urn:first`"));
}

#[test]
fn the_closing_error_quotes_a_subject_in_at_most_64_bytes_with_controls_escaped() {
    // A megabyte of subject is an excerpt, and a BEL and a right-to-left override are escapes.
    let mut long = DiagnosticList::new();
    long.push(warning(BUDGET));
    long.push(warning(0).with_subject("k".repeat(1 << 20)));
    let quoted = format!("`unknown-property` on `{}…`", "k".repeat(64));
    assert_eq!(long[1], closing(1, &quoted));

    let mut hidden = DiagnosticList::new();
    hidden.push(warning(BUDGET));
    hidden.push(warning(0).with_subject("urn:a\u{7}b\u{202E}c"));
    assert_eq!(
        hidden[1],
        closing(1, "`unknown-property` on `urn:a\\u{0007}b\\u{202E}c`")
    );

    // The 64 bytes are the subject's own, cut before the escapes are written: the BEL and the
    // override are 4 of the first 64 bytes, so 54 of the `k`s after them are quoted, where
    // cutting the escaped text would quote 42.
    let mut cut = DiagnosticList::new();
    cut.push(warning(BUDGET));
    cut.push(warning(0).with_subject(format!("urn:a\u{7}b\u{202E}{}", "k".repeat(100))));
    let quoted = format!(
        "`unknown-property` on `urn:a\\u{{0007}}b\\u{{202E}}{}…`",
        "k".repeat(54)
    );
    assert_eq!(cut[1], closing(1, &quoted));
}

#[test]
fn the_closing_error_is_an_error_with_no_subject_whatever_was_left_out() {
    let mut list = DiagnosticList::new();
    list.push(warning(BUDGET));
    list.push(Diagnostic::info(DiagCode::AnalogCoercedToReal, "x"));
    let last = list.last().expect("the closing error");
    assert_eq!(last.severity, Severity::Error);
    assert_eq!(last.code, DiagCode::ResourceLimitExceeded);
    assert_eq!(last.subject, None);
}

#[test]
fn raise_builds_a_diagnostic_only_while_the_list_has_not_ended() {
    let built = Cell::new(0_u32);
    let build = |message_bytes: usize| {
        built.set(built.get() + 1);
        warning(message_bytes)
    };
    let mut list = DiagnosticList::new();
    list.raise(|| build(BUDGET));
    assert_eq!(built.get(), 1);
    // This one is built, since only building it tells that it does not fit.
    list.raise(|| build(1));
    assert_eq!(built.get(), 2);
    for _ in 0..1_000 {
        list.raise(|| build(1));
    }
    assert_eq!(built.get(), 2, "nothing is built once the list has ended");
    assert_eq!(list[1], closing(1_001, A_WARNING));
    assert_eq!(list.raised(), 1_002);
}

#[test]
fn counting_past_the_end_allocates_only_when_the_count_gains_a_digit() {
    let mut list = DiagnosticList::new();
    list.push(warning(BUDGET));
    list.push(warning(1));
    let capacity = list[1].message.capacity();
    let address = list[1].message.as_ptr();
    for _ in 0..8 {
        list.raise(|| warning(1));
    }
    // Nine unlisted: the count is still one digit, and the message is rewritten in place.
    assert_eq!(list[1], closing(9, A_WARNING));
    assert_eq!(list[1].message.capacity(), capacity);
    assert_eq!(list[1].message.as_ptr(), address);
}

#[test]
fn a_listed_message_holds_no_capacity_beyond_its_length() {
    // A message grown by formatting may hold up to twice its length.
    let mut message = String::with_capacity(4_096);
    message.push_str("short");
    let mut list = DiagnosticList::new();
    list.push(Diagnostic::warning(DiagCode::UnknownProperty, message));
    assert_eq!(list[0].message, "short");
    assert_eq!(list[0].message.capacity(), list[0].message.len());
}

#[cfg(target_pointer_width = "64")]
#[test]
fn a_listed_diagnostic_is_a_record_of_48_bytes_beside_its_text() {
    assert_eq!(std::mem::size_of::<Diagnostic>(), 48);
}

#[test]
fn a_list_collects_from_and_yields_its_diagnostics_in_order_the_closing_error_last() {
    let list: DiagnosticList = [warning(1), warning(BUDGET), warning(2)]
        .into_iter()
        .collect();
    assert_eq!(list.raised(), 3);
    let yielded: Vec<Diagnostic> = list.into_iter().collect();
    assert_eq!(yielded, [warning(1), closing(2, A_WARNING)]);
}

#[test]
fn a_list_formats_for_debugging_as_the_diagnostics_it_holds() {
    let mut list = DiagnosticList::new();
    assert_eq!(format!("{list:?}"), "[]");
    list.push(warning(1).with_subject("s"));
    list.push(warning(BUDGET));
    let held = vec![warning(1).with_subject("s"), closing(1, A_WARNING)];
    assert_eq!(format!("{list:?}"), format!("{held:?}"));
}
