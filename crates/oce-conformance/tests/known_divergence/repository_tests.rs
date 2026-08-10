//! Repository path and SHA-256 integrity tests.

use std::path::Path;

use serde_json::json;
use sha2::{Digest as _, Sha256};

use super::reader::{ValidationCode, read_register};
use super::repository::{validate_digest, validate_repository};
use super::test_data::{ARTIFACT_PATH, bytes, entry, register};

fn repository_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("oce-conformance is under the repository crates directory")
}

#[test]
fn checked_in_comparison_and_schema_artifact_are_positive_controls() {
    let parsed = read_register(&bytes(&register(vec![entry("DVG-000001", "base")])))
        .expect("schema test record validates");
    validate_repository(&parsed, repository_root()).expect("checked-in artifact digest matches");
}

#[test]
fn comparison_reference_requires_a_checked_in_regular_file() {
    for (path, expected) in [
        (
            "crates/oce-conformance/src/missing-comparison.rs",
            ValidationCode::ComparisonReferenceMissing,
        ),
        (
            "crates/oce-conformance/src",
            ValidationCode::ComparisonReferenceNotFile,
        ),
    ] {
        let mut value = entry("DVG-000001", "base");
        value["comparison_reference"] = json!(path);
        let parsed = read_register(&bytes(&register(vec![value]))).expect("path shape validates");
        let error = validate_repository(&parsed, repository_root())
            .expect_err("comparison reference rejected");
        assert_eq!(error.code, expected);
        assert_eq!(error.entry, Some(0));
    }
}

#[test]
fn missing_and_directory_evidence_paths_are_distinct_errors() {
    for (path, expected) in [
        (
            "crates/oce-conformance/tests/fixtures/known_divergence/missing.txt",
            ValidationCode::EvidenceMissing,
        ),
        ("crates/oce-conformance", ValidationCode::EvidenceNotFile),
    ] {
        let mut value = entry("DVG-000001", "base");
        value["evidence"][0]["path"] = json!(path);
        let parsed = read_register(&bytes(&register(vec![value]))).expect("path shape validates");
        let error = validate_repository(&parsed, repository_root()).expect_err("path rejected");
        assert_eq!(error.code, expected);
        assert_eq!(error.entry, Some(0));
    }
}

#[test]
fn changed_artifact_bytes_are_predicted_red_without_rewriting_evidence() {
    let parsed = read_register(&bytes(&register(vec![entry("DVG-000001", "base")])))
        .expect("schema test record validates");
    let evidence = &parsed.entries[0].evidence[0];
    let original = std::fs::read(repository_root().join(ARTIFACT_PATH)).expect("artifact reads");
    validate_digest(evidence, &Sha256::digest(&original), 0).expect("unmutated bytes match");
    let mut mutation = original.clone();
    mutation[0] ^= 1;
    assert_ne!(mutation, original, "evidence-byte mutation must apply");
    let error = validate_digest(evidence, &Sha256::digest(&mutation), 0)
        .expect_err("changed bytes must not match the recorded digest");
    assert_eq!(error.code, ValidationCode::EvidenceDigestMismatch);
    validate_digest(evidence, &Sha256::digest(&original), 0).expect("mutation restored");
}

#[test]
fn valid_but_wrong_digest_is_rejected_by_repository_recomputation() {
    let mut value = entry("DVG-000001", "base");
    value["evidence"][0]["sha256"] = json!("0".repeat(64));
    let parsed = read_register(&bytes(&register(vec![value]))).expect("digest shape validates");
    let error = validate_repository(&parsed, repository_root()).expect_err("wrong digest rejected");
    assert_eq!(error.code, ValidationCode::EvidenceDigestMismatch);
}
