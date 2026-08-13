//! Executable adversarial checks for the bound Python evidence verifier.

use std::os::unix::fs::{DirBuilderExt as _, MetadataExt as _};
use std::path::{Path, PathBuf};

use super::repository_root;

struct TempDir {
    path: PathBuf,
    device: u64,
    inode: u64,
}

impl TempDir {
    fn new() -> Self {
        let root = std::env::temp_dir().canonicalize().unwrap();
        for nonce in 0_u32..4096 {
            let path = root.join(format!(
                "oce-toggle-verifier-{}-{nonce}",
                std::process::id()
            ));
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
                Err(error) => panic!("claim verifier temp directory: {error}"),
            }
        }
        panic!("claim verifier temp directory")
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let metadata = std::fs::symlink_metadata(&self.path).expect("stat verifier temp directory");
        // A recursive cleanup is allowed only while the claimed directory identity is unchanged.
        assert!(!metadata.file_type().is_symlink());
        assert_eq!((metadata.dev(), metadata.ino()), (self.device, self.inode));
        std::fs::remove_dir_all(&self.path).expect("remove verifier temp directory");
    }
}

struct MutationCase {
    _temporary: TempDir,
    evidence: PathBuf,
}

fn fixture(root: &Path) -> PathBuf {
    root.join("crates/oce-conformance/tests/fixtures/open_modelica/logical_toggle")
}

fn copy_tree(source: &Path, destination: impl AsRef<Path>) {
    let destination = destination.as_ref();
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(destination)
        .unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        std::fs::copy(entry.path(), destination.join(entry.file_name())).unwrap();
    }
}

fn run(root: &Path, evidence: &Path) -> (bool, String) {
    let verifier = root.join("tools/openmodelica-toggle-reference/toggle/verify_evidence.py");
    let output = std::process::Command::new("perl")
        .args(["-e", "alarm shift; exec @ARGV", "10", "python3"])
        .arg(verifier)
        .arg(evidence)
        .arg(root)
        .output()
        .unwrap();
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn mutate(root: &Path, script: &str) -> MutationCase {
    let temporary = TempDir::new();
    let destination = temporary.path.join("evidence");
    copy_tree(&fixture(root), &destination);
    let output = std::process::Command::new("python3")
        .args(["-c", script])
        .arg(&destination)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    MutationCase {
        _temporary: temporary,
        evidence: destination,
    }
}

fn reject(root: &Path, case: MutationCase, class: &str) {
    let (success, error) = run(root, &case.evidence);
    assert!(!success, "adversarial verifier input passed");
    assert!(error.contains(class), "expected {class:?}, got {error:?}");
    drop(case);
}

#[test]
fn verifier_accepts_checked_evidence_and_rejects_coordinated_mutations() {
    let root = repository_root().canonicalize().unwrap();
    assert!(run(&root, &fixture(&root)).0);

    reject(
        &root,
        mutate(
            &root,
            r#"
import hashlib,json,pathlib,sys
d=pathlib.Path(sys.argv[1]); p=d/'toggle.canonical.csv'; p.write_text(p.read_text().replace('0 1.0 0.0 1.0','0 1.0 0.0 0.0',1)); m=json.loads((d/'manifest.json').read_text()); next(a for a in m['artifacts'] if a['role']=='canonical_csv')['sha256']=hashlib.sha256(p.read_bytes()).hexdigest(); (d/'manifest.json').write_text(json.dumps(m,indent=2)+'\n')
"#,
        ),
        "keep-last projection",
    );
    reject(
        &root,
        mutate(
            &root,
            r#"
import hashlib,json,pathlib,sys
d=pathlib.Path(sys.argv[1]); p=d/'projection-mutation.log'; p.write_text(p.read_text().replace('mutated_schedule_mismatch_rows=1,3,5,7,9,11,12,14,16,17,19','mutated_schedule_mismatch_rows=1')); m=json.loads((d/'manifest.json').read_text()); next(a for a in m['artifacts'] if a['role']=='projection_mutation_log')['sha256']=hashlib.sha256(p.read_bytes()).hexdigest(); (d/'manifest.json').write_text(json.dumps(m,indent=2)+'\n')
"#,
        ),
        "projection mutation record",
    );
    reject(
        &root,
        mutate(
            &root,
            r#"
import hashlib,json,pathlib,sys
d=pathlib.Path(sys.argv[1]); p=d/'run-a.log'; p.write_text(p.read_text().replace('simulationOptions = ','simulationOptions = MUTATED ',1)); h=hashlib.sha256(p.read_bytes()).hexdigest(); m=json.loads((d/'manifest.json').read_text()); m['runs'][0]['log_sha256']=h; next(a for a in m['artifacts'] if a['role']=='run_a_log')['sha256']=h; (d/'manifest.json').write_text(json.dumps(m,indent=2)+'\n')
"#,
        ),
        "simulation options",
    );
    reject(
        &root,
        mutate(
            &root,
            r#"
import json,pathlib,sys
d=pathlib.Path(sys.argv[1]); m=json.loads((d/'manifest.json').read_text()); m['simulation']['simflags']='-noEventEmit'; (d/'manifest.json').write_text(json.dumps(m,indent=2)+'\n')
"#,
        ),
        "simulation literals",
    );
}

#[test]
fn verifier_rejects_final_and_ancestor_path_attacks_without_hanging() {
    let root = repository_root().canonicalize().unwrap();
    for (script, class) in [
        (
            r#"import pathlib,sys; d=pathlib.Path(sys.argv[1]); p=d/'toggle.canonical.csv'; target=d.parent/'real.csv'; p.rename(target); p.symlink_to(target)"#,
            "without following links",
        ),
        (
            r#"import os,pathlib,sys; d=pathlib.Path(sys.argv[1]); p=d/'toggle.canonical.csv'; p.unlink(); os.mkfifo(p)"#,
            "bounded regular file",
        ),
    ] {
        reject(&root, mutate(&root, script), class);
    }

    use std::os::unix::fs::symlink;
    let temporary = TempDir::new();
    let copied_root = temporary.path.join("repository");
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&copied_root)
        .unwrap();
    let tool = copied_root.join("tools/openmodelica-toggle-reference/toggle");
    std::fs::create_dir_all(&tool).unwrap();
    std::fs::copy(
        root.join("tools/openmodelica-toggle-reference/toggle/verify_evidence.py"),
        tool.join("verify_evidence.py"),
    )
    .unwrap();
    copy_tree(&fixture(&root), copied_root.join("evidence"));
    symlink(&copied_root, temporary.path.join("repository-link")).unwrap();
    assert!(
        !run(
            &temporary.path.join("repository-link"),
            &copied_root.join("evidence")
        )
        .0
    );
    symlink(
        copied_root.join("evidence"),
        temporary.path.join("evidence-link"),
    )
    .unwrap();
    assert!(!run(&copied_root, &temporary.path.join("evidence-link")).0);
}
