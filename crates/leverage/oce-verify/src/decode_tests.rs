//! The typed reader standing alone: a load scans for repeated keys first, so these tests call
//! the reader directly to show it refuses them itself, in every object it reads.

use super::*;

const LIMITS: VerifyLimits = VerifyLimits::MAX;

fn refusal(text: &str) -> (VectorErrorKind, u32, u32) {
    let refused = decode(text, &LIMITS).expect_err("refused");
    (refused.kind, refused.at.line, refused.at.column)
}

/// A whole set around one scenario body.
fn set_with(scenario: &str) -> String {
    format!(
        "{{\"schema\": \"oce/vectors/v1\", \"clock\": {{\"step_s\": 1, \"horizon_s\": 2}}, \"scenarios\": [{scenario}]}}"
    )
}

#[test]
fn a_repeated_key_is_refused_in_every_object_the_reader_fills() {
    let cases = [
        r#"{"schema": "oce/vectors/v1", "schema": "oce/vectors/v1"}"#.to_owned(),
        r#"{"clock": {"step_s": 1, "step_s": 1}}"#.to_owned(),
        r#"{"clock": {"horizon_s": 1, "horizon_s": 1}}"#.to_owned(),
        r#"{"params": {"a.k": 1, "a.k": 2}}"#.to_owned(),
        r#"{"scenarios": [], "scenarios": []}"#.to_owned(),
        set_with(r#"{"name": "a", "name": "b"}"#),
        set_with(r#"{"description": "a", "description": "b"}"#),
        set_with(r#"{"params": {}, "params": {}}"#),
        set_with(r#"{"inputs": {}, "inputs": {}}"#),
        set_with(r#"{"expect": [], "expect": []}"#),
        set_with(r#"{"inputs": {"u": 1, "u": 2}}"#),
        set_with(r#"{"inputs": {"u": [{"t": 0, "t": 0}]}}"#),
        set_with(r#"{"inputs": {"u": [{"value": 0, "value": 0}]}}"#),
        set_with(r#"{"expect": [{"output": "y", "output": "y"}]}"#),
        set_with(r#"{"expect": [{"from_s": 0, "from_s": 0}]}"#),
        set_with(r#"{"expect": [{"to_s": 0, "to_s": 0}]}"#),
        set_with(r#"{"expect": [{"equals": 0, "equals": 0}]}"#),
        set_with(r#"{"expect": [{"at_s": 0, "at_s": 0}]}"#),
        set_with(r#"{"expect": [{"diagnostic": {"block": "B"}, "diagnostic": {"block": "B"}}]}"#),
        set_with(r#"{"expect": [{"diagnostic": {"block": "B", "block": "B"}}]}"#),
        set_with(r#"{"expect": [{"diagnostic": {"message": "m", "message": "m"}}]}"#),
    ];
    for case in &cases {
        assert_eq!(
            decode(case, &LIMITS).expect_err("refused").kind,
            VectorErrorKind::DuplicateKey,
            "{case}"
        );
    }
}

#[test]
fn a_repeated_key_is_named_at_its_second_writing() {
    assert_eq!(
        refusal("{\"scenarios\": [],\n \"scenarios\": []}"),
        (VectorErrorKind::DuplicateKey, 2, 2)
    );
}

#[test]
fn a_missing_key_is_named_at_the_object_that_lacks_it() {
    assert_eq!(refusal(" {}"), (VectorErrorKind::MissingKey, 1, 2));
    assert_eq!(
        refusal("{\"schema\": \"oce/vectors/v1\",\n \"clock\":   {\"step_s\": 1}}"),
        (VectorErrorKind::MissingKey, 2, 13)
    );
}

#[test]
fn text_after_the_set_is_refused_by_the_reader_too() {
    let text = format!("{} []", set_with(""));
    assert_eq!(
        decode(&text, &LIMITS).expect_err("refused").kind,
        VectorErrorKind::JsonSyntax
    );
    assert!(decode(&set_with(""), &LIMITS).is_ok());
}

#[test]
fn the_reader_keeps_file_order_in_its_maps_and_lists() {
    let text = set_with(
        r#"{"name": "s", "params": {"z.k": 1, "a.k": 2},
            "inputs": {"zeta": 1, "alpha": [{"t": 0, "value": true}]},
            "expect": [{"diagnostic": {"block": "B"}, "at_s": 1},
                       {"output": "y", "from_s": 0, "to_s": 2, "equals": 1.5}]}"#,
    );
    let set = decode(&text, &LIMITS).expect("reads");
    let scenario = &set.scenarios[0];
    let params: Vec<&str> = scenario
        .params
        .0
        .as_ref()
        .expect("params")
        .0
        .iter()
        .map(|(path, _)| path.as_str())
        .collect();
    assert_eq!(params, ["z.k", "a.k"]);
    let inputs: Vec<&str> = scenario
        .inputs
        .0
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(inputs, ["zeta", "alpha"]);
    assert!(matches!(scenario.inputs.0[0].1, InputDto::Constant(_)));
    assert!(matches!(&scenario.inputs.0[1].1, InputDto::Steps(steps) if steps.len() == 1));
    assert!(matches!(scenario.expect[0], ExpectationDto::Instant(_)));
    assert!(matches!(scenario.expect[1], ExpectationDto::Window(_)));
    assert!(set.params.0.is_none(), "an absent key is absent, not empty");
    assert!(scenario.description.0.is_none());
}
