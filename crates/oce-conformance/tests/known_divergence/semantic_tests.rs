//! Identity, ordering, lifecycle, and semantic validation tests.

use serde_json::{Value, json};

use super::reader::ValidationCode;
use super::test_data::{bytes, code, entry, register};

fn valid(value: &Value) {
    super::reader::read_register(&bytes(value)).expect("schema test record should validate");
}

#[test]
fn complete_schema_record_accepts_open_resolved_and_superseded_states() {
    let open = register(vec![entry("DVG-000001", "open")]);
    valid(&open);

    let mut resolved_entry = entry("DVG-000001", "resolved");
    resolved_entry["state"] = json!("resolved");
    resolved_entry["disposition"] = json!("Schema test resolved by its synthetic review evidence.");
    valid(&register(vec![resolved_entry]));

    let mut superseded = entry("DVG-000001", "old");
    superseded["state"] = json!("superseded");
    superseded["superseded_by"] = json!("DVG-000002");
    superseded["disposition"] = json!("Schema model replaced by DVG-000002.");
    valid(&register(vec![
        superseded,
        entry("DVG-000002", "replacement"),
    ]));
}

#[test]
fn ids_are_exact_unique_and_strictly_ordered() {
    for id in ["DVG-00001", "DVG-0000001", "dvg-000001", "DVG-00000A"] {
        assert_eq!(
            code(&register(vec![entry(id, "bad")])),
            ValidationCode::InvalidId
        );
    }
    let control = register(vec![
        entry("DVG-000001", "first"),
        entry("DVG-000002", "second"),
    ]);
    valid(&control);
    let mut duplicate = control.clone();
    duplicate["entries"][1]["id"] = duplicate["entries"][0]["id"].clone();
    assert_eq!(duplicate["entries"][1]["id"], duplicate["entries"][0]["id"]);
    assert_eq!(code(&duplicate), ValidationCode::IdDuplicate);
    valid(&control);

    let non_adjacent_duplicate = register(vec![
        entry("DVG-000001", "first"),
        entry("DVG-000002", "second"),
        entry("DVG-000001", "third"),
    ]);
    assert_eq!(code(&non_adjacent_duplicate), ValidationCode::IdDuplicate);
    assert_eq!(
        code(&register(vec![
            entry("DVG-000002", "second"),
            entry("DVG-000001", "first"),
        ])),
        ValidationCode::EntryOrder
    );
}

#[test]
fn subjects_and_producer_cases_are_unique_across_entries() {
    let first = entry("DVG-000001", "first");
    let mut duplicate_subject = entry("DVG-000002", "second");
    duplicate_subject["subject"] = first["subject"].clone();
    assert_eq!(
        code(&register(vec![first.clone(), duplicate_subject])),
        ValidationCode::SubjectDuplicate
    );

    let mut duplicate_case = entry("DVG-000002", "second");
    duplicate_case["producer_cases"] = first["producer_cases"].clone();
    assert_eq!(
        code(&register(vec![first, duplicate_case])),
        ValidationCode::ProducerCaseGlobalDuplicate
    );
}

#[test]
fn producer_cases_refuse_empty_duplicate_and_noncanonical_arrays() {
    let base = entry("DVG-000001", "base");
    let mut empty = base.clone();
    empty["producer_cases"] = json!([]);
    assert_eq!(
        code(&register(vec![empty])),
        ValidationCode::ProducerCaseCount
    );

    let mut duplicate = base.clone();
    duplicate["producer_cases"][1] = duplicate["producer_cases"][0].clone();
    assert_eq!(
        code(&register(vec![duplicate])),
        ValidationCode::ProducerCaseDuplicate
    );

    let mut non_adjacent_duplicate = base.clone();
    non_adjacent_duplicate["producer_cases"][2] =
        non_adjacent_duplicate["producer_cases"][0].clone();
    assert_eq!(
        code(&register(vec![non_adjacent_duplicate])),
        ValidationCode::ProducerCaseDuplicate
    );

    let mut out_of_order = base;
    out_of_order["producer_cases"]
        .as_array_mut()
        .unwrap()
        .swap(0, 1);
    assert_eq!(
        code(&register(vec![out_of_order])),
        ValidationCode::ProducerCaseOrder
    );
}

#[test]
fn parties_refuse_short_duplicate_and_noncanonical_arrays() {
    let base = entry("DVG-000001", "base");
    let mut short = base.clone();
    short["parties"] = json!(["analytical"]);
    assert_eq!(code(&register(vec![short])), ValidationCode::PartyCount);

    let mut duplicate = base.clone();
    duplicate["parties"] = json!(["analytical", "analytical", "engine"]);
    assert_eq!(
        code(&register(vec![duplicate])),
        ValidationCode::PartyDuplicate
    );

    let mut non_adjacent_duplicate = base.clone();
    non_adjacent_duplicate["parties"][2] = non_adjacent_duplicate["parties"][0].clone();
    assert_eq!(
        code(&register(vec![non_adjacent_duplicate])),
        ValidationCode::PartyDuplicate
    );

    let mut out_of_order = base;
    out_of_order["parties"].as_array_mut().unwrap().swap(0, 1);
    assert_eq!(
        code(&register(vec![out_of_order])),
        ValidationCode::PartyOrder
    );
}

#[test]
fn evidence_refuses_duplicates_order_drift_and_party_mismatch() {
    let base = entry("DVG-000001", "base");
    let mut duplicate = base.clone();
    duplicate["evidence"][1] = duplicate["evidence"][0].clone();
    assert_eq!(
        code(&register(vec![duplicate])),
        ValidationCode::EvidenceDuplicate
    );

    let mut non_adjacent_duplicate = base.clone();
    non_adjacent_duplicate["evidence"][2] = non_adjacent_duplicate["evidence"][0].clone();
    assert_eq!(
        code(&register(vec![non_adjacent_duplicate])),
        ValidationCode::EvidenceDuplicate
    );

    let mut out_of_order = base.clone();
    out_of_order["evidence"].as_array_mut().unwrap().swap(0, 1);
    assert_eq!(
        code(&register(vec![out_of_order])),
        ValidationCode::EvidenceOrder
    );

    let mut unlisted = base.clone();
    unlisted["evidence"][0]["party"] = json!("dymola");
    assert_eq!(
        code(&register(vec![unlisted])),
        ValidationCode::PartyEvidence
    );

    let mut missing = base;
    missing["evidence"].as_array_mut().unwrap().remove(0);
    assert_eq!(
        code(&register(vec![missing])),
        ValidationCode::EvidenceCount
    );
}

#[test]
fn paths_are_ascii_canonical_and_repository_relative() {
    for path in [
        "/absolute/file",
        "../outside",
        "inside/../outside",
        "inside/./file",
        "inside//file",
        "https://example.test/evidence",
        "C:/absolute/file",
        "inside\\file",
        "inside/dir./file",
        "inside/dir /file",
    ] {
        let mut value = entry("DVG-000001", "base");
        value["evidence"][0]["path"] = json!(path);
        assert_eq!(
            code(&register(vec![value])),
            ValidationCode::InvalidPath,
            "{path}"
        );
    }
    let mut comparison_glob = entry("DVG-000001", "base");
    comparison_glob["comparison_reference"] = json!("crates/*/comparison.rs");
    assert_eq!(
        code(&register(vec![comparison_glob])),
        ValidationCode::InvalidPath
    );

    let mut subject_glob = entry("DVG-000001", "base");
    subject_glob["subject"]["scenario"] = json!("all_*");
    assert_eq!(
        code(&register(vec![subject_glob])),
        ValidationCode::InvalidString
    );

    let mut literal_brackets = entry("DVG-000001", "base");
    literal_brackets["subject"]["signal"] = json!("y[01]");
    valid(&register(vec![literal_brackets]));
    for wildcard in ["y*", "y?"] {
        let mut value = entry("DVG-000001", "base");
        value["subject"]["signal"] = json!(wildcard);
        assert_eq!(code(&register(vec![value])), ValidationCode::InvalidString);
    }
}

#[test]
fn digests_commits_and_human_strings_are_closed_and_trimmed() {
    for digest in ["0", &"A".repeat(64)] {
        let mut value = entry("DVG-000001", "base");
        value["evidence"][0]["sha256"] = json!(digest);
        assert_eq!(code(&register(vec![value])), ValidationCode::InvalidDigest);
    }
    for commit in ["0", &"A".repeat(40)] {
        let mut value = entry("DVG-000001", "base");
        value["reviewed_commit"] = json!(commit);
        assert_eq!(code(&register(vec![value])), ValidationCode::InvalidCommit);
    }
    for summary in ["", " leading", "trailing ", "control\ncharacter"] {
        let mut value = entry("DVG-000001", "base");
        value["summary"] = json!(summary);
        assert_eq!(code(&register(vec![value])), ValidationCode::InvalidString);
    }
    let mut non_ascii_identity = entry("DVG-000001", "base");
    non_ascii_identity["subject"]["signal"] = json!("yé");
    assert_eq!(
        code(&register(vec![non_ascii_identity])),
        ValidationCode::InvalidString
    );
}

#[test]
fn dates_are_canonical_valid_and_review_follows_opening() {
    for date in ["2026-8-01", "2026-02-29", "2026-13-01", "0000-01-01"] {
        let mut value = entry("DVG-000001", "base");
        value["opened_on"] = json!(date);
        assert_eq!(
            code(&register(vec![value])),
            ValidationCode::InvalidDate,
            "{date}"
        );
    }
    let mut leap = entry("DVG-000001", "base");
    leap["opened_on"] = json!("2024-02-29");
    valid(&register(vec![leap]));

    let mut reversed = entry("DVG-000001", "base");
    reversed["opened_on"] = json!("2026-08-11");
    assert_eq!(
        code(&register(vec![reversed])),
        ValidationCode::ReviewBeforeOpen
    );
}

#[test]
fn lifecycle_refuses_state_target_disagreement_and_bad_targets() {
    let mut open_target = entry("DVG-000001", "base");
    open_target["superseded_by"] = json!("DVG-000002");
    assert_eq!(
        code(&register(vec![open_target])),
        ValidationCode::Lifecycle
    );

    let mut missing_target = entry("DVG-000001", "base");
    missing_target["state"] = json!("superseded");
    missing_target["superseded_by"] = json!("DVG-999999");
    assert_eq!(
        code(&register(vec![missing_target])),
        ValidationCode::SupersessionTarget
    );

    let mut self_target = entry("DVG-000001", "base");
    self_target["state"] = json!("superseded");
    self_target["superseded_by"] = json!("DVG-000001");
    assert_eq!(
        code(&register(vec![self_target])),
        ValidationCode::SupersessionSelf
    );
}

#[test]
fn supersession_cycle_mutation_is_predicted_red_and_control_is_restored() {
    let mut first = entry("DVG-000001", "first");
    first["state"] = json!("superseded");
    first["superseded_by"] = json!("DVG-000002");
    let second = entry("DVG-000002", "second");
    let control = register(vec![first, second]);
    valid(&control);

    let mut mutation = control.clone();
    mutation["entries"][1]["state"] = json!("superseded");
    mutation["entries"][1]["superseded_by"] = json!("DVG-000001");
    assert_eq!(code(&mutation), ValidationCode::SupersessionCycle);
    valid(&control);
}

#[test]
fn upstream_status_url_combinations_are_exact() {
    for issue in [
        json!({"status":"filed","url":null}),
        json!({"status":"filed","url":"http://example.test/issue/1"}),
        json!({"status":"filed","url":"https://example.test/issue 1"}),
        json!({"status":"filed","url":"https://?issue=1"}),
        json!({"status":"filed","url":"https://#issue-1"}),
        json!({"status":"filed","url":"https://user@github.com/issues/1"}),
        json!({"status":"filed","url":"https://github.com:443/issues/1"}),
        json!({"status":"filed","url":"https://.github.com/issues/1"}),
        json!({"status":"filed","url":"https://github..com/issues/1"}),
        json!({"status":"filed","url":"https://github.com./issues/1"}),
        json!({"status":"filed","url":"https://-github.com/issues/1"}),
        json!({"status":"filed","url":"https://github-.com/issues/1"}),
        json!({"status":"filed","url":"https://git_hub.com/issues/1"}),
        json!({"status":"not_filed","url":"https://example.test/issue/1"}),
        json!({"status":"not_applicable","url":"https://example.test/issue/1"}),
    ] {
        let mut value = entry("DVG-000001", "base");
        value["upstream_issue"] = issue;
        assert_eq!(code(&register(vec![value])), ValidationCode::UpstreamIssue);
    }
    let mut filed = entry("DVG-000001", "base");
    filed["upstream_issue"] = json!({
        "status":"filed",
        "url":"https://example.test/issue/1"
    });
    valid(&register(vec![filed]));

    for url in [
        "https://github.com/lbl-srg/modelica-buildings/issues/123",
        "https://github.com/lbl-srg/modelica-buildings/issues/123?state=open#discussion",
        "https://github.com?issue=123",
        "https://github.com#issue-123",
    ] {
        let mut value = entry("DVG-000001", "base");
        value["upstream_issue"] = json!({"status":"filed","url":url});
        valid(&register(vec![value]));
    }
}

fn implementation_evidence(party: &str) -> Value {
    json!({
        "party": party,
        "kind": if party == "engine" { "engine_run" } else { "external_run" },
        "path": super::test_data::ARTIFACT_PATH,
        "sha256": super::test_data::ARTIFACT_SHA256,
        "summary": format!("Schema-test evidence for {party}.")
    })
}

#[test]
fn resolved_three_way_implementation_disagreement_requires_filed_issue() {
    let mut value = entry("DVG-000001", "base");
    value["parties"] = json!(["engine", "dymola", "open_modelica"]);
    value["evidence"] = Value::Array(
        ["engine", "dymola", "open_modelica"]
            .map(implementation_evidence)
            .into(),
    );
    value["state"] = json!("resolved");
    value["upstream_issue"] = json!({"status":"not_filed","url":null});
    assert_eq!(
        code(&register(vec![value.clone()])),
        ValidationCode::ThreeWayIssue
    );

    value["upstream_issue"] = json!({
        "status":"filed",
        "url":"https://example.test/issue/1"
    });
    valid(&register(vec![value]));
}
