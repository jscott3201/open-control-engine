//! G36 Generic.TimeSuppression Tier-2 determinism golden.
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

const TIME_SUPPRESSION: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/generic_time_suppression.jsonld");

const SETPOINT_TEMPERATURE: &str = "http://example.org#g36.source.generic_time_suppression.TSet";
const ZONE_TEMPERATURE: &str = "http://example.org#g36.source.generic_time_suppression.TZon";
const AFTER_SUPPRESSION_SOURCE: &str =
    "http://example.org#g36.source.generic_time_suppression.yAftSup";

const ROWS: usize = 91;
const INPUTS: &[PointSpec] = &[
    PointSpec::real(SETPOINT_TEMPERATURE),
    PointSpec::real(ZONE_TEMPERATURE),
];
const OUTPUTS: &[PointSpec] = &[PointSpec::boolean(AFTER_SUPPRESSION_SOURCE)];
const SPEC: SequenceSpec = SequenceSpec {
    name: "generic_time_suppression",
    cxf: TIME_SUPPRESSION,
    t_stop: (ROWS - 1) as u32,
    sample_step: 60.0,
    inputs: INPUTS,
    outputs: OUTPUTS,
    input_fn: time_suppression_inputs,
};

#[test]
fn g36_generic_time_suppression_outputs_match_determinism_golden() {
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

fn time_suppression_inputs(t: f64) -> Vec<(String, Value)> {
    let tick = (t / 60.0).round() as usize;
    assert!(tick < ROWS, "unexpected input time {t}");
    let (setpoint, zone) = match tick {
        0 => (294.15, 294.15),
        1 => (295.15, 294.15),
        2..=5 => (295.15, 294.15),
        6..=7 => (296.15, 294.15),
        8..=11 => (296.35, 294.15),
        12..=14 => (297.15, 294.15),
        15..=22 => (298.45, 297.95),
        23..=47 => (298.65, 297.95),
        48..=90 => (302.65, 298.15),
        _ => unreachable!(),
    };
    vec![
        pair(SETPOINT_TEMPERATURE, Value::Real(setpoint)),
        pair(ZONE_TEMPERATURE, Value::Real(zone)),
    ]
}
