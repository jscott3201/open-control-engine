//! Scope-construction pins for the shared own-declaration mechanism: the vendored
//! `SupplyTemperature` forward-reference chain, parsed dependency discovery, own-name masking,
//! duplicate refusal, cycle refusal with maximal progress, and the specialize invocation's
//! withheld emission. Integration-level behavior (whole-document imports, permutations, probe
//! graduations) lives in `tests/resolve_declaration_scope.rs`.

use std::collections::HashMap;
use std::sync::Arc;

use oce_diag::{DiagCode, Diagnostic};
use oce_expr::EvalResult;
use oce_model::Value;

use crate::dto::Node;

use super::declaration_scope::{Pass, evaluate_declarations, identifier_heads};
use super::specialize::Specialization;

fn node(json: serde_json::Value) -> Node {
    serde_json::from_value(json).expect("test node parses")
}

fn by_id(nodes: &[Node]) -> HashMap<&str, &Node> {
    nodes.iter().map(|n| (n.id.as_str(), n)).collect()
}

fn scalar<'a>(entries: &'a [(Arc<str>, EvalResult)], name: &str) -> Option<&'a Value> {
    entries.iter().find_map(|(entry, result)| {
        (entry.as_ref() == name).then_some(match result {
            EvalResult::Scalar(value) => value,
            other => panic!("entry {name} is not scalar: {other:?}"),
        })
    })
}

/// The vendored `SupplyTemperature` root's `hasParameter` node list, lifted VERBATIM from
/// `third_party/modelica-buildings-cdl/cxf/Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/
/// VAV/SetPoints/SupplyTemperature.jsonld` (pin `85721b82`) in the document's emitted
/// (case-insensitive alphabetical) order. `iniSet`/`maxSet`/`minSet` reference
/// `TSupCoo_max`/`TSupCoo_min`, which the serializer alphabetizes AFTER them — the forward
/// references issue #240 legalizes.
const SUPPLY_TEMPERATURE_PARAMETERS: &str = r#"[
  {"@id": "ex:Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints.SupplyTemperature.delTim", "@type": "S231:Parameter", "S231:accessSpecifier": "public", "S231:description": "Delay timer", "S231:isOfDataType": {"@id": "S231:Real"}, "S231:label": "delTim", "S231:value": 600, "qudt:hasQuantityKind": {"@id": "q:Time"}, "qudt:hasUnit": {"@id": "unit:SEC"}},
  {"@id": "ex:Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints.SupplyTemperature.iniSet", "@type": "S231:Parameter", "S231:accessSpecifier": "protected", "S231:description": "Initial setpoint", "S231:hasDisplayUnit": {"@id": "unit:DEG_C"}, "S231:isOfDataType": {"@id": "S231:Real"}, "S231:label": "iniSet", "S231:value": "TSupCoo_max", "qudt:hasQuantityKind": {"@id": "q:ThermodynamicTemperature"}, "qudt:hasUnit": {"@id": "unit:K"}},
  {"@id": "ex:Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints.SupplyTemperature.maxRes", "@type": "S231:Parameter", "S231:accessSpecifier": "public", "S231:description": "Maximum response per time interval", "S231:hasDisplayUnit": {"@id": "unit:K"}, "S231:isOfDataType": {"@id": "S231:Real"}, "S231:label": "maxRes", "S231:value": {"@value": "-0.6", "@type": "http://www.w3.org/2001/XMLSchema#decimal"}, "qudt:hasQuantityKind": {"@id": "q:TemperatureDifference"}, "qudt:hasUnit": {"@id": "unit:K"}},
  {"@id": "ex:Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints.SupplyTemperature.maxSet", "@type": "S231:Parameter", "S231:accessSpecifier": "protected", "S231:description": "Maximum setpoint", "S231:hasDisplayUnit": {"@id": "unit:DEG_C"}, "S231:isOfDataType": {"@id": "S231:Real"}, "S231:label": "maxSet", "S231:value": "TSupCoo_max", "qudt:hasQuantityKind": {"@id": "q:ThermodynamicTemperature"}, "qudt:hasUnit": {"@id": "unit:K"}},
  {"@id": "ex:Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints.SupplyTemperature.minSet", "@type": "S231:Parameter", "S231:accessSpecifier": "protected", "S231:description": "Minimum setpoint", "S231:hasDisplayUnit": {"@id": "unit:DEG_C"}, "S231:isOfDataType": {"@id": "S231:Real"}, "S231:label": "minSet", "S231:value": "TSupCoo_min", "qudt:hasQuantityKind": {"@id": "q:ThermodynamicTemperature"}, "qudt:hasUnit": {"@id": "unit:K"}},
  {"@id": "ex:Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints.SupplyTemperature.numIgnReq", "@type": "S231:Parameter", "S231:accessSpecifier": "public", "S231:description": "Number of ignorable requests for TrimResponse logic", "S231:isOfDataType": {"@id": "S231:Integer"}, "S231:label": "numIgnReq", "S231:value": 2},
  {"@id": "ex:Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints.SupplyTemperature.resAmo", "@type": "S231:Parameter", "S231:accessSpecifier": "public", "S231:description": "Response amount", "S231:hasDisplayUnit": {"@id": "unit:K"}, "S231:isOfDataType": {"@id": "S231:Real"}, "S231:label": "resAmo", "S231:value": {"@value": "-0.2", "@type": "http://www.w3.org/2001/XMLSchema#decimal"}, "qudt:hasQuantityKind": {"@id": "q:TemperatureDifference"}, "qudt:hasUnit": {"@id": "unit:K"}},
  {"@id": "ex:Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints.SupplyTemperature.samplePeriod", "@type": "S231:Parameter", "S231:accessSpecifier": "public", "S231:description": "Sample period of component", "S231:isOfDataType": {"@id": "S231:Real"}, "S231:label": "samplePeriod", "S231:min": {"@value": "0.001", "@type": "http://www.w3.org/2001/XMLSchema#decimal"}, "S231:value": 120, "qudt:hasQuantityKind": {"@id": "q:Time"}, "qudt:hasUnit": {"@id": "unit:SEC"}},
  {"@id": "ex:Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints.SupplyTemperature.TDeaBan", "@type": "S231:Parameter", "S231:accessSpecifier": "protected", "S231:description": "Default supply temperature setpoint when the AHU is disabled", "S231:hasDisplayUnit": {"@id": "unit:DEG_C"}, "S231:isOfDataType": {"@id": "S231:Real"}, "S231:label": "TDeaBan", "S231:value": "273.15 +26", "qudt:hasQuantityKind": {"@id": "q:ThermodynamicTemperature"}, "qudt:hasUnit": {"@id": "unit:K"}},
  {"@id": "ex:Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints.SupplyTemperature.TOut_max", "@type": "S231:Parameter", "S231:accessSpecifier": "public", "S231:description": "Higher value of the outdoor air temperature reset range. Typically value is 21 degC (70 degF)", "S231:hasDisplayUnit": {"@id": "unit:DEG_C"}, "S231:isOfDataType": {"@id": "S231:Real"}, "S231:label": "TOut_max", "S231:value": {"@value": "294.15", "@type": "http://www.w3.org/2001/XMLSchema#decimal"}, "qudt:hasQuantityKind": {"@id": "q:ThermodynamicTemperature"}, "qudt:hasUnit": {"@id": "unit:K"}},
  {"@id": "ex:Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints.SupplyTemperature.TOut_min", "@type": "S231:Parameter", "S231:accessSpecifier": "public", "S231:description": "Lower value of the outdoor air temperature reset range. Typically value is 16 degC (60 degF)", "S231:hasDisplayUnit": {"@id": "unit:DEG_C"}, "S231:isOfDataType": {"@id": "S231:Real"}, "S231:label": "TOut_min", "S231:value": {"@value": "289.15", "@type": "http://www.w3.org/2001/XMLSchema#decimal"}, "qudt:hasQuantityKind": {"@id": "q:ThermodynamicTemperature"}, "qudt:hasUnit": {"@id": "unit:K"}},
  {"@id": "ex:Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints.SupplyTemperature.triAmo", "@type": "S231:Parameter", "S231:accessSpecifier": "public", "S231:description": "Trim amount", "S231:hasDisplayUnit": {"@id": "unit:K"}, "S231:isOfDataType": {"@id": "S231:Real"}, "S231:label": "triAmo", "S231:value": {"@value": "0.1", "@type": "http://www.w3.org/2001/XMLSchema#decimal"}, "qudt:hasQuantityKind": {"@id": "q:TemperatureDifference"}, "qudt:hasUnit": {"@id": "unit:K"}},
  {"@id": "ex:Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints.SupplyTemperature.TSupCoo_max", "@type": "S231:Parameter", "S231:accessSpecifier": "public", "S231:description": "Highest cooling supply air temperature setpoint. It is typically 18 degC (65 degF) \n    in mild and dry climates, 16 degC (60 degF) or lower in humid climates", "S231:hasDisplayUnit": {"@id": "unit:DEG_C"}, "S231:isOfDataType": {"@id": "S231:Real"}, "S231:label": "TSupCoo_max", "S231:value": {"@value": "291.15", "@type": "http://www.w3.org/2001/XMLSchema#decimal"}, "qudt:hasQuantityKind": {"@id": "q:ThermodynamicTemperature"}, "qudt:hasUnit": {"@id": "unit:K"}},
  {"@id": "ex:Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints.SupplyTemperature.TSupCoo_min", "@type": "S231:Parameter", "S231:accessSpecifier": "public", "S231:description": "Lowest cooling supply air temperature setpoint when the outdoor air temperature is at the\n    higher value of the reset range and above", "S231:hasDisplayUnit": {"@id": "unit:DEG_C"}, "S231:isOfDataType": {"@id": "S231:Real"}, "S231:label": "TSupCoo_min", "S231:value": {"@value": "285.15", "@type": "http://www.w3.org/2001/XMLSchema#decimal"}, "qudt:hasQuantityKind": {"@id": "q:ThermodynamicTemperature"}, "qudt:hasUnit": {"@id": "unit:K"}},
  {"@id": "ex:Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints.SupplyTemperature.TSupWarUpSetBac", "@type": "S231:Parameter", "S231:accessSpecifier": "public", "S231:description": "Supply temperature in warm up and set back mode", "S231:hasDisplayUnit": {"@id": "unit:DEG_C"}, "S231:isOfDataType": {"@id": "S231:Real"}, "S231:label": "TSupWarUpSetBac", "S231:value": {"@value": "308.15", "@type": "http://www.w3.org/2001/XMLSchema#decimal"}, "qudt:hasQuantityKind": {"@id": "q:ThermodynamicTemperature"}, "qudt:hasUnit": {"@id": "unit:K"}}
]"#;

/// Evaluate a root node declaring `param_ids` (in that order) over `nodes` at the lowering
/// invocation with no enclosing entries, returning the produced entries and diagnostics.
fn evaluate_root(
    nodes: &[Node],
    param_ids: &[&str],
) -> (Vec<(Arc<str>, EvalResult)>, Vec<Diagnostic>) {
    let root = node(serde_json::json!({
        "@id": "ex:Root",
        "S231:hasParameter": param_ids
            .iter()
            .map(|id| serde_json::json!({ "@id": id }))
            .collect::<Vec<_>>(),
    }));
    let map = by_id(nodes);
    let mut diags = Vec::new();
    let evaluation = evaluate_declarations(
        &root,
        &[],
        Vec::new(),
        &map,
        Pass::Lowering {
            specialization: &Specialization::default(),
            diags: &mut diags,
        },
    );
    assert!(
        evaluation.withheld.is_empty(),
        "the lowering invocation emits directly and withholds nothing"
    );
    (evaluation.entries, diags)
}

#[test]
fn supply_temperature_setpoints_ground_against_later_declared_bounds_in_both_orders() {
    let nodes: Vec<Node> =
        serde_json::from_str(SUPPLY_TEMPERATURE_PARAMETERS).expect("verbatim vendored nodes");
    let emitted: Vec<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
    let mut reversed = emitted.clone();
    reversed.reverse();

    for order in [&emitted, &reversed] {
        let (entries, diags) = evaluate_root(&nodes, order);
        assert!(
            diags.is_empty(),
            "the vendored chain grounds clean: {diags:?}"
        );
        assert_eq!(entries.len(), 15, "every declaration grounds");
        for (name, expected) in [
            ("iniSet", 291.15),
            ("maxSet", 291.15),
            ("minSet", 285.15),
            ("TSupCoo_max", 291.15),
            ("TSupCoo_min", 285.15),
        ] {
            let got = scalar(&entries, name).unwrap_or_else(|| panic!("{name} must ground"));
            assert!(
                got.bit_eq(&Value::Real(expected)),
                "{name} must ground to {expected} independent of order, got {got:?}"
            );
        }
    }
}

#[test]
fn produced_entries_lie_in_chained_declaration_order_not_evaluation_order() {
    // kDerived evaluates AFTER kBase (dependency) but is declared first; the scope region must
    // keep chained declaration order.
    let nodes = vec![
        node(serde_json::json!({ "@id": "ex:M.kDerived", "S231:value": "kBase + 1.0" })),
        node(serde_json::json!({ "@id": "ex:M.kBase", "S231:value": 2.0 })),
    ];
    let (entries, diags) = evaluate_root(&nodes, &["ex:M.kDerived", "ex:M.kBase"]);
    assert!(diags.is_empty(), "{diags:?}");
    let names: Vec<&str> = entries.iter().map(|(name, _)| name.as_ref()).collect();
    assert_eq!(names, ["kDerived", "kBase"]);
    assert!(
        scalar(&entries, "kDerived")
            .unwrap()
            .bit_eq(&Value::Real(3.0))
    );
}

#[test]
fn own_name_masks_enclosing_binding_even_when_the_own_declaration_cannot_ground() {
    // Enclosing k=1.0; the own chain declares k with NO value plus g="k". g must NOT fall
    // through to the enclosing k — the own binding masks it, so g fails to ground.
    let nodes = vec![
        node(serde_json::json!({ "@id": "ex:M.g", "S231:value": "k" })),
        node(serde_json::json!({ "@id": "ex:M.k" })),
    ];
    let root = node(serde_json::json!({
        "@id": "ex:M",
        "S231:hasParameter": [ { "@id": "ex:M.g" }, { "@id": "ex:M.k" } ],
    }));
    let map = by_id(&nodes);
    let enclosing = vec![(Arc::<str>::from("k"), EvalResult::Scalar(Value::Real(1.0)))];
    let mut diags = Vec::new();
    let evaluation = evaluate_declarations(
        &root,
        &[],
        enclosing,
        &map,
        Pass::Lowering {
            specialization: &Specialization::default(),
            diags: &mut diags,
        },
    );
    assert!(
        !evaluation
            .entries
            .iter()
            .any(|(name, _)| name.as_ref() == "g"),
        "g must not ground through the masked enclosing k"
    );
    assert_eq!(diags.len(), 2, "no-value k plus unresolved g: {diags:?}");
    assert!(
        diags
            .iter()
            .any(|d| d.message.contains("unknown identifier: k")),
        "g's failure names the masked own name: {diags:?}"
    );
}

#[test]
fn three_declarations_of_one_name_refuse_the_two_later_occurrences() {
    let nodes = vec![
        node(serde_json::json!({ "@id": "ex:M.k", "S231:value": 1.0 })),
        node(serde_json::json!({ "@id": "ex:M.inner.k", "S231:value": 2.0 })),
        node(serde_json::json!({ "@id": "ex:M.other.k", "S231:value": 3.0 })),
    ];
    let (entries, diags) = evaluate_root(&nodes, &["ex:M.k", "ex:M.inner.k", "ex:M.other.k"]);
    assert_eq!(
        diags.len(),
        2,
        "one diagnostic per occurrence beyond the first: {diags:?}"
    );
    for (diag, subject) in diags.iter().zip(["ex:M.inner.k", "ex:M.other.k"]) {
        assert_eq!(diag.subject.as_deref(), Some(subject));
        assert!(
            diag.message
                .starts_with("composite/duplicate-declaration: "),
            "{diag:?}"
        );
        assert!(
            diag.message.contains("first declared at ex:M.k"),
            "the message names the first occurrence: {diag:?}"
        );
    }
    assert_eq!(entries.len(), 1, "only the first occurrence grounds");
    assert!(scalar(&entries, "k").unwrap().bit_eq(&Value::Real(1.0)));
}

#[test]
fn cycle_members_refuse_once_while_independent_declarations_still_ground() {
    let nodes = vec![
        node(serde_json::json!({ "@id": "ex:M.a", "S231:value": "b + 1.0" })),
        node(serde_json::json!({ "@id": "ex:M.b", "S231:value": "a + 1.0" })),
        node(serde_json::json!({ "@id": "ex:M.c", "S231:value": 5.0 })),
        node(serde_json::json!({ "@id": "ex:M.d", "S231:value": "c + 1.0" })),
    ];
    let (entries, diags) = evaluate_root(&nodes, &["ex:M.a", "ex:M.b", "ex:M.c", "ex:M.d"]);
    assert_eq!(
        diags.len(),
        1,
        "one diagnostic per distinct cycle: {diags:?}"
    );
    assert_eq!(diags[0].subject.as_deref(), Some("ex:M.a"));
    assert_eq!(
        diags[0].message,
        "composite/declaration-cycle: cycle in the block's own declaration references: \
         ex:M.a -> ex:M.b -> ex:M.a"
    );
    assert!(scalar(&entries, "a").is_none() && scalar(&entries, "b").is_none());
    assert!(scalar(&entries, "d").unwrap().bit_eq(&Value::Real(6.0)));
}

#[test]
fn self_reference_refuses_as_a_length_one_cycle_never_reading_the_enclosing_binding() {
    let nodes = vec![node(
        serde_json::json!({ "@id": "ex:M.x", "S231:value": "x * 2.0" }),
    )];
    let root = node(serde_json::json!({
        "@id": "ex:M",
        "S231:hasParameter": { "@id": "ex:M.x" },
    }));
    let map = by_id(&nodes);
    let enclosing = vec![(Arc::<str>::from("x"), EvalResult::Scalar(Value::Real(1.0)))];
    let mut diags = Vec::new();
    let evaluation = evaluate_declarations(
        &root,
        &[],
        enclosing,
        &map,
        Pass::Lowering {
            specialization: &Specialization::default(),
            diags: &mut diags,
        },
    );
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(
        diags[0].message,
        "composite/declaration-cycle: cycle in the block's own declaration references: \
         ex:M.x -> ex:M.x"
    );
    // The enclosing prefix is preserved; the refused own x adds nothing.
    assert_eq!(evaluation.entries.len(), 1);
    assert!(
        scalar(&evaluation.entries, "x")
            .unwrap()
            .bit_eq(&Value::Real(1.0))
    );
}

#[test]
fn comprehension_iterator_shadows_a_same_named_declaration() {
    let nodes = vec![node(serde_json::json!({
        "@id": "ex:M.j",
        "S231:value": "sum(j for j in 1:3)"
    }))];
    let (entries, diags) = evaluate_root(&nodes, &["ex:M.j"]);
    assert!(
        diags.is_empty(),
        "the iterator must not create a self-cycle: {diags:?}"
    );
    assert!(
        scalar(&entries, "j")
            .unwrap_or_else(|| panic!("j must ground"))
            .bit_eq(&Value::Integer(6))
    );
}

#[test]
fn specialize_invocation_withholds_tagged_findings_and_silences_generic_machinery() {
    let nodes = vec![
        node(serde_json::json!({ "@id": "ex:M.a", "S231:value": "b" })),
        node(serde_json::json!({ "@id": "ex:M.b", "S231:value": "a" })),
        node(serde_json::json!({ "@id": "ex:M.broken", "S231:value": "nosuch" })),
        node(serde_json::json!({ "@id": "ex:M.ok", "S231:value": 4.0 })),
    ];
    let root = node(serde_json::json!({
        "@id": "ex:M",
        "S231:hasParameter": [
            { "@id": "ex:M.a" }, { "@id": "ex:M.b" },
            { "@id": "ex:M.broken" }, { "@id": "ex:M.ok" },
        ],
    }));
    let map = by_id(&nodes);
    let evaluation = evaluate_declarations(
        &root,
        &[],
        Vec::new(),
        &map,
        Pass::Specialize {
            composite_chain: true,
        },
    );
    assert_eq!(
        evaluation.withheld.len(),
        1,
        "only the tagged cycle finding is withheld — the generic grounding failure of `broken` \
         is non-emitting in this pass: {:?}",
        evaluation.withheld
    );
    assert!(
        evaluation.withheld[0]
            .message
            .starts_with("composite/declaration-cycle: "),
    );
    assert!(
        scalar(&evaluation.entries, "ok")
            .unwrap()
            .bit_eq(&Value::Real(4.0))
    );
}

#[test]
fn leaf_chain_cycle_and_duplicate_refuse_silently_at_the_specialize_invocation() {
    // R20-9: a leaf chain still evaluates at the specialize invocation (guards must ground),
    // but its cycle and duplicate participants are refused with NO tagged finding — they simply
    // fail to ground — while non-refused declarations still ground for guard evaluation.
    let nodes = vec![
        node(serde_json::json!({ "@id": "ex:M.con.a", "S231:value": "b" })),
        node(serde_json::json!({ "@id": "ex:M.con.b", "S231:value": "a" })),
        node(serde_json::json!({ "@id": "ex:M.con.k", "S231:value": 1.0 })),
        node(serde_json::json!({ "@id": "ex:M.con.other.k", "S231:value": 2.0 })),
        node(serde_json::json!({ "@id": "ex:M.con.have_hol", "S231:value": false })),
    ];
    let leaf = node(serde_json::json!({
        "@id": "ex:M.con",
        "S231:hasParameter": [
            { "@id": "ex:M.con.a" }, { "@id": "ex:M.con.b" },
            { "@id": "ex:M.con.k" }, { "@id": "ex:M.con.other.k" },
            { "@id": "ex:M.con.have_hol" },
        ],
    }));
    let map = by_id(&nodes);
    let evaluation = evaluate_declarations(
        &leaf,
        &[],
        Vec::new(),
        &map,
        Pass::Specialize {
            composite_chain: false,
        },
    );
    assert!(
        evaluation.withheld.is_empty(),
        "a leaf chain records no tagged finding for its cycle or its duplicate: {:?}",
        evaluation.withheld
    );
    assert!(
        scalar(&evaluation.entries, "a").is_none() && scalar(&evaluation.entries, "b").is_none(),
        "the cycle participants still fail to ground"
    );
    assert!(
        scalar(&evaluation.entries, "k")
            .unwrap()
            .bit_eq(&Value::Real(1.0)),
        "the first duplicate occurrence still grounds; the later one is refused silently"
    );
    assert!(
        scalar(&evaluation.entries, "have_hol")
            .unwrap()
            .bit_eq(&Value::Boolean(false)),
        "guard parameters still ground so guard evaluation can proceed"
    );
}

#[test]
fn numeric_exponent_suffixes_never_yield_identifier_tokens() {
    assert!(identifier_heads("1e-3 + 2E+5 * 10e2").is_empty());
    assert_eq!(identifier_heads("1e-3*k"), vec!["k"]);
}

#[test]
fn qualified_references_do_not_create_scope_dependencies() {
    assert_eq!(
        identifier_heads("Types.Mode.occupied + foo.bar - kBase"),
        vec!["kBase"]
    );
}

#[test]
fn comprehension_sources_use_outer_scope_and_iterators_shadow_the_body() {
    assert_eq!(
        identifier_heads("sum(j + k for j in source)"),
        vec!["source", "k"]
    );
}

#[test]
fn plain_identifiers_tokenize_between_operators_and_literals() {
    assert_eq!(
        identifier_heads("0.01*VMin_flow + max(kA, _b2)"),
        vec!["VMin_flow", "kA", "_b2"],
        "call arguments tokenize; the call head `max` does not"
    );
}

#[test]
fn call_heads_never_yield_identifier_tokens() {
    // oce-expr resolves a name followed by `(` only against its builtin table, never through
    // Scope lookup — so a call head is not a dependency edge, with or without whitespace
    // before the parenthesis.
    assert!(identifier_heads("max(1.0, 2.0)").is_empty());
    assert_eq!(identifier_heads("max + 1.0"), vec!["max"]);
    assert_eq!(
        identifier_heads("max (x)"),
        vec!["x"],
        "interior ASCII whitespace still makes `max` a call head; the argument tokenizes"
    );
}

#[test]
fn string_literal_bodies_never_contribute_declaration_heads() {
    for expression in [
        r#""kBase""#,
        r#""Types.Mode.occupied max(x) sibling""#,
        r#""quoted \"sibling\" and \\other""#,
        r#""non-ASCII sibling: Δ""#,
        r#""Trim amount \"triAmo\" and respond amount \"resAmo\" must have opposite signs.""#,
    ] {
        assert!(
            identifier_heads(expression).is_empty(),
            "string contents are not scope lookups: {expression:?}"
        );
    }
    assert_eq!(identifier_heads(r#""shadow" + actual"#), vec!["actual"]);
    assert_eq!(identifier_heads("\"\" + actual"), vec!["actual"]);
    assert_eq!(
        identifier_heads(r#""escaped multibyte \Δ then" + actual"#),
        vec!["actual"]
    );
}

#[test]
fn malformed_string_literals_do_not_manufacture_declaration_heads() {
    for expression in ["\"sibling", "\"sibling\\"] {
        assert!(
            identifier_heads(expression).is_empty(),
            "the expression parser, not the dependency census, diagnoses {expression:?}"
        );
    }
}

#[test]
fn string_literals_named_for_siblings_ground_without_false_cycles_in_both_orders() {
    let nodes = vec![
        node(serde_json::json!({ "@id": "ex:M.a", "S231:value": "\"b\"" })),
        node(serde_json::json!({ "@id": "ex:M.b", "S231:value": "a" })),
    ];
    for order in [["ex:M.a", "ex:M.b"], ["ex:M.b", "ex:M.a"]] {
        let (entries, diags) = evaluate_root(&nodes, &order);
        assert!(diags.is_empty(), "{order:?}: {diags:?}");
        for name in ["a", "b"] {
            assert!(
                scalar(&entries, name)
                    .unwrap_or_else(|| panic!("{name} must ground under {order:?}"))
                    .bit_eq(&Value::String(Arc::from("b"))),
                "{name} must preserve the string value under {order:?}"
            );
        }
    }
}

#[test]
fn string_literals_do_not_hide_a_real_sibling_cycle() {
    let nodes = vec![
        node(serde_json::json!({ "@id": "ex:M.msg", "S231:value": "\"a\"" })),
        node(serde_json::json!({ "@id": "ex:M.a", "S231:value": "b" })),
        node(serde_json::json!({ "@id": "ex:M.b", "S231:value": "a" })),
    ];
    let (entries, diags) = evaluate_root(&nodes, &["ex:M.msg", "ex:M.a", "ex:M.b"]);
    assert!(
        scalar(&entries, "msg")
            .unwrap()
            .bit_eq(&Value::String(Arc::from("a")))
    );
    assert!(scalar(&entries, "a").is_none() && scalar(&entries, "b").is_none());
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(
        diags[0].message,
        "composite/declaration-cycle: cycle in the block's own declaration references: \
         ex:M.a -> ex:M.b -> ex:M.a"
    );
}

#[test]
fn self_named_string_literal_is_not_a_self_reference() {
    let nodes = vec![node(
        serde_json::json!({ "@id": "ex:M.a", "S231:value": "\"a\"" }),
    )];
    let (entries, diags) = evaluate_root(&nodes, &["ex:M.a"]);
    assert!(diags.is_empty(), "{diags:?}");
    assert!(
        scalar(&entries, "a")
            .unwrap()
            .bit_eq(&Value::String(Arc::from("a")))
    );
}

#[test]
fn unterminated_self_named_literal_reports_the_expression_error_not_a_cycle() {
    let nodes = vec![node(
        serde_json::json!({ "@id": "ex:M.a", "S231:value": "\"a" }),
    )];
    let (entries, diags) = evaluate_root(&nodes, &["ex:M.a"]);
    assert!(entries.is_empty());
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, DiagCode::GroundingFailed);
    assert_eq!(diags[0].subject.as_deref(), Some("ex:M.a"));
    assert_eq!(
        diags[0].message,
        "expression binding did not ground: expression parse error: unterminated string literal"
    );
}
