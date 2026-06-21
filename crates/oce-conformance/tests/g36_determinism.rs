//! Whole-sequence G36 Tier-2 determinism goldens through the B3 facade driver.
//!
//! These fixtures are engine self-output snapshots, not independent correctness oracles.

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

const AHU_SAT_RESET: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/ahu_supply_air_temp_reset.jsonld");
const AHU_ECONOMIZER: &str = include_str!("../../oce-cxf/tests/fixtures/g36/ahu_economizer.jsonld");
const VAV_SINGLE_ZONE: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/vav_single_zone.jsonld");

const GOLDEN_DIR: &str = "tests/fixtures/golden/g36_traces";
const PROVENANCE_SOURCE: &str =
    "engine self-output (determinism snapshot); NOT a correctness oracle";

// The facade exposes flattened runtime connector IDs, while the goldens and provenance preserve
// the fixture-declared output names.
const SAT_ZONE_TEMP: &str = "http://example.org#g36.ahu_supply_air_temp_reset.zone_temp";
const SAT_COOLING_SETPOINT: &str =
    "http://example.org#g36.ahu_supply_air_temp_reset.cooling_setpoint";
const SAT_SETPOINT: &str = "http://example.org#g36.ahu_supply_air_temp_reset.sat_setpoint";
const SAT_COOLING_DEMAND: &str = "http://example.org#g36.ahu_supply_air_temp_reset.cooling_demand";
const SAT_SETPOINT_RUNTIME: &str = "conn#14";
const SAT_COOLING_DEMAND_RUNTIME: &str = "conn#4";

const ECON_RETURN_AIR_TEMP: &str = "http://example.org#g36.ahu_economizer.return_air_temp";
const ECON_OUTDOOR_AIR_TEMP: &str = "http://example.org#g36.ahu_economizer.outdoor_air_temp";
const ECON_OPERATING_MODE: &str = "http://example.org#g36.ahu_economizer.operating_mode";
const ECON_ENABLED: &str = "http://example.org#g36.ahu_economizer.economizer_enabled";
const ECON_DAMPER_COMMAND: &str = "http://example.org#g36.ahu_economizer.damper_command";
const ECON_OPERATING_MODE_REAL: &str = "http://example.org#g36.ahu_economizer.operating_mode_real";
const ECON_OA_TEMP_DELTA: &str = "http://example.org#g36.ahu_economizer.oa_temperature_delta";
const ECON_ENABLED_RUNTIME: &str = "conn#20";
const ECON_DAMPER_COMMAND_RUNTIME: &str = "conn#26";
const ECON_OPERATING_MODE_REAL_RUNTIME: &str = "conn#10";
const ECON_OA_TEMP_DELTA_RUNTIME: &str = "conn#2";

const VAV_ZONE_TEMP: &str = "http://example.org#g36.vav_single_zone.zone_temp";
const VAV_COOLING_SETPOINT: &str = "http://example.org#g36.vav_single_zone.cooling_setpoint";
const VAV_HEATING_SETPOINT: &str = "http://example.org#g36.vav_single_zone.heating_setpoint";
const VAV_DAMPER_COMMAND: &str = "http://example.org#g36.vav_single_zone.damper_command";
const VAV_AIRFLOW_SETPOINT: &str = "http://example.org#g36.vav_single_zone.airflow_setpoint";
const VAV_COOLING_SIGNAL: &str = "http://example.org#g36.vav_single_zone.cooling_signal";
const VAV_HEATING_ENABLED: &str = "http://example.org#g36.vav_single_zone.heating_enabled";
const VAV_DAMPER_COMMAND_RUNTIME: &str = "conn#18";
const VAV_AIRFLOW_SETPOINT_RUNTIME: &str = "conn#16";
const VAV_COOLING_SIGNAL_RUNTIME: &str = "conn#4";
const VAV_HEATING_ENABLED_RUNTIME: &str = "conn#11";

const SAT_INPUTS: &[PointSpec] = &[
    PointSpec::real(SAT_ZONE_TEMP),
    PointSpec::real(SAT_COOLING_SETPOINT),
];
const SAT_OUTPUTS: &[PointSpec] = &[
    PointSpec::real_alias(SAT_SETPOINT, SAT_SETPOINT_RUNTIME),
    PointSpec::real_alias(SAT_COOLING_DEMAND, SAT_COOLING_DEMAND_RUNTIME),
];

const ECON_INPUTS: &[PointSpec] = &[
    PointSpec::real(ECON_RETURN_AIR_TEMP),
    PointSpec::real(ECON_OUTDOOR_AIR_TEMP),
    PointSpec::integer(ECON_OPERATING_MODE),
];
const ECON_OUTPUTS: &[PointSpec] = &[
    PointSpec::boolean_alias(ECON_ENABLED, ECON_ENABLED_RUNTIME),
    PointSpec::real_alias(ECON_DAMPER_COMMAND, ECON_DAMPER_COMMAND_RUNTIME),
    PointSpec::real_alias(ECON_OPERATING_MODE_REAL, ECON_OPERATING_MODE_REAL_RUNTIME),
    PointSpec::real_alias(ECON_OA_TEMP_DELTA, ECON_OA_TEMP_DELTA_RUNTIME),
];

const VAV_INPUTS: &[PointSpec] = &[
    PointSpec::real(VAV_ZONE_TEMP),
    PointSpec::real(VAV_COOLING_SETPOINT),
    PointSpec::real(VAV_HEATING_SETPOINT),
];
const VAV_OUTPUTS: &[PointSpec] = &[
    PointSpec::real_alias(VAV_DAMPER_COMMAND, VAV_DAMPER_COMMAND_RUNTIME),
    PointSpec::real_alias(VAV_AIRFLOW_SETPOINT, VAV_AIRFLOW_SETPOINT_RUNTIME),
    PointSpec::real_alias(VAV_COOLING_SIGNAL, VAV_COOLING_SIGNAL_RUNTIME),
    PointSpec::boolean_alias(VAV_HEATING_ENABLED, VAV_HEATING_ENABLED_RUNTIME),
];

const SEQUENCES: &[SequenceSpec] = &[
    SequenceSpec {
        name: "ahu_supply_air_temp_reset",
        cxf: AHU_SAT_RESET,
        t_stop: 4,
        inputs: SAT_INPUTS,
        outputs: SAT_OUTPUTS,
        input_fn: sat_inputs,
    },
    SequenceSpec {
        name: "ahu_economizer",
        cxf: AHU_ECONOMIZER,
        t_stop: 5,
        inputs: ECON_INPUTS,
        outputs: ECON_OUTPUTS,
        input_fn: economizer_inputs,
    },
    SequenceSpec {
        name: "vav_single_zone",
        cxf: VAV_SINGLE_ZONE,
        t_stop: 5,
        inputs: VAV_INPUTS,
        outputs: VAV_OUTPUTS,
        input_fn: vav_inputs,
    },
];

#[test]
fn g36_whole_sequence_outputs_match_determinism_goldens() {
    for spec in SEQUENCES {
        if bless_enabled() {
            bless_sequence(spec);
        }

        let golden = read_output_golden(spec);
        assert_provenance_matches_outputs(spec, &golden);
        let reference = driver_reference_from_output_golden(spec, &golden);
        let run = drive_trace_with_options(
            spec.cxf.as_bytes(),
            &config_for(spec),
            &reference,
            &options_for(spec),
        )
        .unwrap_or_else(|err| panic!("{} driver run failed: {err}", spec.name));

        assert_output_table_shape(spec, &golden);
        assert_eq!(
            captured_output_table(spec, &run),
            golden,
            "{} captured table drifted from committed golden",
            spec.name
        );
        assert_exact_comparisons_pass(spec, golden.n_rows, &run.comparisons);
    }
}

#[derive(Clone, Copy)]
struct PointSpec {
    reference_name: &'static str,
    cdl_name: &'static str,
    kind: ValueKind,
}

impl PointSpec {
    const fn real(name: &'static str) -> Self {
        Self {
            reference_name: name,
            cdl_name: name,
            kind: ValueKind::Real,
        }
    }

    const fn integer(name: &'static str) -> Self {
        Self {
            reference_name: name,
            cdl_name: name,
            kind: ValueKind::Integer,
        }
    }

    const fn real_alias(reference_name: &'static str, cdl_name: &'static str) -> Self {
        Self {
            reference_name,
            cdl_name,
            kind: ValueKind::Real,
        }
    }

    const fn boolean_alias(reference_name: &'static str, cdl_name: &'static str) -> Self {
        Self {
            reference_name,
            cdl_name,
            kind: ValueKind::Boolean,
        }
    }
}

struct SequenceSpec {
    name: &'static str,
    cxf: &'static str,
    t_stop: u32,
    inputs: &'static [PointSpec],
    outputs: &'static [PointSpec],
    input_fn: fn(f64) -> Vec<(String, Value)>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Provenance {
    tier: String,
    source: String,
    depends_on_oce_blocks: bool,
    engine_rev: String,
    reference_columns: Vec<String>,
}

fn sat_inputs(t: f64) -> Vec<(String, Value)> {
    let zone_temp = match t as u32 {
        0 => 22.0,
        1 => 24.0,
        2 => 24.5,
        _ => 25.5,
    };
    vec![
        pair(SAT_ZONE_TEMP, Value::Real(zone_temp)),
        pair(SAT_COOLING_SETPOINT, Value::Real(24.0)),
    ]
}

fn economizer_inputs(t: f64) -> Vec<(String, Value)> {
    let (return_temp, outdoor_temp, operating_mode) = match t as u32 {
        0 => (24.0, 23.0, 1),
        1..=3 => (24.0, 19.0, 1),
        4 => (24.0, 24.0, 1),
        _ => (24.0, 19.0, 0),
    };
    vec![
        pair(ECON_RETURN_AIR_TEMP, Value::Real(return_temp)),
        pair(ECON_OUTDOOR_AIR_TEMP, Value::Real(outdoor_temp)),
        pair(ECON_OPERATING_MODE, Value::Integer(operating_mode)),
    ]
}

fn vav_inputs(t: f64) -> Vec<(String, Value)> {
    let zone_temp = match t as u32 {
        0 => 22.0,
        1 => 27.0,
        2 => 27.5,
        3 => 19.0,
        4 => 19.3,
        _ => 21.0,
    };
    vec![
        pair(VAV_ZONE_TEMP, Value::Real(zone_temp)),
        pair(VAV_COOLING_SETPOINT, Value::Real(24.0)),
        pair(VAV_HEATING_SETPOINT, Value::Real(20.0)),
    ]
}

fn pair(name: &str, value: Value) -> (String, Value) {
    (name.to_string(), value)
}

fn bless_enabled() -> bool {
    std::env::var_os("OCE_BLESS_G36").is_some()
}

fn bless_sequence(spec: &SequenceSpec) {
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

fn read_output_golden(spec: &SequenceSpec) -> CombiTimeTable {
    CombiTimeTable::read(&golden_path(spec))
        .unwrap_or_else(|err| panic!("{} golden read failed: {err}", spec.name))
}

fn driver_reference_from_output_golden(
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

fn config_for(spec: &SequenceSpec) -> VerifyConfig {
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

fn options_for(spec: &SequenceSpec) -> DriverOptions {
    DriverOptions {
        cadence: DriveCadence::EventAligned {
            instants: (0..=spec.t_stop).map(f64::from).collect(),
        },
        input_replay: DriverInputReplay::ReferenceTable,
        comparison: ComparisonMode::Exact,
    }
}

fn assert_output_table_shape(spec: &SequenceSpec, table: &CombiTimeTable) {
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

fn captured_output_table(spec: &SequenceSpec, run: &oce_conformance::DriverRun) -> CombiTimeTable {
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

fn assert_exact_comparisons_pass(
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

fn assert_provenance_matches_outputs(spec: &SequenceSpec, table: &CombiTimeTable) {
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
