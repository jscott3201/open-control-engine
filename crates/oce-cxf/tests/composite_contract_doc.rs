//! Doc-vs-catalog drift guard for `docs/cxf-composite-subset.md`.
//!
//! Row-parse contract (the doc's rule-catalog table and this test move together): the doc
//! contains EXACTLY ONE markdown table whose header row is
//! `| Rule | Rule id | DiagCode | Message prefix |`. Rows are plain `|`-delimited with that
//! fixed column order; the separator line follows the header; rows end at the first
//! non-`|` line. Cells are read by splitting on `|`, trimming whitespace, and stripping
//! backticks.
//!
//! Only the *Rule id* and *DiagCode* cells are compared against the published catalog. The
//! *Message prefix* cell is NEVER parsed: every catalog `message_prefix` ends with a trailing
//! space, which trim-based markdown-cell parsing cannot round-trip. The prefix leg is instead
//! DERIVED — `format!("composite/{rule_id}: ")` from the doc's rule-id cell — and compared to
//! the catalog's stored `message_prefix`, the same construction `CompositeRule::message_prefix`
//! uses in-crate, so a doc rule-id edit breaks this leg even if the catalog lookup is bypassed.

use std::collections::BTreeMap;

use serde_json::Value as JsonValue;

/// The published contract document — the drift-guard subject.
const DOC: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/cxf-composite-subset.md"
));

/// The checked-in machine-readable rule catalog — the drift-guard authority.
const CATALOG_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tools/reference-catalog/oce-cxf.composite-rules.json"
));

/// The fixed table header the row parser keys on (see the module header's parse contract).
const TABLE_HEADER: &str = "| Rule | Rule id | DiagCode | Message prefix |";

/// Split one table line into trimmed, backtick-stripped cells.
fn cells(line: &str) -> Vec<String> {
    line.trim()
        .trim_matches('|')
        .split('|')
        .map(|cell| cell.trim().trim_matches('`').to_owned())
        .collect()
}

/// Parse the doc's rule-catalog table into (rule_id, diag_code) pairs, row order preserved.
fn doc_table_rows() -> Vec<(String, String)> {
    let lines: Vec<&str> = DOC.lines().collect();
    let header_positions: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| (line.trim() == TABLE_HEADER).then_some(index))
        .collect();
    assert_eq!(
        header_positions.len(),
        1,
        "the doc must contain exactly one rule-catalog table header {TABLE_HEADER:?}"
    );
    let header = header_positions[0];
    assert!(
        lines
            .get(header + 1)
            .is_some_and(|line| line.starts_with('|') && line.contains("---")),
        "the rule-catalog table header must be followed by a separator row"
    );
    lines[header + 2..]
        .iter()
        .take_while(|line| line.trim_start().starts_with('|'))
        .map(|line| {
            let row = cells(line);
            assert_eq!(
                row.len(),
                4,
                "every rule-catalog row must carry exactly 4 cells, got {row:?}"
            );
            (row[1].clone(), row[2].clone())
        })
        .collect()
}

/// The catalog artifact as `rule_id -> entry` (order-independent — this guard checks identity,
/// not presentation order).
fn catalog_entries() -> BTreeMap<String, JsonValue> {
    let catalog: JsonValue = serde_json::from_str(CATALOG_JSON).expect("catalog parses as JSON");
    catalog
        .as_object()
        .expect("catalog top-level object")
        .iter()
        .map(|(rule_id, entry)| (rule_id.clone(), entry.clone()))
        .collect()
}

#[test]
fn doc_table_rule_ids_and_diag_codes_match_the_published_catalog() {
    let rows = doc_table_rows();
    let catalog = catalog_entries();
    assert_eq!(
        rows.len(),
        catalog.len(),
        "the doc table and the catalog must publish the same number of rules"
    );
    for (rule_id, diag_code) in &rows {
        let entry = catalog.get(rule_id).unwrap_or_else(|| {
            panic!("doc table names rule {rule_id:?}, which the catalog does not publish")
        });
        assert_eq!(
            entry["diag_code"].as_str().expect("diag_code string"),
            diag_code,
            "rule {rule_id:?}: the doc's DiagCode cell must match the catalog"
        );
    }
    for rule_id in catalog.keys() {
        assert!(
            rows.iter().any(|(doc_id, _)| doc_id == rule_id),
            "catalog rule {rule_id:?} is missing from the doc table"
        );
    }
}

#[test]
fn doc_table_prefixes_derive_from_rule_ids_and_match_the_stored_catalog_prefixes() {
    // The derived leg: never read the doc's prefix cell (trailing-space hazard) — rebuild the
    // prefix from the rule-id cell and hold it to the catalog's stored `message_prefix`.
    let catalog = catalog_entries();
    for (rule_id, _) in doc_table_rows() {
        let derived = format!("composite/{rule_id}: ");
        let stored = catalog
            .get(&rule_id)
            .unwrap_or_else(|| panic!("no catalog entry for doc rule {rule_id:?}"))["message_prefix"]
            .as_str()
            .expect("message_prefix string");
        assert_eq!(
            derived, stored,
            "rule {rule_id:?}: the derived prefix must equal the catalog's stored prefix"
        );
    }
}

#[test]
fn doc_states_the_corpus_paths_it_publishes() {
    // The "how to test your emitter" pointers must survive doc edits: the corpus directory and
    // both driver test targets are load-bearing for external emitters.
    for needle in [
        "crates/oce-cxf/tests/fixtures/composite_contract/",
        "--test composite_contract_corpus",
        "-p oce-api --test conformance composite_contract",
    ] {
        assert!(
            DOC.contains(needle),
            "the doc must keep the emitter-facing pointer {needle:?}"
        );
    }
}
