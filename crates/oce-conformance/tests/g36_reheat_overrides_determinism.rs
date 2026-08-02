//! G36 Reheat Overrides Tier-2 determinism golden.
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

const REHEAT_OVERRIDES: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/reheat_overrides.jsonld");

const DAMPER_OVERRIDE_INDEX: &str = "http://example.org#g36.source.reheat_overrides.oveDamPos";
const DAMPER_COMMAND_INPUT: &str = "http://example.org#g36.source.reheat_overrides.uDam";
const HEATING_VALVE_OFF: &str = "http://example.org#g36.source.reheat_overrides.uHeaOff";
const HEATING_VALVE_COMMAND_INPUT: &str = "http://example.org#g36.source.reheat_overrides.uVal";

const DAMPER_COMMAND_SOURCE: &str = "http://example.org#g36.source.reheat_overrides.yDam";
const HEATING_VALVE_COMMAND_SOURCE: &str = "http://example.org#g36.source.reheat_overrides.yVal";
const DAMPER_COMMAND_PATH: &str = "http://example.org#g36.source.reheat_overrides.swi1.y";
const HEATING_VALVE_COMMAND_PATH: &str = "http://example.org#g36.source.reheat_overrides.pro.y";

const DAMPER_OVERRIDE_INDICES: [i64; 12] = [1, 2, 0, 0, 2, 3, 1, 3, -1, 1, 2, 0];
const DAMPER_COMMANDS: [f64; 12] = [
    0.37, 0.37, 0.37, 0.37, 0.37, 0.37, 0.37, 0.37, 0.81, 0.81, 0.81, 0.81,
];
const HEATING_VALVE_OFF_STATES: [bool; 12] = [
    false, false, false, true, true, false, true, true, false, false, false, true,
];
const HEATING_VALVE_COMMANDS: [f64; 12] = [
    0.62, 0.62, 0.62, 0.62, 0.62, 0.62, 0.62, 0.62, 0.23, 0.23, 0.23, 0.23,
];

const INPUTS: &[PointSpec] = &[
    PointSpec::integer(DAMPER_OVERRIDE_INDEX),
    PointSpec::real(DAMPER_COMMAND_INPUT),
    PointSpec::boolean(HEATING_VALVE_OFF),
    PointSpec::real(HEATING_VALVE_COMMAND_INPUT),
];
const OUTPUTS: &[PointSpec] = &[
    PointSpec::real_alias(DAMPER_COMMAND_SOURCE, DAMPER_COMMAND_PATH),
    PointSpec::real_alias(HEATING_VALVE_COMMAND_SOURCE, HEATING_VALVE_COMMAND_PATH),
];
const SPEC: SequenceSpec = SequenceSpec {
    name: "reheat_overrides",
    cxf: REHEAT_OVERRIDES,
    t_stop: 11,
    sample_step: 1.0,
    inputs: INPUTS,
    outputs: OUTPUTS,
    input_fn: reheat_overrides_inputs,
};

#[test]
fn reheat_overrides_outputs_match_determinism_golden() {
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

fn reheat_overrides_inputs(t: f64) -> Vec<(String, Value)> {
    let row = t as usize;
    assert!(
        row < DAMPER_OVERRIDE_INDICES.len(),
        "unexpected input time {t}"
    );
    vec![
        pair(
            DAMPER_OVERRIDE_INDEX,
            Value::Integer(DAMPER_OVERRIDE_INDICES[row]),
        ),
        pair(DAMPER_COMMAND_INPUT, Value::Real(DAMPER_COMMANDS[row])),
        pair(
            HEATING_VALVE_OFF,
            Value::Boolean(HEATING_VALVE_OFF_STATES[row]),
        ),
        pair(
            HEATING_VALVE_COMMAND_INPUT,
            Value::Real(HEATING_VALVE_COMMANDS[row]),
        ),
    ]
}
