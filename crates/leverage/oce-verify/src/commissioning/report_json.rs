//! A commissioning body as canonical JSON (commissioning note, "Report and receipt" and
//! "Values"), through the receipt's one writer, so the two kinds share every spelling rule.
//!
//! A number the spec wrote (a window end, a sampling, a tolerance, a parameter value) is written
//! as the spec's canonical form writes it; a tolerance no object gives is `0`. A computed Real
//! (`t`, `from`, `to`, a half-size, an exceedance, `exceedance_time_s`) is the canonical real
//! spelling, and negative zero, a NaN or an infinity the string `"0x"` and the 16 hex digits of
//! its bits. Counts are JSON integers.

use super::report::{
    CommissioningBody, Exceedance, FindingRecord, MaskedInterval, OutputReport, TimeRecord,
};
use super::tolerances::TOLERANCE_NAMES;
use super::trend_types::{SourceRecord, UnitColumn};
use super::units::UNITS_TABLE;
use crate::canonical::Json;
use crate::envelope::HalfSize;
use crate::receipt::{FrameCounts, ReceiptValue};
use crate::receipt_json::{count, entry, time, value, warning};

/// A computed Real.
fn real(x: f64) -> Json {
    value(ReceiptValue::Real(x))
}

/// The body of a commissioning receipt.
pub(crate) fn receipt_body_json(body: &CommissioningBody) -> Json {
    let mut entries = vec![
        entry("status", Json::str(body.status.as_str())),
        entry("reference", count(body.reference)),
        entry("model", Json::str(&body.model)),
        entry("sequence", Json::str(&body.sequence)),
        entry("data", Json::str(body.data.as_str())),
        entry(
            "parameters",
            Json::List(
                body.parameters
                    .iter()
                    .map(|edit| {
                        Json::object([
                            entry("path", Json::str(&edit.path)),
                            entry("value", Json::Raw(edit.value.canonical())),
                        ])
                    })
                    .collect(),
            ),
        ),
        entry("time", time_record(&body.time)),
        entry(
            "units",
            Json::object([
                entry("table", Json::str(UNITS_TABLE)),
                entry(
                    "columns",
                    Json::List(body.unit_columns.iter().map(unit_column).collect()),
                ),
                entry("columns_total", count(body.unit_columns_total)),
            ]),
        ),
        entry(
            "sources",
            Json::List(body.sources.iter().map(source).collect()),
        ),
        entry("frames", frames(&body.frames)),
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
        entry(
            "warnings",
            Json::List(body.warnings.iter().map(warning).collect()),
        ),
        entry("warnings_total", count(body.warnings_total)),
        entry(
            "outputs",
            Json::List(body.outputs.iter().map(output).collect()),
        ),
        entry(
            "unverified_outputs",
            Json::List(
                body.unverified_outputs
                    .iter()
                    .map(|path| Json::str(path))
                    .collect(),
            ),
        ),
        entry(
            "unverified_outputs_total",
            count(body.unverified_outputs_total),
        ),
        entry(
            "findings",
            Json::List(body.findings.iter().map(finding).collect()),
        ),
        entry("findings_total", count(body.findings_total)),
    ];
    if let Some(content_id) = &body.content_id {
        entries.push(entry("content_id", Json::str(content_id)));
    }
    if let Some(mapping) = &body.mapping {
        entries.push(entry(
            "mapping",
            Json::object([
                entry("path", Json::str(&mapping.path)),
                entry("digest", Json::str(&mapping.digest)),
            ]),
        ));
    }
    Json::object(entries)
}

fn time_record(time: &TimeRecord) -> Json {
    let mut entries = vec![
        entry("base", Json::str("recorded")),
        entry(
            "window",
            Json::object([
                entry("start", Json::Raw(time.window_start.canonical())),
                entry("stop", Json::Raw(time.window_stop.canonical())),
            ]),
        ),
    ];
    if let Some(origin) = &time.origin {
        entries.push(entry("origin", Json::str(origin.as_str())));
    }
    entries.push(entry("sampling", Json::Raw(time.sampling.canonical())));
    Json::object(entries)
}

fn unit_column(column: &UnitColumn) -> Json {
    Json::object([
        entry("device", Json::str(&column.device)),
        entry("cdl", Json::str(&column.cdl)),
        entry("from", Json::str(&column.from)),
        entry("to", Json::str(&column.to)),
    ])
}

fn source(source: &SourceRecord) -> Json {
    let mut entries = vec![
        entry("role", Json::str(source.role.as_str())),
        entry("path", Json::str(&source.path)),
        entry("digest", Json::str(&source.digest)),
        entry("rows", count(source.rows)),
    ];
    if let Some(table) = &source.table {
        entries.push(entry("table", Json::str(table)));
    }
    Json::object(entries)
}

fn frames(counts: &FrameCounts) -> Json {
    Json::object([
        entry("executed", count(counts.executed)),
        entry("substituted", count(counts.substituted)),
        entry("refused", count(counts.refused)),
    ])
}

fn finding(finding: &FindingRecord) -> Json {
    Json::object([
        entry("code", Json::str(&finding.code)),
        entry("at", Json::str(&finding.at)),
    ])
}

fn half(size: HalfSize) -> Json {
    Json::object([entry("min", real(size.min)), entry("max", real(size.max))])
}

fn exceedance(at: &Exceedance) -> Json {
    Json::object([
        entry("t", time(at.t)),
        entry("exceedance", real(at.exceedance)),
    ])
}

fn interval(interval: &MaskedInterval) -> Json {
    Json::object([
        entry("from", time(interval.from)),
        entry("to", time(interval.to)),
    ])
}

fn output(report: &OutputReport) -> Json {
    let mut entries = vec![
        entry("output", Json::str(&report.output)),
        entry("type", Json::str(report.output_type.as_str())),
        entry("status", Json::str(report.status.as_str())),
        entry("compared", count(report.compared)),
        entry("masked", count(report.masked)),
        entry("uncovered", count(report.uncovered)),
        entry("exceedance_time_s", real(report.exceedance_time_s)),
        entry(
            "masked_intervals",
            Json::List(report.masked_intervals.iter().map(interval).collect()),
        ),
        entry(
            "masked_intervals_total",
            count(report.masked_intervals_total),
        ),
        entry(
            "masked_exceedance",
            Json::List(report.masked_exceedance.iter().map(exceedance).collect()),
        ),
        entry(
            "masked_exceedance_total",
            count(report.masked_exceedance_total),
        ),
    ];
    if let Some(reference) = report.funnel_reference {
        entries.push(entry("funnel_reference", Json::str(reference.as_str())));
    }
    if let Some(tolerances) = &report.tolerances {
        entries.push(entry(
            "tolerances",
            Json::object(
                TOLERANCE_NAMES
                    .iter()
                    .zip(tolerances.written)
                    .map(|(name, written)| {
                        entry(
                            name,
                            Json::Raw(written.map_or_else(|| "0".to_owned(), |n| n.canonical())),
                        )
                    }),
            ),
        ));
    }
    if let Some(width) = report.half_width {
        entries.push(entry("half_width", half(width)));
    }
    if let Some(height) = report.half_height {
        entries.push(entry("half_height", half(height)));
    }
    if let Some(worst) = &report.worst {
        entries.push(entry("worst", exceedance(worst)));
    }
    if let Some(first) = report.first_exceedance_t {
        entries.push(entry("first_exceedance_t", time(first)));
    }
    Json::object(entries)
}

#[cfg(test)]
#[path = "report_json_tests.rs"]
mod tests;
