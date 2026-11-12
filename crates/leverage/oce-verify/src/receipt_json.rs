//! A receipt as canonical JSON (vectors note, "The receipt" and "Values"; decision 24).
//!
//! The writer is the vector file's ([`crate::canonical::Json`]): keys sorted by their UTF-8 bytes
//! at every depth, so the field order of the note's tables is a reading order, not a byte order.
//! Every field the tables mark "always" is written, absent optional ones are left out (a
//! scenario's `content_id` with no edits, a `diagnostic-missing` record's `message`, the body's
//! `mutants` when the set lists none, a mutant entry's `scenario` and `failure` unless it was
//! caught), and arrays keep the order each field states.
//!
//! Numbers: a count is a JSON integer; a time and a Real value are spelled by
//! [`value`], so a negative zero, a NaN or an infinity is never a JSON number that a reader could
//! round, re-sign or refuse.

use crate::canonical::Json;
use crate::number::canonical_real;
use crate::receipt::{
    Failure, FrameCounts, InstantCounts, MutantEntry, MutantSection, Receipt, ReceiptBody,
    ReceiptHeader, ReceiptValue, ScenarioCounts, ScenarioReceipt, Totals, UnmatchedWarning,
    VectorBody,
};
use crate::refusal_classes::engine_refusal_class;

fn entry(key: &str, value: Json) -> (String, Json) {
    (key.to_owned(), value)
}

fn count(value: u64) -> Json {
    Json::Raw(value.to_string())
}

/// A value in the note's "Values" spelling: `true`/`false`, a JSON integer, a finite Real other
/// than negative zero in the canonical real spelling, otherwise `"0x"` and 16 lowercase hex
/// digits of the bits.
pub(crate) fn value(value: ReceiptValue) -> Json {
    match value {
        ReceiptValue::Real(x) if x.is_finite() && !(x == 0.0 && x.is_sign_negative()) => {
            Json::Raw(canonical_real(x))
        }
        ReceiptValue::Real(x) => Json::Str(format!("0x{:016x}", x.to_bits())),
        ReceiptValue::Integer(x) => Json::Raw(x.to_string()),
        ReceiptValue::Boolean(x) => Json::Raw(x.to_string()),
    }
}

/// A time, spelled as a Real value.
fn time(t: f64) -> Json {
    value(ReceiptValue::Real(t))
}

/// The whole receipt; without `timings` and `receipt_id` when `identified` is false, which is
/// the document `receipt_id` hashes.
pub(crate) fn receipt(receipt: &Receipt, identified: bool) -> Json {
    let mut entries = header(&receipt.header, identified);
    entries.push(entry("body", body(&receipt.body)));
    Json::object(entries)
}

fn header(header: &ReceiptHeader, identified: bool) -> Vec<(String, Json)> {
    let mut entries = vec![
        entry("schema", Json::str(&header.schema)),
        entry("kind", Json::str(header.kind.as_str())),
        entry("content_id", Json::str(&header.content_id)),
        entry("engine_version", Json::str(&header.engine_version)),
        entry("spec_id", Json::str(&header.spec_id)),
        entry(
            "comparison",
            Json::object([entry("mode", Json::str(header.comparison.mode.as_str()))]),
        ),
        entry("passed", Json::Raw(header.passed.to_string())),
    ];
    if identified {
        entries.push(entry("receipt_id", Json::str(&header.receipt_id)));
        entries.push(entry(
            "timings",
            Json::object([
                entry("load_ms", count(header.timings.load_ms)),
                entry("run_ms", count(header.timings.run_ms)),
                entry("compare_ms", count(header.timings.compare_ms)),
            ]),
        ));
    }
    entries
}

fn body(body: &ReceiptBody) -> Json {
    match body {
        ReceiptBody::Vectors(vectors) => vector_body(vectors),
    }
}

fn vector_body(body: &VectorBody) -> Json {
    let mut entries = vec![
        entry("status", Json::str(body.status.as_str())),
        entry(
            "clock",
            Json::object([
                entry("step_s", Json::Raw(body.clock.step_written().canonical())),
                entry(
                    "horizon_s",
                    Json::Raw(body.clock.horizon_written().canonical()),
                ),
            ]),
        ),
        entry(
            "load_warnings",
            Json::List(
                body.load_warnings
                    .iter()
                    .map(|code| Json::str(code))
                    .collect(),
            ),
        ),
        entry("load_warnings_total", count(body.load_warnings_total)),
        entry("totals", totals(&body.totals)),
        entry(
            "scenarios",
            Json::List(body.scenarios.iter().map(scenario).collect()),
        ),
    ];
    if let Some(mutants) = &body.mutants {
        entries.push(entry("mutants", mutant_section(mutants)));
    }
    Json::object(entries)
}

fn mutant_section(section: &MutantSection) -> Json {
    Json::object([
        entry("status", Json::str(section.status.as_str())),
        entry("total", count(section.total)),
        entry("caught", count(section.caught)),
        entry("survived", count(section.survived)),
        entry(
            "entries",
            Json::List(section.entries.iter().map(mutant_entry).collect()),
        ),
    ])
}

fn mutant_entry(mutant: &MutantEntry) -> Json {
    let mut entries = vec![
        entry("path", Json::str(&mutant.path)),
        entry("digest", Json::str(&mutant.digest)),
        entry("content_id", Json::str(&mutant.content_id)),
        entry("status", Json::str(mutant.status.as_str())),
    ];
    if let Some(caught) = &mutant.caught_by {
        entries.push(entry("scenario", Json::str(&caught.scenario)));
        entries.push(entry("failure", failure(&caught.failure)));
    }
    Json::object(entries)
}

fn scenario_counts(counts: &ScenarioCounts) -> Json {
    Json::object([
        entry("total", count(counts.total)),
        entry("pass", count(counts.pass)),
        entry("fail", count(counts.fail)),
        entry("no_data", count(counts.no_data)),
    ])
}

fn frames(counts: &FrameCounts) -> Json {
    Json::object([
        entry("executed", count(counts.executed)),
        entry("substituted", count(counts.substituted)),
        entry("refused", count(counts.refused)),
    ])
}

fn instants(counts: &InstantCounts) -> Json {
    Json::object([
        entry("checked", count(counts.checked)),
        entry("passed", count(counts.passed)),
        entry("failed", count(counts.failed)),
        entry("masked", count(counts.masked)),
    ])
}

fn totals(totals: &Totals) -> Json {
    Json::object([
        entry("scenarios", scenario_counts(&totals.scenarios)),
        entry("frames", frames(&totals.frames)),
        entry("instants", instants(&totals.instants)),
        entry("failures", count(totals.failures)),
        entry("warnings", count(totals.warnings)),
    ])
}

fn scenario(scenario: &ScenarioReceipt) -> Json {
    let mut entries = vec![
        entry("name", Json::str(&scenario.name)),
        entry("status", Json::str(scenario.status.as_str())),
        entry(
            "params",
            Json::List(
                scenario
                    .params
                    .iter()
                    .map(|edit| {
                        Json::object([
                            entry("path", Json::str(&edit.path)),
                            entry("value", value(edit.value)),
                        ])
                    })
                    .collect(),
            ),
        ),
        entry("frames", frames(&scenario.frames)),
        entry("expectations", count(scenario.expectations)),
        entry("instants", instants(&scenario.instants)),
        entry(
            "failures",
            Json::List(scenario.failures.iter().map(failure).collect()),
        ),
        entry("failures_total", count(scenario.failures_total)),
        entry(
            "warnings",
            Json::List(scenario.warnings.iter().map(warning).collect()),
        ),
        entry("warnings_total", count(scenario.warnings_total)),
    ];
    if let Some(content_id) = &scenario.content_id {
        entries.push(entry("content_id", Json::str(content_id)));
    }
    Json::object(entries)
}

/// A failure record, shaped by its code (decision 23).
pub(crate) fn failure(failure: &Failure) -> Json {
    let mut entries = vec![
        entry("code", Json::str(failure.code())),
        entry("t", time(failure.t())),
    ];
    match failure {
        Failure::ValueMismatch {
            expectation,
            output,
            expected,
            actual,
            ..
        } => {
            entries.push(entry("expectation", count(*expectation)));
            entries.push(entry("output", Json::str(output)));
            entries.push(entry("expected", value(*expected)));
            entries.push(entry("actual", value(*actual)));
        }
        Failure::DiagnosticMissing {
            expectation,
            block,
            message,
            ..
        } => {
            entries.push(entry("expectation", count(*expectation)));
            entries.push(entry("block", Json::str(block)));
            if let Some(message) = message {
                entries.push(entry("message", Json::str(message)));
            }
        }
        Failure::FrameRefused { refusal, .. } => {
            entries.push(entry("refusal", Json::str(engine_refusal_class(*refusal))));
        }
    }
    Json::object(entries)
}

fn warning(warning: &UnmatchedWarning) -> Json {
    Json::object([
        entry("t", time(warning.t)),
        entry("block", Json::str(&warning.block)),
        entry("message", Json::str(&warning.message)),
    ])
}

#[cfg(test)]
#[path = "receipt_json_tests.rs"]
mod tests;
