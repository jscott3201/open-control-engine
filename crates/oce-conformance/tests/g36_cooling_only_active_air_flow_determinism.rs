//! G36 CoolingOnly ActiveAirFlow Tier-2 determinism golden.
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

const ACTIVE_AIR_FLOW: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/cooling_only_active_air_flow.jsonld");

const OPERATING_MODE: &str = "http://example.org#g36.source.cooling_only_active_air_flow.uOpeMod";
const OCCUPIED_MINIMUM_AIRFLOW: &str =
    "http://example.org#g36.source.cooling_only_active_air_flow.VOccMin_flow";

const ACTIVE_COOLING_MAXIMUM_AIRFLOW_SOURCE: &str =
    "http://example.org#g36.source.cooling_only_active_air_flow.VActCooMax_flow";
const ACTIVE_MINIMUM_AIRFLOW_SOURCE: &str =
    "http://example.org#g36.source.cooling_only_active_air_flow.VActMin_flow";

const ACTIVE_COOLING_MAXIMUM_AIRFLOW_RUNTIME: &str = "conn#1";
const ACTIVE_MINIMUM_AIRFLOW_RUNTIME: &str = "conn#9";

const OPERATING_MODES: [i64; 14] = [1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7];
const OCCUPIED_MINIMUM_FLOWS: [f64; 14] = [
    0.0, 0.31, 0.31, 0.0, 0.18, 0.31, 0.0, 0.31, 0.12, 0.0, 0.31, 0.0, 0.22, 0.31,
];

const INPUTS: &[PointSpec] = &[
    PointSpec::integer(OPERATING_MODE),
    PointSpec::real(OCCUPIED_MINIMUM_AIRFLOW),
];
// Upstream ActiveAirFlow.mo connects VActMin_flow and VActHeaMax_flow from the same actMin.y
// signal (lines 104-107), so both source names resolve to conn#9. The determinism snapshot lists
// distinct runtime connectors and uses VActMin_flow as the canonical name. The facade, Tier-A exact,
// and funnel suites still assert VActHeaMax_flow bit-exactly under its own source name.
const OUTPUTS: &[PointSpec] = &[
    PointSpec::real_alias(
        ACTIVE_COOLING_MAXIMUM_AIRFLOW_SOURCE,
        ACTIVE_COOLING_MAXIMUM_AIRFLOW_RUNTIME,
    ),
    PointSpec::real_alias(
        ACTIVE_MINIMUM_AIRFLOW_SOURCE,
        ACTIVE_MINIMUM_AIRFLOW_RUNTIME,
    ),
];
const SPEC: SequenceSpec = SequenceSpec {
    name: "cooling_only_active_air_flow",
    cxf: ACTIVE_AIR_FLOW,
    t_stop: 13,
    sample_step: 1.0,
    inputs: INPUTS,
    outputs: OUTPUTS,
    input_fn: active_air_flow_inputs,
};

#[test]
fn cooling_only_active_air_flow_outputs_match_determinism_golden() {
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

fn active_air_flow_inputs(t: f64) -> Vec<(String, Value)> {
    let row = t as usize;
    assert!(row < OPERATING_MODES.len(), "unexpected input time {t}");
    vec![
        pair(OPERATING_MODE, Value::Integer(OPERATING_MODES[row])),
        pair(
            OCCUPIED_MINIMUM_AIRFLOW,
            Value::Real(OCCUPIED_MINIMUM_FLOWS[row]),
        ),
    ]
}
