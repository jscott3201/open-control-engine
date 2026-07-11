//! G36 CoolingOnly Alarms Tier-2 determinism golden.
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

const ALARMS: &str = include_str!("../../oce-cxf/tests/fixtures/g36/cooling_only_alarms.jsonld");

const DISCHARGE_AIRFLOW: &str = "http://example.org#g36.source.cooling_only_alarms.VDis_flow";
const ACTIVE_AIRFLOW_SETPOINT: &str =
    "http://example.org#g36.source.cooling_only_alarms.VActSet_flow";
const SUPPLY_FAN_STATUS: &str = "http://example.org#g36.source.cooling_only_alarms.u1Fan";
const OPERATION_MODE: &str = "http://example.org#g36.source.cooling_only_alarms.uOpeMod";
const DAMPER_POSITION: &str = "http://example.org#g36.source.cooling_only_alarms.uDam";

const LOW_AIRFLOW_ALARM_SOURCE: &str =
    "http://example.org#g36.source.cooling_only_alarms.yLowFloAla";
const AIRFLOW_SENSOR_ALARM_SOURCE: &str =
    "http://example.org#g36.source.cooling_only_alarms.yFloSenAla";
const LEAKING_DAMPER_ALARM_SOURCE: &str =
    "http://example.org#g36.source.cooling_only_alarms.yLeaDamAla";
const LOW_AIRFLOW_ALARM_RUNTIME: &str = "conn#36";
const AIRFLOW_SENSOR_ALARM_RUNTIME: &str = "conn#66";
const LEAKING_DAMPER_ALARM_RUNTIME: &str = "conn#81";

#[derive(Clone, Copy)]
struct Row {
    discharge_airflow: f64,
    active_airflow_setpoint: f64,
    supply_fan_status: bool,
    operation_mode: i64,
    damper_position: f64,
}

const fn row(
    discharge_airflow: f64,
    active_airflow_setpoint: f64,
    supply_fan_status: bool,
    operation_mode: i64,
    damper_position: f64,
) -> Row {
    Row {
        discharge_airflow,
        active_airflow_setpoint,
        supply_fan_status,
        operation_mode,
        damper_position,
    }
}

const ROWS: [Row; 58] = [
    row(0.0, 0.0, false, 0, 1.0),
    row(0.1, 0.02, true, 0, 0.0),
    row(0.1, 0.02, true, 0, 0.0),
    row(0.1, 0.0, true, 0, 0.0),
    row(0.1, 0.0, true, 0, 0.0),
    row(0.1, 0.0, true, 0, 0.0),
    row(0.1, 0.0, true, 0, 0.0),
    row(0.1, 0.0, true, 0, 0.0),
    row(0.1, 0.0, true, 0, 0.0),
    row(0.1, 0.0, true, 0, 0.0),
    row(0.1, 0.0, true, 0, 0.0),
    row(0.1, 0.0, true, 0, 0.0),
    row(0.1, 0.0, true, 0, 0.012),
    row(0.1, 0.0, true, 0, 0.016),
    row(0.1, 0.0, true, 0, 0.0),
    row(0.045, 0.0, true, 0, 0.0),
    row(0.039, 0.0, true, 0, 0.0),
    row(0.1, 0.0, true, 0, 1.0),
    row(0.1, 0.0, true, 0, 1.0),
    row(0.1, 0.0, true, 0, 1.0),
    row(0.1, 0.0, true, 0, 1.0),
    row(0.1, 0.0, true, 0, 1.0),
    row(0.1, 0.0, true, 0, 1.0),
    row(0.1, 0.0, true, 0, 1.0),
    row(0.1, 0.0, true, 0, 1.0),
    row(0.1, 0.0, true, 0, 1.0),
    row(0.1, 0.0, true, 0, 1.0),
    row(0.1, 0.0, true, 0, 1.0),
    row(0.1, 0.0, true, 0, 1.0),
    row(0.1, 0.0, true, 0, 1.0),
    row(0.6, 0.0, true, 0, 1.0),
    row(0.6, 1.0, true, 1, 1.0),
    row(0.6, 1.0, true, 1, 1.0),
    row(0.6, 1.0, true, 1, 1.0),
    row(0.6, 1.0, true, 1, 1.0),
    row(0.6, 1.0, true, 1, 1.0),
    row(0.6, 1.0, true, 1, 1.0),
    row(0.4, 1.0, true, 0, 1.0),
    row(0.505, 1.0, true, 2, 1.0),
    row(0.505, 1.0, true, 1, 1.0),
    row(0.505, 1.0, true, 1, 1.0),
    row(0.505, 1.0, true, 1, 1.0),
    row(0.505, 1.0, true, 1, 1.0),
    row(0.515, 1.0, true, 1, 1.0),
    row(0.705, 1.0, true, 1, 1.0),
    row(0.715, 1.0, false, 1, 1.0),
    row(0.1, 0.007, false, 1, 1.0),
    row(0.1, 0.004, false, 1, 1.0),
    row(0.1, 0.0, false, 1, 1.0),
    row(0.1, 0.0, false, 1, 1.0),
    row(0.1, 0.0, false, 1, 1.0),
    row(0.1, 0.0, false, 1, 1.0),
    row(0.1, 0.0, false, 1, 1.0),
    row(0.1, 0.0, false, 1, 1.0),
    row(0.1, 0.0, false, 1, 1.0),
    row(0.1, 0.0, false, 1, 1.0),
    row(0.1, 0.0, true, 1, 1.0),
    row(0.1, 0.0, false, 1, 1.0),
];

const INPUTS: &[PointSpec] = &[
    PointSpec::boolean(SUPPLY_FAN_STATUS),
    PointSpec::integer(OPERATION_MODE),
    PointSpec::real(DISCHARGE_AIRFLOW),
    PointSpec::real(ACTIVE_AIRFLOW_SETPOINT),
    PointSpec::real(DAMPER_POSITION),
];
const OUTPUTS: &[PointSpec] = &[
    PointSpec::integer_alias(LOW_AIRFLOW_ALARM_SOURCE, LOW_AIRFLOW_ALARM_RUNTIME),
    PointSpec::integer_alias(AIRFLOW_SENSOR_ALARM_SOURCE, AIRFLOW_SENSOR_ALARM_RUNTIME),
    PointSpec::integer_alias(LEAKING_DAMPER_ALARM_SOURCE, LEAKING_DAMPER_ALARM_RUNTIME),
];
const SPEC: SequenceSpec = SequenceSpec {
    name: "cooling_only_alarms",
    cxf: ALARMS,
    t_stop: 57,
    sample_step: 60.0,
    inputs: INPUTS,
    outputs: OUTPUTS,
    input_fn: alarms_inputs,
};

#[test]
fn cooling_only_alarms_outputs_match_determinism_golden() {
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

fn alarms_inputs(t: f64) -> Vec<(String, Value)> {
    let row = (t / 60.0).round() as usize;
    assert!(row < ROWS.len(), "unexpected input time {t}");
    let input = ROWS[row];
    vec![
        pair(SUPPLY_FAN_STATUS, Value::Boolean(input.supply_fan_status)),
        pair(OPERATION_MODE, Value::Integer(input.operation_mode)),
        pair(DISCHARGE_AIRFLOW, Value::Real(input.discharge_airflow)),
        pair(
            ACTIVE_AIRFLOW_SETPOINT,
            Value::Real(input.active_airflow_setpoint),
        ),
        pair(DAMPER_POSITION, Value::Real(input.damper_position)),
    ]
}
