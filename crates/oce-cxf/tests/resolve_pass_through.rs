//! Native boundary input-to-output pass-through lowering contracts.

use oce_cxf::{CxfError, ResolveOptions, export, import_cxf};
use oce_diag::{DiagCode, Diagnostic};
use oce_model::ModelGraph;
use serde_json::{Value, json};

const FIXTURE: &str = include_str!("fixtures/pass_through_miniature.jsonld");
const NESTED_FIXTURE: &str = include_str!("fixtures/nested_composite.jsonld");

fn document() -> Value {
    serde_json::from_str(FIXTURE).expect("pass-through fixture JSON")
}

fn node_mut<'a>(document: &'a mut Value, suffix: &str) -> &'a mut Value {
    document["@graph"]
        .as_array_mut()
        .expect("@graph array")
        .iter_mut()
        .find(|node| {
            node["@id"]
                .as_str()
                .is_some_and(|iri| iri.ends_with(suffix))
        })
        .unwrap_or_else(|| panic!("missing node ending in {suffix}"))
}

fn import(document: &Value) -> Result<ModelGraph, Vec<Diagnostic>> {
    let bytes = serde_json::to_vec(document).expect("serialize document");
    match import_cxf(&bytes, &ResolveOptions::default()) {
        Ok((graph, report)) => {
            assert!(report.is_empty(), "unexpected diagnostics: {report:?}");
            Ok(graph)
        }
        Err(CxfError::Validation(diagnostics)) => Err(diagnostics),
        Err(error) => panic!("unexpected import error: {error:?}"),
    }
}

fn diagnostic_bytes(document: &Value) -> Vec<u8> {
    format!(
        "{:?}",
        import(document).expect_err("document must be rejected")
    )
    .into_bytes()
}

fn assert_repeated_rejection(document: &Value, code: DiagCode, message: &str) {
    let first = diagnostic_bytes(document);
    assert_eq!(first, diagnostic_bytes(document));
    let text = String::from_utf8(first).expect("diagnostics debug text");
    assert!(text.contains(&format!("{code:?}")), "{text}");
    assert!(text.contains(message), "{text}");
}

#[test]
fn three_scalar_types_lower_with_boundary_iris_on_both_sides() {
    let graph = import(&document()).expect("fixture imports");
    assert_eq!(graph.blocks.len(), 4);
    let lowered = &graph.blocks[1..];
    assert_eq!(
        lowered
            .iter()
            .map(|block| block.class_iri.as_ref())
            .collect::<Vec<_>>(),
        [
            "urn:oce:lowering#PassThrough.Real",
            "urn:oce:lowering#PassThrough.Integer",
            "urn:oce:lowering#PassThrough.Boolean",
        ]
    );
    for block in lowered {
        let input = &graph.connectors[block.inputs[0].0 as usize];
        let output = &graph.connectors[block.outputs[0].0 as usize];
        assert!(input.iri.as_deref().is_some_and(|iri| iri.ends_with("In")));
        assert!(
            output
                .iri
                .as_deref()
                .is_some_and(|iri| iri.ends_with("Out"))
        );
        assert!(graph.external_inputs.contains(&input.id));
    }
}

#[test]
fn reverse_and_duplicate_spellings_collapse_to_one_synthetic_without_diagnostics() {
    let mut reverse = document();
    node_mut(&mut reverse, "realIn")
        .as_object_mut()
        .unwrap()
        .remove("S231:isConnectedTo");
    node_mut(&mut reverse, "realOut")["S231:isConnectedTo"] =
        json!({ "@id": "http://example.org#PassThroughMiniature.realIn" });
    assert_eq!(import(&reverse).expect("reverse imports").blocks.len(), 4);

    let mut duplicate = document();
    node_mut(&mut duplicate, "realIn")["S231:isConnectedTo"] = json!([
        { "@id": "http://example.org#PassThroughMiniature.realOut" },
        { "@id": "http://example.org#PassThroughMiniature.realOut" }
    ]);
    assert_eq!(
        import(&duplicate).expect("duplicate imports").blocks.len(),
        4
    );

    node_mut(&mut duplicate, "realOut")["S231:isConnectedTo"] =
        json!({ "@id": "http://example.org#PassThroughMiniature.realIn" });
    assert_eq!(
        import(&duplicate)
            .expect("both orientations import")
            .blocks
            .len(),
        4
    );
}

#[test]
fn rejection_diagnostics_are_named_and_byte_deterministic() {
    let mut mismatch = document();
    node_mut(&mut mismatch, "realOut")["S231:isOfDataType"] = json!({ "@id": "S231:Boolean" });
    assert_repeated_rejection(&mismatch, DiagCode::TypeMismatch, "connected types differ");

    let mut array = document();
    node_mut(&mut array, "realOut")["S231:isArray"] = json!(true);
    node_mut(&mut array, "realOut")["S231:sizeOfDimensions"] = json!("(2)");
    assert_repeated_rejection(
        &array,
        DiagCode::NonSubsetConstruct,
        "array boundary pass-through endpoints are unsupported",
    );

    let mut input_pair = document();
    node_mut(&mut input_pair, "realIn")["S231:isConnectedTo"] =
        json!({ "@id": "http://example.org#PassThroughMiniature.integerIn" });
    assert_repeated_rejection(
        &input_pair,
        DiagCode::DirectionMismatch,
        "connection joins two boundary inputs",
    );

    let mut output_pair = document();
    node_mut(&mut output_pair, "realIn")
        .as_object_mut()
        .unwrap()
        .remove("S231:isConnectedTo");
    node_mut(&mut output_pair, "realOut")["S231:isConnectedTo"] =
        json!({ "@id": "http://example.org#PassThroughMiniature.integerOut" });
    assert_repeated_rejection(
        &output_pair,
        DiagCode::DirectionMismatch,
        "connection joins two boundary outputs",
    );

    let mut multiply = document();
    node_mut(&mut multiply, "integerIn")["S231:isConnectedTo"] =
        json!({ "@id": "http://example.org#PassThroughMiniature.realOut" });
    node_mut(&mut multiply, "integerIn")["S231:isOfDataType"] = json!({ "@id": "S231:Real" });
    assert_repeated_rejection(
        &multiply,
        DiagCode::SingleAssignment,
        "boundary output is multiply driven",
    );
}

#[test]
fn underivable_type_does_not_add_a_spurious_type_mismatch() {
    let mut document = document();
    node_mut(&mut document, "realOut")
        .as_object_mut()
        .expect("output object")
        .remove("S231:isOfDataType");
    node_mut(&mut document, "realOut")["@type"] = json!("S231:Output");
    let diagnostics = import(&document).expect_err("underivable datatype is diagnosed");
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code != DiagCode::TypeMismatch),
        "{diagnostics:?}"
    );
}

#[test]
fn reserved_iri_authored_use_remains_an_unknown_class() {
    let mut document = document();
    node_mut(&mut document, ".keep")["@type"] = json!("urn:oce:lowering#PassThrough.Real");
    let diagnostics = import(&document).expect_err("reserved authored class must not resolve");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagCode::ClassNotFound),
        "{diagnostics:?}"
    );
}

#[test]
fn legacy_dangling_boundary_messages_remain_pinned() {
    let mut missing_target = document();
    node_mut(&mut missing_target, "realIn")["S231:isConnectedTo"] =
        json!({ "@id": "http://example.org#PassThroughMiniature.missing" });
    assert_repeated_rejection(
        &missing_target,
        DiagCode::UnresolvedReference,
        "boundary-input target not found",
    );

    let mut missing_source = document();
    node_mut(&mut missing_source, "realIn")
        .as_object_mut()
        .unwrap()
        .remove("S231:isConnectedTo");
    node_mut(&mut missing_source, "realOut")["S231:isConnectedTo"] =
        json!({ "@id": "http://example.org#PassThroughMiniature.missing" });
    assert_repeated_rejection(
        &missing_source,
        DiagCode::UnresolvedReference,
        "boundary-output source not found",
    );
}

#[test]
fn pass_through_target_array_order_does_not_change_graph_or_export() {
    let mut first = document();
    node_mut(&mut first, "#PassThroughMiniature")["S231:hasOutput"] = json!([
        { "@id": "http://example.org#PassThroughMiniature.realOut" },
        { "@id": "http://example.org#PassThroughMiniature.realOutSecond" },
        { "@id": "http://example.org#PassThroughMiniature.integerOut" },
        { "@id": "http://example.org#PassThroughMiniature.booleanOut" }
    ]);
    first["@graph"].as_array_mut().unwrap().insert(
        6,
        json!({
            "@id": "http://example.org#PassThroughMiniature.realOutSecond",
            "@type": "S231:RealOutput",
            "S231:isOfDataType": { "@id": "S231:Real" }
        }),
    );
    node_mut(&mut first, "realIn")["S231:isConnectedTo"] = json!([
        { "@id": "http://example.org#PassThroughMiniature.realOut" },
        { "@id": "http://example.org#PassThroughMiniature.realOutSecond" }
    ]);
    let mut second = first.clone();
    node_mut(&mut second, "realIn")["S231:isConnectedTo"]
        .as_array_mut()
        .unwrap()
        .reverse();

    let first_graph = import(&first).expect("first spelling imports");
    let second_graph = import(&second).expect("second spelling imports");
    assert_eq!(first_graph.external_inputs, second_graph.external_inputs);
    assert_eq!(
        export(&first_graph).expect("first exports"),
        export(&second_graph).expect("second exports")
    );
}

#[test]
fn nested_pass_through_splices_in_both_spellings() {
    let mut forward: Value = serde_json::from_str(NESTED_FIXTURE).expect("nested fixture JSON");
    node_mut(&mut forward, ".sub")["S231:containsBlock"] =
        json!({ "@id": "http://example.org#g36.profile.nested_composite.sub.enumCarrier" });
    node_mut(&mut forward, ".sub.u")["S231:isConnectedTo"] =
        json!({ "@id": "http://example.org#g36.profile.nested_composite.sub.y" });
    node_mut(&mut forward, ".sub.gain.y")
        .as_object_mut()
        .unwrap()
        .remove("S231:isConnectedTo");
    let forward_graph = import(&forward).expect("forward nested pass-through imports");

    let mut reverse = forward.clone();
    node_mut(&mut reverse, ".sub.u")
        .as_object_mut()
        .unwrap()
        .remove("S231:isConnectedTo");
    node_mut(&mut reverse, ".sub.y")["S231:isConnectedTo"] = json!([
        { "@id": "http://example.org#g36.profile.nested_composite.sub.u" },
        { "@id": "http://example.org#g36.profile.nested_composite.post.u" }
    ]);
    let reverse_graph = import(&reverse).expect("reverse nested pass-through imports");
    assert_eq!(forward_graph.connections, reverse_graph.connections);
    assert_eq!(forward_graph.external_inputs, reverse_graph.external_inputs);
}
