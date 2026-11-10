//! The canonical JSON writer (vectors note, "Canonical form"; decision 16).
//!
//! One spelling per document: every object's keys sorted by their UTF-8 bytes at every depth,
//! two-space indentation, every non-empty object and list spread one entry per line, `{}` and
//! `[]` for empty ones, `": "` after a key, line feeds only, one final line feed, and ASCII-only
//! output: `"` and `\` are `\"` and `\\`, a line feed is `\n`, and any other character outside
//! printable ASCII is `\u` and four lowercase hex digits, an astral character as its surrogate
//! pair. Numbers are spelled by [`crate::number`], so no third-party formatter decides a byte.
//!
//! A hash is taken over these bytes, which is why the keys are sorted here and not left in the
//! order a builder happened to use.

use crate::clock::Clock;
use crate::model::{Expectation, Held, Input, Parameter, Scenario};
use crate::number::Literal;

/// A JSON value ready to be written.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Json {
    /// A string, escaped on output.
    Str(String),
    /// A number, `true`, `false` or `null`, written verbatim.
    Raw(String),
    /// A list, in the given order.
    List(Vec<Json>),
    /// An object; [`Json::object`] sorts its keys.
    Object(Vec<(String, Json)>),
}

impl Json {
    /// A string value.
    pub(crate) fn str(text: &str) -> Self {
        Self::Str(text.to_owned())
    }

    /// An object from `(key, value)` pairs in any order: the keys are sorted by their UTF-8
    /// bytes.
    pub(crate) fn object(entries: impl IntoIterator<Item = (String, Json)>) -> Self {
        let mut entries: Vec<(String, Json)> = entries.into_iter().collect();
        entries.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
        Self::Object(entries)
    }

    /// The canonical document: the value, then one line feed.
    pub(crate) fn to_document(&self) -> String {
        let mut out = String::new();
        self.write(&mut out, 0);
        out.push('\n');
        out
    }

    fn write(&self, out: &mut String, depth: usize) {
        match self {
            Self::Str(text) => write_string(out, text),
            Self::Raw(text) => out.push_str(text),
            Self::List(items) if items.is_empty() => out.push_str("[]"),
            Self::Object(entries) if entries.is_empty() => out.push_str("{}"),
            Self::List(items) => {
                out.push('[');
                for (index, item) in items.iter().enumerate() {
                    out.push_str(if index == 0 { "\n" } else { ",\n" });
                    indent(out, depth + 1);
                    item.write(out, depth + 1);
                }
                out.push('\n');
                indent(out, depth);
                out.push(']');
            }
            Self::Object(entries) => {
                out.push('{');
                for (index, (key, value)) in entries.iter().enumerate() {
                    out.push_str(if index == 0 { "\n" } else { ",\n" });
                    indent(out, depth + 1);
                    write_string(out, key);
                    out.push_str(": ");
                    value.write(out, depth + 1);
                }
                out.push('\n');
                indent(out, depth);
                out.push('}');
            }
        }
    }
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

fn write_string(out: &mut String, text: &str) {
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            ' '..='~' => out.push(character),
            _ => {
                let mut units = [0u16; 2];
                for unit in character.encode_utf16(&mut units) {
                    out.push_str(&format!("\\u{unit:04x}"));
                }
            }
        }
    }
    out.push('"');
}

/// Convert a `serde_json` value into [`Json`] with every object's keys sorted, so the bytes do
/// not depend on the order the value was built in.
pub(crate) fn sorted(value: &serde_json::Value) -> Json {
    match value {
        serde_json::Value::Null => Json::Raw("null".to_owned()),
        serde_json::Value::Bool(boolean) => Json::Raw(boolean.to_string()),
        serde_json::Value::Number(number) => Json::Raw(number.to_string()),
        serde_json::Value::String(text) => Json::str(text),
        serde_json::Value::Array(items) => Json::List(items.iter().map(sorted).collect()),
        serde_json::Value::Object(map) => {
            Json::object(map.iter().map(|(key, item)| (key.clone(), sorted(item))))
        }
    }
}

fn entry(key: &str, value: Json) -> (String, Json) {
    (key.to_owned(), value)
}

fn literal(value: Literal) -> Json {
    Json::Raw(value.canonical())
}

fn parameters(params: &[Parameter]) -> Json {
    Json::object(
        params
            .iter()
            .map(|param| (param.path.clone(), literal(param.value))),
    )
}

fn input(input: &Input) -> Json {
    match &input.held {
        Held::Constant(step) => literal(step.value),
        Held::Steps(steps) => Json::List(
            steps
                .iter()
                .map(|step| {
                    Json::object([
                        entry("t", Json::Raw(step.t.canonical())),
                        entry("value", literal(step.value)),
                    ])
                })
                .collect(),
        ),
    }
}

fn expectation(expectation: &Expectation) -> Json {
    match expectation {
        Expectation::Window(window) => Json::object([
            entry("output", Json::str(&window.output)),
            entry("from_s", Json::Raw(window.from.canonical())),
            entry("to_s", Json::Raw(window.to.canonical())),
            entry("equals", literal(window.equals)),
        ]),
        Expectation::Diagnostic(instant) => {
            let mut diagnostic = vec![entry("block", Json::str(&instant.block))];
            if let Some(message) = &instant.message {
                diagnostic.push(entry("message", Json::str(message)));
            }
            Json::object([
                entry("diagnostic", Json::object(diagnostic)),
                entry("at_s", Json::Raw(instant.at.canonical())),
            ])
        }
    }
}

fn scenario(scenario: &Scenario) -> Json {
    let mut entries = vec![
        entry("name", Json::str(&scenario.name)),
        entry(
            "inputs",
            Json::object(
                scenario
                    .inputs
                    .iter()
                    .map(|each| (each.name.clone(), input(each))),
            ),
        ),
        entry(
            "expect",
            Json::List(scenario.expectations.iter().map(expectation).collect()),
        ),
    ];
    if let Some(description) = &scenario.description {
        entries.push(entry("description", Json::str(description)));
    }
    if let Some(params) = &scenario.params {
        entries.push(entry("params", parameters(params)));
    }
    Json::object(entries)
}

/// The canonical document of a vector set: its tag, clock, optional set parameters and scenarios.
pub(crate) fn vector_set(
    clock: &Clock,
    params: Option<&[Parameter]>,
    scenarios: &[Scenario],
) -> String {
    let mut entries = vec![
        entry("schema", Json::str(crate::VECTORS_SCHEMA)),
        entry(
            "clock",
            Json::object([
                entry("step_s", Json::Raw(clock.step_written().canonical())),
                entry("horizon_s", Json::Raw(clock.horizon_written().canonical())),
            ]),
        ),
        entry(
            "scenarios",
            Json::List(scenarios.iter().map(scenario).collect()),
        ),
    ];
    if let Some(params) = params {
        entries.push(entry("params", parameters(params)));
    }
    Json::object(entries).to_document()
}

#[cfg(test)]
#[path = "canonical_tests.rs"]
mod tests;
