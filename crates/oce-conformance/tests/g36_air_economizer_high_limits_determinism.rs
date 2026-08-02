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
const HIGH_LIMIT_TITLE24_FIXED_24: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_title24_fixed_24.jsonld"
);
const HIGH_LIMIT_TITLE24_FIXED_23: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_title24_fixed_23.jsonld"
);
const HIGH_LIMIT_TITLE24_FIXED_22: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_title24_fixed_22.jsonld"
);
const HIGH_LIMIT_TITLE24_FIXED_21: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_title24_fixed_21.jsonld"
);
const HIGH_LIMIT_ASHRAE_DIFFERENTIAL: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_ashrae_differential.jsonld"
);
const HIGH_LIMIT_TITLE24_DIFFERENTIAL_OFFSET_0: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_title24_differential_offset_0.jsonld"
);
const HIGH_LIMIT_TITLE24_DIFFERENTIAL_OFFSET_1: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_title24_differential_offset_1.jsonld"
);
const HIGH_LIMIT_TITLE24_DIFFERENTIAL_OFFSET_2: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_title24_differential_offset_2.jsonld"
);
const HIGH_LIMIT_TITLE24_DIFFERENTIAL_OFFSET_3: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_title24_differential_offset_3.jsonld"
);

const TEMPERATURE_CUTOFF_FIXED_24_SOURCE: &str =
    "http://example.org#g36.source.generic_air_economizer_high_limits_ashrae_fixed_24.TCut";
const TEMPERATURE_CUTOFF_FIXED_21_SOURCE: &str =
    "http://example.org#g36.source.generic_air_economizer_high_limits_ashrae_fixed_21.TCut";
const TEMPERATURE_CUTOFF_FIXED_18_SOURCE: &str =
    "http://example.org#g36.source.generic_air_economizer_high_limits_ashrae_fixed_18.TCut";
const TEMPERATURE_CUTOFF_TITLE24_FIXED_24_SOURCE: &str =
    "http://example.org#g36.source.generic_air_economizer_high_limits_title24_fixed_24.TCut";
const TEMPERATURE_CUTOFF_TITLE24_FIXED_23_SOURCE: &str =
    "http://example.org#g36.source.generic_air_economizer_high_limits_title24_fixed_23.TCut";
const TEMPERATURE_CUTOFF_TITLE24_FIXED_22_SOURCE: &str =
    "http://example.org#g36.source.generic_air_economizer_high_limits_title24_fixed_22.TCut";
const TEMPERATURE_CUTOFF_TITLE24_FIXED_21_SOURCE: &str =
    "http://example.org#g36.source.generic_air_economizer_high_limits_title24_fixed_21.TCut";
const RETURN_AIR_TEMPERATURE_ASHRAE_DIFFERENTIAL_SOURCE: &str =
    "http://example.org#g36.source.generic_air_economizer_high_limits_ashrae_differential.TRet";
const TEMPERATURE_CUTOFF_ASHRAE_DIFFERENTIAL_SOURCE: &str =
    "http://example.org#g36.source.generic_air_economizer_high_limits_ashrae_differential.TCut";
const RETURN_AIR_TEMPERATURE_TITLE24_DIFFERENTIAL_OFFSET_0_SOURCE: &str = "http://example.org#g36.source.generic_air_economizer_high_limits_title24_differential_offset_0.TRet";
const TEMPERATURE_CUTOFF_TITLE24_DIFFERENTIAL_OFFSET_0_SOURCE: &str = "http://example.org#g36.source.generic_air_economizer_high_limits_title24_differential_offset_0.TCut";
const RETURN_AIR_TEMPERATURE_TITLE24_DIFFERENTIAL_OFFSET_1_SOURCE: &str = "http://example.org#g36.source.generic_air_economizer_high_limits_title24_differential_offset_1.TRet";
const TEMPERATURE_CUTOFF_TITLE24_DIFFERENTIAL_OFFSET_1_SOURCE: &str = "http://example.org#g36.source.generic_air_economizer_high_limits_title24_differential_offset_1.TCut";
const RETURN_AIR_TEMPERATURE_TITLE24_DIFFERENTIAL_OFFSET_2_SOURCE: &str = "http://example.org#g36.source.generic_air_economizer_high_limits_title24_differential_offset_2.TRet";
const TEMPERATURE_CUTOFF_TITLE24_DIFFERENTIAL_OFFSET_2_SOURCE: &str = "http://example.org#g36.source.generic_air_economizer_high_limits_title24_differential_offset_2.TCut";
const RETURN_AIR_TEMPERATURE_TITLE24_DIFFERENTIAL_OFFSET_3_SOURCE: &str = "http://example.org#g36.source.generic_air_economizer_high_limits_title24_differential_offset_3.TRet";
const TEMPERATURE_CUTOFF_TITLE24_DIFFERENTIAL_OFFSET_3_SOURCE: &str = "http://example.org#g36.source.generic_air_economizer_high_limits_title24_differential_offset_3.TCut";
const RETURN_AIR_TEMPERATURES: [f64; 4] = [289.25, 293.15, 297.5, 301.75];

const NO_INPUTS: &[PointSpec] = &[];
const INPUTS_ASHRAE_DIFFERENTIAL: &[PointSpec] = &[PointSpec::real(
    RETURN_AIR_TEMPERATURE_ASHRAE_DIFFERENTIAL_SOURCE,
)];
const INPUTS_TITLE24_DIFFERENTIAL_OFFSET_0: &[PointSpec] = &[PointSpec::real(
    RETURN_AIR_TEMPERATURE_TITLE24_DIFFERENTIAL_OFFSET_0_SOURCE,
)];
const INPUTS_TITLE24_DIFFERENTIAL_OFFSET_1: &[PointSpec] = &[PointSpec::real(
    RETURN_AIR_TEMPERATURE_TITLE24_DIFFERENTIAL_OFFSET_1_SOURCE,
)];
const INPUTS_TITLE24_DIFFERENTIAL_OFFSET_2: &[PointSpec] = &[PointSpec::real(
    RETURN_AIR_TEMPERATURE_TITLE24_DIFFERENTIAL_OFFSET_2_SOURCE,
)];
const INPUTS_TITLE24_DIFFERENTIAL_OFFSET_3: &[PointSpec] = &[PointSpec::real(
    RETURN_AIR_TEMPERATURE_TITLE24_DIFFERENTIAL_OFFSET_3_SOURCE,
)];
const OUTPUTS_FIXED_24: &[PointSpec] = &[PointSpec::real_alias(
    TEMPERATURE_CUTOFF_FIXED_24_SOURCE,
    "http://example.org#g36.source.generic_air_economizer_high_limits_ashrae_fixed_24.con.y",
)];
const OUTPUTS_FIXED_21: &[PointSpec] = &[PointSpec::real_alias(
    TEMPERATURE_CUTOFF_FIXED_21_SOURCE,
    "http://example.org#g36.source.generic_air_economizer_high_limits_ashrae_fixed_21.con1.y",
)];
const OUTPUTS_FIXED_18: &[PointSpec] = &[PointSpec::real_alias(
    TEMPERATURE_CUTOFF_FIXED_18_SOURCE,
    "http://example.org#g36.source.generic_air_economizer_high_limits_ashrae_fixed_18.con2.y",
)];
const OUTPUTS_TITLE24_FIXED_24: &[PointSpec] = &[PointSpec::real_alias(
    TEMPERATURE_CUTOFF_TITLE24_FIXED_24_SOURCE,
    "http://example.org#g36.source.generic_air_economizer_high_limits_title24_fixed_24.con5.y",
)];
const OUTPUTS_TITLE24_FIXED_23: &[PointSpec] = &[PointSpec::real_alias(
    TEMPERATURE_CUTOFF_TITLE24_FIXED_23_SOURCE,
    "http://example.org#g36.source.generic_air_economizer_high_limits_title24_fixed_23.con6.y",
)];
const OUTPUTS_TITLE24_FIXED_22: &[PointSpec] = &[PointSpec::real_alias(
    TEMPERATURE_CUTOFF_TITLE24_FIXED_22_SOURCE,
    "http://example.org#g36.source.generic_air_economizer_high_limits_title24_fixed_22.con7.y",
)];
const OUTPUTS_TITLE24_FIXED_21: &[PointSpec] = &[PointSpec::real_alias(
    TEMPERATURE_CUTOFF_TITLE24_FIXED_21_SOURCE,
    "http://example.org#g36.source.generic_air_economizer_high_limits_title24_fixed_21.con8.y",
)];
const OUTPUTS_ASHRAE_DIFFERENTIAL: &[PointSpec] = &[PointSpec::real_alias(
    TEMPERATURE_CUTOFF_ASHRAE_DIFFERENTIAL_SOURCE,
    "http://example.org#g36.source.generic_air_economizer_high_limits_ashrae_differential.retAirIdentity.y",
)];
const OUTPUTS_TITLE24_DIFFERENTIAL_OFFSET_0: &[PointSpec] = &[PointSpec::real_alias(
    TEMPERATURE_CUTOFF_TITLE24_DIFFERENTIAL_OFFSET_0_SOURCE,
    "http://example.org#g36.source.generic_air_economizer_high_limits_title24_differential_offset_0.retAirIdentity.y",
)];
const OUTPUTS_TITLE24_DIFFERENTIAL_OFFSET_1: &[PointSpec] = &[PointSpec::real_alias(
    TEMPERATURE_CUTOFF_TITLE24_DIFFERENTIAL_OFFSET_1_SOURCE,
    "http://example.org#g36.source.generic_air_economizer_high_limits_title24_differential_offset_1.addPar.y",
)];
const OUTPUTS_TITLE24_DIFFERENTIAL_OFFSET_2: &[PointSpec] = &[PointSpec::real_alias(
    TEMPERATURE_CUTOFF_TITLE24_DIFFERENTIAL_OFFSET_2_SOURCE,
    "http://example.org#g36.source.generic_air_economizer_high_limits_title24_differential_offset_2.addPar1.y",
)];
const OUTPUTS_TITLE24_DIFFERENTIAL_OFFSET_3: &[PointSpec] = &[PointSpec::real_alias(
    TEMPERATURE_CUTOFF_TITLE24_DIFFERENTIAL_OFFSET_3_SOURCE,
    "http://example.org#g36.source.generic_air_economizer_high_limits_title24_differential_offset_3.addPar2.y",
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
    SequenceSpec {
        name: "generic_air_economizer_high_limits_title24_fixed_24",
        cxf: HIGH_LIMIT_TITLE24_FIXED_24,
        t_stop: 0,
        sample_step: 1.0,
        inputs: NO_INPUTS,
        outputs: OUTPUTS_TITLE24_FIXED_24,
        input_fn: no_inputs,
    },
    SequenceSpec {
        name: "generic_air_economizer_high_limits_title24_fixed_23",
        cxf: HIGH_LIMIT_TITLE24_FIXED_23,
        t_stop: 0,
        sample_step: 1.0,
        inputs: NO_INPUTS,
        outputs: OUTPUTS_TITLE24_FIXED_23,
        input_fn: no_inputs,
    },
    SequenceSpec {
        name: "generic_air_economizer_high_limits_title24_fixed_22",
        cxf: HIGH_LIMIT_TITLE24_FIXED_22,
        t_stop: 0,
        sample_step: 1.0,
        inputs: NO_INPUTS,
        outputs: OUTPUTS_TITLE24_FIXED_22,
        input_fn: no_inputs,
    },
    SequenceSpec {
        name: "generic_air_economizer_high_limits_title24_fixed_21",
        cxf: HIGH_LIMIT_TITLE24_FIXED_21,
        t_stop: 0,
        sample_step: 1.0,
        inputs: NO_INPUTS,
        outputs: OUTPUTS_TITLE24_FIXED_21,
        input_fn: no_inputs,
    },
    SequenceSpec {
        name: "generic_air_economizer_high_limits_ashrae_differential",
        cxf: HIGH_LIMIT_ASHRAE_DIFFERENTIAL,
        t_stop: 3,
        sample_step: 1.0,
        inputs: INPUTS_ASHRAE_DIFFERENTIAL,
        outputs: OUTPUTS_ASHRAE_DIFFERENTIAL,
        input_fn: ashrae_differential_inputs,
    },
    SequenceSpec {
        name: "generic_air_economizer_high_limits_title24_differential_offset_0",
        cxf: HIGH_LIMIT_TITLE24_DIFFERENTIAL_OFFSET_0,
        t_stop: 3,
        sample_step: 1.0,
        inputs: INPUTS_TITLE24_DIFFERENTIAL_OFFSET_0,
        outputs: OUTPUTS_TITLE24_DIFFERENTIAL_OFFSET_0,
        input_fn: title24_differential_offset_0_inputs,
    },
    SequenceSpec {
        name: "generic_air_economizer_high_limits_title24_differential_offset_1",
        cxf: HIGH_LIMIT_TITLE24_DIFFERENTIAL_OFFSET_1,
        t_stop: 3,
        sample_step: 1.0,
        inputs: INPUTS_TITLE24_DIFFERENTIAL_OFFSET_1,
        outputs: OUTPUTS_TITLE24_DIFFERENTIAL_OFFSET_1,
        input_fn: title24_differential_offset_1_inputs,
    },
    SequenceSpec {
        name: "generic_air_economizer_high_limits_title24_differential_offset_2",
        cxf: HIGH_LIMIT_TITLE24_DIFFERENTIAL_OFFSET_2,
        t_stop: 3,
        sample_step: 1.0,
        inputs: INPUTS_TITLE24_DIFFERENTIAL_OFFSET_2,
        outputs: OUTPUTS_TITLE24_DIFFERENTIAL_OFFSET_2,
        input_fn: title24_differential_offset_2_inputs,
    },
    SequenceSpec {
        name: "generic_air_economizer_high_limits_title24_differential_offset_3",
        cxf: HIGH_LIMIT_TITLE24_DIFFERENTIAL_OFFSET_3,
        t_stop: 3,
        sample_step: 1.0,
        inputs: INPUTS_TITLE24_DIFFERENTIAL_OFFSET_3,
        outputs: OUTPUTS_TITLE24_DIFFERENTIAL_OFFSET_3,
        input_fn: title24_differential_offset_3_inputs,
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

fn ashrae_differential_inputs(t: f64) -> Vec<(String, Value)> {
    return_air_inputs(RETURN_AIR_TEMPERATURE_ASHRAE_DIFFERENTIAL_SOURCE, t)
}

fn title24_differential_offset_0_inputs(t: f64) -> Vec<(String, Value)> {
    return_air_inputs(
        RETURN_AIR_TEMPERATURE_TITLE24_DIFFERENTIAL_OFFSET_0_SOURCE,
        t,
    )
}

fn title24_differential_offset_1_inputs(t: f64) -> Vec<(String, Value)> {
    return_air_inputs(
        RETURN_AIR_TEMPERATURE_TITLE24_DIFFERENTIAL_OFFSET_1_SOURCE,
        t,
    )
}

fn title24_differential_offset_2_inputs(t: f64) -> Vec<(String, Value)> {
    return_air_inputs(
        RETURN_AIR_TEMPERATURE_TITLE24_DIFFERENTIAL_OFFSET_2_SOURCE,
        t,
    )
}

fn title24_differential_offset_3_inputs(t: f64) -> Vec<(String, Value)> {
    return_air_inputs(
        RETURN_AIR_TEMPERATURE_TITLE24_DIFFERENTIAL_OFFSET_3_SOURCE,
        t,
    )
}

fn return_air_inputs(path: &str, t: f64) -> Vec<(String, Value)> {
    vec![support::pair(
        path,
        Value::Real(RETURN_AIR_TEMPERATURES[t as usize]),
    )]
}
