//! The five commands, and how one run's outcome reaches the streams.
//!
//! Every command returns an [`Outcome`]: its exit status, its report, and optionally a document
//! (CXF, canonical text, a text listing) that stdout carries instead of the report's JSON. Exactly
//! one of the two is written to stdout; the human-readable rendering of the report goes to stderr
//! when asked for.

pub(crate) mod catalog;
pub(crate) mod emit;
pub(crate) mod fmt;
pub(crate) mod validate;
pub(crate) mod vocab;

use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::report::Report;
use crate::{Exit, Streams, codes, input};

/// What one command produced.
pub(crate) struct Outcome {
    /// The exit status.
    pub(crate) exit: Exit,
    /// The report (written to stdout unless `document` is set; rendered to stderr on request).
    pub(crate) report: Report,
    /// A document stdout carries instead of the report.
    pub(crate) document: Option<Vec<u8>>,
}

impl Outcome {
    /// A report-only outcome whose status follows the report: `2` with an error, `1` with an
    /// error finding, else `0`.
    pub(crate) fn judged(report: Report) -> Self {
        let exit = if report.has_errors() {
            Exit::Error
        } else if report.has_error_finding() {
            Exit::No
        } else {
            Exit::Yes
        };
        Self {
            exit,
            report,
            document: None,
        }
    }

    /// An outcome with an explicit status and no document.
    pub(crate) fn with_exit(exit: Exit, report: Report) -> Self {
        Self {
            exit,
            report,
            document: None,
        }
    }

    /// A clean outcome whose product is `document`.
    pub(crate) fn document(report: Report, document: Vec<u8>) -> Self {
        Self {
            exit: Exit::Yes,
            report,
            document: Some(document),
        }
    }
}

/// Write the outcome to the streams. A failed stdout write turns the status into an error.
pub(crate) fn finish(outcome: Outcome, render: bool, streams: &mut Streams<'_>) -> Exit {
    let Outcome {
        exit,
        mut report,
        document,
    } = outcome;
    let ok = exit == Exit::Yes;
    let bytes = document.unwrap_or_else(|| report.to_json(ok).into_bytes());
    let written = streams
        .stdout
        .write_all(&bytes)
        .and_then(|()| streams.stdout.flush());
    if render {
        if written.is_err() {
            report.verdict("stdout could not be written");
        }
        // The rendering is advice; a failed stderr write changes nothing.
        let _ = streams.stderr.write_all(report.to_human().as_bytes());
        let _ = streams.stderr.flush();
    }
    if written.is_err() { Exit::Error } else { exit }
}

/// Write `bytes` to `path` atomically: a temporary file beside it, synced, then renamed over it.
/// On failure the temporary file is removed and the target is untouched.
pub(crate) fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), (&'static str, String)> {
    let temporary = temporary_path(path);
    let result = File::create(&temporary)
        .and_then(|mut file| {
            file.write_all(bytes)?;
            file.sync_all()
        })
        .and_then(|()| fs::rename(&temporary, path));
    result.map_err(|error| {
        let _ = fs::remove_file(&temporary);
        (
            codes::WRITE_FAILED,
            format!("the output could not be written: {}", error.kind()),
        )
    })
}

/// `dir/.name.oce-tmp` beside `path`, so the rename never crosses a file system.
fn temporary_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map_or_else(|| "output".into(), OsStr::to_string_lossy);
    path.with_file_name(format!(".{name}.oce-tmp"))
}

/// The report of a command that reads one sequence.
pub(crate) fn report_for(command: &'static str, file: &OsStr) -> Report {
    Report::new(command, Some(input::display_name(file)))
}
