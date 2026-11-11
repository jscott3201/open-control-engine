//! The closed-shape adapter between `serde_json` and the wire DTOs (GitHub #544).
//!
//! `serde_json` reads a `serde`-derived struct from a JSON list as well as from an object: the
//! derive reads a list positionally as the struct's fields, so `"semantic": ["", "", "K", …]`
//! would load as the object it abbreviates. It reads a derived enum from `{"Real": null}` as well
//! as from `"Real"`. The wire form defines neither spelling. [`Closed`] wraps the deserializer of
//! every value in the document, at every depth, and narrows the three requests whose `serde_json`
//! implementation accepts more than the wire form defines:
//!
//! - a struct is read only from an object: a list, a string, a number, a boolean or `null` is a
//!   wrong type at the value, naming the object the schema defines there (`expected a
//!   SemanticPoint object`);
//! - an enum is read only from a string naming one of its variants;
//! - an option never reads `null` as absent: the wire form has no nullable value (an absent value
//!   is `""`, `[]` or `false`), so the inner type judges `null` and refuses it.
//!
//! Every other request is forwarded unchanged. The adapter sits at the deserializer rather than at
//! a field, so a DTO added later is closed by construction: every value reached from the root is
//! read through it.
//!
//! The adapter also tracks the path of the value being read (`inputs[0].semantic`, `document` for
//! the root) and keeps the path of the innermost value whose read failed, which
//! [`super::classify`] puts on a [`super::WireError::WrongType`].

use std::cell::RefCell;
use std::fmt::{self, Write as _};

use serde::de::value::StrDeserializer;
use serde::de::{
    self, DeserializeSeed, Deserializer, EnumAccess, IntoDeserializer, MapAccess, SeqAccess,
    Visitor,
};

/// The path of the document root.
pub(crate) const ROOT_PATH: &str = "document";

/// One step from a value into a member: an object key or a list index.
enum Step {
    Key(String),
    Index(usize),
}

/// The path of the value being read, and the path of the innermost value whose read failed.
#[derive(Default)]
pub(super) struct Track {
    steps: RefCell<Vec<Step>>,
    failed: RefCell<Option<String>>,
}

impl Track {
    fn enter(&self, step: Step) {
        self.steps.borrow_mut().push(step);
    }

    /// Leave the member entered last, keeping its path if `result` is the first failure seen.
    fn leave<T, E>(&self, result: Result<T, E>) -> Result<T, E> {
        if result.is_err() {
            let mut failed = self.failed.borrow_mut();
            if failed.is_none() {
                *failed = Some(spell(&self.steps.borrow()));
            }
        }
        self.steps.borrow_mut().pop();
        result
    }

    /// The path of the innermost value whose read failed; the root when no member's read failed.
    pub(super) fn failed_path(&self) -> String {
        self.failed
            .borrow()
            .clone()
            .unwrap_or_else(|| ROOT_PATH.to_owned())
    }
}

/// `inputs[0].semantic`: keys joined by dots, indexes in brackets, the root for no step.
fn spell(steps: &[Step]) -> String {
    if steps.is_empty() {
        return ROOT_PATH.to_owned();
    }
    let mut path = String::new();
    for step in steps {
        match step {
            Step::Key(key) => {
                if !path.is_empty() {
                    path.push('.');
                }
                path.push_str(key);
            }
            Step::Index(index) => {
                // Writing to a String cannot fail.
                let _ = write!(path, "[{index}]");
            }
        }
    }
    path
}

/// A deserializer that reads structs from objects only and enums from strings only, and keeps
/// every member it hands out closed the same way.
pub(super) struct Closed<'t, D> {
    inner: D,
    track: &'t Track,
}

impl<'t, D> Closed<'t, D> {
    /// Wrap `inner`, recording paths in `track`.
    pub(super) fn new(inner: D, track: &'t Track) -> Self {
        Self { inner, track }
    }

    fn wrap<V>(&self, visitor: V) -> Members<'t, V> {
        Members {
            inner: visitor,
            track: self.track,
        }
    }
}

/// Forward a request that carries only a visitor, keeping the members it yields closed.
macro_rules! forward_requests {
    ($($method:ident)*) => {$(
        fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
            let members = self.wrap(visitor);
            self.inner.$method(members)
        }
    )*};
}

impl<'de, D: Deserializer<'de>> Deserializer<'de> for Closed<'_, D> {
    type Error = D::Error;

    forward_requests! {
        deserialize_any deserialize_bool
        deserialize_i8 deserialize_i16 deserialize_i32 deserialize_i64 deserialize_i128
        deserialize_u8 deserialize_u16 deserialize_u32 deserialize_u64 deserialize_u128
        deserialize_f32 deserialize_f64 deserialize_char deserialize_str deserialize_string
        deserialize_bytes deserialize_byte_buf deserialize_unit deserialize_seq deserialize_map
        deserialize_identifier deserialize_ignored_any
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        // `null` is never "absent" on the wire: the inner type reads the value, `null` included.
        visitor.visit_some(self)
    }

    fn deserialize_unit_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        let members = self.wrap(visitor);
        self.inner.deserialize_unit_struct(name, members)
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        let members = self.wrap(visitor);
        self.inner.deserialize_newtype_struct(name, members)
    }

    fn deserialize_tuple<V: Visitor<'de>>(
        self,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        let members = self.wrap(visitor);
        self.inner.deserialize_tuple(len, members)
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        let members = self.wrap(visitor);
        self.inner.deserialize_tuple_struct(name, len, members)
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        // A map request: `serde_json` refuses anything but `{` for it, where a struct request
        // also takes `[` and reads the list positionally.
        let object = Object {
            inner: self.wrap(visitor),
            name,
        };
        self.inner.deserialize_map(object)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        // A string request: `serde_json` would also read `{"Variant": null}` as the variant.
        self.inner.deserialize_str(Variant {
            inner: visitor,
            variants,
        })
    }

    fn is_human_readable(&self) -> bool {
        self.inner.is_human_readable()
    }
}

/// A visitor whose lists, objects and inner values are read closed.
struct Members<'t, V> {
    inner: V,
    track: &'t Track,
}

/// Forward a visit that carries only a value.
macro_rules! forward_visits {
    ($($method:ident($ty:ty))*) => {$(
        fn $method<E: de::Error>(self, value: $ty) -> Result<Self::Value, E> {
            self.inner.$method(value)
        }
    )*};
}

impl<'de, V: Visitor<'de>> Visitor<'de> for Members<'_, V> {
    type Value = V::Value;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.expecting(f)
    }

    forward_visits! {
        visit_bool(bool)
        visit_i8(i8) visit_i16(i16) visit_i32(i32) visit_i64(i64) visit_i128(i128)
        visit_u8(u8) visit_u16(u16) visit_u32(u32) visit_u64(u64) visit_u128(u128)
        visit_f32(f32) visit_f64(f64) visit_char(char)
        visit_str(&str) visit_borrowed_str(&'de str) visit_string(String)
        visit_bytes(&[u8]) visit_borrowed_bytes(&'de [u8]) visit_byte_buf(Vec<u8>)
    }

    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        self.inner.visit_none()
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        self.inner.visit_unit()
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        self.inner.visit_some(Closed::new(deserializer, self.track))
    }

    fn visit_newtype_struct<D: Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        self.inner
            .visit_newtype_struct(Closed::new(deserializer, self.track))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<Self::Value, A::Error> {
        self.inner.visit_seq(Items {
            inner: seq,
            track: self.track,
            index: 0,
        })
    }

    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
        self.inner.visit_map(Entries {
            inner: map,
            track: self.track,
            key: None,
        })
    }

    fn visit_enum<A: EnumAccess<'de>>(self, data: A) -> Result<Self::Value, A::Error> {
        // `serde_json` visits an enum only from its own enum request, which `Closed` never
        // forwards (it asks for a string instead), so this is not reached from a wire document.
        self.inner.visit_enum(data)
    }
}

/// A struct's visitor, read from an object only and named as the schema names it.
struct Object<'t, V> {
    inner: Members<'t, V>,
    name: &'static str,
}

impl<'de, V: Visitor<'de>> Visitor<'de> for Object<'_, V> {
    type Value = V::Value;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let article = if self.name.starts_with(['A', 'E', 'I', 'O', 'U']) {
            "an"
        } else {
            "a"
        };
        write!(f, "{article} {} object", self.name)
    }

    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
        self.inner.visit_map(map)
    }
}

/// An enum's visitor, read from a string naming a variant only.
struct Variant<V> {
    inner: V,
    variants: &'static [&'static str],
}

impl<'de, V: Visitor<'de>> Visitor<'de> for Variant<V> {
    type Value = V::Value;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("one of the strings ")?;
        for (index, variant) in self.variants.iter().enumerate() {
            if index > 0 {
                f.write_str(", ")?;
            }
            write!(f, "\"{variant}\"")?;
        }
        Ok(())
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        self.inner.visit_enum(value.into_deserializer())
    }
}

/// A list whose items are read closed, each at its index.
struct Items<'t, A> {
    inner: A,
    track: &'t Track,
    index: usize,
}

impl<'de, A: SeqAccess<'de>> SeqAccess<'de> for Items<'_, A> {
    type Error = A::Error;

    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, A::Error> {
        self.track.enter(Step::Index(self.index));
        self.index += 1;
        let read = self.inner.next_element_seed(Member {
            inner: seed,
            track: self.track,
        });
        self.track.leave(read)
    }

    fn size_hint(&self) -> Option<usize> {
        self.inner.size_hint()
    }
}

/// An object whose values are read closed, each under its key.
struct Entries<'t, A> {
    inner: A,
    track: &'t Track,
    key: Option<String>,
}

impl<'de, A: MapAccess<'de>> MapAccess<'de> for Entries<'_, A> {
    type Error = A::Error;

    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, A::Error> {
        // Read the key as text to name the value's path, then hand the same text to the seed.
        let Some(key) = self.inner.next_key::<String>()? else {
            return Ok(None);
        };
        let read = seed.deserialize(StrDeserializer::<A::Error>::new(&key))?;
        self.key = Some(key);
        Ok(Some(read))
    }

    fn next_value_seed<T: DeserializeSeed<'de>>(&mut self, seed: T) -> Result<T::Value, A::Error> {
        self.track
            .enter(Step::Key(self.key.take().unwrap_or_default()));
        let read = self.inner.next_value_seed(Member {
            inner: seed,
            track: self.track,
        });
        self.track.leave(read)
    }

    fn size_hint(&self) -> Option<usize> {
        self.inner.size_hint()
    }
}

/// A member's seed: the member's deserializer is wrapped before the seed reads it.
struct Member<'t, S> {
    inner: S,
    track: &'t Track,
}

impl<'de, S: DeserializeSeed<'de>> DeserializeSeed<'de> for Member<'_, S> {
    type Value = S::Value;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<S::Value, D::Error> {
        self.inner
            .deserialize(Closed::new(deserializer, self.track))
    }
}

#[cfg(test)]
#[path = "closed_tests.rs"]
mod tests;
