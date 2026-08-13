//! Private facade-bound evaluation for the scoped OpenModelica Nand case.

#![allow(dead_code)]

use std::io::Read as _;
use std::path::Path;

use crate::block_harness::{B, BlockCase, Port, case, drive_case_with_external_reference};
use oce_conformance::{
    AuditDiscrepancy, BooleanDerivation, CombiTimeTable, ComparedParty, ComparisonResult,
    validate_boolean_derivation,
};

const INPUTS: &[Port] = &[
    Port {
        name: "u1",
        kind: B,
    },
    Port {
        name: "u2",
        kind: B,
    },
];
const OUTPUTS: &[Port] = &[Port { name: "y", kind: B }];
const CASE: BlockCase = case(
    "openmodelica_logical_nand",
    "CDL.Logical.Nand",
    "external",
    INPUTS,
    &[],
    OUTPUTS,
);
const SCENARIO: &str = "all_boolean_input_pairs_evented";
const MANIFEST: &str =
    "crates/oce-conformance/tests/fixtures/open_modelica/logical_nand/manifest.json";
pub(crate) const MAX_REFERENCE_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ScopedNandOutcome {
    pub(crate) producer: &'static str,
    pub(crate) class: &'static str,
    pub(crate) scenario: &'static str,
    pub(crate) manifest: &'static str,
    pub(crate) compared_points: usize,
    pub(crate) exact_match: bool,
    pub(crate) first_mismatch: Option<ScopedMismatch>,
    pub(crate) load_warning_count: usize,
    pub(crate) engine: Vec<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ScopedMismatch {
    pub(crate) index: usize,
    pub(crate) time_bits: u64,
    pub(crate) expected: bool,
    pub(crate) observed: bool,
}

pub(crate) fn evaluate(reference: &CombiTimeTable) -> ScopedNandOutcome {
    let run = drive_case_with_external_reference(&CASE, SCENARIO, reference);
    assert_eq!(run.comparisons.len(), 1, "Nand has one output");
    let comparison = &run.comparisons[0];
    assert_eq!(comparison.reference_column, "y");
    assert!(!comparison.masked);
    assert_eq!(comparison.tolerance.atolx, 0.0);
    assert_eq!(comparison.tolerance.atoly, 0.0);
    assert_eq!(comparison.tolerance.rtolx, 0.0);
    assert_eq!(comparison.tolerance.rtoly, 0.0);
    assert_eq!(comparison.tolerance.ltolx, 0.0);
    assert_eq!(comparison.tolerance.ltoly, 0.0);
    let ComparisonResult::Exact(exact) = &comparison.result else {
        panic!("OpenModelica Nand comparison did not use exact mode");
    };
    let output = run
        .trace
        .columns
        .iter()
        .find(|column| column.name.ends_with(".block.y"))
        .expect("captured Nand output");
    ScopedNandOutcome {
        producer: "OpenModelica 1.25.1",
        class: "CDL.Logical.Nand",
        scenario: SCENARIO,
        manifest: MANIFEST,
        compared_points: exact.compared_points,
        exact_match: exact.passed,
        first_mismatch: exact
            .first_mismatch
            .as_ref()
            .map(|mismatch| ScopedMismatch {
                index: mismatch.index,
                time_bits: mismatch.x.to_bits(),
                expected: mismatch.expected == 1.0,
                observed: mismatch.actual == 1.0,
            }),
        load_warning_count: run.load_warnings.len(),
        engine: output.values.iter().map(|value| *value == 1.0).collect(),
    }
}

pub(crate) fn read_reference(name: &str) -> Result<CombiTimeTable, String> {
    if !matches!(name, "nand.canonical.csv" | "and.canonical.csv") {
        return Err("unsupported scoped OpenModelica fixture".into());
    }
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/open_modelica/logical_nand")
        .join(name);
    let metadata = path.metadata().map_err(|error| error.to_string())?;
    if metadata.len() > MAX_REFERENCE_BYTES as u64 {
        return Err(format!(
            "canonical reference exceeds {MAX_REFERENCE_BYTES} bytes"
        ));
    }
    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take((MAX_REFERENCE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    parse_reference_bytes(&bytes)
}

pub(crate) fn parse_reference_bytes(bytes: &[u8]) -> Result<CombiTimeTable, String> {
    if bytes.len() > MAX_REFERENCE_BYTES {
        return Err(format!(
            "canonical reference exceeds {MAX_REFERENCE_BYTES} bytes"
        ));
    }
    let text = std::str::from_utf8(bytes).map_err(|error| error.to_string())?;
    CombiTimeTable::parse(text).map_err(|error| error.to_string())
}

pub(crate) fn analytical_discrepancies(
    reference: &CombiTimeTable,
    engine: &[bool],
) -> Result<Vec<oce_conformance::AuditDiscrepancy>, String> {
    let derivation: BooleanDerivation = serde_json::from_str(include_str!(
        "../fixtures/clean_room/logical_nand.derivation.json"
    ))
    .map_err(|error| error.to_string())?;
    validate_boolean_derivation(&derivation)?;
    let names = reference
        .col_names
        .as_ref()
        .ok_or("canonical columns missing")?;
    let index = |name: &str| {
        names
            .iter()
            .position(|candidate| candidate == name)
            .ok_or_else(|| format!("canonical column {name} missing"))
    };
    let (u1, u2, y) = (index("u1")?, index("u2")?, index("y")?);
    if engine.len() != reference.n_rows {
        return Err(format!(
            "engine row count mismatch: reference={}, engine={}",
            reference.n_rows,
            engine.len()
        ));
    }
    let mut discrepancies = Vec::new();
    for (sample, row) in reference.data.chunks_exact(reference.n_cols).enumerate() {
        let pair = (
            boolean_value(row[u1], sample, "u1")?,
            boolean_value(row[u2], sample, "u2")?,
        );
        let external = boolean_value(row[y], sample, "y")?;
        let expected = derivation
            .rows
            .iter()
            .find(|candidate| (candidate.u1, candidate.u2) == pair)
            .map(|candidate| candidate.y)
            .ok_or_else(|| format!("analytical derivation omits pair {pair:?}"))?;
        for (party, observed) in [
            (ComparedParty::TierA, external),
            (ComparedParty::Engine, engine[sample]),
        ] {
            if observed != expected {
                discrepancies.push(AuditDiscrepancy {
                    id: format!(
                        "openmodelica:{}:{}:{sample}",
                        derivation.class, derivation.scenario
                    ),
                    class: derivation.class.clone(),
                    scenario: derivation.scenario.clone(),
                    sample,
                    inputs: pair,
                    derived: expected,
                    party,
                    observed,
                });
            }
        }
    }
    Ok(discrepancies)
}

fn boolean_value(value: f64, sample: usize, column: &str) -> Result<bool, String> {
    match value {
        0.0 => Ok(false),
        1.0 => Ok(true),
        _ => Err(format!(
            "canonical sample {sample} column {column} is not Boolean"
        )),
    }
}
