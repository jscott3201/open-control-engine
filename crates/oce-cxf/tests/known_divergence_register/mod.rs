//! Per-PR sentinel for the evidence-only known-divergence register.

#[path = "../../../oce-conformance/tests/known_divergence/date.rs"]
mod date;
#[path = "../../../oce-conformance/tests/known_divergence/reader.rs"]
mod reader;
#[path = "../../../oce-conformance/tests/known_divergence/repository.rs"]
mod repository;

use std::path::Path;

const REGISTER: &[u8] =
    include_bytes!("../../../oce-conformance/tests/fixtures/known_divergence/register.json");
const SYNTHETIC_SCHEMA_TEST_REGISTER: &[u8] = include_bytes!(
    "../../../oce-conformance/tests/fixtures/known_divergence/synthetic_schema_test_register.json"
);
#[test]
fn canonical_register_and_current_evidence_validate() {
    let parsed = reader::read_register(REGISTER).expect("known-divergence register validates");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("oce-cxf is under the repository crates directory");
    repository::validate_repository(&parsed, root)
        .expect("known-divergence evidence paths and digests validate");
}

#[test]
fn synthetic_schema_test_entry_exercises_repository_validation() {
    let parsed = reader::read_register(SYNTHETIC_SCHEMA_TEST_REGISTER)
        .expect("synthetic schema-test register validates");
    assert_eq!(parsed.entries.len(), 1, "synthetic register has one entry");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("oce-cxf is under the repository crates directory");
    repository::validate_repository(&parsed, root)
        .expect("synthetic comparison reference and evidence digest validate");
}
