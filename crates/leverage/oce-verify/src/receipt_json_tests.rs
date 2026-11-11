use oce_replay::EngineRefusal;

use super::*;
use crate::receipt::{Failure, ReceiptValue};

fn spelled(value: ReceiptValue) -> String {
    super::value(value).to_document().trim_end().to_owned()
}

#[test]
fn a_finite_real_other_than_negative_zero_is_a_number_in_the_canonical_spelling() {
    for (x, expected) in [
        (0.0, "0.0"),
        (900.0, "900.0"),
        (0.1, "0.1"),
        (-2.5, "-2.5"),
        (1e16, "1e16"),
        (9_999_999_999_999_998.0, "9999999999999998.0"),
        (0.0001, "0.0001"),
        (9.999_999_999_999_999e-5, "9.999999999999999e-5"),
        (5e-324, "5e-324"),
        (f64::MAX, "1.7976931348623157e308"),
        (-f64::MAX, "-1.7976931348623157e308"),
    ] {
        assert_eq!(spelled(ReceiptValue::Real(x)), expected, "{x:e}");
    }
}

#[test]
fn negative_zero_nan_and_the_infinities_are_the_bits_as_a_string() {
    for (x, expected) in [
        (-0.0, "\"0x8000000000000000\""),
        (f64::NAN, "\"0x7ff8000000000000\""),
        (f64::INFINITY, "\"0x7ff0000000000000\""),
        (f64::NEG_INFINITY, "\"0xfff0000000000000\""),
        // A NaN keeps its sign and payload: the bits are what is written.
        (
            f64::from_bits(0xfff8_0000_0000_0001),
            "\"0xfff8000000000001\"",
        ),
    ] {
        assert_eq!(
            spelled(ReceiptValue::Real(x)),
            expected,
            "{:#x}",
            x.to_bits()
        );
    }
}

#[test]
fn integers_and_booleans_are_json_integers_and_literals() {
    assert_eq!(spelled(ReceiptValue::Integer(0)), "0");
    assert_eq!(
        spelled(ReceiptValue::Integer(i64::MIN)),
        "-9223372036854775808"
    );
    assert_eq!(
        spelled(ReceiptValue::Integer(i64::MAX)),
        "9223372036854775807"
    );
    assert_eq!(spelled(ReceiptValue::Boolean(true)), "true");
    assert_eq!(spelled(ReceiptValue::Boolean(false)), "false");
}

/// The keys of a failure record, in canonical (sorted) order.
fn keys(failure: &Failure) -> Vec<String> {
    match super::failure(failure) {
        Json::Object(entries) => entries.into_iter().map(|(key, _)| key).collect(),
        other => panic!("not an object: {other:?}"),
    }
}

#[test]
fn a_failure_record_is_shaped_by_its_code() {
    let mismatch = Failure::ValueMismatch {
        expectation: 0,
        output: "urn:x#top.y".to_owned(),
        t: 900.0,
        expected: ReceiptValue::Boolean(false),
        actual: ReceiptValue::Boolean(true),
    };
    assert_eq!(
        keys(&mismatch),
        ["actual", "code", "expectation", "expected", "output", "t"]
    );
    let with_message = Failure::DiagnosticMissing {
        expectation: 2,
        t: 60.0,
        block: "CDL.Utilities.Assert".to_owned(),
        message: Some("b is false".to_owned()),
    };
    assert_eq!(
        keys(&with_message),
        ["block", "code", "expectation", "message", "t"]
    );
    let without_message = Failure::DiagnosticMissing {
        expectation: 2,
        t: 60.0,
        block: "CDL.Utilities.Assert".to_owned(),
        message: None,
    };
    assert_eq!(
        keys(&without_message),
        ["block", "code", "expectation", "t"]
    );
    // A refused frame reached no expectation, so its record has no `expectation`.
    let refused = Failure::FrameRefused {
        t: 120.0,
        refusal: EngineRefusal::InputDomain,
    };
    assert_eq!(keys(&refused), ["code", "refusal", "t"]);
    assert_eq!(
        super::failure(&refused).to_document(),
        "{\n  \"code\": \"frame-refused\",\n  \"refusal\": \"input-domain\",\n  \"t\": 120.0\n}\n"
    );
}

#[test]
fn a_mismatch_record_spells_a_negative_zero_and_a_nan_by_their_bits() {
    let record = super::failure(&Failure::ValueMismatch {
        expectation: 1,
        output: "urn:x#top.ratio".to_owned(),
        t: 0.0,
        expected: ReceiptValue::Real(0.0),
        actual: ReceiptValue::Real(f64::NAN),
    })
    .to_document();
    assert!(
        record.contains("\"actual\": \"0x7ff8000000000000\""),
        "{record}"
    );
    assert!(record.contains("\"expected\": 0.0"), "{record}");
    assert!(record.contains("\"t\": 0.0"), "{record}");
    let record = super::failure(&Failure::ValueMismatch {
        expectation: 1,
        output: "urn:x#top.zone.y".to_owned(),
        t: 60.0,
        expected: ReceiptValue::Real(0.0),
        actual: ReceiptValue::Real(-0.0),
    })
    .to_document();
    assert!(
        record.contains("\"actual\": \"0x8000000000000000\""),
        "{record}"
    );
}
