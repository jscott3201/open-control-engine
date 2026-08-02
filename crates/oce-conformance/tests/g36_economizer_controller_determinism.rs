//! G36 Economizers.Controller restricted variant Tier-2 determinism golden.
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

const ECONOMIZER_CONTROLLER: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21.jsonld"
);

const OUTDOOR_AIRFLOW_NORMALIZED: &str = "http://example.org#g36.source.multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21.VOut_flow_normalized";
const MINIMUM_OUTDOOR_AIRFLOW_SETPOINT_NORMALIZED: &str = "http://example.org#g36.source.multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21.VOutMinSet_flow_normalized";
const SUPPLY_TEMPERATURE_SIGNAL: &str = "http://example.org#g36.source.multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21.uTSup";
const OUTDOOR_AIR_TEMPERATURE: &str = "http://example.org#g36.source.multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21.TOut";
const SUPPLY_FAN_STATUS: &str = "http://example.org#g36.source.multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21.u1SupFan";
const OPERATION_MODE: &str = "http://example.org#g36.source.multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21.uOpeMod";
const FREEZE_PROTECTION_STAGE: &str = "http://example.org#g36.source.multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21.uFreProSta";

const OUTDOOR_DAMPER_MIN_LIMIT_SOURCE: &str = "http://example.org#g36.source.multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21.yOutDam_min";
const MINIMUM_OUTDOOR_AIR_LOOP_ENABLED_SOURCE: &str = "http://example.org#g36.source.multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21.yEnaMinOut";
const OUTDOOR_DAMPER_COMMAND_SOURCE: &str = "http://example.org#g36.source.multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21.yOutDam";
const RETURN_DAMPER_COMMAND_SOURCE: &str = "http://example.org#g36.source.multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21.yRetDam";

const OUTDOOR_DAMPER_MIN_LIMIT_PATH: &str = "http://example.org#g36.source.multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21.damLim.minOutDam.y";
const MINIMUM_OUTDOOR_AIR_LOOP_ENABLED_PATH: &str = "http://example.org#g36.source.multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21.damLim.and3.y";
const OUTDOOR_DAMPER_COMMAND_PATH: &str = "http://example.org#g36.source.multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21.modRel.min.y";
const RETURN_DAMPER_COMMAND_PATH: &str = "http://example.org#g36.source.multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21.modRel.max.y";

const OUTDOOR_AIRFLOW_NORMALIZED_VALUES: [f64; 24] = [0.0; 24];
const MINIMUM_OUTDOOR_AIRFLOW_SETPOINT_NORMALIZED_VALUES: [f64; 24] = [
    0.2, 0.2, 0.2, 0.2, 0.2, 0.2, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.2, 0.2, 0.2, 0.2,
    0.2, 0.2, 0.2, 0.2, 0.2,
];
const SUPPLY_TEMPERATURE_SIGNAL_VALUES: [f64; 24] = [
    -0.5, -0.25, -0.125, 0.0, 0.125, 0.25, 0.5, -0.5, -0.25, -0.125, 0.0, 0.125, 0.25, 0.5, -0.5,
    -0.25, -0.125, 0.0, 0.125, 0.25, 0.5, -0.5, -0.25, -0.125,
];
const OUTDOOR_AIR_TEMPERATURE_VALUES: [f64; 24] = [
    293.0, 295.0, 295.0, 295.0, 295.0, 295.0, 295.0, 295.0, 295.0, 295.0, 295.0, 295.0, 295.0,
    295.0, 295.0, 295.0, 293.0, 293.0, 293.0, 293.0, 293.0, 293.0, 293.0, 293.0,
];
const SUPPLY_FAN_STATUS_VALUES: [bool; 24] = [
    true, true, true, true, true, true, true, true, true, true, true, false, false, false, false,
    true, true, true, true, true, true, true, true, true,
];
const OPERATION_MODE_VALUES: [i64; 24] = [
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 1, 1,
];
const FREEZE_PROTECTION_STAGE_VALUES: [i64; 24] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0,
];

const INPUTS: &[PointSpec] = &[
    PointSpec::real(OUTDOOR_AIRFLOW_NORMALIZED),
    PointSpec::real(MINIMUM_OUTDOOR_AIRFLOW_SETPOINT_NORMALIZED),
    PointSpec::real(SUPPLY_TEMPERATURE_SIGNAL),
    PointSpec::real(OUTDOOR_AIR_TEMPERATURE),
    PointSpec::boolean(SUPPLY_FAN_STATUS),
    PointSpec::integer(OPERATION_MODE),
    PointSpec::integer(FREEZE_PROTECTION_STAGE),
];
const OUTPUTS: &[PointSpec] = &[
    PointSpec::real_alias(
        OUTDOOR_DAMPER_MIN_LIMIT_SOURCE,
        OUTDOOR_DAMPER_MIN_LIMIT_PATH,
    ),
    PointSpec::boolean_alias(
        MINIMUM_OUTDOOR_AIR_LOOP_ENABLED_SOURCE,
        MINIMUM_OUTDOOR_AIR_LOOP_ENABLED_PATH,
    ),
    PointSpec::real_alias(OUTDOOR_DAMPER_COMMAND_SOURCE, OUTDOOR_DAMPER_COMMAND_PATH),
    PointSpec::real_alias(RETURN_DAMPER_COMMAND_SOURCE, RETURN_DAMPER_COMMAND_PATH),
];
const SPEC: SequenceSpec = SequenceSpec {
    name: "multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21",
    cxf: ECONOMIZER_CONTROLLER,
    t_stop: 23,
    sample_step: 60.0,
    inputs: INPUTS,
    outputs: OUTPUTS,
    input_fn: economizer_controller_inputs,
};

#[test]
fn g36_economizer_controller_outputs_match_determinism_golden() {
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

fn economizer_controller_inputs(t: f64) -> Vec<(String, Value)> {
    let row = (t / 60.0) as usize;
    vec![
        pair(
            OUTDOOR_AIRFLOW_NORMALIZED,
            Value::Real(OUTDOOR_AIRFLOW_NORMALIZED_VALUES[row]),
        ),
        pair(
            MINIMUM_OUTDOOR_AIRFLOW_SETPOINT_NORMALIZED,
            Value::Real(MINIMUM_OUTDOOR_AIRFLOW_SETPOINT_NORMALIZED_VALUES[row]),
        ),
        pair(
            SUPPLY_TEMPERATURE_SIGNAL,
            Value::Real(SUPPLY_TEMPERATURE_SIGNAL_VALUES[row]),
        ),
        pair(
            OUTDOOR_AIR_TEMPERATURE,
            Value::Real(OUTDOOR_AIR_TEMPERATURE_VALUES[row]),
        ),
        pair(
            SUPPLY_FAN_STATUS,
            Value::Boolean(SUPPLY_FAN_STATUS_VALUES[row]),
        ),
        pair(OPERATION_MODE, Value::Integer(OPERATION_MODE_VALUES[row])),
        pair(
            FREEZE_PROTECTION_STAGE,
            Value::Integer(FREEZE_PROTECTION_STAGE_VALUES[row]),
        ),
    ]
}
