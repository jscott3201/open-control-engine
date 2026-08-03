//! G36 CoolingOnly Dampers Tier-2 determinism golden.
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

const COOLING_ONLY_DAMPERS: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/cooling_only_dampers.jsonld");

const ACTIVE_MINIMUM_AIRFLOW: &str =
    "http://example.org#g36.source.cooling_only_dampers.VActMin_flow";
const SUPPLY_AIR_TEMPERATURE: &str = "http://example.org#g36.source.cooling_only_dampers.TSup";
const ZONE_TEMPERATURE: &str = "http://example.org#g36.source.cooling_only_dampers.TZon";
const COOLING_LOOP: &str = "http://example.org#g36.source.cooling_only_dampers.uCoo";
const ACTIVE_COOLING_MAXIMUM_AIRFLOW: &str =
    "http://example.org#g36.source.cooling_only_dampers.VActCooMax_flow";
const ZONE_STATE: &str = "http://example.org#g36.source.cooling_only_dampers.uZonSta";
const AIRFLOW_OVERRIDE_INDEX: &str = "http://example.org#g36.source.cooling_only_dampers.oveFloSet";
const SUPPLY_FAN_STATUS: &str = "http://example.org#g36.source.cooling_only_dampers.u1Fan";
const DISCHARGE_AIRFLOW: &str = "http://example.org#g36.source.cooling_only_dampers.VDis_flow";
const DAMPER_OVERRIDE_INDEX: &str = "http://example.org#g36.source.cooling_only_dampers.oveDamPos";

const AIRFLOW_SETPOINT_SOURCE: &str =
    "http://example.org#g36.source.cooling_only_dampers.VSet_flow";
const DAMPER_COMMAND_SOURCE: &str = "http://example.org#g36.source.cooling_only_dampers.yDam";

#[derive(Clone, Copy)]
struct Row {
    supply_air_temperature: f64,
    cooling_loop: f64,
    zone_state: i64,
    airflow_override_index: i64,
    supply_fan_status: bool,
    discharge_airflow: f64,
    damper_override_index: i64,
}

const fn row(
    supply_air_temperature: f64,
    cooling_loop: f64,
    zone_state: i64,
    airflow_override_index: i64,
    supply_fan_status: bool,
    discharge_airflow: f64,
    damper_override_index: i64,
) -> Row {
    Row {
        supply_air_temperature,
        cooling_loop,
        zone_state,
        airflow_override_index,
        supply_fan_status,
        discharge_airflow,
        damper_override_index,
    }
}

const ROWS: [Row; 34] = [
    row(290.0, 0.0, 2, 0, false, 0.012, 0),
    row(290.0, 0.0, 2, 0, true, 0.012, 0),
    row(290.0, 0.0, 2, 0, true, 0.012, 0),
    row(290.0, 1.0, 2, 0, true, 0.012, 0),
    row(290.0, 1.0, 1, 0, true, 0.012, 0),
    row(290.0, 0.0, 3, 0, true, 0.012, 0),
    row(290.0, 0.5, 3, 0, true, 0.030, 0),
    row(296.0, 0.5, 3, 0, true, 0.030, 0),
    row(294.9, 0.5, 3, 0, true, 0.030, 0),
    row(294.7, 0.5, 3, 0, true, 0.030, 0),
    row(290.0, 0.5, 3, 1, true, 0.020, 0),
    row(290.0, 0.5, 3, 2, true, 0.020, 0),
    row(290.0, 0.5, 3, 3, true, 0.020, 0),
    row(290.0, 0.75, 3, 4, true, 0.020, 0),
    row(290.0, 0.25, 3, -1, true, 0.020, 0),
    row(290.0, 0.6, 3, 0, true, 0.020, 3),
    row(290.0, 0.6, 3, 2, true, 0.010, 0),
    row(290.0, 0.6, 3, 1, true, 0.060, 0),
    row(290.0, 1.0, 3, 0, true, 0.000, 2),
    row(290.0, 1.0, 3, 0, true, 0.000, 1),
    row(290.0, 1.0, 3, 0, true, 0.000, 0),
    row(290.0, 0.0, 3, 0, false, 0.000, 0),
    row(290.0, 0.0, 3, 0, false, 0.090, 0),
    row(290.0, 0.0, 3, 0, true, 0.090, 0),
    row(290.0, 0.0, 3, 0, true, 0.090, 0),
    row(290.0, 0.4, 3, 2, true, 0.000, 0),
    row(290.0, 0.4, 3, 2, true, 0.000, 0),
    row(290.0, 0.4, 3, 2, true, 0.000, 0),
    row(290.0, 0.4, 3, 2, true, 0.000, 0),
    row(290.0, 0.0, 3, 0, true, 0.090, 0),
    row(290.0, 0.0, 3, 0, true, 0.090, 0),
    row(290.0, 0.5, 3, 0, true, 0.040, -1),
    row(296.0, 1.0, 3, 0, true, 0.020, 0),
    row(294.7, 1.0, 3, 0, true, 0.020, 0),
];

const INPUTS: &[PointSpec] = &[
    PointSpec::real(ACTIVE_MINIMUM_AIRFLOW),
    PointSpec::real(SUPPLY_AIR_TEMPERATURE),
    PointSpec::real(ZONE_TEMPERATURE),
    PointSpec::real(COOLING_LOOP),
    PointSpec::real(ACTIVE_COOLING_MAXIMUM_AIRFLOW),
    PointSpec::integer(ZONE_STATE),
    PointSpec::integer(AIRFLOW_OVERRIDE_INDEX),
    PointSpec::boolean(SUPPLY_FAN_STATUS),
    PointSpec::real(DISCHARGE_AIRFLOW),
    PointSpec::integer(DAMPER_OVERRIDE_INDEX),
];
const OUTPUTS: &[PointSpec] = &[
    PointSpec::real(AIRFLOW_SETPOINT_SOURCE),
    PointSpec::real(DAMPER_COMMAND_SOURCE),
];
const SPEC: SequenceSpec = SequenceSpec {
    name: "cooling_only_dampers",
    cxf: COOLING_ONLY_DAMPERS,
    t_stop: 33,
    sample_step: 60.0,
    inputs: INPUTS,
    outputs: OUTPUTS,
    input_fn: cooling_only_dampers_inputs,
};

#[test]
fn cooling_only_dampers_outputs_match_determinism_golden() {
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

fn cooling_only_dampers_inputs(t: f64) -> Vec<(String, Value)> {
    let row = (t / 60.0).round() as usize;
    assert!(row < ROWS.len(), "unexpected input time {t}");
    let input = ROWS[row];
    vec![
        pair(ACTIVE_MINIMUM_AIRFLOW, Value::Real(0.012)),
        pair(
            SUPPLY_AIR_TEMPERATURE,
            Value::Real(input.supply_air_temperature),
        ),
        pair(ZONE_TEMPERATURE, Value::Real(295.0)),
        pair(COOLING_LOOP, Value::Real(input.cooling_loop)),
        pair(ACTIVE_COOLING_MAXIMUM_AIRFLOW, Value::Real(0.075)),
        pair(ZONE_STATE, Value::Integer(input.zone_state)),
        pair(
            AIRFLOW_OVERRIDE_INDEX,
            Value::Integer(input.airflow_override_index),
        ),
        pair(SUPPLY_FAN_STATUS, Value::Boolean(input.supply_fan_status)),
        pair(DISCHARGE_AIRFLOW, Value::Real(input.discharge_airflow)),
        pair(
            DAMPER_OVERRIDE_INDEX,
            Value::Integer(input.damper_override_index),
        ),
    ]
}
