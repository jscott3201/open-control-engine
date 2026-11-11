//! The subject of `oce verify`: its form, a bounded read, and, for an authored form, both
//! validation stages and the emitter (vectors note, "CLI": "Forms" and "Reads").
//!
//! The form is `--subject-form` when given; else CXF for a name ending in `.jsonld` or
//! `.cxf.json`, text for `.oce`, wire for any other `.json`; else `unknown-subject-form`. The
//! `.cxf.json` test comes first because `input::form_of`, which every other command uses, reads
//! every `.json` as the wire form; `verify` does not use it. A CXF subject is bounded at
//! `oce_api::MAX_CXF_BYTES` and goes to the engine untouched. A text or wire subject is bounded at
//! its form's own limit, as `emit` reads it, then validated and emitted with
//! `EmitOptions::default()`: an error finding, or an emitter refusal, is `subject-load-failed`
//! (exit 2), with the findings in the report. There is no `--namespace`: the namespace is inside
//! every path the receipt prints, and a caller who wants another runs `oce emit` first.

use std::ffi::OsStr;
use std::path::Path;

use oce_author::{EmitError, EmitOptions, Severity, emit, validate};
use serde_json::Value;

use super::validate::record;
use crate::input::{self, Loaded};
use crate::report::{Facts, Report};
use crate::{Form, SubjectForm, VerifyArgs, codes};

/// The code `oce-verify` gives a subject the engine refuses; a refused authored subject gets it
/// too.
const SUBJECT_LOAD_FAILED: &str = "subject-load-failed";

/// The form of the subject `args` names.
pub(crate) fn form_of(args: &VerifyArgs) -> Result<SubjectForm, (&'static str, String)> {
    if let Some(form) = args.subject_form {
        return Ok(form);
    }
    let path = Path::new(&args.subject);
    let name = path
        .file_name()
        .map(OsStr::to_string_lossy)
        .unwrap_or_default();
    if name.ends_with(".jsonld") || name.ends_with(".cxf.json") {
        return Ok(SubjectForm::Cxf);
    }
    match path.extension().and_then(OsStr::to_str) {
        Some("oce") => Ok(SubjectForm::Text),
        Some("json") => Ok(SubjectForm::Wire),
        _ => Err((
            codes::UNKNOWN_SUBJECT_FORM,
            "the subject's name ends in none of .jsonld, .cxf.json (CXF), .oce (text) and .json \
             (wire); pass --subject-form cxf, text or wire"
                .to_owned(),
        )),
    }
}

/// The CXF bytes the engine is given for the subject, or the refusal recorded in `report`.
pub(crate) fn load(args: &VerifyArgs, report: &mut Report) -> Option<Vec<u8>> {
    let refuse = |report: &mut Report, (code, detail): (&'static str, String)| {
        report.error_at(code, detail, None, &Facts::about("subject"));
    };
    let form = match form_of(args) {
        Ok(form) => form,
        Err(refusal) => {
            refuse(report, refusal);
            return None;
        }
    };
    let path = Path::new(&args.subject);
    let authored = match form {
        SubjectForm::Cxf => {
            return match input::read_file(path, oce_api::MAX_CXF_BYTES) {
                Ok(bytes) => Some(bytes),
                Err(refusal) => {
                    refuse(report, refusal);
                    None
                }
            };
        }
        SubjectForm::Text => Form::Text,
        SubjectForm::Wire => Form::Wire,
    };
    let bytes = match input::read_file(path, input::limit(authored)) {
        Ok(bytes) => bytes,
        Err(refusal) => {
            refuse(report, refusal);
            return None;
        }
    };
    let loaded = Loaded {
        form: authored,
        bytes,
    };
    let parsed = input::parse(&loaded, report)?;
    let emitted = match emit(&parsed.sequence, &EmitOptions::default()) {
        Ok(bytes) => bytes,
        Err(EmitError::Invalid(mut findings)) => {
            if let Some(spans) = &parsed.spans {
                spans.apply(&parsed.sequence, &mut findings);
            }
            record(report, &findings, false);
            refuse(
                report,
                (
                    SUBJECT_LOAD_FAILED,
                    "the subject has error findings; it was not emitted".to_owned(),
                ),
            );
            return None;
        }
        Err(other) => {
            report.set("stages", Value::Array(vec![Value::from("author")]));
            refuse(
                report,
                (
                    SUBJECT_LOAD_FAILED,
                    format!("the subject could not be emitted: {other}"),
                ),
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
        refuse(
            report,
            (
                SUBJECT_LOAD_FAILED,
                "the subject has error findings; it was not run".to_owned(),
            ),
        );
        return None;
    }
    Some(emitted)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(subject: &str, form: Option<SubjectForm>) -> VerifyArgs {
        VerifyArgs {
            vectors: "v.json".into(),
            subject: subject.into(),
            subject_form: form,
            out: None,
            require_mutants_caught: false,
            deny_warnings: false,
            timings: false,
        }
    }

    #[test]
    fn the_form_comes_from_the_flag_then_the_name_cxf_before_wire() {
        let form = |subject: &str, flag| form_of(&args(subject, flag)).map_err(|e| e.0);
        assert_eq!(form("rule.cxf.jsonld", None), Ok(SubjectForm::Cxf));
        assert_eq!(form("rule.jsonld", None), Ok(SubjectForm::Cxf));
        assert_eq!(form("rule.cxf.json", None), Ok(SubjectForm::Cxf));
        assert_eq!(form("dir/rule.cxf.json", None), Ok(SubjectForm::Cxf));
        assert_eq!(form("rule.json", None), Ok(SubjectForm::Wire));
        assert_eq!(form("rule.oce", None), Ok(SubjectForm::Text));
        assert_eq!(
            form("rule.json", Some(SubjectForm::Cxf)),
            Ok(SubjectForm::Cxf)
        );
        assert_eq!(
            form("rule.cxf.json", Some(SubjectForm::Wire)),
            Ok(SubjectForm::Wire)
        );
        for subject in ["rule", "rule.txt", "rule.OCE", "rule.JSON", ".oce", "dir/"] {
            assert_eq!(
                form(subject, None),
                Err(codes::UNKNOWN_SUBJECT_FORM),
                "{subject}"
            );
        }
    }
}
