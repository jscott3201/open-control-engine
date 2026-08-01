//! Per-PR sentinel for the bounded Nand clean-room derivation audit.

#[path = "../../../oce-conformance/src/clean_room.rs"]
mod detector;

use detector::{
    BooleanDerivation, BooleanReferenceRow, compare_boolean_derivation, validate_derivation_source,
};
use oce_blocks::{Block, Ctx, Nand, NoopDiagnostics};
use oce_model::Value;

const DERIVATION: &str =
    include_str!("../../../oce-conformance/tests/fixtures/clean_room/logical_nand.derivation.json");
const NAND_SOURCE: &str = include_str!(
    "../../../../third_party/modelica-buildings-cdl/Buildings/Controls/OBC/CDL/Logical/Nand.mo"
);
const TIER_A: &str =
    include_str!("../../../../tools/golden-gen/goldens/CDL/Logical/Nand/reference.csv");

fn boolean_cell(cell: &str, column: &str) -> bool {
    match cell {
        "0.0" => false,
        "1.0" => true,
        other => panic!("Nand reference {column} must encode Boolean exactly, got {other}"),
    }
}

fn tier_a_rows() -> Vec<BooleanReferenceRow> {
    TIER_A
        .lines()
        .filter(|line| !line.starts_with('#') && !line.starts_with("double"))
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let cells = line.split_whitespace().collect::<Vec<_>>();
            assert_eq!(cells.len(), 4, "Nand reference row width");
            BooleanReferenceRow {
                time: cells[0].parse().expect("Nand reference time"),
                u1: boolean_cell(cells[1], "u1"),
                u2: boolean_cell(cells[2], "u2"),
                y: boolean_cell(cells[3], "y"),
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
        serde_json::from_str(DERIVATION).expect("strict frozen Nand derivation should parse");
    validate_derivation_source(&record, NAND_SOURCE)
        .expect("source citation should resolve to the vendored Nand equation");
    let discrepancies =
        compare_boolean_derivation(&record, &tier_a_rows(), &engine_values(&record))
            .expect("provenance and three-way comparison should be structurally valid");
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
