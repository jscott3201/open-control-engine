//! A parameter's `S231:min`, `S231:max`, `S231:unit`, `S231:quantity` and `S231:displayUnit` are
//! held to a legal JSON-LD shape where the parameter is read, and a malformed one is refused with
//! `malformed-document` (GitHub issue #559; owner ruling of 2026-11-11). Their values are still not
//! used.
//!
//! Before the check nothing read them on a parameter, so `"S231:max": ["30", "…#double"]` (the
//! list of a typed literal's values) loaded. A bound is one number, typed literal or string (an
//! expression, the form modelica-json writes for a bound over other parameters: `"yMin"`,
//! `"100*1e-15"`); a term is a string, typed literal or IRI node.
//!
//! Every refusal is pinned as the complete diagnostic list, stated by hand; every accepted shape
//! is pinned as an outcome byte-identical to the document as written, since the model carries
//! none of the five attributes.

use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::{DiagCode, Diagnostic};
use serde_json::{Map, Value as Json, json};

const RELIEF_DAMPER: &str = include_str!("fixtures/g36/multizone_vav_relief_damper.jsonld");
const NESTED: &str = include_str!("fixtures/nested_composite.jsonld");
const MEMBERS: &str =
    include_str!("fixtures/composite_contract/accepted/mixed_member_interface.jsonld");
const S231_NS: &str = "http://data.ashrae.org/S231#";
const DOUBLE: &str = "http://www.w3.org/2001/XMLSchema#double";
const DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";
/// The root composite's own declaration carrying `S231:max` (the issue's `dpBuiSet`).
const ROOT_PARAMETER: &str = "http://example.org#g36.source.multizone_vav_relief_damper.dpBuiSet";
/// The root's second declaration, carrying `S231:min` (the issue's `k`).
const ROOT_K: &str = "http://example.org#g36.source.multizone_vav_relief_damper.k";
/// A leaf instance's parameter, read in Step 7.
const LEAF_PARAMETER: &str = "http://example.org#g36.source.multizone_vav_relief_damper.zerDam.k";
const LEAF: &str = "http://example.org#g36.source.multizone_vav_relief_damper.zerDam";
/// A nested composite's own declaration.
const NESTED_PARAMETER: &str = "http://example.org#g36.profile.nested_composite.sub.innerK";
/// A parameter declared as a classified `S231:hasInstance` member.
const MEMBER_PARAMETER: &str = "http://example.org#cc.mixed_member_interface.con.k";

const BOUND_KEYS: [&str; 2] = ["min", "max"];
const TERM_KEYS: [&str; 3] = ["unit", "quantity", "displayUnit"];

fn parse(text: &str) -> Json {
    serde_json::from_str(text).expect("fixture is JSON")
}

fn node_mut<'a>(doc: &'a mut Json, id: &str) -> &'a mut Map<String, Json> {
    doc["@graph"]
        .as_array_mut()
        .expect("@graph array")
        .iter_mut()
        .find(|entry| entry["@id"] == id)
        .and_then(Json::as_object_mut)
        .expect("node present")
}

/// `text` with `S231:<key>` set to `value` on node `id`.
fn with(text: &str, id: &str, key: &str, value: Json) -> Json {
    let mut doc = parse(text);
    node_mut(&mut doc, id).insert(format!("S231:{key}"), value);
    doc
}

/// The complete outcome, rendered: two documents agree iff these strings agree.
fn outcome(doc: &Json) -> String {
    let bytes = serde_json::to_vec(doc).expect("serialize");
    match import_cxf(&bytes, &ResolveOptions::default()) {
        Ok((graph, report)) => format!("ok {graph:?}\n{:?}", report.diagnostics),
        Err(error) => format!("err {error:?}"),
    }
}

/// Every diagnostic of a resolver refusal, in the order returned.
fn refusal(doc: &Json) -> Vec<Diagnostic> {
    let bytes = serde_json::to_vec(doc).expect("serialize");
    match import_cxf(&bytes, &ResolveOptions::default()) {
        Err(CxfError::Validation(diags)) => diags,
        other => panic!("expected a resolver refusal, got {other:?}"),
    }
}

fn bound_refused(subject: &str, which: &str, found: &str) -> Diagnostic {
    Diagnostic::error(
        DiagCode::MalformedDocument,
        format!(
            "S231:{which} on a parameter is {found}; a bound is one number, typed literal or \
             expression string"
        ),
    )
    .with_subject(subject)
}

fn term_refused(subject: &str, which: &str) -> Diagnostic {
    Diagnostic::error(
        DiagCode::MalformedDocument,
        format!(
            "S231:{which} on a parameter is not a string, typed literal, or IRI node — malformed \
             term"
        ),
    )
    .with_subject(subject)
}

/// The legal bound shapes, the vendored translations' forms among them.
fn legal_bounds() -> Vec<Json> {
    vec![
        json!(0),
        json!(-1),
        json!(30.5),
        json!({ "@value": "30", "@type": DOUBLE }),
        json!({ "@value": "0.1", "@type": DECIMAL }),
        json!("k"),
        json!("100*1e-15"),
        json!("30"),
        json!(""),
        Json::Null,
    ]
}

/// The malformed bound shapes and what each message calls it.
fn malformed_bounds() -> Vec<(Json, &'static str)> {
    vec![
        (json!(["30", DOUBLE]), "a list"),
        (json!([]), "a list"),
        (json!([30]), "a list"),
        (json!([{ "@value": "30", "@type": DOUBLE }]), "a list"),
        (json!([[1]]), "a list"),
        (json!(true), "a boolean"),
        (json!(false), "a boolean"),
    ]
}

/// The legal term shapes.
fn legal_terms() -> Vec<Json> {
    vec![
        json!("Pa"),
        json!("1"),
        json!(""),
        json!({ "@value": "Pa", "@type": "http://www.w3.org/2001/XMLSchema#string" }),
        json!({ "@id": "unit:PA" }),
        Json::Null,
    ]
}

/// The malformed term shapes.
fn malformed_terms() -> Vec<Json> {
    vec![
        json!(5),
        json!(1.5),
        json!(true),
        json!([]),
        json!(["Pa"]),
        json!({ "@value": "Pa" }),
        json!({ "@id": 5 }),
        json!({ "x": 1 }),
    ]
}

#[test]
fn the_list_of_a_typed_literals_values_is_refused_where_it_used_to_load() {
    // The issue's two nodes, the root's own declarations. Before: the document loaded.
    let mut doc = with(RELIEF_DAMPER, ROOT_PARAMETER, "max", json!(["30", DOUBLE]));
    node_mut(&mut doc, ROOT_K).insert("S231:min".into(), json!(["0", DOUBLE]));
    assert_eq!(
        refusal(&doc),
        vec![
            bound_refused(ROOT_PARAMETER, "max", "a list"),
            bound_refused(ROOT_K, "min", "a list"),
        ]
    );
}

#[test]
fn every_place_a_parameter_is_read_checks_the_shape() {
    // A root declaration, a leaf's `hasParameter`, the same node through `hasConstant`, a nested
    // composite's declaration and a classified `hasInstance` parameter member.
    let mut constant = parse(RELIEF_DAMPER);
    let leaf = node_mut(&mut constant, LEAF);
    let reference = leaf.remove("S231:hasParameter").expect("the leaf lists k");
    leaf.insert("S231:hasConstant".into(), reference);
    let mut sites = vec![
        (parse(RELIEF_DAMPER), ROOT_PARAMETER),
        (parse(RELIEF_DAMPER), LEAF_PARAMETER),
        (constant, LEAF_PARAMETER),
        (parse(NESTED), NESTED_PARAMETER),
        (parse(MEMBERS), MEMBER_PARAMETER),
    ];
    for (doc, parameter) in &mut sites {
        let written = outcome(doc);
        assert!(written.starts_with("ok "), "{parameter}: {written}");
        node_mut(doc, parameter).insert("S231:min".into(), json!([1, 2]));
        assert_eq!(
            refusal(doc),
            vec![bound_refused(parameter, "min", "a list")],
            "{parameter}"
        );
    }
}

#[test]
fn every_legal_bound_shape_loads_and_changes_nothing() {
    for (text, parameter) in [
        (RELIEF_DAMPER, ROOT_PARAMETER),
        (RELIEF_DAMPER, LEAF_PARAMETER),
    ] {
        let written = outcome(&parse(text));
        assert!(written.starts_with("ok "), "{written}");
        for key in BOUND_KEYS {
            for value in legal_bounds() {
                let doc = with(text, parameter, key, value.clone());
                assert_eq!(outcome(&doc), written, "{parameter} {key} = {value}");
            }
        }
    }
}

#[test]
fn every_malformed_bound_shape_is_refused() {
    for parameter in [ROOT_PARAMETER, LEAF_PARAMETER] {
        for key in BOUND_KEYS {
            for (value, found) in malformed_bounds() {
                let doc = with(RELIEF_DAMPER, parameter, key, value.clone());
                assert_eq!(
                    refusal(&doc),
                    vec![bound_refused(parameter, key, found)],
                    "{parameter} {key} = {value}"
                );
            }
        }
    }
}

#[test]
fn every_legal_term_shape_loads_and_changes_nothing() {
    let written = outcome(&parse(RELIEF_DAMPER));
    for parameter in [ROOT_PARAMETER, LEAF_PARAMETER] {
        for key in TERM_KEYS {
            for value in legal_terms() {
                let doc = with(RELIEF_DAMPER, parameter, key, value.clone());
                assert_eq!(outcome(&doc), written, "{parameter} {key} = {value}");
            }
        }
    }
}

#[test]
fn every_malformed_term_shape_is_refused() {
    for parameter in [ROOT_PARAMETER, LEAF_PARAMETER] {
        for key in TERM_KEYS {
            for value in malformed_terms() {
                let doc = with(RELIEF_DAMPER, parameter, key, value.clone());
                assert_eq!(
                    refusal(&doc),
                    vec![term_refused(parameter, key)],
                    "{parameter} {key} = {value}"
                );
            }
        }
    }
}

#[test]
fn several_malformed_attributes_are_each_refused_in_one_order_whatever_the_node_order() {
    let mut doc = parse(RELIEF_DAMPER);
    let node = node_mut(&mut doc, ROOT_PARAMETER);
    for (key, value) in [
        ("min", json!([0])),
        ("max", json!(true)),
        ("unit", json!(5)),
        ("quantity", json!([])),
        ("displayUnit", json!({ "@value": "Pa" })),
    ] {
        node.insert(format!("S231:{key}"), value);
    }
    let expected = vec![
        term_refused(ROOT_PARAMETER, "displayUnit"),
        bound_refused(ROOT_PARAMETER, "max", "a boolean"),
        bound_refused(ROOT_PARAMETER, "min", "a list"),
        term_refused(ROOT_PARAMETER, "quantity"),
        term_refused(ROOT_PARAMETER, "unit"),
    ];
    assert_eq!(refusal(&doc), expected);

    // The same document with its graph reversed.
    let mut reordered = doc.clone();
    reordered["@graph"]
        .as_array_mut()
        .expect("@graph array")
        .reverse();
    let first = outcome(&doc);
    for _ in 0..8 {
        assert_eq!(outcome(&reordered), first);
        assert_eq!(outcome(&doc), first);
    }
}

#[test]
fn every_key_spelling_of_a_malformed_attribute_is_refused_identically() {
    let compact = outcome(&with(RELIEF_DAMPER, ROOT_PARAMETER, "max", json!([30])));
    assert!(compact.starts_with("err Validation"), "{compact}");
    for (key, prefix) in [
        (format!("{S231_NS}max"), None),
        ("s231:max".to_owned(), Some("s231")),
    ] {
        let mut doc = parse(RELIEF_DAMPER);
        if let Some(prefix) = prefix {
            doc["@context"][prefix] = json!(S231_NS);
        }
        let node = node_mut(&mut doc, ROOT_PARAMETER);
        node.remove("S231:max");
        node.insert(key.clone(), json!([30]));
        assert_eq!(outcome(&doc), compact, "{key}");
    }
}

#[test]
fn a_pruned_parameter_is_not_read_and_not_checked() {
    let model = "http://example.org#Pruned";
    let doc = |has_feature: bool| {
        json!({
            "@context": { "S231": S231_NS },
            "@graph": [
                {
                    "@id": model,
                    "@type": "S231:Block",
                    "S231:hasParameter": { "@id": format!("{model}.hasFeature") },
                    "S231:containsBlock": [
                        { "@id": format!("{model}.con") },
                        { "@id": format!("{model}.opt") }
                    ]
                },
                {
                    "@id": format!("{model}.hasFeature"),
                    "@type": "S231:Parameter",
                    "S231:isOfDataType": { "@id": "S231:Boolean" },
                    "S231:value": has_feature
                },
                {
                    "@id": format!("{model}.con"),
                    "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
                    "S231:hasParameter": { "@id": format!("{model}.con.k") },
                    "S231:hasOutput": { "@id": format!("{model}.con.y") }
                },
                { "@id": format!("{model}.con.k"), "S231:value": 1.0 },
                {
                    "@id": format!("{model}.con.y"),
                    "@type": "S231:RealOutput",
                    "S231:isOfDataType": { "@id": "S231:Real" }
                },
                {
                    "@id": format!("{model}.opt"),
                    "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
                    "S231:isConditionalComponent": true,
                    "S231:conditionalExpression": "hasFeature",
                    "S231:hasParameter": { "@id": format!("{model}.opt.k") },
                    "S231:hasOutput": { "@id": format!("{model}.opt.y") }
                },
                { "@id": format!("{model}.opt.k"), "S231:value": 2.0, "S231:min": [1, 2] },
                {
                    "@id": format!("{model}.opt.y"),
                    "@type": "S231:RealOutput",
                    "S231:isOfDataType": { "@id": "S231:Real" }
                }
            ]
        })
    };
    assert!(outcome(&doc(false)).starts_with("ok "));
    assert_eq!(
        refusal(&doc(true)),
        vec![bound_refused(&format!("{model}.opt.k"), "min", "a list")]
    );
}
