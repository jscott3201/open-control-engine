//! A `hasUnit` key the QUDT schema reading does not reach, and its refusal (issue #675; owner
//! ruling of 2026-11-20).
//!
//! modelica-json and `CXF-Core.jsonld` write a declared unit as `qudt:hasUnit`. The engine reads
//! that key as `unit` by its IRI after `@context` expansion, in the two QUDT schema namespaces
//! ([`QUDT_SCHEMA_NAMESPACES`]) and nowhere else ([`super::property_keys`]). A document one edit
//! from that still plainly declares a unit, and a key left in the pass-through map would drop it
//! without a word: the §7.10 unit gate, the host's point list, the content id and the export
//! would all go without it, and a contradicting unit beside it would load as if it were alone. So
//! such a key is refused, never passed through. It is matched on the expanded IRI, like the key it
//! misses, so a compact key, a full IRI and a `@context` term that expand to one IRI are one case:
//!
//! - **No `qudt` binding.** The expanded IRI is `qudt:hasUnit` itself: the compact key under a
//!   `qudt` prefix the document's `@context` binds nowhere, which JSON-LD reads as an IRI of the
//!   scheme `qudt`. It can only be a missing binding. Refused as `malformed-document`.
//! - **A near miss of a QUDT schema namespace.** The expanded IRI is `hasUnit` in a spelling of
//!   either schema namespace that [`namespace_near_miss`] reads as one: `https:`, the scheme or the
//!   host in another letter case, a default port, or a missing, repeated or wrong separator
//!   (`https://qudt.org/schema/qudt#hasUnit`, `http://qudt.org/schema/qudthasUnit`). Refused as
//!   `vocabulary-namespace-mismatch`, as a modeled S231 term keyed in a foreign namespace is.
//! - **The QUDT unit vocabulary.** The expanded IRI is `hasUnit` in either unit namespace
//!   ([`QUDT_UNIT_NAMESPACES`]), or in a near miss of one: the namespace of the units themselves,
//!   where a document that binds `qudt` to the `unit:` IRI puts the property. Refused as
//!   `vocabulary-namespace-mismatch` too.
//!
//! A key that is one of these only once its surrounding whitespace is trimmed (`"qudt:hasUnit "`
//! with no `qudt` binding) is refused as `malformed-document`, as a whitespace near miss of a
//! modeled key is.
//!
//! No other key is refused here. `hasUnit` under a prefix bound to a namespace that is no QUDT one
//! and no near miss of one (`http://example.org/qudt#`, `http://www.qudt.org/schema/qudt#`, a
//! path in another case, a versioned path such as `http://qudt.org/2.1/schema/qudt#`) is that
//! vocabulary's property and passes through. So does a key that differs in its local name
//! (`qudt:HasUnit`, `qudt:hasUnits`) or its prefix (`QUDT:hasUnit` unbound), and
//! `hasQuantityKind`, which is not read under any binding.
//!
//! A node that carries a `@context` of its own is not judged: the scoped context is refused for
//! that node (`non-subset-construct`), and what a key would mean under it is not this engine's
//! to say, so no second refusal claims the document binds no `qudt`.
//!
//! Every key of every `@graph` node comes through [`unread_has_unit`], and only one that ends in
//! `hasUnit` can be refused, so any other is ruled out by that one comparison: it costs no
//! near-miss check and no allocation.
//!
//! Every message quotes authored text (the key, the namespace as written) as an excerpt of at
//! most 64 bytes with each format or control character escaped ([`visible`] of [`excerpt`]), and
//! holds no other text of the document's, so a document sets neither the size of a message nor
//! the characters in it. The node is the subject, and is not repeated in the message.

use oce_diag::{DiagCode, Diagnostic};

use super::property_keys::QUDT_HAS_UNIT;
use crate::namespace_near_miss::namespace_near_miss;
use crate::unit_iri::{QUDT_SCHEMA_NAMESPACES, QUDT_UNIT_NAMESPACES};
use oce_diag::excerpt;
use oce_diag::visible;

/// The compact key modelica-json and CXF-Core write for a unit, as it stands after expansion in a
/// document that binds no `qudt` prefix.
const UNBOUND_QUDT_HAS_UNIT: &str = "qudt:hasUnit";

/// Why a `hasUnit` key is not the QUDT schema property the engine reads.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum UnreadHasUnit {
    /// The key is `qudt:hasUnit` and the document binds no `qudt` prefix.
    UnboundPrefix,
    /// The key names `hasUnit` in a near miss of a QUDT schema namespace, or in the QUDT unit
    /// vocabulary or a near miss of it. Holds the namespace as the expanded IRI writes it, already
    /// an escaped excerpt, ready for a message.
    Namespace(String),
}

/// How the expanded key `iri` misses the QUDT schema property `hasUnit`, or `None` when it does
/// not: when it is that property, in a schema namespace exactly as written, and when it is some
/// other key.
///
/// An IRI that does not end in `hasUnit` is none of the refused cases, and is ruled out first, by
/// that comparison alone. For one that does, the four QUDT namespaces are asked in turn whether
/// `iri` is a near miss of one. Each of the two namespaces of a vocabulary is a near miss of the
/// other, since they differ in the separator alone, which is how `hasUnit` in a unit namespace
/// exactly is caught here, and why the read spellings are set aside before that.
///
/// Work is one suffix comparison for an IRI that does not end in `hasUnit`, with no near-miss
/// check and no allocation. For one that does it is three more comparisons and at most four
/// near-miss checks, each linear in `iri`; the one allocation, made only for a refused key, is
/// the excerpt of the namespace, at most 64 bytes of the document's text.
pub(super) fn unread_has_unit(iri: &str) -> Option<UnreadHasUnit> {
    if !iri.ends_with(QUDT_HAS_UNIT) {
        return None;
    }
    if iri == UNBOUND_QUDT_HAS_UNIT {
        return Some(UnreadHasUnit::UnboundPrefix);
    }
    let read = QUDT_SCHEMA_NAMESPACES
        .into_iter()
        .any(|namespace| iri.strip_prefix(namespace) == Some(QUDT_HAS_UNIT));
    if read {
        return None;
    }
    let miss = QUDT_SCHEMA_NAMESPACES
        .into_iter()
        .chain(QUDT_UNIT_NAMESPACES)
        .filter_map(|namespace| namespace_near_miss(iri, namespace))
        .find(|miss| miss.local == QUDT_HAS_UNIT)?;
    Some(UnreadHasUnit::Namespace(quoted(miss.written)))
}

/// Authored text as a message quotes it: an excerpt of at most 64 bytes ([`excerpt`]) with each
/// format or control character written as an escape ([`visible`]).
fn quoted(text: &str) -> String {
    visible(&excerpt(text)).into_owned()
}

impl UnreadHasUnit {
    /// The refusal of the key `key`, as written, on the node `subject`: `malformed-document` for
    /// an unbound prefix, `vocabulary-namespace-mismatch` for a namespace.
    pub(super) fn refusal(&self, key: &str, subject: &str) -> Diagnostic {
        let key = quoted(key);
        let [hash, slash] = QUDT_SCHEMA_NAMESPACES;
        match self {
            Self::UnboundPrefix => Diagnostic::error(
                DiagCode::MalformedDocument,
                format!(
                    "property `{key}` reads as `{UNBOUND_QUDT_HAS_UNIT}` under a `qudt` prefix the \
                     document's `@context` does not bind, so it names no property and the unit it \
                     carries would be left unread; bind `qudt` to `{hash}` or `{slash}`, or write \
                     the unit as `S231:unit`"
                ),
            ),
            Self::Namespace(written) => Diagnostic::error(
                DiagCode::VocabularyNamespaceMismatch,
                format!(
                    "property `{key}` names `{QUDT_HAS_UNIT}` in `{written}`; the QUDT schema \
                     property `{QUDT_HAS_UNIT}` is read only in `{hash}` and `{slash}`; write it \
                     in one of them, or write the unit as `S231:unit`"
                ),
            ),
        }
        .with_subject(subject.to_owned())
    }

    /// The refusal of the key `key`, as written, on the node `subject`, where `key` is this case
    /// only once its surrounding whitespace is trimmed: `malformed-document`, as every whitespace
    /// near miss of a key is.
    pub(super) fn whitespace_refusal(&self, key: &str, subject: &str) -> Diagnostic {
        let key = quoted(key);
        let [hash, slash] = QUDT_SCHEMA_NAMESPACES;
        let without = match self {
            Self::UnboundPrefix => format!(
                "it reads as `{UNBOUND_QUDT_HAS_UNIT}` under a `qudt` prefix the document's \
                 `@context` does not bind"
            ),
            Self::Namespace(written) => format!(
                "it names `{QUDT_HAS_UNIT}` in `{written}`, where the QUDT schema property is \
                 not read"
            ),
        };
        Diagnostic::error(
            DiagCode::MalformedDocument,
            format!(
                "property key `{key}` carries surrounding whitespace; without it, {without}; \
                 write the unit as `S231:unit`, or as `{QUDT_HAS_UNIT}` in `{hash}` or `{slash}`"
            ),
        )
        .with_subject(subject.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespace_near_miss::checks_made_by;

    fn namespace(written: &str) -> Option<UnreadHasUnit> {
        Some(UnreadHasUnit::Namespace(written.to_owned()))
    }

    /// A document of one node carrying `keys`, each with the value `0`, under an `ex` prefix.
    fn one_node(keys: impl Iterator<Item = String>) -> Vec<u8> {
        let keys: Vec<String> = keys.map(|key| format!("\"{key}\": 0")).collect();
        format!(
            "{{ \"@context\": {{ \"ex\": \"http://example.org/v#\" }}, \"@graph\": [ \
             {{ \"@id\": \"http://example.org#n\", {} }} ] }}",
            keys.join(", ")
        )
        .into_bytes()
    }

    /// The near-miss checks the expansion pass makes on `bytes`, which it must accept.
    fn checks_to_expand(bytes: &[u8]) -> u64 {
        let document = crate::parse_document(bytes).expect("the document parses");
        let (checks, expanded) =
            checks_made_by(|| crate::expand::expand_document(&document, Some(bytes)));
        expanded.expect("every key is a pass-through key");
        checks
    }

    #[test]
    fn an_iri_that_does_not_end_in_has_unit_costs_no_near_miss_check() {
        let megabyte = format!("https://qudt.org/schema/qudt#{}", "k".repeat(1 << 20));
        for iri in [
            "http://example.org/v#k",
            "http://data.ashrae.org/S231#translationSoftware",
            "https://qudt.org/schema/qudt#hasQuantityKind",
            "https://qudt.org/schema/qudt#hasUnits",
            "https://qudt.org/schema/qudt#hasUnit ",
            "https://qudt.org/schema/qudt#HasUnit",
            "https://qudt.org/schema/qudt#hasunit",
            "https://qudt.org/schema/qudt#",
            "http://qudt.org/vocab/unit#K",
            "qudt:hasQuantityKind",
            "qudt:hasUni",
            "",
            megabyte.as_str(),
        ] {
            assert_eq!(
                checks_made_by(|| unread_has_unit(iri)),
                (0, None),
                "{:?}",
                iri.get(..64).unwrap_or(iri)
            );
        }
        // One that does end in it: none where it is read or is the unbound key, one where the
        // first namespace asked is the one it misses, and all four where it misses none.
        let read = "http://qudt.org/schema/qudt#hasUnit";
        assert_eq!(checks_made_by(|| unread_has_unit(read)), (0, None));
        assert_eq!(
            checks_made_by(|| unread_has_unit("qudt:hasUnit")),
            (0, Some(UnreadHasUnit::UnboundPrefix))
        );
        assert_eq!(
            checks_made_by(|| unread_has_unit("https://qudt.org/schema/qudt#hasUnit")),
            (1, namespace("https://qudt.org/schema/qudt#"))
        );
        assert_eq!(
            checks_made_by(|| unread_has_unit("http://qudt.org/vocab/unit/hasUnit")),
            (3, namespace("http://qudt.org/vocab/unit/"))
        );
        assert_eq!(
            checks_made_by(|| unread_has_unit("http://example.org/v#hasUnit")),
            (4, None)
        );
    }

    #[test]
    fn fifty_thousand_pass_through_keys_on_one_node_cost_no_near_miss_check() {
        // Every key of every node comes through the check, so a node of many keys must not pay
        // for near-miss checks none of its keys can need. Counted, never timed.
        let many = one_node((0..50_000).map(|i| format!("ex:k{i:05}")));
        assert_eq!(checks_to_expand(&many), 0);
        // By full IRI too, and one key that ends in `hasUnit` among them is the only one checked.
        const KEYS: usize = 1_000;
        let one = one_node(
            (0..KEYS)
                .map(|i| format!("http://example.org/v#k{i:05}"))
                .chain(["ex:hasUnit".to_owned()]),
        );
        assert_eq!(checks_to_expand(&one), 4);
        // The control: keys that all end in it are each checked against the four namespaces, so
        // the zero above is the early exit's and not a counter that sees nothing.
        let all = one_node((0..KEYS).map(|i| format!("ex:k{i:05}hasUnit")));
        assert_eq!(checks_to_expand(&all), 4 * KEYS as u64);
    }

    #[test]
    fn the_unbound_compact_key_is_matched_whole_and_in_its_exact_case() {
        assert_eq!(
            unread_has_unit("qudt:hasUnit"),
            Some(UnreadHasUnit::UnboundPrefix)
        );
        for iri in [
            "QUDT:hasUnit",
            "Qudt:hasUnit",
            "qudt:HasUnit",
            "qudt:hasunit",
            "qudt:hasUnits",
            "qudt:hasUnit ",
            " qudt:hasUnit",
            "qudt:hasQuantityKind",
            "qudt:unit",
            "qudt:",
            "q:hasUnit",
            "unit:hasUnit",
            "hasUnit",
            "",
        ] {
            assert_eq!(unread_has_unit(iri), None, "{iri:?}");
        }
    }

    #[test]
    fn has_unit_in_a_near_miss_of_a_schema_namespace_is_unread_naming_the_namespace_written() {
        for written in [
            "https://qudt.org/schema/qudt#",
            "https://qudt.org/schema/qudt/",
            "HTTP://qudt.org/schema/qudt#",
            "http://QUDT.org/schema/qudt#",
            "http://QUDT.ORG/schema/qudt/",
            "http://qudt.org:80/schema/qudt#",
            "http://qudt.org:/schema/qudt/",
            "https://qudt.org:443/schema/qudt#",
            "http://qudt.org/schema/qudt/#",
            "http://qudt.org/schema/qudt#/",
            "http://qudt.org/schema/qudt##",
            "http://qudt.org/schema/qudt",
        ] {
            assert_eq!(
                unread_has_unit(&format!("{written}hasUnit")),
                namespace(written),
                "{written}"
            );
        }
    }

    #[test]
    fn has_unit_in_the_unit_vocabulary_or_a_near_miss_of_it_is_unread() {
        for written in [
            "http://qudt.org/vocab/unit#",
            "http://qudt.org/vocab/unit/",
            "https://qudt.org/vocab/unit#",
            "HTTP://qudt.org/vocab/unit/",
            "http://Qudt.org/vocab/unit#",
            "http://qudt.org:80/vocab/unit#",
            "http://qudt.org/vocab/unit",
            "http://qudt.org/vocab/unit/#",
        ] {
            assert_eq!(
                unread_has_unit(&format!("{written}hasUnit")),
                namespace(written),
                "{written}"
            );
        }
    }

    #[test]
    fn the_read_spellings_and_every_other_key_are_not_this_case() {
        for iri in [
            // What the engine reads.
            "http://qudt.org/schema/qudt#hasUnit",
            "http://qudt.org/schema/qudt/hasUnit",
            "http://data.ashrae.org/S231#hasUnit",
            "http://data.ashrae.org/S231#unit",
            // Another local name in a namespace this would refuse `hasUnit` in.
            "https://qudt.org/schema/qudt#hasQuantityKind",
            "https://qudt.org/schema/qudt#HasUnit",
            "https://qudt.org/schema/qudt#hasunit",
            "https://qudt.org/schema/qudt#hasUnits",
            "https://qudt.org/schema/qudt#hasUnit ",
            "https://qudt.org/schema/qudt#",
            "http://qudt.org/schema/qudt",
            "http://qudt.org/vocab/unit#K",
            "http://qudt.org/vocab/unit#hasunit",
            "http://qudt.org/schema/qudtXhasUnit",
            // `hasUnit` in a namespace that is no QUDT one and no near miss of one: another
            // host, a path that only begins like a QUDT one, a versioned path.
            "http://www.qudt.org/schema/qudt#hasUnit",
            "http://qudt.org/schema/qudt-extra#hasUnit",
            "http://qudt.org/schema/qudtX/hasUnit",
            "http://qudt.org/vocab/units#hasUnit",
            "http://qudt.org/2.1/schema/qudt#hasUnit",
            "http://qudt.org/schema/qudt/2.1/hasUnit",
            "http://qudt.org/2.1/vocab/unit#hasUnit",
            "http://example.org/qudt#hasUnit",
            "http://example.org/schema/qudt#hasUnit",
            "http://qudt.org.example.org/schema/qudt#hasUnit",
            "http://qudt.org/schema/QUDT#hasUnit",
            "http://qudt.org/schema#hasUnit",
            "http://qudt.org/vocab/quantitykind#hasUnit",
            "http://qudt.org/vocab/Unit#hasUnit",
            "http://qudt.org:8080/schema/qudt#hasUnit",
            "http://user@qudt.org/schema/qudt#hasUnit",
            "ftp://qudt.org/schema/qudt#hasUnit",
            "https://data.ashrae.org/S231#hasUnit",
        ] {
            assert_eq!(unread_has_unit(iri), None, "{iri:?}");
        }
    }

    #[test]
    fn a_megabyte_of_namespace_and_of_key_is_quoted_in_at_most_64_bytes_each() {
        // A megabyte of separators: the namespace is cut to 64 bytes and marked.
        let iri = format!("http://qudt.org/schema/qudt{}hasUnit", "/#".repeat(1 << 19));
        let Some(UnreadHasUnit::Namespace(written)) = unread_has_unit(&iri) else {
            panic!("a near miss");
        };
        // 27 bytes of scheme, host and path, then 37 of separators: 18 pairs and a `/`.
        assert_eq!(
            written,
            format!("http://qudt.org/schema/qudt{}/…", "/#".repeat(18))
        );
        assert_eq!(written.len(), 64 + "…".len());
        // The megabyte key is quoted in 64 bytes by both refusals.
        for refusal in [
            UnreadHasUnit::UnboundPrefix.refusal(&iri, "n"),
            UnreadHasUnit::UnboundPrefix.whitespace_refusal(&iri, "n"),
            UnreadHasUnit::Namespace(written.clone()).refusal(&iri, "n"),
            UnreadHasUnit::Namespace(written.clone()).whitespace_refusal(&iri, "n"),
        ] {
            assert!(refusal.message.len() < 512, "{}", refusal.message.len());
        }
    }

    #[test]
    fn a_format_or_control_character_in_the_key_is_written_as_an_escape() {
        // A `@context` term may be spelled with one, so a key may carry it.
        for unread in [
            UnreadHasUnit::UnboundPrefix,
            UnreadHasUnit::Namespace("https://qudt.org/schema/qudt#".to_owned()),
        ] {
            for refusal in [
                unread.refusal("q\u{7}\u{202E}u", "n"),
                unread.whitespace_refusal("q\u{7}\u{202E}u\t", "n"),
            ] {
                assert!(
                    refusal.message.contains("q\\u{0007}\\u{202E}u"),
                    "{}",
                    refusal.message
                );
                assert!(
                    !refusal.message.contains(oce_diag::is_format_or_control),
                    "{:?}",
                    refusal.message
                );
            }
        }
    }
}
