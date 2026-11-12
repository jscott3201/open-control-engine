//! The JSON report: one closed object per run, the agent-facing contract the crate README
//! documents.
//!
//! ```text
//! { "command", "errors", "findings", "input", "ok", "schema", "tool", ...command fields }
//! ```
//!
//! Keys are sorted (`serde_json`'s map is ordered by key, and a test walks every report to hold
//! it), the document is two-space indented with LF line endings and one final line feed, and
//! nothing in it depends on the clock, the locale or the host's paths: `input` is the path exactly
//! as given (`-` for stdin). `findings` are the validator's (or the class rules') answers and make
//! the exit `1`; `errors` are why a question could not be answered and make it `2`.
//!
//! An error is `{ "code", "detail", "span" }`. A `verify` refusal adds, each only when it has a
//! value (owner rulings of 2026-11-11, vectors note "Part 4's readings"): `at`, the place it is
//! about (`scenarios[0].inputs.u`, `mutants[2]`, `subject`, `line 3 column 7`); `path`, the
//! canonical path
//! it concerns; `candidates`, an ambiguous name's matches in their order; `cause`, a value from a
//! closed table `oce-verify` owns; `diagnostic_codes`, the engine's codes of a subject that did not
//! load.

use oce_author::{Finding, Location, Origin, ParseError, Severity};
use oce_verify::{VectorError, VerifyError};
use serde_json::{Map, Value, json};

/// The report schema identifier every report carries under `schema`.
pub const REPORT_SCHEMA: &str = "oce-report/1";

/// One run's report.
#[derive(Debug)]
pub(crate) struct Report {
    command: &'static str,
    input: Option<String>,
    findings: Vec<Value>,
    errors: Vec<Value>,
    fields: Map<String, Value>,
    /// The verdict line of the human-readable rendering.
    verdict: String,
}

impl Report {
    /// An empty report for `command` over `input` (`None` for commands that read no sequence).
    pub(crate) fn new(command: &'static str, input: Option<String>) -> Self {
        Self {
            command,
            input,
            findings: Vec::new(),
            errors: Vec::new(),
            fields: Map::new(),
            verdict: String::new(),
        }
    }

    /// Set a command field (`stages`, `canonical`, `classes`, …).
    pub(crate) fn set(&mut self, key: &str, value: Value) {
        self.fields.insert(key.to_owned(), value);
    }

    /// Record a validator finding.
    pub(crate) fn finding(&mut self, finding: &Finding) {
        self.findings.push(json!({
            "code": finding.code,
            "detail": finding.detail,
            "origin": origin_name(finding.origin),
            "severity": severity_name(finding.severity),
            "span": span(finding.span),
            "subject": finding.subject,
        }));
    }

    /// Record a finding the tool states itself (an emitter or class-rule refusal), always an error.
    pub(crate) fn finding_raw(&mut self, code: &str, subject: &str, detail: &str, origin: &str) {
        self.findings.push(json!({
            "code": code,
            "detail": detail,
            "origin": origin,
            "severity": "error",
            "span": Value::Null,
            "subject": subject,
        }));
    }

    /// Record why the question could not be answered.
    pub(crate) fn error(&mut self, code: &str, detail: impl Into<String>, at: Option<Location>) {
        self.errors.push(json!({
            "code": code,
            "detail": detail.into(),
            "span": span(at),
        }));
    }

    /// Record a refusal about one place (`at`), with the optional facts a `verify` refusal
    /// carries. Each optional key is written only when it has a value.
    pub(crate) fn error_at(
        &mut self,
        code: &str,
        detail: impl Into<String>,
        location: Option<Location>,
        facts: &Facts<'_>,
    ) {
        let mut entry = Map::new();
        entry.insert("at".into(), Value::from(facts.at));
        entry.insert("code".into(), Value::from(code));
        entry.insert("detail".into(), Value::from(detail.into()));
        entry.insert("span".into(), span(location));
        if let Some(path) = facts.path {
            entry.insert("path".into(), Value::from(path));
        }
        if !facts.candidates.is_empty() {
            entry.insert("candidates".into(), strings(facts.candidates));
        }
        if let Some(cause) = facts.cause {
            entry.insert("cause".into(), Value::from(cause));
        }
        if !facts.diagnostic_codes.is_empty() {
            entry.insert("diagnostic_codes".into(), strings(facts.diagnostic_codes));
        }
        self.errors.push(Value::Object(entry));
    }

    /// Record a vector file `oce-verify` refused to load: its code, the place and, for a refusal
    /// found in the text, its line and column.
    pub(crate) fn vector_error(&mut self, error: &VectorError) {
        let wide = |count: u32| usize::try_from(count).unwrap_or(usize::MAX);
        let at = error
            .at()
            .map(|at| Location::new(wide(at.line), wide(at.column)));
        self.error_at(
            error.code(),
            error.detail(),
            at,
            &Facts::about(error.subject()),
        );
    }

    /// Record a bind or run refusal of `oce-verify`, with every fact it carries.
    pub(crate) fn verify_error(&mut self, error: &VerifyError) {
        let candidates: Vec<&str> = error.candidates().iter().map(String::as_str).collect();
        let facts = Facts {
            at: error.subject(),
            path: error.path(),
            candidates: &candidates,
            cause: error.cause(),
            diagnostic_codes: error.diagnostic_codes(),
        };
        self.error_at(error.code(), error.detail(), None, &facts);
    }

    /// Record a text-form parse refusal.
    pub(crate) fn parse_error(&mut self, error: &ParseError) {
        self.error(error.code, error.detail.clone(), Some(error.location));
    }

    /// Whether any finding is an error.
    pub(crate) fn has_error_finding(&self) -> bool {
        self.findings
            .iter()
            .any(|f| f.get("severity").and_then(Value::as_str) == Some("error"))
    }

    /// Whether any error was recorded.
    pub(crate) fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Set the verdict line of the human-readable rendering.
    pub(crate) fn verdict(&mut self, verdict: impl Into<String>) {
        self.verdict = verdict.into();
    }

    /// The report as JSON text: sorted keys, two-space indentation, one final line feed.
    pub(crate) fn to_json(&self, ok: bool) -> String {
        let mut object = self.fields.clone();
        object.insert("command".into(), Value::from(self.command));
        object.insert("errors".into(), Value::Array(self.errors.clone()));
        object.insert("findings".into(), Value::Array(self.findings.clone()));
        object.insert(
            "input".into(),
            self.input.clone().map_or(Value::Null, Value::from),
        );
        object.insert("ok".into(), Value::Bool(ok));
        object.insert("schema".into(), Value::from(REPORT_SCHEMA));
        object.insert("tool".into(), Value::from("oce"));
        // Serializing a `Value` built from strings, numbers and booleans cannot fail; an empty
        // object is still a well-formed report if it ever did.
        let mut text = serde_json::to_string_pretty(&Value::Object(object))
            .unwrap_or_else(|_| String::from("{}"));
        text.push('\n');
        text
    }

    /// The human-readable rendering: one line per finding and error, then the verdict.
    pub(crate) fn to_human(&self) -> String {
        let place = self
            .input
            .clone()
            .unwrap_or_else(|| format!("oce {}", self.command));
        let mut out = String::new();
        for finding in &self.findings {
            let field = |key: &str| finding.get(key).and_then(Value::as_str).unwrap_or("");
            out.push_str(&format!(
                "{}: {}[{}] {}: {}\n",
                located(&place, finding.get("span")),
                field("severity"),
                field("code"),
                field("subject"),
                field("detail"),
            ));
        }
        for error in &self.errors {
            let field = |key: &str| error.get(key).and_then(Value::as_str).unwrap_or("");
            out.push_str(&format!(
                "{}: error[{}] {}\n",
                located(&place, error.get("span")),
                field("code"),
                field("detail"),
            ));
        }
        match &self.input {
            Some(input) => {
                out.push_str(&format!("oce {} {input}: {}\n", self.command, self.verdict))
            }
            None => out.push_str(&format!("oce {}: {}\n", self.command, self.verdict)),
        }
        out
    }
}

/// The place a refusal is about and the optional facts it carries ([`Report::error_at`]).
pub(crate) struct Facts<'a> {
    /// The place, written as the error's `at`: a path into the vector set, `subject`,
    /// `mutants[i]`.
    pub(crate) at: &'a str,
    /// The canonical path the refusal concerns.
    pub(crate) path: Option<&'a str>,
    /// An ambiguous name's matches, in their order.
    pub(crate) candidates: &'a [&'a str],
    /// A value from one of `oce-verify`'s closed cause tables.
    pub(crate) cause: Option<&'a str>,
    /// The engine's codes of a subject that did not load.
    pub(crate) diagnostic_codes: &'a [&'a str],
}

impl<'a> Facts<'a> {
    /// A place and nothing more.
    pub(crate) fn about(at: &'a str) -> Self {
        Self {
            at,
            path: None,
            candidates: &[],
            cause: None,
            diagnostic_codes: &[],
        }
    }
}

fn strings(items: &[&str]) -> Value {
    Value::Array(items.iter().map(|item| Value::from(*item)).collect())
}

/// `place:line:column` when the span is known, else `place`.
fn located(place: &str, span: Option<&Value>) -> String {
    let number = |key: &str| span.and_then(|s| s.get(key)).and_then(Value::as_u64);
    match (number("line"), number("column")) {
        (Some(line), Some(column)) => format!("{place}:{line}:{column}"),
        _ => place.to_owned(),
    }
}

fn span(at: Option<Location>) -> Value {
    at.map_or(
        Value::Null,
        |at| json!({ "column": at.column, "line": at.line }),
    )
}

fn origin_name(origin: Origin) -> &'static str {
    match origin {
        Origin::Author => "author",
        Origin::Engine => "engine",
        _ => "unknown",
    }
}

fn severity_name(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        _ => "unknown",
    }
}
