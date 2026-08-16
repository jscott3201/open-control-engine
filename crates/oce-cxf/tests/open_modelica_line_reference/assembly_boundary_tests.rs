//! Process-level counterexamples for the strict assembly canonicalization boundary.

use super::verifier_adversarial_tests::{ClaimedTempDir, copy_tree};
use std::process::Stdio;
use std::time::{Duration, Instant};

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

#[test]
fn fifo_input_fails_before_copy_without_blocking() {
    let temporary = ClaimedTempDir::new("oce-line-assembly-fifo");
    let copied = temporary.path().join("arm64");
    copy_tree(&super::fixture("arm64"), &copied);
    let log = copied.join("run-a.log");
    std::fs::remove_file(&log).unwrap();
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&log)
            .status()
            .unwrap()
            .success()
    );
    let root = super::repository_root();
    let destination = temporary.path().join("assembled");
    let mut child = std::process::Command::new("sh")
        .arg(root.join("tools/openmodelica-line-reference/line/assemble.sh"))
        .arg(&copied)
        .arg(super::fixture("amd64"))
        .arg(&destination)
        .current_dir(root)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if started.elapsed() > Duration::from_secs(5) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("assembly blocked on a FIFO input");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(!status.success());
    assert!(!destination.exists());
}
