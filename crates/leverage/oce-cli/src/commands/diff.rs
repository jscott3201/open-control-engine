//! `oce diff`: the structural diff of two authoring models or two CXF documents (diff note
//! `docs/design/diff.md`, "CLI", decisions 1, 10 and 16; GitHub issue #668), over `oce-diff`.
//!
//! Stdout is one document. By default (`--format json`) it is the `oce-report/1` report with the
//! diff under `diff`, exactly `oce_diff::Diff::to_json`'s data; with `--format markdown` it is
//! `oce_diff::render_markdown`'s table. Exit `0` when the diff is `equal`, `1` for any difference
//! (a `reordered`-only or metadata-only diff included: scripts that want "same structure" read
//! `summary.graph_equal`), with the diff on stdout either way. Every refusal is exit `2` with the
//! report and no diff. The human-readable rendering on stderr (`--render`) is
//! `oce_diff::render_text`, each line made safe for a terminal; a refusal is rendered as every
//! other verb renders its report, a side's refusal at that side's file.
//!
//! Order, the first refusal winning:
//!
//! 1. `-` on both sides is `stdin-twice`: stdin is read once;
//! 2. the form of both sides: `--from` when given; else each side's own, base first: stdin is
//!    `missing-from`, and a name is read by `id`'s rule (`id::form_by_name`: CXF for a name
//!    ending in `.jsonld` or `.cxf.json`, text for `.oce`, wire for any other `.json`; owner
//!    ruling of 2026-11-21), else `unknown-form`; two forms are `mixed-forms`;
//! 3. a bounded read of the base, then of the head, each at the form's bound (an authored side at
//!    `oce_author::MAX_TEXT_BYTES` or `MAX_WIRE_BYTES`, a CXF side at `oce_api::MAX_CXF_BYTES`),
//!    a file's size from its metadata before a byte is read;
//! 4. an authored side is parsed, base then head, and **never validated** (decision 10: an
//!    agent's broken draft is diffable); a parse refusal carries the form's own code and span.
//!    A CXF side is loaded into a fresh engine, base then head; a refusal is `not-loadable` with
//!    the engine's diagnostic codes. A document that loads with warnings is diffed;
//! 5. the diff: `oce_diff::diff` for two models; for two documents
//!    `oce_diff::diff_topology_with_ids` over the two topologies and canonical content ids
//!    (`diff_topology` if the facade names no id for a loaded document, which it is not expected
//!    to do; the ids are a hint that changes no result), whose refusals
//!    (`outside-root`, `unresolved-ports`) pass through under `oce_diff::DiffError::code`.
//!
//! Every refusal about one side carries `at`, `base` or `head`. The report also carries `head`
//! (the head's path as given; `input` is the base's) and `form` (`cxf`, `text`, `wire`, `null`
//! until both sides are known to be one form). No `stages`: nothing is validated. The verb writes
//! no file.

use oce_api::{ContentId, Engine, Topology};
use oce_author::{Sequence, from_wire_bytes, parse_text_bytes};
use oce_diff::{
    Diff, DiffError, DiffOptions, Side, diff, diff_topology, diff_topology_with_ids,
    render_markdown, render_text,
};
use serde_json::Value;

use super::Outcome;
use super::id::{form_by_name, form_name};
use crate::report::{Facts, Report, printable_lines};
use crate::{DiffArgs, DiffFormat, Exit, Form, IdForm, Streams, codes, input};

/// The `at` of a refusal about the base side.
const BASE: &str = "base";
/// The `at` of a refusal about the head side.
const HEAD: &str = "head";

/// A refusal: its code, a path-free detail, and the side it is about, if one.
struct Refusal {
    code: &'static str,
    detail: String,
    at: Option<&'static str>,
}

impl Refusal {
    fn record(self, report: &mut Report) {
        match self.at {
            Some(at) => report.error_at(self.code, self.detail, None, &Facts::about(at)),
            None => report.error(self.code, self.detail, None),
        }
    }
}

/// Run `diff`.
pub(crate) fn run(args: &DiffArgs, streams: &mut Streams<'_>) -> Outcome {
    let mut report = Report::new("diff", Some(input::display_name(&args.base)));
    report.set("head", Value::from(input::display_name(&args.head)));
    report.set("form", Value::Null);
    report.place(BASE, input::display_name(&args.base));
    report.place(HEAD, input::display_name(&args.head));
    let form = match form_of(args) {
        Ok(form) => form,
        Err(refusal) => {
            refusal.record(&mut report);
            return refused(report, "the two sides have no one form");
        }
    };
    report.set("form", Value::from(form_name(form)));
    let bound = match form {
        IdForm::Cxf => oce_api::MAX_CXF_BYTES,
        IdForm::Text => input::limit(Form::Text),
        IdForm::Wire => input::limit(Form::Wire),
    };
    let Some(base) = read(&args.base, BASE, bound, streams, &mut report) else {
        return refused(report, "the base could not be read");
    };
    let Some(head) = read(&args.head, HEAD, bound, streams, &mut report) else {
        return refused(report, "the head could not be read");
    };
    let mut options = DiffOptions::new();
    options.renames = !args.no_renames;
    let made = match form {
        IdForm::Cxf => documents(&base, &head, &options, &mut report),
        IdForm::Text => models(Form::Text, &base, &head, &options, &mut report),
        IdForm::Wire => models(Form::Wire, &base, &head, &options, &mut report),
    };
    match made {
        Ok(diff) => answer(args.format, &diff, report),
        Err(why) => refused(report, why),
    }
}

/// The one form of both sides `args` names.
fn form_of(args: &DiffArgs) -> Result<IdForm, Refusal> {
    if input::is_stdin(&args.base) && input::is_stdin(&args.head) {
        return Err(Refusal {
            code: codes::STDIN_TWICE,
            detail: "stdin (-) is read once; give at most one side as -".to_owned(),
            at: None,
        });
    }
    if let Some(form) = args.from {
        return Ok(form);
    }
    let base = side_form(&args.base, BASE)?;
    let head = side_form(&args.head, HEAD)?;
    if base != head {
        return Err(Refusal {
            code: codes::MIXED_FORMS,
            detail: format!(
                "the base reads as {} and the head as {}; both sides must be one form (pass \
                 --from to read both in one)",
                form_name(base),
                form_name(head)
            ),
            at: None,
        });
    }
    Ok(base)
}

/// The form one side's name gives.
fn side_form(file: &std::ffi::OsStr, side: &'static str) -> Result<IdForm, Refusal> {
    if input::is_stdin(file) {
        return Err(Refusal {
            code: codes::MISSING_FROM,
            detail: "reading stdin needs --from cxf, --from text or --from wire".to_owned(),
            at: Some(side),
        });
    }
    form_by_name(file).ok_or_else(|| Refusal {
        code: codes::UNKNOWN_FORM,
        detail: "the file name ends in none of .jsonld, .cxf.json (CXF), .oce (text) and .json \
                 (wire); pass --from cxf, text or wire"
            .to_owned(),
        at: Some(side),
    })
}

/// Read one side, stdin or a file, at most `bound` bytes; a refusal is recorded in `report`.
fn read(
    file: &std::ffi::OsStr,
    side: &'static str,
    bound: usize,
    streams: &mut Streams<'_>,
    report: &mut Report,
) -> Option<Vec<u8>> {
    match input::read_named(file, bound, streams.stdin) {
        Ok(bytes) => Some(bytes),
        Err((code, detail)) => {
            Refusal {
                code,
                detail,
                at: Some(side),
            }
            .record(report);
            None
        }
    }
}

/// The diff of two authoring models, each parsed and never validated; or why there is none, its
/// refusal recorded in `report`.
fn models(
    form: Form,
    base: &[u8],
    head: &[u8],
    options: &DiffOptions,
    report: &mut Report,
) -> Result<Diff, &'static str> {
    let base = parse(form, base, BASE, report).ok_or("the base could not be parsed")?;
    let head = parse(form, head, HEAD, report).ok_or("the head could not be parsed")?;
    Ok(diff(&base, &head, options))
}

/// Parse one authored side, or record the form's own refusal (code, detail, span) about it.
fn parse(form: Form, bytes: &[u8], side: &'static str, report: &mut Report) -> Option<Sequence> {
    let facts = Facts::about(side);
    match form {
        Form::Text => match parse_text_bytes(bytes) {
            Ok(sequence) => Some(sequence),
            Err(error) => {
                report.error_at(error.code, error.detail, Some(error.location), &facts);
                None
            }
        },
        Form::Wire => match from_wire_bytes(bytes) {
            Ok(sequence) => Some(sequence),
            Err(error) => {
                report.error_at(error.code(), error.to_string(), error.location(), &facts);
                None
            }
        },
    }
}

/// One CXF side, loaded: its topology view and canonical content id.
struct Loaded {
    topology: Topology,
    id: Option<ContentId>,
}

/// The diff of two CXF documents, each loaded into its own throwaway engine; or why there is
/// none, its refusal recorded in `report`.
fn documents(
    base: &[u8],
    head: &[u8],
    options: &DiffOptions,
    report: &mut Report,
) -> Result<Diff, &'static str> {
    let base = load(base, BASE, report).ok_or("the base could not be loaded")?;
    let head = load(head, HEAD, report).ok_or("the head could not be loaded")?;
    // The ids are a hint that never changes the result (diff note, decision 8, "As landed").
    let made = match (&base.id, &head.id) {
        (Some(base_id), Some(head_id)) => {
            diff_topology_with_ids(&base.topology, base_id, &head.topology, head_id, options)
        }
        _ => diff_topology(&base.topology, &head.topology, options),
    };
    made.map_err(|error| {
        record_diff_error(&error, report);
        "the two documents could not be compared"
    })
}

/// Load one CXF side in a fresh engine, or record the engine's refusal about it.
fn load(bytes: &[u8], side: &'static str, report: &mut Report) -> Option<Loaded> {
    let mut engine = Engine::new();
    if let Err(error) = engine.load_cxf(bytes) {
        let diagnostic_codes: Vec<&str> =
            error.all_diagnostics().map(|d| d.code.as_str()).collect();
        let facts = Facts {
            diagnostic_codes: &diagnostic_codes,
            ..Facts::about(side)
        };
        report.error_at(
            codes::NOT_LOADABLE,
            format!("the engine refused to load the {side}: {error}"),
            None,
            &facts,
        );
        return None;
    }
    Some(Loaded {
        topology: engine.topology(),
        id: engine.content_id().ok(),
    })
}

/// Record a refusal of `oce-diff` under its own code, about the side it names. The tool reads the
/// side from the variants it knows and matches nothing else: a refusal `oce-diff` adds is still
/// reported under its own code and text.
fn record_diff_error(error: &DiffError, report: &mut Report) {
    let side = match error {
        DiffError::OutsideRoot { side, .. }
        | DiffError::UnresolvedPorts { side, .. }
        | DiffError::UnreadableBlob { side, .. }
        | DiffError::UnexpectedRefusal { side, .. } => side_name(*side),
        _ => None,
    };
    Refusal {
        code: error.code(),
        detail: error.to_string(),
        at: side,
    }
    .record(report);
}

/// A side as a refusal's `at` spells it.
fn side_name(side: Side) -> Option<&'static str> {
    match side {
        Side::Base => Some(BASE),
        Side::Head => Some(HEAD),
        _ => None,
    }
}

/// The exit status, stdout and stderr rendering of a diff that was made.
fn answer(format: DiffFormat, diff: &Diff, mut report: Report) -> Outcome {
    let exit = if diff.summary.equal {
        Exit::Yes
    } else {
        Exit::No
    };
    // Read back from the crate's canonical text, so the report holds exactly the diff's own JSON
    // whatever key order the JSON map keeps; that text is always one JSON value.
    let data = serde_json::from_str(&diff.to_json()).unwrap_or(Value::Null);
    report.set("diff", data);
    report.verdict(if diff.summary.equal {
        "equal"
    } else {
        "different"
    });
    let outcome = match format {
        DiffFormat::Json => Outcome::with_exit(exit, report),
        DiffFormat::Markdown => Outcome::verdict(exit, report, render_markdown(diff).into_bytes()),
    };
    outcome.rendered_as(printable_lines(&render_text(diff)))
}

/// A run that made no diff: exit `2`, the report on stdout.
fn refused(mut report: Report, why: &str) -> Outcome {
    report.verdict(format!("refused: {why}; no diff"));
    Outcome::judged(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(base: &str, head: &str, from: Option<IdForm>) -> DiffArgs {
        DiffArgs {
            base: base.into(),
            head: head.into(),
            from,
            no_renames: false,
            format: DiffFormat::Json,
        }
    }

    fn form(
        base: &str,
        head: &str,
        from: Option<IdForm>,
    ) -> Result<IdForm, (&'static str, Option<&'static str>)> {
        form_of(&args(base, head, from)).map_err(|refusal| (refusal.code, refusal.at))
    }

    #[test]
    fn both_sides_are_read_in_one_form_by_name_cxf_before_wire() {
        assert_eq!(form("a.oce", "b.oce", None), Ok(IdForm::Text));
        assert_eq!(form("a.json", "b.json", None), Ok(IdForm::Wire));
        assert_eq!(form("a.jsonld", "b.cxf.json", None), Ok(IdForm::Cxf));
        assert_eq!(form("rule.cxf.json", "dir/b.jsonld", None), Ok(IdForm::Cxf));
        assert_eq!(form("a.oce", "-", Some(IdForm::Text)), Ok(IdForm::Text));
        assert_eq!(form("-", "b.json", Some(IdForm::Cxf)), Ok(IdForm::Cxf));
        // --from reads both sides in its form whatever their names say.
        assert_eq!(
            form("a.oce", "b.jsonld", Some(IdForm::Wire)),
            Ok(IdForm::Wire)
        );
    }

    #[test]
    fn two_forms_are_mixed_and_a_side_without_one_is_named() {
        for (base, head) in [
            ("a.oce", "b.json"),
            ("a.oce", "b.jsonld"),
            ("a.json", "b.cxf.json"),
            ("a.cxf.json", "b.json"),
        ] {
            assert_eq!(
                form(base, head, None),
                Err((codes::MIXED_FORMS, None)),
                "{base} {head}"
            );
        }
        assert_eq!(
            form("a.txt", "b.oce", None),
            Err((codes::UNKNOWN_FORM, Some(BASE)))
        );
        assert_eq!(
            form("a.oce", "b.OCE", None),
            Err((codes::UNKNOWN_FORM, Some(HEAD)))
        );
        // The base is judged first.
        assert_eq!(form("a", "b", None), Err((codes::UNKNOWN_FORM, Some(BASE))));
        assert_eq!(
            form("-", "b.oce", None),
            Err((codes::MISSING_FROM, Some(BASE)))
        );
        assert_eq!(
            form("a.oce", "-", None),
            Err((codes::MISSING_FROM, Some(HEAD)))
        );
    }

    #[test]
    fn stdin_on_both_sides_is_refused_whatever_from_says() {
        for from in [
            None,
            Some(IdForm::Cxf),
            Some(IdForm::Text),
            Some(IdForm::Wire),
        ] {
            assert_eq!(form("-", "-", from), Err((codes::STDIN_TWICE, None)));
        }
    }

    #[test]
    fn a_diff_refusal_names_its_side() {
        let error = DiffError::UnresolvedPorts {
            side: Side::Head,
            block: "b".to_owned(),
            class: "C".to_owned(),
        };
        let mut report = Report::new("diff", None);
        record_diff_error(&error, &mut report);
        let json: Value = serde_json::from_str(&report.to_json(false)).expect("JSON");
        assert_eq!(json["errors"][0]["code"], "unresolved-ports");
        assert_eq!(json["errors"][0]["at"], "head");
        assert_eq!(json["errors"][0]["detail"], error.to_string());
    }
}
