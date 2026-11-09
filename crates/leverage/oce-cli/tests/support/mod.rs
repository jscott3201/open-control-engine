//! Shared test support: run the built `oce` binary and compare its runs with goldens.
//!
//! Every run starts in the crate directory, so inputs are named by relative paths (the fixtures
//! of `oce-author` are referenced in place, `../oce-author/tests/fixtures/...`) and no absolute
//! path can reach an output. Goldens regenerate only with `OCE_BLESS_CLI=1` (the repository's one
//! truthiness policy, `oce-bless`).

#![allow(dead_code)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The golden switch.
pub const BLESS_VAR: &str = "OCE_BLESS_CLI";

/// One finished run of the binary.
#[derive(Debug)]
pub struct Run {
    /// The exit status.
    pub code: i32,
    /// Everything written to stdout.
    pub stdout: Vec<u8>,
    /// Everything written to stderr.
    pub stderr: Vec<u8>,
}

impl Run {
    /// stdout as UTF-8 (panics in the test on anything else).
    pub fn stdout_text(&self) -> &str {
        std::str::from_utf8(&self.stdout).expect("stdout is UTF-8")
    }

    /// stdout parsed as one JSON report.
    pub fn report(&self) -> serde_json::Value {
        serde_json::from_slice(&self.stdout).expect("stdout is one JSON document")
    }

    /// The run as the golden spells it: the exit status, then stdout, then stderr.
    pub fn transcript(&self) -> String {
        format!(
            "exit: {}\n--- stdout\n{}--- stderr\n{}",
            self.code,
            String::from_utf8_lossy(&self.stdout),
            String::from_utf8_lossy(&self.stderr)
        )
    }
}

/// The crate directory every run starts in.
pub fn crate_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// Run `oce` with `args` in `dir`, feeding `stdin`.
pub fn oce_in(dir: &Path, args: &[&str], stdin: &[u8]) -> Run {
    let mut child = Command::new(env!("CARGO_BIN_EXE_oce"))
        .args(args)
        .current_dir(dir)
        .env_remove(BLESS_VAR)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the oce binary starts");
    let mut pipe = child.stdin.take().expect("stdin is piped");
    // The binary may stop reading early (a refused over-size stdin); a broken pipe is expected.
    let _ = pipe.write_all(stdin);
    drop(pipe);
    let output = child.wait_with_output().expect("the oce binary finishes");
    Run {
        code: output
            .status
            .code()
            .expect("the binary exits with a status"),
        stdout: output.stdout,
        stderr: output.stderr,
    }
}

/// Run `oce` with `args` in the crate directory.
pub fn oce(args: &[&str], stdin: &[u8]) -> Run {
    oce_in(crate_dir(), args, stdin)
}

/// A fixture of `oce-author`, referenced in place, as a path relative to the crate directory.
pub fn author_fixture(relative: &str) -> String {
    format!("../oce-author/tests/fixtures/{relative}")
}

/// This crate's own input fixture, relative to the crate directory.
pub fn input(name: &str) -> String {
    format!("tests/fixtures/inputs/{name}")
}

/// Compare `actual` with the golden at `tests/fixtures/<relative>`, or write it when blessing.
pub fn compare_or_bless(relative: &str, actual: &str) {
    let path = crate_dir().join("tests/fixtures").join(relative);
    if oce_bless::enabled(BLESS_VAR) {
        std::fs::create_dir_all(path.parent().expect("a fixture directory")).expect("mkdir");
        std::fs::write(&path, actual).expect("write golden");
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("missing golden {relative}; bless with {BLESS_VAR}=1"));
    assert_eq!(
        actual, expected,
        "golden {relative} differs; bless with {BLESS_VAR}=1"
    );
}

/// A fresh scratch directory for one test, under cargo's per-crate temporary directory.
pub fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("oce-cli")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create the scratch directory");
    dir
}
