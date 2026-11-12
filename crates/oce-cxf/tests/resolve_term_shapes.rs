//! A §7.4.1 term (`S231:unit`, `S231:quantity`, `S231:displayUnit`) on any node that carries one,
//! a connector, a boundary or a parameter, is a string, a typed literal holding exactly `@value`
//! and `@type`, or an IRI node holding exactly `@id`, and is not empty or whitespace-only (GitHub
//! issue #602; owner rulings of 2026-11-11 and 2026-11-12). Anything else is refused as
//! `malformed-document`, subject the node, one refusal per term.
//!
//! Before #602 a connector or boundary took a blank term as a declared unit, an object carrying
//! both `@id` and `@value` as one of the two by whether `@type` was present, and any other key
//! without a word; a parameter refused the blank term but took the rest.
//!
//! Every refusal is pinned as the complete diagnostic list, stated by hand.

use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::{DiagCode, Diagnostic};
use serde_json::{Value as Json, json};

const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
const TERMS: [&str; 3] = ["unit", "quantity", "displayUnit"];

/// Every role a term has, by the node that carries it: a Real block port, an Integer block port,
/// a boundary input, a boundary output and a parameter.
const PORT: &str = "http://example.org#U.add.u1";
const INTEGER_PORT: &str = "http://example.org#U.icon.y";
const BOUNDARY_INPUT: &str = "http://example.org#U.uIn";
const BOUNDARY_OUTPUT: &str = "http://example.org#U.yOut";
const PARAMETER: &str = "http://example.org#U.con.k";
const CONNECTOR_ROLES: [&str; 4] = [PORT, INTEGER_PORT, BOUNDARY_INPUT, BOUNDARY_OUTPUT];

/// A root with a boundary input and output, a Real `Constant` and `Add`, and an Integer
/// `Constant`, carrying no term.
fn base() -> Json {
    let real = |id: &str, kind: &str, to: Option<&str>| {
        let mut node = json!({
            "@id": format!("http://example.org#U.{id}"),
            "@type": format!("S231:Real{kind}"),
            "S231:isOfDataType": { "@id": "S231:Real" }
        });
        if let Some(to) = to {
            node["S231:isConnectedTo"] = json!({ "@id": format!("http://example.org#U.{to}") });
        }
        node
    };
    json!({
        "@context": { "S231": "http://data.ashrae.org/S231#" },
        "@graph": [
            {
                "@id": "http://example.org#U",
                "@type": "S231:Block",
                "S231:containsBlock": [
                    { "@id": "http://example.org#U.con" },
                    { "@id": "http://example.org#U.add" },
                    { "@id": "http://example.org#U.icon" }
                ],
                "S231:hasInput": { "@id": BOUNDARY_INPUT },
                "S231:hasOutput": { "@id": BOUNDARY_OUTPUT }
            },
            {
                "@id": "http://example.org#U.con",
                "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
                "S231:hasParameter": { "@id": PARAMETER },
                "S231:hasOutput": { "@id": "http://example.org#U.con.y" }
            },
            { "@id": PARAMETER, "S231:value": 1.0 },
            real("con.y", "Output", Some("add.u1")),
            {
                "@id": "http://example.org#U.add",
                "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Add",
                "S231:hasInput": [
                    { "@id": PORT },
                    { "@id": "http://example.org#U.add.u2" }
                ],
                "S231:hasOutput": { "@id": "http://example.org#U.add.y" }
            },
            real("add.u1", "Input", None),
            real("add.u2", "Input", None),
            real("add.y", "Output", Some("yOut")),
            real("uIn", "Input", Some("add.u2")),
            real("yOut", "Output", None),
            {
                "@id": "http://example.org#U.icon",
                "@type": "http://example.org#Buildings.Controls.OBC.CDL.Integers.Sources.Constant",
                "S231:hasParameter": { "@id": "http://example.org#U.icon.k" },
                "S231:hasOutput": { "@id": INTEGER_PORT }
            },
            { "@id": "http://example.org#U.icon.k", "S231:value": 1 },
            {
                "@id": INTEGER_PORT,
                "@type": "S231:IntegerOutput",
                "S231:isOfDataType": { "@id": "S231:Integer" }
            }
        ]
    })
}

/// `doc` with `term` set to `value` on the node `id`.
fn with(mut doc: Json, id: &str, term: &str, value: Json) -> Json {
    let node = doc["@graph"]
        .as_array_mut()
        .expect("@graph")
        .iter_mut()
        .find(|node| node["@id"] == id)
        .unwrap_or_else(|| panic!("no node `{id}`"));
    node[format!("S231:{term}")] = value;
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

/// The refusal of `term` on `node` that is `found`.
fn refused(node: &str, term: &str, found: &str) -> Diagnostic {
    let on = if node == PARAMETER {
        " on a parameter"
    } else {
        ""
    };
    Diagnostic::error(
        DiagCode::MalformedDocument,
        format!("S231:{term}{on} is {found} — malformed term"),
    )
    .with_subject(node)
}

/// Every role that carries `term`: all five for `unit`, and the Integer port, which carries no
/// term, is still held to the shape.
fn roles() -> [&'static str; 5] {
    [
        PORT,
        INTEGER_PORT,
        BOUNDARY_INPUT,
        BOUNDARY_OUTPUT,
        PARAMETER,
    ]
}

#[test]
fn the_three_shapes_load_on_every_role() {
    assert_eq!(loads(&base()), 3);
    for term in TERMS {
        for value in [
            json!("K"),
            json!({ "@value": "K", "@type": XSD_STRING }),
            json!({ "@id": "unit:K" }),
        ] {
            // One role at a time: a unit on two connected ports would be unified.
            for node in roles() {
                assert_eq!(
                    loads(&with(base(), node, term, value.clone())),
                    3,
                    "{node} {term} {value}"
                );
            }
        }
    }
}

#[test]
fn a_blank_term_is_refused_on_every_role() {
    // Before, on a connector or boundary: a declared unit `""` (or `" "`), which the gate read as
    // unequal to any other, propagated to peers declaring none and reported to the host.
    let blank_string = "an empty or whitespace-only string";
    let blank_value = "a typed literal whose @value is empty or whitespace-only";
    let blank_id = "an IRI node whose @id is empty or whitespace-only";
    for term in TERMS {
        for (value, found) in [
            (json!(""), blank_string),
            (json!(" "), blank_string),
            (json!("\t\n"), blank_string),
            (json!("\u{a0}"), blank_string),
            (json!({ "@value": "", "@type": XSD_STRING }), blank_value),
            (json!({ "@value": " ", "@type": XSD_STRING }), blank_value),
            (json!({ "@id": "" }), blank_id),
            (json!({ "@id": "\u{3000}" }), blank_id),
        ] {
            for node in roles() {
                assert_eq!(
                    refusal(&with(base(), node, term, value.clone())),
                    vec![refused(node, term, found)],
                    "{node} {term} {value}"
                );
            }
        }
    }
}

#[test]
fn an_object_carrying_both_id_and_value_is_refused_on_every_role() {
    // Before: with `@type` the `@value` was read and the `@id` dropped; without it, the reverse.
    let both = "an object carrying both @id and @value";
    for term in TERMS {
        for value in [
            json!({ "@id": "unit:K", "@value": "degC", "@type": XSD_STRING }),
            json!({ "@id": "unit:K", "@value": "degC" }),
            json!({ "@id": "unit:K", "@value": 5 }),
            json!({ "@id": 5, "@value": "K", "@type": XSD_STRING }),
            json!({ "@id": "", "@value": "", "@type": XSD_STRING }),
        ] {
            for node in roles() {
                assert_eq!(
                    refusal(&with(base(), node, term, value.clone())),
                    vec![refused(node, term, both)],
                    "{node} {term} {value}"
                );
            }
        }
    }
}

#[test]
fn a_key_beside_the_shape_is_refused_on_every_role() {
    // Before: read as the term alone, the key dropped (and on export, every term is a string).
    let long = "q".repeat(1 << 20);
    let excerpt = format!("{}…", "q".repeat(64));
    for term in TERMS {
        for (value, found) in [
            (
                json!({ "@value": "K", "@type": XSD_STRING, "@language": "en" }),
                "a typed literal carrying the key `@language` beside @value and @type".to_owned(),
            ),
            (
                json!({ "@value": "K", "@type": XSD_STRING, "@index": "0" }),
                "a typed literal carrying the key `@index` beside @value and @type".to_owned(),
            ),
            (
                json!({ "@id": "unit:K", "qudt:symbol": "K" }),
                "an IRI node carrying the key `qudt:symbol` beside its @id".to_owned(),
            ),
            (
                json!({ "@id": "unit:K", "@type": XSD_STRING }),
                "an IRI node carrying the key `@type` beside its @id".to_owned(),
            ),
            (
                json!({ "@id": "unit:K", "a": 1, "b": 2, "c": 3, "d": 4, "e": null }),
                "an IRI node carrying the keys `a`, `b`, `c`, `d` and 1 more beside its @id"
                    .to_owned(),
            ),
            (
                json!({ "@id": "unit:K", long.clone(): true }),
                format!("an IRI node carrying the key `{excerpt}` beside its @id"),
            ),
            (
                // A blank term with a key beside it: the key is the problem reported.
                json!({ "@id": "", "rdfs:label": "K" }),
                "an IRI node carrying the key `rdfs:label` beside its @id".to_owned(),
            ),
        ] {
            for node in roles() {
                assert_eq!(
                    refusal(&with(base(), node, term, value.clone())),
                    vec![refused(node, term, &found)],
                    "{node} {term}"
                );
            }
        }
    }
}

#[test]
fn another_json_shape_is_refused_as_before_on_every_role() {
    let other = "not a string, typed literal, or IRI node";
    for term in TERMS {
        for value in [
            json!(5),
            json!(true),
            json!([]),
            json!(["K"]),
            json!({}),
            json!({ "@value": "K" }),
        ] {
            for node in roles() {
                assert_eq!(
                    refusal(&with(base(), node, term, value.clone())),
                    vec![refused(node, term, other)],
                    "{node} {term} {value}"
                );
            }
        }
    }
}

#[test]
fn the_earlier_refusals_still_come_first() {
    // A `@context` in a term object is a scoped context, refused by the expansion pass alone.
    let doc = with(
        base(),
        PORT,
        "unit",
        json!({ "@id": "unit:K", "@context": {} }),
    );
    let diags = refusal(&doc);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, DiagCode::NonSubsetConstruct);
    assert!(diags[0].message.contains("scoped `@context`"), "{diags:?}");
    // A Boolean connector permits no term at all: that refusal is the only one, whatever the
    // term's shape.
    let mut doc = base();
    let graph = doc["@graph"].as_array_mut().expect("@graph");
    let node = graph
        .iter_mut()
        .find(|node| node["@id"] == INTEGER_PORT)
        .expect("port");
    node["@type"] = json!("S231:BooleanOutput");
    node["S231:isOfDataType"] = json!({ "@id": "S231:Boolean" });
    graph
        .iter_mut()
        .find(|node| node["@id"] == "http://example.org#U.icon")
        .expect("icon")["@type"] =
        json!("http://example.org#Buildings.Controls.OBC.CDL.Logical.Sources.Constant");
    graph
        .iter_mut()
        .find(|node| node["@id"] == "http://example.org#U.icon.k")
        .expect("k")["S231:value"] = json!(true);
    assert_eq!(loads(&doc), 3, "the Boolean control loads");
    let doc = with(doc, INTEGER_PORT, "unit", json!(""));
    assert_eq!(
        refusal(&doc),
        vec![
            Diagnostic::error(
                DiagCode::MalformedDocument,
                "§7.4.1 attribute (unit/quantity/displayUnit/min/max) declared on a Boolean \
                 connector, which permits none",
            )
            .with_subject(INTEGER_PORT)
        ]
    );
}

#[test]
fn every_term_refusal_in_a_document_is_reported_in_a_stable_order() {
    let mut doc = base();
    for (index, node) in CONNECTOR_ROLES.into_iter().chain([PARAMETER]).enumerate() {
        doc = with(doc, node, TERMS[index % 3], json!(" "));
    }
    doc = with(
        doc,
        PORT,
        "quantity",
        json!({ "@id": "q:T", "@value": "T" }),
    );
    let first = refusal(&doc);
    let mut expected = vec![
        refused(PORT, "unit", "an empty or whitespace-only string"),
        refused(PORT, "quantity", "an object carrying both @id and @value"),
        refused(
            INTEGER_PORT,
            "quantity",
            "an empty or whitespace-only string",
        ),
        refused(
            BOUNDARY_INPUT,
            "displayUnit",
            "an empty or whitespace-only string",
        ),
        refused(
            BOUNDARY_OUTPUT,
            "unit",
            "an empty or whitespace-only string",
        ),
        refused(PARAMETER, "quantity", "an empty or whitespace-only string"),
    ];
    expected.sort_by(|a, b| {
        (a.subject.as_deref(), &a.message).cmp(&(b.subject.as_deref(), &b.message))
    });
    let mut sorted = first.clone();
    sorted.sort_by(|a, b| {
        (a.subject.as_deref(), &a.message).cmp(&(b.subject.as_deref(), &b.message))
    });
    assert_eq!(sorted, expected, "the same set");
    for _ in 0..8 {
        assert_eq!(refusal(&doc), first, "the same order every time");
    }
}
