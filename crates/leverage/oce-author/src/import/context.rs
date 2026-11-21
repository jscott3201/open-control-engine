//! The document `@context` the CXF importer reads: the closed set of bindings it accepts, and the
//! expansion of a unit IRI against them.
//!
//! The importer reads compact keys only, so its `@context` is closed. It may bind:
//!
//! - **`S231`** to the one namespace the engine reads the vocabulary in ([`S231_IRI`]). `S231`
//!   bound to any other IRI, and any term bound to the namespace written through release 0.4.0, is
//!   [`Kind::NamespaceMismatch`] naming the IRI found and the IRI expected (GitHub #527).
//! - **`base`**, the corpus's inert term, to any string. Nothing reads it: no key and no
//!   identifier is expanded through it.
//! - **`qudt`** to either QUDT schema namespace the engine reads `hasUnit` in
//!   ([`oce_api::QUDT_SCHEMA_NAMESPACES`]: `#` as modelica-json writes it, `/` as QUDT and
//!   CXF-Core do), and **`unit`** to either QUDT unit namespace
//!   ([`oce_api::QUDT_UNIT_NAMESPACES`]) (issue #631; unit-spellings note, decision 6).
//!
//! Any other term, and `qudt` or `unit` bound to anything else, is [`Kind::UnknownKey`] naming
//! the term. The engine reads any prefix map whose bindings are absolute IRIs, and the QUDT key
//! and a unit identifier through whatever prefix expands to them; this reader takes the two
//! prefix names modelica-json and CXF-Core write.
//!
//! The compact key `qudt:hasUnit` is the QUDT schema property only where the document binds
//! `qudt` so ([`Context::reads_qudt`]). Without the binding it is no property the importer knows,
//! and is `unknown-key` on every node. The engine refuses it as well (issue #675), as it refuses
//! `hasUnit` in a near miss of a QUDT namespace, which here is a `qudt` binding outside the closed
//! set or a full-IRI key: neither reader leaves a declared unit unread.
//!
//! [`Context::unit_of`] reads a unit IRI node's `@id`: a compact IRI through `S231`, `qudt` or
//! `unit` where the document binds the prefix (never one whose suffix starts with `//`, which
//! JSON-LD reads as an IRI), and any other identifier as written; then the engine's lookup,
//! [`oce_api::unit_of_iri`]. Each of the three prefixes can only be bound to a namespace the
//! engine accepts under that name, so every identifier this maps, the engine maps to the same
//! unit string. The engine maps more: an identifier through `base` or a whole-token `@context`
//! term, which this reader leaves unexpanded and so refuses. `base` may be any string here, an
//! IRI or not, and must never be able to turn an identifier into a unit.

use std::borrow::Cow;

use serde_json::{Map, Value};

use super::error::{ImportError, ImportErrorKind as Kind};
use super::nodes::{RETIRED_S231_IRI, S231_IRI, refuse};

/// The bindings of one document's `@context`, each accepted by [`Context::read`].
#[derive(Debug, Default)]
pub(super) struct Context<'a> {
    /// `(term, IRI)` in term order.
    bindings: Vec<(&'a str, &'a str)>,
}

impl<'a> Context<'a> {
    /// Read the `@context` of the document object `top`, refusing a top-level key other than
    /// `@context` and `@graph`, a `@context` that is not one inline map, and any binding outside
    /// the closed set (see the module docs). A document with no `@context` binds nothing.
    pub(super) fn read(top: &'a Map<String, Value>) -> Result<Self, ImportError> {
        if let Some(key) = top.keys().find(|k| *k != "@context" && *k != "@graph") {
            return Err(refuse("document", Kind::UnknownKey { key: key.clone() }));
        }
        let context = match top.get("@context") {
            None => return Ok(Self::default()),
            Some(Value::Object(context)) => context,
            Some(_) => {
                return Err(refuse(
                    "document",
                    Kind::WrongShape {
                        key: "@context".to_owned(),
                    },
                ));
            }
        };
        let mut bindings = Vec::with_capacity(context.len());
        for (term, value) in context {
            let iri = match (term.as_str(), value) {
                ("S231", Value::String(iri)) if iri == S231_IRI => iri,
                ("S231", Value::String(iri)) => return Err(wrong_namespace(term, iri)),
                (_, Value::String(iri)) if iri == RETIRED_S231_IRI => {
                    return Err(wrong_namespace(term, iri));
                }
                ("base", Value::String(iri)) => iri,
                ("qudt", Value::String(iri))
                    if oce_api::QUDT_SCHEMA_NAMESPACES.contains(&iri.as_str()) =>
                {
                    iri
                }
                ("unit", Value::String(iri))
                    if oce_api::QUDT_UNIT_NAMESPACES.contains(&iri.as_str()) =>
                {
                    iri
                }
                _ => return Err(refuse("document", Kind::UnknownKey { key: term.clone() })),
            };
            bindings.push((term.as_str(), iri.as_str()));
        }
        Ok(Self { bindings })
    }

    /// The IRI `term` is bound to, if the document binds it.
    fn binding(&self, term: &str) -> Option<&'a str> {
        self.bindings
            .iter()
            .find(|(bound, _)| *bound == term)
            .map(|(_, iri)| *iri)
    }

    /// Whether the compact key `qudt:hasUnit` names the QUDT schema property `hasUnit` in this
    /// document: whether it binds `qudt`, which [`Context::read`] accepts only bound to a QUDT
    /// schema namespace.
    pub(super) fn reads_qudt(&self) -> bool {
        self.binding("qudt").is_some()
    }

    /// `token` expanded: for a compact IRI through `S231`, `qudt` or `unit` whose suffix does
    /// not start with `//`, the binding of its prefix followed by the suffix, where the document
    /// binds the prefix; else `token` as written. `base` is never expanded through, and no
    /// whole-token term is matched (see the module docs).
    fn expand<'t>(&self, token: &'t str) -> Cow<'t, str> {
        match token.split_once(':') {
            Some((prefix @ ("S231" | "qudt" | "unit"), suffix)) if !suffix.starts_with("//") => {
                match self.binding(prefix) {
                    Some(iri) => Cow::Owned(format!("{iri}{suffix}")),
                    None => Cow::Borrowed(token),
                }
            }
            _ => Cow::Borrowed(token),
        }
    }

    /// The unit string the unit IRI node `@id` `id` stands for, or `None` when the engine's
    /// closed unit table does not map it: [`oce_api::unit_of_iri`] of the expanded identifier.
    /// A relative reference, an unbound prefix and an identifier through `base` expand to
    /// themselves and map to nothing. The string returned is never blank and carries no
    /// whitespace and no format or control character: the lookup returns none that does.
    pub(super) fn unit_of(&self, id: &str) -> Option<String> {
        oce_api::unit_of_iri(&self.expand(id)).map(str::to_owned)
    }
}

/// The refusal for a `@context` term that spells the S231 vocabulary in another namespace.
fn wrong_namespace(term: &str, found: &str) -> ImportError {
    refuse(
        "document",
        Kind::NamespaceMismatch {
            term: term.to_owned(),
            found: found.to_owned(),
            expected: S231_IRI,
        },
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const UNIT_HASH: &str = "http://qudt.org/vocab/unit#";
    const UNIT_SLASH: &str = "http://qudt.org/vocab/unit/";

    fn context(value: &Value) -> Result<Vec<(String, String)>, ImportError> {
        let top = value.as_object().expect("an object");
        Context::read(top).map(|context| {
            context
                .bindings
                .iter()
                .map(|(term, iri)| ((*term).to_owned(), (*iri).to_owned()))
                .collect()
        })
    }

    fn unit(bindings: &Value, id: &str) -> Option<String> {
        let top = json!({ "@context": bindings, "@graph": [] });
        Context::read(top.as_object().expect("an object"))
            .expect("the context is accepted")
            .unit_of(id)
    }

    #[test]
    fn the_accepted_bindings_are_the_closed_set() {
        let top = json!({ "@context": {
            "S231": S231_IRI, "base": "anything", "qudt": "http://qudt.org/schema/qudt/",
            "unit": UNIT_HASH
        }, "@graph": [] });
        assert_eq!(context(&top).expect("accepted").len(), 4);
        for (term, iri) in [
            ("qudt", "http://qudt.org/schema/qudt"),
            ("qudt", "https://qudt.org/schema/qudt#"),
            ("qudt", UNIT_HASH),
            ("unit", "http://qudt.org/schema/qudt#"),
            ("unit", "http://example.org/unit#"),
            ("unit", "http://qudt.org/vocab/unit"),
            ("q", "http://qudt.org/vocab/quantitykind#"),
            ("u", UNIT_HASH),
            ("ex", "http://example.org#"),
        ] {
            let top = json!({ "@context": { term: iri }, "@graph": [] });
            let error = context(&top).expect_err("refused");
            assert_eq!(error.subject, "document", "{term} {iri}");
            assert_eq!(
                error.kind,
                Kind::UnknownKey {
                    key: term.to_owned()
                },
                "{term} {iri}"
            );
        }
        for term in ["qudt", "unit"] {
            let top = json!({ "@context": { term: 5 }, "@graph": [] });
            assert_eq!(
                context(&top).expect_err("refused").kind,
                Kind::UnknownKey {
                    key: term.to_owned()
                }
            );
            let retired = json!({ "@context": { term: RETIRED_S231_IRI }, "@graph": [] });
            assert_eq!(
                context(&retired).expect_err("refused").kind.code(),
                "namespace-mismatch"
            );
        }
    }

    #[test]
    fn the_qudt_key_is_read_only_under_a_qudt_binding() {
        for namespace in oce_api::QUDT_SCHEMA_NAMESPACES {
            let top = json!({ "@context": { "qudt": namespace }, "@graph": [] });
            assert!(
                Context::read(top.as_object().expect("an object"))
                    .expect("accepted")
                    .reads_qudt()
            );
        }
        for bindings in [json!({}), json!({ "S231": S231_IRI, "unit": UNIT_HASH })] {
            let top = json!({ "@context": bindings, "@graph": [] });
            assert!(
                !Context::read(top.as_object().expect("an object"))
                    .expect("accepted")
                    .reads_qudt()
            );
        }
        let none = json!({ "@graph": [] });
        assert!(
            !Context::read(none.as_object().expect("an object"))
                .expect("accepted")
                .reads_qudt()
        );
    }

    #[test]
    fn a_unit_identifier_expands_as_the_engine_expands_one() {
        let upstream = json!({ "S231": S231_IRI, "unit": UNIT_HASH });
        let k = Some("K".to_owned());
        // A compact IRI through a bound prefix, in either unit namespace, and a full IRI.
        assert_eq!(unit(&upstream, "unit:K"), k);
        assert_eq!(unit(&json!({ "unit": UNIT_SLASH }), "unit:K"), k);
        assert_eq!(unit(&json!({}), "http://qudt.org/vocab/unit#K"), k);
        assert_eq!(unit(&json!({}), "http://qudt.org/vocab/unit/K"), k);
        // The S231 fallback, compact and full.
        assert_eq!(unit(&upstream, "S231:1/s"), Some("1/s".to_owned()));
        assert_eq!(
            unit(&json!({}), "http://data.ashrae.org/S231#s/K"),
            Some("s/K".to_owned())
        );
        // `base` is inert, whatever it binds: an IRI, a fragment of one, or a string that ends
        // in a character no reader can see. Nor is a whole-token term matched.
        for (base, id) in [
            ("http://qudt.org/vocab/unit#K", "base"),
            (UNIT_HASH, "base:K"),
            ("h", "base:ttp://qudt.org/vocab/unit#K"),
            ("http://data.ashrae.org/S231#K\u{200B}", "base"),
            ("http://data.ashrae.org/S231#", "base:K\u{200B}"),
            ("http://data.ashrae.org/S231#", "base:K"),
        ] {
            assert_eq!(unit(&json!({ "base": base }), id), None, "{base:?} {id:?}");
        }
        for id in ["S231", "qudt", "unit"] {
            assert_eq!(unit(&upstream, id), None, "{id}");
        }
        // Unbound prefixes and relative references expand to themselves and map to nothing.
        for id in ["unit:K", "S231:1/s", "K", "degC", "u:K", "", ":"] {
            assert_eq!(unit(&json!({}), id), None, "{id:?}");
        }
        // A suffix that starts with `//` is never a compact IRI, so `S231://K` is not the
        // fallback `//K`.
        assert_eq!(unit(&upstream, "S231://K"), None);
        assert_eq!(unit(&upstream, "unit://K"), None);
        // What the table lacks, in a namespace it reads.
        for id in [
            "unit:M3-PER-MIN",
            "unit:IN_H2O_60DEG_F",
            "unit:FOO",
            "unit:k",
            "unit:",
        ] {
            assert_eq!(unit(&upstream, id), None, "{id}");
        }
        // A whitespace-bearing fallback is no IRI.
        for id in ["S231:s K", "http://data.ashrae.org/S231# K", "S231:"] {
            assert_eq!(unit(&upstream, id), None, "{id:?}");
        }
        // The lookup refuses a format or control character in a fallback, so no identifier
        // reads as a unit that carries one.
        for id in ["S231:K\u{200B}", "http://data.ashrae.org/S231#K\u{FEFF}"] {
            assert_eq!(unit(&upstream, id), None, "{id:?}");
        }
    }
}
