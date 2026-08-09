//! G36 OutdoorAirFlow Title 24 SumZone Tier-2 determinism golden.
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

const OUTDOOR_AIRFLOW_TITLE24_SUMZONE: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/multizone_vav_outdoor_airflow_title24_sumzone.jsonld"
);

const U_OPE_MOD_1: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_sumzone.uOpeMod_1";
const U_OPE_MOD_2: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_sumzone.uOpeMod_2";
const ABS_MIN_1: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_sumzone.VZonAbsMin_flow_1";
const ABS_MIN_2: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_sumzone.VZonAbsMin_flow_2";
const ABS_MIN_3: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_sumzone.VZonAbsMin_flow_3";
const DES_MIN_1: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_sumzone.VZonDesMin_flow_1";
const DES_MIN_2: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_sumzone.VZonDesMin_flow_2";
const DES_MIN_3: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_sumzone.VZonDesMin_flow_3";
const CO2_1: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_sumzone.uCO2_1";
const CO2_2: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_sumzone.uCO2_2";
const CO2_3: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_sumzone.uCO2_3";
const SUMMED_ABSOLUTE_MIN_FLOW_SOURCE: &str = "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_sumzone.VSumZonAbsMin_flow";
const SUMMED_DESIGN_MIN_FLOW_SOURCE: &str = "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_sumzone.VSumZonDesMin_flow";
const MAX_CO2_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_sumzone.yMaxCO2";

const INPUTS: &[PointSpec] = &[
    PointSpec::integer(U_OPE_MOD_1),
    PointSpec::integer(U_OPE_MOD_2),
    PointSpec::real(ABS_MIN_1),
    PointSpec::real(ABS_MIN_2),
    PointSpec::real(ABS_MIN_3),
    PointSpec::real(DES_MIN_1),
    PointSpec::real(DES_MIN_2),
    PointSpec::real(DES_MIN_3),
    PointSpec::real(CO2_1),
    PointSpec::real(CO2_2),
    PointSpec::real(CO2_3),
];
const OUTPUTS: &[PointSpec] = &[
    PointSpec::real(SUMMED_ABSOLUTE_MIN_FLOW_SOURCE),
    PointSpec::real(SUMMED_DESIGN_MIN_FLOW_SOURCE),
    PointSpec::real(MAX_CO2_SOURCE),
];
const SPEC: SequenceSpec = SequenceSpec {
    name: "multizone_vav_outdoor_airflow_title24_sumzone",
    cxf: OUTDOOR_AIRFLOW_TITLE24_SUMZONE,
    t_stop: 5,
    sample_step: 1.0,
    inputs: INPUTS,
    outputs: OUTPUTS,
    input_fn: outdoor_airflow_title24_sumzone_inputs,
};

#[test]
fn g36_outdoor_airflow_title24_sumzone_outputs_match_determinism_golden() {
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

fn outdoor_airflow_title24_sumzone_inputs(t: f64) -> Vec<(String, Value)> {
    let (operation_modes, absolute_minimums, design_minimums, co2) = match t as u32 {
        0 => ([1, 1], [1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [0.1, 0.6, 0.2]),
        1 => ([1, 7], [2.0, 4.0, 8.0], [1.0, 3.0, 5.0], [-0.5, 0.0, 0.2]),
        2 => ([4, 1], [2.5, 0.5, 1.5], [6.0, 2.0, 1.0], [0.9, 0.3, 0.7]),
        3 => ([7, 6], [10.0, 20.0, 30.0], [3.0, 2.0, 1.0], [0.0, 0.0, 0.0]),
        4 => ([1, 1], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [-1.0, -2.0, -3.0]),
        5 => ([3, 1], [1.25, 2.5, 5.0], [8.0, 13.0, 21.0], [1.2, 1.2, 1.1]),
        _ => unreachable!("unexpected test instant {t}"),
    };

    vec![
        pair(U_OPE_MOD_1, Value::Integer(operation_modes[0])),
        pair(U_OPE_MOD_2, Value::Integer(operation_modes[1])),
        pair(ABS_MIN_1, Value::Real(absolute_minimums[0])),
        pair(ABS_MIN_2, Value::Real(absolute_minimums[1])),
        pair(ABS_MIN_3, Value::Real(absolute_minimums[2])),
        pair(DES_MIN_1, Value::Real(design_minimums[0])),
        pair(DES_MIN_2, Value::Real(design_minimums[1])),
        pair(DES_MIN_3, Value::Real(design_minimums[2])),
        pair(CO2_1, Value::Real(co2[0])),
        pair(CO2_2, Value::Real(co2[1])),
        pair(CO2_3, Value::Real(co2[2])),
    ]
}
