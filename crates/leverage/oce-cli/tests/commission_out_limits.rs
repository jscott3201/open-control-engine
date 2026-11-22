//! `verify --commission --out DIR` under the limits a write can meet after its checks (the review
//! of GitHub #777): a umask that leaves the directories it makes unreadable, and a descriptor
//! limit. `outputs/` and `outputs/<k>/` are made with `mkdirat` and then opened from their
//! parent's handle; a directory made and not opened is removed again (it is empty) and the
//! refusal says so, naming the cause in words ("permission denied", "too many open files"),
//! never "uncategorized error". What a later refusal can leave is the README's: `outputs/`, an
//! empty `outputs/<k>/`, and curves without a receipt.

#![cfg(unix)]

mod support;

use std::fs;
use std::path::Path;
use std::process::Command;

use support::commission::run_layout;

/// `oce verify --commission spec.json --out out --bounds` in `dir` through `sh`, after `setup`
/// (a `umask` or a `ulimit`): its exit status (`None` for a signal) and its stdout.
fn shell_run(dir: &Path, setup: &str) -> (Option<i32>, String) {
    let output = Command::new("sh")
        .arg("-c")
        .arg(format!("{setup} && exec \"$0\" \"$@\""))
        .arg(env!("CARGO_BIN_EXE_oce"))
        .args([
            "verify",
            "--commission",
            "spec.json",
            "--out",
            "out",
            "--bounds",
        ])
        .current_dir(dir)
        .output()
        .expect("sh runs");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    )
}

/// The first error's code, place and detail.
fn first_error(stdout: &str) -> (String, String, String) {
    let report: serde_json::Value = serde_json::from_str(stdout).expect("a report");
    let error = &report["errors"][0];
    let text = |key: &str| error[key].as_str().unwrap_or_default().to_owned();
    (text("code"), text("at"), text("detail"))
}

/// Every path under `dir`, relative, sorted, directories included.
fn paths(dir: &Path) -> Vec<String> {
    fn walk(root: &Path, at: &Path, out: &mut Vec<String>) {
        for entry in fs::read_dir(at).expect("list") {
            let path = entry.expect("entry").path();
            out.push(
                path.strip_prefix(root)
                    .expect("under")
                    .to_string_lossy()
                    .into_owned(),
            );
            if fs::symlink_metadata(&path).expect("kind").is_dir() {
                walk(root, &path, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

#[test]
fn a_umask_that_leaves_outputs_unreadable_is_refused_and_leaves_nothing() {
    // `umask 0477` makes `outputs/` with mode `0300`: made, and not openable read-only. A user
    // the permission bits do not bind opens it anyway: nothing to test then.
    let dir = run_layout("commission_limits_umask", "pass");
    fs::create_dir(dir.join("out")).expect("out");
    let probe = dir.join("probe");
    fs::create_dir(&probe).expect("probe");
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&probe, fs::Permissions::from_mode(0o300)).expect("chmod");
    }
    let privileged = fs::read_dir(&probe).is_ok();
    fs::remove_dir(&probe).expect("probe");
    if privileged {
        return;
    }
    let (code, stdout) = shell_run(&dir, "umask 0477");
    assert_eq!(code, Some(2), "{stdout}");
    assert_eq!(
        first_error(&stdout),
        (
            "write-failed".to_owned(),
            "outputs".to_owned(),
            "`out/outputs`: it was made but could not be opened (permission denied), and is \
             removed; nothing is written there"
                .to_owned()
        )
    );
    assert!(paths(&dir.join("out")).is_empty(), "nothing is left in out");
}

#[test]
fn a_descriptor_limit_never_leaves_a_directory_it_could_not_open_or_an_uncategorized_error() {
    // Swept over descriptor limits, the runs fail at every stage of the reads and the writes, or
    // pass. A directory made and not opened is removed and named as such; no detail says
    // "uncategorized error"; a refused run leaves no receipt and no temporary file; and what a
    // write-time refusal leaves is only what the README says it can.
    let dir = run_layout("commission_limits_descriptors", "pass");
    let (mut unopened, mut passed) = (false, false);
    let mut seen = Vec::new();
    for limit in 3..=32 {
        let out = dir.join("out");
        let _ = fs::remove_dir_all(&out);
        fs::create_dir(&out).expect("out");
        let (code, stdout) = shell_run(&dir, &format!("ulimit -n {limit}"));
        match code {
            Some(0 | 1) => passed = true,
            Some(2) => {
                let (code, at, detail) = first_error(&stdout);
                assert!(
                    !detail.contains("uncategorized"),
                    "{limit}: {code} at {at}: {detail}"
                );
                if detail.contains("was made but could not be opened") {
                    assert!(
                        detail.contains("(too many open files)"),
                        "{limit}: {detail}"
                    );
                    assert!(!out.join(&at).exists(), "{limit}: `{at}` is left");
                    unopened = true;
                }
                let left = paths(&out);
                assert!(!left.iter().any(|path| path == "receipt.json"), "{limit}");
                assert!(
                    !left.iter().any(|path| path.ends_with(".oce-tmp")),
                    "{limit}: {left:?}"
                );
                for path in &left {
                    assert!(
                        path == "outputs" || path.starts_with("outputs/"),
                        "{limit}: `{path}` is not something a write-time refusal leaves"
                    );
                }
                seen.push((limit, code, at));
            }
            // The process could not start, or start its runtime, at so few descriptors.
            _ => seen.push((limit, "no status".to_owned(), String::new())),
        }
    }
    assert!(
        unopened,
        "no run met the limit opening a directory it made: {seen:?}"
    );
    assert!(passed, "no run passed: {seen:?}");
}
