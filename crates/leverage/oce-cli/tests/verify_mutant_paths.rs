//! How `oce verify` resolves a mutant path (the owner's ruling of 2026-11-11; vectors note, "Part
//! 3's readings" and "Part 4's readings"; GitHub issue #522). The format's lexical rule refuses an
//! absolute path, an empty, `.` or `..` segment and a backslash, and admits the aliases below; the
//! CLI resolves each listing, keeps it inside the vector file's directory
//! (`mutant-outside-directory`) and refuses two listings that are one file (`duplicate-mutant`).
//!
//! Where a platform cannot express a case, the test says so and pins what that platform does:
//! - symbolic links and hard links are created on Unix only (Windows needs a privilege to create
//!   a link, and std has no stable file index there, so hard links are not caught on Windows);
//! - case and Unicode normalization twins are one file only where the file system folds them,
//!   which each test detects on its own scratch directory;
//! - a trailing dot or space, and a drive prefix such as `C:`, mean something only on Windows;
//!   elsewhere they are ordinary names.
//!
//! A listing that is not a regular file is refused from its metadata **before** it is opened, so
//! a named pipe, which would block an open for reading until a writer came, cannot hang the run;
//! the named-pipe and socket tests are Unix-only (Windows has neither as a file in a directory).
//! Device nodes need a privilege to create and are not tested; they are refused by the same
//! metadata check. Windows device names (`NUL`, `CON`) are not tested either: they open no
//! regular file, so resolution refuses them as `not-a-file` or `not-found` like any other
//! non-file.

mod support;

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;
use support::{crate_dir, oce_in, oce_in_guarded, scratch};

/// A scratch directory holding `set/rule.cxf.jsonld` (AHU-0016's rule) and returning the `set`
/// directory, where the vector file goes.
fn prepared(name: &str) -> (PathBuf, PathBuf) {
    let root = scratch(name);
    let set = laid_out(&root);
    (root, set)
}

/// `root/set` with AHU-0016's rule in it; returns the `set` directory.
fn laid_out(root: &Path) -> PathBuf {
    let set = root.join("set");
    fs::create_dir(&set).expect("the set's directory");
    fs::copy(
        crate_dir().join("../../oce-api/tests/fixtures/library_rules/AHU-0016/rule.cxf.jsonld"),
        set.join("rule.cxf.jsonld"),
    )
    .expect("copy the rule");
    set
}

/// A short directory of this test's own under the system temporary directory, removed when it
/// is dropped, the test's failure included. A Unix socket's path must fit `sun_path` (104 bytes
/// on macOS, 108 on Linux), which a path under the checkout's target directory may not; the name
/// carries the process id and a clock reading, and is created with `create_dir`, so two tests
/// running at once never share one.
#[cfg(unix)]
struct ShortDir(PathBuf);

#[cfg(unix)]
impl ShortDir {
    fn new(tag: &str) -> Self {
        let base = std::env::temp_dir();
        for attempt in 0..16_u32 {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |since| since.subsec_nanos());
            let dir = base.join(format!(
                "oce-{tag}-{}-{nanos:x}-{attempt}",
                std::process::id()
            ));
            if fs::create_dir(&dir).is_ok() {
                return Self(dir);
            }
        }
        panic!("no fresh directory under the system temporary directory");
    }
}

#[cfg(unix)]
impl Drop for ShortDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A valid mutant of AHU-0016's rule (the persistence delay 900 s to 930 s).
fn mutant_bytes() -> Vec<u8> {
    fs::read(crate_dir().join("../oce-verify/tests/fixtures/AHU-0016/mutants/late_alarm.cxf.json"))
        .expect("the late-alarm mutant")
}

/// Write a one-scenario set listing `paths` into `set/vectors.json`.
fn write_set(set: &Path, paths: &[&str]) {
    let listed: Vec<Value> = paths
        .iter()
        .map(|path| serde_json::json!({ "path": path }))
        .collect();
    let document = serde_json::json!({
        "schema": "oce/vectors/v1",
        "clock": { "step_s": 60, "horizon_s": 60 },
        "mutants": listed,
        "scenarios": [{
            "name": "both_closed",
            "inputs": { "htg_vlv_cmd": 0.0, "clg_vlv_cmd": 0.0 },
            "expect": [{ "output": "yFault", "from_s": 0, "to_s": 60, "equals": false }]
        }]
    });
    fs::write(set.join("vectors.json"), document.to_string()).expect("write the set");
}

/// Write a one-scenario set listing `paths` into `set/vectors.json` and run it from `root`.
fn verify(root: &Path, set: &Path, paths: &[&str]) -> (i32, Value) {
    write_set(set, paths);
    let run = oce_in(
        root,
        &[
            "verify",
            "set/vectors.json",
            "--subject",
            "set/rule.cxf.jsonld",
        ],
        b"",
    );
    let text = run.stdout_text().to_owned();
    for needle in ["/Users/", "/home/", "/tmp/", "/private/", ":\\"] {
        assert!(
            !text.contains(needle),
            "an absolute path reached stdout: {text}"
        );
    }
    (run.code, run.report())
}

/// The first error's code and subject.
fn refusal(report: &Value) -> (String, String) {
    let error = &report["errors"][0];
    (
        error["code"].as_str().unwrap_or_default().to_owned(),
        error["at"].as_str().unwrap_or_default().to_owned(),
    )
}

fn pair(code: &str, subject: &str) -> (String, String) {
    (code.to_owned(), subject.to_owned())
}

/// A receipt (a verdict), not a refusal.
fn verdict(code: i32, report: &Value) {
    assert!(code == 0 || code == 1, "{report}");
    assert!(
        report.get("tool").is_none(),
        "a receipt, not a report: {report}"
    );
}

#[test]
fn an_unreadable_mutant_is_named_by_its_place() {
    let (root, set) = prepared("paths_unreadable");
    fs::create_dir(set.join("dir.cxf.json")).expect("a directory");
    let (code, report) = verify(&root, &set, &["missing.cxf.json"]);
    assert_eq!(
        (code, refusal(&report)),
        (2, pair("not-found", "mutants[0]"))
    );
    let detail = report["errors"][0]["detail"].as_str().expect("detail");
    assert!(detail.contains("`missing.cxf.json`"), "{detail}");
    fs::write(set.join("ok.cxf.json"), mutant_bytes()).expect("a mutant");
    let (code, report) = verify(&root, &set, &["ok.cxf.json", "dir.cxf.json"]);
    assert_eq!(
        (code, refusal(&report)),
        (2, pair("not-a-file", "mutants[1]"))
    );
}

#[test]
fn a_mutant_in_a_subdirectory_resolves_inside() {
    let (root, set) = prepared("paths_subdirectory");
    fs::create_dir(set.join("mutants")).expect("mutants");
    fs::write(set.join("mutants/late.cxf.json"), mutant_bytes()).expect("a mutant");
    let (code, report) = verify(&root, &set, &["mutants/late.cxf.json"]);
    verdict(code, &report);
}

#[cfg(unix)]
#[test]
fn a_link_out_of_the_directory_is_refused() {
    use std::os::unix::fs::symlink;
    let (root, set) = prepared("paths_link_out");
    fs::write(root.join("outside.cxf.json"), mutant_bytes()).expect("outside");
    symlink("../outside.cxf.json", set.join("out.cxf.json")).expect("a link out");
    let (code, report) = verify(&root, &set, &["out.cxf.json"]);
    assert_eq!(
        (code, refusal(&report)),
        (2, pair("mutant-outside-directory", "mutants[0]"))
    );
    // A linked directory out of it is refused the same way.
    fs::create_dir(root.join("elsewhere")).expect("elsewhere");
    fs::write(root.join("elsewhere/m.cxf.json"), mutant_bytes()).expect("m");
    symlink("../elsewhere", set.join("sub")).expect("a linked directory");
    let (code, report) = verify(&root, &set, &["sub/m.cxf.json"]);
    assert_eq!(
        (code, refusal(&report)),
        (2, pair("mutant-outside-directory", "mutants[0]"))
    );
}

#[cfg(unix)]
#[test]
fn a_link_inside_the_directory_resolves_and_is_one_file_with_its_target() {
    use std::os::unix::fs::symlink;
    let (root, set) = prepared("paths_link_in");
    fs::write(set.join("m.cxf.json"), mutant_bytes()).expect("m");
    symlink("m.cxf.json", set.join("link.cxf.json")).expect("a link in");
    let (code, report) = verify(&root, &set, &["link.cxf.json"]);
    verdict(code, &report);
    let (code, report) = verify(&root, &set, &["m.cxf.json", "link.cxf.json"]);
    assert_eq!(
        (code, refusal(&report)),
        (2, pair("duplicate-mutant", "mutants[1]"))
    );
}

#[cfg(unix)]
#[test]
fn hard_links_are_one_file_on_unix() {
    let (root, set) = prepared("paths_hard_link");
    fs::write(set.join("m.cxf.json"), mutant_bytes()).expect("m");
    fs::hard_link(set.join("m.cxf.json"), set.join("h.cxf.json")).expect("a hard link");
    let (code, report) = verify(&root, &set, &["m.cxf.json", "h.cxf.json"]);
    assert_eq!(
        (code, refusal(&report)),
        (2, pair("duplicate-mutant", "mutants[1]"))
    );
}

#[test]
fn case_twins_are_one_file_where_the_file_system_folds_case() {
    let (root, set) = prepared("paths_case_twins");
    fs::write(set.join("m.cxf.json"), mutant_bytes()).expect("m");
    let folds = set.join("M.cxf.json").exists();
    let (code, report) = verify(&root, &set, &["m.cxf.json", "M.cxf.json"]);
    let expected = if folds {
        pair("duplicate-mutant", "mutants[1]")
    } else {
        // Case-sensitive: `M.cxf.json` is another name, and no such file exists.
        pair("not-found", "mutants[1]")
    };
    assert_eq!((code, refusal(&report)), (2, expected), "folds: {folds}");
}

#[test]
fn normalization_twins_are_one_file_where_the_file_system_folds_them() {
    let (root, set) = prepared("paths_normalization_twins");
    let composed = "caf\u{e9}.cxf.json";
    let decomposed = "cafe\u{301}.cxf.json";
    fs::write(set.join(composed), mutant_bytes()).expect("the composed name");
    let folds = set.join(decomposed).exists();
    let (code, report) = verify(&root, &set, &[composed, decomposed]);
    let expected = if folds {
        pair("duplicate-mutant", "mutants[1]")
    } else {
        pair("not-found", "mutants[1]")
    };
    assert_eq!((code, refusal(&report)), (2, expected), "folds: {folds}");
}

#[test]
fn a_trailing_dot_or_space_is_the_same_file_only_on_windows() {
    let (root, set) = prepared("paths_trailing");
    fs::write(set.join("m.cxf.json"), mutant_bytes()).expect("m");
    for twin in ["m.cxf.json.", "m.cxf.json "] {
        let (code, report) = verify(&root, &set, &["m.cxf.json", twin]);
        // Windows strips a trailing dot or space from a name; elsewhere it is another name.
        let expected = if cfg!(windows) {
            pair("duplicate-mutant", "mutants[1]")
        } else {
            pair("not-found", "mutants[1]")
        };
        assert_eq!((code, refusal(&report)), (2, expected), "{twin:?}");
    }
}

#[test]
fn a_drive_prefix_is_a_name_on_unix_and_leaves_the_directory_on_windows() {
    let (root, set) = prepared("paths_drive_prefix");
    if cfg!(windows) {
        // `D:m.cxf.json`, with the scratch directory's own drive, is drive-relative: it names
        // `m.cxf.json` in the current directory on that drive, which `oce_in` makes the scratch
        // root, outside `set`. Joining it onto the set's directory replaces the directory.
        let drive: String = root.to_string_lossy().chars().take(2).collect();
        fs::write(root.join("m.cxf.json"), mutant_bytes()).expect("m outside");
        let listing = format!("{drive}m.cxf.json");
        let (code, report) = verify(&root, &set, &[&listing]);
        assert_eq!(
            (code, refusal(&report)),
            (2, pair("mutant-outside-directory", "mutants[0]"))
        );
    } else {
        // On Unix `C:` is an ordinary directory name inside the set's directory.
        fs::create_dir(set.join("C:")).expect("a directory named C:");
        fs::write(set.join("C:/m.cxf.json"), mutant_bytes()).expect("m");
        let (code, report) = verify(&root, &set, &["C:/m.cxf.json"]);
        verdict(code, &report);
    }
}

#[test]
fn a_dot_dot_segment_never_reaches_resolution() {
    let (root, set) = prepared("paths_dot_dot");
    fs::write(root.join("outside.cxf.json"), mutant_bytes()).expect("outside");
    let (code, report) = verify(&root, &set, &["../outside.cxf.json"]);
    assert_eq!(
        (code, refusal(&report)),
        (2, pair("mutant-path-invalid", "mutants[0].path"))
    );
}

#[cfg(unix)]
#[test]
fn a_named_pipe_is_not_a_file_and_never_opened() {
    let (root, set) = prepared("paths_fifo");
    let made = std::process::Command::new("mkfifo")
        .arg(set.join("pipe.cxf.json"))
        .status()
        .expect("mkfifo runs");
    assert!(made.success(), "mkfifo made the pipe");
    write_set(&set, &["pipe.cxf.json"]);
    // The guard is a hang guard: opening the pipe for reading would block for ever, and a
    // regression must fail the test, not stall it. It asserts nothing about speed.
    let run = oce_in_guarded(
        &root,
        &[
            "verify",
            "set/vectors.json",
            "--subject",
            "set/rule.cxf.jsonld",
        ],
        std::time::Duration::from_secs(120),
    );
    assert_eq!(
        (run.code, refusal(&run.report())),
        (2, pair("not-a-file", "mutants[0]"))
    );
}

#[cfg(unix)]
#[test]
fn a_socket_is_not_a_file_and_never_opened() {
    // The socket is bound in a short directory of its own, so its path fits `sun_path` wherever
    // the checkout is.
    let short = ShortDir::new("sock");
    let set = laid_out(&short.0);
    fs::write(set.join("ok.cxf.json"), mutant_bytes()).expect("a mutant");
    let socket = set.join("socket.cxf.json");
    let _listener = match std::os::unix::net::UnixListener::bind(&socket) {
        Ok(listener) => listener,
        Err(error) => {
            // Not this test's subject: a platform that cannot bind a socket here skips, with
            // the reason, rather than failing.
            #[allow(
                clippy::print_stderr,
                reason = "an explicit skip note in the test output"
            )]
            {
                eprintln!(
                    "skipped: no Unix socket can be bound at a {}-byte path here: {error}",
                    socket.as_os_str().len()
                );
            }
            return;
        }
    };
    write_set(&set, &["ok.cxf.json", "socket.cxf.json"]);
    let run = oce_in_guarded(
        &short.0,
        &[
            "verify",
            "set/vectors.json",
            "--subject",
            "set/rule.cxf.jsonld",
        ],
        std::time::Duration::from_secs(120),
    );
    assert_eq!(
        (run.code, refusal(&run.report())),
        (2, pair("not-a-file", "mutants[1]"))
    );
}
