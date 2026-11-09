//! `oce vocab`: the text-form vocabulary and every code the tool reports.
//!
//! The keywords (`oce_author::TEXT_FORM_KEYWORDS`), types, boundary tags and base-unit symbols
//! (`oce_author::BASE_UNIT_SYMBOLS`); the parse codes (`oce_author::PARSE_CODES`), the stage-1
//! finding codes (`oce_author::STAGE_ONE_CODES`), the stage-2 codes, the wire codes and the tool's
//! own error codes, each with one line of meaning; and the exit statuses.
//!
//! `enumerations` is the hook for the facade's enumeration and constant-package table (issue #468,
//! GitHub #488): `null` until `oce_api::enumerations()` exists on `development`, then each row's
//! class path, kind and members.

use serde_json::{Value, json};

use super::Outcome;
use crate::codes::{
    EXIT_MEANINGS, PARSE_MEANINGS, STAGE_ONE_MEANINGS, STAGE_TWO_MEANINGS, TEXT_FORM_TAGS,
    TOOL_CODES, WIRE_CODES,
};
use crate::report::Report;

/// The text-form value types.
const TYPES: &[&str] = &["Real", "Integer", "Boolean"];

/// Run `vocab`.
pub(crate) fn run(json: bool) -> Outcome {
    let mut report = Report::new("vocab", None);
    report.set("keywords", strings(oce_author::TEXT_FORM_KEYWORDS));
    report.set("types", strings(TYPES));
    report.set("tags", strings(TEXT_FORM_TAGS));
    report.set("base_units", strings(oce_author::BASE_UNIT_SYMBOLS));
    report.set("parse_codes", table(PARSE_MEANINGS));
    report.set("finding_codes", finding_codes());
    report.set("wire_codes", table(WIRE_CODES));
    report.set("error_codes", table(TOOL_CODES));
    report.set(
        "exit_codes",
        Value::Array(
            EXIT_MEANINGS
                .iter()
                .map(|(code, meaning)| json!({ "code": code, "meaning": meaning }))
                .collect(),
        ),
    );
    report.set("enumerations", Value::Null);
    report.verdict("ok");
    if json {
        return Outcome::judged(report);
    }
    let text = render_text();
    Outcome::document(report, text.into_bytes())
}

fn strings(items: &[&str]) -> Value {
    Value::Array(items.iter().map(|s| Value::from(*s)).collect())
}

fn table(rows: &[(&str, &str)]) -> Value {
    Value::Array(
        rows.iter()
            .map(|(code, meaning)| json!({ "code": code, "meaning": meaning }))
            .collect(),
    )
}

/// Stage-1 codes, then the stage-2 codes the validator adds.
fn finding_codes() -> Value {
    let one = STAGE_ONE_MEANINGS
        .iter()
        .map(|(code, meaning)| json!({ "code": code, "meaning": meaning, "stage": 1 }));
    let two = STAGE_TWO_MEANINGS
        .iter()
        .map(|(code, meaning)| json!({ "code": code, "meaning": meaning, "stage": 2 }));
    Value::Array(one.chain(two).collect())
}

/// The text rendering: one section per table.
fn render_text() -> String {
    let mut out = String::new();
    let list = |out: &mut String, title: &str, items: &[&str]| {
        out.push_str(title);
        out.push_str(":\n  ");
        out.push_str(&items.join(" "));
        out.push_str("\n\n");
    };
    list(&mut out, "keywords", oce_author::TEXT_FORM_KEYWORDS);
    list(&mut out, "types", TYPES);
    list(&mut out, "tags", TEXT_FORM_TAGS);
    list(&mut out, "base units", oce_author::BASE_UNIT_SYMBOLS);
    let rows = |out: &mut String, title: &str, rows: &[(&str, &str)]| {
        out.push_str(title);
        out.push_str(":\n");
        let width = rows.iter().map(|(code, _)| code.len()).max().unwrap_or(0);
        for (code, meaning) in rows {
            out.push_str(&format!("  {code:<width$}  {meaning}\n"));
        }
        out.push('\n');
    };
    rows(&mut out, "parse codes (text form, exit 2)", PARSE_MEANINGS);
    rows(
        &mut out,
        "finding codes, stage 1 (exit 1)",
        STAGE_ONE_MEANINGS,
    );
    rows(
        &mut out,
        "finding codes, stage 2 (exit 1; engine codes pass through as well)",
        STAGE_TWO_MEANINGS,
    );
    rows(&mut out, "wire codes (exit 2)", WIRE_CODES);
    rows(&mut out, "error codes (exit 2)", TOOL_CODES);
    out.push_str("exit codes:\n");
    for (code, meaning) in EXIT_MEANINGS {
        out.push_str(&format!("  {code}  {meaning}\n"));
    }
    out.push_str(
        "\nenumerations: not yet published by the facade (issue #468); see the crate README\n",
    );
    out
}
