//! `oce import`: a flat CXF document into the authoring form (importer note
//! `docs/design/importer.md`; GitHub issue #514).
//!
//! Stdout is the imported sequence as canonical text (`oce_author::print_text`), or with `--wire`
//! as the canonical wire form (`oce_author::to_wire_string`), exit `0`. With `-o FILE` the
//! document is written atomically (`atomic_write`) and stdout is the `oce-report/1` report.
//! Anything that leaves the run without a document is exit `2` with the report. The verb never
//! answers `1`: it reads and writes, and judges nothing. In particular it does not validate, so
//! the sequence it writes may still carry what `oce validate` reports (a reserved word as a name,
//! a class the catalog lacks): the importer carries such things by design, for the validator to
//! name in the author's terms.
//!
//! Order, the first refusal winning:
//!
//! 1. the output must not be the input (`output-is-input`): the same file by
//!    `file_identity::same_file`, so another spelling of its path, a symbolic link to it and, on
//!    Unix, a hard link to it are all refused, and nothing is replaced;
//! 2. the output's name, which must not contradict the form written (`output-form-mismatch`).
//!    The form is `--wire` or text and is never chosen by the name; but a name that `oce` would
//!    read as another form is refused, so a file this verb writes is never one the other verbs
//!    misread. The name is read by `id`'s own rule (`id::form_by_name`): `.oce` is text, `.json`
//!    is wire, and `.jsonld` and `.cxf.json` are CXF, which this verb never writes. A name that
//!    rule reads no form from is accepted for either form;
//! 3. a bounded read, stdin (`-`) or a file, at `oce_api::MAX_CXF_BYTES`, a file's size from its
//!    metadata before a byte is read. The input has one form, so there is no `--from` and no
//!    name rule;
//! 4. `oce_author::import_cxf`. A refusal is reported under the importer's own stable code
//!    (`ImportError::code`, one of `oce_author::IMPORT_ERROR_CODES`) with its subject as the
//!    error's `at`. No kind is matched here, so a kind `oce-author` adds is reported under its
//!    own code;
//! 5. the spelling: the output form's writer, then that form's own reader over what was written.
//!    The document is produced only when it reads back equal to the imported sequence, so exit
//!    `0` means `oce` reads what it wrote. Anything else is `unspellable`: a class path the text
//!    form cannot spell, a string past a wire bound, a wire document past the wire form's size
//!    limit;
//! 6. the write: stdout, or the atomic write of `-o` (`write-failed`).
//!
//! The report's `dropped` and `normalised` are the importer's lists (`oce_author::ImportReport`),
//! in reading order, written once the importer accepted the document. Its third list,
//! `truncated`, belongs to the v0 graph JSON importer, which this verb does not expose.

use std::path::Path;

use oce_author::{
    Dropped, ImportError, ImportOptions, Normalised, Sequence, from_wire_str, import_cxf,
    parse_text, print_text, to_wire_string,
};
use serde_json::{Value, json};

use super::id::{form_by_name, form_name};
use super::{Outcome, report_for, same_file, write_atomically};
use crate::report::{Facts, Report};
use crate::{Exit, IdForm, ImportArgs, Streams, codes, input};

/// Run `import`.
pub(crate) fn run(args: &ImportArgs, streams: &mut Streams<'_>) -> Outcome {
    let mut report = report_for("import", &args.file);
    let asked = if args.wire {
        IdForm::Wire
    } else {
        IdForm::Text
    };
    let form = form_name(asked);
    report.set("output_form", Value::from(form));
    let output = args.output.as_deref().filter(|path| !input::is_stdin(path));
    if let Some(path) = output
        && let Err((code, detail)) = output_accepted(&args.file, path, asked)
    {
        report.error(code, detail, None);
        report.verdict("refused: nothing read, nothing written");
        return Outcome::judged(report);
    }
    let bytes = match input::read_named(&args.file, oce_api::MAX_CXF_BYTES, streams.stdin) {
        Ok(bytes) => bytes,
        Err((code, detail)) => {
            report.error(code, detail, None);
            return refused(report);
        }
    };
    let imported = match import_cxf(&bytes, &ImportOptions::new()) {
        Ok(imported) => imported,
        Err(error) => {
            record(&mut report, &error);
            // The rendering prints an error's code and detail; the verdict names the place.
            if !error.subject.is_empty() {
                report.verdict(format!("refused at {}: nothing imported", error.subject));
                return Outcome::judged(report);
            }
            return refused(report);
        }
    };
    let (dropped, normalised) = (&imported.report.dropped, &imported.report.normalised);
    report.set("dropped", dropped.iter().map(dropped_json).collect());
    report.set(
        "normalised",
        normalised.iter().map(normalised_json).collect(),
    );
    let document = match spelled(&imported.sequence, args.wire) {
        Ok(document) => document,
        Err(detail) => {
            report.error(codes::UNSPELLABLE, detail, None);
            return refused(report);
        }
    };
    let carried = format!("{} dropped, {} normalised", dropped.len(), normalised.len());
    let Some(path) = output else {
        report.verdict(format!(
            "{} bytes of {form} on stdout; {carried}",
            document.len()
        ));
        return Outcome::document(report, document);
    };
    match write_atomically(Path::new(path), &document) {
        Ok(()) => {
            // `output` and `bytes` describe a finished write; a failed one carries neither.
            report.set("output", Value::from(input::display_name(path)));
            report.set("bytes", Value::from(document.len()));
            report.verdict(format!(
                "{} bytes of {form} written to {}; {carried}",
                document.len(),
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

/// Whether `-o output` may be written for a document read from `input` in the form `asked`: it
/// must not be the input file, and its name must not read as another form.
fn output_accepted(
    input: &std::ffi::OsStr,
    output: &std::ffi::OsStr,
    asked: IdForm,
) -> Result<(), (&'static str, String)> {
    if !input::is_stdin(input) && same_file(Path::new(input), Path::new(output)) {
        return Err((
            codes::OUTPUT_IS_INPUT,
            "the output is the input file; the CXF document would be replaced by its import"
                .to_owned(),
        ));
    }
    if let Some(named) = form_by_name(output)
        && named != asked
    {
        return Err((
            codes::OUTPUT_FORM_MISMATCH,
            format!(
                "the output is the {} form and its name reads as {}: a name ending in .oce is \
                 read as text, in .json as wire and in .jsonld or .cxf.json as CXF; name it for \
                 the form written (--wire chooses wire, and text otherwise)",
                form_name(asked),
                form_name(named),
            ),
        ));
    }
    Ok(())
}

/// Record the importer's refusal: its code, what the kind says and where it sits. A subject is
/// the error's `at`, which is never written empty.
fn record(report: &mut Report, error: &ImportError) {
    let (code, detail) = (error.code(), error.kind.to_string());
    if error.subject.is_empty() {
        report.error(code, detail, None);
    } else {
        report.error_at(code, detail, None, &Facts::about(&error.subject));
    }
}

/// One dropped property as the report spells it.
fn dropped_json(dropped: &Dropped) -> Value {
    json!({
        "property": dropped.property.as_str(),
        "subject": dropped.subject,
        "value": dropped.value,
    })
}

/// One normalisation as the report spells it.
fn normalised_json(normalised: &Normalised) -> Value {
    json!({
        "data_type": normalised.data_type,
        "subject": normalised.subject,
        "to": normalised.to.as_str(),
    })
}

/// The sequence in the output form, once that form's reader reads it back equal; else why the
/// form has no spelling for it.
fn spelled(sequence: &Sequence, wire: bool) -> Result<Vec<u8>, String> {
    let (written, read_back) = if wire {
        let written = to_wire_string(sequence);
        let read_back = from_wire_str(&written).map_err(|error| {
            format!(
                "the wire form cannot carry the imported sequence; its reader refuses what would \
                 be written ({}): {error}",
                error.code()
            )
        });
        (written, read_back)
    } else {
        let written = print_text(sequence).map_err(|error| {
            format!("the text form cannot carry the imported sequence: {error}")
        })?;
        let read_back = parse_text(&written).map_err(|error| {
            format!(
                "the text form cannot carry the imported sequence; its parser refuses what would \
                 be written ({}): {}",
                error.code, error.detail
            )
        });
        (written, read_back)
    };
    if read_back? == *sequence {
        Ok(written.into_bytes())
    } else {
        Err(format!(
            "the {} form does not read back as the imported sequence",
            if wire { "wire" } else { "text" }
        ))
    }
}

/// A run that produced no document: exit `2`, the report on stdout.
fn refused(mut report: Report) -> Outcome {
    report.verdict("refused: nothing imported");
    Outcome::judged(report)
}

#[cfg(test)]
mod tests {
    use oce_author::{ImportErrorKind, SourceForm};

    use super::*;

    const ECON: &[u8] =
        include_bytes!("../../../oce-author/tests/fixtures/emit/econ_enable.cxf.json");

    fn imported() -> Sequence {
        import_cxf(ECON, &ImportOptions::new())
            .expect("the emitter's golden imports")
            .sequence
    }

    #[test]
    fn both_forms_spell_an_imported_sequence_and_read_it_back() {
        let sequence = imported();
        let text = spelled(&sequence, false).expect("text");
        let wire = spelled(&sequence, true).expect("wire");
        assert_eq!(
            parse_text(std::str::from_utf8(&text).expect("UTF-8")).expect("parses"),
            sequence
        );
        assert_eq!(
            from_wire_str(std::str::from_utf8(&wire).expect("UTF-8")).expect("reads"),
            sequence
        );
    }

    #[test]
    fn a_refusal_is_recorded_under_the_importers_code_with_its_subject_as_at() {
        let error = import_cxf(b"{}", &ImportOptions::new()).expect_err("no graph");
        let mut report = Report::new("import", None);
        record(&mut report, &error);
        let json: Value = serde_json::from_str(&report.to_json(false)).expect("JSON");
        let recorded = &json["errors"][0];
        assert_eq!(recorded["code"], error.code());
        assert_eq!(recorded["at"], error.subject.as_str());
        assert_eq!(recorded["detail"], error.kind.to_string());
        assert!(oce_author::IMPORT_ERROR_CODES.contains(&error.code()));
    }

    #[test]
    fn an_accepted_output_name_is_one_every_verb_reads_in_the_form_written_or_not_at_all() {
        use std::ffi::OsStr;
        let names = [
            "a.oce",
            "a.json",
            "a.cxf.json",
            "a.jsonld",
            "a.cxf.jsonld",
            "a",
            "a.txt",
            "a.OCE",
            "a.JSON",
            "dir/a.oce",
            "a.oce.json",
            "a.json.oce",
            ".oce",
            ".json",
            "a.cxf.oce",
        ];
        let mut accepted = 0;
        for name in names {
            for asked in [IdForm::Text, IdForm::Wire] {
                let outcome = output_accepted(OsStr::new("-"), OsStr::new(name), asked);
                // `id` (and `verify`) by its three-form rule, the function the guard itself reads.
                let by_id = form_by_name(OsStr::new(name));
                // `emit`, `validate` and `fmt` by the authored rule.
                let by_extension = match std::path::Path::new(name)
                    .extension()
                    .and_then(OsStr::to_str)
                {
                    Some("oce") => Some(IdForm::Text),
                    Some("json") => Some(IdForm::Wire),
                    _ => None,
                };
                match outcome {
                    Ok(()) => {
                        assert!(by_id.is_none_or(|form| form == asked), "{name}");
                        assert!(by_extension.is_none_or(|form| form == asked), "{name}");
                        accepted += 1;
                    }
                    Err((code, _)) => {
                        assert_eq!(code, codes::OUTPUT_FORM_MISMATCH, "{name}");
                        assert!(by_id.is_some_and(|form| form != asked), "{name}");
                    }
                }
            }
        }
        // Text under `.oce`, wire under `.json`, and either under a name that gives no form.
        assert_eq!(accepted, 18);
    }

    #[test]
    fn an_empty_subject_writes_no_at() {
        let error = ImportError::new(SourceForm::Cxf, "", ImportErrorKind::DuplicateId);
        let mut report = Report::new("import", None);
        record(&mut report, &error);
        let json: Value = serde_json::from_str(&report.to_json(false)).expect("JSON");
        assert_eq!(json["errors"][0]["code"], "duplicate-id");
        assert!(json["errors"][0].get("at").is_none());
    }
}
