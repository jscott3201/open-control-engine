//! Facade pins for CDL §7.10 single assignment at load: a child input with two drivers is refused
//! in every shape (issue #472).
//!
//! Before issue #472 the mixed shape — one block output and one root boundary input on the same
//! input — loaded warning-free, and the tick's gather read the wire's slot, so the frame value the
//! host staged for the boundary input was discarded without a trace. These tests read the refusal
//! the way a host must, through `load_cxf` and `load_cxf_with_receipt`, over the published
//! rejected-corpus fixtures, a negative control that runs, and hostile spellings of the same
//! double driver.

use std::fs;

use oce_api::{DiagnosticStage, DiagnosticSubject, Engine, OcError, Value};
use serde_json::{Value as Json, json};

const CORPUS: &str = "../oce-cxf/tests/fixtures/composite_contract/rejected";
const MIXED: &str = "input is driven by a block output and a boundary input";
const ECON: &str = "http://example.org#econ_enable";

fn fixture(name: &str) -> Vec<u8> {
    fs::read(format!("{}/{CORPUS}/{name}", env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|e| panic!("read rejected corpus fixture {name}: {e}"))
}

fn econ() -> Json {
    serde_json::from_slice(&fixture("double_driver_econ_enable.jsonld")).expect("fixture JSON")
}

fn iri(local: &str) -> String {
    format!("{ECON}.{local}")
}

fn node_mut<'a>(document: &'a mut Json, local: &str) -> &'a mut Json {
    let id = iri(local);
    document["@graph"]
        .as_array_mut()
        .expect("@graph")
        .iter_mut()
        .find(|node| node["@id"].as_str() == Some(id.as_str()))
        .unwrap_or_else(|| panic!("no node {id}"))
}

fn set_targets(document: &mut Json, local: &str, targets: &[&str]) {
    node_mut(document, local)["S231:isConnectedTo"] = Json::Array(
        targets
            .iter()
            .map(|target| json!({ "@id": iri(target) }))
            .collect(),
    );
}

fn bytes(document: &Json) -> Vec<u8> {
    serde_json::to_vec(document).expect("serialize")
}

/// `(code, subject, message)` of every diagnostic a refused `load_cxf` returned.
fn refusal(bytes: &[u8]) -> Vec<(String, Option<String>, String)> {
    match Engine::new().load_cxf(bytes) {
        Ok((report, _)) => panic!("expected a refusal, loaded with {:?}", report.warnings),
        Err(error @ OcError::Cxf(_)) => error
            .diagnostics()
            .iter()
            .map(|d| {
                (
                    d.code.as_str().to_owned(),
                    d.subject.as_deref().map(str::to_owned),
                    d.message.clone(),
                )
            })
            .collect(),
        Err(other) => panic!("expected an import refusal, got {other:?}"),
    }
}

fn single_assignment(subject: &str, message: &str) -> Vec<(String, Option<String>, String)> {
    vec![(
        "single-assignment".to_owned(),
        Some(subject.to_owned()),
        message.to_owned(),
    )]
}

/// The three double-driver shapes and the worked example, each with its one pinned refusal.
fn shapes() -> [(&'static str, String, &'static str); 4] {
    let abs = "http://example.org#M.abs.u".to_owned();
    [
        (
            "double_driver_two_block_outputs.jsonld",
            abs.clone(),
            "input is multiply driven (in-degree 2)",
        ),
        (
            "double_driver_two_boundary_inputs.jsonld",
            abs.clone(),
            "input is driven by distinct boundary inputs",
        ),
        (
            "double_driver_block_output_and_boundary_input.jsonld",
            abs,
            MIXED,
        ),
        ("double_driver_econ_enable.jsonld", iri("hys.u"), MIXED),
    ]
}

#[test]
fn every_double_driver_shape_is_refused_through_load_cxf() {
    for (name, subject, message) in shapes() {
        assert_eq!(
            refusal(&fixture(name)),
            single_assignment(&subject, message),
            "{name}"
        );
    }
}

#[test]
fn every_double_driver_shape_is_refused_at_import_on_the_receipt_path() {
    for (name, subject, message) in shapes() {
        let failure = Engine::new()
            .load_cxf_with_receipt(&fixture(name))
            .expect_err("a double driver never loads");
        assert_eq!(failure.stage(), DiagnosticStage::Import, "{name}");
        let records: Vec<_> = failure
            .diagnostics()
            .records()
            .iter()
            .map(|record| {
                let key = record.key();
                let subject = match key.subject() {
                    DiagnosticSubject::Opaque(text) => Some(text.clone()),
                    _ => None,
                };
                (key.code().to_owned(), subject, record.message().to_owned())
            })
            .collect();
        assert_eq!(records, single_assignment(&subject, message), "{name}");
    }
}

#[test]
fn removing_the_extra_connection_loads_warning_free_and_the_block_output_drives() {
    let mut document = econ();
    set_targets(&mut document, "TOut", &["sub.u2"]);
    let mut engine = Engine::new();
    let (report, _) = engine
        .load_cxf(&bytes(&document))
        .expect("the worked example without the extra edge loads");
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let (receipt, _) = Engine::new()
        .load_cxf_with_receipt(&bytes(&document))
        .expect("the receipt path agrees");
    assert!(receipt.diagnostics().records().is_empty());
    // hys.u = TOutCut - TOut with uLow = 0, uHigh = dTHys = 1. Each frame separates the two
    // candidate drivers: TOut alone would read high where the difference reads low, and back.
    for (time, t_out, t_out_cut, enabled) in [
        (0.0, 20.0, 10.0, false),
        (1.0, -5.0, 0.0, true),
        (2.0, 0.5, 1.0, true),
        (3.0, 5.0, 4.0, false),
    ] {
        let frame = engine
            .prepare_frame(
                time,
                &[
                    (iri("TOut").as_str(), Value::Real(t_out)),
                    (iri("TOutCut").as_str(), Value::Real(t_out_cut)),
                ],
            )
            .expect("both declared inputs");
        let completed = engine.execute_frame(frame).expect("frame runs");
        let outputs = completed.outputs();
        assert_eq!(outputs.len(), 1, "t = {time}");
        assert_eq!(outputs[0].0, iri("ecoEna"));
        assert!(
            matches!(outputs[0].1, Value::Boolean(value) if value == enabled),
            "t = {time}: {:?}",
            outputs[0].1
        );
    }
}

#[test]
fn removing_the_block_wire_instead_loads_warning_free_and_the_boundary_input_drives() {
    // The other half of the control: the boundary edge alone is a legal single driver, and at run
    // time `hys.u` follows `TOut` alone. `TOutCut` stays at 100, so `sub.y = 100 - TOut` would read
    // high (enabled) on every frame; the disabled frames prove the boundary value is what is read.
    let mut document = econ();
    node_mut(&mut document, "sub.y")
        .as_object_mut()
        .expect("node")
        .remove("S231:isConnectedTo");
    let mut engine = Engine::new();
    let (report, _) = engine
        .load_cxf(&bytes(&document))
        .expect("TOut alone drives hys.u");
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    // uLow = 0, uHigh = 1: below 0 clears, above 1 sets, in between holds.
    for (time, t_out, enabled) in [
        (0.0, -5.0, false),
        (1.0, 2.0, true),
        (2.0, 0.5, true),
        (3.0, -1.0, false),
        (4.0, 0.5, false),
    ] {
        let frame = engine
            .prepare_frame(
                time,
                &[
                    (iri("TOut").as_str(), Value::Real(t_out)),
                    (iri("TOutCut").as_str(), Value::Real(100.0)),
                ],
            )
            .expect("both declared inputs");
        let completed = engine.execute_frame(frame).expect("frame runs");
        let outputs = completed.outputs();
        assert_eq!(outputs.len(), 1, "t = {time}");
        assert_eq!(outputs[0].0, iri("ecoEna"));
        assert!(
            matches!(outputs[0].1, Value::Boolean(value) if value == enabled),
            "t = {time}: {:?}",
            outputs[0].1
        );
    }
}

#[test]
fn two_block_outputs_and_a_boundary_input_are_refused_once_as_mixed() {
    // Three drivers on `hys.u`: `sub.y`, a second block output `extra.y`, and `TOut`. The boundary
    // pass is the only refuser of a frame-driven input, so the input gets exactly one refusal, the
    // mixed message, with no count.
    let mut document = econ();
    document["@graph"][0]["S231:containsBlock"]
        .as_array_mut()
        .expect("containsBlock")
        .push(json!({ "@id": iri("extra") }));
    document["@graph"].as_array_mut().expect("@graph").extend([
        json!({
            "@id": iri("extra"),
            "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
            "S231:hasParameter": { "@id": iri("extra.k") },
            "S231:hasOutput": { "@id": iri("extra.y") }
        }),
        json!({ "@id": iri("extra.k"), "S231:value": 1 }),
        json!({
            "@id": iri("extra.y"), "@type": "S231:RealOutput",
            "S231:isOfDataType": { "@id": "S231:Real" },
            "S231:isConnectedTo": { "@id": iri("hys.u") }
        }),
    ]);
    assert_eq!(
        refusal(&bytes(&document)),
        single_assignment(&iri("hys.u"), MIXED)
    );
}

#[test]
fn a_reversed_boundary_edge_is_the_same_double_driver() {
    // `hys.u isConnectedTo TOut` instead of `TOut isConnectedTo hys.u`: orientation is derived,
    // so the authored direction cannot hide the second driver.
    let mut document = econ();
    set_targets(&mut document, "TOut", &["sub.u2"]);
    set_targets(&mut document, "hys.u", &["TOut"]);
    assert_eq!(
        refusal(&bytes(&document)),
        single_assignment(&iri("hys.u"), MIXED)
    );
}

#[test]
fn a_reversed_block_edge_is_the_same_double_driver() {
    let mut document = econ();
    node_mut(&mut document, "sub.y")
        .as_object_mut()
        .expect("node")
        .remove("S231:isConnectedTo");
    set_targets(&mut document, "hys.u", &["sub.y"]);
    assert_eq!(
        refusal(&bytes(&document)),
        single_assignment(&iri("hys.u"), MIXED)
    );
}

#[test]
fn a_boundary_input_listed_twice_on_the_target_beside_a_wire_is_refused_once() {
    let mut document = econ();
    set_targets(&mut document, "TOut", &["sub.u2", "hys.u", "hys.u"]);
    assert_eq!(
        refusal(&bytes(&document)),
        single_assignment(&iri("hys.u"), MIXED)
    );
}

#[test]
fn a_boundary_input_listed_twice_on_the_target_alone_is_refused() {
    // No block wire: the repeated boundary edge is itself a second assignment, refused by the
    // pre-existing boundary-input count with its own message (unchanged by issue #472).
    let mut document = econ();
    node_mut(&mut document, "sub.y")
        .as_object_mut()
        .expect("node")
        .remove("S231:isConnectedTo");
    set_targets(&mut document, "TOut", &["sub.u2", "hys.u", "hys.u"]);
    assert_eq!(
        refusal(&bytes(&document)),
        single_assignment(&iri("hys.u"), "input is multiply driven (in-degree 2)")
    );
}

/// Insert a nested composite `relay` (one unconnected `Constant` child, so it classifies as a
/// composite) whose boundary input passes straight through to its boundary output, and route `TOut -> relay.u`, `relay.y -> hys.u` instead of `TOut -> hys.u`.
fn with_relay(mut document: Json) -> Json {
    set_targets(&mut document, "TOut", &["sub.u2", "relay.u"]);
    document["@graph"][0]["S231:containsBlock"]
        .as_array_mut()
        .expect("containsBlock")
        .push(json!({ "@id": iri("relay") }));
    document["@graph"].as_array_mut().expect("@graph").extend([
        json!({
            "@id": iri("relay"), "@type": "S231:Block",
            "S231:containsBlock": { "@id": iri("relay.keep") },
            "S231:hasInput": { "@id": iri("relay.u") },
            "S231:hasOutput": { "@id": iri("relay.y") }
        }),
        json!({
            "@id": iri("relay.keep"),
            "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
            "S231:hasParameter": { "@id": iri("relay.keep.k") },
            "S231:hasOutput": { "@id": iri("relay.keep.y") }
        }),
        json!({ "@id": iri("relay.keep.k"), "S231:value": 1 }),
        json!({
            "@id": iri("relay.keep.y"), "@type": "S231:RealOutput",
            "S231:isOfDataType": { "@id": "S231:Real" }
        }),
        json!({
            "@id": iri("relay.u"), "@type": "S231:RealInput",
            "S231:isOfDataType": { "@id": "S231:Real" },
            "S231:isConnectedTo": { "@id": iri("relay.y") }
        }),
        json!({
            "@id": iri("relay.y"), "@type": "S231:RealOutput",
            "S231:isOfDataType": { "@id": "S231:Real" },
            "S231:isConnectedTo": { "@id": iri("hys.u") }
        }),
    ]);
    document
}

#[test]
fn a_boundary_input_reaching_the_target_through_a_nested_pass_through_is_refused() {
    let document = with_relay(econ());
    assert_eq!(
        refusal(&bytes(&document)),
        single_assignment(&iri("hys.u"), MIXED)
    );
}

#[test]
fn the_nested_pass_through_alone_is_a_legal_single_driver() {
    // Control for the two relay variants: with the block wire removed the relay route loads.
    let mut document = with_relay(econ());
    node_mut(&mut document, "sub.y")
        .as_object_mut()
        .expect("node")
        .remove("S231:isConnectedTo");
    let (report, _) = Engine::new()
        .load_cxf(&bytes(&document))
        .expect("TOut through the relay alone drives hys.u");
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
}

#[test]
fn a_block_output_reaching_a_boundary_driven_target_through_a_nested_pass_through_is_refused() {
    // The mirror: `sub.y -> relay.u -> relay.y -> hys.u` while `TOut -> hys.u` stays direct.
    let mut document = with_relay(econ());
    set_targets(&mut document, "TOut", &["sub.u2", "hys.u"]);
    set_targets(&mut document, "sub.y", &["relay.u"]);
    assert_eq!(
        refusal(&bytes(&document)),
        single_assignment(&iri("hys.u"), MIXED)
    );
}

#[test]
fn a_sequence_parameter_wired_to_a_boundary_driven_input_is_refused() {
    // A parameter is not a connector, so `dTHys isConnectedTo hys.u` cannot be a second driver:
    // the edge's source resolves to no connector and the document is refused for that, before
    // any assignment count could see it.
    let mut document = econ();
    node_mut(&mut document, "sub.y")
        .as_object_mut()
        .expect("node")
        .remove("S231:isConnectedTo");
    set_targets(&mut document, "dTHys", &["hys.u"]);
    assert_eq!(
        refusal(&bytes(&document)),
        vec![(
            "unresolved-reference".to_owned(),
            Some(iri("dTHys")),
            "connection source not found".to_owned(),
        )]
    );
}

#[test]
fn repeated_refusals_are_identical() {
    for (name, _, _) in shapes() {
        assert_eq!(refusal(&fixture(name)), refusal(&fixture(name)), "{name}");
    }
}
