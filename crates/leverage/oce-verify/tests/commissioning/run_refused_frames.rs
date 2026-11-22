//! Refused frames (issue #662 and #663; commissioning note, decision 28): a frame refused for a
//! missing sample is recorded and the run goes on, its stretch uncovered and out of the verdict;
//! an engine refusal ends the run with no receipt.

use oce_verify::{CommissionOptions, CommissionRun, OutputStatus};

use super::{body_of, commissioned, output_named, refused_run, run_inputs, run_spec_edited};

/// One ulp above 10 s.
const PAST_TEN: &str = "10.000000000000002";

/// The fixture with its inputs in two sources: `main.csv` (every input but the limit, held for
/// `main_hold` seconds when given) and `limit.csv` (the limit, held for 10 s), and the expected
/// outputs at `expected_rows` (time, setpoint degC, enabled, stage, limit percent).
fn split_sources(
    main_times: &[&str],
    main_hold: Option<u32>,
    limit_rows: &[(&str, &str)],
    expected_rows: &[(&str, &str, &str, &str, &str)],
    sampling: &str,
) -> Result<CommissionRun, oce_verify::VerifyError> {
    let hold = main_hold.map_or(String::new(), |age| {
        format!(r#", "hold": {{ "maxAge": {age} }}"#)
    });
    let spec = run_spec_edited(&[
        (
            r#""Stage": "Stage", "Limit": "Limit" } }"#,
            &format!(
                r#""Stage": "Stage" }}{hold} }},
        {{ "path": "limit.csv", "format": "csv", "timeColumn": "time",
          "columns": {{ "Limit": "Limit" }}, "hold": {{ "maxAge": 10 }} }}"#
            ),
        ),
        (r#""path": "inputs.csv""#, r#""path": "main.csv""#),
        (r#""sampling": 5,"#, &format!(r#""sampling": {sampling},"#)),
    ]);
    let mut main = String::from("time,Supply Temp,Offset,Enable,Stage\n");
    for t in main_times {
        main.push_str(&format!("{t},20,1,true,1\n"));
    }
    let mut limit = String::from("time,Limit\n");
    for (t, value) in limit_rows {
        limit.push_str(&format!("{t},{value}\n"));
    }
    let mut expected = String::from("time,Setpoint,Enabled,Staged,Limited\n");
    for (t, set, en, n, lim) in expected_rows {
        expected.push_str(&format!("{t},{set},{en},{n},{lim}\n"));
    }
    let mut inputs = run_inputs(&[
        ("outputs.csv", expected.as_bytes()),
        ("main.csv", main.as_bytes()),
        ("limit.csv", limit.as_bytes()),
    ]);
    inputs.files.retain(|(path, _)| path != "inputs.csv");
    oce_verify::run_commissioning(&spec, 0, &inputs, &CommissionOptions::default())
}

#[test]
fn a_hold_one_ulp_past_max_age_refuses_one_frame_and_uncovers_its_stretch_only() {
    let run = split_sources(
        &["0", "10", PAST_TEN, "20", "30"],
        None,
        &[("0", "50"), ("20", "50"), ("30", "50")],
        &[
            ("0", "21", "true", "1", "50"),
            ("10", "21", "true", "1", "50"),
            // Inside the stretch, and deliberately wrong: it is not judged.
            ("15", "99", "false", "7", "50"),
            ("20", "21", "true", "1", "50"),
            ("30", "21", "true", "1", "50"),
        ],
        "5",
    )
    .expect("a missing sample is no error");
    let body = body_of(&run.receipt);
    // 10 s is held at exactly its maxAge (substituted); one ulp later it is refused.
    assert_eq!(
        (
            body.frames.executed,
            body.frames.substituted,
            body.frames.refused
        ),
        (3, 1, 1)
    );
    // Of 0, 5, …, 30 s, only 15 s lies in [10 s + 1 ulp, 20 s).
    for name in ["TSet", "enOut", "nOut"] {
        let output = output_named(body, name);
        assert_eq!(
            (output.compared, output.masked, output.uncovered),
            (6, 0, 1),
            "{name}"
        );
        assert_eq!(output.status, OutputStatus::Pass, "{name}");
    }
    // The fan indicator is false at 15 s, so that instant is masked, not uncovered.
    let lim = output_named(body, "limOut");
    assert_eq!((lim.compared, lim.masked, lim.uncovered), (5, 2, 0));
    // An uncovered instant's error is not recorded: its curve point is a NaN.
    let curve = super::curves_named(&run, "TSet");
    assert_eq!(curve.errors.x(), [0.0, 5.0, 10.0, 15.0, 20.0, 25.0, 30.0]);
    assert!(curve.errors.y()[3].is_nan());
    assert!(
        curve
            .errors
            .y()
            .iter()
            .enumerate()
            .all(|(k, e)| k == 3 || *e == 0.0)
    );
}

#[test]
fn a_substituted_frame_ends_the_stretch() {
    let run = split_sources(
        &["0", "10", PAST_TEN, "20", "30"],
        Some(100),
        &[("0", "50"), ("12", "50"), ("30", "50")],
        &[
            ("0", "21", "true", "1", "50"),
            ("11", "99", "false", "7", "50"),
            ("12", "21", "true", "1", "50"),
            ("30", "21", "true", "1", "50"),
        ],
        "1",
    )
    .expect("runs");
    let body = body_of(&run.receipt);
    // 12 s is a frame of the limit source, the main source held there: substituted.
    assert_eq!(body.frames.refused, 1);
    let n = output_named(body, "nOut");
    assert_eq!(
        (n.compared, n.uncovered),
        (30, 1),
        "of 0, 1, …, 30 s only 11 s lies in [10 s + 1 ulp, 12 s)"
    );
    assert_eq!(n.status, OutputStatus::Pass);
}

#[test]
fn a_stretch_no_executed_frame_ends_runs_to_stop_included() {
    let run = split_sources(
        &["0", "10", "20", "30"],
        None,
        &[("0", "50")],
        &[
            ("0", "21", "true", "1", "50"),
            ("10", "21", "true", "1", "50"),
            ("20", "99", "false", "7", "50"),
            ("30", "99", "false", "7", "50"),
        ],
        "5",
    )
    .expect("runs");
    let body = body_of(&run.receipt);
    assert_eq!(body.frames.refused, 2);
    let set = output_named(body, "TSet");
    // 20, 25 and 30 s, the stop included.
    assert_eq!((set.compared, set.uncovered), (4, 3));
    assert_eq!(set.status, OutputStatus::Pass);
}

#[test]
fn every_frame_refused_gives_no_data() {
    let run = split_sources(
        &["0", "10", "20", "30"],
        None,
        &[("100", "50")],
        &[
            ("0", "21", "true", "1", "50"),
            ("30", "21", "true", "1", "50"),
        ],
        "5",
    )
    .expect("a gap is no error");
    let body = body_of(&run.receipt);
    assert_eq!(body.status, OutputStatus::NoData);
    assert!(!run.receipt.header.passed);
    assert!(
        body.outputs
            .iter()
            .all(|o| o.status == OutputStatus::NoData && o.compared == 0)
    );
}

#[test]
fn an_input_outside_its_domain_is_frame_refused_with_its_class_row_time_and_message() {
    // 2000 % is 20, past `lim`'s declared maximum of 10.
    let csv = super::run_file_edited(
        "inputs.csv",
        &[("10,21,1,true,2,50", "10,21,1,true,2,2000")],
    );
    let error = refused_run(&super::run_spec(), &run_inputs(&[("inputs.csv", &csv)]));
    assert_eq!(
        (error.code(), error.cause()),
        ("frame-refused", Some("input-domain"))
    );
    assert_eq!(error.frame_index(), Some(1));
    assert_eq!(error.frame_time(), Some(10.0));
    let message = error.engine_message().expect("the engine's message");
    assert!(!message.is_empty() && message.len() <= 256, "{message}");
    assert_eq!(
        error.path(),
        None,
        "the verifier does not parse the engine's message"
    );
    assert_eq!(error.subject(), "references[0]");
}

#[test]
fn an_engine_refusal_is_the_same_error_for_every_chunk_size() {
    let csv = super::run_file_edited(
        "inputs.csv",
        &[("20,22,2,false,3,40", "20,22,2,false,3,4000")],
    );
    let inputs = run_inputs(&[("inputs.csv", &csv)]);
    let mut errors = Vec::new();
    for chunk in [1, 2, 7, usize::MAX] {
        let mut options = CommissionOptions::default();
        options.chunk_frames = chunk;
        errors.push(
            oce_verify::run_commissioning(&super::run_spec(), 0, &inputs, &options)
                .expect_err("refused"),
        );
    }
    assert!(
        errors.windows(2).all(|pair| pair[0] == pair[1]),
        "{errors:?}"
    );
    assert_eq!(errors[0].frame_index(), Some(2));
    // The fixture itself is not refused.
    commissioned(&super::run_spec(), &run_inputs(&[]));
}
