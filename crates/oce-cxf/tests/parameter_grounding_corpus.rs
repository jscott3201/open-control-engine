//! Every tracked CXF fixture imports byte-identically under strict-finite parameter grounding
//! (issue #362).
//!
//! Parameter grounding refuses non-finite literals and non-finite expression intermediates at every
//! composite level. That refusal must be invisible to every legitimate document: this suite walks
//! every `.jsonld` fixture checked into the workspace (crate fixtures and the vendored CXF
//! translations), imports it, and compares a SHA-256 of the complete import outcome — the full
//! `ModelGraph` debug rendering plus every diagnostic on success, or the full error on refusal —
//! against a checked-in golden. The corpus is the `git ls-files` set, so untracked local files never
//! enter it. The golden was generated from the pre-change resolver, so an
//! unchanged golden proves the change altered no fixture's import, bit for bit, and no fixture
//! now reports `non-finite-parameter`.
//!
//! Regenerate deliberately with `OCE_BLESS=1 cargo test -p oce-cxf --test parameter_grounding_corpus`
//! and explain any diff.

mod bless;

use std::path::{Path, PathBuf};
use std::process::Command;

use oce_cxf::{ResolveOptions, import_cxf};
use sha2::{Digest, Sha256};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn golden_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/golden/fixture_corpus_import.txt")
}

/// Every git-tracked `.jsonld` under `crates/` and `third_party/`, as sorted workspace-relative
/// paths. Enumerated with `git ls-files` (as `hooks_truthiness` does) so an untracked local file
/// can neither join the corpus nor shift its pinned size.
fn corpus() -> Vec<(String, PathBuf)> {
    let root = workspace_root();
    let output = Command::new("git")
        .args(["ls-files", "-z", "--", "crates", "third_party"])
        .current_dir(&root)
        .output()
        .expect("enumerate tracked repository files");
    assert!(output.status.success(), "git ls-files failed");
    let mut corpus: Vec<_> = output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| String::from_utf8(path.to_vec()).expect("tracked path is UTF-8"))
        .filter(|rel| rel.ends_with(".jsonld"))
        .map(|rel| {
            let path = root.join(&rel);
            (rel, path)
        })
        .collect();
    corpus.sort();
    corpus
}

fn outcome_line(rel: &str, bytes: &[u8]) -> String {
    let (status, rendered) = match import_cxf(bytes, &ResolveOptions::default()) {
        Ok((graph, report)) => ("ok", format!("{graph:?}\n{:?}", report.diagnostics)),
        Err(error) => ("err", format!("{error:?}")),
    };
    assert!(
        !rendered.contains("NonFiniteParameter"),
        "{rel}: no tracked fixture may carry a non-finite parameter"
    );
    let digest = Sha256::digest(rendered.as_bytes());
    let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("{rel} {status} {hex}")
}

#[test]
fn every_tracked_fixture_imports_byte_identically() {
    let corpus = corpus();
    // The workspace tracks 214 `.jsonld` files; a walk that silently shrinks fails here.
    assert_eq!(corpus.len(), 214, "fixture corpus size changed");
    let actual: String = corpus
        .iter()
        .map(|(rel, path)| {
            let bytes = std::fs::read(path).expect("fixture readable");
            outcome_line(rel, &bytes) + "\n"
        })
        .collect();
    if bless::enabled() {
        std::fs::write(golden_path(), &actual).expect("write golden");
        return;
    }
    let expected = std::fs::read_to_string(golden_path())
        .expect("golden missing — regenerate deliberately with OCE_BLESS=1");
    for (want, got) in expected.lines().zip(actual.lines()) {
        assert_eq!(got, want, "fixture import outcome changed");
    }
    assert_eq!(actual, expected);
}
