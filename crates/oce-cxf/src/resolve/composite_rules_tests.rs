//! Composite contract-rule catalog guards: byte golden, identity uniqueness, and shape checks.
//!
//! Only the byte golden compares against the checked-in `CATALOG_JSON`; the other tests assert on
//! freshly rendered output or the live table, so a generator/table defect fails them even when a
//! stale or bad re-bless has aligned the artifact with the defect. The `UPDATE_EXPECT=1` write
//! branch is exercised only by explicit mutation probes because it overwrites the artifact; it
//! remains a human-diff-reviewed re-bless path. The emitted-message side of the contract
//! (every tagged rejection starts with its published prefix) lives in the integration suite
//! `tests/resolve_composite_rules.rs`, which drives real imports through the public API.

use std::collections::BTreeSet;

use serde_json::Value as JsonValue;

use super::composite_rules::{CATALOG_JSON, CATALOG_PATH, COMPOSITE_RULES, render_catalog};

#[test]
fn checked_in_catalog_matches_regenerated_bytes() {
    let generated = render_catalog();
    assert_eq!(
        generated,
        render_catalog(),
        "repeated generation must be byte-identical"
    );

    if oce_bless::enabled("UPDATE_EXPECT") {
        std::fs::write(CATALOG_PATH, &generated).expect("write blessed catalog artifact");
        return;
    }

    assert_eq!(
        generated, CATALOG_JSON,
        "tools/reference-catalog/oce-cxf.composite-rules.json is stale. Re-bless deliberately \
         with `UPDATE_EXPECT=1 cargo nextest run -p oce-cxf \
         checked_in_catalog_matches_regenerated_bytes` and review the diff."
    );
}

#[test]
fn rule_ids_and_message_prefixes_are_unique_and_kebab_case() {
    let ids: Vec<&str> = COMPOSITE_RULES.iter().map(|rule| rule.id).collect();
    let unique_ids: BTreeSet<&str> = ids.iter().copied().collect();
    assert_eq!(
        unique_ids.len(),
        ids.len(),
        "rule ids must be unique: {ids:?}"
    );

    let prefixes: Vec<String> = COMPOSITE_RULES
        .iter()
        .map(|rule| rule.message_prefix())
        .collect();
    let unique_prefixes: BTreeSet<&String> = prefixes.iter().collect();
    assert_eq!(
        unique_prefixes.len(),
        prefixes.len(),
        "message prefixes must be unique: {prefixes:?}"
    );

    for rule in &COMPOSITE_RULES {
        assert!(
            !rule.id.is_empty()
                && rule
                    .id
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "rule id must be non-empty kebab-case: {:?}",
            rule.id
        );
        assert_eq!(
            rule.message_prefix(),
            format!("composite/{}: ", rule.id),
            "the prefix convention is `composite/<rule-id>: `"
        );
    }
}

#[test]
fn rendered_catalog_parses_and_maps_every_rule_to_its_diag_code() {
    let catalog: JsonValue =
        serde_json::from_str(&render_catalog()).expect("rendered catalog must be valid JSON");
    let entries = catalog.as_object().expect("top-level JSON object");
    assert_eq!(
        entries.len(),
        COMPOSITE_RULES.len(),
        "catalog must carry exactly one entry per contract rule"
    );
    for rule in &COMPOSITE_RULES {
        let entry = entries
            .get(rule.id)
            .unwrap_or_else(|| panic!("catalog entry missing for rule {:?}", rule.id));
        assert_eq!(
            entry["diag_code"],
            JsonValue::String(rule.code.as_str().to_owned()),
            "{}: catalog diag_code must be the emitting DiagCode string",
            rule.id
        );
        assert_eq!(
            entry["message_prefix"],
            JsonValue::String(rule.message_prefix()),
            "{}: catalog message_prefix must match the emission prefix",
            rule.id
        );
        assert!(
            entry["summary"].as_str().is_some_and(|s| !s.is_empty()),
            "{}: catalog summary must be a non-empty string",
            rule.id
        );
    }
}

#[test]
fn tagged_message_rendering_starts_with_the_published_prefix() {
    for rule in &COMPOSITE_RULES {
        let message = rule.message("detail text");
        assert!(
            message.starts_with(&rule.message_prefix()),
            "{}: message() and message_prefix() must derive from the same id: {message:?}",
            rule.id
        );
        assert_eq!(
            message,
            format!("composite/{}: detail text", rule.id),
            "the tag is a prefix; the remainder stays the human-readable detail"
        );
    }
}
