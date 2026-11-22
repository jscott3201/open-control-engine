//! The library entry over in-memory streams: a stdout that cannot be written turns any outcome
//! into exit 2 (a closed pipe must never read as a clean answer), and the program name in the
//! arguments never reaches the output.

use std::io::{self, Write};

use oce_cli::{Exit, Streams, run};

/// A worked example of the diff note, as `oce-diff` keeps it; the run starts in the crate
/// directory.
const DIFF_BASE: &str = "../oce-diff/tests/fixtures/examples/pressure_request.base.oce";
const DIFF_HEAD: &str = "../oce-diff/tests/fixtures/examples/pressure_request.head.oce";

/// A writer that refuses every write, as a closed pipe does.
struct Closed;

impl Write for Closed {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::from(io::ErrorKind::BrokenPipe))
    }
    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::from(io::ErrorKind::BrokenPipe))
    }
}

fn run_with(args: &[&str], stdout: &mut dyn Write) -> (Exit, Vec<u8>) {
    let mut stdin: &[u8] = b"";
    let mut stderr = Vec::new();
    let mut streams = Streams {
        stdin: &mut stdin,
        stdout,
        stderr: &mut stderr,
        stderr_is_terminal: false,
    };
    (run(args.iter().copied(), &mut streams), stderr)
}

#[test]
fn a_closed_stdout_is_an_error_whatever_the_answer() {
    for args in [
        &["oce", "vocab"][..],
        &["oce", "vocab", "--json"][..],
        &["oce", "catalog"][..],
        &["oce", "--help"][..],
        &["oce", "validate", "missing.oce"][..],
        &["oce", "id", "missing.cxf.json"][..],
        &[
            "oce",
            "id",
            "../oce-author/tests/fixtures/emit/econ_enable.cxf.json",
        ][..],
        &["oce", "import", "missing.cxf.json"][..],
        &[
            "oce",
            "import",
            "../oce-author/tests/fixtures/emit/econ_enable.cxf.json",
        ][..],
        &[
            "oce",
            "import",
            "../oce-author/tests/fixtures/emit/econ_enable.cxf.json",
            "--wire",
        ][..],
        &["oce", "diff", "missing.oce", "missing.oce"][..],
        &["oce", "diff", DIFF_BASE, DIFF_BASE][..],
        &["oce", "diff", DIFF_BASE, DIFF_HEAD][..],
        &["oce", "diff", DIFF_BASE, DIFF_HEAD, "--format", "markdown"][..],
    ] {
        let (exit, _) = run_with(args, &mut Closed);
        assert_eq!(exit, Exit::Error, "{args:?}");
        assert_eq!(exit.code(), 2);
    }
}

#[test]
fn a_closed_stdout_is_named_in_the_rendering_when_asked() {
    let (exit, stderr) = run_with(&["oce", "vocab", "--render", "always"], &mut Closed);
    assert_eq!(exit, Exit::Error);
    assert_eq!(stderr, b"oce vocab: stdout could not be written\n");
}

#[test]
fn a_diff_stdout_could_not_carry_is_not_rendered_as_an_answer() {
    // The diff's text rendering describes an answer; when stdout fails the run answered nothing,
    // so the rendering is the report's, with the failure as its verdict.
    let (exit, stderr) = run_with(
        &["oce", "diff", DIFF_BASE, DIFF_HEAD, "--render", "always"],
        &mut Closed,
    );
    assert_eq!(exit, Exit::Error);
    assert_eq!(
        String::from_utf8(stderr).expect("UTF-8"),
        format!("oce diff {DIFF_BASE}: stdout could not be written\n")
    );
    // With stdout open, the same run renders the diff's text.
    let mut stdout = Vec::new();
    let (exit, stderr) = run_with(
        &["oce", "diff", DIFF_BASE, DIFF_HEAD, "--render", "always"],
        &mut stdout,
    );
    assert_eq!(exit, Exit::No);
    assert!(
        String::from_utf8(stderr)
            .expect("UTF-8")
            .starts_with("# oce diff zone_pressure_request -> zone_pressure_request\n")
    );
}

#[test]
fn the_program_name_never_reaches_the_output() {
    let mut first = Vec::new();
    let mut second = Vec::new();
    let (a, _) = run_with(&["oce", "fmt", "--help"], &mut first);
    let (b, _) = run_with(&["some/where/oce.exe", "fmt", "--help"], &mut second);
    assert_eq!((a, b), (Exit::Yes, Exit::Yes));
    assert_eq!(first, second);
    assert!(String::from_utf8(first).unwrap().contains("Usage: oce fmt"));
}

#[test]
fn exit_codes_are_zero_one_two() {
    assert_eq!(
        [Exit::Yes.code(), Exit::No.code(), Exit::Error.code()],
        [0, 1, 2]
    );
}
