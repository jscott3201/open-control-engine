//! A typing token that names an S231 term only once its surrounding whitespace is trimmed is
//! refused as `malformed-document`, subject the node that carries it (GitHub issue #587). No
//! whitespace-bearing token expands, so before this rule it was kept as written and read as no S231
//! type at all: a composite never reads its `@type`, so a root typed `"S231:ExtensionBlock "` with
//! a child loaded and ran the child, and a connector typed `"S231:RealOutput "` loaded on its
//! `isOfDataType`.
//!
//! Every refusal is pinned as the complete diagnostic list, stated by hand.

use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::{DiagCode, Diagnostic};
use serde_json::{Value as Json, json};

const S231_NS: &str = "http://data.ashrae.org/S231#";
const ROOT: &str = "http://example.org#R";
const CON: &str = "http://example.org#R.con";
const CONSTANT: &str = "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant";

/// A root typed `S231:Block` with one `Constant` child, under `context`.
fn document(context: Json) -> Json {
    json!({
        "@context": context,
        "@graph": [
            { "@id": ROOT, "@type": "S231:Block", "S231:containsBlock": { "@id": CON } },
            {
                "@id": CON,
                "@type": CONSTANT,
                "S231:hasParameter": { "@id": "http://example.org#R.con.k" },
                "S231:hasOutput": { "@id": "http://example.org#R.con.y" }
            },
            { "@id": "http://example.org#R.con.k", "S231:value": 1.0 },
            {
                "@id": "http://example.org#R.con.y",
                "@type": "S231:RealOutput",
                "S231:isOfDataType": { "@id": "S231:Real" }
            }
        ]
    })
}

/// [`document`] under the usual context.
fn base() -> Json {
    document(json!({ "S231": S231_NS }))
}

/// A root with a nested composite `R.sub`, which holds the `Constant`.
fn nested() -> Json {
    let mut doc = base();
    let graph = doc["@graph"].as_array_mut().expect("@graph");
    graph[0]["S231:containsBlock"] = json!({ "@id": "http://example.org#R.sub" });
    graph.push(json!({
        "@id": "http://example.org#R.sub",
        "@type": "S231:Block",
        "S231:containsBlock": { "@id": CON }
    }));
    doc
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
    import_cxf(&bytes, &ResolveOptions::default()).map(|(graph, report)| {
        assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
        graph.blocks.len()
    })
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

/// The refusal of `token` in `slot` on `owner`, which names `term` in `namespace`.
fn near_miss(owner: &str, slot: &str, token: &str, term: &str, namespace: &str) -> Diagnostic {
    Diagnostic::error(
        DiagCode::MalformedDocument,
        format!(
            "{slot} {token:?} on node `{owner}` carries surrounding whitespace; without it, it \
             names `{term}` in `{namespace}`; write it as `S231:{term}`"
        ),
    )
    .with_subject(owner)
}

#[test]
fn the_controls_load() {
    assert_eq!(loads(&base()), 1);
    assert_eq!(loads(&nested()), 1);
}

#[test]
fn a_root_typed_a_near_miss_of_extensionblock_is_refused() {
    // The issue's reproduction. Before: the root loaded and ran its child.
    let doc = with(base(), ROOT, "@type", json!("S231:ExtensionBlock "));
    assert_eq!(
        refusal(&doc),
        vec![
            Diagnostic::error(
                DiagCode::MalformedDocument,
                "@type \"S231:ExtensionBlock \" on node `http://example.org#R` carries \
                 surrounding whitespace; without it, it names `ExtensionBlock` in \
                 `http://data.ashrae.org/S231#`; write it as `S231:ExtensionBlock`",
            )
            .with_subject(ROOT)
        ]
    );
}

#[test]
fn every_whitespace_and_spelling_of_an_s231_type_is_refused_on_the_root() {
    let full = format!("{S231_NS}ExtensionBlock");
    for token in [
        " S231:ExtensionBlock".to_owned(),
        "S231:ExtensionBlock\t".to_owned(),
        "\nS231:Block\n".to_owned(),
        "S231:Block\u{a0}".to_owned(),
        format!("{full} "),
        format!(" {full}"),
    ] {
        let term = if token.contains("ExtensionBlock") {
            "ExtensionBlock"
        } else {
            "Block"
        };
        let doc = with(base(), ROOT, "@type", json!(token));
        assert_eq!(
            refusal(&doc),
            vec![near_miss(ROOT, "@type", &token, term, S231_NS)],
            "{token:?}"
        );
    }
    // One near miss in a list of types is enough.
    let doc = with(
        base(),
        ROOT,
        "@type",
        json!(["S231:Block", "S231:ExtensionBlock "]),
    );
    assert_eq!(
        refusal(&doc),
        vec![near_miss(
            ROOT,
            "@type",
            "S231:ExtensionBlock ",
            "ExtensionBlock",
            S231_NS
        )]
    );
}

#[test]
fn a_near_miss_is_refused_on_a_nested_composite_a_leaf_a_connector_and_a_parameter() {
    // Before, each loaded except the leaf, which no class matched (`class-not-found`).
    let cases = [
        (
            nested(),
            "http://example.org#R.sub",
            "@type",
            "S231:ExtensionBlock ",
            "ExtensionBlock",
        ),
        (
            base(),
            CON,
            "@type",
            " S231:ExtensionBlock",
            "ExtensionBlock",
        ),
        (
            base(),
            "http://example.org#R.con.y",
            "@type",
            "S231:RealOutput ",
            "RealOutput",
        ),
        (
            base(),
            "http://example.org#R.con.k",
            "@type",
            "S231:Parameter ",
            "Parameter",
        ),
    ];
    for (doc, owner, slot, token, term) in cases {
        let doc = with(doc, owner, slot, json!(token));
        assert_eq!(
            refusal(&doc),
            vec![near_miss(owner, slot, token, term, S231_NS)],
            "{owner}"
        );
    }
}

#[test]
fn a_near_miss_data_type_is_refused_by_the_same_rule() {
    // Before: `unresolved-reference`, "unresolved isOfDataType", subject the token.
    let owner = "http://example.org#R.con.y";
    let doc = with(
        base(),
        owner,
        "S231:isOfDataType",
        json!({ "@id": "S231:Real " }),
    );
    assert_eq!(
        refusal(&doc),
        vec![near_miss(
            owner,
            "S231:isOfDataType",
            "S231:Real ",
            "Real",
            S231_NS
        )]
    );
}

#[test]
fn a_near_miss_in_a_foreign_namespace_is_refused_as_a_near_miss() {
    // Before, all three loaded: the retired namespace is refused only as a prefix of the token as
    // written, the `https:` extension block only as an exact spelling, and no other `https:` type
    // at all.
    for (token, namespace) in [
        (
            " http://data.ashrae.org/S231P#ExtensionBlock",
            "http://data.ashrae.org/S231P#",
        ),
        (
            "https://data.ashrae.org/S231#ExtensionBlock ",
            "https://data.ashrae.org/S231#",
        ),
        (
            "https://data.ashrae.org/S231P#Block\t",
            "https://data.ashrae.org/S231P#",
        ),
    ] {
        let term = if token.contains("ExtensionBlock") {
            "ExtensionBlock"
        } else {
            "Block"
        };
        let doc = with(base(), ROOT, "@type", json!(token));
        assert_eq!(
            refusal(&doc),
            vec![near_miss(ROOT, "@type", token, term, namespace)],
            "{token:?}"
        );
    }
    // The retired namespace with only trailing whitespace is a near miss too, reported once.
    // Before: `vocabulary-namespace-mismatch`, since the token as written starts with it.
    let token = "http://data.ashrae.org/S231P#Block ";
    let doc = with(base(), ROOT, "@type", json!(token));
    assert_eq!(
        refusal(&doc),
        vec![near_miss(
            ROOT,
            "@type",
            token,
            "Block",
            "http://data.ashrae.org/S231P#"
        )]
    );
}

#[test]
fn a_near_miss_is_read_through_the_document_s_own_bindings() {
    // A document that binds no `S231` prefix reads compact `S231:` terms in the S231 namespace;
    // another prefix bound to it, and a term bound to an S231 class, read there too.
    let unbound = with(
        document(json!({})),
        ROOT,
        "@type",
        json!("S231:ExtensionBlock "),
    );
    assert_eq!(
        refusal(&unbound),
        vec![near_miss(
            ROOT,
            "@type",
            "S231:ExtensionBlock ",
            "ExtensionBlock",
            S231_NS
        )]
    );
    let context =
        json!({ "S231": S231_NS, "s231": S231_NS, "Ext": format!("{S231_NS}ExtensionBlock") });
    for token in ["s231:ExtensionBlock ", " Ext"] {
        let doc = with(document(context.clone()), ROOT, "@type", json!(token));
        assert_eq!(
            refusal(&doc),
            vec![near_miss(ROOT, "@type", token, "ExtensionBlock", S231_NS)],
            "{token:?}"
        );
    }
}

#[test]
fn tokens_that_name_no_s231_term_once_trimmed_are_left_to_the_readers() {
    // Inner whitespace is not a near miss, and neither is a token that names nothing in the S231
    // namespace once trimmed: a root reads no type, so these load as before.
    for token in [
        "S231: ExtensionBlock",
        "S231:",
        " S231: ",
        " ",
        "http://example.org#Block ",
    ] {
        let doc = with(base(), ROOT, "@type", json!(token));
        assert_eq!(loads(&doc), 1, "{token:?}");
    }
    // A leaf class with stray whitespace still matches no class, as before.
    let token = format!("{CONSTANT} ");
    let diags = refusal(&with(base(), CON, "@type", json!(token)));
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, DiagCode::ClassNotFound, "{diags:?}");
}

#[test]
fn every_near_miss_in_a_document_is_reported_in_a_stable_order() {
    let mut doc = with(base(), ROOT, "@type", json!("S231:ExtensionBlock "));
    doc = with(
        doc,
        "http://example.org#R.con.k",
        "@type",
        json!(" S231:Parameter"),
    );
    doc = with(
        doc,
        "http://example.org#R.con.y",
        "@type",
        json!("S231:RealOutput "),
    );
    doc = with(
        doc,
        "http://example.org#R.con.y",
        "S231:isOfDataType",
        json!({ "@id": "S231:Real\t" }),
    );
    let expected = vec![
        near_miss(
            ROOT,
            "@type",
            "S231:ExtensionBlock ",
            "ExtensionBlock",
            S231_NS,
        ),
        near_miss(
            "http://example.org#R.con.k",
            "@type",
            " S231:Parameter",
            "Parameter",
            S231_NS,
        ),
        near_miss(
            "http://example.org#R.con.y",
            "@type",
            "S231:RealOutput ",
            "RealOutput",
            S231_NS,
        ),
        near_miss(
            "http://example.org#R.con.y",
            "S231:isOfDataType",
            "S231:Real\t",
            "Real",
            S231_NS,
        ),
    ];
    let first = refusal(&doc);
    assert_eq!(first, expected);
    for _ in 0..8 {
        assert_eq!(refusal(&doc), first);
    }
}
