//! The code table against the variants themselves. These tests sit inside the crate because only
//! here may a match over the `#[non_exhaustive]` [`WireError`] omit the wildcard arm: a variant
//! added without a row in [`declaration_index`] does not compile, and one added there without a
//! sample, a code and a row in the pinned list fails.

use std::collections::BTreeSet;

use super::WIRE_CODES;
use crate::location::Location;
use crate::wire::WireError;

/// The position every sample that has one carries.
const AT: Location = Location { line: 3, column: 7 };

/// The variant's position in the declaration of [`WireError`], matched without a wildcard arm.
fn declaration_index(error: &WireError) -> usize {
    match error {
        WireError::NotUtf8 { .. } => 0,
        WireError::ByteOrderMark => 1,
        WireError::TooLarge { .. } => 2,
        WireError::TooDeep { .. } => 3,
        WireError::ListTooLong { .. } => 4,
        WireError::StringTooLong { .. } => 5,
        WireError::DuplicateKey { .. } => 6,
        WireError::NonFiniteNumber { .. } => 7,
        WireError::IntegerOutOfRange { .. } => 8,
        WireError::ControlCharacter { .. } => 9,
        WireError::UnknownKey { .. } => 10,
        WireError::MissingKey { .. } => 11,
        WireError::WrongType { .. } => 12,
        WireError::Syntax { .. } => 13,
        WireError::BoundListTooLong { .. } => 14,
        WireError::MalformedConnection { .. } => 15,
    }
}

/// One value of every variant, in declaration order, with the location it reports.
fn every_variant() -> Vec<(WireError, Option<Location>)> {
    let at = AT;
    vec![
        (WireError::NotUtf8 { offset: 4 }, None),
        (WireError::ByteOrderMark, None),
        (WireError::TooLarge { bytes: 9, limit: 8 }, None),
        (WireError::TooDeep { at, limit: 8 }, Some(at)),
        (WireError::ListTooLong { at, limit: 8 }, Some(at)),
        (WireError::StringTooLong { at, limit: 8 }, Some(at)),
        (
            WireError::DuplicateKey {
                key: "k".into(),
                at,
            },
            Some(at),
        ),
        (WireError::NonFiniteNumber { at }, Some(at)),
        (WireError::IntegerOutOfRange { at }, Some(at)),
        (WireError::ControlCharacter { at }, Some(at)),
        (
            WireError::UnknownKey {
                key: "k".into(),
                at,
            },
            Some(at),
        ),
        (
            WireError::MissingKey {
                key: "k".into(),
                at,
            },
            Some(at),
        ),
        (
            WireError::WrongType {
                detail: "d".into(),
                path: "inputs[0].semantic".into(),
                at,
            },
            Some(at),
        ),
        (
            WireError::Syntax {
                detail: "d".into(),
                at,
            },
            Some(at),
        ),
        (WireError::BoundListTooLong { at }, Some(at)),
        (
            WireError::MalformedConnection {
                index: 0,
                text: "t".into(),
            },
            None,
        ),
    ]
}

#[test]
fn the_samples_hold_every_variant_once_in_declaration_order() {
    let indexes: Vec<usize> = every_variant()
        .iter()
        .map(|(error, _)| declaration_index(error))
        .collect();
    let expected: Vec<usize> = (0..WIRE_CODES.len()).collect();
    assert_eq!(indexes, expected);
}

#[test]
fn every_variant_has_its_pinned_code_in_declaration_order() {
    // A published contract, like `PARSE_CODES` and `IMPORT_ERROR_CODES`: a code added, removed,
    // renamed or reordered fails here.
    let pinned = [
        "not-utf8",
        "byte-order-mark",
        "input-too-large",
        "nesting-too-deep",
        "list-too-long",
        "string-too-long",
        "duplicate-key",
        "non-finite-number",
        "integer-out-of-range",
        "control-character",
        "unknown-key",
        "missing-key",
        "wrong-type",
        "json-syntax",
        "bound-list-too-long",
        "malformed-connection",
    ];
    let codes: Vec<&str> = every_variant()
        .iter()
        .map(|(error, _)| error.code())
        .collect();
    assert_eq!(codes, pinned);
    assert_eq!(WIRE_CODES, pinned);
    for (error, _) in every_variant() {
        assert_eq!(error.code(), WIRE_CODES[declaration_index(&error)]);
    }
}

#[test]
fn every_code_is_kebab_case_and_unique() {
    let mut seen = BTreeSet::new();
    for code in WIRE_CODES {
        assert!(
            code.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                && !code.is_empty()
                && !code.starts_with('-')
                && !code.ends_with('-')
                && !code.contains("--"),
            "{code}"
        );
        assert!(seen.insert(*code), "{code} twice");
    }
    assert_eq!(seen.len(), 16);
}

#[test]
fn the_code_does_not_depend_on_what_the_variant_carries() {
    let far = Location::new(usize::MAX, usize::MAX);
    let pairs = [
        (
            WireError::NotUtf8 { offset: 0 },
            WireError::NotUtf8 { offset: usize::MAX },
        ),
        (
            WireError::TooLarge { bytes: 0, limit: 0 },
            WireError::TooLarge {
                bytes: usize::MAX,
                limit: usize::MAX,
            },
        ),
        (
            WireError::UnknownKey {
                key: String::new(),
                at: AT,
            },
            WireError::UnknownKey {
                key: "wrong-type".into(),
                at: far,
            },
        ),
        (
            WireError::WrongType {
                detail: String::new(),
                path: String::new(),
                at: AT,
            },
            WireError::WrongType {
                detail: "unknown field `x`".into(),
                path: "document".into(),
                at: far,
            },
        ),
        (
            WireError::MalformedConnection {
                index: 0,
                text: String::new(),
            },
            WireError::MalformedConnection {
                index: usize::MAX,
                text: "json-syntax".into(),
            },
        ),
    ];
    for (small, large) in pairs {
        assert_eq!(small.code(), large.code());
    }
}

#[test]
fn the_location_is_the_position_the_variant_carries_or_none() {
    for (error, location) in every_variant() {
        assert_eq!(error.location(), location, "{}", error.code());
    }
    // A saturated position is returned as it is.
    let far = Location::new(usize::MAX, usize::MAX);
    assert_eq!(
        WireError::NonFiniteNumber { at: far }.location(),
        Some(Location {
            line: u32::MAX,
            column: u32::MAX
        })
    );
    let without: Vec<&str> = every_variant()
        .iter()
        .filter(|(error, _)| error.location().is_none())
        .map(|(error, _)| error.code())
        .collect();
    assert_eq!(
        without,
        [
            "not-utf8",
            "byte-order-mark",
            "input-too-large",
            "malformed-connection"
        ]
    );
}

#[test]
fn the_codes_shared_with_another_list_are_exactly_the_documented_ones() {
    // The lists are separate contracts, and the README says which spellings they share. A code
    // added to one list that collides with another moves this.
    let shared = |other: &[&str]| -> Vec<&str> {
        WIRE_CODES
            .iter()
            .copied()
            .filter(|code| other.contains(code))
            .collect()
    };
    assert_eq!(
        shared(crate::PARSE_CODES),
        [
            crate::NOT_UTF8,
            crate::BYTE_ORDER_MARK,
            crate::INPUT_TOO_LARGE,
            crate::NESTING_TOO_DEEP,
            crate::STRING_TOO_LONG,
            crate::NON_FINITE_NUMBER,
            crate::INTEGER_OUT_OF_RANGE,
            crate::CONTROL_CHARACTER,
        ]
    );
    assert_eq!(
        shared(crate::STAGE_ONE_CODES),
        [crate::MALFORMED_CONNECTION]
    );
    assert_eq!(
        shared(crate::IMPORT_ERROR_CODES),
        ["unknown-key", "missing-key", "malformed-connection"]
    );
}
