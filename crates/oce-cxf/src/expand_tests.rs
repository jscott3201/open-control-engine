//! Unit tests for `@context` expansion of identity and typing tokens (`expand.rs`).

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
        "https://x/path",
        "a2+b-c.d://x",
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
            diags[0].message.contains(&format!("`{token}`")) && diags[0].message.contains("@base"),
            "{}",
            diags[0].message
        );
        let mut typing = token.to_owned();
        expand_typing(&mut typing, &table);
        assert_eq!(typing, token);
    }
}

#[test]
fn a_spelling_refused_as_a_binding_is_refused_as_an_identity_when_its_prefix_is_undeclared() {
    // The symmetry property itself: `is_absolute_iri` guards both sides, so no
    // undeclared-prefix spelling is simultaneously an illegal `@context` binding value
    // and a legal durable identity.
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
fn a_declared_prefix_never_rescues_a_whitespace_bearing_suffix() {
    // The joined-expansion check on the declared-prefix arm: the binding supplies a
    // valid scheme and a clean remainder head, so the join fails the absolute-form
    // predicate exactly when the authored suffix carries whitespace — refusing beats
    // minting a key the importer itself would refuse on re-ingest.
    let table = table(&[("ex", "http://example.org#")]);
    for token in ["ex:MinLoop.con x", "ex:c\nd"] {
        let diags = identity(token, &table).unwrap_err();
        assert_eq!(diags.len(), 1, "{token:?}: {diags:?}");
        assert_eq!(diags[0].code, DiagCode::RelativeIri);
        assert_eq!(diags[0].subject.as_deref(), Some(token));
    }
    // Control: a clean suffix expands exactly as before the check.
    assert_eq!(
        identity("ex:MinLoop.con", &table).unwrap(),
        "http://example.org#MinLoop.con"
    );
}

#[test]
fn double_slash_suffix_without_a_valid_scheme_is_refused_as_relative_iri() {
    // The `//` arm runs the same absolute-form guard as the undeclared-prefix arm. A
    // `//`-suffix token is never a CURIE (JSON-LD 1.1 §4.1.5), so with no rescue possible a
    // scheme-invalid spelling has no canonical form at all. The identical tokens stay
    // verbatim in a typing slot: typing is best-effort and the downstream no-match
    // diagnostics own junk there.
    let table = table(&[("ex", "http://example.org#")]);
    for token in [
        "1st://x",
        "://x",
        "2024://x",
        "éttp://x",
        " http://x",
        "http ://x",
    ] {
        let diags = identity(token, &table).unwrap_err();
        assert_eq!(diags.len(), 1, "{token:?}: {diags:?}");
        assert_eq!(diags[0].code, DiagCode::RelativeIri);
        assert_eq!(diags[0].subject.as_deref(), Some(token));
        assert!(
            diags[0].message.contains(&format!("`{token}`")) && diags[0].message.contains("@base"),
            "{}",
            diags[0].message
        );
        let mut typing = token.to_owned();
        expand_typing(&mut typing, &table);
        assert_eq!(typing, token);
    }
}

#[test]
fn a_declared_prefix_never_rescues_a_double_slash_token() {
    // Per JSON-LD 1.1 §4.1.5 (Compact IRIs) a value whose suffix begins with `//` is
    // interpreted as an IRI instead of being CURIE-expanded, so `2024://x` is
    // not a CURIE even with `2024` bound. A fall-through to the prefix lookup would
    // expand it to `http://example.org#//x` — a minted malformed durable key — instead
    // of refusing. (Contrast `2024:x`, which the same declaration turns into an
    // ordinary CURIE that expands.)
    let table = table(&[("2024", "http://example.org#")]);
    let diags = identity("2024://x", &table).unwrap_err();
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, DiagCode::RelativeIri);
    assert_eq!(diags[0].subject.as_deref(), Some("2024://x"));
}

#[test]
fn a_valid_scheme_with_an_empty_or_whitespace_remainder_is_refused_as_relative_iri() {
    // `ab` is an RFC-valid scheme — the refusal is about the remainder, not the scheme.
    // A bare scheme (`ab:` is legal `path-empty` URI syntax) carries no name to key a
    // durable identity on, and no IRI production admits whitespace.
    let table = table(&[]);
    for token in ["ab:", "ab:c d", "ab:c\nd"] {
        let diags = identity(token, &table).unwrap_err();
        assert_eq!(diags.len(), 1, "{token:?}: {diags:?}");
        assert_eq!(diags[0].code, DiagCode::RelativeIri);
        assert_eq!(diags[0].subject.as_deref(), Some(token));
    }
    for token in ["urn:x", "http://x"] {
        assert_eq!(identity(token, &table).unwrap(), token);
    }
}

#[test]
fn a_binding_value_with_an_empty_or_whitespace_remainder_is_refused() {
    // The same predicate guards `@context` binding values, so the remainder tightening
    // narrows acceptance there too: `{"ex": "urn:"}` is legal JSON-LD and refused here
    // by policy — a bare-scheme binding would concatenate every CURIE under it into
    // keys no canonical absolute spelling can reach.
    for value in ["urn:", "ab:c d", "ab:c\nd"] {
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
    }
}

#[test]
fn binding_acceptance_and_identity_acceptance_agree_when_no_prefix_is_declared() {
    // The scoped symmetry contract: with NO prefix declared, [`is_absolute_iri`] guards
    // both routes, so accepted-as-binding == loadable-as-identity across refused shapes
    // and accepted controls alike. A DECLARED prefix legitimately breaks the
    // equivalence for CURIE spellings (`declaring_the_prefix_rescues_…` pins that), so
    // declared-prefix cases stay out of this table by design.
    for token in [
        "1st://x",
        "://x",
        "2024://x",
        "éttp://x",
        " http://x",
        "http ://x",
        "ab:",
        "ab:c d",
        "ab:c\nd",
        "http://x",
        "https://x/path",
        "a2+b-c.d://x",
        "urn:x",
        "mailto:a@b.example",
    ] {
        let mut binding_diags = Vec::new();
        let context = Context::Map(
            [("ex".to_owned(), serde_json::json!(token))]
                .into_iter()
                .collect(),
        );
        prefix_table(&context, &mut binding_diags);
        let accepted_as_binding = binding_diags.is_empty();
        let loadable_as_identity = identity(token, &table(&[])).is_ok();
        assert_eq!(accepted_as_binding, loadable_as_identity, "{token:?}");
    }
}

#[test]
fn double_slash_suffix_is_never_treated_as_a_curie() {
    // `http` declared as a prefix must not capture `http://…` (JSON-LD 1.1 §4.1.5: a
    // value whose suffix begins with `//` is interpreted as an IRI, not a compact IRI).
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
fn identity_changing_context_keywords_are_refused_naming_the_keyword() {
    for keyword in ["@base", "@import", "@vocab"] {
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
    // Controls, including the pre-ruled edge: `S231:x` passes because `S231` is an RFC
    // 3986-valid scheme and is not an active prefix in this context.
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
    expand_document(&doc, Some(&bytes)).expect("document expands")
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
