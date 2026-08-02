//! JSON-LD `@context` expansion of a CXF document's identity and typing tokens (doc 04 R-3).
//!
//! One uniform pre-resolve pass, run by `resolve()` on a working clone of the DTO before the
//! `@id` index is built, so compact and expanded spellings of the same subject IRI key the same
//! point, block, model, and datatype. The pass transforms the typed DTO slots only, in two
//! classes:
//!
//! - **Identity slots** — every [`Node`] `@id` and every [`IriRef`] in `containsBlock`,
//!   `hasInput`, `hasOutput`, `hasParameter`, `hasConstant`, `isConnectedTo`, and
//!   `hasInstance`. A whole-token `@context` term match or a CURIE with a declared prefix
//!   expands; any other token containing `:` is absolute-as-written; a non-empty token with no
//!   `:` is a relative IRI reference and — with no `@base` modeled — is refused with
//!   [`DiagCode::RelativeIri`], because a node the document cannot name canonically has no
//!   identity to key on. An **empty** `@id` is left alone: the resolver's own
//!   `MissingConnectorId` arm owns that case.
//! - **Typing slots** — every `@type` value and the `isOfDataType` reference: the identical
//!   expansion, **best-effort**. Whatever does not expand stays verbatim and is never refused
//!   here — the downstream `ClassNotFound` / `UnresolvedReference` no-match diagnostics are the
//!   pinned behavior for junk typing tokens (`resolve_errors.rs` pins `@type: "NotAnIri"` →
//!   `ClassNotFound`).
//!
//! Deliberately out of scope: the [`crate::dto::TermAttr::Iri`] arm (`unit`/`quantity`/
//! `displayUnit` are lexical terms consumed verbatim by attribute parsing), the `@type`
//! annotation inside `S231:value` typed literals (an XSD datatype, not a graph identity), and
//! the lossless passthrough (`other`) maps — the semantic payload body is carried verbatim for
//! re-emit (`_spec/05` R-SEM-6). A generic JSON walk over `@id` keys would violate all three at
//! once and would refuse the shipped `"degC"` unit term.
//!
//! `@context` handling: keys starting with `@` are JSON-LD keywords (`@version`, `@protected`,
//! …) and are skipped; a non-`@` key with a simple-string value declares a prefix/term; a
//! non-`@` key with any other value shape is refused as `MalformedDocument`. The `List` and
//! `Iri` context shapes yield an empty prefix table — every prefixed token then passes through
//! as written; the context SHAPE alone never refuses a document.

use std::collections::BTreeMap;

use oce_diag::{DiagCode, Diagnostic};

use crate::dto::{Context, CxfDocument, IriRef, Node, OneOrMany};

/// Declared `@context` prefix/term substitutions: term → IRI.
type PrefixTable = BTreeMap<String, String>;

/// Expand `doc`'s identity and typing tokens against its own `@context`.
///
/// Returns the expanded working clone the resolver keys on, or every refusal diagnostic when
/// any identity token cannot be canonicalized or a non-keyword `@context` term is not a simple
/// string IRI. The input document is never mutated — the DTO keeps the raw source (R-3's
/// "retain the raw string" lives in the Layer-A document, not in the resolved graph).
pub(crate) fn expand_document(doc: &CxfDocument) -> Result<CxfDocument, Vec<Diagnostic>> {
    let mut diags = Vec::new();
    let table = prefix_table(&doc.context, &mut diags);
    let mut expanded = doc.clone();
    for node in &mut expanded.graph {
        expand_node(node, &table, &mut diags);
    }
    if diags.is_empty() {
        Ok(expanded)
    } else {
        Err(diags)
    }
}

/// Build the prefix/term table from the document `@context`, refusing malformed term values.
fn prefix_table(context: &Context, diags: &mut Vec<Diagnostic>) -> PrefixTable {
    let mut table = PrefixTable::new();
    match context {
        Context::Map(map) => {
            for (term, value) in map {
                if term.starts_with('@') {
                    // JSON-LD keywords (`@version`, `@protected`, …) are legal context entries
                    // and are not prefix declarations.
                    continue;
                }
                match value {
                    serde_json::Value::String(iri) => {
                        table.insert(term.clone(), iri.clone());
                    }
                    other => diags.push(
                        Diagnostic::error(
                            DiagCode::MalformedDocument,
                            format!(
                                "@context term `{term}` must map to a simple string IRI, \
                                 got `{other}`"
                            ),
                        )
                        .with_subject(term.clone()),
                    ),
                }
            }
        }
        // The other modeled context shapes carry no prefix declaration this pass reads; an
        // empty table makes every prefixed token unknown-prefix passthrough.
        Context::List(_) | Context::Iri(_) => {}
    }
    table
}

/// The outcome of expanding one token against the prefix table.
enum Expansion {
    /// A whole-token term match or a CURIE with a declared prefix: the canonical form.
    Expanded(String),
    /// Kept as written: an absolute IRI, an unknown prefix, or a `//`-suffix.
    Verbatim,
    /// A relative IRI reference: no `:` and no whole-token term match.
    Relative,
}

/// Classify and expand one token. Pure; the caller decides what `Relative` means per slot.
fn expand_token(token: &str, table: &PrefixTable) -> Expansion {
    // A whole-token term match wins over CURIE splitting (JSON-LD term semantics): a token
    // that IS a declared term substitutes even though it contains no `:`.
    if let Some(iri) = table.get(token) {
        return Expansion::Expanded(iri.clone());
    }
    match token.split_once(':') {
        // A compact-IRI suffix must not begin with `//` (JSON-LD §3.2): `http://…` stays an
        // absolute IRI even when its scheme collides with a declared prefix name.
        Some((_, suffix)) if suffix.starts_with("//") => Expansion::Verbatim,
        Some((prefix, suffix)) => match table.get(prefix) {
            Some(iri) => Expansion::Expanded(format!("{iri}{suffix}")),
            None => Expansion::Verbatim,
        },
        None => Expansion::Relative,
    }
}

/// Expand an identity token in place; a relative token is refused with `RelativeIri`.
///
/// `owner` is `None` when the token is the node's own `@id` (the diagnostic then carries the
/// token itself as subject) and `Some(node_id)` for a structural reference on that node.
fn expand_identity(
    token: &mut String,
    slot: &str,
    owner: Option<&str>,
    table: &PrefixTable,
    diags: &mut Vec<Diagnostic>,
) {
    if token.is_empty() {
        // An empty `@id` has its own pinned diagnostic (`MissingConnectorId`) downstream.
        return;
    }
    match expand_token(token, table) {
        Expansion::Expanded(iri) => *token = iri,
        Expansion::Verbatim => {}
        Expansion::Relative => {
            let (message, subject) = match owner {
                None => (
                    format!(
                        "@id `{token}` is a relative IRI reference and the document declares \
                         no @base to resolve it against"
                    ),
                    token.clone(),
                ),
                Some(owner) => (
                    format!(
                        "{slot} reference `{token}` on node `{owner}` is a relative IRI \
                         reference and the document declares no @base to resolve it against"
                    ),
                    owner.to_owned(),
                ),
            };
            diags.push(Diagnostic::error(DiagCode::RelativeIri, message).with_subject(subject));
        }
    }
}

/// Expand a typing token in place — best-effort, never refused (the downstream no-match
/// diagnostics own junk typing tokens).
fn expand_typing(token: &mut String, table: &PrefixTable) {
    if let Expansion::Expanded(iri) = expand_token(token, table) {
        *token = iri;
    }
}

/// Apply `f` to each value of a [`OneOrMany`] in place.
fn each_mut<T>(values: &mut OneOrMany<T>, mut f: impl FnMut(&mut T)) {
    match values {
        OneOrMany::None => {}
        OneOrMany::One(value) => f(value),
        OneOrMany::Many(values) => values.iter_mut().for_each(&mut f),
    }
}

/// Expand every in-scope slot of one node.
fn expand_node(node: &mut Node, table: &PrefixTable, diags: &mut Vec<Diagnostic>) {
    expand_identity(&mut node.id, "@id", None, table, diags);
    let owner = node.id.clone();
    let mut expand_refs = |slot: &str, refs: &mut OneOrMany<IriRef>| {
        each_mut(refs, |reference: &mut IriRef| {
            expand_identity(&mut reference.id, slot, Some(&owner), table, diags);
        });
    };
    expand_refs("S231:hasInput", &mut node.has_input);
    expand_refs("S231:hasOutput", &mut node.has_output);
    expand_refs("S231:hasParameter", &mut node.has_parameter);
    expand_refs("S231:hasConstant", &mut node.has_constant);
    expand_refs("S231:containsBlock", &mut node.contains_block);
    expand_refs("S231:hasInstance", &mut node.has_instance);
    expand_refs("S231:isConnectedTo", &mut node.is_connected_to);
    if let Some(types) = &mut node.r#type {
        each_mut(types, |value: &mut String| expand_typing(value, table));
    }
    if let Some(datatype) = &mut node.is_of_data_type {
        expand_typing(&mut datatype.id, table);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(entries: &[(&str, &str)]) -> PrefixTable {
        entries
            .iter()
            .map(|(term, iri)| ((*term).to_owned(), (*iri).to_owned()))
            .collect()
    }

    fn identity(token: &str, table: &PrefixTable) -> Result<String, Vec<Diagnostic>> {
        let mut token = token.to_owned();
        let mut diags = Vec::new();
        expand_identity(&mut token, "@id", None, table, &mut diags);
        if diags.is_empty() {
            Ok(token)
        } else {
            Err(diags)
        }
    }

    #[test]
    fn absolute_identity_tokens_pass_through_byte_identical() {
        let table = table(&[("ex", "http://example.org#")]);
        for token in [
            "http://example.org#MinLoop.con.y",
            "urn:open-control:cxf-export:root",
            "mailto:a@b.example",
        ] {
            assert_eq!(identity(token, &table).unwrap(), token);
        }
    }

    #[test]
    fn curie_with_declared_prefix_expands_to_the_concatenated_iri() {
        let table = table(&[("ex", "http://example.org#"), ("S231", "http://s231#")]);
        assert_eq!(
            identity("ex:MinLoop.con", &table).unwrap(),
            "http://example.org#MinLoop.con"
        );
        let mut typing = "S231:RealInput".to_owned();
        expand_typing(&mut typing, &table);
        assert_eq!(typing, "http://s231#RealInput");
    }

    #[test]
    fn whole_token_term_match_wins_even_without_a_colon() {
        let table = table(&[("degC", "http://qudt.org/vocab/unit/DEG_C")]);
        // As a declared term the colon-free token substitutes instead of being refused.
        assert_eq!(
            identity("degC", &table).unwrap(),
            "http://qudt.org/vocab/unit/DEG_C"
        );
    }

    #[test]
    fn whole_token_term_match_wins_over_curie_splitting() {
        let table = table(&[
            ("ex", "http://example.org#"),
            ("ex:special", "http://example.org/special"),
        ]);
        // The full-token binding substitutes; the `ex:` prefix never sees the token.
        assert_eq!(
            identity("ex:special", &table).unwrap(),
            "http://example.org/special"
        );
    }

    #[test]
    fn unknown_prefix_passes_through_as_written() {
        let table = table(&[("ex", "http://example.org#")]);
        assert_eq!(identity("vendor:Thing", &table).unwrap(), "vendor:Thing");
        let mut typing = "vendor:Thing".to_owned();
        expand_typing(&mut typing, &table);
        assert_eq!(typing, "vendor:Thing");
    }

    #[test]
    fn double_slash_suffix_is_never_treated_as_a_curie() {
        // `http` declared as a prefix must not capture `http://…` (JSON-LD: a compact-IRI
        // suffix must not begin with `//`).
        let table = table(&[("http", "http://trap.example/")]);
        assert_eq!(
            identity("http://example.org#A", &table).unwrap(),
            "http://example.org#A"
        );
    }

    #[test]
    fn at_keyword_context_entries_are_skipped_not_refused() {
        let mut diags = Vec::new();
        let context = Context::Map(
            [
                ("@version".to_owned(), serde_json::json!(1.1)),
                ("@protected".to_owned(), serde_json::json!(true)),
                (
                    "ex".to_owned(),
                    serde_json::Value::String("http://example.org#".to_owned()),
                ),
            ]
            .into_iter()
            .collect(),
        );
        let table = prefix_table(&context, &mut diags);
        assert!(diags.is_empty(), "keywords must not refuse: {diags:?}");
        assert_eq!(table.len(), 1);
        assert_eq!(table["ex"], "http://example.org#");
    }

    #[test]
    fn non_string_context_term_is_refused_as_malformed_document() {
        let mut diags = Vec::new();
        let context = Context::Map(
            [(
                "ex".to_owned(),
                serde_json::json!({ "@id": "http://example.org#" }),
            )]
            .into_iter()
            .collect(),
        );
        prefix_table(&context, &mut diags);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagCode::MalformedDocument);
        assert_eq!(diags[0].subject.as_deref(), Some("ex"));
    }

    #[test]
    fn list_and_iri_context_shapes_yield_an_empty_table_without_refusing() {
        for context in [
            Context::List(vec![serde_json::json!({ "ex": "http://example.org#" })]),
            Context::Iri("http://example.org/context".to_owned()),
        ] {
            let mut diags = Vec::new();
            let table = prefix_table(&context, &mut diags);
            assert!(diags.is_empty());
            assert!(table.is_empty());
        }
    }

    #[test]
    fn relative_identity_token_is_refused_and_relative_typing_token_is_not() {
        let table = table(&[("ex", "http://example.org#")]);
        let diags = identity("MinLoop", &table).unwrap_err();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagCode::RelativeIri);
        assert!(
            diags[0].message.contains("`MinLoop`"),
            "{}",
            diags[0].message
        );
        assert_eq!(diags[0].subject.as_deref(), Some("MinLoop"));
        // The identical token in a typing slot stays verbatim: the pinned downstream
        // no-match diagnostics own junk there.
        let mut typing = "MinLoop".to_owned();
        expand_typing(&mut typing, &table);
        assert_eq!(typing, "MinLoop");
    }

    #[test]
    fn relative_reference_names_its_slot_owner_and_token() {
        let table = table(&[]);
        let mut token = "gain.u".to_owned();
        let mut diags = Vec::new();
        expand_identity(
            &mut token,
            "S231:isConnectedTo",
            Some("http://example.org#M.con.y"),
            &table,
            &mut diags,
        );
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagCode::RelativeIri);
        assert!(
            diags[0].message.contains("S231:isConnectedTo")
                && diags[0].message.contains("`gain.u`")
                && diags[0].message.contains("http://example.org#M.con.y"),
            "{}",
            diags[0].message
        );
        assert_eq!(
            diags[0].subject.as_deref(),
            Some("http://example.org#M.con.y")
        );
    }

    #[test]
    fn empty_id_is_left_for_the_resolver_missing_connector_id_arm() {
        let table = table(&[]);
        assert_eq!(identity("", &table).unwrap(), "");
    }
}
