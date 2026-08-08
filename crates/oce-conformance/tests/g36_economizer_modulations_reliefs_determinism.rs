//! G36 Economizers.Subsequences.Modulations.Reliefs Tier-2 determinism golden.
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

const ECONOMIZER_MODULATIONS_RELIEFS: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/multizone_vav_economizer_modulations_reliefs.jsonld"
);

const SUPPLY_TEMPERATURE_SIGNAL: &str =
    "http://example.org#g36.source.multizone_vav_economizer_modulations_reliefs.uTSup";
const OUTDOOR_DAMPER_MIN: &str =
    "http://example.org#g36.source.multizone_vav_economizer_modulations_reliefs.uOutDam_min";
const OUTDOOR_DAMPER_MAX: &str =
    "http://example.org#g36.source.multizone_vav_economizer_modulations_reliefs.uOutDam_max";
const RETURN_DAMPER_MIN: &str =
    "http://example.org#g36.source.multizone_vav_economizer_modulations_reliefs.uRetDam_min";
const RETURN_DAMPER_MAX: &str =
    "http://example.org#g36.source.multizone_vav_economizer_modulations_reliefs.uRetDam_max";

const OUTDOOR_DAMPER_COMMAND_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_economizer_modulations_reliefs.yOutDam";
const RETURN_DAMPER_COMMAND_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_economizer_modulations_reliefs.yRetDam";

const SUPPLY_TEMPERATURE_SIGNAL_VALUES: [f64; 7] = [-0.5, -0.25, -0.125, 0.0, 0.125, 0.25, 0.5];

const INPUTS: &[PointSpec] = &[
    PointSpec::real(SUPPLY_TEMPERATURE_SIGNAL),
    PointSpec::real(OUTDOOR_DAMPER_MIN),
    PointSpec::real(OUTDOOR_DAMPER_MAX),
    PointSpec::real(RETURN_DAMPER_MIN),
    PointSpec::real(RETURN_DAMPER_MAX),
];
const OUTPUTS: &[PointSpec] = &[
    PointSpec::real(OUTDOOR_DAMPER_COMMAND_SOURCE),
    PointSpec::real(RETURN_DAMPER_COMMAND_SOURCE),
];
const SPEC: SequenceSpec = SequenceSpec {
    name: "multizone_vav_economizer_modulations_reliefs",
    cxf: ECONOMIZER_MODULATIONS_RELIEFS,
    t_stop: 6,
    sample_step: 1.0,
    inputs: INPUTS,
    outputs: OUTPUTS,
    input_fn: economizer_modulations_reliefs_inputs,
};

#[test]
fn g36_economizer_modulations_reliefs_outputs_match_determinism_golden() {
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

fn economizer_modulations_reliefs_inputs(t: f64) -> Vec<(String, Value)> {
    let row = (t / 1.0) as usize;
    vec![
        pair(
            SUPPLY_TEMPERATURE_SIGNAL,
            Value::Real(SUPPLY_TEMPERATURE_SIGNAL_VALUES[row]),
        ),
        pair(OUTDOOR_DAMPER_MIN, Value::Real(0.25)),
        pair(OUTDOOR_DAMPER_MAX, Value::Real(0.875)),
        pair(RETURN_DAMPER_MIN, Value::Real(0.125)),
        pair(RETURN_DAMPER_MAX, Value::Real(0.75)),
    ]
}
