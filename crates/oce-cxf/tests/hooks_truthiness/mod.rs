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
        for script in ["check-file-size.sh", "check-no-secrets.sh"] {
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

/// Proves that the real pre-commit shell path agrees with `oce_bless` over the pinned ASCII
/// domain. Probes are derived from `BLESS_DISABLED_VALUES` so additions cannot evade the pin.
#[test]
fn real_hook_agrees_with_canonical_ascii_truthiness() {
    let mut probes = BTreeSet::from([
        "",
        "0",
        "false",
        "FALSE",
        "False",
        "  false  ",
        "   ",
        "no",
        "off",
        "1",
        "true",
        "yes",
        "0.0",
    ]);
    probes.extend(oce_bless::BLESS_DISABLED_VALUES);
    let hook = repository_root().join(".githooks/pre-commit");

    for value in probes {
        let repo = TempRepo::new();
        let output = Command::new("bash")
            .arg(&hook)
            .current_dir(&repo.0)
            .env("OCE_SKIP_HOOKS", value)
            .output()
            .expect("run real pre-commit hook");
        assert!(
            output.status.success(),
            "real hook failed for {value:?}: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let skipped = String::from_utf8_lossy(&output.stdout).contains("[pre-commit] skipped");
        assert_eq!(
            skipped,
            oce_bless::enabled_for(value),
            "real hook truthiness diverged for {value:?}"
        );
    }

    let repo = TempRepo::new();
    fs::create_dir(repo.0.join(".githooks")).expect("create helper-free hook directory");
    let copied_hook = repo.0.join(".githooks/pre-commit");
    fs::copy(&hook, &copied_hook).expect("copy real hook without its helper");
    let output = Command::new("bash")
        .arg(&copied_hook)
        .current_dir(&repo.0)
        .env("OCE_SKIP_HOOKS", "1")
        .output()
        .expect("run real pre-commit hook without its helper");
    assert!(
        output.status.success(),
        "missing helper must fall back to checks: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("[pre-commit] skipped"),
        "missing helper must not skip checks"
    );
}

fn visit_shell_sources(root: &Path, sources: &mut Vec<PathBuf>) {
    for child in fs::read_dir(root).expect("enumerate repository") {
        let path = child.expect("read repository entry").path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if path.is_dir() {
            if !matches!(name, ".git" | "target") && !name.starts_with('_') {
                visit_shell_sources(&path, sources);
            }
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("sh")
            || path
                .parent()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                == Some(".githooks")
        {
            sources.push(path);
        }
    }
}

/// Pins the source criterion that shell files may test `OCE_SKIP_HOOKS` only by passing its value
/// to the shared `oce_enabled_for` predicate; presence tests such as `-n` and `-z` are forbidden.
#[test]
fn shell_skip_readers_delegate_to_the_shared_predicate() {
    let root = repository_root();
    let mut sources = Vec::new();
    visit_shell_sources(&root, &mut sources);
    let mut readers = BTreeSet::new();
    for path in sources {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
        if source.contains("${OCE_SKIP_HOOKS") {
            let relative = path
                .strip_prefix(&root)
                .expect("source under repository root");
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
            readers.insert(relative.to_path_buf());
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
