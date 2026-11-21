//! The code list against the kinds, and the bound on every sentence.

use super::*;

/// The position of `kind` in the declaration. The match is exhaustive, so a new variant does
/// not compile until it has a row here, in `ALL` and in `SPEC_ERROR_CODES`.
fn declared_index(kind: SpecErrorKind) -> usize {
    use SpecErrorKind as K;
    match kind {
        K::NotUtf8 => 0,
        K::ByteOrderMark => 1,
        K::JsonSyntax => 2,
        K::InputTooLarge => 3,
        K::NestingTooDeep => 4,
        K::ListTooLong => 5,
        K::StringTooLong => 6,
        K::DuplicateKey => 7,
        K::UnknownKey => 8,
        K::MissingKey => 9,
        K::WrongType => 10,
        K::NonFiniteNumber => 11,
        K::IntegerOutOfRange => 12,
        K::ControlCharacter => 13,
        K::UnterminatedComment => 14,
        K::UnknownSchema => 15,
        K::TooManyParameters => 16,
        K::TooManyKeys => 17,
        K::EmptyName => 18,
        K::UnknownValue => 19,
        K::PathInvalid => 20,
        K::InvalidTimeOrigin => 21,
        K::InvalidTolerance => 22,
        K::InvalidSampling => 23,
        K::InvalidWindow => 24,
        K::InvalidHold => 25,
        K::DuplicatePoint => 26,
        K::UnmatchedKey => 27,
        K::AmbiguousOutput => 28,
        K::ConflictingOverride => 29,
        K::WorkLimitExceeded => 30,
    }
}

#[test]
fn the_code_list_is_every_kind_in_declaration_order() {
    assert_eq!(SpecErrorKind::ALL.len(), 31);
    for (index, kind) in SpecErrorKind::ALL.iter().enumerate() {
        assert_eq!(declared_index(*kind), index, "{kind:?}");
    }
    let codes: Vec<&str> = SpecErrorKind::ALL.iter().map(|kind| kind.code()).collect();
    assert_eq!(codes, SPEC_ERROR_CODES);
}

#[test]
fn the_strict_kinds_are_the_cursors_nine_refusals() {
    use crate::error::VectorErrorKind;
    // Each constant names the kind of the same code as the vector reader's constant does, so
    // the shared cursor reports one word per defect to both readers.
    let pairs = [
        (SpecErrorKind::JSON_SYNTAX, VectorErrorKind::JSON_SYNTAX),
        (
            SpecErrorKind::NESTING_TOO_DEEP,
            VectorErrorKind::NESTING_TOO_DEEP,
        ),
        (SpecErrorKind::LIST_TOO_LONG, VectorErrorKind::LIST_TOO_LONG),
        (
            SpecErrorKind::STRING_TOO_LONG,
            VectorErrorKind::STRING_TOO_LONG,
        ),
        (SpecErrorKind::DUPLICATE_KEY, VectorErrorKind::DUPLICATE_KEY),
        (SpecErrorKind::WRONG_TYPE, VectorErrorKind::WRONG_TYPE),
        (
            SpecErrorKind::NON_FINITE_NUMBER,
            VectorErrorKind::NON_FINITE_NUMBER,
        ),
        (
            SpecErrorKind::INTEGER_OUT_OF_RANGE,
            VectorErrorKind::INTEGER_OUT_OF_RANGE,
        ),
        (
            SpecErrorKind::CONTROL_CHARACTER,
            VectorErrorKind::CONTROL_CHARACTER,
        ),
    ];
    let expected = [
        "json-syntax",
        "nesting-too-deep",
        "list-too-long",
        "string-too-long",
        "duplicate-key",
        "wrong-type",
        "non-finite-number",
        "integer-out-of-range",
        "control-character",
    ];
    for ((spec, vector), code) in pairs.into_iter().zip(expected) {
        assert_eq!(spec.code(), code);
        assert_eq!(vector.code(), code);
    }
}

#[test]
fn a_text_within_the_bound_is_written_whole_and_a_longer_one_is_cut_and_marked() {
    // At the bound, to the byte: whole.
    assert_eq!(printable(&"a".repeat(10), 10), "a".repeat(10));
    assert_eq!(printable("", 10), "");
    // One past it: cut to leave room for the mark, and never longer than the bound.
    assert_eq!(
        printable(&"a".repeat(11), 10),
        format!("{}...", "a".repeat(7))
    );
    assert_eq!(
        printable(&"a".repeat(1_000), 10),
        format!("{}...", "a".repeat(7))
    );
    for length in 0..40 {
        let text = "ab".repeat(length);
        let written = printable(&text, 16);
        assert!(written.len() <= 16, "{written}");
        if text.len() <= 16 {
            assert_eq!(written, text);
        } else {
            assert!(written.ends_with("...") && text.starts_with(&written[..written.len() - 3]));
            assert_eq!(
                written.len(),
                16,
                "a cut text of one-byte characters fills the bound"
            );
        }
    }
}

#[test]
fn a_character_outside_printable_ascii_is_spelled_and_never_cut_in_half() {
    assert_eq!(printable("a\nb", 100), "a\\nb");
    assert_eq!(printable("caf\u{e9}", 100), "caf\\u{e9}");
    assert_eq!(printable("\u{1b}[31m", 100), "\\u{1b}[31m");
    assert_eq!(
        printable("\u{0}\u{7f}\u{9f}\t\r", 100),
        "\\u{0}\\u{7f}\\u{9f}\\u{9}\\u{d}"
    );
    assert_eq!(printable("\u{202e}\u{1f321}", 100), "\\u{202e}\\u{1f321}");
    assert_eq!(
        printable(" ~`\\\"", 100),
        " ~`\\\"",
        "printable ASCII is itself"
    );
    // A spelling of several bytes is kept whole or left out, whatever the bound.
    let text = "\u{1f321}".repeat(20);
    for limit in 3..120 {
        let written = printable(&text, limit);
        assert!(written.len() <= limit, "{limit}: {written}");
        let body = written.strip_suffix("...").unwrap_or(&written);
        assert_eq!(body.len() % "\\u{1f321}".len(), 0, "{limit}: {written}");
        assert!(written.is_ascii());
    }
    // Exactly at the bound with a wide last character: whole.
    assert_eq!(printable("ab\u{e9}", 8), "ab\\u{e9}");
    assert_eq!(printable("ab\u{e9}", 7), "ab...");
}

#[test]
fn a_quoted_name_is_bounded_on_its_own() {
    assert_eq!(shown("TSup*"), "`TSup*`");
    assert_eq!(shown(""), "``");
    let long = shown(&"k".repeat(4_096));
    assert_eq!(long, format!("`{}...`", "k".repeat(93)));
    assert_eq!(long.len(), 98);
    assert_eq!(shown(&"k".repeat(96)), format!("`{}`", "k".repeat(96)));
    assert_eq!(shown("a\nb"), "`a\\nb`");
}

#[test]
fn a_text_refusal_carries_its_place_and_a_whole_document_refusal_none() {
    let text = SpecError::in_text(
        SpecErrorKind::UnknownKey,
        Location::new(3, 14),
        "unknown key `data`",
    );
    assert_eq!(text.at(), Some(Location::new(3, 14)));
    assert_eq!(text.subject(), "line 3 column 14");
    assert_eq!(text.to_string(), "line 3 column 14: unknown key `data`");
    assert_eq!(
        (text.code(), text.kind()),
        ("unknown-key", SpecErrorKind::UnknownKey)
    );
    let whole = SpecError::on(SpecErrorKind::NotUtf8, "document", "byte 3 is not UTF-8");
    assert_eq!((whole.at(), whole.subject()), (None, "document"));
    assert_eq!(whole.detail(), "byte 3 is not UTF-8");
    // Both constructors bound and spell what they are given.
    let hostile = "x\u{1b}".repeat(1_000);
    for error in [
        SpecError::in_text(SpecErrorKind::JsonSyntax, Location::new(1, 1), &hostile),
        SpecError::on(SpecErrorKind::UnmatchedKey, &hostile, &hostile),
    ] {
        for written in [error.subject(), error.detail()] {
            assert!(
                written.len() <= MAX_SPEC_DETAIL_BYTES && written.is_ascii(),
                "{written}"
            );
            assert!(!written.contains('\u{1b}'));
        }
        assert!(error.detail().ends_with("..."));
    }
    // A refusal of the cursor becomes an error at the same place with the same sentence.
    let from: SpecError = Refusal::new(
        SpecErrorKind::WrongType,
        Location::new(2, 5),
        "expected a list",
    )
    .into();
    assert_eq!(
        (from.code(), from.at(), from.detail()),
        ("wrong-type", Some(Location::new(2, 5)), "expected a list")
    );
}
