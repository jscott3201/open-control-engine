//! A JSON reader that refuses what `serde_json::Value` would silently resolve: a key written
//! twice in one object (where `serde_json` keeps the last value) and an integer token outside the
//! `i64` range (where it keeps a `u64` the model cannot hold). Nesting is bounded by
//! `serde_json`'s own recursion limit (128), which it reports as an error, never a stack overflow.

use std::fmt;

use serde::Deserializer;
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};

/// Parse `bytes` strictly.
pub(super) fn parse(bytes: &[u8]) -> Result<Value, serde_json::Error> {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let value = Strict.deserialize(&mut deserializer)?;
    deserializer.end()?;
    Ok(value)
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
