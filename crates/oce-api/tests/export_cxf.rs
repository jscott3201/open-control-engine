//! Facade CXF export, fixpoint, warning, and content-identity contracts.

use oce_api::{ContentId, Engine, EngineStateError, ExportIncomplete, OcError, Value};
use oce_diag::DiagCode;

const MINIMAL: &str = include_str!("../../oce-cxf/tests/fixtures/minimal_loop.jsonld");
const PASS_THROUGH: &str =
    include_str!("../../oce-cxf/tests/fixtures/pass_through_miniature.jsonld");
/// The canonical content id of `minimal_loop.jsonld` (also pinned in
/// `context_expansion_identity.rs`).
const MINIMAL_LOOP_CONTENT_ID: &str =
    "oce:canon1:0cef94f71eddaea00a23797d1e5844ed2e6ca41a0be9a2e938fc587c5d31e6c8";
/// The canonical content id of both issue #338 `Reals.Abs` documents.
const ABS_DOCUMENT_CONTENT_ID: &str =
    "oce:canon1:e17765dcb273746d316a9813b8f2316b43adf1415120469dfe2eb203fc0a1549";
const DEFERRED: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/multizone_vav_return_fan_airflow_tracking.jsonld"
);

fn independent_fnv1a128(bytes: &[u8]) -> u128 {
    const OFFSET: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    const PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;
    let mut hash = OFFSET;
    for byte in bytes {
        hash ^= u128::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

/// Load, export, reload the export and export again: the bytes are a fixpoint and the canonical
/// content id survives the round trip. Returns the id.
fn assert_fixpoint(source: &[u8]) -> ContentId {
    let mut first = Engine::new();
    first.load_cxf(source).expect("fixture loads");
    let first_export = first.export_cxf().expect("first export succeeds");
    first_export
        .complete()
        .expect("fixpoint export is complete");
    let mut second = Engine::new();
    second
        .load_cxf(&first_export.bytes)
        .expect("export reloads");
    let second_export = second.export_cxf().expect("second export succeeds");
    assert_eq!(first_export.bytes, second_export.bytes);
    assert_eq!(
        first.canonical_content_bytes().unwrap(),
        second.canonical_content_bytes().unwrap()
    );
    let id = first.content_id().expect("loaded model has a content id");
    assert_eq!(second.content_id().unwrap(), id);
    id
}

#[test]
fn deterministic_export_reaches_fixpoint_for_regular_and_pass_through_models() {
    let id = assert_fixpoint(MINIMAL.as_bytes());
    assert_eq!(id.as_str(), MINIMAL_LOOP_CONTENT_ID);
    assert_fixpoint(PASS_THROUGH.as_bytes());
}

#[test]
fn export_bytes_are_unchanged_by_the_canonical_id() {
    // The export tag is gone; the emitted bytes it used to hash are pinned unchanged.
    let mut engine = Engine::new();
    engine.load_cxf(MINIMAL.as_bytes()).expect("fixture loads");
    let exported = engine.export_cxf().expect("fixture exports");
    assert_eq!(
        independent_fnv1a128(&exported.bytes),
        0x1291_31e1_e9e1_beaf_4001_9e56_fe1c_b285
    );
}

#[test]
fn completeness_boundary_distinguishes_zero_from_one_warning() {
    let mut complete_engine = Engine::new();
    complete_engine
        .load_cxf(MINIMAL.as_bytes())
        .expect("complete fixture loads");
    let complete = complete_engine
        .export_cxf()
        .expect("complete fixture exports");
    assert!(complete.warnings.is_empty());
    assert_eq!(complete.complete(), Ok(()));

    let mut partial_engine = Engine::new();
    partial_engine
        .load_cxf(DEFERRED.as_bytes())
        .expect("deferred fixture loads");
    let mut exactly_one = partial_engine.export_cxf().expect("survivor cone exports");
    assert_eq!(exactly_one.warnings.len(), 2);
    exactly_one.warnings.truncate(1);
    assert_eq!(exactly_one.warnings.len(), 1);
    assert!(matches!(
        exactly_one.complete(),
        Err(ExportIncomplete {
            warning_count: 1,
            ..
        })
    ));
    exactly_one.warnings.clear();
    assert_eq!(
        exactly_one.complete(),
        Ok(()),
        "the check reads the report as it is"
    );
}

#[test]
fn content_id_tracks_applied_parameter_edits_while_model_id_stays_authored() {
    let mut engine = Engine::new();
    let (loaded, _) = engine.load_cxf(MINIMAL.as_bytes()).expect("fixture loads");
    assert_eq!(loaded.model_id.as_str(), "http://example.org#MinLoop");
    let before = engine.content_id().expect("loaded model has an id");
    let original = engine
        .get_param("http://example.org#MinLoop.con.k")
        .expect("the constant has a value");
    let rendered = engine.export_cxf().expect("fixture exports").bytes;
    let rendered = std::str::from_utf8(&rendered).expect("export is UTF-8 JSON-LD");
    assert!(rendered.contains(r#""@id":"urn:open-control:cxf-export:root""#));
    assert!(!rendered.contains(r#""@id":"http://example.org#MinLoop""#));

    engine.halt().expect("loaded engine halts");
    assert_eq!(engine.content_id().unwrap(), before, "halt edits nothing");
    engine
        .set_param("http://example.org#MinLoop.con.k", Value::Real(3.0))
        .expect("constant parameter is legally tunable at rest");
    // An id always names what would execute, so pending edits refuse, as compile_blob does.
    assert!(matches!(
        engine.content_id(),
        Err(OcError::State(EngineStateError::PendingParameterEdits))
    ));
    assert!(
        engine.export_cxf().is_ok(),
        "export_cxf is unchanged by pending edits"
    );
    engine.resume().expect("edited engine resumes");
    let after = engine.content_id().expect("resumed model has an id");
    assert_ne!(before, after);
    assert_eq!(loaded.model_id.as_str(), "http://example.org#MinLoop");

    // Setting the value back restores the original id: it names the table, not the history.
    engine.halt().unwrap();
    engine
        .set_param("http://example.org#MinLoop.con.k", original)
        .unwrap();
    engine.resume().unwrap();
    assert_eq!(engine.content_id().unwrap(), before);
}

#[test]
fn a_warning_bearing_export_is_incomplete_while_the_content_id_stays_defined() {
    let mut engine = Engine::new();
    engine.load_cxf(DEFERRED.as_bytes()).expect("fixture loads");
    let exported = engine.export_cxf().expect("survivor cone exports");
    assert!(!exported.warnings.is_empty());
    assert!(
        exported
            .warnings
            .iter()
            .all(|warning| warning.code == DiagCode::ExportDeferred)
    );
    let expected_warning_count = exported.warnings.len();
    assert!(matches!(
        exported.complete(),
        Err(ExportIncomplete { warning_count, .. })
            if warning_count == expected_warning_count
    ));
    // The canonical id names the loaded model, partial export or not.
    let id = engine
        .content_id()
        .expect("a partial export still has a content id");
    assert_eq!(engine.content_id().unwrap(), id);
    assert_eq!(
        engine.compile_blob().unwrap().content_id.as_ref(),
        Some(&id),
        "the blob records the same id"
    );
}

/// Issue #338 document A: one `Reals.Abs` between boundary `u` and `y`, with the `m.abs.u` port
/// node before the `m.abs.y` port node. Document B swaps exactly those two `@graph` entries; every
/// node body and edge is unchanged.
fn abs_document(abs_output_node_first: bool) -> String {
    const HEAD: &str = r#"{"@context":{"S231":"http://data.ashrae.org/S231#","base":"urn:example:m#"},"@graph":[
{"@id":"urn:example:m#m","@type":"S231:Block","S231:label":"m",
 "S231:containsBlock":[{"@id":"urn:example:m#m.abs"}],
 "S231:hasInput":[{"@id":"urn:example:m#m.u"}],"S231:hasOutput":[{"@id":"urn:example:m#m.y"}]},
{"@id":"urn:example:m#m.abs","@type":"urn:example:m#Buildings.Controls.OBC.CDL.Reals.Abs",
 "S231:label":"abs","S231:hasInput":{"@id":"urn:example:m#m.abs.u"},
 "S231:hasOutput":{"@id":"urn:example:m#m.abs.y"}},
"#;
    const ABS_INPUT: &str = r#"{"@id":"urn:example:m#m.abs.u","@type":"S231:RealInput",
 "S231:isOfDataType":{"@id":"S231:Real"}}"#;
    const ABS_OUTPUT: &str = r#"{"@id":"urn:example:m#m.abs.y","@type":"S231:RealOutput",
 "S231:isOfDataType":{"@id":"S231:Real"},"S231:isConnectedTo":{"@id":"urn:example:m#m.y"}}"#;
    const TAIL: &str = r#",
{"@id":"urn:example:m#m.u","@type":"S231:RealInput","S231:isOfDataType":{"@id":"S231:Real"},
 "S231:isConnectedTo":{"@id":"urn:example:m#m.abs.u"}},
{"@id":"urn:example:m#m.y","@type":"S231:RealOutput","S231:isOfDataType":{"@id":"S231:Real"}}
]}"#;
    let (first, second) = if abs_output_node_first {
        (ABS_OUTPUT, ABS_INPUT)
    } else {
        (ABS_INPUT, ABS_OUTPUT)
    };
    format!("{HEAD}{first},\n{second}{TAIL}")
}

fn clean_content_id(source: &[u8]) -> (ContentId, Vec<u8>) {
    let mut engine = Engine::new();
    let (receipt, _) = engine
        .load_cxf_with_receipt(source)
        .expect("order variant loads");
    assert!(receipt.diagnostics().records().is_empty());
    // Both orderings carry the same authored model identity.
    assert_eq!(receipt.report().model_id.as_str(), "urn:example:m#m");
    let exported = engine.export_cxf().expect("order variant exports");
    exported
        .complete()
        .expect("order variant export is complete");
    (engine.content_id().unwrap(), exported.bytes)
}

#[test]
fn content_id_is_independent_of_authored_graph_order() {
    let document_a = abs_document(false);
    let document_b = abs_document(true);
    assert_ne!(document_a, document_b);
    assert_eq!(document_a.len(), document_b.len());

    let (id_a, export_a) = clean_content_id(document_a.as_bytes());
    let (id_b, export_b) = clean_content_id(document_b.as_bytes());
    // `@graph` node order is still order-bearing for dense ids (docs/cxf-composite-subset.md):
    // the swapped port nodes receive different connector ids, so the emitted documents differ,
    // exactly as the retired order-bearing export tags did (issue #338).
    assert_eq!(
        independent_fnv1a128(&export_a),
        0xd282_8dca_c6fd_b609_2a77_0b7f_5aec_e499
    );
    assert_eq!(
        independent_fnv1a128(&export_b),
        0x6f68_fd0f_2321_3d72_261d_8a28_2e96_2a71
    );
    // The canonical id names the loaded graph, so authored order no longer moves it.
    assert_eq!(id_a, id_b);
    assert_eq!(id_a.as_str(), ABS_DOCUMENT_CONTENT_ID);

    // Control: the result repeats load to load.
    assert_eq!(clean_content_id(document_a.as_bytes()).0, id_a);
    assert_eq!(clean_content_id(document_b.as_bytes()).0, id_b);
}

#[test]
fn unloaded_engine_returns_an_error_without_panicking() {
    assert!(Engine::new().export_cxf().is_err());
    assert!(matches!(
        Engine::new().content_id(),
        Err(OcError::State(EngineStateError::NoLoadedModel))
    ));
}
