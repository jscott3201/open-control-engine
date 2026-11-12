//! A §7.4.1 term (`S231:unit`, `S231:quantity`, `S231:displayUnit`) on any node that carries one,
//! a connector, a boundary or a parameter, is a string, a typed literal holding exactly `@value`
//! and `@type`, or an IRI node holding exactly `@id`, and is not empty or whitespace-only (GitHub
//! issue #602; owner rulings of 2026-11-11 and 2026-11-12). Anything else is refused as
//! `malformed-document`, subject the node, one refusal per term.
//!
//! Before #602 a connector or boundary took a blank term as a declared unit, an object carrying
//! both `@id` and `@value` as one of the two by whether `@type` was present, and any other key
//! without a word; a parameter refused the blank term but took the rest. An Integer connector
//! permits no term at all (CDL §7.4.1.2 gives it only `min` and `max`): one is refused whatever
//! its shape, as on a Boolean connector (owner ruling of 2026-11-12), where before a well-shaped
//! one was dropped without a word.
//!
//! Every refusal is pinned as the complete diagnostic list, stated by hand.

use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::{DiagCode, Diagnostic};
use serde_json::{Value as Json, json};

const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
const TERMS: [&str; 3] = ["unit", "quantity", "displayUnit"];

/// Every role a term has, by the node that carries it: a Real block port, a boundary input, a
/// boundary output and a parameter.
const PORT: &str = "http://example.org#U.add.u1";
const BOUNDARY_INPUT: &str = "http://example.org#U.uIn";
const BOUNDARY_OUTPUT: &str = "http://example.org#U.yOut";
const PARAMETER: &str = "http://example.org#U.con.k";

/// The connectors that permit no term: an Integer output and the Integer input it drives, and a
/// Boolean output.
const INTEGER_OUTPUT: &str = "http://example.org#U.icon.y";
const INTEGER_INPUT: &str = "http://example.org#U.i2r.u";
const BOOLEAN_OUTPUT: &str = "http://example.org#U.bcon.y";

/// The blocks of [`base`].
const BLOCKS: usize = 5;

/// A root with a boundary input and output, a Real `Constant` and `Add`, an Integer `Constant`
/// driving an `IntegerToReal`, and a Boolean `Constant`, carrying no term.
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
                    { "@id": "http://example.org#U.icon" },
                    { "@id": "http://example.org#U.i2r" },
                    { "@id": "http://example.org#U.bcon" }
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
                "S231:hasOutput": { "@id": INTEGER_OUTPUT }
            },
            { "@id": "http://example.org#U.icon.k", "S231:value": 1 },
            {
                "@id": INTEGER_OUTPUT,
                "@type": "S231:IntegerOutput",
                "S231:isOfDataType": { "@id": "S231:Integer" },
                "S231:isConnectedTo": { "@id": INTEGER_INPUT }
            },
            {
                "@id": "http://example.org#U.i2r",
                "@type": "http://example.org#Buildings.Controls.OBC.CDL.Conversions.IntegerToReal",
                "S231:hasInput": { "@id": INTEGER_INPUT },
                "S231:hasOutput": { "@id": "http://example.org#U.i2r.y" }
            },
            {
                "@id": INTEGER_INPUT,
                "@type": "S231:IntegerInput",
                "S231:isOfDataType": { "@id": "S231:Integer" }
            },
            real("i2r.y", "Output", None),
            {
                "@id": "http://example.org#U.bcon",
                "@type": "http://example.org#Buildings.Controls.OBC.CDL.Logical.Sources.Constant",
                "S231:hasParameter": { "@id": "http://example.org#U.bcon.k" },
                "S231:hasOutput": { "@id": BOOLEAN_OUTPUT }
            },
            { "@id": "http://example.org#U.bcon.k", "S231:value": true },
            {
                "@id": BOOLEAN_OUTPUT,
                "@type": "S231:BooleanOutput",
                "S231:isOfDataType": { "@id": "S231:Boolean" }
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

/// Every role that carries a term and is held to its shape.
fn roles() -> [&'static str; 4] {
    [PORT, BOUNDARY_INPUT, BOUNDARY_OUTPUT, PARAMETER]
}

/// The refusal of a term on an Integer connector.
fn on_integer(node: &str) -> Diagnostic {
    Diagnostic::error(
        DiagCode::MalformedDocument,
        "§7.4.1 term (unit/quantity/displayUnit) declared on an Integer connector, which permits \
         only min and max",
    )
    .with_subject(node)
}

#[test]
fn the_three_shapes_load_on_every_role() {
    assert_eq!(loads(&base()), BLOCKS);
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
                    BLOCKS,
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
    for value in [json!("K"), json!("")] {
        assert_eq!(
            refusal(&with(base(), BOOLEAN_OUTPUT, "unit", value.clone())),
            vec![
                Diagnostic::error(
                    DiagCode::MalformedDocument,
                    "§7.4.1 attribute (unit/quantity/displayUnit/min/max) declared on a Boolean \
                     connector, which permits none",
                )
                .with_subject(BOOLEAN_OUTPUT)
            ],
            "{value}"
        );
    }
}

#[test]
fn a_term_on_an_integer_connector_is_refused_whatever_its_shape() {
    // CDL §7.4.1.2 gives an Integer only `min` and `max`. Before: a well-shaped term loaded and was
    // dropped without a word; this PR's first head refused only a malformed one.
    for node in [INTEGER_OUTPUT, INTEGER_INPUT] {
        for term in TERMS {
            for value in [
                json!("1"),
                json!({ "@value": "1", "@type": XSD_STRING }),
                json!({ "@id": "unit:UNITLESS" }),
                json!(""),
                json!({ "@id": "unit:K", "@value": "K" }),
                json!(5),
            ] {
                assert_eq!(
                    refusal(&with(base(), node, term, value.clone())),
                    vec![on_integer(node)],
                    "{node} {term} {value}"
                );
            }
        }
        // All three terms on one connector are one refusal; its bounds stay legal.
        let mut doc = base();
        for term in TERMS {
            doc = with(doc, node, term, json!("1"));
        }
        doc = with(doc, node, "min", json!(0));
        assert_eq!(refusal(&doc), vec![on_integer(node)], "{node}");
        assert_eq!(
            loads(&with(base(), node, "max", json!(10))),
            BLOCKS,
            "{node}"
        );
    }
}

#[test]
fn every_term_refusal_in_a_document_is_reported_in_a_stable_order() {
    let mut doc = base();
    for (node, term) in [
        (PORT, "unit"),
        (BOUNDARY_INPUT, "displayUnit"),
        (BOUNDARY_OUTPUT, "unit"),
        (PARAMETER, "quantity"),
        (INTEGER_OUTPUT, "quantity"),
    ] {
        doc = with(doc, node, term, json!(" "));
    }
    doc = with(
        doc,
        PORT,
        "quantity",
        json!({ "@id": "q:T", "@value": "T" }),
    );
    let first = refusal(&doc);
    let blank = "an empty or whitespace-only string";
    let mut expected = vec![
        refused(PORT, "unit", blank),
        refused(PORT, "quantity", "an object carrying both @id and @value"),
        refused(BOUNDARY_INPUT, "displayUnit", blank),
        refused(BOUNDARY_OUTPUT, "unit", blank),
        refused(PARAMETER, "quantity", blank),
        on_integer(INTEGER_OUTPUT),
    ];
    let by_subject = |a: &Diagnostic, b: &Diagnostic| {
        (a.subject.as_deref(), &a.message).cmp(&(b.subject.as_deref(), &b.message))
    };
    expected.sort_by(by_subject);
    let mut sorted = first.clone();
    sorted.sort_by(by_subject);
    assert_eq!(sorted, expected, "the same set");
    for _ in 0..8 {
        assert_eq!(refusal(&doc), first, "the same order every time");
    }
}
