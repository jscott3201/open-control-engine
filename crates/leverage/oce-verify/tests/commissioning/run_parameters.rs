//! A reference's parameter edits (issue #662; commissioning note, decisions 24, 29 and 30):
//! resolved with no prefix, typed by the parameter, applied in passes before any frame, echoed
//! as the spec wrote them, and the edited content id in the body only.

use oce_verify::{CommissionOptions, CommissionRun, OutputStatus, SpecInputs, VerifyError};

use super::support;
use super::{body_of, loaded};

/// A reference of the `hysteresis` subject (`uLow` 1, `uHigh` 2) whose trended output is
/// `expected` (four Booleans at 0, 10, 20 and 30 s, the input reading 0, 3, 1.5, 0), with the
/// `parameters` object `parameters`.
fn hysteresis(parameters: &str, expected: [bool; 4]) -> Result<CommissionRun, VerifyError> {
    let spec = loaded(&format!(
        r#"{{"references": [{{"model": "H", "sequence": "hys", "cxf": "h.json",
            "pointNameMapping": [
              {{"cdl": {{"name": "u", "type": "float"}}, "device": {{"name": "U", "type": "float"}}}},
              {{"cdl": {{"name": "y", "type": "bool"}}, "device": {{"name": "Y", "type": "bool"}}}}
            ],
            "parameters": {parameters},
            "window": {{"start": 0, "stop": 30}}, "sampling": 10,
            "inputs": [{{"path": "u.csv", "format": "csv", "timeColumn": "time",
                         "columns": {{"U": "U"}}}}],
            "expected": [{{"path": "y.csv", "format": "csv", "timeColumn": "time",
                           "columns": {{"Y": "Y"}}}}]}}]}}"#
    ));
    let y: String = [0, 10, 20, 30]
        .iter()
        .zip(expected)
        .map(|(t, y)| format!("{t},{y}\n"))
        .collect();
    let inputs = SpecInputs {
        cxf: support::subject("hysteresis"),
        files: vec![
            (
                "u.csv".to_owned(),
                b"time,U\n0,0\n10,3\n20,1.5\n30,0\n".to_vec(),
            ),
            ("y.csv".to_owned(), format!("time,Y\n{y}").into_bytes()),
        ],
    };
    oce_verify::run_commissioning(&spec, 0, &inputs, &CommissionOptions::default())
}

fn passes(run: &CommissionRun) -> bool {
    body_of(&run.receipt).status == OutputStatus::Pass
}

#[test]
fn lowering_and_raising_both_hysteresis_thresholds_both_apply() {
    // As loaded: 1.5 lies in the band, so the output holds true from 3.
    let as_loaded = hysteresis("{}", [false, true, true, false]).expect("runs");
    assert!(passes(&as_loaded));
    // Lowered: `uHigh` sorts first and would drop below the old `uLow`, so it waits a pass.
    let lowered = hysteresis(
        r#"{"hys.uLow": 0.5, "hys.uHigh": 0.8}"#,
        [false, true, true, false],
    )
    .expect("two passes admit it");
    assert!(passes(&lowered));
    // Raised: 1.5 is now below `uLow`, so the output falls at 20 s.
    let raised = hysteresis(
        r#"{"hys.uLow": 2.5, "hys.uHigh": 2.8}"#,
        [false, true, false, false],
    )
    .expect("one pass admits it");
    assert!(passes(&raised));
    let body = body_of(&raised.receipt);
    let paths: Vec<&str> = body.parameters.iter().map(|p| p.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "urn:verify:hysteresis#top.hys.uHigh",
            "urn:verify:hysteresis#top.hys.uLow"
        ],
        "canonical paths, sorted by bytes"
    );
}

#[test]
fn an_edit_moves_the_bodys_content_id_and_never_the_headers() {
    let plain = hysteresis("{}", [false, true, true, false]).expect("runs");
    let edited = hysteresis(r#"{"hys.uLow": 1.25}"#, [false, true, true, false]).expect("runs");
    assert_eq!(body_of(&plain.receipt).content_id, None);
    assert!(body_of(&plain.receipt).parameters.is_empty());
    // The header names the document as loaded (decision 30); the body names what ran.
    assert_eq!(
        plain.receipt.header.content_id,
        edited.receipt.header.content_id
    );
    let ran = body_of(&edited.receipt)
        .content_id
        .clone()
        .expect("with edits");
    assert_ne!(ran, edited.receipt.header.content_id);
    assert!(ran.starts_with("oce:canon1:"));
    // The value is echoed as the spec wrote it.
    let json = edited.receipt.to_canonical_json();
    assert!(json.contains(r#""value": 1.25"#), "{json}");
}

#[test]
fn an_edit_no_pass_admits_and_a_path_the_sequence_lacks_are_refused_before_any_frame() {
    let error =
        hysteresis(r#"{"hys.uLow": 5, "hys.uHigh": 4}"#, [false; 4]).expect_err("uLow > uHigh");
    assert_eq!(
        (error.code(), error.cause()),
        ("param-refused", Some("param-range"))
    );
    assert_eq!(error.subject(), "references[0].parameters");

    let error = hysteresis(r#"{"hys.zz": 1}"#, [false; 4]).expect_err("no such parameter");
    assert_eq!(error.code(), "param-path-unknown");
    assert_eq!(error.subject(), "references[0].parameters.hys.zz");

    let error = hysteresis(r#"{"hys.uLow": true}"#, [false; 4]).expect_err("a Boolean on a Real");
    assert_eq!(error.code(), "value-type-mismatch");

    let error = hysteresis(
        r#"{"hys.uLow": 1, "urn:verify:hysteresis#top.hys.uLow": 2}"#,
        [false; 4],
    )
    .expect_err("two keys on one parameter");
    assert_eq!(error.code(), "param-path-duplicate");
}
