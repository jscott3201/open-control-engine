//! G36 Generic.AirEconomizerHighLimits Tier-2 determinism goldens.
//!
//! These fixtures are engine self-output snapshots, not independent correctness oracles.

use oce_api::Value;
use oce_conformance::drive_trace_with_options;

#[allow(dead_code)]
#[path = "g36_determinism/support.rs"]
mod support;

use support::{
    PointSpec, SequenceSpec, assert_exact_comparisons_pass, assert_output_table_shape,
    assert_provenance_matches_outputs, bless_enabled, bless_sequence, captured_output_table,
    config_for, driver_reference_from_output_golden, options_for, read_output_golden,
};

const HIGH_LIMIT_FIXED_24: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_ashrae_fixed_24.jsonld"
);
const HIGH_LIMIT_FIXED_21: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_ashrae_fixed_21.jsonld"
);
const HIGH_LIMIT_FIXED_18: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_ashrae_fixed_18.jsonld"
);

const TEMPERATURE_CUTOFF_FIXED_24_SOURCE: &str =
    "http://example.org#g36.source.generic_air_economizer_high_limits_ashrae_fixed_24.TCut";
const TEMPERATURE_CUTOFF_FIXED_21_SOURCE: &str =
    "http://example.org#g36.source.generic_air_economizer_high_limits_ashrae_fixed_21.TCut";
const TEMPERATURE_CUTOFF_FIXED_18_SOURCE: &str =
    "http://example.org#g36.source.generic_air_economizer_high_limits_ashrae_fixed_18.TCut";
const TEMPERATURE_CUTOFF_RUNTIME: &str = "conn#0";

const NO_INPUTS: &[PointSpec] = &[];
const OUTPUTS_FIXED_24: &[PointSpec] = &[PointSpec::real_alias(
    TEMPERATURE_CUTOFF_FIXED_24_SOURCE,
    TEMPERATURE_CUTOFF_RUNTIME,
)];
const OUTPUTS_FIXED_21: &[PointSpec] = &[PointSpec::real_alias(
    TEMPERATURE_CUTOFF_FIXED_21_SOURCE,
    TEMPERATURE_CUTOFF_RUNTIME,
)];
const OUTPUTS_FIXED_18: &[PointSpec] = &[PointSpec::real_alias(
    TEMPERATURE_CUTOFF_FIXED_18_SOURCE,
    TEMPERATURE_CUTOFF_RUNTIME,
)];

const SPECS: &[SequenceSpec] = &[
    SequenceSpec {
        name: "generic_air_economizer_high_limits_ashrae_fixed_24",
        cxf: HIGH_LIMIT_FIXED_24,
        t_stop: 0,
        sample_step: 1.0,
        inputs: NO_INPUTS,
        outputs: OUTPUTS_FIXED_24,
        input_fn: no_inputs,
    },
    SequenceSpec {
        name: "generic_air_economizer_high_limits_ashrae_fixed_21",
        cxf: HIGH_LIMIT_FIXED_21,
        t_stop: 0,
        sample_step: 1.0,
        inputs: NO_INPUTS,
        outputs: OUTPUTS_FIXED_21,
        input_fn: no_inputs,
    },
    SequenceSpec {
        name: "generic_air_economizer_high_limits_ashrae_fixed_18",
        cxf: HIGH_LIMIT_FIXED_18,
        t_stop: 0,
        sample_step: 1.0,
        inputs: NO_INPUTS,
        outputs: OUTPUTS_FIXED_18,
        input_fn: no_inputs,
    },
];

#[test]
fn g36_air_economizer_high_limits_outputs_match_determinism_goldens() {
    for spec in SPECS {
        if bless_enabled() {
            bless_sequence(spec);
        }

        let golden = read_output_golden(spec);
        assert_provenance_matches_outputs(spec, &golden);
        let reference = driver_reference_from_output_golden(spec, &golden);
        let run = drive_trace_with_options(
            spec.cxf.as_bytes(),
            &config_for(spec),
            &reference,
            &options_for(spec),
        )
        .unwrap_or_else(|err| panic!("{} driver run failed: {err}", spec.name));

        assert_output_table_shape(spec, &golden);
        assert_eq!(
            captured_output_table(spec, &run),
            golden,
            "{} captured table drifted from committed golden",
            spec.name
        );
        assert_exact_comparisons_pass(spec, golden.n_rows, &run.comparisons);
    }
}

fn no_inputs(_: f64) -> Vec<(String, Value)> {
    Vec::new()
}
