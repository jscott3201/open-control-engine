//! An S231-namespace typing token, a `@type` value or an `isOfDataType` reference on any node, must
//! name a term of the S231 vocabulary the engine reads: a CXF-Core class or data type, a term the
//! engine writes, a registered CDL block class or a CDL or G36 enumeration type, each by its full
//! path (GitHub #620; owner rulings of 2026-11-12). Anything else is refused as
//! `malformed-document`, subject the node, by the expansion pass. Every `https:`-spelled S231 typing
//! token is refused as `vocabulary-namespace-mismatch`.
//!
//! Before #620 a composite never read its `@type`, a connector declaring `isOfDataType` never read
//! its own, and a parameter or any other node read none, so a misspelt S231 type there loaded; an
//! `isOfDataType` naming a CDL enumeration was matched by its last dotted segment alone.
//!
//! Every refusal is pinned as the complete diagnostic list, stated by hand.

use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::{DiagCode, Diagnostic};
use serde_json::{Value as Json, json};

const S231_NS: &str = "http://data.ashrae.org/S231#";
const ROOT: &str = "http://example.org#R";
const SUB: &str = "http://example.org#R.sub";
const CON: &str = "http://example.org#R.con";
const K: &str = "http://example.org#R.con.k";
const Y: &str = "http://example.org#R.con.y";
const ORPHAN: &str = "http://example.org#R.note";
const CONSTANT: &str = "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant";

/// The 33 vocabulary terms, as the engine's table holds them (its pin tests tie them to
/// CXF-Core); stated again here as the document-level contract.
const TERMS: [&str; 33] = [
    "AnalogDataType",
    "AnalogInput",
    "AnalogOutput",
    "Block",
    "BooleanDataType",
    "CompositeBlock",
    "Connector",
    "Constant",
    "DataType",
    "ElementaryBlock",
    "EnumerationDatatype",
    "ExtensionBlock",
    "InputConnector",
    "IntegerDataType",
    "IntegerInput",
    "IntegerOutput",
    "OutputConnector",
    "Package",
    "Parameter",
    "RealDatatype",
    "RealInput",
    "RealOutput",
    "StringDatatype",
    "Boolean",
    "BooleanInput",
    "BooleanOutput",
    "Datatype",
    "EnumerationType",
    "Integer",
    "Real",
    "String",
    "StringInput",
    "StringOutput",
];

/// A root holding a nested composite that holds a `Constant`, plus an orphan node, under
/// `context`.
fn document(context: Json) -> Json {
    json!({
        "@context": context,
        "@graph": [
            { "@id": ROOT, "@type": "S231:Block", "S231:containsBlock": { "@id": SUB } },
            { "@id": SUB, "@type": "S231:Block", "S231:containsBlock": { "@id": CON } },
            {
                "@id": CON,
                "@type": CONSTANT,
                "S231:hasParameter": { "@id": K },
                "S231:hasOutput": { "@id": Y }
            },
            { "@id": K, "@type": "S231:Parameter", "S231:value": 1.0 },
            {
                "@id": Y,
                "@type": "S231:RealOutput",
                "S231:isOfDataType": { "@id": "S231:Real" }
            },
            { "@id": ORPHAN, "@type": "S231:Parameter" }
        ]
    })
}

fn base() -> Json {
    document(json!({ "S231": S231_NS }))
}

/// `doc` with the node `id`'s `key` set to `value`.
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

fn loads(doc: &Json) -> usize {
    import(doc).unwrap_or_else(|e| panic!("expected a load: {e:?}"))
}

fn refusal(doc: &Json) -> Vec<Diagnostic> {
    match import(doc) {
        Err(CxfError::Validation(diags)) => diags,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

/// `text` as a message quotes it: whole up to 64 bytes, otherwise cut on a character boundary
/// and marked `…` (`oce_cxf`'s excerpt rule, restated).
fn excerpt(text: &str) -> String {
    if text.len() <= 64 {
        return text.to_owned();
    }
    let mut end = 64;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

/// The refusal of `token` (as expanded) in `slot` on `owner`, naming `term`, each quoted as an
/// excerpt.
fn unknown(owner: &str, slot: &str, token: &str, term: &str) -> Diagnostic {
    let (quoted, term) = (excerpt(token), excerpt(term));
    Diagnostic::error(
        DiagCode::MalformedDocument,
        format!(
            "{slot} `{quoted}` on node `{owner}` names `{term}`, which is no term of the S231 \
             vocabulary the engine reads (a CXF-Core class or data type, a term the engine writes, \
             a registered CDL block class, or a CDL or G36 enumeration type by its full path)"
        ),
    )
    .with_subject(owner)
}

#[test]
fn every_vocabulary_term_class_and_enumeration_is_accepted() {
    assert_eq!(loads(&base()), 1);
    // The orphan's type is read by nothing else, so every term the vocabulary holds loads there
    // in both slots; an extension block is refused for being one, not for its term.
    let classes = oce_blocks::catalog()
        .iter()
        .filter(|entry| entry.class_path.starts_with("CDL."))
        .map(|entry| format!("Buildings.Controls.OBC.{}", entry.class_path));
    let enumerations = oce_model::enumeration_classes()
        .iter()
        .map(|row| row.qualified_path.to_owned());
    let terms: Vec<String> = TERMS
        .iter()
        .map(|term| (*term).to_owned())
        .chain(classes)
        .chain(enumerations)
        .collect();
    assert_eq!(terms.len(), 33 + 133 + 18);
    for term in terms.iter().filter(|term| *term != "ExtensionBlock") {
        for key in ["@type", "S231:isOfDataType"] {
            let value = if key == "@type" {
                json!(format!("S231:{term}"))
            } else {
                json!({ "@id": format!("S231:{term}") })
            };
            assert_eq!(loads(&with(base(), ORPHAN, key, value)), 1, "{key} {term}");
        }
    }
}

#[test]
fn a_term_outside_the_vocabulary_is_refused_on_every_role() {
    // (node, slot, token as written, token as expanded, term). Before: each loaded, but the leaf
    // (`class-not-found`) and the two data types with no last-segment match
    // (`unresolved-reference`).
    let full = |term: &str| format!("{S231_NS}{term}");
    let cases = [
        (
            ROOT,
            "@type",
            "S231:ExtensonBlock".to_owned(),
            "ExtensonBlock",
        ),
        (ROOT, "@type", "S231:block".to_owned(), "block"),
        (ROOT, "@type", full("Blok"), "Blok"),
        (
            ROOT,
            "@type",
            "S231:Block\u{200b}".to_owned(),
            "Block\u{200b}",
        ),
        (
            ROOT,
            "@type",
            "S231:\u{feff}Block".to_owned(),
            "\u{feff}Block",
        ),
        (ROOT, "@type", "S231:Bl\u{ad}ock".to_owned(), "Bl\u{ad}ock"),
        (
            SUB,
            "@type",
            "S231:CompositBlock".to_owned(),
            "CompositBlock",
        ),
        (
            CON,
            "@type",
            "S231:Buildings.Controls.OBC.CDL.Reals.Sources.Constantt".to_owned(),
            "Buildings.Controls.OBC.CDL.Reals.Sources.Constantt",
        ),
        (Y, "@type", "S231:RealOutptu".to_owned(), "RealOutptu"),
        (K, "@type", "S231:Paramter".to_owned(), "Paramter"),
        (ORPHAN, "@type", "S231:Foo".to_owned(), "Foo"),
        (Y, "S231:isOfDataType", "S231:Reals".to_owned(), "Reals"),
        // An enumeration by its last segment, or by a path no row has: matched by full path now.
        (
            K,
            "S231:isOfDataType",
            "S231:ZeroTime".to_owned(),
            "ZeroTime",
        ),
        (
            K,
            "S231:isOfDataType",
            "S231:Buildings.Controls.OBC.CDL.Typos.ZeroTime".to_owned(),
            "Buildings.Controls.OBC.CDL.Typos.ZeroTime",
        ),
        // The G36 fixture's old spelling: a compact prefix glued to a full IRI.
        (
            K,
            "S231:isOfDataType",
            "S231:http://example.org#Buildings.Controls.OBC.CDL.Types.SimpleController".to_owned(),
            "http://example.org#Buildings.Controls.OBC.CDL.Types.SimpleController",
        ),
    ];
    for (node, slot, token, term) in cases {
        let value = if slot == "@type" {
            json!(token)
        } else {
            json!({ "@id": token })
        };
        let key = if slot == "@type" {
            "@type"
        } else {
            "S231:isOfDataType"
        };
        let quoted = token
            .strip_prefix("S231:")
            .map_or_else(|| token.clone(), &full);
        assert_eq!(
            refusal(&with(base(), node, key, value)),
            vec![unknown(node, slot, &quoted, term)],
            "{node} {slot} {token:?}"
        );
    }
}

#[test]
fn the_term_is_read_through_the_document_s_own_bindings() {
    // No `S231` binding: a compact `S231:` term is read in the S231 namespace, as written.
    let unbound = with(document(json!({})), ROOT, "@type", json!("S231:Blok"));
    assert_eq!(
        refusal(&unbound),
        vec![unknown(ROOT, "@type", "S231:Blok", "Blok")]
    );
    // Another prefix bound to the namespace, and a declared term that expands into it.
    let context = json!({ "S231": S231_NS, "s231": S231_NS, "Bad": format!("{S231_NS}Blok") });
    for token in ["s231:Blok", "Bad"] {
        let doc = with(document(context.clone()), ROOT, "@type", json!(token));
        assert_eq!(
            refusal(&doc),
            vec![unknown(ROOT, "@type", &format!("{S231_NS}Blok"), "Blok")],
            "{token}"
        );
    }
    // Inner whitespace keeps the token from expanding; it is still read in the namespace.
    let doc = with(base(), ROOT, "@type", json!("S231:Extension Block"));
    assert_eq!(
        refusal(&doc),
        vec![unknown(
            ROOT,
            "@type",
            "S231:Extension Block",
            "Extension Block"
        )]
    );
}

#[test]
fn a_token_in_another_namespace_is_left_to_the_readers() {
    // A composite's class in another namespace is not judged here (a follow-up issue covers it);
    // a leaf's is still the registry's (`class-not-found`).
    assert_eq!(
        loads(&with(
            base(),
            ROOT,
            "@type",
            json!("http://example.org#Blok")
        )),
        1
    );
    let diags = refusal(&with(base(), CON, "@type", json!(format!("{CONSTANT}t"))));
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, DiagCode::ClassNotFound);
    // An enumeration IRI in another namespace is still matched by its last segment (a follow-up
    // issue covers it): this one names a G36 class with a typo in its package and loads.
    let doc = with(
        base(),
        ORPHAN,
        "S231:isOfDataType",
        json!({ "@id": "http://example.org#Buildings.Controls.OBC.CDL.Typos.ZeroTime" }),
    );
    assert_eq!(loads(&doc), 1);
}

#[test]
fn every_https_spelled_typing_token_is_a_namespace_mismatch() {
    // Before, only the `https:` extension block was refused; any other term loaded, read by its
    // trailing term.
    let mismatch = |owner: &str, slot: &str, token: &str, namespace: &str, term: &str| {
        Diagnostic::error(
            DiagCode::VocabularyNamespaceMismatch,
            format!(
                "{slot} `{token}` on node `{owner}` is in `{namespace}`; the S231 vocabulary is \
                 read only in `{S231_NS}`; write it as `S231:{term}`"
            ),
        )
        .with_subject(owner)
    };
    let cases = [
        (
            ROOT,
            "@type",
            "https://data.ashrae.org/S231#Block",
            "https://data.ashrae.org/S231#",
            "Block",
        ),
        (
            Y,
            "@type",
            "https://data.ashrae.org/S231P#RealOutput",
            "https://data.ashrae.org/S231P#",
            "RealOutput",
        ),
        (
            Y,
            "S231:isOfDataType",
            "https://data.ashrae.org/S231#Real",
            "https://data.ashrae.org/S231#",
            "Real",
        ),
        (
            ORPHAN,
            "@type",
            "https://data.ashrae.org/S231#Blok",
            "https://data.ashrae.org/S231#",
            "Blok",
        ),
    ];
    for (node, slot, token, namespace, term) in cases {
        let key = if slot == "@type" {
            "@type"
        } else {
            "S231:isOfDataType"
        };
        let value = if slot == "@type" {
            json!(token)
        } else {
            json!({ "@id": token })
        };
        assert_eq!(
            refusal(&with(base(), node, key, value)),
            vec![mismatch(node, slot, token, namespace, term)],
            "{token}"
        );
    }
    // The extension block keeps its own message.
    let token = "https://data.ashrae.org/S231#ExtensionBlock";
    assert_eq!(
        refusal(&with(base(), ROOT, "@type", json!(token))),
        vec![
            Diagnostic::error(
                DiagCode::VocabularyNamespaceMismatch,
                format!(
                    "@type `{token}` on node `{ROOT}` names the class `ExtensionBlock` in \
                     `https://data.ashrae.org/S231#`; the S231 vocabulary is read only in \
                     `{S231_NS}`; write it as `S231:ExtensionBlock`"
                ),
            )
            .with_subject(ROOT)
        ]
    );
}

#[test]
fn the_earlier_typing_checks_still_come_first_and_alone() {
    // The retired namespace, refused as before.
    let token = "http://data.ashrae.org/S231P#Blok";
    assert_eq!(
        refusal(&with(base(), ROOT, "@type", json!(token))),
        vec![
            Diagnostic::error(
                DiagCode::VocabularyNamespaceMismatch,
                format!(
                    "@type `{token}` on node `{ROOT}` is in `http://data.ashrae.org/S231P#`; the \
                     S231 vocabulary is read only in `{S231_NS}`"
                ),
            )
            .with_subject(ROOT)
        ]
    );
    // A whitespace near miss (GitHub #619) keeps its own refusal, which names the term as written.
    assert_eq!(
        refusal(&with(base(), ROOT, "@type", json!("S231:Blok "))),
        vec![
            Diagnostic::error(
                DiagCode::MalformedDocument,
                format!(
                    "@type \"S231:Blok \" on node `{ROOT}` carries surrounding whitespace; without \
                     it, it names `Blok` in `{S231_NS}`; write it as `S231:Blok`"
                ),
            )
            .with_subject(ROOT)
        ]
    );
}

#[test]
fn a_long_term_is_quoted_as_an_excerpt() {
    let term = format!("{}{}", "B".repeat(70), "k".repeat(1 << 20));
    let doc = with(base(), ROOT, "@type", json!(format!("S231:{term}")));
    let expected = unknown(ROOT, "@type", &format!("{S231_NS}{term}"), &term);
    assert_eq!(refusal(&doc), vec![expected.clone()]);
    // The message quotes at most 64 bytes of each, whatever the term's length.
    assert!(expected.message.len() < 400, "{}", expected.message.len());
    assert!(expected.message.contains(&format!("`{}…`", "B".repeat(64))));
}

#[test]
fn every_refusal_in_a_document_is_reported_in_a_stable_order() {
    let mut doc = with(base(), ROOT, "@type", json!("S231:Blok"));
    doc = with(doc, K, "@type", json!("S231:Paramter"));
    doc = with(doc, Y, "S231:isOfDataType", json!({ "@id": "S231:Reals" }));
    doc = with(doc, ORPHAN, "@type", json!(["S231:Parameter", "S231:Foo"]));
    let first = refusal(&doc);
    let mut expected = vec![
        unknown(ROOT, "@type", &format!("{S231_NS}Blok"), "Blok"),
        unknown(K, "@type", &format!("{S231_NS}Paramter"), "Paramter"),
        unknown(Y, "S231:isOfDataType", &format!("{S231_NS}Reals"), "Reals"),
        unknown(ORPHAN, "@type", &format!("{S231_NS}Foo"), "Foo"),
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
