//! Strict-schema, anti-allowlist, determinism, and resource-bound tests.

use serde_json::{Value, json};

use super::reader::{
    MAX_ENTRIES, MAX_EVIDENCE, MAX_HUMAN_TEXT_BYTES, MAX_IDENTITY_BYTES, MAX_INPUT_BYTES,
    MAX_PARTIES, MAX_PATH_BYTES, MAX_PRODUCER_CASES, MAX_SUMMARY_BYTES, MAX_URL_BYTES,
    ValidationCode, parse_register, read_register,
};
use super::test_data::{bytes, code, entry, register};

const CANONICAL: &[u8] = include_bytes!("../fixtures/known_divergence/register.json");
const CANONICAL_TEXT: &str =
    "{\n  \"format\": \"oce-known-divergence-register-v1\",\n  \"entries\": []\n}\n";

#[test]
fn canonical_golden_is_exactly_empty_and_repeated_reads_are_equal() {
    assert_eq!(CANONICAL, CANONICAL_TEXT.as_bytes());
    let first = read_register(CANONICAL).expect("canonical register validates");
    let second = read_register(CANONICAL).expect("canonical register validates again");
    assert_eq!(first, second);
    assert!(first.entries.is_empty());
}

#[test]
fn unknown_fields_are_refused_at_every_object_level_with_entry_context() {
    let mut cases = Vec::new();
    let mut top = register(vec![entry("DVG-000001", "base")]);
    top.as_object_mut()
        .unwrap()
        .insert("unknown".into(), json!(true));
    cases.push((top, None));

    for pointer in [
        "/entries/0",
        "/entries/0/subject",
        "/entries/0/producer_cases/0",
        "/entries/0/evidence/0",
        "/entries/0/upstream_issue",
    ] {
        let mut value = register(vec![entry("DVG-000001", "base")]);
        value
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unknown".into(), json!(true));
        cases.push((value, Some(0)));
    }

    for (value, expected_entry) in cases {
        let error = read_register(&bytes(&value)).expect_err("unknown field rejected");
        assert_eq!(error.code, ValidationCode::Schema);
        assert_eq!(error.entry, expected_entry);
    }
}

#[test]
fn malformed_duplicate_missing_and_wrong_shape_inputs_have_stable_classes() {
    let cases: &[(&[u8], ValidationCode, Option<usize>)] = &[
        (b"{", ValidationCode::JsonSyntax, None),
        (b"[]", ValidationCode::Schema, None),
        (
            b"{\"format\":\"oce-known-divergence-register-v1\"}",
            ValidationCode::Schema,
            None,
        ),
        (
            b"{\"format\":\"oce-known-divergence-register-v1\",\"format\":\"oce-known-divergence-register-v1\",\"entries\":[]}",
            ValidationCode::Schema,
            None,
        ),
        (
            b"{\"format\":\"oce-known-divergence-register-v1\",\"entries\":[{}]}",
            ValidationCode::Schema,
            Some(0),
        ),
        (&[0xff], ValidationCode::JsonSyntax, None),
    ];
    for (input, expected, entry) in cases {
        let error = read_register(input).expect_err("invalid input rejected");
        assert_eq!(error.code, *expected, "{}", error.detail);
        assert_eq!(error.entry, *entry, "{}", error.detail);
    }

    let text = String::from_utf8(bytes(&register(vec![entry("DVG-000001", "base")]))).unwrap();
    let mutated = text.replacen(
        "\"state\":\"open\"",
        "\"state\":\"open\",\"state\":\"open\"",
        1,
    );
    let error = read_register(mutated.as_bytes()).expect_err("duplicate entry field rejected");
    assert_eq!(error.code, ValidationCode::Schema);
    assert_eq!(error.entry, Some(0));

    let mut missing_nested = entry("DVG-000001", "base");
    missing_nested["evidence"][0]
        .as_object_mut()
        .unwrap()
        .remove("sha256");
    assert_eq!(
        code(&register(vec![missing_nested])),
        ValidationCode::Schema
    );
}

#[test]
fn format_and_closed_enums_are_refused_semantically_or_by_schema() {
    let mut wrong_format = register(vec![]);
    wrong_format["format"] = json!("oce-known-divergence-register-v0");
    assert_eq!(code(&wrong_format), ValidationCode::Format);

    for (pointer, replacement) in [
        ("/entries/0/conformance_effect", json!("allows_failure")),
        ("/entries/0/state", json!("ignored")),
        ("/entries/0/parties/0", json!("oracle")),
        ("/entries/0/producer_cases/0/producer", json!("other")),
        ("/entries/0/evidence/0/kind", json!("waiver")),
        ("/entries/0/upstream_issue/status", json!("closed")),
    ] {
        let mut value = register(vec![entry("DVG-000001", "base")]);
        *value.pointer_mut(pointer).unwrap() = replacement;
        assert_eq!(code(&value), ValidationCode::Schema, "{pointer}");
    }
}

#[test]
fn anti_allowlist_fields_are_predicted_red_and_restore_to_valid() {
    for field in [
        "tolerance",
        "atoly",
        "rtoly",
        "allowed",
        "ignore",
        "xfail",
        "suppresses_failure",
        "expected",
    ] {
        let control = register(vec![entry("DVG-000001", "base")]);
        read_register(&bytes(&control)).expect("unmutated schema control is valid");
        let mut mutation = control.clone();
        mutation["entries"][0]
            .as_object_mut()
            .unwrap()
            .insert(field.into(), json!(true));
        assert_eq!(code(&mutation), ValidationCode::Schema, "{field}");
        read_register(&bytes(&control)).expect("mutation is restored in the same test");
    }
}

#[test]
fn input_cap_precedes_deserialization_and_accepts_the_exact_boundary() {
    let mut exact = CANONICAL.to_vec();
    exact.resize(MAX_INPUT_BYTES, b' ');
    read_register(&exact).expect("exact input-byte boundary accepted");
    exact.push(b'{');
    let error = read_register(&exact).expect_err("over-limit bytes rejected");
    assert_eq!(error.code, ValidationCode::InputTooLarge);
}

#[test]
fn entry_and_nested_collection_caps_are_exact() {
    let one = entry("DVG-000001", "base");
    let exact_entries = register(vec![one.clone(); MAX_ENTRIES]);
    let parsed = parse_register(&bytes(&exact_entries)).expect("entry cap parses");
    assert_eq!(parsed.entries.len(), MAX_ENTRIES);
    let over_entries = register(vec![one.clone(); MAX_ENTRIES + 1]);
    let error = parse_register(&bytes(&over_entries)).unwrap_err();
    assert_eq!(error.code, ValidationCode::EntryCount);
    assert_eq!(error.entry, Some(MAX_ENTRIES));

    for (pointer, item, exact, expected) in [
        (
            "/entries/0/producer_cases",
            json!({"producer":"clean_room","case_id":"schema-case"}),
            MAX_PRODUCER_CASES,
            ValidationCode::ProducerCaseCount,
        ),
        (
            "/entries/0/parties",
            json!("engine"),
            MAX_PARTIES,
            ValidationCode::PartyCount,
        ),
        (
            "/entries/0/evidence",
            entry("DVG-000001", "base")["evidence"][0].clone(),
            MAX_EVIDENCE,
            ValidationCode::EvidenceCount,
        ),
    ] {
        let mut value = register(vec![one.clone()]);
        *value.pointer_mut(pointer).unwrap() = Value::Array(vec![item.clone(); exact]);
        parse_register(&bytes(&value)).expect("exact nested collection cap parses");
        *value.pointer_mut(pointer).unwrap() = Value::Array(vec![item; exact + 1]);
        assert_eq!(parse_register(&bytes(&value)).unwrap_err().code, expected);
    }
}

#[test]
fn string_family_caps_accept_boundary_and_reject_over_limit() {
    let cases = [
        ("/entries/0/summary", MAX_SUMMARY_BYTES, "x"),
        ("/entries/0/subject/scenario", MAX_IDENTITY_BYTES, "x"),
        ("/entries/0/disposition", MAX_HUMAN_TEXT_BYTES, "x"),
        ("/entries/0/comparison_reference", MAX_PATH_BYTES, "x"),
    ];
    for (pointer, limit, fill) in cases {
        let mut exact = register(vec![entry("DVG-000001", "base")]);
        *exact.pointer_mut(pointer).unwrap() = json!(fill.repeat(limit));
        if pointer.ends_with("comparison_reference") {
            *exact.pointer_mut(pointer).unwrap() = json!(format!("a/{}", "x".repeat(limit - 2)));
        }
        read_register(&bytes(&exact)).expect("exact string boundary accepted");
        let mut over = exact;
        let current = over.pointer(pointer).unwrap().as_str().unwrap();
        *over.pointer_mut(pointer).unwrap() = json!(format!("{current}x"));
        assert_eq!(code(&over), ValidationCode::InvalidString, "{pointer}");
    }

    let mut filed = register(vec![entry("DVG-000001", "base")]);
    filed["entries"][0]["upstream_issue"] = json!({
        "status": "filed",
        "url": format!("https://x/{}", "a".repeat(MAX_URL_BYTES - 10))
    });
    read_register(&bytes(&filed)).expect("exact URL boundary accepted");
    filed["entries"][0]["upstream_issue"]["url"] =
        json!(format!("https://x/{}", "a".repeat(MAX_URL_BYTES - 9)));
    assert_eq!(code(&filed), ValidationCode::UpstreamIssue);
}

#[test]
fn validation_precedence_is_deterministic() {
    let mut value = register(vec![entry("bad", "base")]);
    value["entries"][0]["reviewed_on"] = json!("not-a-date");
    let first = read_register(&bytes(&value)).unwrap_err();
    let second = read_register(&bytes(&value)).unwrap_err();
    assert_eq!(first, second);
    assert_eq!(first.code, ValidationCode::InvalidId);
    assert_eq!(first.entry, Some(0));
}
