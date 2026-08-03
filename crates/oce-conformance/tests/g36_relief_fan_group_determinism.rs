//! G36 ReliefFanGroup Tier-2 determinism golden.
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

const RELIEF_FAN_GROUP: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/multizone_vav_relief_fan_group.jsonld");

const SUPPLY_FAN_1: &str =
    "http://example.org#g36.source.multizone_vav_relief_fan_group.u1SupFan_1";
const SUPPLY_FAN_2: &str =
    "http://example.org#g36.source.multizone_vav_relief_fan_group.u1SupFan_2";
const BUILDING_PRESSURE: &str =
    "http://example.org#g36.source.multizone_vav_relief_fan_group.dpBui";
const ALARM_1: &str = "http://example.org#g36.source.multizone_vav_relief_fan_group.uRelFanAla_1";
const ALARM_2: &str = "http://example.org#g36.source.multizone_vav_relief_fan_group.uRelFanAla_2";
const ALARM_3: &str = "http://example.org#g36.source.multizone_vav_relief_fan_group.uRelFanAla_3";
const ALARM_4: &str = "http://example.org#g36.source.multizone_vav_relief_fan_group.uRelFanAla_4";
const PROOF_1: &str = "http://example.org#g36.source.multizone_vav_relief_fan_group.u1RelFan_1";
const PROOF_2: &str = "http://example.org#g36.source.multizone_vav_relief_fan_group.u1RelFan_2";
const PROOF_3: &str = "http://example.org#g36.source.multizone_vav_relief_fan_group.u1RelFan_3";
const PROOF_4: &str = "http://example.org#g36.source.multizone_vav_relief_fan_group.u1RelFan_4";

const AVERAGED_PRESSURE_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_relief_fan_group.yDpBui";
const RELIEF_FAN_1_SPEED_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_relief_fan_group.yRelFan_1";
const RELIEF_FAN_2_SPEED_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_relief_fan_group.yRelFan_2";
const RELIEF_FAN_3_SPEED_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_relief_fan_group.yRelFan_3";
const RELIEF_FAN_4_SPEED_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_relief_fan_group.yRelFan_4";
const RELIEF_DAMPER_1_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_relief_fan_group.yDam_1";
const RELIEF_DAMPER_2_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_relief_fan_group.yDam_2";
const RELIEF_DAMPER_3_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_relief_fan_group.yDam_3";
const RELIEF_DAMPER_4_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_relief_fan_group.yDam_4";

const INPUTS: &[PointSpec] = &[
    PointSpec::boolean(SUPPLY_FAN_1),
    PointSpec::boolean(SUPPLY_FAN_2),
    PointSpec::real(BUILDING_PRESSURE),
    PointSpec::integer(ALARM_1),
    PointSpec::integer(ALARM_2),
    PointSpec::integer(ALARM_3),
    PointSpec::integer(ALARM_4),
    PointSpec::boolean(PROOF_1),
    PointSpec::boolean(PROOF_2),
    PointSpec::boolean(PROOF_3),
    PointSpec::boolean(PROOF_4),
];
const OUTPUTS: &[PointSpec] = &[
    PointSpec::real(AVERAGED_PRESSURE_SOURCE),
    PointSpec::real(RELIEF_FAN_1_SPEED_SOURCE),
    PointSpec::real(RELIEF_FAN_2_SPEED_SOURCE),
    PointSpec::real(RELIEF_FAN_3_SPEED_SOURCE),
    PointSpec::real(RELIEF_FAN_4_SPEED_SOURCE),
    PointSpec::real(RELIEF_DAMPER_1_SOURCE),
    PointSpec::real(RELIEF_DAMPER_2_SOURCE),
    PointSpec::real(RELIEF_DAMPER_3_SOURCE),
    PointSpec::real(RELIEF_DAMPER_4_SOURCE),
];
const SPEC: SequenceSpec = SequenceSpec {
    name: "multizone_vav_relief_fan_group",
    cxf: RELIEF_FAN_GROUP,
    t_stop: 160,
    sample_step: 15.0,
    inputs: INPUTS,
    outputs: OUTPUTS,
    input_fn: relief_fan_group_inputs,
};

#[test]
fn g36_relief_fan_group_outputs_match_determinism_golden() {
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

fn relief_fan_group_inputs(t: f64) -> Vec<(String, Value)> {
    let supply_enabled = (300.0..2220.0).contains(&t);
    let building_pressure = if (300.0..1620.0).contains(&t) {
        18.0
    } else {
        12.0
    };
    let alarms = [if (360.0..600.0).contains(&t) { 2 } else { 0 }, 0, 0, 0];
    let proofs = [
        (1500.0..2100.0).contains(&t),
        false,
        (900.0..2100.0).contains(&t),
        false,
    ];

    vec![
        pair(SUPPLY_FAN_1, Value::Boolean(supply_enabled)),
        pair(SUPPLY_FAN_2, Value::Boolean(supply_enabled)),
        pair(BUILDING_PRESSURE, Value::Real(building_pressure)),
        pair(ALARM_1, Value::Integer(alarms[0])),
        pair(ALARM_2, Value::Integer(alarms[1])),
        pair(ALARM_3, Value::Integer(alarms[2])),
        pair(ALARM_4, Value::Integer(alarms[3])),
        pair(PROOF_1, Value::Boolean(proofs[0])),
        pair(PROOF_2, Value::Boolean(proofs[1])),
        pair(PROOF_3, Value::Boolean(proofs[2])),
        pair(PROOF_4, Value::Boolean(proofs[3])),
    ]
}
