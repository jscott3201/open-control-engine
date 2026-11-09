//! A JSON reader that refuses what `serde_json::Value` would silently resolve: a key written
//! twice in one object (where `serde_json` keeps the last value) and an integer token outside the
//! `i64` range (where it keeps a `u64`, or reads a longer token as a float). Integer range is
//! enforced lexically before parsing: every token outside a string that is an optional `-` and
//! digits with no fraction or exponent must parse as an `i64`, at any depth. The token `-0` is the
//! Integer zero, as in the wire form (`serde_json` would read it as the Real `-0.0`). Nesting is
//! bounded by `serde_json`'s own recursion limit (128), which it reports as an error, never a
//! stack overflow.

use std::fmt;

use serde::Deserializer;
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};

/// Parse `bytes` strictly; the error is a message for `ImportError::Malformed`.
pub(super) fn parse(bytes: &[u8]) -> Result<Value, String> {
    let negative_zeros = integer_tokens(bytes)?;
    let mut owned;
    let mut source = bytes;
    if !negative_zeros.is_empty() {
        owned = bytes.to_vec();
        for at in negative_zeros {
            owned[at] = b' ';
        }
        source = &owned;
    }
    let mut deserializer = serde_json::Deserializer::from_slice(source);
    let value = Strict
        .deserialize(&mut deserializer)
        .map_err(|e| e.to_string())?;
    deserializer.end().map_err(|e| e.to_string())?;
    Ok(value)
}

/// Walk the raw bytes outside strings and check every integer-shaped token (`-`? digits, not
/// followed by `.`, `e` or `E`) against the `i64` range. Returns the offsets of the `-` of every
/// `-0` token, whose sign the caller blanks. Not a JSON parser: anything malformed is left for
/// `serde_json` to report with a position.
fn integer_tokens(bytes: &[u8]) -> Result<Vec<usize>, String> {
    let mut negative_zeros = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'"' => {
                at += 1;
                while at < bytes.len() && bytes[at] != b'"' {
                    at += if bytes[at] == b'\\' { 2 } else { 1 };
                }
                at += 1;
            }
            b'-' | b'0'..=b'9' => {
                let start = at;
                at += usize::from(bytes[at] == b'-');
                let digits = at;
                while at < bytes.len() && bytes[at].is_ascii_digit() {
                    at += 1;
                }
                let fraction = matches!(bytes.get(at), Some(b'.' | b'e' | b'E'));
                if fraction {
                    while at < bytes.len()
                        && matches!(bytes[at], b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-')
                    {
                        at += 1;
                    }
                    continue;
                }
                if at == digits {
                    continue;
                }
                let token = std::str::from_utf8(&bytes[start..at]).unwrap_or_default();
                match token.parse::<i64>() {
                    Ok(0) if bytes[start] == b'-' => negative_zeros.push(start),
                    Ok(_) => {}
                    Err(_) => {
                        return Err(format!(
                            "the integer {token} at byte {start} is outside the i64 range"
                        ));
                    }
                }
            }
            _ => at += 1,
        }
    }
    Ok(negative_zeros)
}

/// The seed that reads one strict value.
#[derive(Clone, Copy)]
struct Strict;

impl<'de> DeserializeSeed<'de> for Strict {
    type Value = Value;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Strict {
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
            .map_err(|_| E::custom(format!("the integer {v} is outside the i64 range")))
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Value, E> {
        Number::from_f64(v)
            .map(Value::Number)
            .ok_or_else(|| E::custom("a number that is not finite"))
    }

    fn visit_str<E>(self, v: &str) -> Result<Value, E> {
        Ok(Value::String(v.to_owned()))
    }

    fn visit_string<E>(self, v: String) -> Result<Value, E> {
        Ok(Value::String(v))
    }

    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = seq.next_element_seed(Strict)? {
            items.push(item);
        }
        Ok(Value::Array(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut object = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if object.contains_key(&key) {
                return Err(de::Error::custom(format!(
                    "the key `{key}` is written twice"
                )));
            }
            let value = map.next_value_seed(Strict)?;
            object.insert(key, value);
        }
        Ok(Value::Object(object))
    }
}
