//! `oce validate`: both validation stages (or stage 1 alone) as a JSON report.
//!
//! Stage 1 judges the model against the catalog; stage 2 runs only when stage 1 found no error
//! (`oce_author::validate`). The report's `stages` names the stages that ran. Exit `0` with no
//! error finding, `1` with one, `2` when the input could not be read or parsed.

use oce_author::{Finding, Origin, Severity, validate, validate_author_side};
use serde_json::Value;

use super::{Outcome, report_for};
use crate::report::Report;
use crate::{Source, Streams, input};

/// Run `validate`.
pub(crate) fn run(source: &Source, author_side: bool, streams: &mut Streams<'_>) -> Outcome {
    let mut report = report_for("validate", &source.file);
    let Some(loaded) = input::load(source, streams.stdin, &mut report) else {
        return refused(report);
    };
    let Some(parsed) = input::parse(&loaded, &mut report) else {
        return refused(report);
    };
    let mut findings = if author_side {
        validate_author_side(&parsed.sequence)
    } else {
        validate(&parsed.sequence)
    };
    if let Some(spans) = &parsed.spans {
        spans.apply(&parsed.sequence, &mut findings);
    }
    record(&mut report, &findings, author_side);
    let errors = findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .count();
    report.verdict(if errors == 0 {
        "clean".to_owned()
    } else {
        format!("{errors} error finding(s)")
    });
    Outcome::judged(report)
}

/// Record findings and the stages that ran: stage 2 runs unless `author_side` or a stage-1 error.
pub(crate) fn record(report: &mut Report, findings: &[Finding], author_side: bool) {
    for finding in findings {
        report.finding(finding);
    }
    let stage_one_failed = findings
        .iter()
        .any(|f| f.origin == Origin::Author && f.severity == Severity::Error);
    let stages: &[&str] = if author_side || stage_one_failed {
        &["author"]
    } else {
        &["author", "engine"]
    };
    report.set(
        "stages",
        Value::Array(stages.iter().map(|s| Value::from(*s)).collect()),
    );
}

/// A run that stopped at the read or the parse: no stage ran.
fn refused(mut report: Report) -> Outcome {
    report.set("stages", Value::Array(Vec::new()));
    report.verdict("refused");
    Outcome::judged(report)
}
