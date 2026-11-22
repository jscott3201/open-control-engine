//! The `when` mask of a window run against a subject (vectors note, "Tolerance windows",
//! `when`; decision 46), a mutant caught by `tolerance-exceeded` (decision 44), and the trace's
//! inputs kept only where a mask reads one.
//!
//! The `signals` subject drives a `CDL.Utilities.Assert` from its Boolean input `b` and passes
//! `b` to the output `bOut`, so a mask can name the input or the output of the same signal.

mod support;

use oce_verify::{Failure, ScenarioReceipt, ScenarioStatus, VerifyOptions};

use support::{edited, only_scenario, receipt, refused, run_mutants, run_with, set_on, subject};

/// The `signals` subject's inputs with `ahu.T` and `b` held as given (JSON constants or step
/// lists).
fn inputs(ahu_t: &str, b: &str) -> String {
    format!(r#""ahu.T": {ahu_t}, "zone.T": 2.0, "n": 3, "b": {b}, "lim": 5.0"#)
}

/// One scenario on the 60 s clock to 1,800 s with the given inputs and expectations.
fn one_scenario(inputs: &str, expect: &str) -> String {
    set_on("60", "1800", &support::scenario(inputs, expect))
}

fn scenario_of(text: &str) -> ScenarioReceipt {
    only_scenario(&receipt(text, &subject("signals"))).clone()
}

/// `b` false until 600 s, then true.
const ENABLED_AT_600: &str = r#"[{"t": 0, "value": false}, {"t": 600, "value": true}]"#;

/// `ahu.T` 35 (off its expectation of 21) until 600 s, then 21.
const SETTLES_AT_600: &str = r#"[{"t": 0, "value": 35.0}, {"t": 600, "value": 21.0}]"#;

#[test]
fn a_mask_true_false_and_changing_counts_its_ticks_for_both_window_forms() {
    // (b, the masked ticks, then the checked ones, of 31)
    let cases = [("true", 0), ("false", 31), (ENABLED_AT_600, 10)];
    for (b, masked) in cases {
        for (signal, window) in [
            (
                "b",
                r#"{"output": "ahu.y", "from_s": 0, "to_s": 1800, "within": {"atoly": 0.5}, "of": 21.0, "when": ["b"]}"#,
            ),
            (
                "bOut",
                r#"{"output": "ahu.y", "from_s": 0, "to_s": 1800, "within": {"atoly": 0.5}, "of": 21.0, "when": ["bOut"]}"#,
            ),
            (
                "b",
                r#"{"output": "ahu.y", "from_s": 0, "to_s": 1800, "equals": 21.0, "when": ["b"]}"#,
            ),
            (
                "bOut",
                r#"{"output": "ahu.y", "from_s": 0, "to_s": 1800, "equals": 21.0, "when": ["bOut"]}"#,
            ),
        ] {
            let scenario = scenario_of(&one_scenario(&inputs(SETTLES_AT_600, b), window));
            let instants = scenario.instants;
            assert_eq!(instants.masked, masked, "{b} {signal} {window}");
            assert_eq!(instants.checked, 31 - masked, "{b} {signal}");
            assert_eq!(instants.passed + instants.failed, instants.checked);
            // The ten ticks before 600 s are off; only a mask covering them passes.
            let expected = match masked {
                31 => ScenarioStatus::NoData,
                10 => ScenarioStatus::Pass,
                _ => ScenarioStatus::Fail,
            };
            assert_eq!(scenario.status, expected, "{b} {signal} {window}");
            if masked == 0 {
                assert_eq!(instants.failed, 10);
            }
        }
    }
}

#[test]
fn a_mask_is_the_and_of_its_signals() {
    // b true from 600 s and bOut, its pass-through, true on the same ticks: their AND masks the
    // same ten ticks either masks alone, and naming one signal three times changes nothing.
    let window = |names: &str| {
        format!(
            r#"{{"output": "ahu.y", "from_s": 0, "to_s": 1800, "within": {{"atoly": 0.5}}, "of": 21.0, "when": [{names}]}}"#
        )
    };
    let both = scenario_of(&one_scenario(
        &inputs(SETTLES_AT_600, ENABLED_AT_600),
        &window(r#""b", "bOut""#),
    ));
    assert_eq!(
        (both.instants.masked, both.status),
        (10, ScenarioStatus::Pass)
    );
    let repeated = scenario_of(&one_scenario(
        &inputs(SETTLES_AT_600, ENABLED_AT_600),
        &window(r#""b", "b", "b""#),
    ));
    assert_eq!(repeated.instants, both.instants);
}

/// One `CDL.Logical.Not`: Boolean input `u`, Boolean output `y`, always opposite.
const NOT: &str = r#"{"@context": {"S231": "http://data.ashrae.org/S231#"}, "@graph": [
  {"@id": "urn:doc#top", "@type": "S231:Block",
   "S231:containsBlock": [{"@id": "urn:doc#top.not"}],
   "S231:hasInput": [{"@id": "urn:doc#top.u"}], "S231:hasOutput": [{"@id": "urn:doc#top.y"}]},
  {"@id": "urn:doc#top.not", "@type": "CDL.Logical.Not",
   "S231:hasInput": {"@id": "urn:doc#top.not.u"}, "S231:hasOutput": {"@id": "urn:doc#top.not.y"}},
  {"@id": "urn:doc#top.not.u", "@type": "S231:BooleanInput",
   "S231:isOfDataType": {"@id": "S231:Boolean"}},
  {"@id": "urn:doc#top.not.y", "@type": "S231:BooleanOutput",
   "S231:isOfDataType": {"@id": "S231:Boolean"}, "S231:isConnectedTo": {"@id": "urn:doc#top.y"}},
  {"@id": "urn:doc#top.u", "@type": "S231:BooleanInput",
   "S231:isOfDataType": {"@id": "S231:Boolean"}, "S231:isConnectedTo": {"@id": "urn:doc#top.not.u"}},
  {"@id": "urn:doc#top.y", "@type": "S231:BooleanOutput",
   "S231:isOfDataType": {"@id": "S231:Boolean"}}
]}"#;

#[test]
fn a_mask_of_two_signals_that_are_never_true_together_masks_every_tick() {
    // `u` and `y` are opposite at every tick, so `when: [u, y]` is false everywhere, `[u]` and
    // `[y]` each true where the other is false.
    let u = r#"[{"t": 0, "value": false}, {"t": 600, "value": true}]"#;
    let window = |names: &str| {
        set_on(
            "60",
            "1800",
            &support::scenario(
                &format!(r#""u": {u}"#),
                &format!(
                    r#"{{"output": "y", "from_s": 0, "to_s": 1800, "equals": false, "when": [{names}]}}"#
                ),
            ),
        )
    };
    let counts = |names: &str| {
        let scenario = only_scenario(&receipt(&window(names), NOT.as_bytes())).clone();
        (
            scenario.instants.masked,
            scenario.instants.checked,
            scenario.instants.failed,
        )
    };
    assert_eq!(counts(r#""u", "y""#), (31, 0, 0));
    assert_eq!(counts(r#""y", "u""#), (31, 0, 0));
    // y is false exactly where u is true: masked by u, every checked tick passes.
    assert_eq!(counts(r#""u""#), (10, 21, 0));
    // Masked by y, the checked ticks are those where y is true, and each fails `equals false`.
    assert_eq!(counts(r#""y""#), (21, 10, 10));
}

#[test]
fn a_masked_tick_zeroes_both_series_before_the_envelope_is_built() {
    // The rounding case of decision 41 sits on ticks 0 and 60 s of the window; masked there,
    // both series are zero, so the expectation no longer leaves its own funnel and nothing is
    // refused. Unmasked, the same window is `compare-refused`.
    let of = r#"[{"t": 0, "value": 2e6}, {"t": 120, "value": 0}]"#;
    let window = |when: &str| {
        format!(
            r#"{{"output": "ahu.y", "from_s": 0, "to_s": 240, "within": {{"atolx": 60}}, "of": {of}{when}}}"#
        )
    };
    let b = r#"[{"t": 0, "value": false}, {"t": 120, "value": true}]"#;
    let ahu_t = r#"[{"t": 0, "value": 5.0}, {"t": 120, "value": 0.0}]"#;
    let masked = scenario_of(&one_scenario(
        &inputs(ahu_t, b),
        &window(r#", "when": ["b"]"#),
    ));
    assert_eq!(masked.instants.masked, 2);
    assert_eq!(masked.instants.checked, 3);
    assert_eq!(masked.status, ScenarioStatus::Pass);
    let error = refused(
        &one_scenario(&inputs(ahu_t, b), &window("")),
        &subject("signals"),
    );
    assert_eq!(
        (error.code(), error.cause()),
        ("compare-refused", Some("reference-outside-envelope"))
    );
}

#[test]
fn an_out_of_range_output_at_a_masked_tick_fails_nothing() {
    // ratio = zone.T / ahu.T is 0 / 0 until 600 s, masked there.
    let text = one_scenario(
        r#""ahu.T": [{"t": 0, "value": 0.0}, {"t": 600, "value": 2.0}],
           "zone.T": [{"t": 0, "value": 0.0}, {"t": 600, "value": 1.0}],
           "n": 3, "b": [{"t": 0, "value": false}, {"t": 600, "value": true}], "lim": 5.0"#,
        r#"{"output": "ratio", "from_s": 0, "to_s": 1800, "within": {"atoly": 0.01}, "of": 0.5, "when": ["b"]}"#,
    );
    let scenario = scenario_of(&text);
    assert_eq!(scenario.status, ScenarioStatus::Pass);
    assert_eq!(
        (scenario.instants.masked, scenario.instants.checked),
        (10, 21)
    );
}

#[test]
fn the_trace_keeps_the_inputs_only_where_a_mask_names_one() {
    let input_mask = one_scenario(
        &inputs("21.0", "true"),
        r#"{"output": "ahu.y", "from_s": 0, "to_s": 60, "equals": 21.0, "when": ["b"]}"#,
    );
    let output_mask = input_mask.replace(r#"["b"]"#, r#"["bOut"]"#);
    let no_mask = input_mask.replace(r#", "when": ["b"]"#, "");
    let options = VerifyOptions {
        keep_traces: true,
        ..VerifyOptions::default()
    };
    let kept = |text: &str| {
        let run = run_with(text, &subject("signals"), &options).expect("runs");
        run.traces[0].1.inputs().map(<[_]>::len)
    };
    assert_eq!(kept(&input_mask), Some(5));
    assert_eq!(kept(&output_mask), None);
    assert_eq!(kept(&no_mask), None);
    // Keeping them changes no receipt byte but the ones the mask decides.
    let a = receipt(&input_mask, &subject("signals"));
    let b = receipt(&output_mask, &subject("signals"));
    assert_eq!(only_scenario(&a).instants, only_scenario(&b).instants);
}

#[test]
fn a_mutant_is_caught_by_tolerance_exceeded() {
    // The mutant drives ahu.y from zone.T instead of ahu.T: 2 where the expectation is 21.
    let set = format!(
        r#"{{"schema": "oce/vectors/v1", "clock": {{"step_s": 60, "horizon_s": 1800}},
           "mutants": [{{"path": "m/swapped.cxf.json"}}],
           "scenarios": [{}]}}"#,
        support::scenario(
            &inputs("21.0", "true"),
            r#"{"output": "ahu.y", "from_s": 0, "to_s": 1800, "within": {"atoly": 0.5, "atolx": 60}, "of": 21.0}"#,
        )
    );
    let signals = subject("signals");
    let mutant = edited(
        &signals,
        &[
            (
                r#"[{"@id": "urn:verify:signals#top.div.u2"}, {"@id": "urn:verify:signals#top.ahu.y"}]"#,
                r#"[{"@id": "urn:verify:signals#top.div.u2"}]"#,
            ),
            (
                r#"{"@id": "urn:verify:signals#top.zone.y"}]"#,
                r#"{"@id": "urn:verify:signals#top.zone.y"}, {"@id": "urn:verify:signals#top.ahu.y"}]"#,
            ),
        ],
    );
    let run =
        run_mutants(&set, &signals, &[("m/swapped.cxf.json".to_owned(), mutant)]).expect("runs");
    let body = support::body(&run.receipt);
    let section = body.mutants.as_ref().expect("scored");
    assert_eq!((section.caught, section.total), (1, 1));
    let caught = section.entries[0].caught_by.as_ref().expect("caught");
    assert!(
        matches!(caught.failure, Failure::ToleranceExceeded { .. }),
        "{:?}",
        caught.failure
    );
    assert_eq!(caught.failure.code(), "tolerance-exceeded");
}
