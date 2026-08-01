//! Pins shell hook opt-in truthiness to the canonical Rust policy.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TempRepo(PathBuf);

impl TempRepo {
    fn new() -> Self {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "oce-hook-truthiness-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(path.join(".github/scripts")).expect("create hook probe repository");
        let init = Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&path)
            .status()
            .expect("run git init for hook probe");
        assert!(init.success(), "git init failed for hook probe");
        for script in [
            "check-file-size.sh",
            "check-no-secrets.sh",
            "check-default-no-db.sh",
        ] {
            fs::write(path.join(".github/scripts").join(script), "exit 0\n")
                .expect("write hook probe stub");
        }
        Self(path)
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonical repository root")
}

/// Proves that both real hook shell paths agree with `oce_bless` over the pinned ASCII domain.
/// Probes are derived from `BLESS_DISABLED_VALUES` so additions cannot evade the pin.
#[test]
fn real_hook_agrees_with_canonical_ascii_truthiness() {
    let mut probes = BTreeSet::from([
        "",
        "0",
        "false",
        "FALSE",
        "False",
        "  false  ",
        "\nfalse",
        "   ",
        "no",
        "off",
        "1",
        "true",
        "yes",
        "0.0",
    ]);
    probes.extend(oce_bless::BLESS_DISABLED_VALUES);
    for hook_name in ["pre-commit", "pre-push"] {
        let hook = repository_root().join(".githooks").join(hook_name);
        for value in &probes {
            let repo = TempRepo::new();
            let output = Command::new("bash")
                .arg(&hook)
                .current_dir(&repo.0)
                .env("OCE_SKIP_HOOKS", value)
                .output()
                .unwrap_or_else(|error| panic!("run real {hook_name} hook: {error}"));
            assert!(
                output.status.success(),
                "real {hook_name} hook failed for {value:?}: stdout={} stderr={}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            let marker = format!("[{hook_name}] skipped");
            let skipped = String::from_utf8_lossy(&output.stdout).contains(&marker);
            assert_eq!(
                skipped,
                oce_bless::enabled_for(value),
                "real {hook_name} truthiness diverged for {value:?}"
            );
        }

        let repo = TempRepo::new();
        fs::create_dir(repo.0.join(".githooks")).expect("create helper-free hook directory");
        let copied_hook = repo.0.join(".githooks").join(hook_name);
        fs::copy(&hook, &copied_hook).expect("copy real hook without its helper");
        let output = Command::new("bash")
            .arg(&copied_hook)
            .current_dir(&repo.0)
            .env("OCE_SKIP_HOOKS", "1")
            .output()
            .unwrap_or_else(|error| panic!("run real {hook_name} without its helper: {error}"));
        assert!(
            output.status.success(),
            "missing helper must fall back to {hook_name} checks: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !String::from_utf8_lossy(&output.stdout).contains(&format!("[{hook_name}] skipped")),
            "missing helper must not skip {hook_name} checks"
        );
    }
}

fn tracked_shell_sources(root: &Path) -> Vec<PathBuf> {
    let output = Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(root)
        .output()
        .expect("enumerate tracked repository files");
    assert!(output.status.success(), "git ls-files failed");
    output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| PathBuf::from(String::from_utf8(path.to_vec()).expect("tracked path is UTF-8")))
        .filter(|path| {
            path.extension().and_then(|extension| extension.to_str()) == Some("sh")
                || path
                    .parent()
                    .and_then(Path::file_name)
                    .and_then(|name| name.to_str())
                    == Some(".githooks")
        })
        .collect()
}

/// Pins the source criterion that shell files may test `OCE_SKIP_HOOKS` only by passing its value
/// to the shared `oce_enabled_for` predicate; presence tests such as `-n` and `-z` are forbidden.
#[test]
fn shell_skip_readers_delegate_to_the_shared_predicate() {
    let root = repository_root();
    let mut readers = BTreeSet::new();
    for relative in tracked_shell_sources(&root) {
        let source = fs::read_to_string(root.join(&relative))
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", relative.display()));
        if source.contains("${OCE_SKIP_HOOKS") || source.contains("$OCE_SKIP_HOOKS") {
            assert!(
                !source.lines().any(|line| {
                    line.contains("OCE_SKIP_HOOKS")
                        && (line.contains("[ -n") || line.contains("[ -z"))
                }),
                "{} tests OCE_SKIP_HOOKS for presence",
                relative.display()
            );
            assert!(
                source.contains("oce_enabled_for \"${OCE_SKIP_HOOKS:-}\""),
                "{} must delegate OCE_SKIP_HOOKS to oce_enabled_for",
                relative.display()
            );
            readers.insert(relative);
        }
    }
    assert_eq!(
        readers,
        BTreeSet::from([
            PathBuf::from(".githooks/pre-commit"),
            PathBuf::from(".githooks/pre-push"),
        ]),
        "the shared predicate must be the only OCE_SKIP_HOOKS reader path"
    );
}

/// Pins LF index bytes, LF working-tree bytes, and the root `eol=lf` attribute for representative
/// byte-exact goldens and vendored inputs that must remain platform-invariant.
#[test]
fn representative_integrity_inputs_are_pinned_to_lf() {
    let root = repository_root();
    for path in [
        "crates/oce-conformance/tests/fixtures/golden/g36_traces/ahu_economizer.csv",
        "third_party/modelica-buildings-cdl/Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Controller.mo",
        "crates/oce-conformance/tests/fixtures/golden/g36_traces/ahu_economizer.prov.json",
    ] {
        let output = Command::new("git")
            .args(["ls-files", "--eol", "--", path])
            .current_dir(&root)
            .output()
            .expect("inspect tracked line-ending attributes");
        assert!(
            output.status.success(),
            "git ls-files --eol failed for {path}"
        );
        let verdict = String::from_utf8(output.stdout).expect("git eol output is UTF-8");
        assert!(
            verdict.contains("i/lf") && verdict.contains("w/lf") && verdict.contains("eol=lf"),
            "{path} must report i/lf, w/lf, and eol=lf; got {verdict:?}"
        );
    }
}
