//! G36 Economizers.Subsequences.Modulations.ReturnFan Tier-2 determinism golden.
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

const ECONOMIZER_MODULATIONS_RETURN_FAN: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/multizone_vav_economizer_modulations_return_fan.jsonld"
);

const SUPPLY_TEMPERATURE_SIGNAL: &str =
    "http://example.org#g36.source.multizone_vav_economizer_modulations_return_fan.uTSup";
const RETURN_DAMPER_MIN: &str =
    "http://example.org#g36.source.multizone_vav_economizer_modulations_return_fan.uRetDam_min";
const RETURN_DAMPER_MAX: &str =
    "http://example.org#g36.source.multizone_vav_economizer_modulations_return_fan.uRetDam_max";

const RETURN_DAMPER_COMMAND_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_economizer_modulations_return_fan.yRetDam";
const OUTDOOR_DAMPER_COMMAND_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_economizer_modulations_return_fan.yOutDam";
const RETURN_DAMPER_COMMAND_PATH: &str =
    "http://example.org#g36.source.multizone_vav_economizer_modulations_return_fan.retDamPos.y";
const OUTDOOR_DAMPER_COMMAND_PATH: &str =
    "http://example.org#g36.source.multizone_vav_economizer_modulations_return_fan.one.y";

const SUPPLY_TEMPERATURE_SIGNAL_VALUES: [f64; 7] = [-0.5, -0.25, -0.125, 0.0, 0.125, 0.25, 0.5];

const INPUTS: &[PointSpec] = &[
    PointSpec::real(SUPPLY_TEMPERATURE_SIGNAL),
    PointSpec::real(RETURN_DAMPER_MIN),
    PointSpec::real(RETURN_DAMPER_MAX),
];
const OUTPUTS: &[PointSpec] = &[
    PointSpec::real_alias(RETURN_DAMPER_COMMAND_SOURCE, RETURN_DAMPER_COMMAND_PATH),
    PointSpec::real_alias(OUTDOOR_DAMPER_COMMAND_SOURCE, OUTDOOR_DAMPER_COMMAND_PATH),
];
const SPEC: SequenceSpec = SequenceSpec {
    name: "multizone_vav_economizer_modulations_return_fan",
    cxf: ECONOMIZER_MODULATIONS_RETURN_FAN,
    t_stop: 6,
    sample_step: 1.0,
    inputs: INPUTS,
    outputs: OUTPUTS,
    input_fn: economizer_modulations_return_fan_inputs,
};

#[test]
fn g36_economizer_modulations_return_fan_outputs_match_determinism_golden() {
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

fn economizer_modulations_return_fan_inputs(t: f64) -> Vec<(String, Value)> {
    let row = t as usize;
    vec![
        pair(
            SUPPLY_TEMPERATURE_SIGNAL,
            Value::Real(SUPPLY_TEMPERATURE_SIGNAL_VALUES[row]),
        ),
        pair(RETURN_DAMPER_MIN, Value::Real(0.125)),
        pair(RETURN_DAMPER_MAX, Value::Real(0.75)),
    ]
}
