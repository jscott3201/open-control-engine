//! Adversarial process tests for the retained Python verifier and assembler.

use std::os::unix::fs::{DirBuilderExt as _, MetadataExt as _, symlink};
use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};

fn verifier() -> PathBuf {
    super::repository_root().join("tools/openmodelica-reliefs-reference/reliefs/verify_evidence.py")
}

fn run(arguments: &[&Path]) -> std::process::Output {
    let mut command = std::process::Command::new("python3");
    command.arg(verifier()).env("PYTHONDONTWRITEBYTECODE", "1");
    for argument in arguments {
        command.arg(argument);
    }
    command.output().expect("execute Reliefs evidence verifier")
}

fn copy_tree(source: &Path, destination: &Path) {
    std::fs::create_dir(destination).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let target = destination.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn write_json(path: &Path, value: &serde_json::Value) {
    let mut bytes = serde_json::to_vec_pretty(value).unwrap();
    bytes.push(b'\n');
    std::fs::write(path, bytes).unwrap();
}

fn rebind_projection(directory: &Path, field: &str, file: &str) {
    let record = directory.join("architecture.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&record).unwrap()).unwrap();
    value["projection_mutation"][field] = format!(
        "{:x}",
        Sha256::digest(std::fs::read(directory.join(file)).unwrap())
    )
    .into();
    write_json(&record, &value);
}

#[test]
fn malformed_duplicate_unknown_and_type_confused_manifests_fail_closed() {
    for (label, mutate, expected) in [
        (
            "malformed",
            Box::new(|_: &str| "{".to_owned()) as Box<dyn Fn(&str) -> String>,
            "invalid JSON",
        ),
        (
            "duplicate",
            Box::new(|text: &str| {
                text.replacen("\"format\":", "\"format\": \"wrong\", \"format\":", 1)
            }),
            "duplicate JSON key",
        ),
        (
            "unknown",
            Box::new(|text: &str| {
                text.replacen("\"format\":", "\"unknown\": true, \"format\":", 1)
            }),
            "fields are not closed",
        ),
        (
            "boolean-as-one",
            Box::new(|text: &str| {
                text.replacen("\"event_emission\": true", "\"event_emission\": 1", 1)
            }),
            "unsupported or open simulation record",
        ),
    ] {
        let temporary = ClaimedTempDir::new(&format!("oce-reliefs-manifest-{label}"));
        let copied = temporary.path().join("fixture");
        copy_tree(&super::fixture(""), &copied);
        let path = copied.join("manifest.json");
        let original = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, mutate(&original)).unwrap();
        let output = run(&[Path::new("final"), &copied, &super::repository_root()]);
        assert!(!output.status.success(), "{label}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{label}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn rehashed_canonical_and_projection_tampering_fail_semantic_reproduction() {
    let temporary = ClaimedTempDir::new("oce-reliefs-canonical-tamper");
    let copied = temporary.path().join("arm64");
    copy_tree(&super::fixture("arm64"), &copied);
    let canonical = copied.join("reliefs.canonical.csv");
    let body = std::fs::read_to_string(&canonical).unwrap();
    std::fs::write(&canonical, body.replacen("-0.125 0.25", "-0.125 0.5", 1)).unwrap();
    let record = copied.join("architecture.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&record).unwrap()).unwrap();
    value["canonical_sha256"] =
        format!("{:x}", Sha256::digest(std::fs::read(&canonical).unwrap())).into();
    write_json(&record, &value);
    let output = run(&[
        Path::new("architecture"),
        &copied,
        &super::repository_root(),
        Path::new("arm64"),
    ]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("strict canonical bytes"));

    copy_tree(
        &super::fixture("arm64"),
        &temporary.path().join("projection"),
    );
    let projection = temporary.path().join("projection");
    std::fs::copy(
        projection.join("reliefs.canonical.csv"),
        projection.join("projection-keep-first.canonical.csv"),
    )
    .unwrap();
    rebind_projection(
        &projection,
        "canonical_sha256",
        "projection-keep-first.canonical.csv",
    );
    let output = run(&[
        Path::new("architecture"),
        &projection,
        &super::repository_root(),
        Path::new("arm64"),
    ]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("keep-first"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn missing_mixed_architecture_and_partial_assembly_leave_no_output() {
    let temporary = ClaimedTempDir::new("oce-reliefs-assembly-boundary");
    let missing = temporary.path().join("missing");
    let destination = temporary.path().join("assembled");
    let script =
        super::repository_root().join("tools/openmodelica-reliefs-reference/reliefs/assemble.sh");
    let output = std::process::Command::new("sh")
        .args([
            script.as_os_str(),
            super::fixture("arm64").as_os_str(),
            missing.as_os_str(),
            destination.as_os_str(),
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!destination.exists());

    let output = std::process::Command::new("sh")
        .args([
            script.as_os_str(),
            super::fixture("amd64").as_os_str(),
            super::fixture("arm64").as_os_str(),
            destination.as_os_str(),
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!destination.exists());
}

#[test]
fn symlink_hardlink_fifo_oversize_and_missing_entries_are_rejected() {
    for mode in ["symlink", "hardlink", "fifo", "oversize", "missing"] {
        let temporary = ClaimedTempDir::new(&format!("oce-reliefs-path-{mode}"));
        let copied = temporary.path().join("arm64");
        copy_tree(&super::fixture("arm64"), &copied);
        let path = copied.join("reliefs.canonical.csv");
        match mode {
            "symlink" => {
                let target = temporary.path().join("target");
                std::fs::rename(&path, &target).unwrap();
                symlink(target, path).unwrap();
            }
            "hardlink" => {
                let target = temporary.path().join("target");
                std::fs::hard_link(&path, target).unwrap();
            }
            "fifo" => {
                std::fs::remove_file(&path).unwrap();
                assert!(
                    std::process::Command::new("mkfifo")
                        .arg(path)
                        .status()
                        .unwrap()
                        .success()
                );
            }
            "oversize" => std::fs::write(path, vec![b'x'; 1024 * 1024 + 1]).unwrap(),
            "missing" => std::fs::remove_file(path).unwrap(),
            _ => unreachable!(),
        }
        let output = run(&[Path::new("precopy"), &copied]);
        assert!(!output.status.success(), "{mode}");
    }
}

struct ClaimedTempDir {
    path: PathBuf,
    device: u64,
    inode: u64,
}

impl ClaimedTempDir {
    fn new(prefix: &str) -> Self {
        let root = std::env::temp_dir().canonicalize().unwrap();
        for nonce in 0_u32..4096 {
            let path = root.join(format!("{prefix}-{}-{nonce}", std::process::id()));
            match std::fs::DirBuilder::new().mode(0o700).create(&path) {
                Ok(()) => {
                    let metadata = std::fs::symlink_metadata(&path).unwrap();
                    return Self {
                        path,
                        device: metadata.dev(),
                        inode: metadata.ino(),
                    };
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("cannot claim verifier directory: {error}"),
            }
        }
        panic!("cannot claim verifier directory")
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for ClaimedTempDir {
    fn drop(&mut self) {
        let metadata = std::fs::symlink_metadata(&self.path).unwrap();
        assert_eq!((metadata.dev(), metadata.ino()), (self.device, self.inode));
        std::fs::remove_dir_all(&self.path).unwrap();
    }
}
