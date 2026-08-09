//! G36 ThermalZones.ZoneStates Tier-2 determinism golden.
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

const ZONE_STATES: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/thermal_zones_zone_states.jsonld");

const HEATING_CONTROL: &str = "http://example.org#g36.source.thermal_zones_zone_states.uHea";
const COOLING_CONTROL: &str = "http://example.org#g36.source.thermal_zones_zone_states.uCoo";
const ZONE_STATE_SOURCE: &str = "http://example.org#g36.source.thermal_zones_zone_states.yZonSta";

const ROWS: usize = 44;
const INPUTS: &[PointSpec] = &[
    PointSpec::real(HEATING_CONTROL),
    PointSpec::real(COOLING_CONTROL),
];
const OUTPUTS: &[PointSpec] = &[PointSpec::integer(ZONE_STATE_SOURCE)];
const SPEC: SequenceSpec = SequenceSpec {
    name: "thermal_zones_zone_states",
    cxf: ZONE_STATES,
    t_stop: (ROWS - 1) as u32,
    sample_step: 60.0,
    inputs: INPUTS,
    outputs: OUTPUTS,
    input_fn: zone_state_inputs,
};

const INPUT_SCHEDULE: [(f64, f64); ROWS] = [
    (0.0, 0.0),
    // ZS4 heating signal strict arm, band hold, uLow equality, and release.
    (0.05, 0.0),
    (0.051, 0.0),
    (0.03, 0.0),
    (0.01, 0.0),
    (0.009, 0.0),
    // ZS4 cooling signal mirror probes.
    (0.0, 0.05),
    (0.0, 0.051),
    (0.0, 0.03),
    (0.0, 0.01),
    (0.0, 0.009),
    // ZS1-ZS3 simultaneous-demand tie-break histories and heating priority.
    (0.08, 0.06),
    (0.07, 0.065),
    (0.061, 0.07),
    (0.06, 0.071),
    (0.07, 0.065),
    (0.079, 0.07),
    (0.081, 0.07),
    (0.075, 0.075),
    (0.07, 0.081),
    (0.075, 0.075),
    (0.081, 0.07),
    // ZS3-ZS4 signal-latch releases under both priorities.
    (0.0, 0.03),
    (0.0, 0.01),
    (0.0, 0.009),
    (0.03, 0.0),
    (0.06, 0.0),
    (0.03, 0.0),
    (0.009, 0.0),
    // ZS2-ZS3 held simultaneous demands and combinational deadband.
    (0.07, 0.06),
    (0.04, 0.03),
    (0.009, 0.03),
    (0.009, 0.009),
    // ZS5 enum plateaus and all required transition pairs.
    (0.0, 0.0),
    (0.06, 0.0),
    (0.06, 0.0),
    (0.0, 0.0),
    (0.0, 0.06),
    (0.0, 0.06),
    (0.0, 0.0),
    (0.06, 0.08),
    (0.08, 0.06),
    (0.08, 0.06),
    (0.0, 0.0),
];

#[test]
fn g36_thermal_zones_zone_states_outputs_match_determinism_golden() {
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

fn zone_state_inputs(t: f64) -> Vec<(String, Value)> {
    let tick = (t / 60.0).round() as usize;
    assert!(tick < ROWS, "unexpected input time {t}");
    let (heating, cooling) = INPUT_SCHEDULE[tick];
    vec![
        pair(HEATING_CONTROL, Value::Real(heating)),
        pair(COOLING_CONTROL, Value::Real(cooling)),
    ]
}
