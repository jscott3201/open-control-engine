//! A typing token (a `@type` value or an `isOfDataType` reference) that carries a Unicode format
//! (`Cf`) or control (`Cc`) character inside its surrounding whitespace is refused as
//! `malformed-document`, subject the node, in any namespace and on any node (GitHub #641; owner
//! ruling of 2026-11-12). The message names the first such code point and quotes the token with
//! each such character escaped, never as it is.
//!
//! Before, such a token escaped every check: before or inside the `S231:` prefix it never expands,
//! so it was read as no S231 type, and a nested composite typed `" \u{200B}S231:ExtensionBlock"`
//! lowered its children as a composite where an FMU was meant.
//!
//! Every refusal is pinned as the complete diagnostic list, stated by hand.

use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::{DiagCode, Diagnostic};
use serde_json::{Value as Json, json};

const S231_NS: &str = "http://data.ashrae.org/S231#";
const ROOT: &str = "http://example.org#R";
const SUB: &str = "http://example.org#R.sub";
const K: &str = "http://example.org#R.con.k";
const Y: &str = "http://example.org#R.con.y";

/// The invisible characters the review found escaping every check, with the category each is in.
const INVISIBLE: [(char, &str); 6] = [
    ('\u{200B}', "Cf"),
    ('\u{FEFF}', "Cf"),
    ('\u{2060}', "Cf"),
    ('\u{180E}', "Cf"),
    ('\u{AD}', "Cf"),
    ('\u{1}', "Cc"),
];

/// A root holding a nested composite that holds a `Constant`.
fn base() -> Json {
    json!({
        "@context": { "S231": S231_NS },
        "@graph": [
            { "@id": ROOT, "@type": "S231:Block", "S231:containsBlock": { "@id": SUB } },
            {
                "@id": SUB,
                "@type": "S231:Block",
                "S231:containsBlock": { "@id": "http://example.org#R.con" }
            },
            {
                "@id": "http://example.org#R.con",
                "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
                "S231:hasParameter": { "@id": K },
                "S231:hasOutput": { "@id": Y }
            },
            { "@id": K, "@type": "S231:Parameter", "S231:value": 1.0 },
            {
                "@id": Y,
                "@type": "S231:RealOutput",
                "S231:isOfDataType": { "@id": "S231:Real" }
            }
        ]
    })
}

fn with(mut doc: Json, id: &str, key: &str, value: Json) -> Json {
    let node = doc["@graph"]
        .as_array_mut()
        .expect("@graph")
        .iter_mut()
        .find(|node| node["@id"] == id)
        .unwrap_or_else(|| panic!("no node `{id}`"));
    node[key] = value;
    doc
}

fn import(doc: &Json) -> Result<usize, CxfError> {
    let bytes = serde_json::to_vec(doc).expect("serialize");
    import_cxf(&bytes, &ResolveOptions::default()).map(|(graph, _)| graph.blocks.len())
}

fn refusal(doc: &Json) -> Vec<Diagnostic> {
    let diags = match import(doc) {
        Err(CxfError::Validation(diags)) => diags,
        other => panic!("expected a refusal, got {other:?}"),
    };
    for diag in &diags {
        assert!(
            !diag.message.chars().any(is_invisible),
            "a message carries an invisible character as it is: {:?}",
            diag.message
        );
    }
    diags
}

/// The characters this file writes into tokens, as the engine's table classifies them.
fn is_invisible(c: char) -> bool {
    c.is_control() || INVISIBLE.iter().any(|(invisible, _)| *invisible == c)
}

/// `token` as a message quotes it: each invisible character written `\u{…}`.
fn visible(token: &str) -> String {
    token
        .chars()
        .map(|c| {
            if is_invisible(c) {
                format!("\\u{{{:04X}}}", u32::from(c))
            } else {
                c.to_string()
            }
        })
        .collect()
}

/// The refusal of `token` in `slot` on `owner`, which carries `c` of `category`.
fn invisible(owner: &str, slot: &str, token: &str, c: char, category: &str) -> Diagnostic {
    let kind = if category == "Cc" {
        "a control character (Cc)"
    } else {
        "a format character (Cf)"
    };
    Diagnostic::error(
        DiagCode::MalformedDocument,
        format!(
            "{slot} `{}` on node `{owner}` carries U+{:04X}, {kind}, which no reader can see; write \
             the type without format or control characters",
            visible(token),
            u32::from(c)
        ),
    )
    .with_subject(owner)
}

#[test]
fn the_control_loads() {
    assert_eq!(import(&base()).expect("loads"), 1);
}

#[test]
fn an_invisible_character_is_refused_before_inside_and_after_the_prefix_and_in_any_namespace() {
    for (c, category) in INVISIBLE {
        let tokens = [
            format!(" {c}S231:Block"),
            format!("S{c}231:Block"),
            format!("S231:Bl{c}ock"),
            format!("{c}{S231_NS}Block"),
            format!("{S231_NS}Bl{c}ock"),
            format!("http://example.org#Bl{c}ock"),
        ];
        for token in tokens {
            assert_eq!(
                refusal(&with(base(), ROOT, "@type", json!(token))),
                vec![invisible(ROOT, "@type", &token, c, category)],
                "{token:?}"
            );
        }
    }
}

#[test]
fn a_nested_composite_typed_an_invisible_extension_block_is_refused_not_lowered() {
    // The review's case: before, each of these loaded and its children ran as a composite.
    for (c, category) in INVISIBLE {
        for token in [
            format!(" {c}S231:ExtensionBlock"),
            format!("S{c}231:ExtensionBlock"),
            format!("{c}{S231_NS}ExtensionBlock"),
        ] {
            assert_eq!(
                refusal(&with(base(), SUB, "@type", json!(token))),
                vec![invisible(SUB, "@type", &token, c, category)],
                "{token:?}"
            );
        }
    }
}

#[test]
fn an_invisible_character_is_refused_on_a_connector_a_parameter_and_a_data_type() {
    for (c, category) in INVISIBLE {
        let cases = [
            (Y, "@type", format!("S231:Real{c}Output")),
            (Y, "@type", format!(" {c}S231:RealOutput")),
            (K, "@type", format!("S231:Para{c}meter")),
            (K, "@type", format!("http://example.org#Para{c}meter")),
            (Y, "S231:isOfDataType", format!("S231:Re{c}al")),
            (Y, "S231:isOfDataType", format!("{c}{S231_NS}Real")),
            (
                Y,
                "S231:isOfDataType",
                format!("http://example.org#Re{c}al"),
            ),
        ];
        for (node, slot, token) in cases {
            let value = if slot == "@type" {
                json!(token)
            } else {
                json!({ "@id": token })
            };
            assert_eq!(
                refusal(&with(base(), node, slot, value)),
                vec![invisible(node, slot, &token, c, category)],
                "{node} {slot} {token:?}"
            );
        }
    }
}

#[test]
fn the_check_runs_first_and_leaves_surrounding_whitespace_to_the_near_miss_check() {
    // Surrounding whitespace alone, a tab or a line break or U+0085 included (each a control
    // character that is also whitespace), is #619's near miss, as before.
    for token in ["S231:Block\t", "\nS231:Block", "\u{85}S231:Block"] {
        let diags = refusal(&with(base(), ROOT, "@type", json!(token)));
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert!(
            diags[0].message.contains("carries surrounding whitespace"),
            "{diags:?}"
        );
    }
    // Whitespace around and an invisible character inside: this refusal, once.
    let token = "S231:Bl\u{200B}ock ";
    assert_eq!(
        refusal(&with(base(), ROOT, "@type", json!(token))),
        vec![invisible(ROOT, "@type", token, '\u{200B}', "Cf")]
    );
    // A tab inside a token is a control character inside its surrounding whitespace.
    let token = "S231:Bl\tock";
    assert_eq!(
        refusal(&with(base(), ROOT, "@type", json!(token))),
        vec![invisible(ROOT, "@type", token, '\t', "Cc")]
    );
    // An astral format character is named by its full code point.
    let token = "S231:Block\u{E0001}";
    let diags = refusal(&with(base(), ROOT, "@type", json!(token)));
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert!(
        diags[0]
            .message
            .contains("carries U+E0001, a format character (Cf)"),
        "{diags:?}"
    );
    assert!(
        diags[0].message.contains("S231:Block\\u{E0001}"),
        "{diags:?}"
    );
}

#[test]
fn a_long_token_is_quoted_as_an_escaped_excerpt() {
    let token = format!("\u{200B}S231:{}", "B".repeat(1 << 20));
    let diags = refusal(&with(base(), ROOT, "@type", json!(token)));
    assert_eq!(diags.len(), 1, "{diags:?}");
    // The excerpt keeps 64 bytes of the token (the 3-byte U+200B, `S231:`, then letters), with the
    // invisible character escaped.
    let expected = format!("`\\u{{200B}}S231:{}…`", "B".repeat(64 - 3 - 5));
    assert!(diags[0].message.contains(&expected), "{}", diags[0].message);
    assert!(diags[0].message.len() < 300, "{}", diags[0].message.len());
}

#[test]
fn every_refusal_in_a_document_is_reported_in_a_stable_order() {
    let mut doc = with(base(), ROOT, "@type", json!("S\u{200B}231:Block"));
    doc = with(doc, SUB, "@type", json!(" \u{FEFF}S231:ExtensionBlock"));
    doc = with(doc, K, "@type", json!("S231:Para\u{AD}meter"));
    doc = with(
        doc,
        Y,
        "S231:isOfDataType",
        json!({ "@id": "S231:Re\u{1}al" }),
    );
    let first = refusal(&doc);
    let mut expected = vec![
        invisible(ROOT, "@type", "S\u{200B}231:Block", '\u{200B}', "Cf"),
        invisible(
            SUB,
            "@type",
            " \u{FEFF}S231:ExtensionBlock",
            '\u{FEFF}',
            "Cf",
        ),
        invisible(K, "@type", "S231:Para\u{AD}meter", '\u{AD}', "Cf"),
        invisible(Y, "S231:isOfDataType", "S231:Re\u{1}al", '\u{1}', "Cc"),
    ];
    let order = |a: &Diagnostic, b: &Diagnostic| {
        (a.subject.as_deref(), &a.message).cmp(&(b.subject.as_deref(), &b.message))
    };
    expected.sort_by(order);
    let mut sorted = first.clone();
    sorted.sort_by(order);
    assert_eq!(sorted, expected);
    for _ in 0..8 {
        assert_eq!(refusal(&doc), first);
    }
}
