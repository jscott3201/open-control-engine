//! Facade contract of `@context` expansion: compact and expanded spellings of one document name
//! the same points and mint the same content identity, and a document in the modelica-json
//! emitter dialect loads clean with authored, expanded point paths.

use std::collections::BTreeSet;

use oce_api::{Engine, PointDirection, PointInfo};

/// The canonical content id of `minimal_loop.jsonld`, pinned in `export_cxf.rs` too.
const MINIMAL_LOOP_CONTENT_ID: &str =
    "oce:canon1:9cc5f3e2e97c2af2cda6a67fe223022fdecf53fb0e0bf445889f11bc22db21d3";
const ORIGINAL: &[u8] = include_bytes!("../../oce-cxf/tests/fixtures/minimal_loop.jsonld");
const COMPACT_TWIN: &[u8] =
    include_bytes!("../../oce-cxf/tests/fixtures/minimal_loop_compact.jsonld");
const MODELICA_JSON_STYLE: &[u8] =
    include_bytes!("../../oce-cxf/tests/fixtures/modelica_json_style.jsonld");

fn load(bytes: &[u8]) -> Engine {
    let mut engine = Engine::new();
    let (report, _) = engine.load_cxf(bytes).expect("fixture loads");
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    engine
}

fn point_paths(engine: &Engine, direction: PointDirection) -> BTreeSet<String> {
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
    for engine in [&original, &compact] {
        engine
            .export_cxf()
            .expect("twin exports")
            .complete()
            .expect("twin export is complete");
    }
    let original_id = original.content_id().expect("original has a content id");
    assert_eq!(
        original_id,
        compact.content_id().expect("twin has a content id")
    );
    // The value pinned for minimal_loop (export_cxf.rs): both spellings reach the identical
    // canonical bytes.
    assert_eq!(original_id.as_str(), MINIMAL_LOOP_CONTENT_ID);
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
    // The fixture writes `gai.y`'s unit as the IRI node `unit:K` under `S231:unit`, with `unit`
    // bound to QUDT's unit namespace. The host reads the unit string it stands for (issue #631);
    // before, the row read the verbatim CURIE `unit:K`, and the content id covered that text.
    let rows = engine.point_list(None).expect("point inventory");
    let gai_y = rows
        .iter()
        .find(|point| point.path == format!("{prefix}.gai.y"))
        .expect("gai.y has a row");
    assert_eq!(gai_y.unit.as_deref(), Some("K"));
    assert_eq!(
        engine.content_id().expect("content id").as_str(),
        "oce:canon1:4e4b755dd43d528a524f336b10db58e288a8649c6f10f436d83e28dfbf600f48"
    );
}
