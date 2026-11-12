//! `oce vocab`: the text-form vocabulary and every code the tool reports.
//!
//! The keywords (`oce_author::TEXT_FORM_KEYWORDS`), types, boundary tags and base-unit symbols
//! (`oce_author::BASE_UNIT_SYMBOLS`); the parse codes (`oce_author::PARSE_CODES`), the stage-1
//! finding codes (`oce_author::STAGE_ONE_CODES`), the stage-2 codes, the wire codes, the tool's
//! own error codes, and the vector file's and the verifier's codes `verify` passes through
//! (`oce_verify::VECTOR_ERROR_CODES`, `oce_verify::VERIFY_ERROR_CODES`), each with one line of
//! meaning; and the exit statuses.
//!
//! `scale_factors` is the vocabulary's unit scale-factor table (`oce_vocab::SCALE_FACTORS`,
//! vocabulary note decision 14, GitHub issue #500), in table order: the symbols that are an
//! exact integer multiple of a base unit, time only (`min`, `h`, `d`). It states the table; the
//! text form reads it (issue #501): one of these symbols alone as a unit suffix scales the number
//! exactly, on a parameter declared in the base unit. The output says how where an agent reads
//! it: in the text heading, and in the meanings of `unit-not-base` under `parse_codes` and of
//! `unit-suffix-mismatch` under the stage-1 codes, which are in both the text and the JSON (the
//! table's rows have no place for prose).
//!
//! `enumerations` is the facade's enumeration and Integer constant-package table
//! (`oce_api::enumerations()`, GitHub #488), in table order: each row's authoring class path, kind
//! (`enumeration` or `constant-package`) and members with the value each stands for (an
//! enumeration's 1-based ordinal, a package's Integer).

use oce_api::{EnumerationKind, MemberValue, enumerations};
use oce_vocab::SCALE_FACTORS;
use serde_json::{Value, json};

use super::Outcome;
use crate::codes::{
    EXIT_MEANINGS, PARSE_MEANINGS, STAGE_ONE_MEANINGS, STAGE_TWO_MEANINGS, TEXT_FORM_TAGS,
    TOOL_CODES, WIRE_CODES,
};
use crate::report::Report;
use crate::verify_codes::{VECTOR_MEANINGS, VERIFY_MEANINGS};

/// The text-form value types.
const TYPES: &[&str] = &["Real", "Integer", "Boolean"];

/// Run `vocab`.
pub(crate) fn run(json: bool) -> Outcome {
    let mut report = Report::new("vocab", None);
    report.set("keywords", strings(oce_author::TEXT_FORM_KEYWORDS));
    report.set("types", strings(TYPES));
    report.set("tags", strings(TEXT_FORM_TAGS));
    report.set("base_units", strings(oce_author::BASE_UNIT_SYMBOLS));
    report.set("scale_factors", scale_factors_json());
    report.set("parse_codes", table(PARSE_MEANINGS));
    report.set("finding_codes", finding_codes());
    report.set("wire_codes", table(WIRE_CODES));
    report.set("error_codes", table(TOOL_CODES));
    report.set("vector_codes", table(VECTOR_MEANINGS));
    report.set("verify_codes", table(VERIFY_MEANINGS));
    report.set(
        "exit_codes",
        Value::Array(
            EXIT_MEANINGS
                .iter()
                .map(|(code, meaning)| json!({ "code": code, "meaning": meaning }))
                .collect(),
        ),
    );
    report.set("enumerations", enumerations_json());
    report.verdict("ok");
    if json {
        return Outcome::judged(report);
    }
    let text = render_text();
    Outcome::document(report, text.into_bytes())
}

/// The scale-factor table as JSON rows: one `symbol` is exactly `factor` of `base`.
fn scale_factors_json() -> Value {
    Value::Array(
        SCALE_FACTORS
            .iter()
            .map(|row| json!({ "base": row.base, "factor": row.factor, "symbol": row.symbol }))
            .collect(),
    )
}

/// The enumeration table as JSON rows.
fn enumerations_json() -> Value {
    Value::Array(
        enumerations()
            .iter()
            .map(|row| {
                let members: Vec<Value> = row
                    .members
                    .iter()
                    .map(|member| {
                        let value = match row.member_value(member) {
                            Some(MemberValue::Ordinal(v)) => Value::from(v),
                            Some(MemberValue::Integer(v)) => Value::from(v),
                            None => Value::Null,
                        };
                        json!({ "name": member, "value": value })
                    })
                    .collect();
                json!({
                    "class_path": row.class_path,
                    "kind": kind_name(row.kind),
                    "members": members,
                })
            })
            .collect(),
    )
}

fn kind_name(kind: EnumerationKind) -> &'static str {
    match kind {
        EnumerationKind::Enumeration => "enumeration",
        EnumerationKind::ConstantPackage => "constant-package",
    }
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
    // How the text form uses the table (issue #501).
    out.push_str(
        "scale factors (time only; one symbol is exactly this many base units; alone as a unit \
         suffix on a parameter declared in the base unit, it scales the number, rounding once):\n",
    );
    let width = SCALE_FACTORS
        .iter()
        .map(|row| row.symbol.len())
        .max()
        .unwrap_or(0);
    for row in SCALE_FACTORS {
        out.push_str(&format!(
            "  {:<width$}  {} {}\n",
            row.symbol, row.factor, row.base
        ));
    }
    out.push('\n');
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
    rows(
        &mut out,
        "vector file codes (verify, exit 2)",
        VECTOR_MEANINGS,
    );
    rows(
        &mut out,
        "bind and run codes (verify, exit 2)",
        VERIFY_MEANINGS,
    );
    out.push_str("exit codes:\n");
    for (code, meaning) in EXIT_MEANINGS {
        out.push_str(&format!("  {code}  {meaning}\n"));
    }
    out.push_str(
        "\nenumerations (Type.Member values; a constant package's members are Integers):\n",
    );
    for row in enumerations() {
        let members: Vec<String> = row
            .members
            .iter()
            .map(|member| match row.member_value(member) {
                Some(MemberValue::Ordinal(v)) => format!("{member}={v}"),
                Some(MemberValue::Integer(v)) => format!("{member}={v}"),
                None => (*member).to_owned(),
            })
            .collect();
        out.push_str(&format!(
            "  {} ({}): {}\n",
            row.class_path,
            kind_name(row.kind),
            members.join(" ")
        ));
    }
    out
}
