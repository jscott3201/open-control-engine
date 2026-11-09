//! The canonical JSON writer of the two vocabulary files.
//!
//! One spelling per document: every object's keys sorted, two-space indentation, every non-empty
//! object and array spread one entry per line, `[]` for an empty array, LF line endings, one
//! trailing newline, and ASCII-only output (anything else is a `\uXXXX` escape, astral characters
//! as surrogate pairs). The only numbers the files carry are small unsigned integers, written
//! without a fraction, so no third-party formatter decides a byte.

/// A JSON value. Objects sort their keys when built ([`Json::object`]).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Json {
    /// A string, escaped on output.
    Str(String),
    /// An unsigned integer, `true` or `false`, written verbatim.
    Raw(String),
    /// An array, in the given order.
    Arr(Vec<Json>),
    /// An object, keys sorted.
    Obj(Vec<(&'static str, Json)>),
}

impl Json {
    /// A string value.
    pub(crate) fn str(text: &str) -> Self {
        Self::Str(text.to_owned())
    }

    /// An unsigned integer, without a fraction.
    pub(crate) fn unsigned(value: u32) -> Self {
        Self::Raw(value.to_string())
    }

    /// A Boolean.
    pub(crate) fn boolean(value: bool) -> Self {
        Self::Raw(if value { "true" } else { "false" }.to_owned())
    }

    /// An array of the mapped items, in order.
    pub(crate) fn array<T>(items: &[T], map: impl Fn(&T) -> Json) -> Self {
        Self::Arr(items.iter().map(map).collect())
    }

    /// An object from `(key, value)` pairs, sorted by key so the caller's order never reaches a
    /// byte.
    pub(crate) fn object<const N: usize>(entries: [(&'static str, Json); N]) -> Self {
        let mut entries = Vec::from(entries);
        entries.sort_by(|a, b| a.0.cmp(b.0));
        Self::Obj(entries)
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

#[cfg(test)]
mod tests {
    use super::Json;

    #[test]
    fn object_keys_are_sorted_whatever_order_they_are_given_in() {
        let value = Json::object([
            ("b", Json::unsigned(2)),
            ("a", Json::unsigned(1)),
            ("c", Json::Arr(Vec::new())),
        ]);
        assert_eq!(
            value.to_document(),
            "{\n  \"a\": 1,\n  \"b\": 2,\n  \"c\": []\n}\n"
        );
    }

    #[test]
    fn strings_are_ascii_with_quotes_backslashes_and_non_ascii_escaped() {
        let value = Json::str("a\"b\\c\u{e9}\u{1F600}\n\u{7f}");
        assert_eq!(
            value.to_document(),
            "\"a\\\"b\\\\c\\u00e9\\ud83d\\ude00\\u000a\\u007f\"\n"
        );
    }

    #[test]
    fn nested_values_indent_two_spaces_per_level_and_end_with_one_newline() {
        let value = Json::object([(
            "list",
            Json::Arr(vec![Json::object([("on", Json::boolean(true))])]),
        )]);
        assert_eq!(
            value.to_document(),
            "{\n  \"list\": [\n    {\n      \"on\": true\n    }\n  ]\n}\n"
        );
    }
}
