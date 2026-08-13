//! Compatibility with the current clean-room discrepancy producer identity.

use oce_conformance::{BooleanDerivation, BooleanReferenceRow, compare_boolean_derivation};

use super::reader::read_register;
use super::test_data::{bytes, entry, register};

fn derivation() -> BooleanDerivation {
    serde_json::from_str(include_str!(
        "../fixtures/clean_room/logical_nand.derivation.json"
    ))
    .expect("frozen derivation parses")
}

fn reference(record: &BooleanDerivation) -> Vec<BooleanReferenceRow> {
    record
        .rows
        .iter()
        .map(|row| BooleanReferenceRow {
            time: row.sample as f64 * 60.0,
            u1: row.u1,
            u2: row.u2,
            y: row.y,
        })
        .collect()
}

#[test]
fn producer_case_membership_never_consumes_or_changes_a_discrepancy() {
    let record = derivation();
    let tier_a = reference(&record);
    let mut engine = record.rows.iter().map(|row| row.y).collect::<Vec<_>>();
    engine[0] = !engine[0];
    let discrepancy = compare_boolean_derivation(&record, &tier_a, &engine)
        .expect("mutated comparison remains structurally valid");
    assert_eq!(discrepancy.len(), 1, "mutation must produce evidence");
    assert_eq!(
        discrepancy[0].id,
        "clean-room:CDL.Logical.Nand:all_boolean_input_pairs:0"
    );

    let mut schema_entry = entry("DVG-000001", "producer");
    schema_entry["producer_cases"][0]["case_id"] = serde_json::json!(discrepancy[0].id);
    let parsed = read_register(&bytes(&register(vec![schema_entry])))
        .expect("current producer case id is valid schema data");
    assert_eq!(parsed.entries.len(), 1);
    assert_eq!(
        discrepancy.len(),
        1,
        "register parsing cannot consume evidence"
    );
    assert!(
        discrepancy[0]
            .to_string()
            .contains("Tier-A adjudication required")
    );

    engine[0] = !engine[0];
    assert!(
        compare_boolean_derivation(&record, &tier_a, &engine)
            .expect("restored comparison is valid")
            .is_empty(),
        "producer mutation is restored in the same test"
    );
}
