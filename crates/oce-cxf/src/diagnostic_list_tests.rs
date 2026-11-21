//! The text budget of a [`DiagnosticList`]: what counts, where the list ends, and what the
//! closing error says.

use std::cell::Cell;
use std::sync::Arc;

use oce_diag::{DiagCode, Diagnostic, Severity, has_errors};

use super::{DiagnosticList, MAX_DIAGNOSTIC_TEXT_BYTES};

/// The budget, restated: a change to the constant fails here until it is reviewed.
const BUDGET: usize = 8 * 1024 * 1024;

/// A warning whose message is `message_bytes` of `m`, with no subject.
fn warning(message_bytes: usize) -> Diagnostic {
    Diagnostic::warning(DiagCode::UnknownProperty, "m".repeat(message_bytes))
}

/// The closing error for `unlisted` diagnostics, stated by hand.
fn closing(unlisted: u64) -> Diagnostic {
    Diagnostic::error(
        DiagCode::ResourceLimitExceeded,
        format!(
            "{unlisted} more diagnostic(s) are not listed: a diagnostic list holds at most \
             8388608 bytes of message and subject text"
        ),
    )
}

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
    assert_eq!(&*over, &[closing(1)]);
    assert!(has_errors(&over));
    assert!(!over.is_empty());
    assert_eq!(over.raised(), 1);
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
    assert_eq!(list[8], closing(1));
    assert_eq!(list.raised(), 9);
}

#[test]
fn a_subject_shared_as_one_allocation_is_counted_once() {
    // Seven messages of 1 MiB and one 1 MiB subject, held once, fill the budget exactly.
    let subject: Arc<str> = Arc::from("s".repeat(1 << 20));
    let mut list = DiagnosticList::new();
    for _ in 0..7 {
        list.push(warning(1 << 20).with_subject(Arc::clone(&subject)));
    }
    assert_eq!(list.len(), 7);
    // The budget is full, and one more diagnostic on the same allocation costs nothing.
    list.push(warning(0).with_subject(Arc::clone(&subject)));
    assert_eq!(list.len(), 8);
    assert!(!has_errors(&list));
    list.push(warning(1));
    assert_eq!(list[8], closing(1));
}

#[test]
fn a_subject_copied_for_each_diagnostic_is_counted_for_each() {
    // The same megabyte of text, copied eight times, fills the budget with no message at all.
    let text = "s".repeat(1 << 20);
    let mut list = DiagnosticList::new();
    for _ in 0..8 {
        list.push(warning(0).with_subject(text.as_str()));
    }
    assert_eq!(list.len(), 8);
    assert!(!has_errors(&list));
    list.push(warning(0).with_subject("s"));
    assert_eq!(list[8], closing(1));
}

#[test]
fn a_shared_subject_that_does_not_fit_is_counted_again_when_it_is_next_raised() {
    // The subject of a diagnostic that ended the list was never held, so it was never counted.
    let subject: Arc<str> = Arc::from("s".repeat(BUDGET));
    let mut list = DiagnosticList::new();
    list.push(warning(1).with_subject(Arc::clone(&subject)));
    list.push(warning(0).with_subject(Arc::clone(&subject)));
    assert_eq!(&*list, &[closing(2)]);
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
    assert_eq!(list[1], closing(4));
    assert_eq!(list.raised(), 5);
    let (listed, last) = list.into_parts();
    assert_eq!(listed, [warning(BUDGET)]);
    assert_eq!(last, Some(closing(4)));
}

#[test]
fn a_diagnostic_larger_than_the_whole_budget_leaves_the_closing_error_alone() {
    let mut list = DiagnosticList::new();
    list.push(warning(BUDGET + 1));
    assert_eq!(&*list, &[closing(1)]);
    assert_eq!(list.raised(), 1);
    assert_eq!(list.into_parts(), (Vec::new(), Some(closing(1))));
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
    assert_eq!(list[1], closing(1_001));
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
    assert_eq!(list[1], closing(9));
    assert_eq!(list[1].message.capacity(), capacity);
    assert_eq!(list[1].message.as_ptr(), address);
}

#[test]
fn append_adds_what_the_other_list_raised_in_order_with_its_unlisted_count() {
    let mut first = DiagnosticList::new();
    first.push(warning(1));
    let mut second = DiagnosticList::new();
    second.push(warning(2));
    second.push(warning(3));
    first.append(second);
    assert_eq!(&*first, &[warning(1), warning(2), warning(3)]);
    assert_eq!(first.raised(), 3);

    // A list that ended brings its count with it, and ends the list it joins.
    let mut ended = DiagnosticList::new();
    ended.push(warning(4));
    ended.push(warning(BUDGET));
    ended.push(warning(0));
    assert_eq!(ended.raised(), 3);
    first.append(ended);
    assert_eq!(
        &*first,
        &[warning(1), warning(2), warning(3), warning(4), closing(2)]
    );
    assert_eq!(first.raised(), 6);

    // What the joined list has no room for is counted there, beside what the other left out.
    let mut full = DiagnosticList::new();
    full.push(warning(BUDGET));
    let mut more = DiagnosticList::new();
    more.push(warning(1));
    more.push(warning(1));
    full.append(more);
    assert_eq!(&*full, &[warning(BUDGET), closing(2)]);
}

#[test]
fn leaving_one_unlisted_ends_an_open_list_and_counts_on_an_ended_one() {
    let mut list = DiagnosticList::new();
    list.push(warning(1));
    list.leave_unlisted();
    assert_eq!(&*list, &[warning(1), closing(1)]);
    list.leave_unlisted();
    assert_eq!(&*list, &[warning(1), closing(2)]);
    assert_eq!(list.raised(), 3);
    // Nothing is listed after it, though the budget has room.
    list.push(warning(1));
    assert_eq!(&*list, &[warning(1), closing(3)]);
}

#[test]
fn a_list_collects_from_and_yields_its_diagnostics_in_order_the_closing_error_last() {
    let list: DiagnosticList = [warning(1), warning(BUDGET), warning(2)]
        .into_iter()
        .collect();
    assert_eq!(list.raised(), 3);
    let yielded: Vec<Diagnostic> = list.into_iter().collect();
    assert_eq!(yielded, [warning(1), closing(2)]);
}

#[test]
fn a_list_formats_for_debugging_as_the_diagnostics_it_holds() {
    let mut list = DiagnosticList::new();
    assert_eq!(format!("{list:?}"), "[]");
    list.push(warning(1).with_subject("s"));
    list.leave_unlisted();
    let held = vec![warning(1).with_subject("s"), closing(1)];
    assert_eq!(format!("{list:?}"), format!("{held:?}"));
}
