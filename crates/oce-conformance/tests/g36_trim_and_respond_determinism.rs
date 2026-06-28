//! G36 Generic.TrimAndRespond `have_hol=false` Tier-2 determinism golden.
//!
//! This fixture is an engine self-output snapshot, not an independent correctness oracle.

use oce_api::Value;
use oce_conformance::drive_trace_with_options;

#[allow(dead_code)]
#[path = "g36_determinism/support.rs"]
mod support;

use support::{
    PointSpec, SequenceSpec, assert_exact_comparisons_pass, assert_output_table_shape,
    assert_provenance_matches_outputs, bless_enabled, bless_sequence, captured_output_table,
    config_for, driver_reference_from_output_golden, options_for, pair, read_output_golden,
};

const TRIM_AND_RESPOND: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/trim_and_respond_have_hol_false.jsonld");

const REQUEST_COUNT: &str =
    "http://example.org#g36.source.trim_and_respond_have_hol_false.numOfReq";
const DEVICE_STATUS: &str = "http://example.org#g36.source.trim_and_respond_have_hol_false.uDevSta";
const SETPOINT_SOURCE: &str = "http://example.org#g36.source.trim_and_respond_have_hol_false.y";
const SETPOINT_RUNTIME: &str = "conn#23";

const INPUTS: &[PointSpec] = &[
    PointSpec::integer(REQUEST_COUNT),
    PointSpec::boolean(DEVICE_STATUS),
];
const OUTPUTS: &[PointSpec] = &[PointSpec::real_alias(SETPOINT_SOURCE, SETPOINT_RUNTIME)];
const SPEC: SequenceSpec = SequenceSpec {
    name: "trim_and_respond_have_hol_false",
    cxf: TRIM_AND_RESPOND,
    t_stop: 22,
    sample_step: 60.0,
    inputs: INPUTS,
    outputs: OUTPUTS,
    input_fn: trim_and_respond_inputs,
};

#[test]
fn g36_trim_and_respond_outputs_match_determinism_golden() {
    if bless_enabled() {
        bless_sequence(&SPEC);
    }

    let golden = read_output_golden(&SPEC);
    assert_provenance_matches_outputs(&SPEC, &golden);
    let reference = driver_reference_from_output_golden(&SPEC, &golden);
    let run = drive_trace_with_options(
        SPEC.cxf.as_bytes(),
        &config_for(&SPEC),
        &reference,
        &options_for(&SPEC),
    )
    .unwrap_or_else(|err| panic!("{} driver run failed: {err}", SPEC.name));

    assert_output_table_shape(&SPEC, &golden);
    assert_eq!(
        captured_output_table(&SPEC, &run),
        golden,
        "{} captured table drifted from committed golden",
        SPEC.name
    );
    assert_exact_comparisons_pass(&SPEC, golden.n_rows, &run.comparisons);
}

fn trim_and_respond_inputs(t: f64) -> Vec<(String, Value)> {
    let requests = if (840.0..1080.0).contains(&t) {
        6
    } else if (720.0..840.0).contains(&t) {
        3
    } else {
        0
    };
    let device_status = !(1080.0..1260.0).contains(&t);
    vec![
        pair(REQUEST_COUNT, Value::Integer(requests)),
        pair(DEVICE_STATUS, Value::Boolean(device_status)),
    ]
}
