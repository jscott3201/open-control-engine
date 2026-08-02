//! G36 OutdoorAirFlow Title 24 AHU Tier-2 determinism golden.
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

const OUTDOOR_AIRFLOW_TITLE24_AHU: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/multizone_vav_outdoor_airflow_title24_ahu.jsonld"
);

const ABSOLUTE_MIN_FLOW: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_ahu.VSumZonAbsMin_flow";
const DESIGN_MIN_FLOW: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_ahu.VSumZonDesMin_flow";
const CO2_LOOP_MAX: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_ahu.uCO2Loo_max";
const MEASURED_OUTDOOR_AIR: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_ahu.VAirOut_flow";
const EFFECTIVE_ABSOLUTE_OUTDOOR_AIR_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_ahu.VEffAbsOutAir_flow";
const EFFECTIVE_ABSOLUTE_NORMALIZED_SOURCE: &str = "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_ahu.effAbsOutAir_normalized";
const EFFECTIVE_DESIGN_OUTDOOR_AIR_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_ahu.VEffDesOutAir_flow";
const EFFECTIVE_DESIGN_NORMALIZED_SOURCE: &str = "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_ahu.effDesOutAir_normalized";
const EFFECTIVE_OUTDOOR_AIR_NORMALIZED_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_ahu.effOutAir_normalized";
const MEASURED_NORMALIZED_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_ahu.outAir_normalized";
const EFFECTIVE_ABSOLUTE_OUTDOOR_AIR_PATH: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_ahu.min1.y";
const EFFECTIVE_ABSOLUTE_NORMALIZED_PATH: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_ahu.norVOutMin1.y";
const EFFECTIVE_DESIGN_OUTDOOR_AIR_PATH: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_ahu.min2.y";
const EFFECTIVE_DESIGN_NORMALIZED_PATH: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_ahu.norVOutMin.y";
const EFFECTIVE_OUTDOOR_AIR_NORMALIZED_PATH: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_ahu.norVOutMin2.y";
const MEASURED_NORMALIZED_PATH: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_ahu.norVOut.y";

const INPUTS: &[PointSpec] = &[
    PointSpec::real(ABSOLUTE_MIN_FLOW),
    PointSpec::real(DESIGN_MIN_FLOW),
    PointSpec::real(CO2_LOOP_MAX),
    PointSpec::real(MEASURED_OUTDOOR_AIR),
];
const OUTPUTS: &[PointSpec] = &[
    PointSpec::real_alias(
        EFFECTIVE_ABSOLUTE_OUTDOOR_AIR_SOURCE,
        EFFECTIVE_ABSOLUTE_OUTDOOR_AIR_PATH,
    ),
    PointSpec::real_alias(
        EFFECTIVE_ABSOLUTE_NORMALIZED_SOURCE,
        EFFECTIVE_ABSOLUTE_NORMALIZED_PATH,
    ),
    PointSpec::real_alias(
        EFFECTIVE_DESIGN_OUTDOOR_AIR_SOURCE,
        EFFECTIVE_DESIGN_OUTDOOR_AIR_PATH,
    ),
    PointSpec::real_alias(
        EFFECTIVE_DESIGN_NORMALIZED_SOURCE,
        EFFECTIVE_DESIGN_NORMALIZED_PATH,
    ),
    PointSpec::real_alias(
        EFFECTIVE_OUTDOOR_AIR_NORMALIZED_SOURCE,
        EFFECTIVE_OUTDOOR_AIR_NORMALIZED_PATH,
    ),
    PointSpec::real_alias(MEASURED_NORMALIZED_SOURCE, MEASURED_NORMALIZED_PATH),
];
const SPEC: SequenceSpec = SequenceSpec {
    name: "multizone_vav_outdoor_airflow_title24_ahu",
    cxf: OUTDOOR_AIRFLOW_TITLE24_AHU,
    t_stop: 5,
    sample_step: 1.0,
    inputs: INPUTS,
    outputs: OUTPUTS,
    input_fn: outdoor_airflow_title24_inputs,
};

#[test]
fn g36_outdoor_airflow_title24_ahu_outputs_match_determinism_golden() {
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

fn outdoor_airflow_title24_inputs(t: f64) -> Vec<(String, Value)> {
    let (absolute, design, co2, measured) = match t as u32 {
        0 => (1.0, 2.0, 0.0, 4.0),
        1 => (4.0, 10.0, 0.5, 6.0),
        2 => (4.0, 10.0, 0.75, 0.0),
        3 => (2.5, 4.0, 1.4, 9.0),
        4 => (0.0, 0.0, 1.0, 8.0),
        5 => (1.0, 5.0, -0.25, 1.6),
        _ => unreachable!("unexpected test instant {t}"),
    };

    vec![
        pair(ABSOLUTE_MIN_FLOW, Value::Real(absolute)),
        pair(DESIGN_MIN_FLOW, Value::Real(design)),
        pair(CO2_LOOP_MAX, Value::Real(co2)),
        pair(MEASURED_OUTDOOR_AIR, Value::Real(measured)),
    ]
}
