//! The one-to-one pairing over constructed trace rows. The integration tests drive it through a
//! real run (`tests/verdicts.rs`); these pin the order on shapes a subject is awkward to build.

use oce_api::AssertLevel;
use oce_replay::DiagnosticRow;

use super::{matches, pair};
use crate::bind::{BoundDiagnostic, BoundExpectation};

const ASSERT: &str = "CDL.Utilities.Assert";
const AVERAGE: &str = "CDL.Reals.MovingAverage";

fn row(frame: u64, block: &str, message: &str) -> DiagnosticRow {
    #[expect(clippy::cast_precision_loss, reason = "a small test frame index")]
    let time = frame as f64 * 60.0;
    DiagnosticRow {
        frame,
        block: block.to_owned(),
        message: message.to_owned(),
        time,
        level: AssertLevel::Warning,
    }
}

fn diagnostic(tick: u64, block: &str, message: Option<&str>) -> BoundDiagnostic {
    BoundDiagnostic {
        tick,
        block: block.to_owned(),
        message: message.map(str::to_owned),
    }
}

fn expect(tick: u64, block: &str, message: Option<&str>) -> BoundExpectation {
    BoundExpectation::Diagnostic(diagnostic(tick, block, message))
}

/// The pairing as two lists: which expectations matched, which rows were taken.
fn outcome(
    rows: &[DiagnosticRow],
    expectations: &[BoundExpectation],
    compared: u64,
) -> (Vec<bool>, Vec<bool>) {
    let pairing = pair(rows, expectations, compared);
    (pairing.matched, pairing.taken)
}

#[test]
fn a_warning_matches_the_block_and_the_message_when_given() {
    let named = diagnostic(1, ASSERT, Some("b is false"));
    assert!(matches(&row(1, ASSERT, "b is false"), &named));
    assert!(
        !matches(&row(1, AVERAGE, "b is false"), &named),
        "another class"
    );
    assert!(
        !matches(&row(1, ASSERT, "b is False"), &named),
        "byte for byte"
    );
    assert!(!matches(&row(1, ASSERT, "b is false "), &named));
    let any = diagnostic(1, ASSERT, None);
    assert!(matches(&row(1, ASSERT, "anything at all"), &any));
    assert!(matches(&row(1, ASSERT, ""), &any));
    assert!(!matches(&row(1, AVERAGE, ""), &any));
}

#[test]
fn one_expectation_takes_one_of_two_equal_warnings_and_leaves_the_other() {
    // Two `Assert` instances warn with one text at tick 1; one expectation names it.
    let rows = [row(1, ASSERT, "b is false"), row(1, ASSERT, "b is false")];
    for expectation in [
        expect(1, ASSERT, Some("b is false")),
        expect(1, ASSERT, None),
    ] {
        let (matched, taken) = outcome(&rows, &[expectation], 31);
        assert_eq!(matched, [true]);
        // The earliest in emission order is taken; the second stays unmatched.
        assert_eq!(taken, [true, false]);
    }
}

#[test]
fn two_expectations_take_two_equal_warnings_one_each() {
    let rows = [row(1, ASSERT, "b is false"), row(1, ASSERT, "b is false")];
    let expectations = [
        expect(1, ASSERT, Some("b is false")),
        expect(1, ASSERT, Some("b is false")),
    ];
    assert_eq!(
        outcome(&rows, &expectations, 31),
        (vec![true, true], vec![true, true])
    );
}

#[test]
fn with_more_expectations_than_warnings_the_later_expectation_fails() {
    let rows = [row(1, ASSERT, "b is false")];
    let expectations = [
        expect(1, ASSERT, Some("b is false")),
        expect(1, ASSERT, Some("b is false")),
    ];
    assert_eq!(
        outcome(&rows, &expectations, 31),
        (vec![true, false], vec![true])
    );
}

#[test]
fn an_expectation_naming_a_message_is_paired_before_a_block_only_one() {
    // The block-only expectation comes first in the file, and either warning would do for it;
    // the message-bearing one needs the first. Pairing the named one first passes both.
    let rows = [
        row(1, ASSERT, "freezestat tripped"),
        row(1, ASSERT, "b is false"),
    ];
    let expectations = [
        expect(1, ASSERT, None),
        expect(1, ASSERT, Some("freezestat tripped")),
    ];
    assert_eq!(
        outcome(&rows, &expectations, 31),
        (vec![true, true], vec![true, true])
    );
    // Each takes the earliest free warning it matches: the named one the first `b is false`, the
    // block-only one the next free warning, and the third warning is left unmatched.
    let rows = [
        row(1, ASSERT, "b is false"),
        row(1, ASSERT, "b is false"),
        row(1, ASSERT, "freezestat tripped"),
    ];
    let expectations = [
        expect(1, ASSERT, None),
        expect(1, ASSERT, Some("b is false")),
    ];
    assert_eq!(
        outcome(&rows, &expectations, 31),
        (vec![true, true], vec![true, true, false])
    );
}

#[test]
fn only_warnings_of_the_expectations_own_tick_and_class_are_offered() {
    let rows = [
        row(0, ASSERT, "b is false"),
        row(1, AVERAGE, "close ticks"),
        row(2, ASSERT, "b is false"),
    ];
    let expectations = [expect(1, ASSERT, None)];
    assert_eq!(
        outcome(&rows, &expectations, 31),
        (vec![false], vec![false, false, false])
    );
}

#[test]
fn an_expectation_at_or_after_the_refused_tick_takes_nothing() {
    let rows = [row(0, ASSERT, "b is false"), row(1, ASSERT, "b is false")];
    let expectations = [expect(0, ASSERT, None), expect(1, ASSERT, None)];
    // Only tick 0 ran before the refusal at row 1.
    assert_eq!(
        outcome(&rows, &expectations, 1),
        (vec![true, false], vec![true, false])
    );
}

#[test]
fn a_window_expectation_is_never_matched() {
    let window = BoundExpectation::Window(crate::bind::BoundWindow {
        output: 0,
        from_s: 0.0,
        to_s: 60.0,
        first_tick: 0,
        last_tick: 1,
        equals: crate::receipt::ReceiptValue::Boolean(true),
    });
    let rows = [row(1, ASSERT, "b is false")];
    assert_eq!(
        outcome(&rows, &[window, expect(1, ASSERT, None)], 31),
        (vec![false, true], vec![true])
    );
}
