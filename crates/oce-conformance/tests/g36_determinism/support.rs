use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use oce_api::Value;
use oce_conformance::{
    CombiTimeTable, ComparisonMode, ComparisonResult, DriveCadence, DriverInputReplay,
    DriverOptions, PointEnd, PointMapEntry, ReferenceSpec, Tolerances, ValueKind, VerifyConfig,
    drive_trace_with_options,
};
use serde::{Deserialize, Serialize};

const GOLDEN_DIR: &str = "tests/fixtures/golden/g36_traces";
const PROVENANCE_SOURCE: &str =
    "engine self-output (determinism snapshot); NOT a correctness oracle";

#[derive(Clone, Copy)]
pub(crate) struct PointSpec {
    pub(crate) reference_name: &'static str,
    pub(crate) cdl_name: &'static str,
    pub(crate) kind: ValueKind,
}

impl PointSpec {
    pub(crate) const fn real(name: &'static str) -> Self {
        Self {
            reference_name: name,
            cdl_name: name,
            kind: ValueKind::Real,
        }
    }

    pub(crate) const fn integer(name: &'static str) -> Self {
        Self {
            reference_name: name,
            cdl_name: name,
            kind: ValueKind::Integer,
        }
    }

    pub(crate) const fn boolean(name: &'static str) -> Self {
        Self {
            reference_name: name,
            cdl_name: name,
            kind: ValueKind::Boolean,
        }
    }

    pub(crate) const fn real_alias(reference_name: &'static str, cdl_name: &'static str) -> Self {
        Self {
            reference_name,
            cdl_name,
            kind: ValueKind::Real,
        }
    }

    pub(crate) const fn boolean_alias(
        reference_name: &'static str,
        cdl_name: &'static str,
    ) -> Self {
        Self {
            reference_name,
            cdl_name,
            kind: ValueKind::Boolean,
        }
    }
}

pub(crate) struct SequenceSpec {
    pub(crate) name: &'static str,
    pub(crate) cxf: &'static str,
    pub(crate) t_stop: u32,
    pub(crate) inputs: &'static [PointSpec],
    pub(crate) outputs: &'static [PointSpec],
    pub(crate) input_fn: fn(f64) -> Vec<(String, Value)>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Provenance {
    tier: String,
    source: String,
    depends_on_oce_blocks: bool,
    engine_rev: String,
    reference_columns: Vec<String>,
}

pub(crate) fn pair(name: &str, value: Value) -> (String, Value) {
    (name.to_string(), value)
}

pub(crate) fn bless_enabled() -> bool {
    std::env::var_os("OCE_BLESS_G36").is_some()
}

pub(crate) fn bless_sequence(spec: &SequenceSpec) {
    fs::create_dir_all(golden_dir()).expect("create G36 golden directory");
    let seed = seed_reference(spec);
    let run = drive_trace_with_options(
        spec.cxf.as_bytes(),
        &config_for(spec),
        &seed,
        &options_for(spec),
    )
    .unwrap_or_else(|err| panic!("{} bless driver run failed: {err}", spec.name));
    let table = captured_output_table(spec, &run);
    assert_output_table_shape(spec, &table);
    table
        .write(&golden_path(spec))
        .unwrap_or_else(|err| panic!("{} golden write failed: {err}", spec.name));
    write_provenance(spec, &table);
}

pub(crate) fn read_output_golden(spec: &SequenceSpec) -> CombiTimeTable {
    CombiTimeTable::read(&golden_path(spec))
        .unwrap_or_else(|err| panic!("{} golden read failed: {err}", spec.name))
}

pub(crate) fn driver_reference_from_output_golden(
    spec: &SequenceSpec,
    golden: &CombiTimeTable,
) -> CombiTimeTable {
    assert_output_table_shape(spec, golden);
    let mut names = Vec::with_capacity(1 + spec.inputs.len() + spec.outputs.len());
    names.push("time".to_string());
    names.extend(
        spec.inputs
            .iter()
            .map(|input| input.reference_name.to_string()),
    );
    names.extend(
        spec.outputs
            .iter()
            .map(|output| output.reference_name.to_string()),
    );

    let n_cols = names.len();
    let mut data = Vec::with_capacity(golden.n_rows * n_cols);
    for row in 0..golden.n_rows {
        let t = golden.data[row * golden.n_cols];
        data.push(t);
        push_input_cells(spec, t, &mut data);
        for output_col in 1..golden.n_cols {
            data.push(golden.data[row * golden.n_cols + output_col]);
        }
    }

    CombiTimeTable {
        name: spec.name.to_string(),
        n_rows: golden.n_rows,
        n_cols,
        data,
        col_names: Some(names),
    }
}

fn seed_reference(spec: &SequenceSpec) -> CombiTimeTable {
    let names = reference_column_names(spec);
    let n_rows = spec.t_stop as usize + 1;
    let n_cols = names.len();
    let mut data = Vec::with_capacity(n_rows * n_cols);
    for tick in 0..=spec.t_stop {
        let t = f64::from(tick);
        data.push(t);
        push_input_cells(spec, t, &mut data);
        data.extend(spec.outputs.iter().map(|_| 0.0));
    }
    CombiTimeTable {
        name: spec.name.to_string(),
        n_rows,
        n_cols,
        data,
        col_names: Some(names),
    }
}

fn reference_column_names(spec: &SequenceSpec) -> Vec<String> {
    let mut names = Vec::with_capacity(1 + spec.inputs.len() + spec.outputs.len());
    names.push("time".to_string());
    names.extend(
        spec.inputs
            .iter()
            .map(|input| input.reference_name.to_string()),
    );
    names.extend(
        spec.outputs
            .iter()
            .map(|output| output.reference_name.to_string()),
    );
    names
}

fn push_input_cells(spec: &SequenceSpec, t: f64, data: &mut Vec<f64>) {
    let inputs = (spec.input_fn)(t);
    for point in spec.inputs {
        let value = inputs
            .iter()
            .find_map(|(name, value)| (name == point.reference_name).then_some(value))
            .unwrap_or_else(|| {
                panic!(
                    "{} input {} missing at t={t}",
                    spec.name, point.reference_name
                )
            });
        data.push(encode(point.kind, value));
    }
}

pub(crate) fn config_for(spec: &SequenceSpec) -> VerifyConfig {
    VerifyConfig {
        references: vec![ReferenceSpec {
            model: "g36".to_string(),
            sequence: spec.name.to_string(),
            point_name_mapping: point_mapping(spec),
        }],
        tolerances: Tolerances {
            atolx: 0.0,
            atoly: 0.0,
            rtolx: 0.0,
            rtoly: 0.0,
            ltolx: 0.0,
            ltoly: 0.0,
        },
        outputs: Vec::new(),
        indicators: Vec::new(),
        sampling: Some(1.0),
        run_controller: true,
    }
}

fn point_mapping(spec: &SequenceSpec) -> Vec<PointMapEntry> {
    spec.inputs
        .iter()
        .chain(spec.outputs.iter())
        .map(|point| PointMapEntry {
            cdl: point_end(point.cdl_name, point.kind),
            device: point_end(point.reference_name, point.kind),
        })
        .collect()
}

fn point_end(name: &str, kind: ValueKind) -> PointEnd {
    PointEnd {
        name: name.to_string(),
        unit: None,
        kind: Some(kind_name(kind).to_string()),
    }
}

pub(crate) fn options_for(spec: &SequenceSpec) -> DriverOptions {
    DriverOptions {
        cadence: DriveCadence::EventAligned {
            instants: (0..=spec.t_stop).map(f64::from).collect(),
        },
        input_replay: DriverInputReplay::ReferenceTable,
        comparison: ComparisonMode::Exact,
    }
}

pub(crate) fn assert_output_table_shape(spec: &SequenceSpec, table: &CombiTimeTable) {
    assert_eq!(table.name, spec.name, "{} golden table name", spec.name);
    assert_eq!(
        table.n_rows,
        spec.t_stop as usize + 1,
        "{} golden row count",
        spec.name
    );
    assert_eq!(
        table.n_cols,
        spec.outputs.len() + 1,
        "{} golden column count",
        spec.name
    );
    assert_eq!(
        table.col_names.as_deref(),
        Some(output_column_names(spec).as_slice()),
        "{} golden output columns",
        spec.name
    );
}

fn output_column_names(spec: &SequenceSpec) -> Vec<String> {
    let mut names = Vec::with_capacity(1 + spec.outputs.len());
    names.push("time".to_string());
    names.extend(
        spec.outputs
            .iter()
            .map(|output| output.reference_name.to_string()),
    );
    names
}

pub(crate) fn captured_output_table(
    spec: &SequenceSpec,
    run: &oce_conformance::DriverRun,
) -> CombiTimeTable {
    let mut table = run.trace.to_table(spec.name);
    let mut runtime_names = Vec::with_capacity(1 + spec.outputs.len());
    runtime_names.push("time".to_string());
    runtime_names.extend(
        spec.outputs
            .iter()
            .map(|output| output.cdl_name.to_string()),
    );
    assert_eq!(
        table.col_names.as_deref(),
        Some(runtime_names.as_slice()),
        "{} runtime capture columns",
        spec.name
    );
    table.col_names = Some(output_column_names(spec));
    table
}

pub(crate) fn assert_exact_comparisons_pass(
    spec: &SequenceSpec,
    expected_rows: usize,
    comparisons: &[oce_conformance::SignalComparison],
) {
    assert_eq!(
        comparisons.len(),
        spec.outputs.len(),
        "{} comparison count",
        spec.name
    );
    for output in spec.outputs {
        let comparison = comparisons
            .iter()
            .find(|comparison| comparison.reference_column == output.reference_name)
            .unwrap_or_else(|| {
                panic!(
                    "{} missing comparison for {}",
                    spec.name, output.reference_name
                )
            });
        assert_eq!(
            comparison.output, output.cdl_name,
            "{} runtime output for {}",
            spec.name, output.reference_name
        );
        assert_eq!(
            comparison.reference_column, output.reference_name,
            "{} reference column for {}",
            spec.name, output.reference_name
        );
        match &comparison.result {
            ComparisonResult::Exact(result) => {
                assert!(
                    result.passed,
                    "{} exact comparison failed for {}: {:?}",
                    spec.name, output.reference_name, result.first_mismatch
                );
                assert_eq!(
                    result.compared_points, expected_rows,
                    "{} compared points for {}",
                    spec.name, output.reference_name
                );
                assert!(
                    result.first_mismatch.is_none(),
                    "{} first mismatch for {}: {:?}",
                    spec.name,
                    output.reference_name,
                    result.first_mismatch
                );
            }
            other => panic!(
                "{} comparison for {} used non-exact mode: {other:?}",
                spec.name, output.reference_name
            ),
        }
    }
}

pub(crate) fn assert_provenance_matches_outputs(spec: &SequenceSpec, table: &CombiTimeTable) {
    let text = fs::read_to_string(provenance_path(spec))
        .unwrap_or_else(|err| panic!("{} provenance read failed: {err}", spec.name));
    let provenance: Provenance = serde_json::from_str(&text)
        .unwrap_or_else(|err| panic!("{} provenance JSON invalid: {err}", spec.name));
    assert_eq!(provenance.tier, "2", "{} provenance tier", spec.name);
    assert_eq!(
        provenance.source, PROVENANCE_SOURCE,
        "{} provenance source",
        spec.name
    );
    assert!(
        provenance.depends_on_oce_blocks,
        "{} provenance must mark oce-blocks dependency",
        spec.name
    );
    assert!(
        !provenance.engine_rev.trim().is_empty(),
        "{} provenance engine_rev must be set",
        spec.name
    );
    assert_eq!(
        provenance.reference_columns,
        table.col_names.as_ref().expect("checked table columns")[1..],
        "{} provenance reference columns",
        spec.name
    );
}

fn write_provenance(spec: &SequenceSpec, table: &CombiTimeTable) {
    let provenance = Provenance {
        tier: "2".to_string(),
        source: PROVENANCE_SOURCE.to_string(),
        depends_on_oce_blocks: true,
        engine_rev: engine_rev(),
        reference_columns: table.col_names.as_ref().expect("checked table columns")[1..].to_vec(),
    };
    let text = serde_json::to_string_pretty(&provenance).expect("serialize provenance");
    fs::write(provenance_path(spec), format!("{text}\n"))
        .unwrap_or_else(|err| panic!("{} provenance write failed: {err}", spec.name));
}

fn engine_rev() -> String {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("run git rev-parse HEAD");
    assert!(
        output.status.success(),
        "git rev-parse HEAD failed with status {}",
        output.status
    );
    String::from_utf8(output.stdout)
        .expect("git rev-parse HEAD produced non-UTF-8")
        .trim()
        .to_string()
}

fn encode(kind: ValueKind, value: &Value) -> f64 {
    match (kind, value) {
        (ValueKind::Real, Value::Real(x)) => *x,
        (ValueKind::Integer, Value::Integer(i)) => *i as f64,
        (ValueKind::Boolean, Value::Boolean(b)) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        (expected, found) => panic!(
            "input kind mismatch: expected {}, found {:?}",
            kind_name(expected),
            found
        ),
    }
}

fn kind_name(kind: ValueKind) -> &'static str {
    match kind {
        ValueKind::Real => "Real",
        ValueKind::Integer => "Integer",
        ValueKind::Boolean => "Boolean",
    }
}

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(GOLDEN_DIR)
}

fn golden_path(spec: &SequenceSpec) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(GOLDEN_DIR)
        .join(format!("{}.csv", spec.name))
}

fn provenance_path(spec: &SequenceSpec) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(GOLDEN_DIR)
        .join(format!("{}.prov.json", spec.name))
}
