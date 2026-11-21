//! `oce id`: the canonical content id of a CXF document or an authored sequence (content-id note
//! `docs/design/content-id.md`, "Mapping table for the library re-stamp"; GitHub issue #513).
//!
//! Stdout is the id (`oce_api::Engine::content_id`: `oce:canon1:` and 64 lowercase hex digits)
//! and one line feed, exit `0`. Anything that leaves the input without an id is exit `2` with the
//! `oce-report/1` report; the verb never answers `1`. `--json` prints the report in place of the
//! id line, with `content_id` and `form`.
//!
//! Order, the first refusal winning:
//!
//! 1. the form: `--from` when given; else stdin is `missing-from`; else CXF for a name ending in
//!    `.jsonld` or `.cxf.json` (tested before the `.json` rule, as `verify` reads a subject),
//!    text for `.oce`, wire for any other `.json`; else `unknown-form`;
//! 2. `--namespace` beside a CXF document is `namespace-needs-authored`: the flag names the
//!    namespace the emitter writes, and a CXF document is never rewritten;
//! 3. a bounded read: a CXF document at `oce_api::MAX_CXF_BYTES`, a text or wire file at its
//!    form's own limit, a file's size from its metadata before a byte is read;
//! 4. for a text or wire file, what `oce emit` does: the parse, the emitter under the
//!    `--namespace` rule, then both validation stages. An error finding is `not-loadable` with
//!    the findings in the report; a malformed namespace is `invalid-namespace`;
//! 5. a fresh engine loads the CXF bytes (`not-loadable`, with the engine's diagnostic codes)
//!    and names the loaded model.
//!
//! The id leaves the instance namespace out, so `--namespace` never moves it; the flag is here
//! so that the verb accepts exactly the invocations `oce emit` accepts.

use std::ffi::OsStr;
use std::path::Path;

use oce_api::Engine;
use oce_author::{EmitError, EmitOptions, Severity, emit, validate};
use serde_json::Value;

use super::Outcome;
use super::validate::record;
use crate::input::{self, Loaded};
use crate::report::{Facts, Report};
use crate::{Form, IdArgs, IdForm, Streams, codes};

/// A refusal: its code and a path-free detail.
type Refusal = (&'static str, String);

/// Run `id`.
pub(crate) fn run(args: &IdArgs, streams: &mut Streams<'_>) -> Outcome {
    let mut report = Report::new("id", Some(input::display_name(&args.file)));
    report.set("form", Value::Null);
    let form = match form_of(args) {
        Ok(form) => form,
        Err((code, detail)) => {
            report.error(code, detail, None);
            return refused(report);
        }
    };
    report.set("form", Value::from(form_name(form)));
    let Some(cxf) = document(args, form, streams, &mut report) else {
        return refused(report);
    };
    let id = match content_id(&cxf) {
        Ok(id) => id,
        Err(refusal) => {
            refusal.record(&mut report);
            return refused(report);
        }
    };
    report.set("content_id", Value::from(id.clone()));
    report.verdict(id.clone());
    if args.json {
        return Outcome::judged(report);
    }
    Outcome::document(report, format!("{id}\n").into_bytes())
}

/// The form of the input `args` names.
fn form_of(args: &IdArgs) -> Result<IdForm, Refusal> {
    if let Some(form) = args.from {
        return Ok(form);
    }
    if input::is_stdin(&args.file) {
        return Err((
            codes::MISSING_FROM,
            "reading stdin needs --from cxf, --from text or --from wire".to_owned(),
        ));
    }
    let path = Path::new(&args.file);
    let name = path
        .file_name()
        .map(OsStr::to_string_lossy)
        .unwrap_or_default();
    if name.ends_with(".jsonld") || name.ends_with(".cxf.json") {
        return Ok(IdForm::Cxf);
    }
    match path.extension().and_then(OsStr::to_str) {
        Some("oce") => Ok(IdForm::Text),
        Some("json") => Ok(IdForm::Wire),
        _ => Err((
            codes::UNKNOWN_FORM,
            "the file name ends in none of .jsonld, .cxf.json (CXF), .oce (text) and .json \
             (wire); pass --from cxf, text or wire"
                .to_owned(),
        )),
    }
}

/// The form as the report spells it.
fn form_name(form: IdForm) -> &'static str {
    match form {
        IdForm::Cxf => "cxf",
        IdForm::Text => "text",
        IdForm::Wire => "wire",
    }
}

/// The CXF bytes an engine is given for the input, or the refusal recorded in `report`.
fn document(
    args: &IdArgs,
    form: IdForm,
    streams: &mut Streams<'_>,
    report: &mut Report,
) -> Option<Vec<u8>> {
    let authored = match form {
        IdForm::Cxf => {
            if args.namespace.is_some() {
                report.error(
                    codes::NAMESPACE_NEEDS_AUTHORED,
                    "--namespace names what the emitter writes for a text or wire file; a CXF \
                     document is read as it is",
                    None,
                );
                return None;
            }
            return read(args, oce_api::MAX_CXF_BYTES, streams, report);
        }
        IdForm::Text => Form::Text,
        IdForm::Wire => Form::Wire,
    };
    let bytes = read(args, input::limit(authored), streams, report)?;
    let loaded = Loaded {
        form: authored,
        bytes,
    };
    let parsed = input::parse(&loaded, report)?;
    let mut options = EmitOptions::new();
    if let Some(namespace) = &args.namespace {
        options = options.with_namespace(namespace);
    }
    let emitted = match emit(&parsed.sequence, &options) {
        Ok(bytes) => bytes,
        Err(EmitError::Invalid(mut findings)) => {
            if let Some(spans) = &parsed.spans {
                spans.apply(&parsed.sequence, &mut findings);
            }
            record(report, &findings, false);
            report.error(
                codes::NOT_LOADABLE,
                "the sequence has error findings; it was not emitted",
                None,
            );
            return None;
        }
        Err(EmitError::InvalidNamespace { namespace, problem }) => {
            report.set("stages", Value::Array(Vec::new()));
            report.error(
                codes::INVALID_NAMESPACE,
                format!("namespace `{namespace}` {problem}"),
                None,
            );
            return None;
        }
        Err(other) => {
            report.set("stages", Value::Array(vec![Value::from("author")]));
            report.error(
                codes::NOT_LOADABLE,
                format!("the sequence could not be emitted: {other}"),
                None,
            );
            return None;
        }
    };
    let mut findings = validate(&parsed.sequence);
    if let Some(spans) = &parsed.spans {
        spans.apply(&parsed.sequence, &mut findings);
    }
    record(report, &findings, false);
    if findings.iter().any(|f| f.severity == Severity::Error) {
        report.error(
            codes::NOT_LOADABLE,
            "the sequence has error findings; it has no id",
            None,
        );
        return None;
    }
    Some(emitted)
}

/// Read the input, stdin or a file, at most `bound` bytes; a refusal is recorded in `report`.
fn read(
    args: &IdArgs,
    bound: usize,
    streams: &mut Streams<'_>,
    report: &mut Report,
) -> Option<Vec<u8>> {
    let read = if input::is_stdin(&args.file) {
        input::read_bounded(&mut *streams.stdin, bound)
    } else {
        input::read_file(Path::new(&args.file), bound)
    };
    match read {
        Ok(bytes) => Some(bytes),
        Err((code, detail)) => {
            report.error(code, detail, None);
            None
        }
    }
}

/// Why a CXF document has no id: the code, the detail and the engine's diagnostic codes.
struct Unnamed {
    code: &'static str,
    detail: String,
    diagnostic_codes: Vec<&'static str>,
}

impl Unnamed {
    /// Record the refusal as one error about the input.
    fn record(&self, report: &mut Report) {
        let facts = Facts {
            diagnostic_codes: &self.diagnostic_codes,
            ..Facts::about("input")
        };
        report.error_at(self.code, self.detail.clone(), None, &facts);
    }
}

/// Load `cxf` in a fresh engine and name the loaded model.
fn content_id(cxf: &[u8]) -> Result<String, Unnamed> {
    let mut engine = Engine::new();
    if let Err(error) = engine.load_cxf(cxf) {
        return Err(Unnamed {
            code: codes::NOT_LOADABLE,
            detail: format!("the engine refused to load the document: {error}"),
            diagnostic_codes: error.all_diagnostics().map(|d| d.code.as_str()).collect(),
        });
    }
    match engine.content_id() {
        Ok(id) => Ok(id.to_string()),
        Err(error) => Err(Unnamed {
            code: codes::NO_CONTENT_ID,
            detail: format!("the loaded model has no content id: {error}"),
            diagnostic_codes: Vec::new(),
        }),
    }
}

/// A run that produced no id: exit `2`, the report on stdout.
fn refused(mut report: Report) -> Outcome {
    report.verdict("refused: no content id");
    Outcome::judged(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(file: &str, from: Option<IdForm>) -> IdArgs {
        IdArgs {
            file: file.into(),
            from,
            namespace: None,
            json: false,
        }
    }

    #[test]
    fn the_form_comes_from_the_flag_then_the_name_cxf_before_wire() {
        let form = |file: &str, flag| form_of(&args(file, flag)).map_err(|e| e.0);
        assert_eq!(form("rule.cxf.jsonld", None), Ok(IdForm::Cxf));
        assert_eq!(form("rule.jsonld", None), Ok(IdForm::Cxf));
        assert_eq!(form("rule.cxf.json", None), Ok(IdForm::Cxf));
        assert_eq!(form("dir/rule.cxf.json", None), Ok(IdForm::Cxf));
        assert_eq!(form("rule.json", None), Ok(IdForm::Wire));
        assert_eq!(form("rule.oce", None), Ok(IdForm::Text));
        assert_eq!(form("rule.json", Some(IdForm::Cxf)), Ok(IdForm::Cxf));
        assert_eq!(form("rule.cxf.json", Some(IdForm::Wire)), Ok(IdForm::Wire));
        assert_eq!(form("rule.txt", Some(IdForm::Text)), Ok(IdForm::Text));
        for file in [
            "rule",
            "rule.txt",
            "rule.OCE",
            "rule.JSONLD",
            ".oce",
            "dir/",
        ] {
            assert_eq!(form(file, None), Err(codes::UNKNOWN_FORM), "{file}");
        }
    }

    #[test]
    fn stdin_has_no_name_to_read_a_form_from() {
        assert_eq!(
            form_of(&args("-", None)).map_err(|e| e.0),
            Err(codes::MISSING_FROM)
        );
        for from in [IdForm::Cxf, IdForm::Text, IdForm::Wire] {
            assert_eq!(form_of(&args("-", Some(from))), Ok(from));
        }
    }

    #[test]
    fn a_document_the_engine_refuses_carries_the_engines_codes() {
        let refusal = content_id(b"{}").expect_err("an empty object is no CXF document");
        assert_eq!(refusal.code, codes::NOT_LOADABLE);
        assert!(refusal.detail.starts_with("the engine refused to load"));
        let junk = content_id(b"\xff\x00").expect_err("junk is no CXF document");
        assert_eq!(junk.code, codes::NOT_LOADABLE);
    }
}
