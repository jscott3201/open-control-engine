//! Facade CXF export, fixpoint, warning, and content-identity contracts.

use oce_api::{ContentIdError, Engine, Value};
use oce_diag::DiagCode;

const MINIMAL: &str = include_str!("../../oce-cxf/tests/fixtures/minimal_loop.jsonld");
const PASS_THROUGH: &str =
    include_str!("../../oce-cxf/tests/fixtures/pass_through_miniature.jsonld");
const DEFERRED: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/multizone_vav_return_fan_airflow_tracking.jsonld"
);

fn independent_content_id(bytes: &[u8]) -> String {
    const OFFSET: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    const PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;
    let mut hash = OFFSET;
    for byte in bytes {
        hash ^= u128::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    format!("cxf:fnv1a128:{hash:032x}")
}

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
    assert_eq!(
        first_export.content_id_complete(),
        second_export.content_id_complete()
    );
    first_export
        .content_id_complete()
        .expect("fixpoint export is complete")
}

#[test]
fn deterministic_export_reaches_fixpoint_for_regular_and_pass_through_models() {
    let id = assert_fixpoint(MINIMAL.as_bytes());
    assert_eq!(id, "cxf:fnv1a128:d9e52c9615c014596ecb65661e61447d");
    assert_fixpoint(PASS_THROUGH.as_bytes());
}

#[test]
fn content_id_hashes_exact_export_bytes_without_a_prefix_or_length_input() {
    let mut engine = Engine::in_memory();
    engine.load_cxf(MINIMAL.as_bytes()).expect("fixture loads");
    let exported = engine.export_cxf().expect("fixture exports");
    assert_eq!(
        exported
            .content_id_complete()
            .expect("minimal export is complete"),
        independent_content_id(&exported.bytes)
    );
}

#[test]
fn content_id_tracks_exported_synthetic_document_while_model_id_stays_authored() {
    let mut engine = Engine::in_memory();
    let loaded = engine.load_cxf(MINIMAL.as_bytes()).expect("fixture loads");
    assert_eq!(loaded.model_id.as_str(), "http://example.org#MinLoop");
    let before = engine.export_cxf().expect("fixture exports");
    let rendered = std::str::from_utf8(&before.bytes).expect("export is UTF-8 JSON-LD");
    assert!(rendered.contains(r#""@id":"urn:open-control:cxf-export:root""#));
    assert!(!rendered.contains(r#""@id":"http://example.org#MinLoop""#));

    engine.halt().expect("loaded engine halts");
    engine
        .set_param("http://example.org#MinLoop.con.k", Value::Real(3.0))
        .expect("constant parameter is legally tunable at rest");
    engine.resume().expect("edited engine resumes");
    let after = engine.export_cxf().expect("edited fixture exports");
    assert_ne!(
        before
            .content_id_complete()
            .expect("unedited export is complete"),
        after
            .content_id_complete()
            .expect("edited export is complete")
    );
    assert_eq!(loaded.model_id.as_str(), "http://example.org#MinLoop");
}

#[test]
fn warning_bearing_content_id_identifies_the_partial_document() {
    let mut engine = Engine::in_memory();
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
        exported.content_id_complete(),
        Err(ContentIdError::Incomplete { warning_count, .. })
            if warning_count == expected_warning_count
    ));

    // This test deliberately pins the deprecated compatibility path's unchanged behavior.
    #[allow(deprecated)]
    let legacy_id = exported.content_id();
    assert_eq!(legacy_id, independent_content_id(&exported.bytes));
}

#[test]
fn unloaded_engine_returns_an_error_without_panicking() {
    assert!(Engine::in_memory().export_cxf().is_err());
}
