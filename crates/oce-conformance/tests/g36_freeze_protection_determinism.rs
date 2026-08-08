//! G36 FreezeProtection Tier-2 determinism golden.
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

const FREEZE_PROTECTION: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/multizone_vav_freeze_protection.jsonld");

const OUTDOOR_DAMPER_MIN_POSITION: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.uOutDamPosMin";
const OUTDOOR_DAMPER: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.uOutDam";
const HEATING_COIL: &str = "http://example.org#g36.source.multizone_vav_freeze_protection.uHeaCoi";
const MINIMUM_OUTDOOR_DAMPER: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.uMinOutDam";
const RETURN_DAMPER: &str = "http://example.org#g36.source.multizone_vav_freeze_protection.uRetDam";
const SUPPLY_AIR_TEMPERATURE: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.TAirSup";
const SOFTWARE_RESET: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.u1SofSwiRes";
const SUPPLY_FAN_STATUS_INPUT: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.u1SupFan";
const SUPPLY_FAN_SPEED_INPUT: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.uSupFan";
const COOLING_COIL: &str = "http://example.org#g36.source.multizone_vav_freeze_protection.uCooCoi";
const MIXED_AIR_TEMPERATURE: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.TAirMix";

const FREEZE_PROTECTION_STAGE_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.yFreProSta";
const CHILLED_WATER_PUMP_ENABLE_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.y1EneCHWPum";
const RETURN_DAMPER_COMMAND_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.yRetDam";
const OUTDOOR_DAMPER_COMMAND_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.yOutDam";
const MINIMUM_OUTDOOR_DAMPER_COMMAND_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.yMinOutDam";
const SUPPLY_FAN_STATUS_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.y1SupFan";
const SUPPLY_FAN_SPEED_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.ySupFan";
const COOLING_COIL_COMMAND_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.yCooCoi";
const HEATING_COIL_COMMAND_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.yHeaCoi";
const HOT_WATER_PLANT_REQUEST_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.yHotWatPlaReq";
const ALARM_LEVEL_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.yAla";

const INPUTS: &[PointSpec] = &[
    PointSpec::real(OUTDOOR_DAMPER_MIN_POSITION),
    PointSpec::real(OUTDOOR_DAMPER),
    PointSpec::real(HEATING_COIL),
    PointSpec::real(MINIMUM_OUTDOOR_DAMPER),
    PointSpec::real(RETURN_DAMPER),
    PointSpec::real(SUPPLY_AIR_TEMPERATURE),
    PointSpec::boolean(SOFTWARE_RESET),
    PointSpec::boolean(SUPPLY_FAN_STATUS_INPUT),
    PointSpec::real(SUPPLY_FAN_SPEED_INPUT),
    PointSpec::real(COOLING_COIL),
    PointSpec::real(MIXED_AIR_TEMPERATURE),
];
const OUTPUTS: &[PointSpec] = &[
    PointSpec::integer(FREEZE_PROTECTION_STAGE_SOURCE),
    PointSpec::boolean(CHILLED_WATER_PUMP_ENABLE_SOURCE),
    PointSpec::real(RETURN_DAMPER_COMMAND_SOURCE),
    PointSpec::real(OUTDOOR_DAMPER_COMMAND_SOURCE),
    PointSpec::real(MINIMUM_OUTDOOR_DAMPER_COMMAND_SOURCE),
    PointSpec::boolean(SUPPLY_FAN_STATUS_SOURCE),
    PointSpec::real(SUPPLY_FAN_SPEED_SOURCE),
    PointSpec::real(COOLING_COIL_COMMAND_SOURCE),
    PointSpec::real(HEATING_COIL_COMMAND_SOURCE),
    PointSpec::integer(HOT_WATER_PLANT_REQUEST_SOURCE),
    PointSpec::integer(ALARM_LEVEL_SOURCE),
];
const SPEC: SequenceSpec = SequenceSpec {
    name: "multizone_vav_freeze_protection",
    cxf: FREEZE_PROTECTION,
    t_stop: 110,
    sample_step: 60.0,
    inputs: INPUTS,
    outputs: OUTPUTS,
    input_fn: freeze_protection_inputs,
};

#[test]
fn g36_freeze_protection_outputs_match_determinism_golden() {
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

fn freeze_protection_inputs(t: f64) -> Vec<(String, Value)> {
    vec![
        pair(OUTDOOR_DAMPER_MIN_POSITION, Value::Real(0.20)),
        pair(OUTDOOR_DAMPER, Value::Real(0.62)),
        pair(HEATING_COIL, Value::Real(0.31)),
        pair(MINIMUM_OUTDOOR_DAMPER, Value::Real(0.17)),
        pair(RETURN_DAMPER, Value::Real(0.73)),
        pair(
            SUPPLY_AIR_TEMPERATURE,
            Value::Real(supply_air_temperature(t)),
        ),
        pair(
            SOFTWARE_RESET,
            Value::Boolean((t == 1980.0) || (t == 6360.0)),
        ),
        pair(SUPPLY_FAN_STATUS_INPUT, Value::Boolean(true)),
        pair(SUPPLY_FAN_SPEED_INPUT, Value::Real(0.58)),
        pair(COOLING_COIL, Value::Real(0.44)),
        pair(MIXED_AIR_TEMPERATURE, Value::Real(mixed_air_temperature(t))),
    ]
}

fn supply_air_temperature(t: f64) -> f64 {
    let celsius = if t < 60.0 {
        8.0
    } else if t < 600.0 {
        4.0
    } else if t < 960.0 {
        7.2
    } else if t < 1500.0 {
        3.0
    } else if t < 1860.0 {
        0.5
    } else if t < 2040.0 {
        7.2
    } else if t < 4920.0 {
        5.5
    } else if t < 5400.0 {
        7.2
    } else if t < 6360.0 {
        2.0
    } else {
        7.2
    };
    273.15 + celsius
}

fn mixed_air_temperature(t: f64) -> f64 {
    if (1800.0..1920.0).contains(&t) {
        299.75
    } else if (1920.0..2040.0).contains(&t) || (6300.0..6420.0).contains(&t) {
        300.50
    } else {
        300.15
    }
}
