//! G36 Economizers.Subsequences.Limits.Common Tier-2 determinism golden.
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

const ECONOMIZER_LIMITS_COMMON: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/multizone_vav_economizer_limits_common.jsonld");

const OUTDOOR_AIRFLOW_NORMALIZED: &str =
    "http://example.org#g36.source.multizone_vav_economizer_limits_common.VOut_flow_normalized";
const MINIMUM_OUTDOOR_AIRFLOW_SETPOINT_NORMALIZED: &str = "http://example.org#g36.source.multizone_vav_economizer_limits_common.VOutMinSet_flow_normalized";
const OPERATION_MODE: &str =
    "http://example.org#g36.source.multizone_vav_economizer_limits_common.uOpeMod";
const SUPPLY_FAN_STATUS: &str =
    "http://example.org#g36.source.multizone_vav_economizer_limits_common.u1SupFan";

const OUTDOOR_DAMPER_MIN_LIMIT_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_economizer_limits_common.yOutDam_min";
const OUTDOOR_DAMPER_MAX_LIMIT_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_economizer_limits_common.yOutDam_max";
const RETURN_DAMPER_MIN_LIMIT_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_economizer_limits_common.yRetDam_min";
const RETURN_DAMPER_MAX_LIMIT_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_economizer_limits_common.yRetDam_max";
const RETURN_DAMPER_PHYSICAL_MAX_LIMIT_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_economizer_limits_common.yRetDamPhy_max";
const MINIMUM_OUTDOOR_AIR_LOOP_ENABLED_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_economizer_limits_common.yEnaMinOut";

const OUTDOOR_DAMPER_MIN_LIMIT_PATH: &str =
    "http://example.org#g36.source.multizone_vav_economizer_limits_common.minOutDam.y";
const OUTDOOR_DAMPER_MAX_LIMIT_PATH: &str =
    "http://example.org#g36.source.multizone_vav_economizer_limits_common.outDamPosMaxSwitch.y";
const RETURN_DAMPER_MIN_LIMIT_PATH: &str =
    "http://example.org#g36.source.multizone_vav_economizer_limits_common.retDamPosMinSwitch.y";
const RETURN_DAMPER_MAX_LIMIT_PATH: &str =
    "http://example.org#g36.source.multizone_vav_economizer_limits_common.minRetDam.y";
const RETURN_DAMPER_PHYSICAL_MAX_LIMIT_PATH: &str =
    "http://example.org#g36.source.multizone_vav_economizer_limits_common.retDamPhyPosMaxSig.y";
const MINIMUM_OUTDOOR_AIR_LOOP_ENABLED_PATH: &str =
    "http://example.org#g36.source.multizone_vav_economizer_limits_common.and3.y";

const OUTDOOR_AIRFLOW_NORMALIZED_VALUES: [f64; 8] = [0.0; 8];
const MINIMUM_OUTDOOR_AIRFLOW_SETPOINT_NORMALIZED_VALUES: [f64; 8] =
    [1.0, 1.0, 1.0, 12.0, 24.0, 8.0, 8.0, 8.0];
const OPERATION_MODE_VALUES: [i64; 8] = [1, 1, 1, 1, 1, 0, 1, 1];
const SUPPLY_FAN_STATUS_VALUES: [bool; 8] = [false, true, true, true, true, true, false, true];

const INPUTS: &[PointSpec] = &[
    PointSpec::real(OUTDOOR_AIRFLOW_NORMALIZED),
    PointSpec::real(MINIMUM_OUTDOOR_AIRFLOW_SETPOINT_NORMALIZED),
    PointSpec::integer(OPERATION_MODE),
    PointSpec::boolean(SUPPLY_FAN_STATUS),
];
const OUTPUTS: &[PointSpec] = &[
    PointSpec::real_alias(
        OUTDOOR_DAMPER_MIN_LIMIT_SOURCE,
        OUTDOOR_DAMPER_MIN_LIMIT_PATH,
    ),
    PointSpec::real_alias(
        OUTDOOR_DAMPER_MAX_LIMIT_SOURCE,
        OUTDOOR_DAMPER_MAX_LIMIT_PATH,
    ),
    PointSpec::real_alias(RETURN_DAMPER_MIN_LIMIT_SOURCE, RETURN_DAMPER_MIN_LIMIT_PATH),
    PointSpec::real_alias(RETURN_DAMPER_MAX_LIMIT_SOURCE, RETURN_DAMPER_MAX_LIMIT_PATH),
    PointSpec::real_alias(
        RETURN_DAMPER_PHYSICAL_MAX_LIMIT_SOURCE,
        RETURN_DAMPER_PHYSICAL_MAX_LIMIT_PATH,
    ),
    PointSpec::boolean_alias(
        MINIMUM_OUTDOOR_AIR_LOOP_ENABLED_SOURCE,
        MINIMUM_OUTDOOR_AIR_LOOP_ENABLED_PATH,
    ),
];
const SPEC: SequenceSpec = SequenceSpec {
    name: "multizone_vav_economizer_limits_common",
    cxf: ECONOMIZER_LIMITS_COMMON,
    t_stop: 7,
    sample_step: 1.0,
    inputs: INPUTS,
    outputs: OUTPUTS,
    input_fn: economizer_limits_common_inputs,
};

#[test]
fn g36_economizer_limits_common_outputs_match_determinism_golden() {
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

fn economizer_limits_common_inputs(t: f64) -> Vec<(String, Value)> {
    let row = t as usize;
    vec![
        pair(
            OUTDOOR_AIRFLOW_NORMALIZED,
            Value::Real(OUTDOOR_AIRFLOW_NORMALIZED_VALUES[row]),
        ),
        pair(
            MINIMUM_OUTDOOR_AIRFLOW_SETPOINT_NORMALIZED,
            Value::Real(MINIMUM_OUTDOOR_AIRFLOW_SETPOINT_NORMALIZED_VALUES[row]),
        ),
        pair(OPERATION_MODE, Value::Integer(OPERATION_MODE_VALUES[row])),
        pair(
            SUPPLY_FAN_STATUS,
            Value::Boolean(SUPPLY_FAN_STATUS_VALUES[row]),
        ),
    ]
}
