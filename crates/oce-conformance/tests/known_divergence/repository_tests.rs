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

#[cfg(unix)]
#[test]
fn intermediate_symlink_cannot_escape_repository_root() {
    use std::os::unix::fs::symlink;

    struct Cleanup(std::path::PathBuf);

    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    let base = std::env::temp_dir().join(format!(
        "oce-known-divergence-symlink-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&base);
    let cleanup = Cleanup(base.clone());
    let root = base.join("root");
    let outside = base.join("outside");
    std::fs::create_dir_all(&root).expect("temporary repository root created");
    std::fs::create_dir_all(&outside).expect("outside directory created");
    std::fs::write(root.join("comparison.rs"), b"inside comparison")
        .expect("inside comparison written");
    std::fs::write(outside.join("comparison.rs"), b"outside comparison")
        .expect("outside comparison written");
    std::fs::write(
        outside.join("artifact.txt"),
        include_bytes!("../fixtures/known_divergence/schema_test_artifact.txt"),
    )
    .expect("outside artifact written");
    symlink(&outside, root.join("escape")).expect("intermediate symlink created");
    symlink(
        outside.join("comparison.rs"),
        root.join("final-comparison.rs"),
    )
    .expect("final comparison symlink created");
    symlink(
        outside.join("artifact.txt"),
        root.join("final-artifact.txt"),
    )
    .expect("final artifact symlink created");

    let mut comparison_escape = entry("DVG-000001", "comparison-escape");
    comparison_escape["comparison_reference"] = json!("escape/comparison.rs");
    let parsed = read_register(&bytes(&register(vec![comparison_escape])))
        .expect("comparison escape has valid schema");
    let error = validate_repository(&parsed, &root).expect_err("comparison escape rejected");
    assert_eq!(
        error.code,
        ValidationCode::ComparisonReferenceOutsideRepository
    );

    let mut evidence_escape = entry("DVG-000001", "evidence-escape");
    evidence_escape["comparison_reference"] = json!("comparison.rs");
    for evidence in evidence_escape["evidence"].as_array_mut().unwrap() {
        evidence["path"] = json!("escape/artifact.txt");
    }
    let parsed = read_register(&bytes(&register(vec![evidence_escape])))
        .expect("evidence escape has valid schema");
    let error = validate_repository(&parsed, &root).expect_err("evidence escape rejected");
    assert_eq!(error.code, ValidationCode::EvidenceOutsideRepository);

    let mut final_comparison = entry("DVG-000001", "final-comparison");
    final_comparison["comparison_reference"] = json!("final-comparison.rs");
    let parsed = read_register(&bytes(&register(vec![final_comparison])))
        .expect("final comparison symlink has valid schema");
    let error = validate_repository(&parsed, &root).expect_err("final symlink rejected");
    assert_eq!(error.code, ValidationCode::ComparisonReferenceNotFile);

    let mut final_evidence = entry("DVG-000001", "final-evidence");
    final_evidence["comparison_reference"] = json!("comparison.rs");
    for evidence in final_evidence["evidence"].as_array_mut().unwrap() {
        evidence["path"] = json!("final-artifact.txt");
    }
    let parsed = read_register(&bytes(&register(vec![final_evidence])))
        .expect("final evidence symlink has valid schema");
    let error = validate_repository(&parsed, &root).expect_err("final symlink rejected");
    assert_eq!(error.code, ValidationCode::EvidenceNotFile);

    drop(cleanup);
    assert!(!base.exists(), "temporary test tree removed");
}
