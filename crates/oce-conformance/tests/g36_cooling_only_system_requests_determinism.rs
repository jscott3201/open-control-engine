//! G36 CoolingOnly SystemRequests Tier-2 determinism golden.
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

const SYSTEM_REQUESTS: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/cooling_only_system_requests.jsonld");

const AFTER_SUPPRESSION: &str =
    "http://example.org#g36.source.cooling_only_system_requests.uAftSup";
const COOLING_SETPOINT: &str = "http://example.org#g36.source.cooling_only_system_requests.TCooSet";
const ZONE_TEMPERATURE: &str = "http://example.org#g36.source.cooling_only_system_requests.TZon";
const COOLING_LOOP: &str = "http://example.org#g36.source.cooling_only_system_requests.uCoo";
const AIRFLOW_SETPOINT: &str =
    "http://example.org#g36.source.cooling_only_system_requests.VSet_flow";
const DISCHARGE_AIRFLOW: &str =
    "http://example.org#g36.source.cooling_only_system_requests.VDis_flow";
const DAMPER_POSITION: &str = "http://example.org#g36.source.cooling_only_system_requests.uDam";

const ZONE_TEMPERATURE_REQUEST_SOURCE: &str =
    "http://example.org#g36.source.cooling_only_system_requests.yZonTemResReq";
const ZONE_PRESSURE_REQUEST_SOURCE: &str =
    "http://example.org#g36.source.cooling_only_system_requests.yZonPreResReq";
const ZONE_TEMPERATURE_REQUEST_RUNTIME: &str = "conn#43";
const ZONE_PRESSURE_REQUEST_RUNTIME: &str = "conn#51";

#[derive(Clone, Copy)]
struct Row {
    after_suppression: bool,
    zone_temperature: f64,
    cooling_loop: f64,
    airflow_setpoint: f64,
    discharge_airflow: f64,
    damper_position: f64,
}

const COOLING_SETPOINT_VALUE: f64 = 295.0;

const fn row(
    after_suppression: bool,
    temperature_difference: f64,
    cooling_loop: f64,
    airflow_setpoint: f64,
    discharge_airflow: f64,
    damper_position: f64,
) -> Row {
    Row {
        after_suppression,
        zone_temperature: COOLING_SETPOINT_VALUE + temperature_difference,
        cooling_loop,
        airflow_setpoint,
        discharge_airflow,
        damper_position,
    }
}

const ROWS: [Row; 31] = [
    row(false, 0.0, 0.0, 0.0, 0.1, 0.0),
    row(true, 3.5, 1.0, 1.0, 0.2, 1.0),
    row(true, 3.5, 1.0, 1.0, 0.2, 1.0),
    row(true, 3.5, 1.0, 1.0, 0.2, 1.0),
    row(true, 2.9, 0.0, 1.0, 0.6, 1.0),
    row(true, 2.7, 0.0, 0.0, 0.8, 1.0),
    row(true, 0.0, 0.0, 0.0, 0.8, 1.0),
    row(true, 2.5, 0.0, 0.006, 0.001, 1.0),
    row(true, 0.0, 0.0, 0.006, 0.001, 1.0),
    row(true, 2.5, 0.0, 0.006, 0.001, 0.945),
    row(true, 2.5, 0.0, 0.0, 0.1, 0.945),
    row(true, 2.5, 0.0, 0.0, 0.1, 0.93),
    row(true, 0.0, 1.0, 0.0, 0.1, 0.93),
    row(true, 0.0, 0.945, 0.0, 0.1, 0.0),
    row(true, 0.0, 0.945, 0.0, 0.1, 0.0),
    row(true, 0.0, 0.93, 0.0, 0.1, 0.0),
    row(true, 0.0, 0.93, 0.0, 0.1, 0.0),
    row(false, 3.5, 1.0, 0.0, 0.1, 0.0),
    row(false, 3.5, 1.0, 0.0, 0.1, 0.0),
    row(false, 3.5, 1.0, 0.0, 0.1, 0.0),
    row(true, 0.0, 0.0, 0.0, 0.1, 0.0),
    row(true, 0.0, 0.0, 1.0, 0.49, 1.0),
    row(true, 0.0, 0.0, 1.0, 0.49, 1.0),
    row(true, 0.0, 0.0, 1.0, 0.505, 1.0),
    row(true, 0.0, 0.0, 1.0, 0.505, 1.0),
    row(true, 0.0, 0.0, 1.0, 0.515, 1.0),
    row(true, 0.0, 0.0, 1.0, 0.515, 1.0),
    row(true, 0.0, 0.0, 1.0, 0.705, 1.0),
    row(true, 0.0, 0.0, 1.0, 0.705, 1.0),
    row(true, 0.0, 0.0, 1.0, 0.715, 1.0),
    row(true, 0.0, 0.0, 1.0, 0.715, 1.0),
];

const INPUTS: &[PointSpec] = &[
    PointSpec::boolean(AFTER_SUPPRESSION),
    PointSpec::real(COOLING_SETPOINT),
    PointSpec::real(ZONE_TEMPERATURE),
    PointSpec::real(COOLING_LOOP),
    PointSpec::real(AIRFLOW_SETPOINT),
    PointSpec::real(DISCHARGE_AIRFLOW),
    PointSpec::real(DAMPER_POSITION),
];
const OUTPUTS: &[PointSpec] = &[
    PointSpec::integer_alias(
        ZONE_TEMPERATURE_REQUEST_SOURCE,
        ZONE_TEMPERATURE_REQUEST_RUNTIME,
    ),
    PointSpec::integer_alias(ZONE_PRESSURE_REQUEST_SOURCE, ZONE_PRESSURE_REQUEST_RUNTIME),
];
const SPEC: SequenceSpec = SequenceSpec {
    name: "cooling_only_system_requests",
    cxf: SYSTEM_REQUESTS,
    t_stop: 30,
    sample_step: 60.0,
    inputs: INPUTS,
    outputs: OUTPUTS,
    input_fn: system_requests_inputs,
};

#[test]
fn cooling_only_system_requests_outputs_match_determinism_golden() {
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

fn system_requests_inputs(t: f64) -> Vec<(String, Value)> {
    let row = (t / 60.0).round() as usize;
    assert!(row < ROWS.len(), "unexpected input time {t}");
    let input = ROWS[row];
    vec![
        pair(AFTER_SUPPRESSION, Value::Boolean(input.after_suppression)),
        pair(COOLING_SETPOINT, Value::Real(COOLING_SETPOINT_VALUE)),
        pair(ZONE_TEMPERATURE, Value::Real(input.zone_temperature)),
        pair(COOLING_LOOP, Value::Real(input.cooling_loop)),
        pair(AIRFLOW_SETPOINT, Value::Real(input.airflow_setpoint)),
        pair(DISCHARGE_AIRFLOW, Value::Real(input.discharge_airflow)),
        pair(DAMPER_POSITION, Value::Real(input.damper_position)),
    ]
}
