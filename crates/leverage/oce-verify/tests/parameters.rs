//! Parameter edits (vectors note decision 14): keys resolve by the one name rule to canonical
//! paths, two keys of one map on one parameter are refused, a scenario's map overrides the set's
//! by resolved path whatever either spelling, and the edits are applied on each scenario's fresh
//! engine inside one halt, in passes in ascending path order, so lowering both
//! `Reals.Hysteresis` thresholds succeeds where one pass in path order would not. The receipt
//! lists each scenario's effective edits by canonical path and the content id of what ran.

mod support;

use oce_api::Engine;
use oce_verify::{ReceiptValue, ScenarioReceipt, ScenarioStatus};

use support::{
    AHU_0016, HYSTERESIS, SIGNALS, SIGNALS_INPUTS, body, library_subject, only_scenario, receipt,
    refused, subject,
};

const BOTH_OPEN: &str = r#""htg_vlv_cmd": 20.0, "clg_vlv_cmd": 30.0"#;

fn window(output: &str, from_s: u32, to_s: u32, equals: &str) -> String {
    format!(r#"{{"output": "{output}", "from_s": {from_s}, "to_s": {to_s}, "equals": {equals}}}"#)
}

/// `both_open` with the alarm expected from `delay` seconds on: the persistence timer fires at
/// exactly its delay.
fn alarm_from(delay: u32) -> String {
    format!(
        "{}, {}",
        window("yFault", 0, delay - 60, "false"),
        window("yFault", delay, 1800, "true")
    )
}

/// A set on AHU-0016's clock with the given set-level `params` body and scenarios.
fn ahu_set(params: Option<&str>, scenarios: &[String]) -> String {
    let params = params.map_or_else(String::new, |body| format!(r#""params": {{{body}}}, "#));
    format!(
        r#"{{"schema": "oce/vectors/v1", "clock": {{"step_s": 60, "horizon_s": 1800}}, {params}"scenarios": [{}]}}"#,
        scenarios.join(", ")
    )
}

/// A scenario with its own `params` body.
fn with_params(name: &str, params: &str, inputs: &str, expect: &str) -> String {
    format!(
        r#"{{"name": "{name}", "params": {{{params}}}, "inputs": {{{inputs}}}, "expect": [{expect}]}}"#
    )
}

fn edits(scenario: &ScenarioReceipt) -> Vec<(String, u64)> {
    scenario
        .params
        .iter()
        .map(|edit| match edit.value {
            ReceiptValue::Real(x) => (edit.path.clone(), x.to_bits()),
            other => panic!("a Real edit: {other:?}"),
        })
        .collect()
}

#[test]
fn a_parameter_key_resolves_by_exact_path_or_a_unique_suffix() {
    let full = format!("{AHU_0016}persist.delayTime");
    for key in [full.as_str(), "persist.delayTime", "delayTime"] {
        let set = ahu_set(
            None,
            &[with_params(
                "short",
                &format!(r#""{key}": 300.0"#),
                BOTH_OPEN,
                &alarm_from(300),
            )],
        );
        let receipt = receipt(&set, &library_subject("AHU-0016"));
        let scenario = only_scenario(&receipt);
        assert_eq!(scenario.status, ScenarioStatus::Pass, "{key}: {scenario:?}");
        assert_eq!(
            edits(scenario),
            [(full.clone(), 300.0_f64.to_bits())],
            "{key}"
        );
    }
}

#[test]
fn an_ambiguous_or_unknown_parameter_key_is_refused_by_its_code() {
    let subject = library_subject("AHU-0016");
    let error = refused(
        &ahu_set(Some(r#""t": 4.0"#), &[support::scenario(BOTH_OPEN, "")]),
        &subject,
    );
    assert_eq!(error.code(), "param-path-ambiguous");
    assert_eq!(error.subject(), "params.t");
    assert_eq!(
        error.candidates(),
        [format!("{AHU_0016}clgThr.t"), format!("{AHU_0016}htgThr.t")]
    );
    for key in [
        "persist.delay",
        "Time",
        "ahu_fc_050.persist.delayTime",
        "yFault",
    ] {
        let error = refused(
            &ahu_set(
                None,
                &[with_params("s", &format!(r#""{key}": 1.0"#), BOTH_OPEN, "")],
            ),
            &subject,
        );
        assert_eq!(error.code(), "param-path-unknown", "{key}");
        assert_eq!(error.subject(), format!("scenarios[0].params.{key}"));
    }
}

#[test]
fn two_spellings_of_one_parameter_in_one_map_are_refused() {
    let full = format!("{AHU_0016}persist.delayTime");
    let subject = library_subject("AHU-0016");
    for set in [
        ahu_set(
            Some(&format!(r#""delayTime": 300.0, "{full}": 300.0"#)),
            &[support::scenario(BOTH_OPEN, "")],
        ),
        ahu_set(
            None,
            &[with_params(
                "s",
                r#""persist.delayTime": 300.0, "delayTime": 600.0"#,
                BOTH_OPEN,
                "",
            )],
        ),
    ] {
        let error = refused(&set, &subject);
        assert_eq!(error.code(), "param-path-duplicate", "{set}");
        assert_eq!(error.path(), Some(full.as_str()));
    }
}

#[test]
fn a_scenario_overrides_the_set_by_resolved_path_whatever_the_spelling() {
    let full = format!("{AHU_0016}persist.delayTime");
    let set = ahu_set(
        Some(r#""persist.delayTime": 600"#),
        &[
            with_params(
                "own",
                &format!(r#""{full}": 300.0"#),
                BOTH_OPEN,
                &alarm_from(300),
            ),
            support::named("inherited", BOTH_OPEN, &alarm_from(600)),
        ],
    );
    let subject = library_subject("AHU-0016");
    let receipt = receipt(&set, &subject);
    let scenarios = &body(&receipt).scenarios;
    for scenario in scenarios {
        assert_eq!(scenario.status, ScenarioStatus::Pass, "{scenario:?}");
    }
    // One edit per path: the scenario's value, never both.
    assert_eq!(edits(&scenarios[0]), [(full.clone(), 300.0_f64.to_bits())]);
    // The set's integer token lands on the Real parameter as `600.0`.
    assert_eq!(edits(&scenarios[1]), [(full, 600.0_f64.to_bits())]);
    // Each edited scenario records the id of what ran; the header keeps the subject's as loaded.
    let mut engine = Engine::new();
    engine.load_cxf(&subject).expect("loads");
    let loaded = engine.content_id().expect("an id").to_string();
    assert_eq!(receipt.header.content_id, loaded);
    let ids: Vec<&str> = scenarios
        .iter()
        .map(|scenario| scenario.content_id.as_deref().expect("an edited id"))
        .collect();
    assert!(
        ids[0] != loaded && ids[1] != loaded && ids[0] != ids[1],
        "{ids:?}"
    );
    let text = receipt.to_canonical_json();
    assert!(text.contains("\"value\": 600.0"), "{text}");
}

#[test]
fn an_edit_that_lands_on_the_value_it_had_still_records_the_id_of_what_ran() {
    let set = ahu_set(
        None,
        &[with_params(
            "same",
            r#""delayTime": 900.0"#,
            BOTH_OPEN,
            &alarm_from(900),
        )],
    );
    let receipt = receipt(&set, &library_subject("AHU-0016"));
    let scenario = only_scenario(&receipt);
    assert_eq!(scenario.status, ScenarioStatus::Pass);
    assert_eq!(
        scenario.content_id.as_deref(),
        Some(receipt.header.content_id.as_str())
    );
}

/// The hysteresis subject's scenario with `params` and input `u`, expecting `y` to be `equals`.
fn hysteresis(params: &str, u: &str, equals: &str) -> String {
    support::set_of(&with_params(
        "s",
        params,
        &format!(r#""u": {u}"#),
        &window("y", 0, 1800, equals),
    ))
}

#[test]
fn lowering_both_hysteresis_thresholds_waits_for_the_second_pass_and_raising_takes_one() {
    let subject = subject("hysteresis");
    // The defaults are uLow 1 and uHigh 2: `u = 0` is below the band and `y` stays false.
    let control = receipt(&hysteresis("", "0.0", "false"), &subject);
    assert_eq!(only_scenario(&control).status, ScenarioStatus::Pass);
    // Lowered to (-2, -1): `uHigh` sorts first and would drop below the old `uLow`, so it waits
    // one pass; then `u = 0` is above the band.
    let lowered = receipt(
        &hysteresis(r#""uLow": -2.0, "uHigh": -1.0"#, "0.0", "true"),
        &subject,
    );
    let scenario = only_scenario(&lowered);
    assert_eq!(scenario.status, ScenarioStatus::Pass, "{scenario:?}");
    assert_eq!(
        edits(scenario),
        [
            (format!("{HYSTERESIS}hys.uHigh"), (-1.0_f64).to_bits()),
            (format!("{HYSTERESIS}hys.uLow"), (-2.0_f64).to_bits()),
        ],
        "listed by canonical path bytes"
    );
    // Raised to (10, 20): `u = 5` is now below the band, where the defaults put it above.
    let above = receipt(&hysteresis("", "5.0", "true"), &subject);
    assert_eq!(only_scenario(&above).status, ScenarioStatus::Pass);
    let raised = receipt(
        &hysteresis(r#""hys.uHigh": 20.0, "hys.uLow": 10.0"#, "5.0", "false"),
        &subject,
    );
    assert_eq!(only_scenario(&raised).status, ScenarioStatus::Pass);
}

#[test]
fn an_edit_no_pass_admits_is_param_refused_with_its_class() {
    let subject = subject("hysteresis");
    // `uHigh = 3` applies in the first pass (3 >= the old uLow 1); `uLow = 5` is then above it
    // in every pass.
    let error = refused(
        &hysteresis(r#""uLow": 5.0, "uHigh": 3.0"#, "0.0", "false"),
        &subject,
    );
    assert_eq!(error.code(), "param-refused");
    assert_eq!(error.subject(), "scenarios[0]");
    assert_eq!(error.path(), Some(format!("{HYSTERESIS}hys.uLow").as_str()));
    assert_eq!(error.cause(), Some("param-range"));
    // An edit outside a static bound is refused on its own: `Reals.MovingAverage` declares
    // `delta(min=1E-5)`, and AHU-0004's moving window is `rate.delta`.
    let error = refused(
        &support::set_on(
            "300",
            "14400",
            &with_params("s", r#""rate.delta": 0.0"#, r#""operating_state": 0"#, ""),
        ),
        &library_subject("AHU-0004"),
    );
    assert_eq!(
        (error.code(), error.cause()),
        ("param-refused", Some("param-range"))
    );
    assert_eq!(
        error.path(),
        Some("urn:cxf-library:ahu-fc-004#ahu_fc_004.rate.delta")
    );
}

#[test]
fn a_parameter_value_is_typed_by_the_parameters_declared_type() {
    // A Boolean parameter takes no number, a Real no boolean, and a String parameter nothing.
    let ahu = library_subject("AHU-0016");
    for (key, value) in [("delayOnInit", "1"), ("delayTime", "true")] {
        let error = refused(
            &ahu_set(
                Some(&format!(r#""{key}": {value}"#)),
                &[support::scenario(BOTH_OPEN, "")],
            ),
            &ahu,
        );
        assert_eq!(error.code(), "value-type-mismatch", "{key}");
        assert_eq!(error.subject(), format!("params.{key}"));
    }
    let error = refused(
        &support::set_of(&with_params("s", r#""chk.message": 1"#, SIGNALS_INPUTS, "")),
        &subject("signals"),
    );
    assert_eq!(error.code(), "value-type-mismatch");
    assert_eq!(error.path(), Some(format!("{SIGNALS}chk.message").as_str()));
    // A Boolean parameter takes `false`, and the run reads it: with `delayOnInit` false the
    // timer does not delay an input that is true from the first frame, so the alarm is up from
    // `t = 0` where the default holds it for 900 s.
    let set = ahu_set(
        None,
        &[with_params(
            "s",
            r#""delayOnInit": false"#,
            BOTH_OPEN,
            &window("yFault", 0, 1800, "true"),
        )],
    );
    let receipt = receipt(&set, &ahu);
    let scenario = only_scenario(&receipt);
    assert_eq!(scenario.status, ScenarioStatus::Pass, "{scenario:?}");
    assert!(matches!(
        scenario.params[0].value,
        ReceiptValue::Boolean(false)
    ));
}
