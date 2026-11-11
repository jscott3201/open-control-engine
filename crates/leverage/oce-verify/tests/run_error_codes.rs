//! Every bind and run code of the vectors note's table, pinned, and produced by a run, the mutant
//! rows included (`tests/mutant_validation.rs` pins each of those in depth). Four codes are
//! refusals no loadable subject or loaded set reaches through a run today, so their cases live
//! beside the code that makes them, each with the reason; this file holds the two lists to cover
//! the pinned list exactly between them.
//!
//! The frame budget is pinned at its bound: at the limit a run is accepted, one frame past it is
//! refused before any frame runs.

mod support;

use std::collections::BTreeSet;

use oce_replay::ReplayLimits;
use oce_verify::{VERIFY_ERROR_CODES, VerifyError, VerifyLimits, VerifyOptions};

use support::{
    SIGNALS, SIGNALS_INPUTS, ahu_mutants, edited, library_subject, library_vectors, mutant,
    mutant_set, one, run_mutants, run_mutants_with, run_with, set_of, subject,
};

/// The note's bind and run rows, in the order `VerifyErrorKind` declares them.
const PINNED: &[&str] = &[
    "subject-load-failed",
    "unknown-input",
    "ambiguous-input",
    "duplicate-input",
    "input-not-supplied",
    "unknown-output",
    "ambiguous-output",
    "param-path-unknown",
    "param-path-ambiguous",
    "param-path-duplicate",
    "param-refused",
    "value-type-mismatch",
    "value-not-representable",
    "enum-port-unsupported",
    "unsupported-port-type",
    "too-many-frames",
    "mutant-missing",
    "mutant-unlisted",
    "duplicate-mutant",
    "too-many-mutant-bytes",
    "mutant-load-failed",
    "mutant-boundary-differs",
    "mutant-identical",
    "mutant-param-refused",
    "run-refused",
    "replay-failed",
    "window-uncovered",
];

/// The codes no run reaches today, each with the unit test that pins it and why.
const PINNED_BESIDE_THE_CODE: &[(&str, &str)] = &[
    (
        "enum-port-unsupported",
        "src/bind_tests.rs: the importer refuses an Enum boundary pass-through and no elementary block takes or gives an Enum signal",
    ),
    (
        "unsupported-port-type",
        "src/bind_tests.rs: a String is never a signal, and the importer refuses a String boundary pass-through",
    ),
    (
        "run-refused",
        "src/run_tests.rs: a fresh engine never exhausts its frame sequence or loses its slot map",
    ),
    (
        "window-uncovered",
        "src/run_tests.rs: load proves every window holds a tick, and a run that refused no frame ran every tick",
    ),
];

#[test]
fn the_code_list_is_pinned() {
    assert_eq!(VERIFY_ERROR_CODES, PINNED);
}

fn window(output: &str, equals: &str) -> String {
    format!(r#"{{"output": "{output}", "from_s": 0, "to_s": 1800, "equals": {equals}}}"#)
}

fn with_limits(limits: VerifyLimits) -> VerifyOptions {
    VerifyOptions {
        limits,
        ..VerifyOptions::default()
    }
}

/// One run per code: the code, the subject the refusal must name, and the run.
fn cases() -> Vec<(
    &'static str,
    String,
    Result<oce_verify::VectorRun, VerifyError>,
)> {
    let signals = subject("signals");
    let ahu = library_subject("AHU-0016");
    let defaults = VerifyOptions::default();
    let run = |text: &str, subject: &[u8]| run_with(text, subject, &defaults);
    let both_open = r#""htg_vlv_cmd": 20.0, "clg_vlv_cmd": 30.0"#;
    let params = |body: &str| {
        format!(
            r#"{{"schema": "oce/vectors/v1", "clock": {{"step_s": 60, "horizon_s": 1800}}, "params": {{{body}}}, "scenarios": [{}]}}"#,
            support::scenario(both_open, "")
        )
    };
    vec![
        (
            "subject-load-failed",
            "subject".to_owned(),
            run(&one(SIGNALS_INPUTS, ""), b"not a CXF document"),
        ),
        (
            "unknown-input",
            "scenarios[0].inputs.spare".to_owned(),
            run(
                &one(&format!(r#"{SIGNALS_INPUTS}, "spare": 0.0"#), ""),
                &signals,
            ),
        ),
        (
            "ambiguous-input",
            "scenarios[0].inputs.T".to_owned(),
            run(
                &one(r#""T": 1.0, "n": 3, "b": true, "lim": 5.0"#, ""),
                &signals,
            ),
        ),
        (
            "duplicate-input",
            // The names are taken in byte order: `b`, then the full path.
            format!("scenarios[0].inputs.{SIGNALS}b"),
            run(
                &one(&format!(r#"{SIGNALS_INPUTS}, "{SIGNALS}b": true"#), ""),
                &signals,
            ),
        ),
        (
            "input-not-supplied",
            "scenarios[0].inputs".to_owned(),
            run(
                &one(r#""ahu.T": 1.0, "zone.T": 2.0, "n": 3, "b": true"#, ""),
                &signals,
            ),
        ),
        (
            "unknown-output",
            "scenarios[0].expect[0].output".to_owned(),
            run(&one(SIGNALS_INPUTS, &window("div.y", "1.0")), &signals),
        ),
        (
            "ambiguous-output",
            "scenarios[0].expect[0].output".to_owned(),
            run(&one(SIGNALS_INPUTS, &window("y", "1.0")), &signals),
        ),
        (
            "param-path-unknown",
            "params.persist.delay".to_owned(),
            run(&params(r#""persist.delay": 300.0"#), &ahu),
        ),
        (
            "param-path-ambiguous",
            "params.t".to_owned(),
            run(&params(r#""t": 4.0"#), &ahu),
        ),
        (
            "param-path-duplicate",
            "params.persist.delayTime".to_owned(),
            run(
                &params(r#""delayTime": 300.0, "persist.delayTime": 300.0"#),
                &ahu,
            ),
        ),
        (
            "param-refused",
            "scenarios[0]".to_owned(),
            run(
                &set_of(
                    r#"{"name": "s", "params": {"uLow": 5.0, "uHigh": 3.0}, "inputs": {"u": 0.0}, "expect": []}"#,
                ),
                &subject("hysteresis"),
            ),
        ),
        (
            "value-type-mismatch",
            "scenarios[0].inputs.n".to_owned(),
            run(
                &one(
                    r#""ahu.T": 1.0, "zone.T": 2.0, "n": 1.0, "b": true, "lim": 5.0"#,
                    "",
                ),
                &signals,
            ),
        ),
        (
            "value-not-representable",
            "scenarios[0].inputs.ahu.T".to_owned(),
            run(
                &one(
                    r#""ahu.T": -9007199254740993, "zone.T": 2.0, "n": 3, "b": true, "lim": 5.0"#,
                    "",
                ),
                &signals,
            ),
        ),
        (
            "too-many-frames",
            "scenarios".to_owned(),
            run_with(
                &String::from_utf8(library_vectors("AHU-0016")).expect("UTF-8"),
                &ahu,
                &with_limits(VerifyLimits {
                    max_total_frames: 216,
                    ..VerifyLimits::MAX
                }),
            ),
        ),
        (
            "mutant-missing",
            "mutants[2]".to_owned(),
            run_mutants(&mutant_set(), &ahu, &ahu_mutants()[..2]),
        ),
        (
            "mutant-unlisted",
            "mutants".to_owned(),
            run_mutants(
                &mutant_set(),
                &ahu,
                &[
                    ahu_mutants(),
                    vec![("mutants/extra.cxf.json".to_owned(), mutant("late_alarm"))],
                ]
                .concat(),
            ),
        ),
        (
            "duplicate-mutant",
            "mutants".to_owned(),
            run_mutants(
                &mutant_set(),
                &ahu,
                &[ahu_mutants(), ahu_mutants()[..1].to_vec()].concat(),
            ),
        ),
        (
            "too-many-mutant-bytes",
            "mutants".to_owned(),
            run_mutants_with(
                &mutant_set(),
                &ahu,
                &ahu_mutants(),
                &with_limits(VerifyLimits {
                    max_mutant_bytes: 1024,
                    ..VerifyLimits::MAX
                }),
            ),
        ),
        (
            "mutant-load-failed",
            "mutants[0]".to_owned(),
            run_mutants(
                &mutant_set(),
                &ahu,
                &[
                    vec![(
                        "mutants/threshold_inclusive.cxf.json".to_owned(),
                        b"[]".to_vec(),
                    )],
                    ahu_mutants()[1..].to_vec(),
                ]
                .concat(),
            ),
        ),
        (
            "mutant-boundary-differs",
            "mutants[1]".to_owned(),
            run_mutants(
                &mutant_set(),
                &ahu,
                &[
                    ahu_mutants()[..1].to_vec(),
                    vec![(
                        "mutants/no_persistence.cxf.json".to_owned(),
                        edited(&ahu, &[("ahu_fc_050.clg_vlv_cmd", "ahu_fc_050.clg_valve")]),
                    )],
                    ahu_mutants()[2..].to_vec(),
                ]
                .concat(),
            ),
        ),
        (
            "mutant-identical",
            "mutants[2]".to_owned(),
            run_mutants(
                &mutant_set(),
                &ahu,
                &[
                    ahu_mutants()[..2].to_vec(),
                    vec![("mutants/late_alarm.cxf.json".to_owned(), ahu.clone())],
                ]
                .concat(),
            ),
        ),
        (
            "mutant-param-refused",
            "mutants[0]".to_owned(),
            run_mutants(
                r#"{"schema": "oce/vectors/v1", "clock": {"step_s": 60, "horizon_s": 1800},
                    "mutants": [{"path": "m.cxf.json"}],
                    "scenarios": [{"name": "s", "params": {"uHigh": 1.5}, "inputs": {"u": 0.0}, "expect": []}]}"#,
                &subject("hysteresis"),
                &[(
                    "m.cxf.json".to_owned(),
                    edited(
                        &subject("hysteresis"),
                        &[(r#""@value": "1.0""#, r#""@value": "1.8""#)],
                    ),
                )],
            ),
        ),
        (
            "replay-failed",
            "scenarios[0]".to_owned(),
            run_with(
                &one(SIGNALS_INPUTS, ""),
                &signals,
                &with_limits(VerifyLimits {
                    replay: ReplayLimits {
                        max_trace_bytes: 100,
                        ..ReplayLimits::MAX
                    },
                    ..VerifyLimits::MAX
                }),
            ),
        ),
    ]
}

#[test]
fn every_reachable_code_is_produced_by_its_case_at_the_place_it_names() {
    for (code, at, outcome) in cases() {
        let error = match outcome {
            Ok(run) => panic!(
                "{code}: the run gave a receipt\n{}",
                run.receipt.to_canonical_json()
            ),
            Err(error) => error,
        };
        assert_eq!(error.code(), code, "{error}");
        assert_eq!(error.kind().code(), code);
        assert_eq!(error.subject(), at, "{code}: {error}");
        assert!(!error.detail().is_empty(), "{code}");
        assert_eq!(error.to_string(), format!("{at}: {}", error.detail()));
        let detailed_kinds = [
            ("ambiguous-input", error.candidates().len() == 2),
            ("ambiguous-output", error.candidates().len() == 2),
            ("param-path-ambiguous", error.candidates().len() == 2),
            ("param-refused", error.cause() == Some("param-range")),
            ("replay-failed", error.cause() == Some("trace-too-large")),
            (
                "input-not-supplied",
                error.path() == Some(&format!("{SIGNALS}lim")[..]),
            ),
            ("mutant-param-refused", error.cause() == Some("param-range")),
            (
                "mutant-boundary-differs",
                error.path() == Some("urn:cxf-library:ahu-fc-050#ahu_fc_050.clg_valve"),
            ),
        ];
        for (kind, holds) in detailed_kinds {
            if kind == code {
                assert!(holds, "{code}: {error:?}");
            }
        }
    }
}

#[test]
fn the_cases_and_the_codes_pinned_beside_the_code_cover_the_list_between_them() {
    let produced: Vec<&str> = cases().iter().map(|(code, _, _)| *code).collect();
    let beside: Vec<&str> = PINNED_BESIDE_THE_CODE
        .iter()
        .map(|(code, _)| *code)
        .collect();
    let mut all: Vec<&str> = produced.iter().chain(&beside).copied().collect();
    let unique: BTreeSet<&str> = all.iter().copied().collect();
    assert_eq!(unique.len(), all.len(), "no code is in both lists or twice");
    all.sort_by_key(|code| VERIFY_ERROR_CODES.iter().position(|c| c == code));
    assert_eq!(all, VERIFY_ERROR_CODES);
}

#[test]
fn a_subject_the_engine_refuses_carries_the_engines_diagnostic_codes() {
    // A String pass-through: the importer refuses it with a coded diagnostic.
    let document = r#"{"@context": {"S231": "http://data.ashrae.org/S231#"}, "@graph": [
      {"@id": "urn:x#top", "@type": "S231:Block", "S231:containsBlock": [{"@id": "urn:x#top.not"}],
       "S231:hasInput": [{"@id": "urn:x#top.label"}, {"@id": "urn:x#top.b"}],
       "S231:hasOutput": [{"@id": "urn:x#top.labelOut"}, {"@id": "urn:x#top.y"}]},
      {"@id": "urn:x#top.not", "@type": "CDL.Logical.Not", "S231:hasInput": {"@id": "urn:x#top.not.u"}, "S231:hasOutput": {"@id": "urn:x#top.not.y"}},
      {"@id": "urn:x#top.not.u", "@type": "S231:BooleanInput", "S231:isOfDataType": {"@id": "S231:Boolean"}},
      {"@id": "urn:x#top.not.y", "@type": "S231:BooleanOutput", "S231:isOfDataType": {"@id": "S231:Boolean"}, "S231:isConnectedTo": {"@id": "urn:x#top.y"}},
      {"@id": "urn:x#top.b", "@type": "S231:BooleanInput", "S231:isOfDataType": {"@id": "S231:Boolean"}, "S231:isConnectedTo": {"@id": "urn:x#top.not.u"}},
      {"@id": "urn:x#top.y", "@type": "S231:BooleanOutput", "S231:isOfDataType": {"@id": "S231:Boolean"}},
      {"@id": "urn:x#top.label", "@type": "S231:StringInput", "S231:isOfDataType": {"@id": "S231:String"}, "S231:isConnectedTo": {"@id": "urn:x#top.labelOut"}},
      {"@id": "urn:x#top.labelOut", "@type": "S231:StringOutput", "S231:isOfDataType": {"@id": "S231:String"}}
    ]}"#;
    let error = run_with(
        &one(r#""b": true"#, ""),
        document.as_bytes(),
        &VerifyOptions::default(),
    )
    .expect_err("refused at load");
    assert_eq!(error.code(), "subject-load-failed");
    assert_eq!(error.diagnostic_codes(), ["non-subset-construct"]);
    // Bytes that are not JSON carry no structured diagnostic.
    let error = run_with(&one("", ""), b"{", &VerifyOptions::default()).expect_err("refused");
    assert_eq!(error.code(), "subject-load-failed");
    assert!(error.diagnostic_codes().is_empty());
}

#[test]
fn the_total_frame_budget_is_accepted_at_the_limit_and_refused_one_past() {
    // AHU-0016: seven scenarios of 31 ticks, 217 frames.
    let set = String::from_utf8(library_vectors("AHU-0016")).expect("UTF-8");
    let ahu = library_subject("AHU-0016");
    let at = |max_total_frames| {
        run_with(
            &set,
            &ahu,
            &with_limits(VerifyLimits {
                max_total_frames,
                ..VerifyLimits::MAX
            }),
        )
    };
    assert!(at(217).is_ok());
    assert_eq!(at(216).expect_err("one past").code(), "too-many-frames");
    // A set with no scenario runs no frame, so even a budget of zero admits it.
    let empty = set_of("");
    let error = run_with(
        &empty,
        &subject("signals"),
        &with_limits(VerifyLimits {
            max_total_frames: 0,
            ..VerifyLimits::MAX
        }),
    );
    assert!(error.is_ok());
}

#[test]
fn one_scenarios_ticks_are_held_to_the_replay_frame_limit_at_bind() {
    let set = one(SIGNALS_INPUTS, "");
    let signals = subject("signals");
    let at = |max_frames| {
        run_with(
            &set,
            &signals,
            &with_limits(VerifyLimits {
                replay: ReplayLimits {
                    max_frames,
                    ..ReplayLimits::MAX
                },
                ..VerifyLimits::MAX
            }),
        )
    };
    assert!(at(31).is_ok());
    let error = at(30).expect_err("one past");
    assert_eq!(
        (error.code(), error.subject()),
        ("too-many-frames", "clock")
    );
}

#[test]
fn a_lowered_replay_row_limit_is_a_source_failure() {
    // A step list changing on every tick makes 31 source rows; 30 are allowed.
    let steps: Vec<String> = (0..31)
        .map(|k| format!(r#"{{"t": {}, "value": {k}.0}}"#, k * 60))
        .collect();
    let inputs = format!(
        r#""ahu.T": [{}], "zone.T": 2.0, "n": 3, "b": true, "lim": 5.0"#,
        steps.join(", ")
    );
    let options = with_limits(VerifyLimits {
        replay: ReplayLimits {
            max_rows: 30,
            ..ReplayLimits::MAX
        },
        ..VerifyLimits::MAX
    });
    let error = run_with(&one(&inputs, ""), &subject("signals"), &options)
        .expect_err("refused by the source");
    assert_eq!(
        (error.code(), error.cause()),
        ("replay-failed", Some("source"))
    );
    let options = with_limits(VerifyLimits {
        replay: ReplayLimits {
            max_rows: 31,
            ..ReplayLimits::MAX
        },
        ..VerifyLimits::MAX
    });
    assert!(run_with(&one(&inputs, ""), &subject("signals"), &options).is_ok());
}
