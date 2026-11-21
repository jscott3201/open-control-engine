//! The code list against the kinds, and what a refusal carries.

use super::*;

/// Every kind, in declaration order. The match in `declared_index` is exhaustive, so a new kind
/// fails to compile there until it has a row; this list then fails the test until it has one.
const ALL: [SourceErrorKind; 38] = {
    use SourceErrorKind as K;
    [
        K::ReferenceOutOfRange,
        K::TooManySources,
        K::DuplicateFile,
        K::FileUnlisted,
        K::FileMissing,
        K::MappingRequired,
        K::MappingRefused,
        K::ColumnReferenceInvalid,
        K::UnmappedPoint,
        K::PointReadTwice,
        K::TypePairRefused,
        K::UnitRefused,
        K::UnitOnDiscretePoint,
        K::FormatTagMissing,
        K::TableSyntax,
        K::TableRequired,
        K::TableNotFound,
        K::DuplicateTable,
        K::SizeMismatch,
        K::BadNumber,
        K::NonFiniteNumber,
        K::BadCell,
        K::CellRefused,
        K::ColumnMissing,
        K::DuplicateColumn,
        K::NoHeader,
        K::NoPointColumns,
        K::RaggedRow,
        K::NotUtf8,
        K::CellTooLong,
        K::TooManyRows,
        K::TooManyColumns,
        K::TooManyBytes,
        K::TimeMissing,
        K::BadTime,
        K::TimeOriginRequired,
        K::TimeRegression,
        K::Unreadable,
    ]
};

#[test]
fn the_code_list_is_every_kind_in_declaration_order() {
    assert_eq!(SOURCE_ERROR_CODES.len(), ALL.len());
    for (index, kind) in ALL.iter().enumerate() {
        assert_eq!(kind.declared_index(), index, "{kind:?}");
        assert_eq!(kind.code(), SOURCE_ERROR_CODES[index]);
    }
}

#[test]
fn every_code_is_kebab_case_and_unique() {
    let mut seen = std::collections::BTreeSet::new();
    for code in SOURCE_ERROR_CODES {
        assert!(seen.insert(*code), "{code} twice");
        assert!(!code.is_empty() && !code.starts_with('-') && !code.ends_with('-'));
        assert!(
            code.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
            "{code}"
        );
        assert!(!code.contains("--"), "{code}");
    }
}

#[test]
fn a_refusal_carries_its_place_and_cause_and_shows_them() {
    let error = SourceError::new(SourceErrorKind::CellRefused, "0.5 is not a Boolean")
        .in_file("data/fan.csv")
        .at_row(3)
        .at_column("Enable")
        .caused_by("not-boolean");
    assert_eq!(error.code(), "cell-refused");
    assert_eq!(error.file(), Some("data/fan.csv"));
    assert_eq!((error.line(), error.row()), (None, Some(3)));
    assert_eq!(error.column(), Some("Enable"));
    assert_eq!(error.cause(), Some("not-boolean"));
    assert_eq!(
        error.to_string(),
        "`data/fan.csv` row 3 column `Enable`: 0.5 is not a Boolean"
    );
    let bare = SourceError::new(SourceErrorKind::TooManySources, "too many");
    assert_eq!(bare.to_string(), "too many");
    let line = SourceError::new(SourceErrorKind::TableSyntax, "x")
        .in_file("a.mos")
        .at_line(7);
    assert_eq!(line.to_string(), "`a.mos` line 7: x");
}

#[test]
fn every_text_a_refusal_carries_is_bounded_and_printable() {
    let hostile = format!("\u{1b}[31m{}\u{0}\n", "\u{e9}".repeat(100_000));
    let error = SourceError::new(SourceErrorKind::BadCell, &hostile)
        .in_file(&hostile)
        .at_column(&hostile);
    for text in [
        error.detail(),
        error.file().unwrap(),
        error.column().unwrap(),
    ] {
        assert!(text.len() <= MAX_SPEC_DETAIL_BYTES, "{}", text.len());
        assert!(text.bytes().all(|b| (b' '..=b'~').contains(&b)), "{text}");
    }
    // The error stays two words wide, so a `Result` carrying one is small.
    assert!(std::mem::size_of::<SourceError>() <= 16);
}
