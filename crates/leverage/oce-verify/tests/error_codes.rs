//! Every load-stage code of the vectors note's table is produced by at least one document, and
//! the list is pinned: one small case per code, each asserted to give exactly that code at the
//! place it names. The case table and [`oce_verify::VECTOR_ERROR_CODES`] must cover each other,
//! so a new kind needs a case here before the suite passes.

mod support;

use std::collections::BTreeSet;

use oce_verify::{
    MAX_DEPTH, MAX_LIST_LEN, MAX_STRING_BYTES, VECTOR_ERROR_CODES, VectorSet, VerifyLimits,
};

use support::{one, scenario, set_of, set_on};

/// The note's load rows, in the order `VectorErrorKind` declares them: the table's order, with
/// the two rows of the mutant list's paths last.
const PINNED: &[&str] = &[
    "not-utf8",
    "byte-order-mark",
    "json-syntax",
    "input-too-large",
    "nesting-too-deep",
    "list-too-long",
    "string-too-long",
    "duplicate-key",
    "unknown-key",
    "missing-key",
    "wrong-type",
    "non-finite-number",
    "integer-out-of-range",
    "control-character",
    "unknown-schema",
    "invalid-clock",
    "horizon-off-cadence",
    "too-many-ticks",
    "too-many-scenarios",
    "too-many-expectations",
    "too-many-mutants",
    "too-many-parameters",
    "duplicate-scenario",
    "empty-name",
    "step-list-empty",
    "step-list-not-from-zero",
    "step-times-not-increasing",
    "step-after-horizon",
    "invalid-window",
    "window-outside-clock",
    "window-uncovered",
    "ambiguous-expectation",
    "instant-off-cadence",
    "mutant-path-invalid",
    "duplicate-mutant",
];

#[test]
fn the_code_list_is_pinned() {
    assert_eq!(VECTOR_ERROR_CODES, PINNED);
}

const WINDOW: &str = r#"{"output": "y", "from_s": 0, "to_s": 1800, "equals": false}"#;

fn many(item: &str, count: usize) -> String {
    vec![item; count].join(",")
}

/// One document per code: the code, the subject the refusal must name, and the bytes.
fn cases() -> Vec<(&'static str, String, Vec<u8>)> {
    let text = |code: &'static str, subject: &str, document: String| {
        (code, subject.to_owned(), document.into_bytes())
    };
    let mut not_utf8 = set_of("").into_bytes();
    not_utf8[20] = 0xff;
    let named = |name: &str| format!("{{\"name\": \"{name}\", \"inputs\": {{}}, \"expect\": []}}");
    vec![
        ("not-utf8", "document".to_owned(), not_utf8),
        text(
            "byte-order-mark",
            "document",
            format!("\u{feff}{}", set_of("")),
        ),
        text(
            "json-syntax",
            "line 1 column 91",
            format!("{} ,", set_of("")),
        ),
        (
            "input-too-large",
            "document".to_owned(),
            vec![b' '; 4 * 1024 * 1024 + 1],
        ),
        text(
            "nesting-too-deep",
            "line 1 column 9",
            format!("{}{}", "[".repeat(MAX_DEPTH + 1), "]".repeat(MAX_DEPTH + 1)),
        ),
        text(
            "list-too-long",
            "line 1 column 118",
            one(
                &format!(
                    "\"u\": [{}]",
                    many("{\"t\": 0, \"value\": 0}", MAX_LIST_LEN + 1)
                ),
                "",
            ),
        ),
        text(
            "string-too-long",
            "line 1 column 97",
            set_of(&named(&"n".repeat(MAX_STRING_BYTES + 1))),
        ),
        text(
            "duplicate-key",
            "line 1 column 121",
            one("\"u\": 1, \"u\": 2", ""),
        ),
        text(
            "unknown-key",
            "line 1 column 132",
            format!(
                "{{\"schema\": \"oce/vectors/v1\", \"clock\": {{\"step_s\": 60, \"horizon_s\": 1800}}, \"scenarios\": [{}], \"within\": []}}",
                scenario("", "")
            ),
        ),
        text(
            "missing-key",
            "line 1 column 1",
            r#"{"schema": "oce/vectors/v1", "scenarios": []}"#.to_owned(),
        ),
        text("wrong-type", "line 1 column 118", one("\"u\": \"on\"", "")),
        text(
            "non-finite-number",
            "line 1 column 118",
            one("\"u\": NaN", ""),
        ),
        text(
            "integer-out-of-range",
            "line 1 column 118",
            one("\"u\": 9223372036854775808", ""),
        ),
        text(
            "control-character",
            "line 1 column 101",
            set_of(&named("tab\\there")),
        ),
        text(
            "unknown-schema",
            "schema",
            set_of("").replace("oce/vectors/v1", "cxf-library/vectors/v1"),
        ),
        text("invalid-clock", "clock.step_s", set_on("0", "1800", "")),
        text(
            "horizon-off-cadence",
            "clock.horizon_s",
            set_on("60", "1830", ""),
        ),
        text("too-many-ticks", "clock", set_on("1", "16777216", "")),
        text(
            "too-many-scenarios",
            "line 1 column 187394",
            set_of(
                &(0..=4_096)
                    .map(|index| named(&format!("s{index}")))
                    .collect::<Vec<_>>()
                    .join(","),
            ),
        ),
        text(
            "too-many-expectations",
            "line 1 column 245887",
            one("", &many(WINDOW, 4_097)),
        ),
        text(
            "too-many-mutants",
            &format!(
                "line 1 column {}",
                nth_start(&mutants(257), "{\"path\"", 257)
            ),
            mutants(257),
        ),
        text(
            "too-many-parameters",
            "line 1 column 3369",
            format!(
                "{{\"schema\": \"oce/vectors/v1\", \"params\": {{{}}}}}",
                (0..=256)
                    .map(|index| format!("\"p{index:03}.k\": 0"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ),
        text(
            "duplicate-scenario",
            "scenarios[2].name",
            set_of(&[named("a"), named("b"), named("a")].join(",")),
        ),
        text("empty-name", "scenarios[0].name", set_of(&named(""))),
        text(
            "step-list-empty",
            "scenarios[0].inputs.u",
            one("\"u\": []", ""),
        ),
        text(
            "step-list-not-from-zero",
            "scenarios[0].inputs.u[0].t",
            one("\"u\": [{\"t\": 60, \"value\": 1}]", ""),
        ),
        text(
            "step-times-not-increasing",
            "scenarios[0].inputs.u[2].t",
            one(
                "\"u\": [{\"t\": 0, \"value\": 1}, {\"t\": 60, \"value\": 2}, {\"t\": 60, \"value\": 3}]",
                "",
            ),
        ),
        text(
            "step-after-horizon",
            "scenarios[0].inputs.u[1].t",
            one(
                "\"u\": [{\"t\": 0, \"value\": 1}, {\"t\": 1801, \"value\": 2}]",
                "",
            ),
        ),
        text(
            "invalid-window",
            "scenarios[0].expect[0]",
            one(
                "",
                r#"{"output": "y", "from_s": 120, "to_s": 60, "equals": false}"#,
            ),
        ),
        text(
            "window-outside-clock",
            "scenarios[0].expect[1]",
            one(
                "",
                &format!(
                    "{WINDOW}, {}",
                    r#"{"output": "y", "from_s": 0, "to_s": 1860, "equals": false}"#
                ),
            ),
        ),
        text(
            "window-uncovered",
            "scenarios[0].expect[0]",
            one(
                "",
                r#"{"output": "y", "from_s": 61, "to_s": 119, "equals": false}"#,
            ),
        ),
        text(
            "ambiguous-expectation",
            "line 1 column 127",
            one(
                "",
                r#"{"output": "y", "from_s": 0, "to_s": 60, "equals": false, "at_s": 60}"#,
            ),
        ),
        text(
            "instant-off-cadence",
            "scenarios[0].expect[0].at_s",
            one("", r#"{"diagnostic": {"block": "B"}, "at_s": 90}"#),
        ),
        text(
            "mutant-path-invalid",
            "mutants[1].path",
            listing(&["m/a.cxf.json", "m/../a.cxf.json"]),
        ),
        text(
            "duplicate-mutant",
            "mutants[2].path",
            listing(&["m/a.cxf.json", "m/b.cxf.json", "m/a.cxf.json"]),
        ),
    ]
}

/// A set with no scenario listing `count` mutants, `m000.cxf.json` and up.
fn mutants(count: usize) -> String {
    let paths: Vec<String> = (0..count)
        .map(|index| format!("m{index:03}.cxf.json"))
        .collect();
    let paths: Vec<&str> = paths.iter().map(String::as_str).collect();
    listing(&paths)
}

/// A set with no scenario listing mutants at `paths`, in order.
fn listing(paths: &[&str]) -> String {
    let entries: Vec<String> = paths
        .iter()
        .map(|path| format!("{{\"path\": \"{path}\"}}"))
        .collect();
    format!(
        "{{\"schema\": \"oce/vectors/v1\", \"clock\": {{\"step_s\": 60, \"horizon_s\": 1800}}, \"mutants\": [{}], \"scenarios\": []}}",
        entries.join(", ")
    )
}

/// The 1-based column at which the `n`th occurrence of `needle` starts in one-line `text`.
fn nth_start(text: &str, needle: &str, n: usize) -> usize {
    text.match_indices(needle)
        .nth(n - 1)
        .map(|(at, _)| at + 1)
        .expect("enough occurrences")
}

#[test]
fn every_code_is_produced_by_its_case_at_the_place_it_names() {
    for (code, subject, bytes) in cases() {
        let refused = VectorSet::from_json_bytes(&bytes, &VerifyLimits::default())
            .expect_err("every case is a refusal");
        assert_eq!(refused.code(), code, "{refused}");
        assert_eq!(refused.subject(), subject, "{code}: {refused}");
        assert_eq!(refused.kind().code(), code);
        assert!(!refused.detail().is_empty(), "{code}");
        // A refusal found in the text has a location; one placed by path or on the bytes has
        // none, and its subject is the path.
        assert_eq!(
            refused.at().is_some(),
            subject.starts_with("line "),
            "{code}"
        );
        if let Some(at) = refused.at() {
            assert_eq!(subject, format!("line {} column {}", at.line, at.column));
        }
        assert_eq!(
            refused.to_string(),
            format!("{subject}: {}", refused.detail())
        );
    }
}

#[test]
fn the_cases_and_the_code_list_cover_each_other() {
    let covered: Vec<&str> = cases().iter().map(|(code, _, _)| *code).collect();
    assert_eq!(covered, VECTOR_ERROR_CODES, "one case per code, in order");
    let unique: BTreeSet<&str> = covered.iter().copied().collect();
    assert_eq!(unique.len(), VECTOR_ERROR_CODES.len());
}

#[test]
fn a_refusal_is_the_same_on_every_load() {
    for (code, _, bytes) in cases() {
        let first = VectorSet::from_json_bytes(&bytes, &VerifyLimits::default());
        let second = VectorSet::from_json_bytes(&bytes, &VerifyLimits::default());
        assert_eq!(first, second, "{code}");
    }
}

#[test]
fn each_case_stops_being_refused_when_its_one_defect_is_removed() {
    // The controls: the same documents without their defect load, so each case above is
    // refused for the defect it names and for nothing else.
    for healthy in [
        set_of(""),
        set_of(&scenario("", "")),
        one("\"u\": 1", ""),
        one("\"u\": true", ""),
        one("\"u\": 9223372036854775807", ""),
        one("\"u\": [{\"t\": 0, \"value\": 1}]", ""),
        one(
            "\"u\": [{\"t\": 0, \"value\": 1}, {\"t\": 60, \"value\": 2}, {\"t\": 61, \"value\": 3}]",
            "",
        ),
        one(
            "\"u\": [{\"t\": 0, \"value\": 1}, {\"t\": 1800, \"value\": 2}]",
            "",
        ),
        one("", WINDOW),
        one(
            "",
            r#"{"output": "y", "from_s": 60, "to_s": 120, "equals": false}"#,
        ),
        one(
            "",
            r#"{"output": "y", "from_s": 60, "to_s": 119, "equals": false}"#,
        ),
        one("", r#"{"diagnostic": {"block": "B"}, "at_s": 60}"#),
        one("", &many(WINDOW, 4_096)),
        set_on("60", "1800", ""),
        set_on("1", "16777215", ""),
    ] {
        assert_eq!(support::code_of(&healthy), "loaded", "{healthy:.200}");
    }
}
