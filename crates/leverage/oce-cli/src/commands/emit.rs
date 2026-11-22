//! `oce emit`: an authored sequence to flat CXF, after both validation stages.
//!
//! Order: the namespace and stage 1 (`oce_author::emit`), then stage 2 (`oce_author::validate`,
//! a throwaway engine load). Nothing is written while any error finding stands (exit `1`, the
//! report on stdout). On success the CXF is stdout, or with `-o FILE` it is written atomically
//! (`atomic_write`) and the report is stdout, with `output` and `bytes` set only once the write
//! succeeded. A malformed namespace, an unreadable or unparsable input and a failed write are
//! errors (exit `2`). So, decided before anything is read and in this order, are an `-o` that is
//! not absent or a regular file (`output-not-a-file`: a symbolic link, dangling or not, is never
//! written through; `atomic_write::target_accepted`) and an `-o` that is the input file
//! (`output-is-input`: the same file by `file_identity::same_file`, so another spelling of the
//! path, an input named through a symbolic link and, on Unix, a hard link to it), which would
//! replace the authored sequence with its CXF. On Unix the same holds for stdin (`-`) when the
//! shell made file descriptor 0 a regular file (`emit - --from text -o doc < doc`,
//! `file_identity::stdin_is_file`); on Windows stdin is not checked.
//!
//! The output's name must not contradict the CXF written (`output-form-mismatch`, also decided
//! before anything is read): a name `id::form_by_name` reads as text (`.oce`) or wire (`.json`)
//! is refused, so a file this verb writes is never one the other verbs misread. `.jsonld` and
//! `.cxf.json` are CXF and accepted; a name that rule reads no form from is accepted.

use std::ffi::OsStr;
use std::path::Path;

use oce_author::{DOCUMENT_TOO_LARGE, EmitError, EmitOptions, Severity, emit, validate};
use serde_json::Value;

use super::id::{form_by_name, form_name};
use super::validate::record;
use super::{
    Named, Outcome, report_for, same_file, stdin_is_file, target_accepted, write_atomically,
};
use crate::input::{self, Parsed};
use crate::report::Report;
use crate::{Exit, IdForm, Source, Streams, codes};

/// Run `emit`.
pub(crate) fn run(
    source: &Source,
    namespace: Option<&str>,
    output: Option<&OsStr>,
    streams: &mut Streams<'_>,
) -> Outcome {
    let mut report = report_for("emit", &source.file);
    let output = output.filter(|path| !input::is_stdin(path));
    if let Some(path) = output
        && let Err((code, detail)) = output_accepted(&source.file, path)
    {
        report.error(code, detail, None);
        return refused(report);
    }
    let Some(loaded) = input::load(source, streams.stdin, &mut report) else {
        return refused(report);
    };
    let Some(parsed) = input::parse(&loaded, &mut report) else {
        return refused(report);
    };
    let mut options = EmitOptions::new();
    if let Some(namespace) = namespace {
        options = options.with_namespace(namespace);
    }
    let bytes = match emit(&parsed.sequence, &options) {
        Ok(bytes) => bytes,
        Err(error) => return emit_refused(report, &parsed, error),
    };
    let mut findings = validate(&parsed.sequence);
    if let Some(spans) = &parsed.spans {
        spans.apply(&parsed.sequence, &mut findings);
    }
    if findings.iter().any(|f| f.severity == Severity::Error) {
        record(&mut report, &findings, false);
        report.verdict("refused: error findings, nothing written");
        return Outcome::judged(report);
    }
    record(&mut report, &findings, false);
    match output {
        None => {
            report.verdict(format!("{} bytes of CXF on stdout", bytes.len()));
            Outcome::document(report, bytes)
        }
        Some(path) => {
            match write_atomically(Path::new(path), &bytes) {
                Ok(()) => {
                    // `output` and `bytes` describe a finished write; a failed one carries neither.
                    report.set("output", Value::from(input::display_name(path)));
                    report.set("bytes", Value::from(bytes.len()));
                    report.verdict(format!(
                        "{} bytes of CXF written to {}",
                        bytes.len(),
                        input::display_name(path)
                    ));
                    Outcome::with_exit(Exit::Yes, report)
                }
                Err((code, detail)) => {
                    report.error(code, detail, None);
                    report.verdict("refused: the output could not be written");
                    Outcome::judged(report)
                }
            }
        }
    }
}

/// Whether `-o output` may be written for the input `input`: it must be absent or a regular file,
/// it must not be the input file (for stdin, the file descriptor 0 points at), and its name must
/// not read as text or wire.
fn output_accepted(input: &OsStr, output: &OsStr) -> Result<(), (&'static str, String)> {
    target_accepted(
        Path::new(output),
        &input::display_name(output),
        Named::Caller,
    )?;
    let is_input = if input::is_stdin(input) {
        stdin_is_file(Path::new(output))
    } else {
        same_file(Path::new(input), Path::new(output))
    };
    if is_input {
        return Err((
            codes::OUTPUT_IS_INPUT,
            "the output is the input file; the sequence would be replaced by its CXF".to_owned(),
        ));
    }
    if let Some(named @ (IdForm::Text | IdForm::Wire)) = form_by_name(output) {
        return Err((
            codes::OUTPUT_FORM_MISMATCH,
            format!(
                "the output is CXF and its name reads as {}: a name ending in .oce is read as \
                 text and in .json as wire; name it .jsonld or .cxf.json, or give it no such \
                 extension",
                form_name(named),
            ),
        ));
    }
    Ok(())
}

/// The emitter refused: stage-1 findings (exit 1), an over-size document (exit 1) or a malformed
/// namespace (exit 2).
fn emit_refused(mut report: Report, parsed: &Parsed, error: EmitError) -> Outcome {
    match error {
        EmitError::Invalid(mut findings) => {
            if let Some(spans) = &parsed.spans {
                spans.apply(&parsed.sequence, &mut findings);
            }
            record(&mut report, &findings, false);
            report.verdict("refused: error findings, nothing written");
        }
        EmitError::TooLarge { bytes, limit } => {
            report.set("stages", Value::Array(vec![Value::from("author")]));
            report.finding_raw(
                DOCUMENT_TOO_LARGE,
                "sequence",
                &format!("the document is {bytes} bytes; an engine loads at most {limit}"),
                "author",
            );
            report.verdict("refused: the document is too large, nothing written");
        }
        EmitError::InvalidNamespace { namespace, problem } => {
            report.set("stages", Value::Array(Vec::new()));
            report.error(
                codes::INVALID_NAMESPACE,
                format!("namespace `{namespace}` {problem}"),
                None,
            );
            report.verdict("refused: invalid namespace");
        }
        other => {
            report.set("stages", Value::Array(Vec::new()));
            report.error(codes::EMIT_REFUSED, other.to_string(), None);
            report.verdict("refused");
        }
    }
    Outcome::judged(report)
}

/// A run that stopped before any stage: at the output guard, the read or the parse.
fn refused(mut report: Report) -> Outcome {
    report.set("stages", Value::Array(Vec::new()));
    report.verdict("refused");
    Outcome::judged(report)
}
