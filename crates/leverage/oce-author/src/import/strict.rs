//! A JSON reader that refuses what `serde_json::Value` would silently resolve: a key written
//! twice in one object (where `serde_json` keeps the last value) and an integer token outside the
//! `i64` range (where it keeps a `u64`, or reads a longer token as a float). Integer range is
//! enforced lexically before parsing: every token outside a string that is an optional `-` and
//! digits with no fraction or exponent must parse as an `i64`, at any depth. The token `-0` is the
//! Integer zero, as in the wire form (`serde_json` would read it as the Real `-0.0`). Nesting is
//! bounded by `serde_json`'s own recursion limit (128), which it reports as an error, never a
//! stack overflow.
//!
//! The v0 graph reader also passes [`Bounds`]: the wire form's depth, list-length and
//! string-length limits, checked while the value is built, so a refused document never finishes
//! allocating its over-long list.

use std::cell::RefCell;
use std::fmt;

use serde::Deserializer;
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};

use super::error::{ImportErrorKind, Limit, StrictJson};

/// The read bounds of the v0 graph reader.
#[derive(Clone, Copy, Debug)]
pub(super) struct Bounds {
    /// The deepest nesting of objects and arrays.
    pub(super) depth: usize,
    /// The longest list.
    pub(super) list: usize,
    /// The longest string, in bytes after unescaping (object keys included).
    pub(super) string: usize,
}

/// Parse `bytes` strictly, without read bounds (the CXF reader).
pub(super) fn parse(bytes: &[u8]) -> Result<Value, ImportErrorKind> {
    parse_bounded(bytes, None)
}

/// Parse `bytes` strictly under `bounds`, when given.
pub(super) fn parse_bounded(
    bytes: &[u8],
    bounds: Option<Bounds>,
) -> Result<Value, ImportErrorKind> {
    if let Err(error) = std::str::from_utf8(bytes) {
        return Err(ImportErrorKind::NotStrictJson(StrictJson::NotUtf8 {
            offset: error.valid_up_to(),
        }));
    }
    let negative_zeros = integer_tokens(bytes).map_err(ImportErrorKind::NotStrictJson)?;
    let mut owned;
    let mut source = bytes;
    if !negative_zeros.is_empty() {
        owned = bytes.to_vec();
        for at in negative_zeros {
            if let Some(byte) = owned.get_mut(at) {
                *byte = b' ';
            }
        }
        source = &owned;
    }
    let failure = RefCell::new(None);
    let seed = Strict {
        failure: &failure,
        bounds,
        depth: 0,
    };
    let mut deserializer = serde_json::Deserializer::from_slice(source);
    let parsed = seed
        .deserialize(&mut deserializer)
        .and_then(|value| deserializer.end().map(|()| value));
    match (parsed, failure.into_inner()) {
        (_, Some(kind)) => Err(kind),
        (Ok(value), None) => Ok(value),
        (Err(error), None) => Err(ImportErrorKind::NotStrictJson(StrictJson::Syntax {
            detail: error.to_string(),
        })),
    }
}

/// Walk the raw bytes outside strings and check every integer-shaped token (`-`? digits, not
/// followed by `.`, `e` or `E`) against the `i64` range. Returns the offsets of the `-` of every
/// `-0` token, whose sign the caller blanks. Not a JSON parser: anything malformed is left for
/// `serde_json` to report with a position.
fn integer_tokens(bytes: &[u8]) -> Result<Vec<usize>, StrictJson> {
    let byte = |at: usize| bytes.get(at).copied();
    let mut negative_zeros = Vec::new();
    let mut at = 0;
    while let Some(current) = byte(at) {
        match current {
            b'"' => {
                at += 1;
                while let Some(inner) = byte(at) {
                    if inner == b'"' {
                        break;
                    }
                    at += if inner == b'\\' { 2 } else { 1 };
                }
                at += 1;
            }
            b'-' | b'0'..=b'9' => {
                let start = at;
                at += usize::from(current == b'-');
                let digits = at;
                while byte(at).is_some_and(|b| b.is_ascii_digit()) {
                    at += 1;
                }
                if matches!(byte(at), Some(b'.' | b'e' | b'E')) {
                    while byte(at).is_some_and(|b| {
                        matches!(b, b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-')
                    }) {
                        at += 1;
                    }
                    continue;
                }
                if at == digits {
                    continue;
                }
                let token = bytes
                    .get(start..at)
                    .and_then(|t| std::str::from_utf8(t).ok())
                    .unwrap_or_default();
                match token.parse::<i64>() {
                    Ok(0) if current == b'-' => negative_zeros.push(start),
                    Ok(_) => {}
                    Err(_) => {
                        return Err(StrictJson::IntegerOutOfRange {
                            token: token.to_owned(),
                        });
                    }
                }
            }
            _ => at += 1,
        }
    }
    Ok(negative_zeros)
}

/// The seed that reads one strict value at `depth` (the number of enclosing objects and arrays).
#[derive(Clone, Copy)]
struct Strict<'f> {
    /// The typed refusal, when the visitor stops on one (the serde error then only unwinds).
    failure: &'f RefCell<Option<ImportErrorKind>>,
    bounds: Option<Bounds>,
    depth: usize,
}

impl Strict<'_> {
    fn refuse<E: de::Error>(&self, kind: ImportErrorKind) -> E {
        let message = kind.to_string();
        self.failure.borrow_mut().get_or_insert(kind);
        E::custom(message)
    }

    fn check_string<E: de::Error>(&self, v: &str) -> Result<(), E> {
        match self.bounds {
            Some(bounds) if v.len() > bounds.string => Err(self.refuse(
                ImportErrorKind::LimitExceeded(Limit::StringLength {
                    limit: bounds.string,
                }),
            )),
            _ => Ok(()),
        }
    }

    /// The seed one level deeper, refusing past the depth bound.
    fn nested<E: de::Error>(&self) -> Result<Self, E> {
        let depth = self.depth + 1;
        match self.bounds {
            Some(bounds) if depth > bounds.depth => {
                Err(self.refuse(ImportErrorKind::LimitExceeded(Limit::Depth {
                    limit: bounds.depth,
                })))
            }
            _ => Ok(Self { depth, ..*self }),
        }
    }
}

impl<'de> DeserializeSeed<'de> for Strict<'_> {
    type Value = Value;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Strict<'_> {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a JSON value")
    }

    fn visit_bool<E>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }

    fn visit_i64<E>(self, v: i64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Value, E> {
        i64::try_from(v)
            .map(|i| Value::Number(i.into()))
            .map_err(|_| {
                self.refuse(ImportErrorKind::NotStrictJson(
                    StrictJson::IntegerOutOfRange {
                        token: v.to_string(),
                    },
                ))
            })
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Value, E> {
        Number::from_f64(v).map(Value::Number).ok_or_else(|| {
            self.refuse(ImportErrorKind::NotStrictJson(StrictJson::Syntax {
                detail: "a number that is not finite".to_owned(),
            }))
        })
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Value, E> {
        self.check_string(v)?;
        Ok(Value::String(v.to_owned()))
    }

    fn visit_string<E: de::Error>(self, v: String) -> Result<Value, E> {
        self.check_string(&v)?;
        Ok(Value::String(v))
    }

    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let inner = self.nested()?;
        let mut items = Vec::new();
        while let Some(item) = seq.next_element_seed(inner)? {
            if let Some(bounds) = self.bounds
                && items.len() >= bounds.list
            {
                return Err(
                    self.refuse(ImportErrorKind::LimitExceeded(Limit::ListLength {
                        limit: bounds.list,
                    })),
                );
            }
            items.push(item);
        }
        Ok(Value::Array(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let inner = self.nested()?;
        let mut object = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            self.check_string(&key)?;
            if object.contains_key(&key) {
                return Err(self.refuse(ImportErrorKind::NotStrictJson(
                    StrictJson::DuplicateKey { key },
                )));
            }
            let value = map.next_value_seed(inner)?;
            object.insert(key, value);
        }
        Ok(Value::Object(object))
    }
}
