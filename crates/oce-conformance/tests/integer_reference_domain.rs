//! Portable CDL Integer evidence is separate from the implementation's i64 stress tests.
//!
//! OBC CDL §7.4.1.2 / Modelica 3.6 §4.8.2 recommend at least the signed-32-bit range.
//! These Tier-A fixtures stay within that portable range; this is not a new engine domain rule.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use oce_conformance::CombiTimeTable;
use serde_json::Value;

fn provenance_paths(dir: &Path, paths: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            provenance_paths(&path, paths);
        } else if path.to_string_lossy().ends_with(".prov.json") {
            paths.push(path);
        }
    }
}

fn portable_integer(value: f64) -> bool {
    value.is_finite()
        && value.fract() == 0.0
        && value >= f64::from(i32::MIN)
        && value <= f64::from(i32::MAX)
}

fn audit_column(kind: &str, values: &[f64], label: &str, errors: &mut BTreeSet<String>) {
    match kind {
        "Integer" => {
            for (row, &value) in values.iter().enumerate() {
                if !portable_integer(value) {
                    errors.insert(format!("{label} row {row}: {value} is not a portable i32"));
                }
            }
        }
        "Real" | "Boolean" => {}
        other => panic!("unknown signal kind {other}"),
    }
}

fn column(table: &CombiTimeTable, name: &str) -> Vec<f64> {
    let index = table
        .col_names
        .as_ref()
        .unwrap()
        .iter()
        .position(|column| column == name)
        .unwrap_or_else(|| panic!("missing column {name}"));
    (0..table.n_rows)
        .map(|row| table.data[row * table.n_cols + index])
        .collect()
}

#[test]
fn every_cdl_integer_reference_cell_is_portable_and_matches_provenance() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/golden-gen/goldens/CDL");
    let mut paths = Vec::new();
    provenance_paths(&root, &mut paths);
    paths.sort();
    assert_eq!(paths.len(), 280, "CDL provenance census must not go silent");
    let mut errors = BTreeSet::new();
    let mut signals = 0;
    let mut integer_inputs = BTreeSet::new();
    let mut integer_outputs = BTreeSet::new();
    for path in paths {
        let record: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(record["tier"], "A");
        assert_eq!(record["depends_on_oce_blocks"], false);
        let class = record["class_path"].as_str().unwrap();
        if record["steppable"] == false {
            assert!(matches!(class, "CDL.Constants" | "CDL.Types"));
            continue;
        }
        signals += 1;
        let scenario = record["scenario"].as_str().unwrap_or("");
        assert_eq!(record["reference_csv"], "reference.csv");
        let reference =
            CombiTimeTable::read(&path.parent().unwrap().join("reference.csv")).unwrap();
        assert_eq!(
            reference.n_rows as u64,
            record["n_samples"].as_u64().unwrap()
        );
        let names: Vec<_> = record["reference_columns"]
            .as_array()
            .unwrap()
            .iter()
            .map(|name| name.as_str().unwrap().to_owned())
            .collect();
        assert_eq!(reference.col_names.as_ref().unwrap(), &names);
        for input in record["inputs"].as_array().unwrap() {
            let kind = input["value_kind"].as_str().unwrap();
            let name = input["name"].as_str().unwrap();
            let values = column(&reference, name);
            audit_column(
                kind,
                &values,
                &format!("{class}/{scenario} input {name}"),
                &mut errors,
            );
            if kind == "Integer" {
                integer_inputs.insert((class.to_owned(), scenario.to_owned(), name.to_owned()));
                let declared: Vec<_> = input["values"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|value| value.as_f64().expect("Integer provenance must be numeric"))
                    .collect();
                audit_column(
                    kind,
                    &declared,
                    &format!("{class}/{scenario} provenance {name}"),
                    &mut errors,
                );
                assert_eq!(
                    values.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                    declared.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                    "{} {name}",
                    path.display()
                );
            }
        }
        let kind = record["value_kind"].as_str().unwrap();
        let signal = record["signal"].as_str().unwrap();
        let values = column(&reference, signal);
        audit_column(
            kind,
            &values,
            &format!("{class}/{scenario} output {signal}"),
            &mut errors,
        );
        if kind == "Integer" {
            integer_outputs.insert((class.to_owned(), scenario.to_owned(), signal.to_owned()));
            let output =
                CombiTimeTable::read(&path.with_file_name(format!("{signal}.csv"))).unwrap();
            assert_eq!(output.n_rows, reference.n_rows);
            assert_eq!(output.n_cols, 2);
            for (row, value) in values.iter().enumerate() {
                assert_eq!(
                    output.data[row * 2].to_bits(),
                    reference.data[row * reference.n_cols].to_bits()
                );
                assert_eq!(output.data[row * 2 + 1].to_bits(), value.to_bits());
            }
        }
    }
    assert_eq!(signals, 278);
    assert_eq!(
        integer_inputs.len(),
        51,
        "Integer input type census changed"
    );
    assert_eq!(
        integer_outputs.len(),
        59,
        "Integer output type census changed"
    );
    assert!(
        errors.is_empty(),
        "{} Integer input columns, {} Integer output columns; violations:\n{}",
        integer_inputs.len(),
        integer_outputs.len(),
        errors.into_iter().collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn domain_guard_detects_each_integer_boundary_mutation_without_restricting_real_outputs() {
    for value in [f64::from(i32::MIN), -1.0, 0.0, f64::from(i32::MAX)] {
        assert!(portable_integer(value));
    }
    for value in [
        f64::from(i32::MIN) - 1.0,
        f64::from(i32::MAX) + 1.0,
        9_007_199_254_740_992.0,
        -9_007_199_254_740_992.0,
        0.5,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ] {
        let mut errors = BTreeSet::new();
        audit_column("Integer", &[value], "mutated input", &mut errors);
        assert_eq!(errors.len(), 1, "mutation escaped: {value}");
        errors.clear();
        audit_column("Real", &[value], "Real output", &mut errors);
        assert!(
            errors.is_empty(),
            "Real output was mistaken for an Integer input"
        );
    }
}
