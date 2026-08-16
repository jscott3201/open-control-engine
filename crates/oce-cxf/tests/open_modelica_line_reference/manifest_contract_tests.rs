//! Counterexamples for architecture-specific raw identities and retained native records.

use sha2::{Digest as _, Sha256};
use std::path::PathBuf;

use super::verifier_adversarial_tests::ClaimedTempDir;

fn manifest_value() -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(super::fixture("manifest.json")).unwrap()).unwrap()
}

fn encoded(value: &serde_json::Value) -> Vec<u8> {
    let mut bytes = serde_json::to_vec_pretty(value).unwrap();
    bytes.push(b'\n');
    bytes
}

#[test]
fn architecture_specific_repeat_raw_digests_are_allowed() {
    let mut value = manifest_value();
    let alternate = "1111111111111111111111111111111111111111111111111111111111111111";
    let architecture = &mut value["architectures"][1];
    architecture["raw_run_a_sha256"] = alternate.into();
    architecture["raw_run_b_sha256"] = alternate.into();
    architecture["runs"][0]["raw_sha256"] = alternate.into();
    architecture["runs"][1]["raw_sha256"] = alternate.into();
    super::manifest::parse(&encoded(&value)).unwrap();
}

#[test]
fn same_architecture_repeat_raw_digests_must_match() {
    let mut value = manifest_value();
    value["architectures"][1]["raw_run_b_sha256"] =
        "2222222222222222222222222222222222222222222222222222222222222222".into();
    assert!(
        super::manifest::parse(&encoded(&value))
            .unwrap_err()
            .contains("same-architecture repeat raw digest")
    );
}

#[test]
fn native_record_semantic_mutation_fails_after_artifact_digest_update() {
    assert_native_record_mutation_fails(|value| {
        value["artifact_toolchain"]["python_version"] = "Python 3.13.8".into();
    });
}

#[test]
fn native_record_unknown_field_fails_after_artifact_digest_update() {
    assert_native_record_mutation_fails(|value| {
        value["unknown"] = true.into();
    });
}

#[test]
fn coordinated_noop_keep_first_rehash_fails_structural_reproduction() {
    let (_temporary, root, mut manifest) = copied_repository();
    let base = root.join("crates/oce-conformance/tests/fixtures/open_modelica/reals_line/arm64");
    let output = base.join("projection-keep-first.canonical.csv");
    let bytes = std::fs::read(base.join("line.canonical.csv")).unwrap();
    std::fs::write(&output, &bytes).unwrap();
    let digest = sha(&bytes);
    update_projection_record(&base, "canonical_sha256", &digest, &mut manifest, &root);
    manifest.architectures[0]
        .projection_mutation
        .canonical_sha256 = digest.clone();
    manifest
        .artifacts
        .iter_mut()
        .find(|artifact| artifact.role == "arm64_projection_keep_first_canonical_csv")
        .unwrap()
        .sha256 = digest;
    let error = super::repository::validate(&manifest, &root).unwrap_err();
    assert!(
        error.contains("retained keep-first bytes are not reproducible"),
        "{error}"
    );
}

#[test]
fn coordinated_mismatch_log_rehash_fails_structural_reproduction() {
    let (_temporary, root, mut manifest) = copied_repository();
    let base = root.join("crates/oce-conformance/tests/fixtures/open_modelica/reals_line/arm64");
    let log = base.join("projection-mutation.log");
    let bytes = std::fs::read_to_string(&log)
        .unwrap()
        .replace(
            "mutated_schedule_mismatch_rows=2,4,6,8",
            "mutated_schedule_mismatch_rows=2",
        )
        .into_bytes();
    std::fs::write(&log, &bytes).unwrap();
    let digest = sha(&bytes);
    update_projection_record(&base, "log_sha256", &digest, &mut manifest, &root);
    manifest.architectures[0].projection_mutation.log_sha256 = digest.clone();
    manifest
        .artifacts
        .iter_mut()
        .find(|artifact| artifact.role == "arm64_projection_mutation_log")
        .unwrap()
        .sha256 = digest;
    let error = super::repository::validate(&manifest, &root).unwrap_err();
    assert!(
        error.contains("execution log is not reproducible"),
        "{error}"
    );
}

fn assert_native_record_mutation_fails(mutate: impl FnOnce(&mut serde_json::Value)) {
    let (_temporary, copied_root, mut manifest) = copied_repository();
    let record = copied_root.join(
        "crates/oce-conformance/tests/fixtures/open_modelica/reals_line/arm64/architecture.json",
    );
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&record).unwrap()).unwrap();
    mutate(&mut value);
    let bytes = encoded(&value);
    std::fs::write(&record, &bytes).unwrap();
    let digest = Sha256::digest(&bytes);
    let digest = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    manifest
        .artifacts
        .iter_mut()
        .find(|artifact| artifact.role == "arm64_architecture_record")
        .unwrap()
        .sha256 = digest;
    let error = super::repository::validate(&manifest, &copied_root).unwrap_err();
    assert!(
        error.contains("native architecture record") || error.contains("unknown field"),
        "{error}"
    );
}

fn copied_repository() -> (ClaimedTempDir, PathBuf, super::schema::Manifest) {
    let temporary = ClaimedTempDir::new("oce-line-structural-repository");
    let copied_root = temporary.path().join("repository");
    let source_root = super::repository_root();
    let manifest = super::checked_manifest();
    for artifact in &manifest.artifacts {
        let destination = copied_root.join(&artifact.path);
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::copy(source_root.join(&artifact.path), destination).unwrap();
    }
    (temporary, copied_root, manifest)
}

fn update_projection_record(
    base: &std::path::Path,
    field: &str,
    digest: &str,
    manifest: &mut super::schema::Manifest,
    root: &std::path::Path,
) {
    let record = base.join("architecture.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&record).unwrap()).unwrap();
    value["projection_mutation"][field] = digest.into();
    let bytes = encoded(&value);
    std::fs::write(&record, &bytes).unwrap();
    manifest
        .artifacts
        .iter_mut()
        .find(|artifact| artifact.role == "arm64_architecture_record")
        .unwrap()
        .sha256 = sha(&bytes);
    assert!(record.starts_with(root));
}

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
