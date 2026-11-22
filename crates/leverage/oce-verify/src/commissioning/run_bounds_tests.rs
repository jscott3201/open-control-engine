use super::*;

#[test]
fn the_runs_work_is_held_to_sixteen_mebi_points_inclusive() {
    assert!(run_points_within(MAX_RUN_POINTS, "references[0]").is_ok());
    let refused = run_points_within(MAX_RUN_POINTS + 1, "references[0]").expect_err("one past");
    assert_eq!(refused.code(), "too-many-points");
    assert_eq!(refused.subject(), "references[0].outputs");
    assert_eq!(MAX_RUN_POINTS, 16_777_216);
}

#[test]
fn an_output_counts_its_instants_once_and_once_more_per_indicator() {
    assert_eq!(output_points(1_000, 0), 1_000);
    assert_eq!(output_points(1_000, 3), 4_000);
    // 2^20 instants with fifteen indicators are the run's whole budget, exactly; one more
    // indicator passes it.
    assert_eq!(output_points(1 << 20, 15), MAX_RUN_POINTS);
    assert!(run_points_within(output_points(1 << 20, 15), "r").is_ok());
    assert!(run_points_within(output_points(1 << 20, 16), "r").is_err());
    assert_eq!(output_points(u64::MAX, 64), u64::MAX);
}

#[test]
fn a_trace_row_is_replays_formula_and_the_trace_is_held_to_its_bytes_before_the_replay() {
    let input = |value_type| InputDefinition {
        path: "urn:x#top.u".to_owned(),
        value_type,
        min: None,
        max: None,
    };
    let inputs = [input(ValueType::Real), input(ValueType::Boolean)];
    let outputs = [ValueType::Real, ValueType::Integer, ValueType::Boolean];
    // 9 for the time and status, 17 for the outputs, 9 for the retained inputs.
    assert_eq!(bytes_per_frame(&inputs, &outputs, false), 35);
    // One substitution bit per input, rounded up to a byte, when an input can be held.
    assert_eq!(bytes_per_frame(&inputs, &outputs, true), 36);
    assert!(check_trace_bytes(10, 35, 350, "r").is_ok());
    let refused = check_trace_bytes(11, 35, 350, "r").expect_err("one row past");
    assert_eq!(
        (refused.code(), refused.cause()),
        ("replay-failed", Some("trace-too-large"))
    );
    assert!(check_trace_bytes(usize::MAX, u64::MAX, u64::MAX, "r").is_ok());
}
