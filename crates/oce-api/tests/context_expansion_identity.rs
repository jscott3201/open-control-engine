//! Facade contract of `@context` expansion: compact and expanded spellings of one document name
//! the same points and mint the same content identity, and a document in the modelica-json
//! emitter dialect loads clean with authored, expanded point paths.

use std::collections::BTreeSet;

use oce_api::oce_store::Store;
use oce_api::{Engine, PointDirection, PointInfo};

const ORIGINAL: &[u8] = include_bytes!("../../oce-cxf/tests/fixtures/minimal_loop.jsonld");
const COMPACT_TWIN: &[u8] =
    include_bytes!("../../oce-cxf/tests/fixtures/minimal_loop_compact.jsonld");
const MODELICA_JSON_STYLE: &[u8] =
    include_bytes!("../../oce-cxf/tests/fixtures/modelica_json_style.jsonld");

fn load(bytes: &[u8]) -> Engine<impl Store> {
    let mut engine = Engine::in_memory();
    let report = engine.load_cxf(bytes).expect("fixture loads");
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    engine
}

fn point_paths<S: Store>(engine: &Engine<S>, direction: PointDirection) -> BTreeSet<String> {
    engine
        .point_list(None)
        .expect("point inventory")
        .into_iter()
        .filter(|point: &PointInfo| point.direction == direction)
        .map(|point| point.path)
        .collect()
}

#[test]
fn compact_and_expanded_twins_name_the_same_points_and_content_identity() {
    let original = load(ORIGINAL);
    let compact = load(COMPACT_TWIN);
    for direction in [PointDirection::In, PointDirection::Out] {
        let original_paths = point_paths(&original, direction);
        assert!(
            !original_paths.is_empty(),
            "twin comparison must not be vacuous"
        );
        assert_eq!(
            original_paths,
            point_paths(&compact, direction),
            "point paths diverge between spellings"
        );
    }
    let original_id = original
        .export_cxf()
        .expect("original exports")
        .content_id_complete()
        .expect("original export is complete");
    let compact_id = compact
        .export_cxf()
        .expect("compact twin exports")
        .content_id_complete()
        .expect("compact twin export is complete");
    assert_eq!(original_id, compact_id);
    // The value already pinned for minimal_loop's export fixpoint (export_cxf.rs): both
    // spellings reach the identical canonical bytes.
    assert_eq!(original_id, "cxf:fnv1a128:de3ae3208d304eb9b8300520e730fed7");
}

#[test]
fn modelica_json_style_document_loads_clean_with_expanded_point_paths() {
    let engine = load(MODELICA_JSON_STYLE);
    let prefix = "http://example.org#Buildings.Controls.OBC.Examples.SupplyGain";
    assert_eq!(
        point_paths(&engine, PointDirection::In),
        BTreeSet::from([format!("{prefix}.TSup")]),
        "boundary input point must carry the expanded authored @id"
    );
    assert_eq!(
        point_paths(&engine, PointDirection::Out),
        BTreeSet::from([format!("{prefix}.gai.y")]),
        "block output point must carry the expanded authored @id"
    );
}
