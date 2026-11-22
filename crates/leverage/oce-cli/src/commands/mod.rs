//! The nine commands, and how one run's outcome reaches the streams.
//!
//! Every command returns an [`Outcome`]: its exit status, its report, and optionally a document
//! (CXF, canonical text, a wire document, a text listing, a receipt, a content id, a diff's
//! Markdown table) that stdout carries instead of the report's JSON. Exactly one of the two is
//! written to stdout; the human-readable rendering goes to stderr when asked for: the report's
//! own, or one the command gives (a diff's text). A document comes with exit `0`, except a
//! `verify` receipt (of a vector set or a commissioning spec) and a `diff` table, which are also
//! stdout for an answer of exit `1` (vectors note, "CLI"; commissioning note, "CLI"; diff note,
//! "CLI").

mod atomic_write;
pub(crate) mod catalog;
mod catalog_params;
mod commission_files;
mod commission_out;
mod contained;
pub(crate) mod diff;
pub(crate) mod emit;
mod file_identity;
pub(crate) mod fmt;
pub(crate) mod id;
pub(crate) mod import;
pub(crate) mod validate;
pub(crate) mod verify;
pub(crate) mod verify_commission;
mod verify_mutants;
mod verify_subject;
pub(crate) mod vocab;

use std::ffi::OsStr;

use crate::report::Report;
use crate::{Exit, Streams, input};

pub(crate) use atomic_write::{write_atomically, write_atomically_no_follow};
pub(crate) use file_identity::{same_file, stdin_is_file};

/// What one command produced.
pub(crate) struct Outcome {
    /// The exit status.
    pub(crate) exit: Exit,
    /// The report (written to stdout unless `document` is set; rendered to stderr on request).
    pub(crate) report: Report,
    /// A document stdout carries instead of the report.
    pub(crate) document: Option<Vec<u8>>,
    /// The human-readable rendering stderr carries instead of the report's, already made safe
    /// for a terminal; `None` for the report's own.
    pub(crate) rendering: Option<String>,
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
            rendering: None,
        }
    }

    /// An outcome with an explicit status and no document.
    pub(crate) fn with_exit(exit: Exit, report: Report) -> Self {
        Self {
            exit,
            report,
            document: None,
            rendering: None,
        }
    }

    /// A clean outcome whose product is `document`.
    pub(crate) fn document(report: Report, document: Vec<u8>) -> Self {
        Self {
            exit: Exit::Yes,
            report,
            document: Some(document),
            rendering: None,
        }
    }

    /// A verdict whose product is `document` whatever the answer: a `verify` receipt or a `diff`
    /// table on stdout for exit `0` or `1`.
    pub(crate) fn verdict(exit: Exit, report: Report, document: Vec<u8>) -> Self {
        Self {
            exit,
            report,
            document: Some(document),
            rendering: None,
        }
    }

    /// The outcome with `rendering` as the human-readable rendering in place of the report's.
    /// `rendering` must already be safe for a terminal (`report::printable_lines`).
    pub(crate) fn rendered_as(mut self, rendering: String) -> Self {
        self.rendering = Some(rendering);
        self
    }
}

/// Write the outcome to the streams. A failed stdout write turns the status into an error.
pub(crate) fn finish(outcome: Outcome, render: bool, streams: &mut Streams<'_>) -> Exit {
    let Outcome {
        exit,
        mut report,
        document,
        rendering,
    } = outcome;
    let ok = exit == Exit::Yes;
    let bytes = document.unwrap_or_else(|| report.to_json(ok).into_bytes());
    let written = streams
        .stdout
        .write_all(&bytes)
        .and_then(|()| streams.stdout.flush());
    if render {
        // A command's own rendering describes an answer stdout did not carry when the write
        // failed, so the report's, with that failure as its verdict, is rendered instead.
        let human = match rendering {
            Some(rendering) if written.is_ok() => rendering,
            _ => {
                if written.is_err() {
                    report.verdict("stdout could not be written");
                }
                report.to_human()
            }
        };
        // The rendering is advice; a failed stderr write changes nothing.
        let _ = streams.stderr.write_all(human.as_bytes());
        let _ = streams.stderr.flush();
    }
    if written.is_err() { Exit::Error } else { exit }
}

/// The report of a command that reads one sequence.
pub(crate) fn report_for(command: &'static str, file: &OsStr) -> Report {
    Report::new(command, Some(input::display_name(file)))
}
