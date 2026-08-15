//! Adversarial process tests for the retained Python evidence verifier.

use std::os::unix::fs::{DirBuilderExt as _, MetadataExt as _, symlink};
use std::path::{Path, PathBuf};

fn verifier() -> PathBuf {
    super::repository_root().join("tools/openmodelica-line-reference/line/verify_evidence.py")
}

fn run(arguments: &[&Path]) -> std::process::Output {
    let mut command = std::process::Command::new("python3");
    command.arg(verifier());
    for argument in arguments {
        command.arg(argument);
    }
    command.output().expect("execute Line evidence verifier")
}

pub(super) fn copy_tree(source: &Path, destination: &Path) {
    std::fs::create_dir(destination).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let kind = entry.file_type().unwrap();
        let target = destination.join(entry.file_name());
        if kind.is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            assert!(kind.is_file());
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn write_json(path: &Path, value: &serde_json::Value) {
    let mut bytes = serde_json::to_vec_pretty(value).unwrap();
    bytes.push(b'\n');
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn checked_python_verifier_accepts_the_retained_closure() {
    let root = super::repository_root();
    let fixture = super::fixture("");
    let mode = Path::new("final");
    let output = run(&[mode, &fixture, &root]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.stdout,
        b"Line assembled evidence verification passed\n"
    );
}

#[test]
fn digest_correct_canonical_output_tamper_still_fails_raw_reproduction() {
    let temporary = ClaimedTempDir::new("oce-line-verifier-canonical");
    let copied = temporary.path().join("arm64");
    copy_tree(&super::fixture("arm64"), &copied);
    let canonical = copied.join("line.canonical.csv");
    let original = std::fs::read_to_string(&canonical).unwrap();
    std::fs::write(&canonical, original.replacen("0.25 0.25", "0.5 0.25", 1)).unwrap();
    let update = std::process::Command::new("python3")
        .args([
            "-c",
            "import hashlib,json,pathlib,sys; d=pathlib.Path(sys.argv[1]); p=d/'line.canonical.csv'; m=json.loads((d/'architecture.json').read_text()); m['canonical_sha256']=hashlib.sha256(p.read_bytes()).hexdigest(); (d/'architecture.json').write_text(json.dumps(m,indent=2)+'\\n')",
        ])
        .arg(&copied)
        .status()
        .unwrap();
    assert!(update.success());
    let root = super::repository_root();
    let output = run(&[
        Path::new("architecture"),
        &copied,
        &root,
        Path::new("arm64"),
    ]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("raw keep-last projection"));
}

#[test]
fn verifier_rejects_malformed_duplicate_and_unknown_manifest_fields() {
    for (label, mutate, expected) in [
        (
            "malformed",
            Box::new(|_: &str| "{".to_string()) as Box<dyn Fn(&str) -> String>,
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
    ] {
        let temporary = ClaimedTempDir::new(&format!("oce-line-verifier-{label}"));
        let copied = temporary.path().join("fixture");
        copy_tree(&super::fixture(""), &copied);
        let manifest = copied.join("manifest.json");
        let original = std::fs::read_to_string(&manifest).unwrap();
        std::fs::write(&manifest, mutate(&original)).unwrap();
        let root = super::repository_root();
        let output = run(&[Path::new("final"), &copied, &root]);
        assert!(!output.status.success(), "{label}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{label}"
        );
    }
}

#[test]
fn final_verifier_rejects_open_artifact_and_nested_record_escapes() {
    for (label, mutate, expected) in [
        (
            "empty-artifacts",
            Box::new(|value: &mut serde_json::Value| {
                value["artifacts"] = serde_json::json!([]);
            }) as Box<dyn Fn(&mut serde_json::Value)>,
            "artifact closure count",
        ),
        (
            "unknown-role",
            Box::new(|value: &mut serde_json::Value| {
                value["artifacts"][0]["role"] = serde_json::json!("unknown");
            }),
            "unknown or misplaced artifact role/path",
        ),
        (
            "unknown-path",
            Box::new(|value: &mut serde_json::Value| {
                value["artifacts"][0]["path"] = serde_json::json!("unknown/path");
            }),
            "unknown or misplaced artifact role/path",
        ),
        (
            "missing-nested",
            Box::new(|value: &mut serde_json::Value| {
                value["image"].as_object_mut().unwrap().remove("tag");
            }),
            "unsupported or open image record",
        ),
        (
            "fixed-literal",
            Box::new(|value: &mut serde_json::Value| {
                value["simulation"]["method"] = serde_json::json!("euler");
            }),
            "unsupported or open simulation record",
        ),
    ] {
        let temporary = ClaimedTempDir::new(&format!("oce-line-final-{label}"));
        let copied = temporary.path().join("fixture");
        copy_tree(&super::fixture(""), &copied);
        let manifest = copied.join("manifest.json");
        let mut value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&manifest).unwrap()).unwrap();
        mutate(&mut value);
        write_json(&manifest, &value);
        let root = super::repository_root();
        let output = run(&[Path::new("final"), &copied, &root]);
        assert!(!output.status.success(), "{label}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{label}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn wrapper_change_after_native_generation_breaks_provenance_binding() {
    let temporary = ClaimedTempDir::new("oce-line-native-provenance");
    let copied = temporary.path().join("arm64");
    copy_tree(&super::fixture("arm64"), &copied);
    let root = temporary.path().join("repository");
    let repository = super::repository_root();
    for relative in [
        "tools/openmodelica-line-reference/line/LinePilot.mo",
        "tools/openmodelica-line-reference/line/LineFlagPilot.mo",
        "tools/openmodelica-line-reference/line/runner.sh",
        "tools/openmodelica-line-reference/line/regenerate.sh",
        "crates/oce-cxf/tests/open_modelica_line_reference/canonicalizer.rs",
        "tools/openmodelica-line-reference/src/main.rs",
        "tools/openmodelica-line-reference/Cargo.toml",
        "tools/openmodelica-line-reference/Cargo.lock",
        "tools/openmodelica-line-reference/line/generate_architecture.py",
        "tools/openmodelica-line-reference/line/verify_evidence.py",
    ] {
        let destination = root.join(relative);
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::copy(repository.join(relative), destination).unwrap();
    }
    let wrapper = root.join("tools/openmodelica-line-reference/line/LinePilot.mo");
    let mut bytes = std::fs::read(&wrapper).unwrap();
    bytes.extend_from_slice(b"\n// changed after native generation\n");
    std::fs::write(wrapper, bytes).unwrap();
    let output = run(&[
        Path::new("architecture"),
        &copied,
        &root,
        Path::new("arm64"),
    ]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("native generator inputs do not match assembly repository bytes")
    );
}

#[test]
fn verifier_rejects_symlink_and_fifo_without_blocking() {
    let temporary = ClaimedTempDir::new("oce-line-verifier-paths");
    let copied = temporary.path().join("arm64");
    copy_tree(&super::fixture("arm64"), &copied);
    let canonical = copied.join("line.canonical.csv");
    let target = temporary.path().join("canonical.csv");
    std::fs::rename(&canonical, &target).unwrap();
    symlink(&target, &canonical).unwrap();
    let root = super::repository_root();
    let output = run(&[
        Path::new("architecture"),
        &copied,
        &root,
        Path::new("arm64"),
    ]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("regular file"));

    std::fs::remove_file(&canonical).unwrap();
    std::fs::copy(&target, &canonical).unwrap();
    let record = copied.join("architecture.json");
    std::fs::remove_file(&record).unwrap();
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&record)
            .status()
            .unwrap()
            .success()
    );
    let output = run(&[
        Path::new("architecture"),
        &copied,
        &root,
        Path::new("arm64"),
    ]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("regular file"));
}

pub(super) struct ClaimedTempDir {
    path: PathBuf,
    device: u64,
    inode: u64,
}

impl ClaimedTempDir {
    pub(super) fn new(prefix: &str) -> Self {
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

    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for ClaimedTempDir {
    fn drop(&mut self) {
        let metadata = std::fs::symlink_metadata(&self.path).unwrap();
        assert!(!metadata.file_type().is_symlink());
        assert_eq!((metadata.dev(), metadata.ino()), (self.device, self.inode));
        std::fs::remove_dir_all(&self.path).unwrap();
    }
}
