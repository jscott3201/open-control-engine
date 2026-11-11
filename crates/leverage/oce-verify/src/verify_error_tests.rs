use std::collections::BTreeSet;

use super::*;

/// The position of `kind` in the declaration. The match is exhaustive, so a new variant does not
/// compile until it has a row here, in `ALL` and in `VERIFY_ERROR_CODES`.
fn declared_index(kind: VerifyErrorKind) -> usize {
    use VerifyErrorKind as K;
    match kind {
        K::SubjectLoadFailed => 0,
        K::UnknownInput => 1,
        K::AmbiguousInput => 2,
        K::DuplicateInput => 3,
        K::InputNotSupplied => 4,
        K::UnknownOutput => 5,
        K::AmbiguousOutput => 6,
        K::ParamPathUnknown => 7,
        K::ParamPathAmbiguous => 8,
        K::ParamPathDuplicate => 9,
        K::ParamRefused => 10,
        K::ValueTypeMismatch => 11,
        K::ValueNotRepresentable => 12,
        K::EnumPortUnsupported => 13,
        K::UnsupportedPortType => 14,
        K::TooManyFrames => 15,
        K::MutantMissing => 16,
        K::MutantUnlisted => 17,
        K::DuplicateMutant => 18,
        K::TooManyMutantBytes => 19,
        K::MutantLoadFailed => 20,
        K::MutantBoundaryDiffers => 21,
        K::MutantIdentical => 22,
        K::MutantParamRefused => 23,
        K::RunRefused => 24,
        K::ReplayFailed => 25,
        K::WindowUncovered => 26,
    }
}

#[test]
fn the_code_list_is_every_kind_in_declaration_order() {
    assert_eq!(VerifyErrorKind::ALL.len(), 27);
    for (index, kind) in VerifyErrorKind::ALL.iter().enumerate() {
        assert_eq!(declared_index(*kind), index, "{kind:?}");
    }
    let codes: Vec<&str> = VerifyErrorKind::ALL.iter().map(|k| k.code()).collect();
    assert_eq!(codes, VERIFY_ERROR_CODES);
}

#[test]
fn every_table_is_unique_lowercase_kebab_case() {
    for table in [
        VERIFY_ERROR_CODES,
        REPLAY_FAILURE_CAUSES,
        PARAM_REFUSAL_CLASSES,
        MUTANT_PARAM_REFUSAL_CAUSES,
        crate::ENGINE_REFUSAL_CLASSES,
    ] {
        let mut seen = BTreeSet::new();
        for code in table {
            assert!(seen.insert(*code), "{code} twice");
            assert!(
                code.bytes()
                    .all(|b| b.is_ascii_lowercase() || b == b'-' || b.is_ascii_digit()),
                "{code}"
            );
            assert!(!code.starts_with('-') && !code.ends_with('-'), "{code}");
        }
    }
}

#[test]
fn the_replay_causes_and_parameter_classes_are_the_notes_tables() {
    assert_eq!(
        REPLAY_FAILURE_CAUSES,
        [
            "trace-too-large",
            "output-set-changed",
            "grid-not-increasing",
            "engine-setup",
            "source"
        ]
    );
    assert_eq!(
        PARAM_REFUSAL_CLASSES,
        ["param-range", "param-type", "param-structural"]
    );
}

#[test]
fn a_code_shared_with_the_load_list_names_the_same_refusal() {
    // A window with no executed tick, and one mutant path twice, are each one refusal whether
    // load or the run finds it, so each has one word in both lists; they are the only two.
    assert_eq!(VerifyErrorKind::WindowUncovered.code(), "window-uncovered");
    assert_eq!(VerifyErrorKind::DuplicateMutant.code(), "duplicate-mutant");
    let shared: Vec<&str> = VERIFY_ERROR_CODES
        .iter()
        .copied()
        .filter(|code| crate::VECTOR_ERROR_CODES.contains(code))
        .collect();
    assert_eq!(shared, ["duplicate-mutant", "window-uncovered"]);
}

#[test]
fn a_refusal_carries_only_the_details_it_was_given() {
    let plain = VerifyError::new(
        VerifyErrorKind::UnknownInput,
        "scenarios[0].inputs.u",
        "none",
    );
    assert_eq!(plain.code(), "unknown-input");
    assert_eq!(plain.kind(), VerifyErrorKind::UnknownInput);
    assert_eq!(plain.subject(), "scenarios[0].inputs.u");
    assert_eq!(plain.detail(), "none");
    assert_eq!(plain.to_string(), "scenarios[0].inputs.u: none");
    assert_eq!(plain.path(), None);
    assert!(plain.candidates().is_empty());
    assert_eq!(plain.cause(), None);
    assert!(plain.diagnostic_codes().is_empty());
    let detailed = VerifyError::new(VerifyErrorKind::ReplayFailed, "scenarios[1]", "stopped")
        .with_path("urn:x#top.u")
        .with_candidates(vec!["a".to_owned(), "b".to_owned()])
        .with_cause("source")
        .with_diagnostic_codes(vec!["malformed-document"]);
    assert_eq!(detailed.path(), Some("urn:x#top.u"));
    assert_eq!(detailed.candidates(), ["a", "b"]);
    assert_eq!(detailed.cause(), Some("source"));
    assert_eq!(detailed.diagnostic_codes(), ["malformed-document"]);
}
