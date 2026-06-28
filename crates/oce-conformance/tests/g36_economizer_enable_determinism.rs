//! G36 Economizers.Subsequences.Enable Tier-2 determinism golden.
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

const ECONOMIZER_ENABLE: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/multizone_vav_economizer_enable.jsonld");

const OUTDOOR_AIR_TEMPERATURE: &str =
    "http://example.org#g36.source.multizone_vav_economizer_enable.TOut";
const OUTDOOR_AIR_CUTOFF: &str =
    "http://example.org#g36.source.multizone_vav_economizer_enable.TOutCut";
const OUTDOOR_DAMPER_MIN: &str =
    "http://example.org#g36.source.multizone_vav_economizer_enable.uOutDam_min";
const OUTDOOR_DAMPER_MAX: &str =
    "http://example.org#g36.source.multizone_vav_economizer_enable.uOutDam_max";
const RETURN_DAMPER_MAX: &str =
    "http://example.org#g36.source.multizone_vav_economizer_enable.uRetDam_max";
const RETURN_DAMPER_MIN: &str =
    "http://example.org#g36.source.multizone_vav_economizer_enable.uRetDam_min";
const RETURN_DAMPER_PHYSICAL_MAX: &str =
    "http://example.org#g36.source.multizone_vav_economizer_enable.uRetDamPhy_max";
const SUPPLY_FAN_ON: &str =
    "http://example.org#g36.source.multizone_vav_economizer_enable.u1SupFan";
const FREEZE_PROTECTION_STAGE: &str =
    "http://example.org#g36.source.multizone_vav_economizer_enable.uFreProSta";
const OUTDOOR_DAMPER_MAX_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_economizer_enable.yOutDam_max";
const RETURN_DAMPER_MAX_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_economizer_enable.yRetDam_max";
const RETURN_DAMPER_MIN_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_economizer_enable.yRetDam_min";
const OUTDOOR_DAMPER_MAX_RUNTIME: &str = "conn#13";
const RETURN_DAMPER_MAX_RUNTIME: &str = "conn#21";
const RETURN_DAMPER_MIN_RUNTIME: &str = "conn#25";

const INPUTS: &[PointSpec] = &[
    PointSpec::real(OUTDOOR_AIR_TEMPERATURE),
    PointSpec::real(OUTDOOR_AIR_CUTOFF),
    PointSpec::real(OUTDOOR_DAMPER_MIN),
    PointSpec::real(OUTDOOR_DAMPER_MAX),
    PointSpec::real(RETURN_DAMPER_MAX),
    PointSpec::real(RETURN_DAMPER_MIN),
    PointSpec::real(RETURN_DAMPER_PHYSICAL_MAX),
    PointSpec::boolean(SUPPLY_FAN_ON),
    PointSpec::integer(FREEZE_PROTECTION_STAGE),
];
const OUTPUTS: &[PointSpec] = &[
    PointSpec::real_alias(OUTDOOR_DAMPER_MAX_SOURCE, OUTDOOR_DAMPER_MAX_RUNTIME),
    PointSpec::real_alias(RETURN_DAMPER_MAX_SOURCE, RETURN_DAMPER_MAX_RUNTIME),
    PointSpec::real_alias(RETURN_DAMPER_MIN_SOURCE, RETURN_DAMPER_MIN_RUNTIME),
];
const SPEC: SequenceSpec = SequenceSpec {
    name: "multizone_vav_economizer_enable",
    cxf: ECONOMIZER_ENABLE,
    t_stop: 23,
    sample_step: 60.0,
    inputs: INPUTS,
    outputs: OUTPUTS,
    input_fn: economizer_enable_inputs,
};

#[test]
fn g36_economizer_enable_outputs_match_determinism_golden() {
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

fn economizer_enable_inputs(t: f64) -> Vec<(String, Value)> {
    let outdoor_air_temperature = match t as u32 {
        0 => 294.0,
        60..=900 => 296.0,
        960..=1380 => 293.0,
        _ => unreachable!("unexpected test instant {t}"),
    };
    let supply_fan_on = !matches!(t as u32, 660..=840);
    let freeze_protection_stage = if (900.0..960.0).contains(&t) { 1 } else { 0 };

    vec![
        pair(
            OUTDOOR_AIR_TEMPERATURE,
            Value::Real(outdoor_air_temperature),
        ),
        pair(OUTDOOR_AIR_CUTOFF, Value::Real(295.0)),
        pair(OUTDOOR_DAMPER_MIN, Value::Real(0.2)),
        pair(OUTDOOR_DAMPER_MAX, Value::Real(0.9)),
        pair(RETURN_DAMPER_MAX, Value::Real(0.8)),
        pair(RETURN_DAMPER_MIN, Value::Real(0.1)),
        pair(RETURN_DAMPER_PHYSICAL_MAX, Value::Real(1.0)),
        pair(SUPPLY_FAN_ON, Value::Boolean(supply_fan_on)),
        pair(
            FREEZE_PROTECTION_STAGE,
            Value::Integer(freeze_protection_stage),
        ),
    ]
}
