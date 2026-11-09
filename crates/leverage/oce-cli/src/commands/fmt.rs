//! `oce fmt`: the canonical text form.
//!
//! Plain `fmt` prints the canonical text (`oce_author::fmt_text`; a wire file is converted with
//! `oce_author::print_text`). `--check` prints a report instead and answers whether the input is
//! already canonical **in its own form**: text against `fmt_text`, wire against
//! `oce_author::to_wire_string` (exit `0` yes, `1` no). `--write` rewrites a text file atomically
//! and only when it changes (`atomic_write`: a symbolic link is followed, the file is replaced,
//! not rewritten in place), reporting `changed` and `output` only once the write succeeded; it
//! refuses stdin and wire files. A read or parse failure is exit `2`.

use std::path::Path;

use oce_author::{fmt_text, from_wire_bytes, print_text, to_wire_string};
use serde_json::Value;

use super::{Outcome, report_for, write_atomically};
use crate::input::{self, Loaded};
use crate::report::Report;
use crate::{Exit, Form, Source, Streams, codes};

/// Run `fmt`.
pub(crate) fn run(source: &Source, check: bool, write: bool, streams: &mut Streams<'_>) -> Outcome {
    let mut report = report_for("fmt", &source.file);
    report.set("form", Value::Null);
    if write && input::is_stdin(&source.file) {
        report.error(
            codes::WRITE_NEEDS_FILE,
            "--write rewrites a file in place and cannot rewrite stdin",
            None,
        );
        return refused(report);
    }
    let Some(loaded) = input::load(source, streams.stdin, &mut report) else {
        return refused(report);
    };
    report.set("form", Value::from(form_name(loaded.form)));
    if write && loaded.form == Form::Wire {
        report.error(
            codes::WRITE_NEEDS_TEXT,
            "--write rewrites text files only; redirect `oce fmt FILE.json` to a .oce file instead",
            None,
        );
        return refused(report);
    }
    let Some((canonical, original_is_canonical)) = canonical(&loaded, !check, &mut report) else {
        return refused(report);
    };
    if check {
        report.set("canonical", Value::Bool(original_is_canonical));
        report.verdict(if original_is_canonical {
            "canonical"
        } else {
            "not canonical"
        });
        let exit = if original_is_canonical {
            Exit::Yes
        } else {
            Exit::No
        };
        return Outcome::with_exit(exit, report);
    }
    if write {
        if !original_is_canonical
            && let Err((code, detail)) =
                write_atomically(Path::new(&source.file), canonical.as_bytes())
        {
            // `changed` and `output` describe a finished write; a failed one carries neither.
            report.error(code, detail, None);
            report.verdict("refused: the file could not be rewritten");
            return Outcome::judged(report);
        }
        report.set("changed", Value::Bool(!original_is_canonical));
        report.set("output", Value::from(input::display_name(&source.file)));
        report.verdict(if original_is_canonical {
            "already canonical"
        } else {
            "rewritten"
        });
        return Outcome::with_exit(Exit::Yes, report);
    }
    report.verdict("canonical text on stdout");
    Outcome::document(report, canonical.into_bytes())
}

/// The canonical text (empty for a wire file when `need_text` is false), and whether the input was
/// canonical in its own form.
fn canonical(loaded: &Loaded, need_text: bool, report: &mut Report) -> Option<(String, bool)> {
    match loaded.form {
        Form::Text => {
            let text = input::decode_text(&loaded.bytes, report)?;
            match fmt_text(text) {
                Ok(canonical) => {
                    let same = canonical == text;
                    Some((canonical, same))
                }
                Err(error) => {
                    report.parse_error(&error);
                    None
                }
            }
        }
        Form::Wire => {
            let sequence = match from_wire_bytes(&loaded.bytes) {
                Ok(sequence) => sequence,
                Err(error) => {
                    input::wire_error(&error, report);
                    return None;
                }
            };
            let same = to_wire_string(&sequence).as_bytes() == loaded.bytes.as_slice();
            if !need_text {
                return Some((String::new(), same));
            }
            match print_text(&sequence) {
                Ok(text) => Some((text, same)),
                Err(error) => {
                    report.error(codes::UNSPELLABLE, error.to_string(), None);
                    None
                }
            }
        }
    }
}

fn form_name(form: Form) -> &'static str {
    match form {
        Form::Text => "text",
        Form::Wire => "wire",
    }
}

fn refused(mut report: Report) -> Outcome {
    report.verdict("refused");
    Outcome::judged(report)
}
