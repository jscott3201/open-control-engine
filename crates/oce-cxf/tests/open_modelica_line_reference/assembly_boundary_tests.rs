//! Process-level counterexamples for the strict assembly canonicalization boundary.

use super::verifier_adversarial_tests::{ClaimedTempDir, copy_tree};

fn assert_strict_boundary_rejects(label: &str, mutate: impl FnOnce(&mut Vec<u8>), expected: &str) {
    let temporary = ClaimedTempDir::new(&format!("oce-line-assembly-{label}"));
    let copied = temporary.path().join("arm64");
    copy_tree(&super::fixture("arm64"), &copied);
    let raw = copied.join("line-run-a.raw.csv");
    let mut bytes = std::fs::read(&raw).unwrap();
    mutate(&mut bytes);
    std::fs::write(raw, bytes).unwrap();
    let root = super::repository_root();
    let output = std::process::Command::new("cargo")
        .args(["run", "--manifest-path"])
        .arg(root.join("tools/openmodelica-line-reference/Cargo.toml"))
        .args([
            "--offline",
            "--locked",
            "--quiet",
            "--",
            "verify-architecture-canonical",
        ])
        .arg(&copied)
        .current_dir(root)
        .env("CARGO_NET_OFFLINE", "true")
        .output()
        .unwrap();
    assert!(!output.status.success(), "{label}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(expected),
        "{label}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn numeric_cell_over_128_bytes_is_refused_before_assembly() {
    assert_strict_boundary_rejects(
        "long-cell",
        |bytes| {
            let body = String::from_utf8(bytes.clone()).unwrap();
            *bytes = body
                .replacen("0,1.25,", &format!("0,{},", "1".repeat(129)), 1)
                .into_bytes();
        },
        "CellType",
    );
}

#[test]
fn crlf_raw_records_are_refused_before_assembly() {
    assert_strict_boundary_rejects(
        "crlf",
        |bytes| {
            *bytes = String::from_utf8(bytes.clone())
                .unwrap()
                .replace('\n', "\r\n")
                .into_bytes();
        },
        "CsvSyntax",
    );
}

#[test]
fn missing_final_lf_is_refused_before_assembly() {
    assert_strict_boundary_rejects(
        "missing-final-lf",
        |bytes| {
            assert_eq!(bytes.pop(), Some(b'\n'));
        },
        "CsvSyntax",
    );
}
