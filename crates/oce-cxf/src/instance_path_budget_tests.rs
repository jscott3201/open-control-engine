//! The budget's own edges, at its real size.

use oce_diag::{DiagCode, EXCERPT_BYTES};

use super::{
    InstancePath, InstancePathBudget, MAX_DOCUMENT_INSTANCE_PATH_BYTES, path_bytes, reserving,
};

#[test]
fn the_whole_budget_fits_and_one_byte_more_does_not() {
    let mut budget = InstancePathBudget::per_document();
    assert!(
        budget
            .take(
                InstancePath::Parameter,
                Some(MAX_DOCUMENT_INSTANCE_PATH_BYTES + 1)
            )
            .is_err()
    );
    budget
        .take(
            InstancePath::Parameter,
            Some(MAX_DOCUMENT_INSTANCE_PATH_BYTES),
        )
        .expect("the whole budget fits");
    budget
        .take(InstancePath::Parameter, Some(0))
        .expect("nothing fits in nothing");
    assert!(budget.take(InstancePath::Parameter, Some(1)).is_err());
}

#[test]
fn a_refusal_reserves_nothing() {
    let mut budget = InstancePathBudget::per_document();
    budget
        .take(
            InstancePath::Parameter,
            Some(MAX_DOCUMENT_INSTANCE_PATH_BYTES - 10),
        )
        .expect("fits");
    assert!(budget.take(InstancePath::PaddedOutputs, Some(11)).is_err());
    assert!(budget.take(InstancePath::BoundaryInput, None).is_err());
    budget
        .take(InstancePath::BoundaryInput, Some(10))
        .expect("the refusals took nothing");
}

#[test]
fn a_path_is_its_owner_a_dot_and_its_suffix_and_never_wraps() {
    assert_eq!(path_bytes(10, 3), Some(14));
    assert_eq!(path_bytes(0, 0), Some(1));
    assert_eq!(path_bytes(usize::MAX, 0), None);
    assert_eq!(path_bytes(usize::MAX - 1, 1), None);
    assert_eq!(path_bytes(usize::MAX - 1, 0), Some(usize::MAX));
    let mut budget = InstancePathBudget::per_document();
    let refusal = budget
        .take_path(InstancePath::Parameter, usize::MAX, 1)
        .expect_err("an overflowing path never fits");
    assert!(
        refusal
            .diagnostic("urn:p", "urn:o")
            .message
            .contains("more than usize::MAX"),
    );
}

#[test]
fn a_refusal_names_its_subject_and_quotes_the_owner_in_an_excerpt() {
    let owner = format!("urn:{}", "o".repeat(64 * 1024));
    let mut budget = InstancePathBudget::per_document();
    budget
        .take(
            InstancePath::Parameter,
            Some(MAX_DOCUMENT_INSTANCE_PATH_BYTES),
        )
        .expect("fits");
    for kind in [
        InstancePath::Parameter,
        InstancePath::PaddedOutputs,
        InstancePath::BoundaryInput,
    ] {
        let diagnostic = budget
            .take_path(kind, owner.len(), 1)
            .expect_err("full")
            .diagnostic("urn:subject", &owner);
        assert_eq!(diagnostic.code, DiagCode::ResourceLimitExceeded);
        assert_eq!(diagnostic.subject.as_deref(), Some("urn:subject"));
        assert!(
            diagnostic.message.len() < 256 + EXCERPT_BYTES,
            "{}",
            diagnostic.message
        );
        assert!(
            diagnostic
                .message
                .contains(&format!("{} byte(s)", owner.len() + 2))
        );
        assert!(
            diagnostic
                .message
                .contains("already take 8388608 of the 8388608")
        );
    }
}

#[test]
fn a_reservation_holds_for_its_reads_alone() {
    reserving(MAX_DOCUMENT_INSTANCE_PATH_BYTES - 5, || {
        let mut budget = InstancePathBudget::per_document();
        assert!(budget.take(InstancePath::Parameter, Some(6)).is_err());
        budget.take(InstancePath::Parameter, Some(5)).expect("fits");
    });
    let mut budget = InstancePathBudget::per_document();
    budget
        .take(
            InstancePath::Parameter,
            Some(MAX_DOCUMENT_INSTANCE_PATH_BYTES),
        )
        .expect("the reservation ended with its reads");
}
