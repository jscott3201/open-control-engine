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
//! The human-readable rendering ([`Report::to_human`]) goes to a terminal, and a report quotes
//! strings a document wrote: a wire file's block `class` in a `class-not-found` finding, a CXF
//! node's `@id` in an `import` refusal. So every format or control character and every line or
//! paragraph separator in it is written as a `\u{…}` escape ([`printable`]): nothing a document
//! carries can move the cursor, recolour the line, start a new one or reorder it. The JSON
//! needs no such step; its writer escapes what JSON requires.
//!
//! An error is `{ "code", "detail", "span" }`, plus optional keys, each written only when it has
//! a value. `at` is the place the refusal is about: a `verify` refusal's (`scenarios[0].inputs.u`,
//! `mutants[2]`, `subject`, `line 3 column 7`; owner rulings of 2026-11-11, vectors note "Part
//! 4's readings"), `"input"` for an `id` document the engine refuses, an `import` refusal's
//! subject (a node `@id`, or `document`), and the side of a `diff` refusal (`base`, `head`). In
//! the human-readable rendering an error whose `at` names a place the command registered
//! ([`Report::place`]) is located at that place's input name, so a refusal of `diff`'s head is
//! shown at the head's file. The others are `verify`'s and `id`'s: `path`, the
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
    /// The input names an error's `at` locates it at in the human-readable rendering.
    places: Vec<(&'static str, String)>,
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
            places: Vec::new(),
        }
    }

    /// Render an error whose `at` is `at` at the input `name` (as given) instead of at `input`.
    pub(crate) fn place(&mut self, at: &'static str, name: String) {
        self.places.push((at, name));
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

    /// The human-readable rendering: one line per finding and error, then the verdict, with
    /// every format, control or separator character in a quoted string written as an escape
    /// ([`printable`]).
    pub(crate) fn to_human(&self) -> String {
        let input = self.input.as_deref().map(printable);
        let place = input
            .clone()
            .unwrap_or_else(|| format!("oce {}", self.command));
        let mut out = String::new();
        for finding in &self.findings {
            let field =
                |key: &str| printable(finding.get(key).and_then(Value::as_str).unwrap_or(""));
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
            let field = |key: &str| printable(error.get(key).and_then(Value::as_str).unwrap_or(""));
            let at = error.get("at").and_then(Value::as_str);
            let place = self
                .places
                .iter()
                .find(|(registered, _)| Some(*registered) == at)
                .map_or_else(|| place.clone(), |(_, name)| printable(name));
            out.push_str(&format!(
                "{}: error[{}] {}\n",
                located(&place, error.get("span")),
                field("code"),
                field("detail"),
            ));
        }
        let verdict = printable(&self.verdict);
        match &input {
            Some(input) => out.push_str(&format!("oce {} {input}: {verdict}\n", self.command)),
            None => out.push_str(&format!("oce {}: {verdict}\n", self.command)),
        }
        out
    }
}

/// `text` as one terminal-safe run: every Unicode format (`Cf`) or control (`Cc`) character, a
/// line feed and an escape included, and the line and paragraph separators (U+2028, U+2029) are
/// written as `\u{…}` with the code point in at least four upper-case hexadecimal digits
/// (`\u{001B}`, `\u{202E}`), the spelling the engine's own messages use for such a character.
/// The first class is the engine's table (`oce_api::is_format_or_control`), the one the importer
/// refuses in a term; the two separators are category `Z` and outside it.
///
/// A backslash is written as it is, as the engine writes it. So the rendering does not tell a
/// U+202E from the eight characters `\u{202E}` a document spelled out: it is advice for a
/// person, the JSON report is the exact form, and doubling every backslash would double every
/// separator of a Windows path in the lines a person reads.
fn printable(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        if unprintable(character) {
            out.push_str(&format!("\\u{{{:04X}}}", u32::from(character)));
        } else {
            out.push(character);
        }
    }
    out
}

/// `text` with each of its lines written by [`printable`]: its line feeds are kept and every
/// other format, control or separator character is an escape. For a multi-line rendering a
/// command writes to stderr itself (a diff's text).
pub(crate) fn printable_lines(text: &str) -> String {
    text.split('\n')
        .map(printable)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Whether [`printable`] writes `character` as an escape.
fn unprintable(character: char) -> bool {
    oce_api::is_format_or_control(character) || matches!(character, '\u{2028}' | '\u{2029}')
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rendering_escapes_every_format_and_control_character_a_report_quotes() {
        let hostile = "a\u{1b}[31mb\u{7}c\nd\u{202e}e\u{9b}f\u{200b}g\u{2028}h\u{2029}i";
        let mut report = Report::new("import", Some(hostile.to_owned()));
        report.error_at("no-root", hostile, None, &Facts::about(hostile));
        report.finding_raw(hostile, hostile, hostile, "author");
        report.verdict(hostile);
        let human = report.to_human();
        let escaped =
            "a\\u{001B}[31mb\\u{0007}c\\u{000A}d\\u{202E}e\\u{009B}f\\u{200B}g\\u{2028}h\\u{2029}i";
        assert_eq!(printable(hostile), escaped);
        // Three lines, and the only control characters left are their own line feeds.
        assert_eq!(human.matches('\n').count(), 3, "{human:?}");
        assert!(
            human.chars().all(|c| c == '\n' || !unprintable(c)),
            "{human:?}"
        );
        // The input name twice, a finding's code, subject and detail, an error's detail, the input name
        // in the verdict line and the verdict.
        assert_eq!(human.matches(escaped).count(), 8, "{human:?}");
    }

    #[test]
    fn a_multi_line_rendering_keeps_its_line_feeds_and_escapes_the_rest() {
        let hostile = "# oce diff a\u{202e}b -> c\n> x\u{1b}[2J\r\n\u{2028}\n";
        assert_eq!(
            printable_lines(hostile),
            "# oce diff a\\u{202E}b -> c\n> x\\u{001B}[2J\\u{000D}\n\\u{2028}\n"
        );
        assert_eq!(printable_lines(""), "");
        assert_eq!(printable_lines("plain\n"), "plain\n");
    }

    #[test]
    fn an_error_at_a_registered_place_is_rendered_at_that_places_name() {
        let mut report = Report::new("diff", Some("base.oce".to_owned()));
        report.place("head", "head\u{1b}.oce".to_owned());
        report.error_at("json-syntax", "bad", None, &Facts::about("head"));
        report.error_at("not-found", "gone", None, &Facts::about("base"));
        report.error("stdin-twice", "twice", None);
        report.verdict("refused");
        assert_eq!(
            report.to_human(),
            "head\\u{001B}.oce: error[json-syntax] bad\nbase.oce: error[not-found] gone\n\
             base.oce: error[stdin-twice] twice\noce diff base.oce: refused\n"
        );
    }

    #[test]
    fn plain_text_and_a_backslash_are_rendered_as_they_are() {
        let plain =
            "dir\\rule.cxf.jsonld: `S231:max` on a block port — é, 日本, \\u{202E} spelled out";
        assert_eq!(printable(plain), plain);
        // A code point past the basic plane keeps all its digits.
        assert_eq!(printable("\u{e0001}"), "\\u{E0001}");
    }
}
