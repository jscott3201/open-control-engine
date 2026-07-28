//! Facade CXF export, fixpoint, warning, and content-identity contracts.

use oce_api::Engine;

const MINIMAL: &str = include_str!("../../oce-cxf/tests/fixtures/minimal_loop.jsonld");
const PASS_THROUGH: &str =
    include_str!("../../oce-cxf/tests/fixtures/pass_through_miniature.jsonld");
const DEFERRED: &str = include_str!("../../oce-cxf/tests/fixtures/enum_deferral_miniature.jsonld");

fn assert_fixpoint(source: &[u8]) -> String {
    let mut first = Engine::in_memory();
    first.load_cxf(source).expect("fixture loads");
    let first_export = first.export_cxf().expect("first export succeeds");
    let mut second = Engine::in_memory();
    second
        .load_cxf(&first_export.bytes)
        .expect("export reloads");
    let second_export = second.export_cxf().expect("second export succeeds");
    assert_eq!(first_export.bytes, second_export.bytes);
    assert_eq!(first_export.content_id(), second_export.content_id());
    first_export.content_id()
}

#[test]
fn deterministic_export_reaches_fixpoint_for_regular_and_pass_through_models() {
    let id = assert_fixpoint(MINIMAL.as_bytes());
    assert_eq!(id, "cxf:fnv1a128:d9e52c9615c014596ecb65661e61447d");
    assert_fixpoint(PASS_THROUGH.as_bytes());
}

#[test]
fn content_id_is_distinct_from_authored_model_identity() {
    let mut engine = Engine::in_memory();
    let loaded = engine.load_cxf(MINIMAL.as_bytes()).expect("fixture loads");
    let exported = engine.export_cxf().expect("fixture exports");
    assert_ne!(loaded.model_id.as_str(), exported.content_id());
}

#[test]
fn warning_bearing_content_id_identifies_the_partial_document() {
    let mut engine = Engine::in_memory();
    engine.load_cxf(DEFERRED.as_bytes()).expect("fixture loads");
    let exported = engine.export_cxf().expect("survivor cone exports");
    assert!(!exported.warnings.is_empty());
    assert!(exported.content_id().starts_with("cxf:fnv1a128:"));
}

#[test]
fn unloaded_engine_returns_an_error_without_panicking() {
    assert!(Engine::in_memory().export_cxf().is_err());
}
