//! `oce emit`: an authored sequence to flat CXF, after both validation stages.
//!
//! Order: the namespace and stage 1 (`oce_author::emit`), then stage 2 (`oce_author::validate`,
//! a throwaway engine load). Nothing is written while any error finding stands (exit `1`, the
//! report on stdout). On success the CXF is stdout, or with `-o FILE` it is written atomically
//! (`atomic_write`) and the report is stdout, with `output` and `bytes` set only once the write
//! succeeded. A malformed namespace, an unreadable or unparsable input and a failed write are
//! errors (exit `2`).

use std::ffi::OsStr;
use std::path::Path;

use oce_author::{DOCUMENT_TOO_LARGE, EmitError, EmitOptions, Severity, emit, validate};
use serde_json::Value;

use super::validate::record;
use super::{Outcome, report_for, write_atomically};
use crate::input::{self, Parsed};
use crate::report::Report;
use crate::{Exit, Source, Streams, codes};

/// Run `emit`.
pub(crate) fn run(
    source: &Source,
    namespace: Option<&str>,
    output: Option<&OsStr>,
    streams: &mut Streams<'_>,
) -> Outcome {
    let mut report = report_for("emit", &source.file);
    let output = output.filter(|path| !input::is_stdin(path));
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

/// A run that stopped at the read or the parse.
fn refused(mut report: Report) -> Outcome {
    report.set("stages", Value::Array(Vec::new()));
    report.verdict("refused");
    Outcome::judged(report)
}
