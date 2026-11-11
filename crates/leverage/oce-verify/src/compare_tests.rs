//! Which warnings count as matched, over constructed trace rows. The rest of the comparison is
//! driven end to end by the crate's integration tests, which need a real trace.

use oce_api::AssertLevel;
use oce_replay::DiagnosticRow;

use super::{matches, unmatched};
use crate::bind::{BoundDiagnostic, BoundExpectation};

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

fn expect(tick: u64, block: &str, message: Option<&str>) -> BoundExpectation {
    BoundExpectation::Diagnostic(BoundDiagnostic {
        tick,
        block: block.to_owned(),
        message: message.map(str::to_owned),
    })
}

const ASSERT: &str = "CDL.Utilities.Assert";

#[test]
fn a_diagnostic_matches_its_tick_its_block_and_its_message_when_given() {
    let diagnostic = BoundDiagnostic {
        tick: 1,
        block: ASSERT.to_owned(),
        message: Some("b is false".to_owned()),
    };
    assert!(matches(&row(1, ASSERT, "b is false"), &diagnostic));
    assert!(
        !matches(&row(2, ASSERT, "b is false"), &diagnostic),
        "another tick"
    );
    assert!(
        !matches(
            &row(1, "CDL.Reals.MovingAverage", "b is false"),
            &diagnostic
        ),
        "another class"
    );
    assert!(
        !matches(&row(1, ASSERT, "b is False"), &diagnostic),
        "byte for byte"
    );
    assert!(!matches(&row(1, ASSERT, "b is false "), &diagnostic));
    let any_message = BoundDiagnostic {
        message: None,
        ..diagnostic
    };
    assert!(matches(&row(1, ASSERT, "anything at all"), &any_message));
    assert!(matches(&row(1, ASSERT, ""), &any_message));
}

#[test]
fn every_warning_a_diagnostic_expectation_matches_is_matched_and_the_rest_are_listed() {
    let rows = [
        row(0, ASSERT, "b is false"),
        row(1, ASSERT, "b is false"),
        row(1, ASSERT, "b is false"),
        row(1, ASSERT, "another check"),
        row(2, ASSERT, "b is false"),
    ];
    // One expectation at tick 1 naming the message matches both equal warnings of that tick and
    // not the third one there, nor the warnings of other ticks.
    let expectations = [expect(1, ASSERT, Some("b is false"))];
    let (listed, total) = unmatched(&rows, &expectations, 16);
    assert_eq!(total, 3);
    let seen: Vec<(u64, &str)> = listed
        .iter()
        .map(|w| (w.t.to_bits(), w.message.as_str()))
        .collect();
    assert_eq!(
        seen,
        [
            (0.0_f64.to_bits(), "b is false"),
            (60.0_f64.to_bits(), "another check"),
            (120.0_f64.to_bits(), "b is false"),
        ]
    );
    // An expectation that names no message matches every warning of its class at its tick.
    let (listed, total) = unmatched(&rows, &[expect(1, ASSERT, None)], 16);
    assert_eq!((listed.len(), total), (2, 2));
    // With no diagnostic expectation every warning is unmatched.
    let (_, total) = unmatched(&rows, &[], 16);
    assert_eq!(total, 5);
}

#[test]
fn the_list_stops_at_the_cap_and_the_total_does_not() {
    let rows: Vec<DiagnosticRow> = (0..40)
        .map(|frame| row(frame, ASSERT, "b is false"))
        .collect();
    let (listed, total) = unmatched(&rows, &[], 16);
    assert_eq!((listed.len(), total), (16, 40));
    assert_eq!(
        listed[15].t.to_bits(),
        (15.0_f64 * 60.0).to_bits(),
        "the first 16, in row order"
    );
    let (listed, total) = unmatched(&rows, &[], 0);
    assert_eq!((listed.len(), total), (0, 40));
}
