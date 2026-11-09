//! The strict-JSON wire form (authoring note, "Wire form"; decisions 8 and 11).
//!
//! [`from_wire_str`] reads a document into the model and [`to_wire_string`] writes the model
//! canonically. Reading is bounded before anything is allocated per element: the document size
//! ([`MAX_WIRE_BYTES`]), nesting depth ([`MAX_WIRE_DEPTH`]), list lengths ([`MAX_WIRE_LIST_LEN`])
//! and string lengths ([`MAX_WIRE_STRING_BYTES`]) are each checked, and every refusal is a typed
//! [`WireError`], never a panic. Names are carried unchecked so that [`crate::validate()`] reports a
//! bad name as a finding.
//!
//! The canonical spelling is fixed: the key order of the schema, two-space indentation, one
//! entry per line, one trailing newline, ASCII-only strings, integers without a fraction and reals
//! in the shortest round-trip spelling. For every document `d` this module writes,
//! `to_wire_string(&from_wire_str(d)?) == d`.

mod dto;
mod scan;

use std::fmt;

use crate::json::Json;
use crate::model::{Boundary, Literal, ParamValue, Sequence};

pub(crate) use dto::WireSequence;

/// The largest wire document, in bytes (4 MiB, the text form's bound).
pub const MAX_WIRE_BYTES: usize = 4 * 1024 * 1024;

/// The deepest nesting of objects and arrays. The deepest well-formed document nests six levels
/// (sequence, blocks, block, params, assignment, value object); two more are allowed so that a
/// misplaced value is reported as a wrong type, not as depth.
pub const MAX_WIRE_DEPTH: usize = 8;

/// The longest list (parameters, boundaries, blocks, assignments, connections or haystack tags).
pub const MAX_WIRE_LIST_LEN: usize = 65_536;

/// The longest string token, in bytes as written (escapes counted as written).
pub const MAX_WIRE_STRING_BYTES: usize = 4_096;

/// A 1-based line and column (bytes) in a wire document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    /// 1-based line.
    pub line: usize,
    /// 1-based column, in bytes.
    pub column: usize,
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {} column {}", self.line, self.column)
    }
}

/// Why a wire document was refused. Never a panic.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WireError {
    /// The bytes are not UTF-8; carries the offset of the first bad byte.
    NotUtf8 {
        /// Byte offset.
        offset: usize,
    },
    /// The document starts with a byte-order mark.
    ByteOrderMark,
    /// The document is larger than [`MAX_WIRE_BYTES`].
    TooLarge {
        /// The document size in bytes.
        bytes: usize,
        /// The limit.
        limit: usize,
    },
    /// Objects and arrays nest deeper than [`MAX_WIRE_DEPTH`].
    TooDeep {
        /// Where the refused level opens.
        at: Position,
        /// The limit.
        limit: usize,
    },
    /// A list is longer than [`MAX_WIRE_LIST_LEN`].
    ListTooLong {
        /// Where the list opens.
        at: Position,
        /// The limit.
        limit: usize,
    },
    /// A string is longer than [`MAX_WIRE_STRING_BYTES`].
    StringTooLong {
        /// Where the string opens.
        at: Position,
        /// The limit.
        limit: usize,
    },
    /// An object repeats a key.
    DuplicateKey {
        /// The repeated key.
        key: String,
        /// Where the repeat is.
        at: Position,
    },
    /// `NaN` or `Infinity` spelled as a number, or a real that overflows binary64.
    NonFiniteNumber {
        /// Where the number is.
        at: Position,
    },
    /// An integer token (no fraction or exponent) outside the `i64` range.
    IntegerOutOfRange {
        /// Where the number is.
        at: Position,
    },
    /// A string carries a control character (anything in C0 but line feed, DEL or C1), raw or
    /// escaped.
    ControlCharacter {
        /// Where it is.
        at: Position,
    },
    /// An object carries a key its schema does not define.
    UnknownKey {
        /// The key.
        key: String,
        /// Where the parse stopped.
        at: Position,
    },
    /// An object lacks a required key.
    MissingKey {
        /// The key.
        key: String,
        /// Where the parse stopped.
        at: Position,
    },
    /// A value has the wrong JSON type or an unknown enumerator.
    WrongType {
        /// What the parser found and expected.
        detail: String,
        /// Where the parse stopped.
        at: Position,
    },
    /// The document is not JSON.
    Syntax {
        /// What the parser found.
        detail: String,
        /// Where the parse stopped.
        at: Position,
    },
    /// A connection string does not have exactly one `->`.
    MalformedConnection {
        /// The connection's index in `connections`.
        index: usize,
        /// The string as written.
        text: String,
    },
}

impl fmt::Display for WireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotUtf8 { offset } => write!(f, "byte {offset} is not UTF-8"),
            Self::ByteOrderMark => f.write_str("the document starts with a byte-order mark"),
            Self::TooLarge { bytes, limit } => {
                write!(
                    f,
                    "the document is {bytes} bytes; at most {limit} are allowed"
                )
            }
            Self::TooDeep { at, limit } => {
                write!(f, "nesting deeper than {limit} levels at {at}")
            }
            Self::ListTooLong { at, limit } => {
                write!(f, "the list at {at} has more than {limit} entries")
            }
            Self::StringTooLong { at, limit } => {
                write!(f, "the string at {at} is longer than {limit} bytes")
            }
            Self::DuplicateKey { key, at } => write!(f, "duplicate key `{key}` at {at}"),
            Self::NonFiniteNumber { at } => write!(f, "a non-finite number at {at}"),
            Self::IntegerOutOfRange { at } => {
                write!(f, "an integer outside the 64-bit signed range at {at}")
            }
            Self::ControlCharacter { at } => {
                write!(f, "a control character in a string at {at}")
            }
            Self::UnknownKey { key, at } => write!(f, "unknown key `{key}` at {at}"),
            Self::MissingKey { key, at } => write!(f, "missing key `{key}` at {at}"),
            Self::WrongType { detail, at } => write!(f, "{detail} at {at}"),
            Self::Syntax { detail, at } => write!(f, "not JSON: {detail} at {at}"),
            Self::MalformedConnection { index, text } => write!(
                f,
                "connection {index} `{text}` is not `source -> target` with exactly one `->`"
            ),
        }
    }
}

impl std::error::Error for WireError {}

/// Read wire bytes: [`from_wire_str`] after a UTF-8 check.
///
/// # Errors
///
/// [`WireError::NotUtf8`], or any refusal of [`from_wire_str`].
pub fn from_wire_bytes(bytes: &[u8]) -> Result<Sequence, WireError> {
    if bytes.len() > MAX_WIRE_BYTES {
        return Err(WireError::TooLarge {
            bytes: bytes.len(),
            limit: MAX_WIRE_BYTES,
        });
    }
    let text = std::str::from_utf8(bytes).map_err(|error| WireError::NotUtf8 {
        offset: error.valid_up_to(),
    })?;
    from_wire_str(text)
}

/// Read a wire document into the model.
///
/// # Errors
///
/// A typed [`WireError`] for a document over a bound, with a byte-order mark, a duplicate key, a
/// non-finite or out-of-range number, a control character, an unknown or missing key, a wrong
/// type, broken syntax, or a connection without exactly one `->`. Never panics.
pub fn from_wire_str(text: &str) -> Result<Sequence, WireError> {
    if text.len() > MAX_WIRE_BYTES {
        return Err(WireError::TooLarge {
            bytes: text.len(),
            limit: MAX_WIRE_BYTES,
        });
    }
    if text.starts_with('\u{feff}') {
        return Err(WireError::ByteOrderMark);
    }
    let negative_zeros = scan::scan(text)?;
    let dto: WireSequence = if negative_zeros.is_empty() {
        serde_json::from_str(text)
    } else {
        // Blank the sign of each integer `-0` token (same length, so positions hold).
        let mut bytes = text.as_bytes().to_vec();
        for at in negative_zeros {
            bytes[at] = b' ';
        }
        let unsigned = String::from_utf8(bytes).map_err(|error| WireError::NotUtf8 {
            offset: error.utf8_error().valid_up_to(),
        })?;
        serde_json::from_str(&unsigned)
    }
    .map_err(|error| classify(&error))?;
    dto.into_model()
}

/// Map a `serde_json` refusal onto the typed vocabulary. serde's data-error messages are stable
/// (`unknown field`, `missing field`, `duplicate field`); anything else is a wrong type, and a
/// syntax or end-of-input error is [`WireError::Syntax`].
fn classify(error: &serde_json::Error) -> WireError {
    let at = Position {
        line: error.line(),
        column: error.column(),
    };
    let full = error.to_string();
    let suffix = format!(" at line {} column {}", at.line, at.column);
    let detail = full.strip_suffix(&suffix).unwrap_or(&full).to_owned();
    let quoted = |prefix: &str| -> Option<String> {
        let rest = detail.strip_prefix(prefix)?;
        let end = rest.find('`')?;
        Some(rest[..end].to_owned())
    };
    match error.classify() {
        serde_json::error::Category::Data => {
            if let Some(key) = quoted("unknown field `") {
                WireError::UnknownKey { key, at }
            } else if let Some(key) = quoted("missing field `") {
                WireError::MissingKey { key, at }
            } else if let Some(key) = quoted("duplicate field `") {
                WireError::DuplicateKey { key, at }
            } else {
                WireError::WrongType { detail, at }
            }
        }
        _ => WireError::Syntax { detail, at },
    }
}

/// Write the model in the canonical wire spelling (see the module documentation). Infallible: a
/// name the validator would refuse is written as it is, and a non-finite Real as `null`.
#[must_use]
pub fn to_wire_string(sequence: &Sequence) -> String {
    let params = sequence
        .params
        .iter()
        .map(|p| {
            Json::obj([
                ("name", Json::str(p.name.as_str())),
                ("type", Json::str(p.value_type.as_str())),
                ("value", literal(&p.value)),
            ])
        })
        .collect();
    let blocks = sequence
        .blocks
        .iter()
        .map(|b| {
            let assignments = b
                .assignments
                .iter()
                .map(|a| {
                    let value = match &a.value {
                        ParamValue::Literal(lit) => literal(lit),
                        ParamValue::Ref(name) => Json::obj([("ref", Json::str(name.as_str()))]),
                    };
                    Json::obj([("name", Json::str(a.name.as_str())), ("value", value)])
                })
                .collect();
            Json::obj([
                ("name", Json::str(b.name.as_str())),
                ("class", Json::str(&b.class_path)),
                ("params", Json::Arr(assignments)),
            ])
        })
        .collect();
    let connections = sequence
        .connections
        .iter()
        .map(|c| Json::Str(c.spelling()))
        .collect();
    Json::obj([
        ("sequence", Json::str(sequence.name.as_str())),
        (
            "extends",
            Json::str(sequence.extends.as_ref().map_or("", |id| id.as_str())),
        ),
        ("params", Json::Arr(params)),
        ("inputs", boundaries(&sequence.inputs)),
        ("outputs", boundaries(&sequence.outputs)),
        ("blocks", Json::Arr(blocks)),
        ("connections", Json::Arr(connections)),
    ])
    .to_document()
}

fn literal(value: &Literal) -> Json {
    match value {
        Literal::Integer(v) => Json::integer(*v),
        Literal::Real(v) => Json::real(*v),
        Literal::Boolean(v) => Json::boolean(*v),
        Literal::Text(v) => Json::str(v),
        Literal::EnumMember(m) => Json::obj([("member", Json::Str(m.spelling()))]),
    }
}

fn text(value: Option<&String>) -> Json {
    Json::str(value.map_or("", String::as_str))
}

fn boundaries(list: &[Boundary]) -> Json {
    Json::Arr(
        list.iter()
            .map(|b| {
                let s = &b.semantic;
                Json::obj([
                    ("name", Json::str(b.name.as_str())),
                    ("type", Json::str(b.value_type.as_str())),
                    ("quantity", text(b.quantity.as_ref())),
                    ("display_unit", text(b.display_unit.as_ref())),
                    (
                        "semantic",
                        Json::obj([
                            ("name", text(s.name.as_ref())),
                            ("label", text(s.label.as_ref())),
                            ("unit", text(s.unit.as_ref())),
                            ("brick", text(s.brick.as_ref())),
                            ("s223", text(s.s223.as_ref())),
                            ("qudt", text(s.qudt.as_ref())),
                            (
                                "haystack",
                                Json::Arr(s.haystack.iter().map(|t| Json::str(t)).collect()),
                            ),
                            ("provisional", Json::boolean(s.provisional)),
                        ]),
                    ),
                ])
            })
            .collect(),
    )
}
