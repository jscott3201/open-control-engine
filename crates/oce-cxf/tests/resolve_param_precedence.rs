//! Parameter-scope precedence tests for issue #239: a leaf member's VALUE reference resolves
//! enclosing-first — when a name is bound both in the enclosing scope and by a sibling member,
//! the enclosing binding wins, so member array order can never change a grounded threshold —
//! while a DIMENSION reference (`sizeOfDimensions`) keeps resolving nearest-wins on the
//! undivided scope (owner-ruled; its sole pin is the cross-scope count-divergence refusal
//! asserted here in full, both counts included). Split corollary: one name bound in both
//! regions has two readings inside one block — the sibling drives the shape, the enclosing
//! binding drives the values — silent when the element counts agree, exactly like the scalar
//! path. Reals compared by IEEE-754 bits (`TESTING.md` pillar 2).

use std::collections::BTreeMap;

use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::DiagCode;
use oce_model::{ModelGraph, Value};
use serde_json::{Value as Json, json};

// ---- import helpers -------------------------------------------------------------------------

/// Import expecting success and zero diagnostics; returns the graph.
#[track_caller]
fn import_ok(doc: &Json) -> ModelGraph {
    let bytes = serde_json::to_vec(doc).expect("serialize doc");
    let (g, report) =
        import_cxf(&bytes, &ResolveOptions::default()).expect("fixture must resolve without error");
    assert!(
        report.is_empty(),
        "expected zero diagnostics, got: {:?}",
        report.diagnostics
    );
    g
}

/// The one leaf block's Real parameters as a name → `f64::to_bits` map — order-independent by
/// construction, so two member array orders can be compared for value identity bit-exactly.
fn real_param_bits(g: &ModelGraph) -> BTreeMap<String, u64> {
    g.blocks[0]
        .params
        .values
        .iter()
        .filter_map(|(name, v)| match v {
            Value::Real(r) => Some((name.to_string(), r.to_bits())),
            _ => None,
        })
        .collect()
}

// ---- document builders ----------------------------------------------------------------------

/// The issue #239 counter-example: a composite `Z` with root parameter `uLow = 0.01` and one
/// `Hysteresis` child whose members bind `uLow = "-uLow"` and `uHigh = "uLow"` against it.
/// `member_order` lists the child's `hasParameter` references in authored array order — the only
/// thing the two variants differ in.
fn hysteresis_doc(member_order: [&str; 2]) -> Json {
    let refs: Vec<Json> = member_order
        .iter()
        .map(|name| json!({ "@id": format!("http://example.org#Z.hys.{name}") }))
        .collect();
    json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#", "base": "http://example.org#" },
        "@graph": [
            { "@id": "http://example.org#Z", "@type": "S231:Block", "S231:label": "Z",
              "S231:hasParameter": [ { "@id": "http://example.org#Z.uLow" } ],
              "S231:containsBlock": [ { "@id": "http://example.org#Z.hys" } ],
              "S231:hasInput": { "@id": "http://example.org#Z.uIn" },
              "S231:hasOutput": { "@id": "http://example.org#Z.yOut" } },
            { "@id": "http://example.org#Z.uLow", "S231:label": "uLow", "S231:value": 0.01 },
            { "@id": "http://example.org#Z.hys",
              "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Hysteresis",
              "S231:label": "hys",
              "S231:hasParameter": refs,
              "S231:hasInput": { "@id": "http://example.org#Z.hys.u" },
              "S231:hasOutput": { "@id": "http://example.org#Z.hys.y" } },
            { "@id": "http://example.org#Z.hys.uLow", "S231:label": "uLow",
              "S231:value": "-uLow" },
            { "@id": "http://example.org#Z.hys.uHigh", "S231:label": "uHigh",
              "S231:value": "uLow" },
            { "@id": "http://example.org#Z.hys.u", "@type": "S231:RealInput",
              "S231:isOfDataType": { "@id": "S231:Real" } },
            { "@id": "http://example.org#Z.hys.y", "@type": "S231:BooleanOutput",
              "S231:isOfDataType": { "@id": "S231:Boolean" },
              "S231:isConnectedTo": { "@id": "http://example.org#Z.yOut" } },
            { "@id": "http://example.org#Z.uIn", "@type": "S231:RealInput",
              "S231:isOfDataType": { "@id": "S231:Real" },
              "S231:isConnectedTo": { "@id": "http://example.org#Z.hys.u" } },
            { "@id": "http://example.org#Z.yOut", "@type": "S231:BooleanOutput",
              "S231:isOfDataType": { "@id": "S231:Boolean" } }
        ]
    })
}

/// A composite `A` with the given root parameter nodes and one `Constant` child carrying the
/// given member parameter nodes — the `array_expression_preserved.jsonld` skeleton plus an
/// enclosing scope. `root_params` and `members` are (local name, parameter-node body) pairs;
/// reference arrays follow the pair order.
fn constant_doc(root_params: &[(&str, Json)], members: &[(&str, Json)]) -> Json {
    let root_refs: Vec<Json> = root_params
        .iter()
        .map(|(name, _)| json!({ "@id": format!("http://example.org#A.{name}") }))
        .collect();
    let member_refs: Vec<Json> = members
        .iter()
        .map(|(name, _)| json!({ "@id": format!("http://example.org#A.con.{name}") }))
        .collect();
    let mut graph = vec![
        json!({ "@id": "http://example.org#A", "@type": "S231:Block", "S231:label": "A",
                "S231:hasParameter": root_refs,
                "S231:containsBlock": [ { "@id": "http://example.org#A.con" } ],
                "S231:hasOutput": { "@id": "http://example.org#A.yOut" } }),
        json!({ "@id": "http://example.org#A.con",
                "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
                "S231:label": "con",
                "S231:hasParameter": member_refs,
                "S231:hasOutput": { "@id": "http://example.org#A.con.y" } }),
    ];
    for (name, body) in root_params {
        let mut node = body.clone();
        node["@id"] = json!(format!("http://example.org#A.{name}"));
        graph.push(node);
    }
    for (name, body) in members {
        let mut node = body.clone();
        node["@id"] = json!(format!("http://example.org#A.con.{name}"));
        graph.push(node);
    }
    graph.push(
        json!({ "@id": "http://example.org#A.con.y", "@type": "S231:RealOutput",
                       "S231:isOfDataType": { "@id": "S231:Real" },
                       "S231:isConnectedTo": { "@id": "http://example.org#A.yOut" } }),
    );
    graph.push(
        json!({ "@id": "http://example.org#A.yOut", "@type": "S231:RealOutput",
                       "S231:isOfDataType": { "@id": "S231:Real" } }),
    );
    json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#", "base": "http://example.org#" },
        "@graph": graph
    })
}

// ---- enclosing-wins value precedence --------------------------------------------------------

#[test]
fn hysteresis_thresholds_are_invariant_under_member_array_order() {
    // Issue #239's counter-example: composite uLow=0.01; child members uLow="-uLow",
    // uHigh="uLow". Under the retired latest-wins scan the two member orders diverged
    // (uHigh grounded to -0.01 or +0.01 depending on array position); under enclosing-wins both
    // orders ground to the identical name → bits map, with zero diagnostics.
    let low_first = real_param_bits(&import_ok(&hysteresis_doc(["uLow", "uHigh"])));
    let high_first = real_param_bits(&import_ok(&hysteresis_doc(["uHigh", "uLow"])));
    assert_eq!(
        low_first, high_first,
        "member array order must not change any grounded parameter value"
    );
    assert_eq!(
        low_first.get("uLow").copied(),
        Some((-0.01f64).to_bits()),
        "uLow = -(enclosing 0.01), bit-exact"
    );
    assert_eq!(
        low_first.get("uHigh").copied(),
        Some(0.01f64.to_bits()),
        "uHigh = enclosing uLow = +0.01, bit-exact — never the sibling's -0.01"
    );
}

#[test]
fn enclosing_binding_beats_same_named_sibling_for_value_references() {
    // Root g=2.0; child members bind a same-named g=5.0 FIRST, then k="g". The value reference
    // must read the enclosing 2.0, not the earlier sibling 5.0.
    let doc = constant_doc(
        &[("g", json!({ "S231:label": "g", "S231:value": 2.0 }))],
        &[
            ("g", json!({ "S231:label": "g", "S231:value": 5.0 })),
            ("k", json!({ "S231:label": "k", "S231:value": "g" })),
        ],
    );
    let bits = real_param_bits(&import_ok(&doc));
    assert_eq!(
        bits.get("k").copied(),
        Some(2.0f64.to_bits()),
        "k references `g`: the enclosing g=2.0 wins over the earlier sibling g=5.0"
    );
    assert_eq!(
        bits.get("g").copied(),
        Some(5.0f64.to_bits()),
        "the sibling member g still grounds to its own authored value"
    );
}

#[test]
fn sibling_only_name_still_resolves_against_earlier_sibling() {
    // The ratified half of the rule: a name bound ONLY by an earlier sibling keeps resolving
    // against it even when an enclosing scope exists (here binding an unrelated name).
    let doc = constant_doc(
        &[(
            "offset",
            json!({ "S231:label": "offset", "S231:value": 10.0 }),
        )],
        &[
            ("base", json!({ "S231:label": "base", "S231:value": 1.5 })),
            (
                "k",
                json!({ "S231:label": "k", "S231:value": "base + offset" }),
            ),
        ],
    );
    let bits = real_param_bits(&import_ok(&doc));
    assert_eq!(
        bits.get("k").copied(),
        Some(11.5f64.to_bits()),
        "k = sibling base 1.5 + enclosing offset 10.0, bit-exact"
    );
}

// ---- the undivided dimension view and its refusal pin ---------------------------------------

/// Root `nin = 5` over a leaf declaring its own `nin = 2` and `k[nin]` with the given value —
/// the one-name-two-readings fixture (shape from the sibling, values from the enclosing scope).
fn cross_scope_array_doc(k_value: &str) -> Json {
    constant_doc(
        &[("nin", json!({ "S231:label": "nin", "S231:value": 5 }))],
        &[
            ("nin", json!({ "S231:label": "nin", "S231:value": 2 })),
            (
                "k",
                json!({ "S231:label": "k[nin]", "S231:isArray": true,
                        "S231:numberDimensions": 1, "S231:sizeOfDimensions": "(nin)",
                        "S231:value": k_value }),
            ),
        ],
    )
}

#[test]
fn cross_scope_dimension_count_divergence_refuses_with_both_counts() {
    // Rulings 2+3: dimensions resolve on the undivided view (sibling nin=2 → shape 2) while the
    // value expression resolves enclosing-first (root nin=5 → 5 elements). The divergence must
    // refuse with the exact message carrying BOTH counts. This test is the sole pin of the
    // undivided dims view: under a dims-split mutation the dims also read nin=5, the import
    // stops refusing, and k_1..k_5 are minted — so a weaker assertion here guards nothing.
    let bytes = serde_json::to_vec(&cross_scope_array_doc("fill(1, nin)")).expect("serialize doc");
    match import_cxf(&bytes, &ResolveOptions::default()) {
        Err(CxfError::Validation(diags)) => assert!(
            diags.iter().any(|d| {
                d.code == DiagCode::GroundingFailed
                    && d.is_error()
                    && d.message
                        == "array expression evaluated to 5 element(s) but the declared \
                            dimensions imply 2"
            }),
            "expected the exact count-divergence refusal, got {diags:#?}"
        ),
        Ok(_) => panic!("expected Validation(GroundingFailed), but import succeeded"),
        Err(other) => panic!("expected Validation(GroundingFailed), got {other:?}"),
    }
}

#[test]
fn one_name_drives_shape_from_sibling_and_values_from_enclosing() {
    // Ruling 5's corollary in one fixture: root nin=5, leaf nin=2, k dims "(nin)", value
    // "fill(nin, 2)". The sibling nin drives the SHAPE (2 elements, undivided dims view); the
    // enclosing nin drives the VALUES (Integer 5, split value view). The element counts agree,
    // so — exactly like the scalar path — the value divergence is silent: zero diagnostics.
    let g = import_ok(&cross_scope_array_doc("fill(nin, 2)"));
    let vals = &g.blocks[0].params.values;
    let keys: Vec<&str> = vals.iter().map(|(n, _)| n.as_ref()).collect();
    assert_eq!(
        keys,
        vec!["nin", "k_1", "k_2"],
        "shape 2 comes from the sibling nin"
    );
    assert!(vals[0].1.bit_eq(&Value::Integer(2)), "leaf's own nin == 2");
    assert!(
        vals[1].1.bit_eq(&Value::Integer(5)),
        "k_1 value comes from the enclosing nin == 5"
    );
    assert!(
        vals[2].1.bit_eq(&Value::Integer(5)),
        "k_2 value comes from the enclosing nin == 5"
    );
}
