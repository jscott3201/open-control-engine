//! Per-PR sentinel for the bounded Nand clean-room derivation audit.

#[path = "../../oce-conformance/src/clean_room.rs"]
mod detector;

use detector::{BooleanDerivation, compare_boolean_derivation};
use oce_blocks::{Block, Ctx, Nand, NoopDiagnostics};
use oce_model::Value;

const DERIVATION: &str =
    include_str!("../../oce-conformance/tests/fixtures/clean_room/logical_nand.derivation.json");
const TIER_A: &str =
    include_str!("../../../tools/golden-gen/goldens/CDL/Logical/Nand/reference.csv");

fn tier_a_values() -> Vec<bool> {
    TIER_A
        .lines()
        .filter(|line| !line.starts_with('#') && !line.starts_with("double"))
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let value = line
                .split_whitespace()
                .nth(3)
                .expect("Nand reference output column");
            match value {
                "0.0" => false,
                "1.0" => true,
                other => panic!("Nand reference must encode Boolean exactly, got {other}"),
            }
        })
        .collect()
}

fn engine_values(record: &BooleanDerivation) -> Vec<bool> {
    let diagnostics = NoopDiagnostics;
    let context = Ctx::new(0.0, &diagnostics);
    record
        .rows
        .iter()
        .map(|row| {
            let mut output = None;
            Nand.step_algebraic(
                &context,
                &[Value::Boolean(row.u1), Value::Boolean(row.u2)],
                &mut |index, value| {
                    assert_eq!(index, 0, "Nand emits one output");
                    output = Some(value);
                },
            );
            match output.expect("Nand output") {
                Value::Boolean(value) => value,
                other => panic!("Nand output must be Boolean, got {other:?}"),
            }
        })
        .collect()
}

#[test]
fn frozen_derivation_agrees_with_tier_a_and_engine() {
    let record: BooleanDerivation =
        serde_json::from_str(DERIVATION).expect("frozen Nand derivation should parse");
    let discrepancies =
        compare_boolean_derivation(&record, &tier_a_values(), &engine_values(&record))
            .expect("three-way comparison should be structurally valid");
    assert!(
        discrepancies.is_empty(),
        "{}",
        discrepancies
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    );
}
