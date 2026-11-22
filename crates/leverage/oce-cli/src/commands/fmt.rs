//! `oce fmt`: the canonical text form.
//!
//! Plain `fmt` prints the canonical text (`oce_author::fmt_text`; a wire file is converted with
//! `oce_author::print_text`). `--check` prints a report instead and answers whether the input is
//! already canonical **in its own form**: text against `fmt_text`, wire against
//! `oce_author::to_wire_string` (exit `0` yes, `1` no). `--write` rewrites a text file atomically
//! and only when it changes (`atomic_write`: the file is replaced, not rewritten in place),
//! reporting `changed` and `output` only once the write succeeded; it refuses stdin and wire
//! files. The file it rewrites is its input, so the output rule holds for the input: before
//! anything is read, a `FILE` that is a symbolic link, dangling or not, a directory or another
//! non-file is `output-not-a-file` (`atomic_write::target_accepted`), and the way out is the
//! path of the file a link names. A read or parse failure is exit `2`.
//!
//! **The file `--write` rewrites is read through its directory's handle** (Unix; issue #771):
//! the parent, as the path gives it, is opened once, following the caller's links; the entry is
//! judged from that handle without following a link (a link, a directory or another non-file
//! swapped in after the first check is the same `output-not-a-file`), opened with
//! `O_NOFOLLOW | O_NONBLOCK`, and the handle must be a regular file with the identity checked; its
//! size and bytes come from that handle. The rewrite goes into the same directory handle, takes
//! the mode of the file read (from its handle, never from whatever is at the name later), and is
//! renamed over the entry only if the entry is still the file read (device and inode, checked
//! just before the rename; `write-failed` otherwise, and nothing is written). A file swapped in
//! between that check and the rename is replaced (never written through); the review of GitHub
//! #777 hit that window in 402 of 3,000 contended runs, and issue #780 closes it. Windows reads
//! and writes by path (`directory`).

use std::ffi::OsString;
use std::path::Path;

use oce_author::{fmt_text, from_wire_bytes, print_text, to_wire_string};
use serde_json::Value;

use super::atomic_write::{Lend, Plan, not_a_target, write_in};
use super::contained::changed;
use super::directory::{Directory, Entry, Kind, ReadError, split};
use super::{Named, Outcome, report_for, target_accepted};
use crate::input::{self, Loaded, io_refusal, read_bounded, too_large};
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
    if write
        && let Err((code, detail)) = target_accepted(
            Path::new(&source.file),
            &input::display_name(&source.file),
            Named::Caller,
        )
    {
        report.error(code, detail, None);
        return refused(report);
    }
    let (rewrite, loaded) = if write {
        let form = match input::form_of(source) {
            Ok(form) => form,
            Err((code, detail)) => {
                report.error(code, detail, None);
                return refused(report);
            }
        };
        let shown = input::display_name(&source.file);
        match Rewrite::read(Path::new(&source.file), &shown, input::limit(form)) {
            Ok((rewrite, bytes)) => (Some(rewrite), Loaded { form, bytes }),
            Err((code, detail)) => {
                report.error(code, detail, None);
                return refused(report);
            }
        }
    } else {
        let Some(loaded) = input::load(source, streams.stdin, &mut report) else {
            return refused(report);
        };
        (None, loaded)
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
    if let Some(rewrite) = rewrite {
        if !original_is_canonical && let Err((code, detail)) = rewrite.write(canonical.as_bytes()) {
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

/// The file `--write` rewrites, as it was read (module header).
struct Rewrite {
    /// Its directory, opened once.
    directory: Directory,
    /// Its name in that directory.
    name: OsString,
    /// Its handle's metadata when it was read: the mode the rewrite takes and the identity the
    /// entry must still have.
    read: Entry,
}

impl Rewrite {
    /// Read the file at `path` (shown as `shown`) through its directory's handle, at most `bound`
    /// bytes (module header).
    fn read(
        path: &Path,
        shown: &str,
        bound: usize,
    ) -> Result<(Self, Vec<u8>), (&'static str, String)> {
        let Some((parent, name)) = split(path) else {
            // No last name (`/`, `..`): a directory, which the first check already refused.
            return Err(not_a_target(Kind::Directory, shown, Named::Caller));
        };
        let directory =
            Directory::open(parent).map_err(|error| {
                match super::directory::unlistable(parent, &error) {
                    Some(why) => (
                        codes::READ_FAILED,
                        format!("{why}; oce rewrites a file only through its directory"),
                    ),
                    None => io_refusal(error),
                }
            })?;
        let (file, read) = directory.read(&name, false).map_err(|error| match error {
            ReadError::Io(error) => io_refusal(error),
            ReadError::Link => not_a_target(Kind::Link, shown, Named::Caller),
            ReadError::NotFile(kind) => not_a_target(kind, shown, Named::Caller),
            ReadError::Changed => changed(),
        })?;
        if read.len > u64::try_from(bound).unwrap_or(u64::MAX) {
            return Err(too_large(read.len, bound));
        }
        let bytes = read_bounded(file, bound)?;
        Ok((
            Self {
                directory,
                name,
                read,
            },
            bytes,
        ))
    }

    /// Replace the file with `bytes` (module header).
    fn write(&self, bytes: &[u8]) -> Result<(), (&'static str, String)> {
        let plan = Plan {
            lend: Lend::Read(self.read.mode),
            unchanged: Some(self.read),
            made: &[],
        };
        write_in(&self.directory, &self.name, bytes, &plan)
    }
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

#[cfg(test)]
#[path = "fmt_tests.rs"]
mod tests;
