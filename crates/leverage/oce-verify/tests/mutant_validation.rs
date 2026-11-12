//! Mutant validation at bind (vectors note decision 27; owner rulings of 2026-11-11 on parameter
//! edits and on the frame budget): every listed mutant is supplied once and nothing else, the
//! bytes fit their bound, and each one loads, has the subject's boundary, differs from the
//! subject's content id and admits every edit list the subject admits. Any failure is an error
//! with no receipt, before any frame of any scenario runs. The order the note's part 3 reading 1
//! gives is pinned here too: the subject's own refusals and the frame budget first, then the
//! bytes as given, the listing, the byte bound, and each mutant in file order, its edit lists
//! included.

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

#[test]
fn a_mutant_that_holds_an_edited_parameter_as_structural_is_invalid() {
    // A block instance can carry a parameter its class does not declare. The subject's
    // `Logical.Not` carries an Integer `nin`, which it admits as settable; the mutant's
    // `gate` is a `Logical.MultiAnd`, where `nin` is the structural input count.
    let set = r#"{"schema": "oce/vectors/v1", "clock": {"step_s": 60, "horizon_s": 1800},
        "mutants": [{"path": "m.cxf.json"}],
        "scenarios": [{"name": "s", "params": {"gate.nin": 1}, "inputs": {"u": 0.0}, "expect": []}]}"#;
    let mutants = vec![("m.cxf.json".to_owned(), subject("gate_multiand"))];
    let error = param_refusal(set, &subject("gate"), &mutants);
    assert_eq!(
        (error.code(), error.subject()),
        ("mutant-param-refused", "mutants[0]")
    );
    assert_eq!(error.path(), Some("urn:verify:gate#top.gate.nin"));
    assert_eq!(error.cause(), Some("param-structural"));
    // The subject admits the edit on its own, and the mutant is valid under a set without it.
    let unedited = set.replace(r#""params": {"gate.nin": 1}, "#, "");
    assert_ne!(unedited, set);
    assert!(run_mutants(&unedited, &subject("gate"), &mutants).is_ok());
}

#[test]
fn a_mutant_past_the_engines_document_bound_fails_to_load_whatever_the_byte_bound() {
    // Each mutant is held to the engine's own CXF bound, as the subject is, whatever room
    // `max_mutant_bytes` leaves: trailing spaces pad the late alarm to the bound and one past it.
    let padded = |len: usize| {
        let mut bytes = mutant("late_alarm");
        bytes.resize(len, b' ');
        bytes
    };
    let past = refused(&with_late_alarm(padded(oce_api::MAX_CXF_BYTES + 1)));
    assert_eq!(
        (past.code(), past.subject()),
        ("mutant-load-failed", "mutants[2]")
    );
    assert!(past.diagnostic_codes().is_empty(), "{past:?}");
    let at = run_mutants(
        &mutant_set(),
        &library_subject("AHU-0016"),
        &with_late_alarm(padded(oce_api::MAX_CXF_BYTES)),
    );
    assert!(at.is_ok(), "{:?}", at.err());
    assert!(
        VerifyLimits::MAX.max_mutant_bytes > u64::try_from(oce_api::MAX_CXF_BYTES).expect("fits")
    );
}

/// `VerifyOptions` with `limits` over the maximum ones.
fn options(limits: VerifyLimits) -> VerifyOptions {
    VerifyOptions {
        limits,
        ..VerifyOptions::default()
    }
}

#[test]
fn the_frame_budget_is_checked_before_any_mutant_is_judged() {
    // Owner ruling of 2026-11-11: the budget is arithmetic on the listed count, so a set over
    // it is `too-many-frames` whatever its mutants hold.
    let over = options(VerifyLimits {
        max_total_frames: 867,
        ..VerifyLimits::MAX
    });
    let mut missing = ahu_mutants();
    missing.pop();
    let mut unlisted = ahu_mutants();
    unlisted.insert(
        0,
        ("mutants/extra.cxf.json".to_owned(), mutant("late_alarm")),
    );
    let ahu = library_subject("AHU-0016");
    for (defect, mutants, code) in [
        (
            "unloadable",
            with_late_alarm(b"{".to_vec()),
            "mutant-load-failed",
        ),
        (
            "identical",
            with_late_alarm(ahu.clone()),
            "mutant-identical",
        ),
        ("missing", missing, "mutant-missing"),
        ("unlisted", unlisted, "mutant-unlisted"),
    ] {
        let error =
            run_mutants_with(&mutant_set(), &ahu, &mutants, &over).expect_err("over the budget");
        assert_eq!(error.code(), "too-many-frames", "{defect}");
        // The control: within the budget the defect is what is refused.
        assert_eq!(refused(&mutants).code(), code, "{defect}");
    }
    // Over the byte bound too: the budget still wins.
    let both = options(VerifyLimits {
        max_total_frames: 867,
        max_mutant_bytes: 0,
        ..VerifyLimits::MAX
    });
    let error =
        run_mutants_with(&mutant_set(), &ahu, &ahu_mutants(), &both).expect_err("over both");
    assert_eq!(error.code(), "too-many-frames");
}

#[test]
fn the_bytes_are_bounded_together_before_any_mutant_loads() {
    let unloadable = with_late_alarm(b"{".to_vec());
    let none = options(VerifyLimits {
        max_mutant_bytes: 0,
        ..VerifyLimits::MAX
    });
    let error = run_mutants_with(
        &mutant_set(),
        &library_subject("AHU-0016"),
        &unloadable,
        &none,
    )
    .expect_err("over the byte bound");
    assert_eq!(error.code(), "too-many-mutant-bytes");
    assert_eq!(refused(&unloadable).code(), "mutant-load-failed");
}

#[test]
fn of_two_misuses_of_the_bytes_the_one_given_first_wins() {
    let extra = || ("mutants/extra.cxf.json".to_owned(), mutant("late_alarm"));
    let mut repeat_first = ahu_mutants();
    repeat_first.push(repeat_first[0].clone());
    repeat_first.push(extra());
    assert_eq!(refused(&repeat_first).code(), "duplicate-mutant");
    let mut unlisted_first = vec![extra()];
    unlisted_first.extend(ahu_mutants());
    unlisted_first.push(unlisted_first[1].clone());
    assert_eq!(refused(&unlisted_first).code(), "mutant-unlisted");
}

/// The hysteresis set of [`hysteresis_set`] listing `m0.cxf.json` and `m1.cxf.json`.
fn two_mutant_set(params: &[&str]) -> String {
    hysteresis_set(params).replace(
        r#""mutants": [{"path": "m.cxf.json"}]"#,
        r#""mutants": [{"path": "m0.cxf.json"}, {"path": "m1.cxf.json"}]"#,
    )
}

#[test]
fn a_mutants_edit_lists_are_tried_before_the_next_mutant_loads() {
    let hysteresis = subject("hysteresis");
    let refusing = edited(&hysteresis, &[(r#""@value": "1.0""#, r#""@value": "1.8""#)]);
    let set = two_mutant_set(&[r#""uHigh": 1.5"#]);
    assert_ne!(set, hysteresis_set(&[r#""uHigh": 1.5"#]));
    let pair = |first: Vec<u8>, second: Vec<u8>| {
        vec![
            ("m0.cxf.json".to_owned(), first),
            ("m1.cxf.json".to_owned(), second),
        ]
    };
    let error = param_refusal(&set, &hysteresis, &pair(refusing.clone(), b"{".to_vec()));
    assert_eq!(
        (error.code(), error.subject()),
        ("mutant-param-refused", "mutants[0]")
    );
    let error = param_refusal(&set, &hysteresis, &pair(b"{".to_vec(), refusing));
    assert_eq!(
        (error.code(), error.subject()),
        ("mutant-load-failed", "mutants[0]")
    );
}

#[test]
fn the_subjects_own_parameter_refusal_comes_before_any_mutant_defect() {
    // The subject's band is (1, 2): uHigh 0.5 is below uLow in every pass.
    let set = hysteresis_set(&[r#""uHigh": 0.5"#]);
    let error = param_refusal(
        &set,
        &subject("hysteresis"),
        &[("m.cxf.json".to_owned(), b"{".to_vec())],
    );
    assert_eq!(
        (error.code(), error.subject()),
        ("param-refused", "scenarios[0]")
    );
    let error = param_refusal(&set, &subject("hysteresis"), &[]);
    assert_eq!(
        error.code(),
        "param-refused",
        "even with the mutant missing"
    );
}
