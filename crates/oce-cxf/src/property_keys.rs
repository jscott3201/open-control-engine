//! Normalization of modeled `S231:` property keys written in a non-compact spelling (issue #383).
//!
//! The Layer-A [`Node`] DTO matches its modeled properties by the literal compact key
//! (`#[serde(rename = "S231:min")]`, …). JSON-LD lets a document write the same property as its
//! full IRI (`"http://data.ashrae.org/S231P#min"`), through another prefix bound to the same
//! namespace (`"s231:min"`), or as a declared `@context` term. Those spellings used to land in the
//! opaque pass-through map, so the value was neither validated nor enforced nor exported — a
//! silent drop.
//!
//! This pass runs on the resolver's working clone, before scoped-context refusal and identity
//! expansion, so every later stage sees one spelling:
//!
//! - **The document's own namespace normalizes.** Each pass-through key is expanded against the
//!   document `@context` with the same [`super::expand_token`] the identity slots use. A key that
//!   expands to `<own namespace><term>` for a term the DTO models ([`MODELED_TERMS`]) moves into
//!   that modeled field, read through the field's own serde shape. The own namespace is the
//!   document's `S231` binding. A document that binds no `S231` has compact keys with no IRI at
//!   all; it is read in [`crate::export::S231_CONTEXT_IRI`] (S231P), the namespace the exporter
//!   binds, so S231P full IRIs normalize there. Compact `S231:` keys keep their meaning whatever
//!   the binding.
//! - **Any other S231 namespace is refused** (`non-subset-construct`), in both directions. The
//!   known namespaces are S231P (`…/S231P#`, the exporter's) and S231 (`…/S231#`, what upstream
//!   modelica-json emits), plus their `https:` near-miss spellings ([`KNOWN_NAMESPACES`]). A
//!   modeled term in one of them other than the document's own is not literally the document's
//!   property, and passing it through would drop it silently again; the message names the
//!   namespace the document reads modeled keys in.
//! - **Two spellings of one property on one node** — compact plus expanded, or two expanded — are
//!   refused (`malformed-document`) rather than one being chosen. A compact key the DTO cannot
//!   represent (an explicit `null` or `[]`) counts: it is read from the authored JSON.
//! - **Any key written twice on one node** — a repeated expanded spelling, or a repeated
//!   pass-through key — is refused (`malformed-document`): JSON parsing keeps one of the values
//!   silently. (A repeated compact modeled key is already a parse error.)
//! - **Whitespace near misses** — a key that names a modeled property only after trimming
//!   surrounding whitespace (`"S231:min "`) — are refused (`malformed-document`).
//! - A value that does not fit the modeled field's shape is refused (`malformed-document`), the
//!   resolver-side twin of the parse error the compact spelling raises.
//!
//! Every other key — unmodeled `S231:` terms, other vocabularies, keys that do not expand — stays
//! in the pass-through map unchanged. The Layer-A DTO itself is never rewritten: `parse_document`
//! and `write_document` stay lossless (RT-1), and export always writes the compact spelling.

use std::collections::BTreeMap;
use std::fmt;

use oce_diag::{DiagCode, Diagnostic};
use serde::de::{IgnoredAny, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value as Json};

use super::{Expansion, PrefixTable, expand_token};
use crate::dto::{CxfDocument, Node};
use crate::export::S231_CONTEXT_IRI;

/// Every S231 namespace a modeled term can plausibly be spelled in: S231P (the exporter's), S231
/// (upstream modelica-json's), and the `https:` spelling of each. Only the document's own
/// namespace normalizes; a modeled term in any other of these is refused, never passed through.
const KNOWN_NAMESPACES: [&str; 4] = [
    "http://data.ashrae.org/S231P#",
    "http://data.ashrae.org/S231#",
    "https://data.ashrae.org/S231P#",
    "https://data.ashrae.org/S231#",
];

/// Every property term the [`Node`] DTO models as a typed field, i.e. every `S231:<term>` serde
/// rename. A unit test pins this list against the DTO in both directions.
pub(super) const MODELED_TERMS: [&str; 25] = [
    "hasInput",
    "hasOutput",
    "hasParameter",
    "hasConstant",
    "containsBlock",
    "hasInstance",
    "isConnectedTo",
    "isOfDataType",
    "label",
    "accessSpecifier",
    "value",
    "min",
    "max",
    "nominal",
    "unit",
    "quantity",
    "displayUnit",
    "isFinal",
    "isArray",
    "numberDimensions",
    "sizeOfDimensions",
    "hasFmuPath",
    "isConditionalComponent",
    "conditionalExpression",
    "isReplaceable",
];

/// One pass-through key that names a modeled property: the authored key and the modeled term.
struct Alias {
    key: String,
    term: &'static str,
}

/// One `@graph` node's keys in authored order, repeats kept; values are skipped, not built.
struct AuthoredKeys(Vec<String>);

impl<'de> Deserialize<'de> for AuthoredKeys {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct KeyVisitor;
        impl<'de> Visitor<'de> for KeyVisitor {
            type Value = AuthoredKeys;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a JSON object")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<AuthoredKeys, A::Error> {
                let mut keys = Vec::new();
                while let Some(key) = map.next_key::<String>()? {
                    map.next_value::<IgnoredAny>()?;
                    keys.push(key);
                }
                Ok(AuthoredKeys(keys))
            }
        }
        deserializer.deserialize_map(KeyVisitor)
    }
}

/// The key lists of the `@graph` nodes, in `@graph` order.
#[derive(Deserialize)]
struct RawGraph {
    #[serde(rename = "@graph")]
    graph: Vec<AuthoredKeys>,
}

/// Every `@graph` node's authored keys, or `None` without raw JSON. The DTO cannot show either
/// thing this is read for: a key written twice (serde keeps one value) and a compact key whose
/// value it dropped (`null`, `[]`). The scan reads keys only; it never builds a value.
fn authored_keys(raw: Option<&[u8]>) -> Option<Vec<Vec<String>>> {
    let doc = serde_json::from_slice::<RawGraph>(raw?).ok()?;
    Some(doc.graph.into_iter().map(|keys| keys.0).collect())
}

/// Each key `keys` holds more than once, with its count, in key order.
fn repeated_keys(keys: &[String]) -> BTreeMap<&str, usize> {
    let mut counts = BTreeMap::new();
    for key in keys {
        *counts.entry(key.as_str()).or_insert(0_usize) += 1;
    }
    counts.retain(|_, count| *count > 1);
    counts
}

/// Move every non-compact spelling of a modeled property into its typed field, in place.
///
/// `raw` is the authored JSON. With it, a node that writes any key more than once — modeled or
/// pass-through — is refused (`malformed-document`): JSON parsing silently keeps one of the
/// values, and a safety-critical import must not pick one. Diagnostics are appended in `@graph`
/// order, then key order, so the refusal list is deterministic. A node with any refusal keeps its
/// pass-through map untouched; the caller returns the refusals before any later stage reads the
/// document.
pub(super) fn normalize(
    doc: &mut CxfDocument,
    table: &PrefixTable,
    raw: Option<&[u8]>,
    diags: &mut Vec<Diagnostic>,
) {
    let own = table.get("S231").map_or(S231_CONTEXT_IRI, String::as_str);
    let binding = match table.get("S231") {
        Some(iri) => format!("binds `S231` to `{iri}`"),
        None => format!("binds no `S231`, so its modeled keys are read in `{S231_CONTEXT_IRI}`"),
    };
    let authored = authored_keys(raw);
    for (index, node) in doc.graph.iter_mut().enumerate() {
        let subject = if node.id.is_empty() {
            format!("@graph[{index}]")
        } else {
            node.id.clone()
        };
        let keys = authored
            .as_ref()
            .and_then(|nodes| nodes.get(index))
            .map_or(&[][..], Vec::as_slice);
        let repeated = repeated_keys(keys);
        if !repeated.is_empty() {
            for (key, count) in repeated {
                diags.push(malformed(
                    &subject,
                    format!(
                        "node `{subject}` writes the key `{key}` {count} times; a key may appear \
                         once per node, and a repeat would silently keep only one of the values"
                    ),
                ));
            }
            continue;
        }
        let scope = KeyScope {
            own,
            binding: &binding,
            table,
            subject: &subject,
        };
        let aliases = scope.aliases_of(node, diags);
        if !aliases.is_empty() {
            let mut explicit = |key: &str| keys.iter().any(|authored| authored == key);
            rewrite(node, &aliases, &subject, &mut explicit, diags);
        }
    }
}

/// The modeled term an expanded key `iri` names, if any: the whole remainder after `namespace`.
fn modeled_term(iri: &str, namespace: &str) -> Option<&'static str> {
    let term = iri.strip_prefix(namespace)?;
    MODELED_TERMS
        .iter()
        .copied()
        .find(|modeled| *modeled == term)
}

/// What a key means against one document: its own namespace, how it binds `S231`, its prefix
/// table, and the node being read (for messages).
struct KeyScope<'a> {
    own: &'a str,
    binding: &'a str,
    table: &'a PrefixTable,
    subject: &'a str,
}

/// How one pass-through key relates to the modeled properties.
enum KeyMeaning {
    /// A modeled term in the document's own namespace.
    Own(&'static str),
    /// A modeled term in another known S231 namespace.
    Foreign(&'static str, &'static str),
    /// Anything else.
    Unrelated,
}

impl KeyScope<'_> {
    /// Classify `key` (already trimmed by the caller when probing a near miss).
    fn meaning(&self, key: &str) -> KeyMeaning {
        if let Some(term) = key.strip_prefix("S231:").and_then(|t| modeled_term(t, "")) {
            return KeyMeaning::Own(term);
        }
        let iri = match expand_token(key, self.table) {
            Expansion::Expanded(iri) => iri,
            Expansion::Verbatim => key.to_owned(),
            Expansion::Relative => return KeyMeaning::Unrelated,
        };
        if let Some(term) = modeled_term(&iri, self.own) {
            return KeyMeaning::Own(term);
        }
        KNOWN_NAMESPACES
            .iter()
            .find_map(|ns| modeled_term(&iri, ns).map(|term| KeyMeaning::Foreign(term, ns)))
            .unwrap_or(KeyMeaning::Unrelated)
    }

    /// Collect the pass-through keys of `node` that name a modeled property in the document's own
    /// namespace, refusing foreign-namespace and whitespace near-miss spellings.
    fn aliases_of(&self, node: &Node, diags: &mut Vec<Diagnostic>) -> Vec<Alias> {
        let subject = self.subject;
        let mut aliases = Vec::new();
        for key in node.other.keys().filter(|key| !key.starts_with('@')) {
            let trimmed = key.trim();
            if trimmed != key {
                if let KeyMeaning::Own(term) | KeyMeaning::Foreign(term, _) = self.meaning(trimmed)
                {
                    diags.push(malformed(
                        subject,
                        format!(
                            "property key {key:?} on node `{subject}` carries surrounding \
                             whitespace; without it, it names the modeled term `{term}`; write \
                             it as `S231:{term}`"
                        ),
                    ));
                }
                continue;
            }
            match self.meaning(key) {
                KeyMeaning::Own(term) => aliases.push(Alias {
                    key: key.clone(),
                    term,
                }),
                KeyMeaning::Foreign(term, namespace) => diags.push(
                    Diagnostic::error(
                        DiagCode::NonSubsetConstruct,
                        format!(
                            "property `{key}` on node `{subject}` names the modeled term \
                             `{term}` in `{namespace}`, but the document {}; write it as \
                             `S231:{term}`",
                            self.binding
                        ),
                    )
                    .with_subject(subject.to_owned()),
                ),
                KeyMeaning::Unrelated => {}
            }
        }
        aliases
    }
}

/// Rename each alias to its compact key and re-read the node through the DTO, so the value takes
/// exactly the serde path the compact spelling takes. Refuses a second spelling of one property;
/// `explicit` reports whether the node was authored with a key the DTO dropped (`null`, `[]`).
fn rewrite(
    node: &mut Node,
    aliases: &[Alias],
    subject: &str,
    explicit: &mut dyn FnMut(&str) -> bool,
    diags: &mut Vec<Diagnostic>,
) {
    let mut object = match serde_json::to_value(&*node) {
        Ok(Json::Object(object)) => object,
        Ok(_) | Err(_) => {
            diags.push(malformed(
                subject,
                format!("node `{subject}` could not be re-read to normalize its property keys"),
            ));
            return;
        }
    };
    let mut claimed: BTreeMap<&str, &str> = BTreeMap::new();
    let mut duplicate = false;
    for alias in aliases {
        let compact = format!("S231:{}", alias.term);
        let first = if object.contains_key(&compact) || explicit(&compact) {
            Some(compact.as_str())
        } else {
            claimed.get(alias.term).copied()
        };
        if let Some(first) = first {
            duplicate = true;
            diags.push(malformed(
                subject,
                format!(
                    "node `{subject}` writes `S231:{}` twice, as `{first}` and as `{}`; \
                     a property must have exactly one spelling",
                    alias.term, alias.key
                ),
            ));
            continue;
        }
        claimed.insert(alias.term, &alias.key);
    }
    if duplicate {
        return;
    }
    for alias in aliases {
        rename(&mut object, &alias.key, alias.term);
    }
    match serde_json::from_value::<Node>(Json::Object(object)) {
        Ok(normalized) => *node = normalized,
        Err(error) => {
            let keys: Vec<&str> = aliases.iter().map(|alias| alias.key.as_str()).collect();
            diags.push(malformed(
                subject,
                format!(
                    "node `{subject}` carries a modeled property written as `{}` whose value does \
                     not fit the modeled field: {error}",
                    keys.join("`, `")
                ),
            ));
        }
    }
}

/// Move `key`'s value to the compact `S231:<term>` key.
fn rename(object: &mut Map<String, Json>, key: &str, term: &str) {
    if let Some(value) = object.remove(key) {
        object.insert(format!("S231:{term}"), value);
    }
}

/// A `malformed-document` refusal about `subject`.
fn malformed(subject: &str, message: String) -> Diagnostic {
    Diagnostic::error(DiagCode::MalformedDocument, message).with_subject(subject.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a node carrying every modeled term under its compact key.
    fn every_compact_term() -> Json {
        let mut object = Map::new();
        object.insert("@id".into(), Json::from("http://example.org#n"));
        for term in MODELED_TERMS {
            let value = match term {
                "isFinal" | "isArray" | "isConditionalComponent" | "isReplaceable" => {
                    Json::Bool(true)
                }
                "numberDimensions" => Json::from(1),
                "hasInput" | "hasOutput" | "hasParameter" | "hasConstant" | "containsBlock"
                | "hasInstance" | "isConnectedTo" | "isOfDataType" | "displayUnit" => {
                    serde_json::json!({ "@id": "http://example.org#x" })
                }
                _ => Json::from("x"),
            };
            object.insert(format!("S231:{term}"), value);
        }
        Json::Object(object)
    }

    #[test]
    fn modeled_terms_are_exactly_the_typed_node_fields() {
        let json = every_compact_term();
        let node: Node = serde_json::from_value(json.clone()).expect("every term deserializes");
        assert!(
            node.other.is_empty(),
            "a listed term fell into pass-through: {:?}",
            node.other.keys().collect::<Vec<_>>()
        );
        // The reverse direction: every key the DTO can emit besides `@id`/`@type` is listed.
        let emitted = serde_json::to_value(&node).expect("serialize");
        let emitted: Vec<&str> = emitted
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .filter(|key| !key.starts_with('@'))
            .collect();
        assert_eq!(emitted.len(), MODELED_TERMS.len());
        for key in emitted {
            let term = key.strip_prefix("S231:").expect("compact S231 key");
            assert!(MODELED_TERMS.contains(&term), "unlisted modeled key {key}");
        }
    }

    #[test]
    fn modeled_term_matches_only_whole_terms_after_the_namespace() {
        let ns = "http://data.ashrae.org/S231P#";
        assert_eq!(modeled_term(&format!("{ns}min"), ns), Some("min"));
        assert_eq!(modeled_term(&format!("{ns}minimum"), ns), None);
        assert_eq!(modeled_term(&format!("{ns}Min"), ns), None);
        assert_eq!(modeled_term(ns, ns), None);
        assert_eq!(modeled_term("http://example.org#min", ns), None);
        assert_eq!(modeled_term(&format!("{ns}moSource"), ns), None);
    }
}
