//! What `oce verify --commission --out DIR` refuses to write over, and what it reports when a
//! write fails (commissioning note, "CLI" "Output", decision 37; issue #664 and the review of
//! GitHub #757):
//!
//! - a regular file already at `DIR/receipt.json` is `write-failed` before the run, so a stale
//!   receipt never stands beside new curves; a link there, to a file outside `DIR` or inside it,
//!   or dangling, is `output-not-a-file` before anything is read (issue #758, ruled by the owner
//!   on 2026-11-22), and nothing anywhere is written: a link never carries the receipt out of
//!   `DIR`;
//! - a dangling link at `DIR/outputs`, and a `DIR` that is a file, are refused before the run;
//! - a curve that cannot be written fails the run (exit 2, no receipt), and the report's `curves`
//!   lists exactly the files that were written;
//! - `--deny-warnings` counts a load warning as it counts a run-time one.

mod support;

use std::fs;
use std::path::Path;
#[cfg(any(target_os = "macos", target_os = "linux"))]
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
    // dangling link: each refused before anything is read, each target untouched or never
    // created.
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
            ("output-not-a-file".to_owned(), "receipt.json".to_owned()),
            "{name}"
        );
        assert!(
            run.report()["errors"][0]["detail"]
                .as_str()
                .expect("detail")
                .starts_with("`out/receipt.json` is a symbolic link"),
            "{name}: {}",
            run.transcript()
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

/// Whether `path` (relative, `/`-separated) names a regular file below `dir`, reached one
/// component at a time from directory handles, so its length is no obstacle.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn is_file_below(dir: &Path, path: &str) -> bool {
    use rustix::fs::{CWD, Mode, OFlags, openat};
    let Ok(mut handle) = openat(CWD, dir, OFlags::RDONLY | OFlags::DIRECTORY, Mode::empty()) else {
        return false;
    };
    let mut segments: Vec<&str> = path.split('/').collect();
    let Some(last) = segments.pop() else {
        return false;
    };
    for segment in segments {
        match openat(
            &handle,
            segment,
            OFlags::RDONLY | OFlags::DIRECTORY,
            Mode::empty(),
        ) {
            Ok(next) => handle = next,
            Err(_) => return false,
        }
    }
    rustix::fs::statat(&handle, last, rustix::fs::AtFlags::empty())
        .is_ok_and(|stat| rustix::fs::FileType::from_raw_mode(stat.st_mode).is_file())
}

/// The top directories of the long paths a sweep made under `dir`, removed when it ends, an
/// assertion failing included: a path past the platform's limit left under `target/` keeps
/// `git worktree remove` from removing the worktree ("File name too long"). `remove_dir_all`
/// walks below each top directory from handles, so the depth below it is no obstacle.
#[cfg(any(target_os = "macos", target_os = "linux"))]
struct Sweep<'d> {
    dir: &'d Path,
    made: Vec<String>,
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
impl Sweep<'_> {
    /// Remove every top directory made, and name those that could not be removed.
    fn clear(&mut self) -> Vec<String> {
        let mut left = Vec::new();
        for top in self.made.drain(..) {
            match fs::remove_dir_all(self.dir.join(&top)) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => left.push(format!("{top}: {error}")),
            }
        }
        left
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
impl Drop for Sweep<'_> {
    fn drop(&mut self) {
        let _ = self.clear();
    }
}

/// A relative directory path of exactly `len` bytes, unique for `tag`, every component at most
/// 200 bytes; its top directory is `s<tag>`.
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
fn a_dir_just_under_the_path_limit_has_every_curve_written_through_its_handles() {
    // `DIR` a relative path just under the platform's path limit (1,024 bytes on macOS, 4,096 on
    // Linux, the terminating NUL included). Until issue #771 the curves were written by path, so
    // the longer names under `DIR` were refused (ENAMETOOLONG) and the runs failed at every
    // stage; each file is now created relative to the handle of the directory it goes into, so
    // the path's length is spent once, on `DIR`, and every run of the sweep writes its receipt
    // and all twelve curves. The files are checked from handles too: by its full path, no file
    // under such a `DIR` can be reached.
    let limit: usize = if cfg!(target_os = "macos") {
        1024
    } else {
        4096
    };
    let dir = run_layout("commission_guard_long", "pass");
    let mut sweep = Sweep {
        dir: &dir,
        made: Vec::new(),
    };
    for slack in 12..=64 {
        let out = long_relative(limit - slack, slack);
        sweep.made.push(format!("s{slack:03}"));
        let made = Command::new("mkdir")
            .args(["-p", &out])
            .current_dir(&dir)
            .status()
            .expect("mkdir runs");
        assert!(made.success(), "{slack}");
        let run = commission(&dir, &["--out", &out, "--bounds"]);
        assert_eq!(run.code, 0, "{slack}: {}", run.transcript());
        let report = run.report();
        let listed: Vec<String> = report["curves"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|output| output["files"].as_array().cloned().unwrap_or_default())
            .map(|file| file.as_str().expect("a name").to_owned())
            .collect();
        assert_eq!(listed.len(), 12, "{slack}");
        for file in &listed {
            assert!(
                is_file_below(&dir, file),
                "{slack}: `{file}` is listed and absent"
            );
        }
        assert!(
            is_file_below(&dir, &format!("{out}/receipt.json")),
            "{slack}: the receipt"
        );
    }
    assert_eq!(
        sweep.clear(),
        Vec::<String>::new(),
        "every long path removed"
    );
    for slack in 12..=64 {
        assert!(
            !dir.join(format!("s{slack:03}")).exists(),
            "s{slack:03} left"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_read_only_dir_is_refused_at_outputs_and_left_as_it_was() {
    // `DIR` opens (it can be listed) but nothing can be created in it: `mkdirat` of `outputs`
    // fails, the run is `write-failed` at `outputs` with no receipt, and `DIR` is unchanged. A
    // user the permission bits do not bind can write anyway: nothing to test then.
    use std::os::unix::fs::PermissionsExt;
    let dir = run_layout("commission_guard_read_only", "pass");
    let out = dir.join("out");
    fs::create_dir(&out).expect("out");
    fs::write(out.join("note.txt"), b"KEEP").expect("note");
    fs::set_permissions(&out, fs::Permissions::from_mode(0o555)).expect("chmod");
    let writable = fs::create_dir(out.join("probe")).is_ok();
    let run = commission(&dir, &["--out", "out"]);
    fs::set_permissions(&out, fs::Permissions::from_mode(0o755)).expect("chmod back");
    if writable {
        return;
    }
    let (code, at) = refusal(&run);
    assert_eq!((code.as_str(), at.as_str()), ("write-failed", "outputs"));
    assert!(run.report()["curves"].as_array().is_none_or(Vec::is_empty));
    let mut left: Vec<String> = fs::read_dir(&out)
        .expect("list")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    assert_eq!(left, ["note.txt"], "DIR is left as it was");
    assert_eq!(fs::read(out.join("note.txt")).expect("note"), b"KEEP");
}
