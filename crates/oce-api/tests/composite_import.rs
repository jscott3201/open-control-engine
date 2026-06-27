//! Full-pipeline smoke tests for nested CXF composite import.

use oce_api::{CollectSpec, Engine, InputSource, PointDirection, SimSpec, Value};
use oce_store::ModelStore;

const FANOUT: &str = include_str!("../../oce-cxf/tests/fixtures/boundary_fanout.jsonld");
const NESTED: &str = include_str!("../../oce-cxf/tests/fixtures/nested_composite.jsonld");
const G36_TRIM_AND_RESPOND: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/trim_and_respond_have_hol_false.jsonld");
const FANOUT_INPUT: &str = "http://example.org#g36.profile.boundary_fanout.u";
const INPUT: &str = "http://example.org#g36.profile.nested_composite.u";
const G36_TRIM_MODEL: &str = "http://example.org#g36.source.trim_and_respond_have_hol_false";

#[test]
fn nested_composite_loads_builds_and_ticks_through_frozen_facade() {
    let mut engine = Engine::in_memory();
    let report = engine
        .load_cxf(NESTED.as_bytes())
        .expect("nested CXF composite loads");
    assert_eq!(report.block_count, 3);
    assert_eq!(report.stateful_blocks, 0);
    assert!(
        report.warnings.is_empty(),
        "fixture should not warn: {:?}",
        report.warnings
    );
    let output = engine
        .io()
        .iter()
        .filter(|point| point.direction == PointDirection::Out)
        .last()
        .expect("post gain output is visible")
        .path
        .clone();

    let metrics = engine
        .simulate(&SimSpec {
            t_start: 0.0,
            t_stop: 2.0,
            step: 1.0,
            inputs: InputSource::Closure(Box::new(|_| vec![(INPUT.to_owned(), Value::Real(8.0))])),
            collect: CollectSpec::Named {
                points: vec![output.clone()],
                stride: 1,
            },
        })
        .expect("nested CXF composite simulates");

    assert_eq!(metrics.trace.columns(), &[output]);
    for (row, value) in metrics
        .trace
        .column(0)
        .expect("output column")
        .iter()
        .enumerate()
    {
        assert!(
            value.bit_eq(&Value::Real(8.0)),
            "row {row} should carry top input through 0.5 then 2.0 gains, got {value:?}"
        );
    }
}

#[test]
fn boundary_input_fanout_loads_as_one_point_and_stages_every_target() {
    let mut engine = Engine::in_memory();
    let report = engine
        .load_cxf(FANOUT.as_bytes())
        .expect("boundary fanout CXF composite loads");
    assert_eq!(report.block_count, 3);
    assert_eq!(report.stateful_blocks, 0);
    assert!(
        report.warnings.is_empty(),
        "fixture should not warn: {:?}",
        report.warnings
    );

    let inventory_paths: Vec<String> = engine.io().iter().map(|point| point.path).collect();
    assert_eq!(
        inventory_paths
            .iter()
            .filter(|path| path.as_str() == FANOUT_INPUT)
            .count(),
        1,
        "fanout input should be exposed once as a logical host point"
    );
    let resolved = engine
        .store()
        .load_model(&report.model_id)
        .expect("load stores resolved model");
    assert_eq!(
        resolved
            .points
            .iter()
            .filter(|point| point.key.as_str() == FANOUT_INPUT)
            .count(),
        1,
        "durable point projection should also coalesce the fanout key"
    );
    let output = engine
        .io()
        .iter()
        .filter(|point| point.direction == PointDirection::Out)
        .last()
        .expect("fanout output is visible")
        .path
        .clone();

    engine
        .set_input(FANOUT_INPUT, Value::Real(4.0))
        .expect("fanout input stages");
    engine.tick(0.0).expect("fanout model ticks");
    assert!(
        engine
            .get_output(&output)
            .expect("fanout output exists")
            .bit_eq(&Value::Real(20.0)),
        "fanout output should compute 2*u + 3*u"
    );
}

#[test]
fn source_verified_g36_trim_and_respond_fixture_loads_through_frozen_facade() {
    let mut engine = Engine::in_memory();
    let report = engine
        .load_cxf(G36_TRIM_AND_RESPOND.as_bytes())
        .expect("source-verified G36 TrimAndRespond CXF fixture loads");
    assert_eq!(report.block_count, 44);
    assert_eq!(report.stateful_blocks, 5);
    assert!(
        report.warnings.is_empty(),
        "fixture should not warn: {:?}",
        report.warnings
    );

    let paths: Vec<String> = engine.io().iter().map(|point| point.path).collect();
    let num_of_req = format!("{G36_TRIM_MODEL}.numOfReq");
    let device_status = format!("{G36_TRIM_MODEL}.uDevSta");
    let hold = format!("{G36_TRIM_MODEL}.uHol");
    assert!(paths.contains(&num_of_req));
    assert!(paths.contains(&device_status));
    assert!(
        !paths.contains(&hold),
        "inactive have_hol=false optional input should not survive facade IO export"
    );
}
