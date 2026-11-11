//! Mutant validation at bind (vectors note decision 27; owner ruling of 2026-11-11 on parameter
//! edits): every listed mutant is supplied once and nothing else, the bytes fit their bound, and
//! each one loads, has the subject's boundary, differs from the subject's content id and admits
//! every edit list the subject admits. Any failure is an error with no receipt, before any frame
//! of any scenario runs.

mod support;

use oce_replay::ReplayLimits;
use oce_verify::{VerifyError, VerifyLimits, VerifyOptions};

use support::{
    AHU_0016, HYSTERESIS, ahu_mutants, ahu_real_fault, edited, library_subject, mutant, mutant_set,
    run_mutants, run_mutants_with, subject,
};

const LATE_ALARM: &str = "mutants/late_alarm.cxf.json";

fn refused(mutants: &[(String, Vec<u8>)]) -> VerifyError {
    match run_mutants(&mutant_set(), &library_subject("AHU-0016"), mutants) {
        Ok(run) => panic!(
            "the run was meant to be refused\n{}",
            run.receipt.to_canonical_json()
        ),
        Err(error) => error,
    }
}

/// The three mutants with the third's bytes replaced by `bytes`.
fn with_late_alarm(bytes: Vec<u8>) -> Vec<(String, Vec<u8>)> {
    let mut mutants = ahu_mutants();
    mutants[2] = (LATE_ALARM.to_owned(), bytes);
    mutants
}

#[test]
fn a_mutant_with_the_subjects_content_id_is_identical_whatever_its_spelling() {
    for bytes in [
        library_subject("AHU-0016"),
        // Another layout of the same graph: the canonical id ignores it.
        edited(&library_subject("AHU-0016"), &[("\n    {", "\n\n    {")]),
    ] {
        let error = refused(&with_late_alarm(bytes));
        assert_eq!(error.code(), "mutant-identical");
        assert_eq!(error.subject(), "mutants[2]");
    }
}

#[test]
fn a_mutant_whose_boundary_differs_by_one_port_is_an_error_naming_it() {
    let ahu = library_subject("AHU-0016");
    for (bytes, path) in [
        // One input path renamed.
        (
            edited(&ahu, &[("ahu_fc_050.htg_vlv_cmd", "ahu_fc_050.htg_valve")]),
            format!("{AHU_0016}htg_valve"),
        ),
        // One output path renamed.
        (
            edited(&ahu, &[("ahu_fc_050.yFault", "ahu_fc_050.yAlarm")]),
            format!("{AHU_0016}yAlarm"),
        ),
        // One output type: the fault output is a Real.
        (ahu_real_fault(), format!("{AHU_0016}yFault")),
    ] {
        let error = refused(&with_late_alarm(bytes));
        assert_eq!(error.code(), "mutant-boundary-differs", "{path}");
        assert_eq!(error.subject(), "mutants[2]");
        assert_eq!(error.path(), Some(path.as_str()));
    }
    let error = refused(&with_late_alarm(ahu_real_fault()));
    assert!(
        error
            .detail()
            .contains("a Boolean in the subject and a Real in the mutant"),
        "{error}"
    );
}

#[test]
fn a_mutant_the_engine_refuses_is_an_error_with_the_engines_codes() {
    let error = refused(&with_late_alarm(b"{".to_vec()));
    assert_eq!(
        (error.code(), error.subject()),
        ("mutant-load-failed", "mutants[2]")
    );
    assert!(
        error.diagnostic_codes().is_empty(),
        "malformed JSON has none"
    );
    // A graph naming a class the engine does not know carries the engine's code.
    let unknown = edited(
        &library_subject("AHU-0016"),
        &[("CDL.Logical.TrueDelay", "CDL.Logical.NoSuchDelay")],
    );
    let error = refused(&with_late_alarm(unknown));
    assert_eq!(error.code(), "mutant-load-failed");
    assert!(!error.diagnostic_codes().is_empty(), "{error:?}");
}

#[test]
fn every_listed_mutant_is_supplied_once_and_nothing_else_is() {
    let mut missing = ahu_mutants();
    missing.pop();
    let error = refused(&missing);
    assert_eq!(
        (error.code(), error.subject()),
        ("mutant-missing", "mutants[2]")
    );
    let mut unlisted = ahu_mutants();
    unlisted.push(("mutants/extra.cxf.json".to_owned(), mutant("late_alarm")));
    let error = refused(&unlisted);
    assert_eq!(
        (error.code(), error.subject()),
        ("mutant-unlisted", "mutants")
    );
    let mut twice = ahu_mutants();
    twice.push(twice[0].clone());
    let error = refused(&twice);
    assert_eq!(
        (error.code(), error.subject()),
        ("duplicate-mutant", "mutants")
    );
    // Bytes the set never asked for are refused before a missing listing is reported.
    missing.push(("mutants/extra.cxf.json".to_owned(), mutant("late_alarm")));
    assert_eq!(refused(&missing).code(), "mutant-unlisted");
    // Mutant bytes given to a set that lists none are unlisted too.
    let error = run_mutants(
        &String::from_utf8(support::library_vectors("AHU-0016")).expect("UTF-8"),
        &library_subject("AHU-0016"),
        &ahu_mutants(),
    )
    .expect_err("no mutant is listed");
    assert_eq!(error.code(), "mutant-unlisted");
}

#[test]
fn the_mutants_bytes_are_bounded_together_at_their_limit() {
    let mutants = ahu_mutants();
    let total: u64 = mutants
        .iter()
        .map(|(_, bytes)| u64::try_from(bytes.len()).expect("fits"))
        .sum();
    let under = |max_mutant_bytes| VerifyOptions {
        limits: VerifyLimits {
            max_mutant_bytes,
            ..VerifyLimits::MAX
        },
        ..VerifyOptions::default()
    };
    let at = run_mutants_with(
        &mutant_set(),
        &library_subject("AHU-0016"),
        &mutants,
        &under(total),
    );
    assert!(at.is_ok(), "{:?}", at.err());
    let past = run_mutants_with(
        &mutant_set(),
        &library_subject("AHU-0016"),
        &mutants,
        &under(total - 1),
    )
    .expect_err("one byte past");
    assert_eq!(
        (past.code(), past.subject()),
        ("too-many-mutant-bytes", "mutants")
    );
    assert_eq!(VerifyLimits::MAX.max_mutant_bytes, 64 * 1024 * 1024);
}

#[test]
fn the_frame_budget_counts_every_mutant_once_per_scenario() {
    // Seven scenarios of 31 ticks on the subject and on each of three mutants: 868 frames.
    let under = |max_total_frames| VerifyOptions {
        limits: VerifyLimits {
            max_total_frames,
            ..VerifyLimits::MAX
        },
        ..VerifyOptions::default()
    };
    let subject = library_subject("AHU-0016");
    assert!(run_mutants_with(&mutant_set(), &subject, &ahu_mutants(), &under(868)).is_ok());
    let error = run_mutants_with(&mutant_set(), &subject, &ahu_mutants(), &under(867))
        .expect_err("one frame past");
    assert_eq!(error.code(), "too-many-frames");
}

/// The hysteresis subject's set, one scenario per `params` body, each holding `u` at 0.
fn hysteresis_set(params: &[&str]) -> String {
    let scenarios: Vec<String> = params
        .iter()
        .enumerate()
        .map(|(index, body)| {
            format!(
                r#"{{"name": "s{index}", "params": {{{body}}}, "inputs": {{"u": 0.0}}, "expect": []}}"#
            )
        })
        .collect();
    format!(
        r#"{{"schema": "oce/vectors/v1", "clock": {{"step_s": 60, "horizon_s": 1800}}, "mutants": [{{"path": "m.cxf.json"}}], "scenarios": [{}]}}"#,
        scenarios.join(", ")
    )
}

fn hysteresis_mutant(edits: &[(&str, &str)]) -> Vec<(String, Vec<u8>)> {
    vec![(
        "m.cxf.json".to_owned(),
        edited(&subject("hysteresis"), edits),
    )]
}

fn param_refusal(set: &str, subject: &[u8], mutants: &[(String, Vec<u8>)]) -> VerifyError {
    run_mutants(set, subject, mutants).expect_err("refused at bind")
}

#[test]
fn a_mutant_whose_class_rule_refuses_an_edit_the_subject_admits_is_invalid() {
    // The subject's band is (1, 2) and admits uHigh 1.5; the mutant's is (1.8, 2), where uHigh
    // 1.5 is below uLow in every pass.
    let mutants = hysteresis_mutant(&[(r#""@value": "1.0""#, r#""@value": "1.8""#)]);
    let error = param_refusal(
        &hysteresis_set(&[r#""uHigh": 1.5"#]),
        &subject("hysteresis"),
        &mutants,
    );
    assert_eq!(error.code(), "mutant-param-refused");
    assert_eq!(error.subject(), "mutants[0]");
    assert_eq!(
        error.path(),
        Some(format!("{HYSTERESIS}hys.uHigh").as_str())
    );
    assert_eq!(error.cause(), Some("param-range"));
    assert!(error.detail().contains("m.cxf.json"), "{error}");
}

#[test]
fn a_mutant_without_a_parameter_the_subject_edits_is_invalid() {
    // The mutant's block is named `band`: `top.hys.uLow` names nothing in it.
    let mutants = hysteresis_mutant(&[("top.hys", "top.band")]);
    let error = param_refusal(
        &hysteresis_set(&[r#""uLow": 0.5"#]),
        &subject("hysteresis"),
        &mutants,
    );
    assert_eq!(error.code(), "mutant-param-refused");
    assert_eq!(error.path(), Some(format!("{HYSTERESIS}hys.uLow").as_str()));
    assert_eq!(error.cause(), Some("param-path-unknown"));
}

#[test]
fn a_mutant_with_the_parameter_of_another_type_is_invalid() {
    // The subject's `cmp.t` is a Real; the mutant reads the same input through an Integer
    // threshold, whose `cmp.t` is an Integer.
    let set = r#"{"schema": "oce/vectors/v1", "clock": {"step_s": 60, "horizon_s": 1800},
        "mutants": [{"path": "m.cxf.json"}],
        "scenarios": [{"name": "s", "params": {"cmp.t": 4.0}, "inputs": {"u": 0.0}, "expect": []}]}"#;
    let mutants = vec![(
        "m.cxf.json".to_owned(),
        support::fixture_bytes("subjects/threshold_integer.cxf.json"),
    )];
    let error = param_refusal(set, &subject("threshold"), &mutants);
    assert_eq!(error.code(), "mutant-param-refused");
    assert_eq!(error.path(), Some("urn:verify:threshold#top.cmp.t"));
    assert_eq!(error.cause(), Some("param-type"));
}

#[test]
fn edit_lists_are_tried_in_the_order_of_their_first_scenario() {
    // Every edit is unknown to the renamed mutant, so the list tried first is the one named: the
    // first scenario's.
    let mutants = hysteresis_mutant(&[("top.hys", "top.band")]);
    let hysteresis = subject("hysteresis");
    let first = param_refusal(
        &hysteresis_set(&[r#""uLow": 0.5"#, r#""uHigh": 3.0"#, r#""uLow": 0.5"#]),
        &hysteresis,
        &mutants,
    );
    assert_eq!(first.path(), Some(format!("{HYSTERESIS}hys.uLow").as_str()));
    let swapped = param_refusal(
        &hysteresis_set(&[r#""uHigh": 3.0"#, r#""uLow": 0.5"#]),
        &hysteresis,
        &mutants,
    );
    assert_eq!(
        swapped.path(),
        Some(format!("{HYSTERESIS}hys.uHigh").as_str())
    );
    // A mutant that admits every list is valid, and a set without edits tries none.
    let wider = hysteresis_mutant(&[(r#""@value": "2.0""#, r#""@value": "4.0""#)]);
    assert!(
        run_mutants(
            &hysteresis_set(&[r#""uLow": 0.5"#, r#""uHigh": 3.0"#]),
            &hysteresis,
            &wider
        )
        .is_ok()
    );
}

#[test]
fn an_invalid_mutant_is_refused_before_any_frame_of_any_scenario_runs() {
    // A trace bound of zero bytes would fail the first replay to run, so a mutant refusal that
    // arrives under it was found before any scenario ran.
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
    let hysteresis = subject("hysteresis");
    let refusing = hysteresis_mutant(&[(r#""@value": "1.0""#, r#""@value": "1.8""#)]);
    let error = run_mutants_with(
        &hysteresis_set(&["", r#""uHigh": 1.5"#]),
        &hysteresis,
        &refusing,
        &options,
    )
    .expect_err("refused at bind");
    assert_eq!(error.code(), "mutant-param-refused");
    let identical = vec![("m.cxf.json".to_owned(), hysteresis.clone())];
    let error = run_mutants_with(&hysteresis_set(&[""]), &hysteresis, &identical, &options)
        .expect_err("refused at bind");
    assert_eq!(error.code(), "mutant-identical");
    // The control: a valid mutant under the same options reaches the replay and fails there.
    let valid = hysteresis_mutant(&[(r#""@value": "2.0""#, r#""@value": "4.0""#)]);
    let error = run_mutants_with(&hysteresis_set(&[""]), &hysteresis, &valid, &options)
        .expect_err("the replay fails");
    assert_eq!(
        (error.code(), error.cause()),
        ("replay-failed", Some("trace-too-large"))
    );
}
