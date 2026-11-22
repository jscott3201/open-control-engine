//! What `oce verify --commission --out DIR` refuses to write over, and what it reports when a
//! write fails (commissioning note, "CLI" "Output", decision 37; issue #664 and the review of
//! GitHub #757):
//!
//! - any entry already at `DIR/receipt.json`, a file, a link to a file outside `DIR` or inside it,
//!   or a dangling link, is `write-failed` before the run, and nothing anywhere is written: a link
//!   never carries the receipt out of `DIR`, and a stale receipt never stands beside new curves;
//! - a dangling link at `DIR/outputs`, and a `DIR` that is a file, are refused before the run;
//! - a curve that cannot be written fails the run (exit 2, no receipt), and the report's `curves`
//!   lists exactly the files that were written;
//! - `--deny-warnings` counts a load warning as it counts a run-time one.

mod support;

use std::fs;
use std::path::Path;
use std::process::Command;

use support::commission::{edit, run_layout, signals_layout, tree};
use support::{Run, oce_in};

fn commission(dir: &Path, extra: &[&str]) -> Run {
    let mut args = vec!["verify", "--commission", "spec.json"];
    args.extend_from_slice(extra);
    oce_in(dir, &args, b"")
}

/// The one error of a refused run: `(code, at)`.
fn refusal(run: &Run) -> (String, String) {
    assert_eq!(run.code, 2, "{}", run.transcript());
    let report = run.report();
    let error = &report["errors"][0];
    (
        error["code"].as_str().expect("code").to_owned(),
        error["at"].as_str().unwrap_or_default().to_owned(),
    )
}

#[test]
fn a_receipt_already_in_dir_is_refused_before_the_run_and_nothing_is_written() {
    // The review's case: a receipt left by an earlier run, its `outputs/` gone. Written over,
    // it would stand beside a new run's partial curves; it is refused and kept as it is.
    let dir = run_layout("commission_guard_stale", "pass");
    fs::create_dir(dir.join("out")).expect("out");
    fs::write(dir.join("out/receipt.json"), b"{\"stale\": true}\n").expect("a stale receipt");
    let run = commission(&dir, &["--out", "out"]);
    assert_eq!(
        refusal(&run),
        ("write-failed".to_owned(), "receipt.json".to_owned())
    );
    assert!(
        run.report()["errors"][0]["detail"]
            .as_str()
            .expect("detail")
            .starts_with("`out/receipt.json`: it already exists"),
        "{}",
        run.transcript()
    );
    assert_eq!(
        tree(&dir.join("out")),
        [("receipt.json".to_owned(), b"{\"stale\": true}\n".to_vec())]
    );
}

#[cfg(unix)]
#[test]
fn a_link_at_receipt_json_is_refused_and_its_target_is_never_written() {
    use std::os::unix::fs::symlink;
    // A link to a file outside DIR, a link to a file of the bundle the run does not read, and a
    // dangling link: each refused before the run, each target untouched or never created.
    for (name, target, exists) in [
        ("outside", "../victimdir/victim.txt", true),
        ("unread", "../spec2.json", true),
        ("dangling", "../nowhere.txt", false),
    ] {
        let dir = run_layout(&format!("commission_guard_link_{name}"), "pass");
        fs::create_dir(dir.join("out")).expect("out");
        fs::create_dir(dir.join("victimdir")).expect("victimdir");
        fs::write(dir.join("victimdir/victim.txt"), b"PRECIOUS\n").expect("victim");
        fs::copy(dir.join("spec.json"), dir.join("spec2.json")).expect("spec2");
        symlink(target, dir.join("out/receipt.json")).expect("a link");
        let before = tree(&dir);
        let run = commission(&dir, &["--out", "out"]);
        assert_eq!(
            refusal(&run),
            ("write-failed".to_owned(), "receipt.json".to_owned()),
            "{name}"
        );
        assert_eq!(tree(&dir), before, "{name}: nothing written anywhere");
        assert_eq!(dir.join("out").join(target).exists(), exists, "{name}");
        assert!(
            fs::symlink_metadata(dir.join("out/receipt.json"))
                .expect("the link")
                .file_type()
                .is_symlink(),
            "{name}: the link is left as it was"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_dangling_link_at_outputs_is_refused_before_the_run() {
    // The engine would refuse a frame of this run; the dangling link is refused first, by the
    // check before the run, which reads the entry itself and not where it points.
    let dir = run_layout("commission_guard_outputs_link", "pass");
    edit(
        &dir,
        "inputs.csv",
        "10,21,1,true,2,50",
        "10,21,1,true,2,2000",
    );
    fs::create_dir(dir.join("out")).expect("out");
    std::os::unix::fs::symlink("../nowhere", dir.join("out/outputs")).expect("a dangling link");
    let run = commission(&dir, &["--out", "out"]);
    assert_eq!(
        refusal(&run),
        ("write-failed".to_owned(), "outputs".to_owned())
    );
    assert!(!dir.join("nowhere").exists());
}

#[test]
fn an_out_that_is_a_file_is_refused_as_not_a_directory() {
    let dir = run_layout("commission_guard_out_file", "pass");
    fs::write(dir.join("out"), b"a file").expect("a file");
    let run = commission(&dir, &["--out", "out"]);
    assert_eq!(refusal(&run), ("write-failed".to_owned(), ".".to_owned()));
    assert_eq!(
        run.report()["errors"][0]["detail"],
        "`out`: the output directory is not a directory"
    );
    assert_eq!(fs::read(dir.join("out")).expect("out"), b"a file");
}

#[test]
fn deny_warnings_counts_a_load_warning() {
    // The `signals` subject loads with `unused-boundary-input`, and these rows fire no Assert.
    let dir = signals_layout(
        "commission_guard_load_warning",
        &[(0, 2.0, 2.0, true), (10, 3.0, 3.0, true)],
    );
    let plain = commission(&dir, &[]);
    let denied = commission(&dir, &["--deny-warnings"]);
    assert_eq!((plain.code, denied.code), (0, 1), "{}", denied.transcript());
    assert_eq!(
        plain.stdout, denied.stdout,
        "the receipt bytes do not change"
    );
    let receipt: serde_json::Value = serde_json::from_slice(&plain.stdout).expect("a receipt");
    assert_eq!(receipt["body"]["load_warnings_total"], 1);
    assert_eq!(receipt["body"]["warnings_total"], 0);
}

/// Whether `path`, relative to `dir`, names an entry (a relative path a test process could not
/// name absolutely when it is near the platform's path limit).
fn exists_in(dir: &Path, path: &str) -> bool {
    Command::new("test")
        .args(["-e", path])
        .current_dir(dir)
        .status()
        .expect("test runs")
        .success()
}

/// A relative directory path of exactly `len` bytes, unique for `tag`, every component at most
/// 200 bytes.
fn long_relative(len: usize, tag: usize) -> String {
    let mut path = format!("s{tag:03}");
    while path.len() < len {
        let room = len - path.len() - 1;
        if room == 0 {
            path.push('z');
        } else {
            path.push('/');
            path.push_str(&"d".repeat(room.min(200)));
        }
    }
    path
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn a_curve_that_cannot_be_written_fails_the_run_and_the_report_lists_only_what_exists() {
    // `DIR` a relative path just under the platform's path limit (1,024 bytes on macOS, 4,096 on
    // Linux, the terminating NUL included), so the longer temporary names under it are refused
    // (ENAMETOOLONG). Swept over a range of lengths, the runs fail at every stage: before the run,
    // at `outputs/`, at an output's folder, at a curve after others were written, at the receipt,
    // or not at all. Every run must answer consistently: a failure is exit 2 with no receipt, and
    // `curves` lists exactly files that exist; a pass has its receipt and every listed file.
    let limit: usize = if cfg!(target_os = "macos") {
        1024
    } else {
        4096
    };
    let dir = run_layout("commission_guard_long", "pass");
    let (mut curve_failure, mut partial, mut passed) = (false, false, false);
    for slack in 12..=64 {
        let out = long_relative(limit - slack, slack);
        let made = Command::new("mkdir")
            .args(["-p", &out])
            .current_dir(&dir)
            .status()
            .expect("mkdir runs");
        assert!(made.success(), "{slack}");
        let run = commission(&dir, &["--out", &out, "--bounds"]);
        let report = run.report();
        let listed: Vec<String> = report["curves"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|output| output["files"].as_array().cloned().unwrap_or_default())
            .map(|file| file.as_str().expect("a name").to_owned())
            .collect();
        for file in &listed {
            assert!(
                exists_in(&dir, file),
                "{slack}: `{file}` is listed and absent"
            );
        }
        let receipt = exists_in(&dir, &format!("{out}/receipt.json"));
        match run.code {
            0 => {
                assert!(receipt, "{slack}: a pass has its receipt");
                assert_eq!(listed.len(), 12, "{slack}");
                passed = true;
            }
            2 => {
                assert!(!receipt, "{slack}: a failure leaves no receipt");
                let (code, at) = refusal(&run);
                assert_eq!(code, "write-failed", "{slack}: {}", run.transcript());
                if at.starts_with("outputs/") && at.ends_with(".csv") {
                    curve_failure = true;
                    assert!(!exists_in(&dir, &format!("{out}/{at}")), "{slack}: {at}");
                    partial |= !listed.is_empty();
                }
            }
            other => panic!("{slack}: exit {other}\n{}", run.transcript()),
        }
    }
    assert!(curve_failure, "some run failed at a curve");
    assert!(partial, "some run failed at a curve after writing others");
    assert!(passed, "some run passed");
}
