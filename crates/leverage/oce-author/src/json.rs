//! The canonical JSON writer shared by the wire form and the published schema.
//!
//! One spelling per document: objects keep the key order they are built with, two-space
//! indentation, every non-empty object and array spread one entry per line, `[]` and `{}` for
//! empty ones, one trailing newline, and ASCII-only output (anything else is a `\uXXXX` escape,
//! astral characters as surrogate pairs). Numbers are written by the caller (see
//! [`Json::integer`] and [`Json::real`]), so no third-party formatter decides a byte.

/// A JSON value with a fixed key order.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Json {
    /// A string, escaped on output.
    Str(String),
    /// A number, `true`, `false` or `null`, written verbatim.
    Raw(String),
    /// An array.
    Arr(Vec<Json>),
    /// An object, in the given key order.
    Obj(Vec<(String, Json)>),
}

impl Json {
    /// A string value.
    pub(crate) fn str(text: &str) -> Self {
        Self::Str(text.to_owned())
    }

    /// An integer, without a fraction.
    pub(crate) fn integer(value: i64) -> Self {
        Self::Raw(value.to_string())
    }

    /// A real in the shortest round-trip spelling (Rust's `{:?}` for `f64`: `1.0`, `0.1`,
    /// `1e-7`, `-0.0`), which always carries a fraction or an exponent and so reads back as a
    /// real. A non-finite value has no JSON spelling and is written `null`, which the wire parser
    /// refuses; the validator reports it first as `non-finite-parameter`.
    pub(crate) fn real(value: f64) -> Self {
        if value.is_finite() {
            Self::Raw(format!("{value:?}"))
        } else {
            Self::Raw("null".to_owned())
        }
    }

    /// A Boolean.
    pub(crate) fn boolean(value: bool) -> Self {
        Self::Raw(if value { "true" } else { "false" }.to_owned())
    }

    /// An object from `(key, value)` pairs, in order.
    pub(crate) fn obj<const N: usize>(entries: [(&str, Json); N]) -> Self {
        Self::Obj(
            entries
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value))
                .collect(),
        )
    }

    /// The canonical document: the value, then one newline.
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
            Self::Arr(items) if items.is_empty() => out.push_str("[]"),
            Self::Obj(entries) if entries.is_empty() => out.push_str("{}"),
            Self::Arr(items) => {
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
            Self::Obj(entries) => {
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
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            ' '..='~' => out.push(ch),
            _ => {
                let mut units = [0u16; 2];
                for unit in ch.encode_utf16(&mut units) {
                    out.push_str(&format!("\\u{unit:04x}"));
                }
            }
        }
    }
    out.push('"');
}

/// Convert a `serde_json` value into [`Json`] with every object's keys sorted, so the bytes do
/// not depend on whether a build unifies `serde_json/preserve_order`.
pub(crate) fn sorted(value: &serde_json::Value) -> Json {
    match value {
        serde_json::Value::Null => Json::Raw("null".to_owned()),
        serde_json::Value::Bool(b) => Json::boolean(*b),
        serde_json::Value::Number(n) => Json::Raw(n.to_string()),
        serde_json::Value::String(s) => Json::str(s),
        serde_json::Value::Array(items) => Json::Arr(items.iter().map(sorted).collect()),
        serde_json::Value::Object(map) => {
            let mut entries: Vec<(String, Json)> =
                map.iter().map(|(k, v)| (k.clone(), sorted(v))).collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            Json::Obj(entries)
        }
    }
}

/// The length in bytes of `text` as [`Json`] writes it, without the quotes: escapes counted as
/// written (`\"`, `\\`, `\n` two bytes; any other non-printable or non-ASCII character six bytes
/// per UTF-16 unit).
pub(crate) fn wire_string_len(text: &str) -> usize {
    text.chars()
        .map(|ch| match ch {
            '"' | '\\' | '\n' => 2,
            ' '..='~' => 1,
            _ => 6 * ch.len_utf16(),
        })
        .sum()
}
