//! Verdicts, counts and records (vectors note, "After a scenario's run", "The receipt";
//! decisions 8, 11, 12, 22, 23, 24 and 26), on the `signals` subject (see
//! `support::SIGNALS_INPUTS`).

mod support;

use oce_replay::EngineRefusal;
use oce_verify::{
    Failure, MAX_LOAD_WARNINGS, ReceiptValue, ScenarioStatus, VerifyLimits, VerifyOptions,
};

use support::{
    SIGNALS, SIGNALS_INPUTS, body, named, one, only_scenario, receipt, run_with, set_of, subject,
};

const ASSERT: &str = "CDL.Utilities.Assert";

fn signals() -> Vec<u8> {
    subject("signals")
}

fn window(output: &str, from_s: u32, to_s: u32, equals: &str) -> String {
    format!(r#"{{"output": "{output}", "from_s": {from_s}, "to_s": {to_s}, "equals": {equals}}}"#)
}

fn diagnostic(at_s: u32, message: Option<&str>) -> String {
    let message = message.map_or_else(String::new, |text| format!(r#", "message": "{text}""#));
    format!(r#"{{"diagnostic": {{"block": "{ASSERT}"{message}}}, "at_s": {at_s}}}"#)
}

/// The signals inputs with `b` false, so the assertion warns on every frame.
const B_FALSE: &str = r#""ahu.T": 1.0, "zone.T": 2.0, "n": 3, "b": false, "lim": 5.0"#;

#[test]
fn a_scenario_with_only_a_diagnostic_expectation_is_not_no_data() {
    let receipt = receipt(
        &one(B_FALSE, &diagnostic(60, Some("b is false"))),
        &signals(),
    );
    let scenario = only_scenario(&receipt);
    assert_eq!(scenario.status, ScenarioStatus::Pass);
    assert_eq!(
        (scenario.instants.checked, scenario.instants.passed),
        (1, 1),
        "one diagnostic is one instant"
    );
    // The warning at 60 s is matched; the other 30 are listed up to the cap and counted.
    assert_eq!(scenario.warnings_total, 30);
    assert_eq!(scenario.warnings.len(), 16);
    assert_eq!(scenario.warnings[0].t.to_bits(), 0.0_f64.to_bits());
    assert_eq!(scenario.warnings[1].t.to_bits(), 120.0_f64.to_bits());
    assert!(
        scenario
            .warnings
            .iter()
            .all(|warning| warning.block == ASSERT && warning.message == "b is false")
    );
    assert_eq!(body(&receipt).totals.warnings, 30);
    // The message is optional; without it the class alone matches.
    let any_message = support::receipt(&one(B_FALSE, &diagnostic(1800, None)), &signals());
    assert_eq!(only_scenario(&any_message).status, ScenarioStatus::Pass);
}

#[test]
fn a_diagnostic_the_frame_did_not_emit_fails_with_its_record() {
    // With `b` true nothing warns; with `b` false every frame warns `b is false`, which does not
    // equal the expected text, so all 31 warnings stay unmatched.
    for (inputs, message, unmatched) in [
        (SIGNALS_INPUTS, Some("b is false"), 0),
        (B_FALSE, Some("b is not false"), 31),
    ] {
        let receipt = receipt(&one(inputs, &diagnostic(60, message)), &signals());
        let scenario = only_scenario(&receipt);
        assert_eq!(scenario.status, ScenarioStatus::Fail);
        assert_eq!(scenario.warnings_total, unmatched);
        assert_eq!(
            (scenario.instants.checked, scenario.instants.failed),
            (1, 1)
        );
        let Failure::DiagnosticMissing {
            expectation,
            t,
            block,
            message: expected,
        } = &scenario.failures[0]
        else {
            panic!("{:?}", scenario.failures);
        };
        assert_eq!(
            (*expectation, t.to_bits(), block.as_str()),
            (0, 60.0_f64.to_bits(), ASSERT)
        );
        assert_eq!(expected.as_deref(), message);
    }
}

#[test]
fn unmatched_warnings_never_change_a_verdict_and_keep_their_total_past_the_cap() {
    let receipt = receipt(&one(B_FALSE, &window("bOut", 0, 1800, "false")), &signals());
    let scenario = only_scenario(&receipt);
    assert_eq!(scenario.status, ScenarioStatus::Pass);
    assert_eq!((scenario.warnings.len(), scenario.warnings_total), (16, 31));
    // In row order: the first 16 frames, 0 s to 900 s.
    let times: Vec<u64> = scenario.warnings.iter().map(|w| w.t.to_bits()).collect();
    #[expect(clippy::cast_precision_loss, reason = "small tick indexes")]
    let expected: Vec<u64> = (0..16_u64).map(|k| (k as f64 * 60.0).to_bits()).collect();
    assert_eq!(times, expected);
    // A lowered cap lists fewer and counts the same.
    let options = VerifyOptions {
        limits: VerifyLimits {
            max_recorded_warnings: 2,
            ..VerifyLimits::MAX
        },
        ..VerifyOptions::default()
    };
    let lowered = run_with(
        &one(B_FALSE, &window("bOut", 0, 1800, "false")),
        &signals(),
        &options,
    )
    .expect("a receipt")
    .receipt;
    let scenario = only_scenario(&lowered);
    assert_eq!((scenario.warnings.len(), scenario.warnings_total), (2, 31));
}

#[test]
fn a_frame_refused_mid_scenario_fails_it_without_counting_later_ticks_or_checking_coverage() {
    // `lim` is bounded to [0, 10]; the step to 11 at 120 s is refused by the engine.
    let inputs = r#""ahu.T": 1.0, "zone.T": 2.0, "n": 3, "b": true, "lim": [{"t": 0, "value": 5.0}, {"t": 120, "value": 11.0}]"#;
    let expect = [
        window("limOut", 0, 1800, "5.0"),
        // Entirely after the refused tick: uncounted, and not an uncovered window either.
        window("limOut", 300, 600, "5.0"),
        diagnostic(600, None),
    ]
    .join(", ");
    let receipt = receipt(&one(inputs, &expect), &signals());
    let scenario = only_scenario(&receipt);
    assert_eq!(scenario.status, ScenarioStatus::Fail);
    assert_eq!(
        (
            scenario.frames.executed,
            scenario.frames.substituted,
            scenario.frames.refused
        ),
        (2, 0, 1)
    );
    // Ticks 0 and 60 s were compared; the refused tick and every later one were not.
    assert_eq!(
        (
            scenario.instants.checked,
            scenario.instants.passed,
            scenario.instants.failed
        ),
        (2, 2, 0)
    );
    assert_eq!(scenario.failures_total, 1);
    let Failure::FrameRefused { t, refusal } = &scenario.failures[0] else {
        panic!("{:?}", scenario.failures);
    };
    assert_eq!(t.to_bits(), 120.0_f64.to_bits());
    assert_eq!(*refusal, EngineRefusal::InputDomain);
    let text = receipt.to_canonical_json();
    assert!(text.contains("\"refusal\": \"input-domain\""), "{text}");
}

#[test]
fn a_refused_first_frame_fails_the_scenario_rather_than_leaving_it_without_data() {
    // An Integer input carries the CDL Integer range as its bounds: 2^31 is outside it.
    let inputs = r#""ahu.T": 1.0, "zone.T": 2.0, "n": 2147483648, "b": true, "lim": 5.0"#;
    let receipt = receipt(&one(inputs, &window("nOut", 0, 1800, "3")), &signals());
    let scenario = only_scenario(&receipt);
    assert_eq!(scenario.status, ScenarioStatus::Fail);
    assert_eq!(scenario.instants.checked, 0);
    assert_eq!((scenario.frames.executed, scenario.frames.refused), (0, 1));
    assert!(matches!(
        scenario.failures[..],
        [Failure::FrameRefused { t, refusal: EngineRefusal::InputDomain }] if t.to_bits() == 0
    ));
}

#[test]
fn a_negative_zero_and_a_nan_output_are_recorded_by_their_bits() {
    // `zone.y` passes `-0.0` through, and `ratio` is `-0.0 / 0.0`, a NaN.
    let inputs = r#""ahu.T": 0.0, "zone.T": -0.0, "n": 3, "b": true, "lim": 5.0"#;
    let expect = [
        window("zone.y", 0, 0, "0"),
        window("ratio", 0, 0, "0.0"),
        // The same outputs match their own bits.
        window("zone.y", 0, 0, "-0.0"),
    ]
    .join(", ");
    let receipt = receipt(&one(inputs, &expect), &signals());
    let scenario = only_scenario(&receipt);
    assert_eq!(
        (scenario.instants.checked, scenario.instants.failed),
        (3, 2)
    );
    let actuals: Vec<(u64, u64)> = scenario
        .failures
        .iter()
        .map(|failure| match failure {
            Failure::ValueMismatch {
                expectation,
                actual: ReceiptValue::Real(x),
                ..
            } => (*expectation, x.to_bits()),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(actuals[0], (0, (-0.0_f64).to_bits()));
    assert_eq!(actuals[1].0, 1);
    assert!(f64::from_bits(actuals[1].1).is_nan());
    let text = receipt.to_canonical_json();
    assert!(
        text.contains("\"actual\": \"0x8000000000000000\""),
        "{text}"
    );
    assert!(
        text.contains("\"actual\": \"0x7ff8000000000000\""),
        "{text}"
    );
    assert!(text.contains("\"expected\": 0.0"), "{text}");
}

#[test]
fn failures_are_recorded_by_tick_then_expectation_up_to_the_cap_with_the_total() {
    // `bOut` and `nOut` both fail on every one of the 31 ticks.
    let expect = [
        window("bOut", 0, 1800, "false"),
        window("nOut", 0, 1800, "4"),
    ]
    .join(", ");
    let receipt = receipt(&one(SIGNALS_INPUTS, &expect), &signals());
    let scenario = only_scenario(&receipt);
    assert_eq!(scenario.failures_total, 62);
    assert_eq!(scenario.failures.len(), 16);
    let order: Vec<(u64, u64)> = scenario
        .failures
        .iter()
        .map(|failure| match failure {
            Failure::ValueMismatch { expectation, t, .. } => (t.to_bits(), *expectation),
            other => panic!("{other:?}"),
        })
        .collect();
    #[expect(clippy::cast_precision_loss, reason = "small tick indexes")]
    let expected: Vec<(u64, u64)> = (0..8_u64)
        .flat_map(|k| {
            [
                ((k as f64 * 60.0).to_bits(), 0),
                ((k as f64 * 60.0).to_bits(), 1),
            ]
        })
        .collect();
    assert_eq!(order, expected);
    assert_eq!(body(&receipt).totals.failures, 62);
    let Failure::ValueMismatch {
        output,
        expected,
        actual,
        ..
    } = &scenario.failures[1]
    else {
        unreachable!()
    };
    assert_eq!(output, &format!("{SIGNALS}nOut"));
    assert!(expected.bit_eq(&ReceiptValue::Integer(4)));
    assert!(actual.bit_eq(&ReceiptValue::Integer(3)));
}

#[test]
fn load_warnings_are_listed_in_load_order_up_to_their_cap_with_the_total() {
    let one_warning = receipt(&one(SIGNALS_INPUTS, ""), &signals());
    assert_eq!(body(&one_warning).load_warnings, ["unused-boundary-input"]);
    assert_eq!(body(&one_warning).load_warnings_total, 1);
    // A subject with 65 unused boundary inputs warns 65 times; 64 are listed.
    let many = receipt(&one(r#""u": true"#, ""), &unused_inputs(65).into_bytes());
    let body = body(&many);
    assert_eq!(body.load_warnings.len(), MAX_LOAD_WARNINGS);
    assert_eq!(body.load_warnings_total, 65);
    assert!(
        body.load_warnings
            .iter()
            .all(|code| code == "unused-boundary-input")
    );
}

/// A subject with one Boolean input through a `Not` and `count` unused boundary inputs.
fn unused_inputs(count: usize) -> String {
    let ns = "urn:verify:unused#top";
    let mut inputs = vec![format!(r#"{{"@id": "{ns}.u"}}"#)];
    let mut nodes = Vec::new();
    for index in 0..count {
        inputs.push(format!(r#"{{"@id": "{ns}.spare{index:03}"}}"#));
        nodes.push(format!(
            r#"{{"@id": "{ns}.spare{index:03}", "@type": "S231:RealInput", "S231:isOfDataType": {{"@id": "S231:Real"}}}}"#
        ));
    }
    format!(
        r#"{{"@context": {{"S231": "http://data.ashrae.org/S231#"}}, "@graph": [
  {{"@id": "{ns}", "@type": "S231:Block", "S231:containsBlock": [{{"@id": "{ns}.not"}}],
   "S231:hasInput": [{}], "S231:hasOutput": [{{"@id": "{ns}.y"}}]}},
  {{"@id": "{ns}.not", "@type": "CDL.Logical.Not", "S231:hasInput": {{"@id": "{ns}.not.u"}}, "S231:hasOutput": {{"@id": "{ns}.not.y"}}}},
  {{"@id": "{ns}.not.u", "@type": "S231:BooleanInput", "S231:isOfDataType": {{"@id": "S231:Boolean"}}}},
  {{"@id": "{ns}.not.y", "@type": "S231:BooleanOutput", "S231:isOfDataType": {{"@id": "S231:Boolean"}}, "S231:isConnectedTo": {{"@id": "{ns}.y"}}}},
  {{"@id": "{ns}.u", "@type": "S231:BooleanInput", "S231:isOfDataType": {{"@id": "S231:Boolean"}}, "S231:isConnectedTo": {{"@id": "{ns}.not.u"}}}},
  {{"@id": "{ns}.y", "@type": "S231:BooleanOutput", "S231:isOfDataType": {{"@id": "S231:Boolean"}}}},
  {}
]}}"#,
        inputs.join(", "),
        nodes.join(",\n  ")
    )
}

#[test]
fn the_set_verdict_is_fail_over_no_data_over_pass() {
    let pass = named("pass", SIGNALS_INPUTS, &window("nOut", 0, 1800, "3"));
    let fail = named("fail", SIGNALS_INPUTS, &window("nOut", 0, 1800, "4"));
    let empty = named("empty", SIGNALS_INPUTS, "");
    for (scenarios, status) in [
        (vec![pass.clone()], ScenarioStatus::Pass),
        (vec![pass.clone(), empty.clone()], ScenarioStatus::NoData),
        (vec![empty.clone(), fail.clone()], ScenarioStatus::Fail),
        (vec![pass.clone(), fail.clone()], ScenarioStatus::Fail),
        (Vec::new(), ScenarioStatus::NoData),
    ] {
        let receipt = receipt(&set_of(&scenarios.join(", ")), &signals());
        let body = body(&receipt);
        assert_eq!(body.status, status, "{scenarios:?}");
        assert_eq!(receipt.header.passed, status == ScenarioStatus::Pass);
        let counts = body.totals.scenarios;
        assert_eq!(counts.total, u64::try_from(scenarios.len()).expect("fits"));
        assert_eq!(counts.pass + counts.fail + counts.no_data, counts.total);
    }
}

#[test]
fn a_set_with_no_scenarios_still_binds_its_subject_and_records_it() {
    let receipt = receipt(&set_of(""), &signals());
    let body = body(&receipt);
    assert_eq!(body.status, ScenarioStatus::NoData);
    assert!(body.scenarios.is_empty());
    assert_eq!(body.totals.frames.executed, 0);
    assert!(receipt.header.content_id.starts_with("oce:canon1:"));
}

#[test]
fn each_scenario_runs_on_a_fresh_engine_so_their_order_changes_no_verdict() {
    // AHU-0016's `both_open` fires the persistence timer; a scenario after it would start with
    // the alarm already up if state leaked from one scenario to the next.
    let text = String::from_utf8(support::library_vectors("AHU-0016")).expect("UTF-8");
    let set = oce_verify::VectorSet::from_json_bytes(text.as_bytes(), &VerifyLimits::default())
        .expect("loads");
    let subject = support::library_subject("AHU-0016");
    let forward = receipt(&text, &subject);
    // The same scenarios, last first.
    let canonical = set.to_canonical_json();
    let reversed = reverse_scenarios(&canonical);
    let backward = receipt(&reversed, &subject);
    let summary = |receipt: &oce_verify::Receipt| {
        let mut rows: Vec<(String, &str, u64, u64)> = body(receipt)
            .scenarios
            .iter()
            .map(|s| {
                (
                    s.name.clone(),
                    s.status.as_str(),
                    s.instants.checked,
                    s.instants.passed,
                )
            })
            .collect();
        rows.sort();
        rows
    };
    assert_eq!(summary(&forward), summary(&backward));
    assert!(forward.header.passed && backward.header.passed);
    assert_ne!(
        forward.header.spec_id, backward.header.spec_id,
        "file order is in the set"
    );
}

/// A canonical set with its scenario list reversed, by its JSON structure.
fn reverse_scenarios(canonical: &str) -> String {
    let document: serde_json::Value = serde_json::from_str(canonical).expect("JSON");
    let mut document = document;
    let scenarios = document["scenarios"]
        .as_array_mut()
        .expect("a scenario list");
    scenarios.reverse();
    serde_json::to_string(&document).expect("JSON")
}

/// The `twin_asserts` subject: two `CDL.Utilities.Assert` instances on one input `b`, both with
/// the message `b is false`, so a frame with `b` false emits two equal warnings.
fn twins() -> Vec<u8> {
    subject("twin_asserts")
}

#[test]
fn one_expectation_matches_one_of_two_equal_warnings_and_the_other_stays_unmatched() {
    // 31 frames with `b` false: 62 warnings, two at 60 s.
    for expectation in [diagnostic(60, Some("b is false")), diagnostic(60, None)] {
        let receipt = receipt(&one(r#""b": false"#, &expectation), &twins());
        let scenario = only_scenario(&receipt);
        assert_eq!(scenario.status, ScenarioStatus::Pass);
        assert_eq!(scenario.instants.checked, 1);
        // One warning at 60 s was taken; the second instance's is unmatched with the rest.
        assert_eq!(scenario.warnings_total, 61, "{expectation}");
        let at_sixty = scenario
            .warnings
            .iter()
            .filter(|warning| warning.t.to_bits() == 60.0_f64.to_bits())
            .count();
        assert_eq!(at_sixty, 1, "the second warning at 60 s is listed");
        assert_eq!(body(&receipt).totals.warnings, 61);
    }
}

#[test]
fn two_expectations_take_the_two_equal_warnings_of_their_tick_one_each() {
    let expect = [
        diagnostic(60, Some("b is false")),
        diagnostic(60, Some("b is false")),
    ]
    .join(", ");
    let receipt = receipt(&one(r#""b": false"#, &expect), &twins());
    let scenario = only_scenario(&receipt);
    assert_eq!(scenario.status, ScenarioStatus::Pass);
    assert_eq!(
        (scenario.instants.checked, scenario.instants.passed),
        (2, 2)
    );
    assert_eq!(scenario.warnings_total, 60);
    assert!(
        scenario
            .warnings
            .iter()
            .all(|warning| warning.t.to_bits() != 60.0_f64.to_bits())
    );
    // A third expectation at that tick has no warning left: the later expectation fails.
    let expect = [
        diagnostic(60, None),
        diagnostic(60, Some("b is false")),
        diagnostic(60, Some("b is false")),
    ]
    .join(", ");
    let receipt = support::receipt(&one(r#""b": false"#, &expect), &twins());
    let scenario = only_scenario(&receipt);
    assert_eq!(scenario.status, ScenarioStatus::Fail);
    assert_eq!(
        (scenario.instants.checked, scenario.instants.failed),
        (3, 1)
    );
    // The two that name the message are paired first; the block-only one, first in the file,
    // finds nothing left.
    assert!(matches!(
        scenario.failures[..],
        [Failure::DiagnosticMissing {
            expectation: 0,
            message: None,
            ..
        }]
    ));
    assert_eq!(scenario.warnings_total, 60);
}

#[test]
fn a_refused_frame_keeps_its_record_when_earlier_mismatches_fill_the_list() {
    // `bOut` is true while the window expects false: one mismatch on each of the 20 ticks before
    // `lim` steps out of its bounds at 1200 s, which the engine refuses.
    let inputs = r#""ahu.T": 1.0, "zone.T": 2.0, "n": 3, "b": true, "lim": [{"t": 0, "value": 5.0}, {"t": 1200, "value": 11.0}]"#;
    let set = one(inputs, &window("bOut", 0, 1800, "false"));
    let receipt = receipt(&set, &signals());
    let scenario = only_scenario(&receipt);
    assert_eq!(scenario.failures_total, 21, "20 mismatches and the refusal");
    assert_eq!(scenario.failures.len(), 16);
    // The last slot is the refusal's, so 15 mismatches are listed before it.
    let mismatches = scenario
        .failures
        .iter()
        .take(15)
        .filter(|failure| matches!(failure, Failure::ValueMismatch { .. }))
        .count();
    assert_eq!(mismatches, 15);
    assert!(matches!(
        scenario.failures[15],
        Failure::FrameRefused { t, refusal: EngineRefusal::InputDomain } if t.to_bits() == 1200.0_f64.to_bits()
    ));
    assert_eq!(scenario.frames.refused, 1);
    // A cap of one lists the refusal alone; a cap of zero lists nothing; the total never moves.
    for (cap, listed) in [(1, 1), (0, 0)] {
        let options = VerifyOptions {
            limits: VerifyLimits {
                max_recorded_failures: cap,
                ..VerifyLimits::MAX
            },
            ..VerifyOptions::default()
        };
        let receipt = run_with(&set, &signals(), &options)
            .expect("a receipt")
            .receipt;
        let scenario = only_scenario(&receipt);
        assert_eq!(scenario.failures.len(), listed, "cap {cap}");
        assert_eq!(scenario.failures_total, 21, "cap {cap}");
        if listed == 1 {
            assert!(matches!(scenario.failures[0], Failure::FrameRefused { .. }));
        }
    }
    // Without a refusal every slot goes to the earliest failures.
    let receipt = support::receipt(
        &one(SIGNALS_INPUTS, &window("bOut", 0, 1800, "false")),
        &signals(),
    );
    let scenario = only_scenario(&receipt);
    assert_eq!((scenario.failures.len(), scenario.failures_total), (16, 31));
}
