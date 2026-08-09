//! G36 ThermalZones.ControlLoops Tier-2 determinism golden.
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

const CONTROL_LOOPS: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/thermal_zones_control_loops.jsonld");

const COOLING_SETPOINT: &str = "http://example.org#g36.source.thermal_zones_control_loops.TCooSet";
const ZONE_TEMPERATURE: &str = "http://example.org#g36.source.thermal_zones_control_loops.TZon";
const HEATING_SETPOINT: &str = "http://example.org#g36.source.thermal_zones_control_loops.THeaSet";
const COOLING_LOOP_SOURCE: &str = "http://example.org#g36.source.thermal_zones_control_loops.yCoo";
const HEATING_LOOP_SOURCE: &str = "http://example.org#g36.source.thermal_zones_control_loops.yHea";

const ROWS: usize = 54;
const INPUTS: &[PointSpec] = &[
    PointSpec::real(COOLING_SETPOINT),
    PointSpec::real(ZONE_TEMPERATURE),
    PointSpec::real(HEATING_SETPOINT),
];
const OUTPUTS: &[PointSpec] = &[
    PointSpec::real(COOLING_LOOP_SOURCE),
    PointSpec::real(HEATING_LOOP_SOURCE),
];
const SPEC: SequenceSpec = SequenceSpec {
    name: "thermal_zones_control_loops",
    cxf: CONTROL_LOOPS,
    t_stop: (ROWS - 1) as u32,
    sample_step: 60.0,
    inputs: INPUTS,
    outputs: OUTPUTS,
    input_fn: control_loop_inputs,
};

const INPUT_SCHEDULE: [(f64, f64, f64); ROWS] = [
    // Neither loop enabled.
    (297.15, 295.15, 293.15),
    // CL1-CL3: cooling-only direct-acting reset and monotonic ramp.
    (297.15, 317.15, 293.15),
    (297.15, 317.15, 293.15),
    (297.15, 317.15, 293.15),
    (297.15, 317.15, 293.15),
    (297.15, 317.15, 293.15),
    (297.15, 317.15, 293.15),
    (297.15, 317.15, 293.15),
    (297.15, 317.15, 293.15),
    (297.15, 317.15, 293.15),
    // CL8: sustained high-limit saturation and back-calculation.
    (297.15, 317.15, 293.15),
    (297.15, 317.15, 293.15),
    (297.15, 317.15, 293.15),
    (297.15, 317.15, 293.15),
    (297.15, 317.15, 293.15),
    (297.15, 317.15, 293.15),
    (297.15, 317.15, 293.15),
    (297.15, 317.15, 293.15),
    (297.15, 317.15, 293.15),
    (297.15, 317.15, 293.15),
    (297.15, 317.15, 293.15),
    (297.15, 317.15, 293.15),
    (297.15, 317.15, 293.15),
    (297.15, 317.15, 293.15),
    (297.15, 317.15, 293.15),
    (297.15, 317.15, 293.15),
    (297.15, 317.15, 293.15),
    (297.15, 317.15, 293.15),
    // CL4-CL6: disabled cooling PI stays live before its downstream gate closes.
    (297.15, 295.15, 270.00),
    (297.15, 295.15, 270.00),
    (297.15, 295.15, 270.00),
    (297.15, 295.15, 270.00),
    (297.15, 287.15, 270.00),
    (297.15, 287.15, 270.00),
    (297.15, 287.15, 270.00),
    (297.15, 287.15, 270.00),
    (297.15, 287.15, 270.00),
    (297.15, 287.15, 270.00),
    // CL3/CL7: re-enable, arm near-zero, hold in-band, then release at t+h.
    (297.15, 297.65, 270.00),
    (297.15, 297.70, 270.00),
    (297.15, 297.74, 270.00),
    (297.15, 297.77, 270.00),
    // CL1/CL2: heating-only reverse-acting reset and monotonic ramp.
    (320.00, 283.15, 293.15),
    (320.00, 283.15, 293.15),
    (320.00, 283.15, 293.15),
    (320.00, 283.15, 293.15),
    (320.00, 283.15, 293.15),
    (320.00, 283.15, 293.15),
    // CL3: a one-tick disable pulse drops before the 30-second sampled delay expires.
    (320.00, 310.00, 293.15),
    (320.00, 283.15, 293.15),
    (320.00, 283.15, 293.15),
    (320.00, 283.15, 293.15),
    // Both loops enabled, then neither.
    (294.00, 295.00, 296.00),
    (297.15, 295.15, 293.15),
];

#[test]
fn g36_thermal_zones_control_loops_outputs_match_determinism_golden() {
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

fn control_loop_inputs(t: f64) -> Vec<(String, Value)> {
    let tick = (t / 60.0).round() as usize;
    assert!(tick < ROWS, "unexpected input time {t}");
    let (cooling_setpoint, zone_temperature, heating_setpoint) = INPUT_SCHEDULE[tick];
    vec![
        pair(COOLING_SETPOINT, Value::Real(cooling_setpoint)),
        pair(ZONE_TEMPERATURE, Value::Real(zone_temperature)),
        pair(HEATING_SETPOINT, Value::Real(heating_setpoint)),
    ]
}
