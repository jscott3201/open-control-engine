//! G36 OutdoorAirFlow ASHRAE 62.1 SumZone Tier-2 determinism golden.
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

const OUTDOOR_AIRFLOW_SUMZONE: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/multizone_vav_outdoor_airflow_sumzone.jsonld");

const U_OPE_MOD_1: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_sumzone.uOpeMod_1";
const U_OPE_MOD_2: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_sumzone.uOpeMod_2";
const POP_1: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_sumzone.VAdjPopBreZon_flow_1";
const POP_2: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_sumzone.VAdjPopBreZon_flow_2";
const POP_3: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_sumzone.VAdjPopBreZon_flow_3";
const AREA_1: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_sumzone.VAdjAreBreZon_flow_1";
const AREA_2: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_sumzone.VAdjAreBreZon_flow_2";
const AREA_3: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_sumzone.VAdjAreBreZon_flow_3";
const PRI_1: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_sumzone.VZonPri_flow_1";
const PRI_2: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_sumzone.VZonPri_flow_2";
const PRI_3: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_sumzone.VZonPri_flow_3";
const MIN_OA_1: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_sumzone.VMinOA_flow_1";
const MIN_OA_2: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_sumzone.VMinOA_flow_2";
const MIN_OA_3: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_sumzone.VMinOA_flow_3";
const SUMMED_POP_FLOW_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_sumzone.VSumAdjPopBreZon_flow";
const SUMMED_AREA_FLOW_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_sumzone.VSumAdjAreBreZon_flow";
const SUMMED_PRIMARY_FLOW_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_sumzone.VSumZonPri_flow";
const MAX_OUTDOOR_AIR_FRACTION_SOURCE: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_sumzone.uOutAirFra_max";
const SUMMED_POP_FLOW_RUNTIME: &str = "conn#52";
const SUMMED_AREA_FLOW_RUNTIME: &str = "conn#55";
const SUMMED_PRIMARY_FLOW_RUNTIME: &str = "conn#58";
const MAX_OUTDOOR_AIR_FRACTION_RUNTIME: &str = "conn#62";

const INPUTS: &[PointSpec] = &[
    PointSpec::integer(U_OPE_MOD_1),
    PointSpec::integer(U_OPE_MOD_2),
    PointSpec::real(POP_1),
    PointSpec::real(POP_2),
    PointSpec::real(POP_3),
    PointSpec::real(AREA_1),
    PointSpec::real(AREA_2),
    PointSpec::real(AREA_3),
    PointSpec::real(PRI_1),
    PointSpec::real(PRI_2),
    PointSpec::real(PRI_3),
    PointSpec::real(MIN_OA_1),
    PointSpec::real(MIN_OA_2),
    PointSpec::real(MIN_OA_3),
];
const OUTPUTS: &[PointSpec] = &[
    PointSpec::real_alias(SUMMED_POP_FLOW_SOURCE, SUMMED_POP_FLOW_RUNTIME),
    PointSpec::real_alias(SUMMED_AREA_FLOW_SOURCE, SUMMED_AREA_FLOW_RUNTIME),
    PointSpec::real_alias(SUMMED_PRIMARY_FLOW_SOURCE, SUMMED_PRIMARY_FLOW_RUNTIME),
    PointSpec::real_alias(
        MAX_OUTDOOR_AIR_FRACTION_SOURCE,
        MAX_OUTDOOR_AIR_FRACTION_RUNTIME,
    ),
];
const SPEC: SequenceSpec = SequenceSpec {
    name: "multizone_vav_outdoor_airflow_sumzone",
    cxf: OUTDOOR_AIRFLOW_SUMZONE,
    t_stop: 5,
    sample_step: 1.0,
    inputs: INPUTS,
    outputs: OUTPUTS,
    input_fn: outdoor_airflow_sumzone_inputs,
};

#[test]
fn g36_outdoor_airflow_sumzone_outputs_match_determinism_golden() {
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

fn outdoor_airflow_sumzone_inputs(t: f64) -> Vec<(String, Value)> {
    let (operation_modes, population, area, primary, minimum_outdoor_air) = match t as u32 {
        0 => (
            [1, 1],
            [1.0, 2.0, 3.0],
            [4.0, 5.0, 6.0],
            [10.0, 20.0, 30.0],
            [1.0, 2.0, 3.0],
        ),
        1 => (
            [1, 7],
            [2.0, 4.0, 8.0],
            [1.0, 3.0, 5.0],
            [2.0, 4.0, 8.0],
            [1.0, 2.0, 4.0],
        ),
        2 => (
            [4, 1],
            [2.5, 0.5, 1.5],
            [6.0, 2.0, 1.0],
            [5.0, 0.5, 1.5],
            [10.0, 0.1, 2.0],
        ),
        3 => (
            [7, 6],
            [10.0, 20.0, 30.0],
            [3.0, 2.0, 1.0],
            [0.0, 0.0, 0.0],
            [1.0, 1.0, 1.0],
        ),
        4 => (
            [1, 1],
            [0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.00005, 0.5],
            [1.0, 1.0, 0.1],
        ),
        5 => (
            [1, 1],
            [1.25, 2.5, 5.0],
            [8.0, 13.0, 21.0],
            [1.0, 1.0, 1.0],
            [0.2, 0.8, 0.3],
        ),
        _ => unreachable!("unexpected test instant {t}"),
    };

    vec![
        pair(U_OPE_MOD_1, Value::Integer(operation_modes[0])),
        pair(U_OPE_MOD_2, Value::Integer(operation_modes[1])),
        pair(POP_1, Value::Real(population[0])),
        pair(POP_2, Value::Real(population[1])),
        pair(POP_3, Value::Real(population[2])),
        pair(AREA_1, Value::Real(area[0])),
        pair(AREA_2, Value::Real(area[1])),
        pair(AREA_3, Value::Real(area[2])),
        pair(PRI_1, Value::Real(primary[0])),
        pair(PRI_2, Value::Real(primary[1])),
        pair(PRI_3, Value::Real(primary[2])),
        pair(MIN_OA_1, Value::Real(minimum_outdoor_air[0])),
        pair(MIN_OA_2, Value::Real(minimum_outdoor_air[1])),
        pair(MIN_OA_3, Value::Real(minimum_outdoor_air[2])),
    ]
}
