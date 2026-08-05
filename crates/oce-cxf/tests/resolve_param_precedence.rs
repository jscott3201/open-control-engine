//! Parameter-scope precedence tests for issue #239: a leaf member's VALUE reference resolves
//! enclosing-first — when a name is bound both in the enclosing scope and by a sibling member,
//! the enclosing binding wins, so member array order can never change a grounded threshold —
//! while a DIMENSION reference (`sizeOfDimensions`) keeps resolving nearest-wins on the
//! undivided scope (owner-ruled; pinned by three tests in this file: the cross-scope
//! count-divergence refusal asserted in full with both counts, the one-name-two-readings
//! corollary fixture, and the member-order dimension-reading characterization). Split
//! corollary: one name bound in both regions has two readings inside one block — the sibling
//! drives the shape when the sibling is grounded earlier (member array order still decides the
//! dimension reading; values are order-invariant under member order, dimensions are not), the
//! enclosing binding drives the values — silent when the element counts agree, exactly like
//! the scalar path. Reals compared by IEEE-754 bits (`TESTING.md` pillar 2).

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

/// A TWO-LEVEL enclosing chain: root composite `N` binds `g = 1.0`, its nested composite `mid`
/// re-binds `g = 2.0`, and the leaf `Constant` inside `mid` binds `k = "g"`. The leaf inherits
/// the scope `[root g, mid g]` in chain order, so only a reverse scan of the enclosing region
/// reads the innermost composite's binding — a forward scan reads the root's.
fn two_level_gain_doc() -> Json {
    json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#", "base": "http://example.org#" },
        "@graph": [
            { "@id": "http://example.org#N", "@type": "S231:Block", "S231:label": "N",
              "S231:hasParameter": [ { "@id": "http://example.org#N.g" } ],
              "S231:containsBlock": [ { "@id": "http://example.org#N.mid" } ],
              "S231:hasOutput": { "@id": "http://example.org#N.yOut" } },
            { "@id": "http://example.org#N.g", "S231:label": "g", "S231:value": 1.0 },
            { "@id": "http://example.org#N.mid",
              "@type": "http://example.org#Buildings.Controls.OBC.ASHRAE.G36.Profile.MidStage",
              "S231:label": "mid",
              "S231:hasParameter": [ { "@id": "http://example.org#N.mid.g" } ],
              "S231:containsBlock": [ { "@id": "http://example.org#N.mid.con" } ],
              "S231:hasOutput": { "@id": "http://example.org#N.mid.yOut" } },
            { "@id": "http://example.org#N.mid.g", "S231:label": "g", "S231:value": 2.0 },
            { "@id": "http://example.org#N.mid.con",
              "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
              "S231:label": "con",
              "S231:hasParameter": [ { "@id": "http://example.org#N.mid.con.k" } ],
              "S231:hasOutput": { "@id": "http://example.org#N.mid.con.y" } },
            { "@id": "http://example.org#N.mid.con.k", "S231:label": "k", "S231:value": "g" },
            { "@id": "http://example.org#N.mid.con.y", "@type": "S231:RealOutput",
              "S231:isOfDataType": { "@id": "S231:Real" },
              "S231:isConnectedTo": { "@id": "http://example.org#N.mid.yOut" } },
            { "@id": "http://example.org#N.mid.yOut", "@type": "S231:RealOutput",
              "S231:isOfDataType": { "@id": "S231:Real" },
              "S231:isConnectedTo": { "@id": "http://example.org#N.yOut" } },
            { "@id": "http://example.org#N.yOut", "@type": "S231:RealOutput",
              "S231:isOfDataType": { "@id": "S231:Real" } }
        ]
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
fn innermost_enclosing_composite_wins_across_a_two_level_chain() {
    // Two enclosing bindings of `g` exist above the leaf: root 1.0, mid 2.0. The leaf's k="g"
    // must read the INNERMOST enclosing composite — the enclosing region is scanned in reverse,
    // last-pushed (innermost) first. A forward scan over the enclosing region would read the
    // root's 1.0 instead.
    let bits = real_param_bits(&import_ok(&two_level_gain_doc()));
    assert_eq!(
        bits.get("k").copied(),
        Some(2.0f64.to_bits()),
        "k = mid's g (2.0, the innermost enclosing composite), never the root's 1.0 — bit-exact"
    );
}

#[test]
fn list_element_value_references_resolve_enclosing_first() {
    // The list-element path of array expansion grounds each element on the SAME split view the
    // scalar path uses: root g=2.0; leaf binds a same-named sibling g=5.0 FIRST, then the array
    // k[2] with the JSON list value ["g", "g + 1.0"]. Each element's reference to `g` must read
    // the enclosing 2.0, never the earlier sibling 5.0 (which the undivided latest-wins view
    // would return).
    let doc = constant_doc(
        &[("g", json!({ "S231:label": "g", "S231:value": 2.0 }))],
        &[
            ("g", json!({ "S231:label": "g", "S231:value": 5.0 })),
            (
                "k",
                json!({ "S231:label": "k[2]", "S231:isArray": true,
                        "S231:numberDimensions": 1, "S231:sizeOfDimensions": "(2)",
                        "S231:value": ["g", "g + 1.0"] }),
            ),
        ],
    );
    let bits = real_param_bits(&import_ok(&doc));
    assert_eq!(
        bits.get("k_1").copied(),
        Some(2.0f64.to_bits()),
        "k_1 = enclosing g = 2.0, never the earlier sibling's 5.0 — bit-exact"
    );
    assert_eq!(
        bits.get("k_2").copied(),
        Some(3.0f64.to_bits()),
        "k_2 = enclosing g + 1.0 = 3.0, never the sibling's 6.0 — bit-exact"
    );
}

#[test]
fn g36_enum_validation_reads_the_enclosing_binding_and_refuses_a_class_mismatch() {
    // `validate_g36_parameter_value` receives the same split view value grounding uses: the root
    // binds `venStd` as a G36 FreezeStat enum value while the leaf binds a same-named sibling
    // `venStd` as VentilationStandard. The leaf member `x`, declared VentilationStandard with
    // the bare reference value "venStd", must be validated against the ENCLOSING FreezeStat
    // binding and refuse with TypeMismatch — on the undivided latest-wins view the sibling's
    // matching class would slip through silently.
    let doc = constant_doc(
        &[(
            "venStd",
            json!({ "S231:label": "venStd",
                    "S231:isOfDataType": { "@id":
                        "http://example.org#Buildings.Controls.OBC.ASHRAE.G36.Types.FreezeStat" },
                    "S231:value":
                        "Buildings.Controls.OBC.ASHRAE.G36.Types.FreezeStat.Hardwired_to_BAS" }),
        )],
        &[
            ("k", json!({ "S231:label": "k", "S231:value": 1.0 })),
            (
                "venStd",
                json!({ "S231:label": "venStd",
                        "S231:isOfDataType": { "@id":
                            "http://example.org#Buildings.Controls.OBC.ASHRAE.G36.Types.VentilationStandard" },
                        "S231:value":
                            "Buildings.Controls.OBC.ASHRAE.G36.Types.VentilationStandard.ASHRAE62_1" }),
            ),
            (
                "x",
                json!({ "S231:label": "x",
                        "S231:isOfDataType": { "@id":
                            "http://example.org#Buildings.Controls.OBC.ASHRAE.G36.Types.VentilationStandard" },
                        "S231:value": "venStd" }),
            ),
        ],
    );
    let bytes = serde_json::to_vec(&doc).expect("serialize doc");
    match import_cxf(&bytes, &ResolveOptions::default()) {
        Err(CxfError::Validation(diags)) => assert!(
            diags.iter().any(|d| {
                d.code == DiagCode::TypeMismatch
                    && d.is_error()
                    && d.message.contains("does not match declared type")
            }),
            "expected an error-severity TypeMismatch naming the declared-type mismatch, \
             got {diags:#?}"
        ),
        Ok(_) => panic!(
            "expected Validation(TypeMismatch): the enclosing FreezeStat binding must be the \
             one validated, but import succeeded"
        ),
        Err(other) => panic!("expected Validation(TypeMismatch), got {other:?}"),
    }
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
    // refuse with the exact message carrying BOTH counts. Under a dims-split mutation the dims
    // also read nin=5, the import stops refusing, and k_1..k_5 are minted — which reds this
    // refusal AND `one_name_drives_shape_from_sibling_and_values_from_enclosing` (the corollary
    // fixture) AND `member_array_order_decides_the_dimension_reading` (the member-order
    // characterization): the undivided dims view is pinned by that three-test set, not by this
    // test alone. A weaker assertion here would still guard nothing — hold the full message.
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
fn member_array_order_decides_the_dimension_reading() {
    // The documented dims-side residual, pinned as accepted behavior so any change to it is
    // loud: VALUES are order-invariant under member array order (enclosing-first), DIMENSIONS
    // are not. Root nin=5; leaf members are a same-named nin=2 and k[nin] with a one-element
    // broadcast list. With the sibling nin grounded FIRST, the undivided dims view reads
    // nin=2 → two elements; with the array FIRST, only the enclosing nin=5 is grounded when
    // the dims parse → five elements. Both orders import with zero diagnostics — member array
    // order decides the dimension reading.
    let array_member = || {
        (
            "k",
            json!({ "S231:label": "k[nin]", "S231:isArray": true,
                    "S231:numberDimensions": 1, "S231:sizeOfDimensions": "(nin)",
                    "S231:value": [7.0] }),
        )
    };
    let nin_member = || ("nin", json!({ "S231:label": "nin", "S231:value": 2 }));
    let root = [("nin", json!({ "S231:label": "nin", "S231:value": 5 }))];

    let sibling_first = import_ok(&constant_doc(&root, &[nin_member(), array_member()]));
    let keys: Vec<&str> = sibling_first.blocks[0]
        .params
        .values
        .iter()
        .map(|(n, _)| n.as_ref())
        .collect();
    assert_eq!(
        keys,
        vec!["nin", "k_1", "k_2"],
        "order [nin, k]: the already-grounded sibling nin=2 drives the shape"
    );

    let array_first = import_ok(&constant_doc(&root, &[array_member(), nin_member()]));
    let keys: Vec<&str> = array_first.blocks[0]
        .params
        .values
        .iter()
        .map(|(n, _)| n.as_ref())
        .collect();
    assert_eq!(
        keys,
        vec!["k_1", "k_2", "k_3", "k_4", "k_5", "nin"],
        "order [k, nin]: only the enclosing nin=5 is grounded at dims-parse time, so it \
         drives the shape — same document, different member order, different arity"
    );
}

#[test]
fn minted_element_names_are_shadowed_by_a_same_named_enclosing_binding() {
    // Characterization of documented current behavior: the leaf's array k[2] mints k_1/k_2 into
    // the SIBLING region of the scope, so a later member's value reference to "k_1" still reads
    // a same-named ENCLOSING binding first — w grounds to the enclosing 100.0, not the minted
    // element's 1.0 — with zero diagnostics. (A same-named SIBLING parameter, by contrast,
    // refuses via ArrayFlattenCollision.) Pinned so any change to this shadowing is loud.
    let doc = constant_doc(
        &[("k_1", json!({ "S231:label": "k_1", "S231:value": 100.0 }))],
        &[
            (
                "k",
                json!({ "S231:label": "k[2]", "S231:isArray": true,
                        "S231:numberDimensions": 1, "S231:sizeOfDimensions": "(2)",
                        "S231:value": [1.0, 2.0] }),
            ),
            ("w", json!({ "S231:label": "w", "S231:value": "k_1" })),
        ],
    );
    let bits = real_param_bits(&import_ok(&doc));
    assert_eq!(
        bits.get("k_1").copied(),
        Some(1.0f64.to_bits()),
        "the minted element itself keeps its authored value — bit-exact"
    );
    assert_eq!(
        bits.get("w").copied(),
        Some(100.0f64.to_bits()),
        "w references `k_1`: the ENCLOSING binding (100.0) wins over the minted sibling \
         element (1.0) — bit-exact"
    );
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
