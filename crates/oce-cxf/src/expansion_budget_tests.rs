//! Unit tests for the bytes `@context` expansion may add to one document
//! (`expansion_budget.rs`), on the budget alone and through the whole expansion pass.

use serde_json::{Value as Json, json};

use super::super::expand_document;
use super::*;
use crate::dto::CxfDocument;

/// The budget, restated so a change to it fails here.
const BUDGET: usize = 8 * 1024 * 1024;

/// An absolute IRI of exactly `bytes` bytes.
fn iri(bytes: usize) -> String {
    let stem = "urn:long:";
    format!("{stem}{}", "x".repeat(bytes - stem.len()))
}

/// Run the whole pass on `document`.
fn expand(document: &Json) -> Result<CxfDocument, Vec<Diagnostic>> {
    let bytes = serde_json::to_vec(document).expect("serialize test document");
    let doc = crate::parse_document(&bytes).expect("parse test document");
    expand_document(&doc, Some(&bytes)).map_err(|diags| diags.into_iter().collect())
}

/// The refusal of `token` at `what`, expanding to `expanded` bytes after `added` were added,
/// about `subject`, stated by hand.
fn refusal(what: &str, token: &str, expanded: usize, added: usize, subject: &str) -> Diagnostic {
    let adds = expanded - token.len();
    Diagnostic::error(
        DiagCode::ResourceLimitExceeded,
        format!(
            "{what} `{token}` expands to {expanded} bytes through the document's @context, \
             {adds} more than it is written in; expansion had added {added} bytes, and may add \
             at most 8388608 to one document"
        ),
    )
    .with_subject(subject.to_owned())
}

/// A context that binds `p` to an IRI each `p:T` adds 64 KiB through, `ab` to an IRI one byte
/// longer than the term, `abc` to one of its own length and `abcd` to a shorter one.
fn edge_context() -> Json {
    json!({ "p": iri(64 * 1024 + 2), "ab": "a:b", "abc": "a:c", "abcd": "a:d" })
}

/// One node typed `p:T` 128 times, which adds the budget exactly, and then `more`.
fn at_the_edge(more: &[&str]) -> Json {
    let mut types = vec!["p:T"; 128];
    types.extend(more);
    json!({ "@context": edge_context(), "@graph": [ { "@id": "urn:n", "@type": types } ] })
}

#[test]
fn the_budget_is_eight_mebibytes() {
    assert_eq!(MAX_CONTEXT_EXPANSION_BYTES, BUDGET);
}

#[test]
fn expansions_that_add_the_budget_exactly_are_all_made_and_one_byte_more_is_refused() {
    let expanded = expand(&at_the_edge(&[])).expect("the budget is met, not passed");
    let types = expanded.graph[0].r#type.as_ref().expect("types");
    assert_eq!(types.len(), 128);
    let whole = format!("{}T", iri(64 * 1024 + 2));
    assert!(types.iter().all(|expanded| *expanded == whole));

    // `ab` is two bytes and expands to three.
    let refused = expand(&at_the_edge(&["ab"])).expect_err("one byte past the budget");
    assert_eq!(refused, [refusal("@type", "ab", 3, BUDGET, "urn:n")]);
}

#[test]
fn an_expansion_no_longer_than_its_token_is_made_on_a_spent_budget() {
    let expanded = expand(&at_the_edge(&["abc", "abcd", "abc"])).expect("these three add nothing");
    let types = expanded.graph[0].r#type.as_ref().expect("types");
    let last: Vec<&str> = types.iter().skip(128).map(String::as_str).collect();
    assert_eq!(last, ["a:c", "a:d", "a:c"]);
}

#[test]
fn a_token_written_as_an_absolute_iri_adds_nothing_however_long_it_is() {
    let long = iri(BUDGET + 1);
    let expanded = expand(&json!({
        "@context": edge_context(),
        "@graph": [ { "@id": long, "@type": [ long, long ],
                      "S231:hasInput": [ { "@id": long }, { "@id": long } ] } ]
    }))
    .expect("nothing is expanded");
    assert_eq!(expanded.graph[0].id, long);
}

#[test]
fn every_slot_is_refused_by_its_own_name_about_its_node() {
    // One token through `p` adds one byte more than the whole budget.
    let binding = iri(BUDGET + 3);
    let expanded = BUDGET + 4;
    let node = |members: Json| {
        let mut node = json!({ "@id": "urn:n" });
        node.as_object_mut()
            .expect("an object")
            .extend(members.as_object().expect("an object").clone());
        json!({ "@context": { "p": binding }, "@graph": [ node ] })
    };
    let own_id = json!({ "@context": { "p": binding }, "@graph": [ { "@id": "p:T" } ] });
    let mut cases = vec![
        ("@id".to_owned(), own_id, "p:T"),
        ("@type".to_owned(), node(json!({ "@type": "p:T" })), "urn:n"),
        (
            "@type".to_owned(),
            node(json!({ "@type": [ "urn:first", "p:T" ] })),
            "urn:n",
        ),
        (
            "S231:isOfDataType".to_owned(),
            node(json!({ "S231:isOfDataType": { "@id": "p:T" } })),
            "urn:n",
        ),
        (
            "S231:unit".to_owned(),
            node(json!({ "S231:unit": { "@id": "p:T" } })),
            "urn:n",
        ),
        (
            "S231:displayUnit".to_owned(),
            node(json!({ "S231:displayUnit": { "@id": "p:T" } })),
            "urn:n",
        ),
    ];
    for slot in [
        "S231:hasInput",
        "S231:hasOutput",
        "S231:hasParameter",
        "S231:hasConstant",
        "S231:containsBlock",
        "S231:hasInstance",
        "S231:isConnectedTo",
    ] {
        let mut members = serde_json::Map::new();
        members.insert(
            slot.to_owned(),
            json!([ { "@id": "urn:first" }, { "@id": "p:T" } ]),
        );
        cases.push((
            format!("{slot} reference"),
            node(Json::Object(members)),
            "urn:n",
        ));
    }
    assert_eq!(cases.len(), 13);
    for (what, document, subject) in cases {
        let refused = expand(&document).expect_err("past the budget");
        assert_eq!(
            refused,
            [refusal(&what, "p:T", expanded, 0, subject)],
            "{what}"
        );
    }
}

#[test]
fn a_typing_token_on_a_node_with_no_id_is_refused_about_the_token() {
    let refused = expand(&json!({
        "@context": { "p": iri(BUDGET + 3) },
        "@graph": [ { "@type": "p:T" } ]
    }))
    .expect_err("past the budget");
    assert_eq!(refused, [refusal("@type", "p:T", BUDGET + 4, 0, "p:T")]);
}

#[test]
fn a_long_token_is_quoted_as_an_excerpt_with_its_format_characters_escaped() {
    // Two bytes of prefix, a three-byte zero-width space and 59 letters are the 64 bytes quoted.
    let token = format!("p:\u{200b}{}", "k".repeat(200));
    let refused = expand(&json!({
        "@context": { "p": iri(BUDGET + 3) },
        "@graph": [ { "@id": "urn:n", "S231:hasInput": { "@id": token } } ]
    }))
    .expect_err("past the budget");
    let quoted = format!("p:\\u{{200B}}{}…", "k".repeat(59));
    let expanded = BUDGET + 3 + token.len() - 2;
    assert_eq!(refused.len(), 1);
    assert_eq!(
        refused[0].message,
        format!(
            "S231:hasInput reference `{quoted}` expands to {expanded} bytes through the \
             document's @context, {} more than it is written in; expansion had added 0 bytes, \
             and may add at most 8388608 to one document",
            expanded - token.len()
        )
    );
    assert_eq!(refused[0].subject.as_deref(), Some("urn:n"));
    assert!(refused[0].is_error());
}

#[test]
fn the_refusal_is_the_last_diagnostic_and_nothing_after_the_token_is_read() {
    // Each `p:T` adds a byte more than half the budget, so the second one does not fit.
    let binding = iri(BUDGET / 2 + 3);
    let adds = BUDGET / 2 + 1;
    let refused = expand(&json!({
        "@context": { "p": binding },
        "@graph": [
            { "@id": "urn:first", "S231:hasInput": { "@id": "relative" } },
            { "@id": "urn:second",
              "S231:hasInput": [ { "@id": "p:T" }, { "@id": "p:T" }, { "@id": "after" } ],
              "S231:hasOutput": { "@id": "after" },
              "@type": [ "https://data.ashrae.org/S231#Real", "\u{200b}T", " S231:Real" ],
              "S231:isOfDataType": { "@id": "https://data.ashrae.org/S231#Real" },
              "S231:unit": { "@id": "p:T" } },
            { "@id": "after", "S231:hasInput": { "@id": "after" } }
        ]
    }))
    .expect_err("past the budget");
    assert_eq!(refused.len(), 2, "{refused:?}");
    assert_eq!(refused[0].code, DiagCode::RelativeIri);
    assert_eq!(refused[0].subject.as_deref(), Some("urn:first"));
    assert_eq!(
        refused[1],
        refusal(
            "S231:hasInput reference",
            "p:T",
            adds + 3,
            adds,
            "urn:second"
        )
    );
}

#[test]
fn a_node_whose_own_id_does_not_fit_raises_nothing_else() {
    let refused = expand(&json!({
        "@context": { "p": iri(BUDGET + 3) },
        "@graph": [ { "@id": "p:T", "S231:hasInput": { "@id": "relative" },
                      "@type": "https://data.ashrae.org/S231#Real" } ]
    }))
    .expect_err("past the budget");
    assert_eq!(refused, [refusal("@id", "p:T", BUDGET + 4, 0, "p:T")]);
}

#[test]
fn a_token_that_is_only_read_is_never_charged() {
    // Every token here is read through a binding as long as the whole budget, so joining any
    // two of them would pass it.
    let binding = iri(BUDGET);
    let document = |node: Json| json!({ "@context": { "p": binding }, "@graph": [ node ] });

    // A suffix that holds whitespace has no absolute form: refused as relative, never joined.
    let refused = expand(&document(json!({
        "@id": "urn:n",
        "S231:hasInput": [ { "@id": "p:a b" }, { "@id": "p:c d" }, { "@id": "p:e f" } ]
    })))
    .expect_err("three relative references");
    assert_eq!(refused.len(), 3);
    assert!(
        refused
            .iter()
            .all(|diag| diag.code == DiagCode::RelativeIri)
    );

    // A typing token in surrounding whitespace is read trimmed, to see whether it names an S231
    // term; these do not, and stay as written.
    let expanded = expand(&document(json!({
        "@id": "urn:n", "@type": [ " p:a", "p:b ", " p:c " ]
    })))
    .expect("no near miss");
    let types = expanded.graph[0].r#type.as_ref().expect("types");
    assert_eq!(types.as_slice(), [" p:a", "p:b ", " p:c "]);

    // A pass-through key is classified in place unless it ends in `hasUnit`.
    let expanded = expand(&document(json!({
        "@id": "urn:n", "p:a": 0, "p:b": 0, "p:c": 0, "p:hasUnits": 0
    })))
    .expect("four keys the reader does not read");
    assert_eq!(expanded.graph[0].other.len(), 4);
}

#[test]
fn a_key_ending_in_has_unit_through_a_prefix_is_charged_and_refused_past_the_budget() {
    // The near-miss check reads the key's expansion whole, so a compact one is joined for it.
    let binding = iri(BUDGET / 2 + 3);
    let adds = BUDGET / 2 + 1;
    let one = json!({
        "@context": { "p": binding },
        "@graph": [ { "@id": "urn:n", "p:ahasUnit": 0 } ]
    });
    assert_eq!(expand(&one).expect("one fits").graph[0].other.len(), 1);

    // Nothing after the key that does not fit is read: neither the foreign-namespace key after
    // it on the same node nor the one on the next node is refused.
    let https = "https://data.ashrae.org/S231#";
    let two = json!({
        "@context": { "p": binding, "x": https },
        "@graph": [ { "@id": "urn:n", "p:ahasUnit": 0, "p:bhasUnit": 0, "p:chasUnit": 0,
                      "x:min": 0 },
                    { "@id": "urn:later", "x:max": 0 } ]
    });
    // `p:bhasUnit` is ten bytes; its expansion is the binding and the eight bytes after `p:`.
    assert_eq!(
        expand(&two).expect_err("the second does not fit"),
        [refusal(
            "property key",
            "p:bhasUnit",
            BUDGET / 2 + 3 + 8,
            adds,
            "urn:n"
        )]
    );

    // On a node with no `@id` the refusal names the node by its place.
    let unnamed = json!({
        "@context": { "p": iri(BUDGET + 3) },
        "@graph": [ { "@id": "urn:first" }, { "p:hasUnit": 0 } ]
    });
    assert_eq!(
        expand(&unnamed).expect_err("past the budget"),
        [refusal(
            "property key",
            "p:hasUnit",
            BUDGET + 3 + 7,
            0,
            "@graph[1]"
        )]
    );
}

#[test]
fn a_whole_term_key_ending_in_has_unit_is_read_where_it_is_bound_and_never_charged() {
    let binding = format!("{}hasUnit", iri(BUDGET + 3));
    let expanded = expand(&json!({
        "@context": { "t": binding },
        "@graph": [ { "@id": "urn:n", "t": 0 } ]
    }))
    .expect("the bound IRI is read in place");
    assert_eq!(expanded.graph[0].other.len(), 1);
}

#[test]
fn keys_and_tokens_are_charged_to_one_budget() {
    let binding = iri(BUDGET / 2 + 3);
    let adds = BUDGET / 2 + 1;
    let refused = expand(&json!({
        "@context": { "p": binding },
        "@graph": [ { "@id": "urn:n", "p:ahasUnit": 0, "@type": "p:T" } ]
    }))
    .expect_err("the key and the type together pass the budget");
    assert_eq!(refused, [refusal("@type", "p:T", adds + 3, adds, "urn:n")]);
}

#[test]
fn once_an_expansion_does_not_fit_no_later_one_is_made_and_nothing_more_is_raised() {
    let table: super::super::PrefixTable = [
        ("p".to_owned(), iri(BUDGET + 3)),
        ("q".to_owned(), "urn:q:".to_owned()),
    ]
    .into_iter()
    .collect();
    let expansion = |token| match super::super::expand_token(token, &table) {
        super::super::Expansion::Expanded(iri) => iri,
        _ => panic!("{token} expands"),
    };
    let site = Site {
        slot: "@type",
        reference: false,
        node: Some("urn:n"),
    };
    let mut budget = ExpansionBudget::new();
    let mut diags = DiagnosticList::new();
    assert!(!budget.has_ended());
    assert_eq!(
        budget
            .expand(expansion("q:a"), "q:a", site, &mut diags)
            .as_deref(),
        Some("urn:q:a")
    );
    assert!(!budget.has_ended() && diags.is_empty());

    assert_eq!(
        budget.expand(expansion("p:T"), "p:T", site, &mut diags),
        None
    );
    assert!(budget.has_ended());
    assert_eq!(diags.len(), 1);

    // One that would have fit is not made, and a second that would not is not refused again.
    assert_eq!(
        budget.expand(expansion("q:a"), "q:a", site, &mut diags),
        None
    );
    assert_eq!(
        budget.expand(expansion("p:T"), "p:T", site, &mut diags),
        None
    );
    assert_eq!(
        diags.into_iter().collect::<Vec<_>>(),
        [refusal("@type", "p:T", BUDGET + 4, 4, "urn:n")]
    );
}

#[test]
fn a_node_whose_long_own_id_does_not_fit_is_the_subject_whole() {
    // The message quotes 64 bytes of the token; the subject is the identity as written.
    let token = format!("p:{}", "k".repeat(200));
    let refused = expand(&json!({
        "@context": { "p": iri(BUDGET + 3) },
        "@graph": [ { "@id": token } ]
    }))
    .expect_err("past the budget");
    assert_eq!(refused.len(), 1);
    assert_eq!(refused[0].subject.as_deref(), Some(token.as_str()));
    assert!(
        refused[0]
            .message
            .starts_with(&format!("@id `p:{}…` expands to ", "k".repeat(62))),
        "{}",
        refused[0].message
    );
}

#[test]
fn a_typing_token_that_does_not_fit_is_not_judged_as_it_is_written() {
    // Left as written, `S231:NoSuchTerm` would be refused as no term of the vocabulary. It is
    // refused for the budget alone: ten bytes are left and its expansion adds 23.
    let namespace = "http://data.ashrae.org/S231#";
    let refused = expand(&json!({
        "@context": { "S231": namespace, "p": iri(BUDGET - 8) },
        "@graph": [ { "@id": "urn:n", "@type": [ "p:T", "S231:NoSuchTerm" ] } ]
    }))
    .expect_err("past the budget");
    let expanded = namespace.len() + "NoSuchTerm".len();
    assert_eq!(
        refused,
        [refusal(
            "@type",
            "S231:NoSuchTerm",
            expanded,
            BUDGET - 10,
            "urn:n"
        )]
    );
}

#[test]
fn nothing_of_a_later_node_is_read_once_a_key_did_not_fit() {
    // The later node writes a key twice and carries a `@context` of its own; each is a refusal
    // where it is read. Written as text, since a JSON value cannot hold a key twice.
    let binding = iri(BUDGET + 3);
    let bytes = format!(
        r#"{{ "@context": {{ "p": "{binding}" }},
             "@graph": [ {{ "@id": "urn:n", "p:hasUnit": 0 }},
                         {{ "@id": "urn:later", "k": 0, "k": 1, "@context": {{}} }} ] }}"#
    );
    let doc = crate::parse_document(bytes.as_bytes()).expect("parse test document");
    let refused: Vec<Diagnostic> = expand_document(&doc, Some(bytes.as_bytes()))
        .expect_err("past the budget")
        .into_iter()
        .collect();
    assert_eq!(
        refused,
        [refusal(
            "property key",
            "p:hasUnit",
            BUDGET + 3 + 7,
            0,
            "urn:n"
        )]
    );
}
