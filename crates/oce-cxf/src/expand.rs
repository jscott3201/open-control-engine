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
//!   expands; a token whose first `:` is followed by `//` is never CURIE-split and stays as
//!   written (JSON-LD §3.2, scheme unchecked); any other token stays as written only when it
//!   is itself a syntactically absolute IRI — the same [`is_absolute_iri`] predicate a
//!   `@context` binding value must pass, so a spelling refused as a binding cannot load as a
//!   durable identity. What remains — no `:`, an empty scheme (`:x`), a scheme-invalid
//!   spelling (`2024:x`) — is a relative IRI reference and — `@base` being a refused
//!   construct, below — is refused with [`DiagCode::RelativeIri`], because a node the
//!   document cannot name canonically has no identity to key on. An **empty** `@id` is left
//!   alone: the resolver's own `MissingConnectorId` arm owns that case.
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
//! `@context` handling: the supported form is an **inline prefix map** — a single map, or a
//! list of maps merged in order with later bindings overriding earlier ones (JSON-LD context
//! processing order). Within a map, `@base` and `@vocab` are refused as `NonSubsetConstruct`:
//! both change identity semantics this engine does not implement, and skipping them silently
//! would reintroduce the exact spelling-dependent-identity hazard this pass exists to close.
//! Every other `@`-keyword entry (`@version`, `@protected`, …) is skipped; a non-`@` key with
//! a simple-string value declares a prefix term, and that value must itself be a syntactically
//! absolute IRI (see [`is_absolute_iri`]) or the binding is refused as `NonSubsetConstruct` —
//! a prefix bound to a relative value would smuggle relative identities past the per-token
//! guard; any other value shape is refused as `MalformedDocument`. A **remote context** — the whole `@context` a string IRI, or a string
//! element inside the list — is refused as `NonSubsetConstruct`: a deterministic embedded
//! engine dereferences nothing at load. Context-shape validation runs BEFORE slot expansion
//! and its refusals return alone, so a `RelativeIri` refusal's "declares no @base" clause is
//! literally true whenever it fires.

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
    let mut context_diags = Vec::new();
    let table = prefix_table(&doc.context, &mut context_diags);
    if !context_diags.is_empty() {
        // Context-shape refusals return alone, before any slot is inspected: whenever the slot
        // pass below reports `RelativeIri`, "declares no @base" is literally true, because a
        // document declaring one was already refused here.
        return Err(context_diags);
    }
    let mut diags = Vec::new();
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

/// Build the prefix/term table from the document `@context`: a single inline map, or a list of
/// inline maps merged in order (later bindings win). Remote references, `@base`, `@vocab`, and
/// malformed term values are refused — see the module docs for why each is a refusal rather
/// than a skip.
fn prefix_table(context: &Context, diags: &mut Vec<Diagnostic>) -> PrefixTable {
    let mut table = PrefixTable::new();
    match context {
        Context::Map(map) => merge_context_entries(map.iter(), &mut table, diags),
        Context::List(entries) => {
            for entry in entries {
                match entry {
                    serde_json::Value::Object(map) => {
                        merge_context_entries(map.iter(), &mut table, diags);
                    }
                    serde_json::Value::String(reference) => {
                        diags.push(remote_context_refusal(reference));
                    }
                    other => diags.push(Diagnostic::error(
                        DiagCode::MalformedDocument,
                        format!("@context list entry must be an inline map, got `{other}`"),
                    )),
                }
            }
        }
        Context::Iri(reference) => diags.push(remote_context_refusal(reference)),
    }
    table
}

/// The refusal for a remote `@context` reference (a string IRI in either context shape).
fn remote_context_refusal(reference: &str) -> Diagnostic {
    Diagnostic::error(
        DiagCode::NonSubsetConstruct,
        format!(
            "@context references the remote context `{reference}`; a deterministic embedded \
             engine cannot dereference remote contexts at load — inline the bindings as a \
             prefix map"
        ),
    )
    .with_subject(reference.to_owned())
}

/// Merge one inline context map's entries into `table`. Entries are processed in order and a
/// later binding for the same term overrides an earlier one (JSON-LD context processing
/// order), which is what gives a list of maps its later-wins semantics.
fn merge_context_entries<'a>(
    entries: impl Iterator<Item = (&'a String, &'a serde_json::Value)>,
    table: &mut PrefixTable,
    diags: &mut Vec<Diagnostic>,
) {
    for (term, value) in entries {
        if term == "@base" || term == "@vocab" {
            // Both keywords change identity semantics this engine does not implement.
            // Skipping them silently would be the spelling-dependent-identity hazard again:
            // the document means one set of subject IRIs, the engine would key another.
            diags.push(
                Diagnostic::error(
                    DiagCode::NonSubsetConstruct,
                    format!(
                        "@context declares `{term}`, which changes identity semantics this \
                         engine does not implement; write prefix terms or absolute IRIs \
                         instead"
                    ),
                )
                .with_subject(term.clone()),
            );
            continue;
        }
        if term.starts_with('@') {
            // Remaining JSON-LD keywords (`@version`, `@protected`, …) are legal context
            // entries that do not affect identity; skipped.
            continue;
        }
        match value {
            serde_json::Value::String(iri) if is_absolute_iri(iri) => {
                table.insert(term.clone(), iri.clone());
            }
            serde_json::Value::String(iri) => diags.push(
                // A prefix bound to a non-absolute value would concatenate every CURIE under
                // it into a RELATIVE identity that the per-token guard cannot see (the joined
                // token contains a `:` from the CURIE spelling), defeating the canonical-key
                // contract. Refuse the binding itself, up front.
                Diagnostic::error(
                    DiagCode::NonSubsetConstruct,
                    format!(
                        "@context term `{term}` binds `{iri}`, which is not an absolute IRI; \
                         a prefix must bind an absolute IRI (a scheme, then `:`)"
                    ),
                )
                .with_subject(term.clone()),
            ),
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

/// Whether `value` is a syntactically absolute IRI: it contains a `:` and everything before
/// the FIRST colon is a nonempty ASCII scheme per RFC 3986 §3.1 — a letter, then letters,
/// digits, `+`, `-`, or `.`. The contract is syntactic canonical-absolute FORM, not semantic
/// reachability, so `S231:x` passes (`S231` is an RFC-valid scheme) — nested-CURIE expansion
/// is deliberately not chased.
fn is_absolute_iri(value: &str) -> bool {
    match value.split_once(':') {
        Some((scheme, _)) => {
            let mut chars = scheme.chars();
            match chars.next() {
                Some(first) if first.is_ascii_alphabetic() => {
                    chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
                }
                _ => false,
            }
        }
        None => false,
    }
}

/// The outcome of expanding one token against the prefix table.
enum Expansion {
    /// A whole-token term match or a CURIE with a declared prefix: the canonical form.
    Expanded(String),
    /// Kept as written: a `//`-suffix token (never CURIE-split), or an undeclared-prefix
    /// token that is already a syntactically absolute IRI.
    Verbatim,
    /// No canonical absolute form: no `:` and no whole-token term match, or an undeclared
    /// prefix that is not an RFC 3986 scheme (empty, or scheme-invalid like `2024`).
    Relative,
}

/// Classify and expand one token. Pure; the caller decides what `Relative` means per slot.
///
/// The verbatim arm is symmetric with the `@context` binding check: an undeclared-prefix
/// token survives as written only when [`is_absolute_iri`] accepts it, so a spelling refused
/// as a binding value cannot become a durable key here. The `//`-suffix arm is the one
/// carve-out — it never inspects the scheme.
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
            None if is_absolute_iri(token) => Expansion::Verbatim,
            // `2024:x` and `:x` expand against nothing and are not absolute (the part before
            // the first `:` is not an RFC 3986 scheme): refusing beats minting a durable key
            // from a malformed spelling with zero diagnostics.
            None => Expansion::Relative,
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
    fn unknown_prefix_in_absolute_form_passes_through_as_written() {
        // `vendor` is an RFC 3986-valid scheme, so the token is already canonical-absolute
        // and survives without a binding. Contrast the scheme-invalid refusals below.
        let table = table(&[("ex", "http://example.org#")]);
        assert_eq!(identity("vendor:Thing", &table).unwrap(), "vendor:Thing");
        let mut typing = "vendor:Thing".to_owned();
        expand_typing(&mut typing, &table);
        assert_eq!(typing, "vendor:Thing");
    }

    #[test]
    fn unknown_prefix_without_absolute_form_is_refused_as_relative_iri() {
        // Before the symmetric guard, ANY colon-containing token passed the verbatim arm:
        // a digit-led scheme and an empty scheme both loaded clean as durable keys. They now
        // refuse exactly like colon-free relatives — same code, same subject, same message
        // shape. The identical token in a typing slot stays verbatim: typing is best-effort
        // and the downstream no-match diagnostics own junk there.
        let table = table(&[("ex", "http://example.org#")]);
        for token in ["2024:MinLoop.con.y", ":x"] {
            let diags = identity(token, &table).unwrap_err();
            assert_eq!(diags.len(), 1, "{token}: {diags:?}");
            assert_eq!(diags[0].code, DiagCode::RelativeIri);
            assert_eq!(diags[0].subject.as_deref(), Some(token));
            assert!(
                diags[0].message.contains(&format!("`{token}`"))
                    && diags[0].message.contains("@base"),
                "{}",
                diags[0].message
            );
            let mut typing = token.to_owned();
            expand_typing(&mut typing, &table);
            assert_eq!(typing, token);
        }
    }

    #[test]
    fn a_spelling_refused_as_a_binding_is_refused_as_an_identity_token() {
        // The symmetry property itself: `is_absolute_iri` guards both sides, so no spelling
        // is simultaneously an illegal `@context` binding value and a legal durable identity.
        for spelling in ["1st:x", "2024:x", ":x"] {
            let mut binding_diags = Vec::new();
            let context = Context::Map(
                [("ex".to_owned(), serde_json::json!(spelling))]
                    .into_iter()
                    .collect(),
            );
            prefix_table(&context, &mut binding_diags);
            assert_eq!(binding_diags.len(), 1, "{spelling}: {binding_diags:?}");
            assert_eq!(binding_diags[0].code, DiagCode::NonSubsetConstruct);
            let identity_diags = identity(spelling, &table(&[])).unwrap_err();
            assert_eq!(identity_diags.len(), 1, "{spelling}: {identity_diags:?}");
            assert_eq!(identity_diags[0].code, DiagCode::RelativeIri);
        }
    }

    #[test]
    fn export_root_urn_needs_no_binding_to_survive_byte_identical() {
        // The load-bearing regression: the export fixtures' root subject declares no `urn`
        // binding, and `urn` is an RFC 3986-valid scheme, so the token passes the
        // absolute-form guard and stays byte-identical through an empty table.
        assert_eq!(
            identity("urn:open-control:cxf-export:root", &table(&[])).unwrap(),
            "urn:open-control:cxf-export:root"
        );
    }

    #[test]
    fn declaring_the_prefix_rescues_a_scheme_invalid_compact_token() {
        // The refusal is undeclared-prefix-only. Term NAMES are not scheme-checked (only
        // binding values are), so declaring `2024` turns the same spelling into an ordinary
        // CURIE that expands instead of refusing.
        let table = table(&[("2024", "http://example.org#")]);
        assert_eq!(
            identity("2024:MinLoop.con.y", &table).unwrap(),
            "http://example.org#MinLoop.con.y"
        );
    }

    #[test]
    fn double_slash_suffix_stays_verbatim_without_a_scheme_check() {
        // The `//` arm precedes the absolute-form guard and does not inspect the scheme, so
        // these scheme-invalid spellings still pass verbatim. Pinned so a future tightening
        // of that arm is a deliberate acceptance change, not a drive-by.
        let table = table(&[("ex", "http://example.org#")]);
        for token in ["1st://x", "://x"] {
            assert_eq!(identity(token, &table).unwrap(), token);
        }
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
    fn list_of_maps_merges_in_order_with_later_bindings_winning() {
        let context = Context::List(vec![
            serde_json::json!({ "ex": "http://first.example#", "S231": "http://s231#" }),
            serde_json::json!({ "ex": "http://second.example#" }),
        ]);
        let mut diags = Vec::new();
        let table = prefix_table(&context, &mut diags);
        assert!(diags.is_empty(), "{diags:?}");
        // JSON-LD processes context entries in order: the later `ex` binding overrides.
        assert_eq!(table["ex"], "http://second.example#");
        assert_eq!(table["S231"], "http://s231#");
    }

    #[test]
    fn remote_context_references_are_refused_in_both_shapes() {
        for context in [
            Context::Iri("http://example.org/context.jsonld".to_owned()),
            Context::List(vec![
                serde_json::json!({ "ex": "http://example.org#" }),
                serde_json::json!("http://example.org/context.jsonld"),
            ]),
        ] {
            let mut diags = Vec::new();
            prefix_table(&context, &mut diags);
            assert_eq!(diags.len(), 1, "{diags:?}");
            assert_eq!(diags[0].code, DiagCode::NonSubsetConstruct);
            assert_eq!(
                diags[0].subject.as_deref(),
                Some("http://example.org/context.jsonld")
            );
            assert!(diags[0].message.contains("remote"), "{}", diags[0].message);
        }
    }

    #[test]
    fn base_and_vocab_context_keywords_are_refused_naming_the_keyword() {
        for keyword in ["@base", "@vocab"] {
            let mut diags = Vec::new();
            let context = Context::Map(
                [
                    (keyword.to_owned(), serde_json::json!("http://example.org/")),
                    (
                        "ex".to_owned(),
                        serde_json::Value::String("http://example.org#".to_owned()),
                    ),
                ]
                .into_iter()
                .collect(),
            );
            prefix_table(&context, &mut diags);
            assert_eq!(diags.len(), 1, "{keyword}: {diags:?}");
            assert_eq!(diags[0].code, DiagCode::NonSubsetConstruct);
            assert_eq!(diags[0].subject.as_deref(), Some(keyword));
            assert!(diags[0].message.contains(keyword), "{}", diags[0].message);
        }
    }

    #[test]
    fn non_absolute_prefix_bindings_are_refused_naming_the_term() {
        // Each value would concatenate CURIEs into relative identities the per-token guard
        // cannot see (the joined token carries the CURIE's own `:`): empty, scheme-less,
        // relative-path, and digit-led-scheme spellings all refuse.
        for value in ["", "example.org/", "../up#", "1st:x"] {
            let mut diags = Vec::new();
            let context = Context::Map(
                [("ex".to_owned(), serde_json::json!(value))]
                    .into_iter()
                    .collect(),
            );
            prefix_table(&context, &mut diags);
            assert_eq!(diags.len(), 1, "{value:?}: {diags:?}");
            assert_eq!(diags[0].code, DiagCode::NonSubsetConstruct);
            assert_eq!(diags[0].subject.as_deref(), Some("ex"));
            assert!(
                diags[0].message.contains("absolute IRI") && diags[0].message.contains("ex"),
                "{}",
                diags[0].message
            );
        }
    }

    #[test]
    fn absolute_prefix_bindings_pass_the_scheme_predicate() {
        // Controls, including the pre-ruled edge: `S231:x` passes because `S231` is an
        // RFC 3986-valid scheme — the contract is syntactic absolute form, and nested-CURIE
        // expansion is deliberately not chased.
        for value in ["http://example.org#", "urn:oce:names#", "S231:x"] {
            let mut diags = Vec::new();
            let context = Context::Map(
                [("ex".to_owned(), serde_json::json!(value))]
                    .into_iter()
                    .collect(),
            );
            let table = prefix_table(&context, &mut diags);
            assert!(diags.is_empty(), "{value:?}: {diags:?}");
            assert_eq!(table["ex"], value);
        }
    }

    /// Parse a JSON document and run the whole pass — the level at which dropping one
    /// `expand_refs` arm in `expand_node` is visible.
    fn expand_json(document: &serde_json::Value) -> CxfDocument {
        let bytes = serde_json::to_vec(document).expect("serialize test document");
        let doc = crate::parse_document(&bytes).expect("parse test document");
        expand_document(&doc).expect("document expands")
    }

    #[test]
    fn has_constant_references_expand_like_every_other_identity_slot() {
        let expanded = expand_json(&serde_json::json!({
            "@context": { "ex": "http://example.org#" },
            "@graph": [ {
                "@id": "ex:M",
                "S231:hasConstant": [ { "@id": "ex:M.limit" } ]
            } ]
        }));
        assert_eq!(
            expanded.graph[0].has_constant.as_slice()[0].id,
            "http://example.org#M.limit"
        );
    }

    #[test]
    fn has_instance_references_expand_like_every_other_identity_slot() {
        let expanded = expand_json(&serde_json::json!({
            "@context": { "ex": "http://example.org#" },
            "@graph": [ {
                "@id": "ex:M",
                "S231:hasInstance": [ { "@id": "ex:M.member" } ]
            } ]
        }));
        assert_eq!(
            expanded.graph[0].has_instance.as_slice()[0].id,
            "http://example.org#M.member"
        );
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
