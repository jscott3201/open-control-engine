//! Binding a set to its subject (vectors note, "Binding and running"; decisions 6 and 7): every
//! name by the one rule (the exact canonical path, or the one path the name ends at a `.`
//! boundary), every value typed by what it lands on, and every refusal an error before any frame
//! runs, never a failed scenario.
//!
//! The subject is `tests/fixtures/subjects/signals.cxf.json` (see `support::SIGNALS_INPUTS`): five
//! boundary inputs, two of which share the last segment `T`, and six outputs, two of which share
//! the last segment `y`.

mod support;

use oce_replay::ReplayLimits;
use oce_verify::{Failure, ReceiptValue, ScenarioStatus, VerifyLimits, VerifyOptions};

use support::{SIGNALS, SIGNALS_INPUTS, one, only_scenario, receipt, refused, run_with, subject};

fn signals() -> Vec<u8> {
    subject("signals")
}

/// A window over the whole clock.
fn window(output: &str, equals: &str) -> String {
    format!(r#"{{"output": "{output}", "from_s": 0, "to_s": 1800, "equals": {equals}}}"#)
}

/// The code, the subject and the canonical path of the refusal of `inputs` and `expect`.
fn refusal(inputs: &str, expect: &str) -> (&'static str, String, Option<String>) {
    let error = refused(&one(inputs, expect), &signals());
    (
        error.code(),
        error.subject().to_owned(),
        error.path().map(str::to_owned),
    )
}

#[test]
fn inputs_and_outputs_resolve_by_exact_path_or_a_unique_suffix_to_the_right_slots() {
    // `zone.T` by its full path, `ahu.T` by a two-segment suffix; each value reaches its own
    // pass-through output, and `ratio` is their quotient.
    let inputs = format!(r#""{SIGNALS}zone.T": 6.0, "ahu.T": 3.0, "n": 3, "b": true, "lim": 5.0"#);
    let expect = [
        window(&format!("{SIGNALS}zone.y"), "6.0"),
        window("ahu.y", "3.0"),
        window("ratio", "2.0"),
        window("nOut", "3"),
        window("bOut", "true"),
        window("limOut", "5.0"),
    ]
    .join(", ");
    let receipt = receipt(&one(&inputs, &expect), &signals());
    let scenario = only_scenario(&receipt);
    assert_eq!(scenario.status, ScenarioStatus::Pass, "{scenario:?}");
    assert_eq!(scenario.instants.checked, 6 * 31);
}

#[test]
fn a_suffix_shared_by_two_inputs_or_two_outputs_is_ambiguous_with_both_candidates() {
    let error = refused(
        &one(r#""T": 1.0, "n": 3, "b": true, "lim": 5.0"#, ""),
        &signals(),
    );
    assert_eq!(error.code(), "ambiguous-input");
    assert_eq!(error.subject(), "scenarios[0].inputs.T");
    assert_eq!(
        error.candidates(),
        [format!("{SIGNALS}ahu.T"), format!("{SIGNALS}zone.T")]
    );
    let error = refused(&one(SIGNALS_INPUTS, &window("y", "1.0")), &signals());
    assert_eq!(error.code(), "ambiguous-output");
    assert_eq!(error.subject(), "scenarios[0].expect[0].output");
    assert_eq!(
        error.candidates(),
        [format!("{SIGNALS}ahu.y"), format!("{SIGNALS}zone.y")]
    );
}

#[test]
fn a_name_that_matches_nothing_is_unknown_at_a_dot_boundary_only() {
    for name in [
        // Declared in the document but read by nothing, so not a boundary input (the load
        // warns `unused-boundary-input` about it instead).
        "spare", // A suffix that is not at a `.` boundary.
        "one.T", "im",
        // `#` is not a boundary: the root name with the last segment is not a suffix.
        "top.n", // An output is not an input.
        "nOut",
    ] {
        let (code, at, _) = refusal(&format!(r#"{SIGNALS_INPUTS}, "{name}": 1.0"#), "");
        assert_eq!(
            (code, at.as_str()),
            (
                "unknown-input",
                format!("scenarios[0].inputs.{name}").as_str()
            )
        );
    }
    for name in ["Out", "lim", "div.y", "chk.u", "top.ratio"] {
        let (code, at, _) = refusal(SIGNALS_INPUTS, &window(name, "1.0"));
        assert_eq!(
            (code, at.as_str()),
            ("unknown-output", "scenarios[0].expect[0].output"),
            "{name}"
        );
    }
}

#[test]
fn two_names_for_one_input_are_refused_and_two_windows_on_one_output_are_not() {
    let (code, at, path) = refusal(&format!(r#"{SIGNALS_INPUTS}, "{SIGNALS}n": 3"#), "");
    assert_eq!(code, "duplicate-input");
    // The names are taken in byte order, so the full path is the second.
    assert_eq!(at, format!("scenarios[0].inputs.{SIGNALS}n"));
    assert_eq!(path, Some(format!("{SIGNALS}n")));
    // An output may be named by several windows, by any spelling.
    let expect = [
        window("nOut", "3"),
        window(&format!("{SIGNALS}nOut"), "3"),
        window("nOut", "3"),
    ]
    .join(", ");
    let receipt = receipt(&one(SIGNALS_INPUTS, &expect), &signals());
    assert_eq!(only_scenario(&receipt).instants.checked, 3 * 31);
}

#[test]
fn every_input_must_be_supplied_and_the_first_unsupplied_in_slot_order_is_named() {
    let (code, at, path) = refusal(r#""ahu.T": 1.0, "zone.T": 2.0, "n": 3"#, "");
    assert_eq!(code, "input-not-supplied");
    assert_eq!(at, "scenarios[0].inputs");
    // Slot order is path order: `b` sorts before `lim`.
    assert_eq!(path, Some(format!("{SIGNALS}b")));
    // An output no window names is not an error.
    let receipt = receipt(&one(SIGNALS_INPUTS, &window("nOut", "3")), &signals());
    assert_eq!(only_scenario(&receipt).status, ScenarioStatus::Pass);
}

#[test]
fn the_integer_token_minus_zero_is_positive_zero_on_a_real_and_minus_zero_point_zero_is_not() {
    // `-0` lands as `+0.0`: it equals the output `0` and not `-0.0`.
    let zero = receipt(
        &one(
            r#""ahu.T": 1.0, "zone.T": -0, "n": 3, "b": true, "lim": 5.0"#,
            &[window("zone.y", "0"), window("zone.y", "-0.0")].join(", "),
        ),
        &signals(),
    );
    let scenario = only_scenario(&zero);
    assert_eq!(
        (scenario.instants.passed, scenario.instants.failed),
        (31, 31)
    );
    assert!(
        scenario
            .failures
            .iter()
            .all(|failure| matches!(failure, Failure::ValueMismatch { expectation: 1, .. }))
    );
    // `-0.0` keeps its sign through the engine: it equals `-0.0` and not `0`.
    let negative = receipt(
        &one(
            r#""ahu.T": 1.0, "zone.T": -0.0, "n": 3, "b": true, "lim": 5.0"#,
            &[window("zone.y", "-0.0"), window("zone.y", "0")].join(", "),
        ),
        &signals(),
    );
    let scenario = only_scenario(&negative);
    assert_eq!(
        (scenario.instants.passed, scenario.instants.failed),
        (31, 31)
    );
    let Failure::ValueMismatch {
        expectation,
        actual,
        expected,
        ..
    } = &scenario.failures[0]
    else {
        panic!("{:?}", scenario.failures);
    };
    assert_eq!(*expectation, 1);
    assert!(actual.bit_eq(&ReceiptValue::Real(-0.0)));
    assert!(expected.bit_eq(&ReceiptValue::Real(0.0)));
}

#[test]
fn an_integer_takes_an_integer_token_and_refuses_one_point_zero() {
    let (code, at, path) = refusal(
        r#""ahu.T": 1.0, "zone.T": 2.0, "n": 1.0, "b": true, "lim": 5.0"#,
        "",
    );
    assert_eq!(
        (code, at.as_str(), path),
        (
            "value-type-mismatch",
            "scenarios[0].inputs.n",
            Some(format!("{SIGNALS}n"))
        )
    );
    // In a step list the refusal names the step.
    let (code, at, _) = refusal(
        r#""ahu.T": 1.0, "zone.T": 2.0, "n": [{"t": 0, "value": 3}, {"t": 60, "value": 4.0}], "b": true, "lim": 5.0"#,
        "",
    );
    assert_eq!(
        (code, at.as_str()),
        ("value-type-mismatch", "scenarios[0].inputs.n[1].value")
    );
}

#[test]
fn an_integer_token_past_two_to_the_fifty_three_lands_on_a_real_only_when_exact() {
    let (code, at, _) = refusal(
        r#""ahu.T": 1.0, "zone.T": 9007199254740993, "n": 3, "b": true, "lim": 5.0"#,
        "",
    );
    assert_eq!(
        (code, at.as_str()),
        ("value-not-representable", "scenarios[0].inputs.zone.T")
    );
    // 2^53 and 2^53 + 2 are binary64s; each reaches the output with its exact bits.
    for (token, value) in [
        ("9007199254740992", 9_007_199_254_740_992.0_f64),
        ("9007199254740994", 9_007_199_254_740_994.0_f64),
    ] {
        let receipt = receipt(
            &one(
                &format!(r#""ahu.T": 1.0, "zone.T": {token}, "n": 3, "b": true, "lim": 5.0"#),
                &window("zone.y", &format!("{value:?}")),
            ),
            &signals(),
        );
        assert_eq!(
            only_scenario(&receipt).status,
            ScenarioStatus::Pass,
            "{token}"
        );
    }
}

#[test]
fn a_boolean_takes_true_or_false_and_a_real_takes_no_boolean() {
    for (inputs, at) in [
        (
            r#""ahu.T": 1.0, "zone.T": 2.0, "n": 3, "b": 1, "lim": 5.0"#,
            "scenarios[0].inputs.b",
        ),
        (
            r#""ahu.T": true, "zone.T": 2.0, "n": 3, "b": true, "lim": 5.0"#,
            "scenarios[0].inputs.ahu.T",
        ),
    ] {
        let (code, subject, _) = refusal(inputs, "");
        assert_eq!((code, subject.as_str()), ("value-type-mismatch", at));
    }
}

#[test]
fn an_equals_of_the_wrong_type_is_refused_before_any_frame_runs() {
    // A trace bound of zero bytes would fail the first replay to run; the refusal is the
    // binder's, so no replay ran.
    let options = VerifyOptions {
        limits: VerifyLimits {
            replay: ReplayLimits {
                max_trace_bytes: 0,
                ..ReplayLimits::MAX
            },
            ..VerifyLimits::MAX
        },
        ..VerifyOptions::default()
    };
    for (output, equals) in [("nOut", "3.0"), ("bOut", "0"), ("ratio", "false")] {
        let set = support::set_of(
            &[
                support::named("first", SIGNALS_INPUTS, &window("nOut", "3")),
                support::named("second", SIGNALS_INPUTS, &window(output, equals)),
            ]
            .join(", "),
        );
        let error = run_with(&set, &signals(), &options).expect_err("refused at bind");
        assert_eq!(
            error.code(),
            "value-type-mismatch",
            "{output} equals {equals}"
        );
        assert_eq!(error.subject(), "scenarios[1].expect[0].equals");
        assert_eq!(error.path(), Some(format!("{SIGNALS}{output}").as_str()));
    }
    // The control: the same options refuse a set that binds, in its first replay.
    let set = one(SIGNALS_INPUTS, &window("nOut", "3"));
    let error = run_with(&set, &signals(), &options).expect_err("the replay fails");
    assert_eq!(
        (error.code(), error.cause()),
        ("replay-failed", Some("trace-too-large"))
    );
    // An integer token on a Real output is taken when exact.
    let receipt = receipt(&one(SIGNALS_INPUTS, &window("ratio", "2")), &signals());
    assert_eq!(only_scenario(&receipt).status, ScenarioStatus::Pass);
}
