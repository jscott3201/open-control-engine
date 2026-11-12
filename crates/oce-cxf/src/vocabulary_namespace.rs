//! The one namespace the importer reads the S231 vocabulary in, and the refusal of every other
//! (GitHub #527).
//!
//! The exporter binds the `S231` prefix to [`S231_CONTEXT_IRI`], the namespace the
//! OpenBuildingControl (OBC) specification lists (OBC §11.3) and modelica-json's `CXF-Core.jsonld`
//! binds. The importer reads the vocabulary in that namespace and in no other:
//!
//! - **`S231` bound to any other IRI is refused**, the `https:` spelling of the same host and
//!   path included: an IRI is compared as a string, and a document under another binding does not
//!   use this vocabulary.
//! - **Any prefix bound to [`RETIRED_NAMESPACE`] is refused.** That is the IRI this engine wrote
//!   through release 0.4.0. A document of that layout is refused, never read: there is no second
//!   spelling, alias or importer for it.
//! - **A document that binds no `S231` prefix** has compact `S231:` keys with no IRI at all; they
//!   are read in [`S231_CONTEXT_IRI`], as before.
//! - **Another prefix bound to [`S231_CONTEXT_IRI`]** (`"s231"`, say) is the same namespace under
//!   another name and stays accepted; [`super::property_keys`] normalizes keys written through it.
//! - **A typing token in [`RETIRED_NAMESPACE`]** (a `@type` value or an `isOfDataType` reference
//!   written as a full IRI) is refused. Types are otherwise read by their trailing term in any
//!   namespace, which is unchanged.
//! - **A typing token in the `https:` spelling of either namespace** is refused, whatever its term
//!   (GitHub #620; the class `ExtensionBlock` since GitHub #582). The vocabulary is read only in
//!   [`S231_CONTEXT_IRI`], the way a modeled key is read, so such a token would be read by its
//!   trailing term, and `ExtensionBlock` as an ordinary class, lowering an extension block that
//!   lists children as a composite. A modeled key in the same namespaces is refused for the same
//!   reason ([`super::property_keys`]). A token in [`S231_CONTEXT_IRI`] must then name a term of the
//!   vocabulary ([`super::vocabulary_terms`]).
//! - **A typing token that names an S231 term only once its surrounding whitespace is trimmed**
//!   (`"S231:ExtensionBlock "`, `" S231:Real"`) is refused as `malformed-document` (GitHub #587),
//!   as a whitespace near-miss property key is ([`super::property_keys`]). No whitespace-bearing
//!   token expands, so it would be kept as written and read as no S231 type at all: a composite
//!   never reads its `@type`, so a root typed `"S231:ExtensionBlock "` with a child loaded and ran
//!   the child. The check reads the trimmed token the way the resolver reads an S231 type (a
//!   literal `S231:` prefix, else the expanded IRI) in [`S231_CONTEXT_IRI`] and in
//!   [`FOREIGN_NAMESPACES`], and replaces the checks above for that token.
//!
//! The check reads the effective prefix table: in a list of context maps a later binding replaces
//! an earlier one (JSON-LD context processing order), so only the binding in force is judged.
//!
//! Every refusal here and the per-key refusal in [`super::property_keys`] carries
//! [`DiagCode::VocabularyNamespaceMismatch`] and a message naming the IRI found and the IRI
//! expected.

use oce_diag::{DiagCode, Diagnostic};

use super::{Expansion, PrefixTable, expand_token};
use crate::excerpt::excerpt;
use crate::export::S231_CONTEXT_IRI;
use crate::resolve::extension_blocks::EXTENSION_BLOCK_TERM;

/// The namespace IRI this engine bound through release 0.4.0. Kept only to refuse it.
pub(super) const RETIRED_NAMESPACE: &str = "http://data.ashrae.org/S231P#";

/// The `https:` spelling of [`S231_CONTEXT_IRI`]: a different IRI, one character from the right
/// one.
const HTTPS_NEAR_MISS: &str = "https://data.ashrae.org/S231#";

/// The `https:` spelling of [`RETIRED_NAMESPACE`]. Nothing wrote it; it is listed so that a
/// modeled key spelled in it keeps the refusal it has had since issue #383.
const HTTPS_RETIRED_NEAR_MISS: &str = "https://data.ashrae.org/S231P#";

/// Namespaces a modeled term may be keyed in by mistake. A modeled term spelled in one of them is
/// refused, never passed through: passing it through would drop the property silently.
pub(super) const FOREIGN_NAMESPACES: [&str; 3] =
    [RETIRED_NAMESPACE, HTTPS_NEAR_MISS, HTTPS_RETIRED_NEAR_MISS];

/// Refuse every `@context` binding that spells the S231 vocabulary in another namespace, in term
/// order (the table is sorted, so the refusal list is deterministic).
pub(super) fn check_bindings(table: &PrefixTable, diags: &mut Vec<Diagnostic>) {
    for (term, iri) in table {
        let refused = if term == "S231" {
            iri != S231_CONTEXT_IRI
        } else {
            iri == RETIRED_NAMESPACE
        };
        if refused {
            diags.push(
                Diagnostic::error(
                    DiagCode::VocabularyNamespaceMismatch,
                    format!(
                        "@context term `{term}` binds `{iri}`; the S231 vocabulary is read only \
                         in `{S231_CONTEXT_IRI}`"
                    ),
                )
                .with_subject(term.clone()),
            );
        }
    }
}

/// Refuse a typing token (`@type` value or `isOfDataType` reference) in [`RETIRED_NAMESPACE`],
/// [`HTTPS_NEAR_MISS`] or [`HTTPS_RETIRED_NEAR_MISS`]. A type is recognised by its trailing term
/// in any namespace, so without this a full IRI in one of them would be read through it as if it
/// were the S231 vocabulary's (GitHub #527, #582; every `https:` term since GitHub #620). The class
/// `ExtensionBlock` keeps its own message. `owner` is the node's expanded `@id`.
pub(super) fn check_typing(token: &str, slot: &str, owner: &str, diags: &mut Vec<Diagnostic>) {
    let subject = if owner.is_empty() { token } else { owner };
    if token.starts_with(RETIRED_NAMESPACE) {
        diags.push(
            Diagnostic::error(
                DiagCode::VocabularyNamespaceMismatch,
                format!(
                    "{slot} `{token}` on node `{owner}` is in `{RETIRED_NAMESPACE}`; the S231 \
                     vocabulary is read only in `{S231_CONTEXT_IRI}`"
                ),
            )
            .with_subject(subject.to_owned()),
        );
        return;
    }
    let Some((namespace, term)) = [HTTPS_NEAR_MISS, HTTPS_RETIRED_NEAR_MISS]
        .into_iter()
        .find_map(|namespace| Some((namespace, token.strip_prefix(namespace)?)))
    else {
        return;
    };
    let message = if slot == "@type" && term == EXTENSION_BLOCK_TERM {
        format!(
            "{slot} `{token}` on node `{owner}` names the class `{EXTENSION_BLOCK_TERM}` in \
             `{namespace}`; the S231 vocabulary is read only in `{S231_CONTEXT_IRI}`; write it as \
             `S231:{EXTENSION_BLOCK_TERM}`"
        )
    } else {
        let (quoted, named) = (excerpt(token), excerpt(term));
        format!(
            "{slot} `{quoted}` on node `{owner}` is in `{namespace}`; the S231 vocabulary is read \
             only in `{S231_CONTEXT_IRI}`; write it as `S231:{named}`"
        )
    };
    diags.push(
        Diagnostic::error(DiagCode::VocabularyNamespaceMismatch, message)
            .with_subject(subject.to_owned()),
    );
}

/// Refuse a typing token (`@type` value or `isOfDataType` reference, as written) that names a
/// term in the S231 vocabulary's namespace, or in one of [`FOREIGN_NAMESPACES`], only once its
/// surrounding whitespace is trimmed (GitHub #587). Returns whether it refused, in which case the
/// caller skips [`check_typing`] for the token. `owner` is the node's expanded `@id`.
///
/// Whitespace is what [`str::trim`] removes. A token with whitespace inside only (`"S231: Real"`)
/// is not a near miss and is left to the readers, as a key is in [`super::property_keys`]. One
/// with surrounding whitespace and whitespace inside (`" S231: Real"`) is refused, but its message
/// offers no spelling, since none without the surrounding whitespace names a term (GitHub #602).
/// The message quotes the token and the term as excerpts ([`crate::excerpt`]).
pub(super) fn check_typing_whitespace(
    token: &str,
    slot: &str,
    owner: &str,
    table: &PrefixTable,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    let trimmed = token.trim();
    if trimmed == token {
        return false;
    }
    let iri = match expand_token(trimmed, table) {
        Expansion::Expanded(iri) => iri,
        Expansion::Verbatim | Expansion::Relative => trimmed.to_owned(),
    };
    let named = match trimmed.strip_prefix("S231:") {
        Some(term) => Some((S231_CONTEXT_IRI, term)),
        None => std::iter::once(S231_CONTEXT_IRI)
            .chain(FOREIGN_NAMESPACES)
            .find_map(|namespace| Some((namespace, iri.strip_prefix(namespace)?))),
    };
    let Some((namespace, term)) = named.filter(|(_, term)| !term.is_empty()) else {
        return false;
    };
    let subject = if owner.is_empty() { token } else { owner };
    let (quoted, named) = (excerpt(token), excerpt(term));
    let message = if term.contains(char::is_whitespace) {
        // No spelling of it without the surrounding whitespace names a term, so none is offered.
        format!(
            "{slot} {quoted:?} on node `{owner}` carries surrounding whitespace, and whitespace \
             inside it too, so it names no term in `{namespace}` as written; write the S231 term \
             with no whitespace"
        )
    } else {
        format!(
            "{slot} {quoted:?} on node `{owner}` carries surrounding whitespace; without it, it \
             names `{named}` in `{namespace}`; write it as `S231:{named}`"
        )
    };
    diags.push(
        Diagnostic::error(DiagCode::MalformedDocument, message).with_subject(subject.to_owned()),
    );
    true
}

/// The refusal for a modeled term keyed by its full IRI in a foreign namespace.
pub(super) fn foreign_key(key: &str, subject: &str, term: &str, namespace: &str) -> Diagnostic {
    Diagnostic::error(
        DiagCode::VocabularyNamespaceMismatch,
        format!(
            "property `{key}` on node `{subject}` names the modeled term `{term}` in \
             `{namespace}`; the S231 vocabulary is read only in `{S231_CONTEXT_IRI}`; write it \
             as `S231:{term}`"
        ),
    )
    .with_subject(subject.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(bindings: &[(&str, &str)]) -> PrefixTable {
        bindings
            .iter()
            .map(|(term, iri)| ((*term).to_owned(), (*iri).to_owned()))
            .collect()
    }

    fn refusals(bindings: &[(&str, &str)]) -> Vec<(String, String)> {
        let mut diags = Vec::new();
        check_bindings(&table(bindings), &mut diags);
        diags
            .into_iter()
            .map(|diag| {
                assert_eq!(diag.code, DiagCode::VocabularyNamespaceMismatch);
                assert!(diag.is_error());
                (diag.subject.expect("subject").to_string(), diag.message)
            })
            .collect()
    }

    #[test]
    fn the_published_binding_no_binding_and_another_prefix_name_are_accepted() {
        assert!(refusals(&[("S231", S231_CONTEXT_IRI)]).is_empty());
        assert!(refusals(&[]).is_empty());
        assert!(refusals(&[("ex", "http://example.org#")]).is_empty());
        assert!(refusals(&[("s231", S231_CONTEXT_IRI)]).is_empty());
        assert!(refusals(&[("S231", S231_CONTEXT_IRI), ("cdl", S231_CONTEXT_IRI)]).is_empty());
    }

    #[test]
    fn s231_bound_to_any_other_iri_is_refused_naming_both_iris() {
        for found in [
            RETIRED_NAMESPACE,
            HTTPS_NEAR_MISS,
            "http://data.ashrae.org/S231",
            "http://data.ashrae.org/S231/",
            "http://data.ashrae.org/s231#",
            "http://data.ashrae.org/S231#x",
            "http://example.org#",
        ] {
            assert_eq!(
                refusals(&[("S231", found)]),
                [(
                    "S231".to_owned(),
                    format!(
                        "@context term `S231` binds `{found}`; the S231 vocabulary is read only \
                         in `http://data.ashrae.org/S231#`"
                    )
                )],
                "{found}"
            );
        }
    }

    #[test]
    fn the_retired_namespace_is_refused_under_any_prefix_name() {
        for term in ["S231P", "s231", "old", "base"] {
            let refused = refusals(&[("S231", S231_CONTEXT_IRI), (term, RETIRED_NAMESPACE)]);
            assert_eq!(refused.len(), 1, "{term}");
            assert_eq!(refused[0].0, term);
            assert!(refused[0].1.contains(RETIRED_NAMESPACE), "{}", refused[0].1);
            assert!(refused[0].1.contains(S231_CONTEXT_IRI), "{}", refused[0].1);
        }
    }

    #[test]
    fn every_offending_binding_is_reported_in_term_order() {
        let refused = refusals(&[
            ("zeta", RETIRED_NAMESPACE),
            ("S231", RETIRED_NAMESPACE),
            ("alpha", RETIRED_NAMESPACE),
            ("ex", "http://example.org#"),
        ]);
        let subjects: Vec<&str> = refused
            .iter()
            .map(|(subject, _)| subject.as_str())
            .collect();
        assert_eq!(subjects, ["S231", "alpha", "zeta"]);
    }

    #[test]
    fn another_prefix_bound_to_a_near_miss_is_not_a_binding_refusal() {
        // Its modeled keys are refused one by one in `property_keys`; an unrelated use of the
        // IRI is not this engine's to judge.
        assert!(refusals(&[("x", HTTPS_NEAR_MISS)]).is_empty());
        assert!(refusals(&[("x", HTTPS_RETIRED_NEAR_MISS)]).is_empty());
    }
}
