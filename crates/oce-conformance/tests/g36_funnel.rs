//! G36 Tier-A independent-oracle checks through the B3 facade driver.

use std::path::{Path, PathBuf};

use oce_conformance::{
    CombiTimeTable, ComparisonMode, ComparisonResult, DriveCadence, DriverInputReplay,
    DriverOptions, IndicatorPattern, PointEnd, PointMapEntry, ReferenceSpec, Tolerances, ValueKind,
    VerifyConfig, drive_trace_with_options, escape_regex,
};
use serde_json::Value;

#[path = "g36_funnel_band/sequences.rs"]
mod sequences;
use sequences::*;

#[test]
fn g36_tier_a_sequence_oracles_match_engine_outputs() {
    for spec in SEQUENCES {
        let reference = read_reference(spec);
        assert_reference_shape(spec, &reference);
        assert_signal_provenance(spec, &reference);

        if !spec.exact_outputs.is_empty() {
            let run = drive_trace_with_options(
                spec.cxf.as_bytes(),
                &config_for(spec, spec.exact_outputs, Vec::new()),
                &reference,
                &options_for(spec, ComparisonMode::Exact),
            )
            .unwrap_or_else(|err| panic!("{} exact driver run failed: {err}", spec.name));
            assert_exact_comparisons(spec, spec.exact_outputs, reference.n_rows, &run.comparisons);
        }

        if !spec.masked_outputs.is_empty() {
            let run = drive_trace_with_options(
                spec.cxf.as_bytes(),
                &config_for(spec, spec.masked_outputs, vav_mask_indicators()),
                &reference,
                &options_for(spec, ComparisonMode::Funnel),
            )
            .unwrap_or_else(|err| panic!("{} masked driver run failed: {err}", spec.name));
            assert_masked_funnel_comparisons(spec, spec.masked_outputs, &run);
            assert_vav_heating_guard_is_non_vacuous(&run);
        }
    }
}

fn read_reference(spec: &SequenceSpec) -> CombiTimeTable {
    CombiTimeTable::read(&reference_path(spec))
        .unwrap_or_else(|err| panic!("{} reference read failed: {err}", spec.name))
}

fn config_for(
    spec: &SequenceSpec,
    outputs: &[PointSpec],
    indicators: Vec<IndicatorPattern>,
) -> VerifyConfig {
    VerifyConfig {
        references: vec![ReferenceSpec {
            model: "g36".to_string(),
            sequence: spec.name.to_string(),
            point_name_mapping: point_mapping(spec.inputs, outputs),
        }],
        tolerances: zero_tolerances(),
        outputs: Vec::new(),
        indicators,
        sampling: Some(spec.sample_step),
        run_controller: true,
    }
}

fn point_mapping(inputs: &[PointSpec], outputs: &[PointSpec]) -> Vec<PointMapEntry> {
    inputs
        .iter()
        .chain(outputs.iter())
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

fn options_for(spec: &SequenceSpec, comparison: ComparisonMode) -> DriverOptions {
    DriverOptions {
        cadence: DriveCadence::EventAligned {
            instants: (0..=spec.t_stop)
                .map(|tick| f64::from(tick) * spec.sample_step)
                .collect(),
        },
        input_replay: DriverInputReplay::ReferenceTable,
        comparison,
    }
}

fn vav_mask_indicators() -> Vec<IndicatorPattern> {
    // Anchored AND escaped: authored point paths carry `.` segments, and an unescaped dot can
    // silently select a point the pattern never named.
    vec![IndicatorPattern {
        pattern: format!(
            "^({}|{})$",
            escape_regex(VAV_AIRFLOW_SETPOINT_PATH),
            escape_regex(VAV_DAMPER_COMMAND_PATH)
        ),
        signals: vec![VAV_HEATING_ENABLED_PATH.to_string()],
    }]
}

fn zero_tolerances() -> Tolerances {
    Tolerances {
        atolx: 0.0,
        atoly: 0.0,
        rtolx: 0.0,
        rtoly: 0.0,
        ltolx: 0.0,
        ltoly: 0.0,
    }
}

fn assert_exact_comparisons(
    spec: &SequenceSpec,
    outputs: &[PointSpec],
    expected_rows: usize,
    comparisons: &[oce_conformance::SignalComparison],
) {
    assert_eq!(
        comparisons.len(),
        outputs.len(),
        "{} exact comparison count",
        spec.name
    );
    for output in outputs {
        let comparison = find_comparison(spec, comparisons, output);
        assert!(
            !comparison.masked,
            "{} {} must be unmasked",
            spec.name, output.reference_name
        );
        assert_eq!(
            comparison.output, output.cdl_name,
            "{} runtime output for {}",
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
                    "{} exact compared points for {}",
                    spec.name, output.reference_name
                );
                assert_eq!(
                    result.first_mismatch, None,
                    "{} exact first mismatch for {}",
                    spec.name, output.reference_name
                );
            }
            other => panic!(
                "{} {} used non-exact comparison: {other:?}",
                spec.name, output.reference_name
            ),
        }
    }
}

fn assert_masked_funnel_comparisons(
    spec: &SequenceSpec,
    outputs: &[PointSpec],
    run: &oce_conformance::DriverRun,
) {
    assert_eq!(
        run.comparisons.len(),
        outputs.len(),
        "{} masked comparison count",
        spec.name
    );
    for output in outputs {
        let comparison = find_comparison(spec, &run.comparisons, output);
        assert!(
            comparison.masked,
            "{} {} must be masked",
            spec.name, output.reference_name
        );
        match &comparison.result {
            ComparisonResult::Funnel(result) => {
                assert!(
                    result.passed,
                    "{} masked funnel failed for {} at {:?}",
                    spec.name, output.reference_name, result.first_failure_x
                );
                assert_eq!(
                    result.max_error.to_bits(),
                    0.0f64.to_bits(),
                    "{} masked funnel max error for {}",
                    spec.name,
                    output.reference_name
                );
                assert!(
                    result.compared_points > 0,
                    "{} masked funnel for {} passed vacuously",
                    spec.name,
                    output.reference_name
                );
                assert_eq!(
                    result.first_failure_x, None,
                    "{} masked funnel first failure for {}",
                    spec.name, output.reference_name
                );
            }
            other => panic!(
                "{} {} used non-funnel comparison: {other:?}",
                spec.name, output.reference_name
            ),
        }
    }
}

fn assert_vav_heating_guard_is_non_vacuous(run: &oce_conformance::DriverRun) {
    let indicator = run
        .trace
        .column(VAV_HEATING_ENABLED_PATH)
        .expect("captured VAV heating indicator");
    assert_eq!(
        run.trace.times,
        vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0],
        "VAV event-aligned mask trace times"
    );
    assert_eq!(
        indicator.values,
        vec![0.0, 0.0, 0.0, 1.0, 1.0, 0.0],
        "VAV heating_enabled must be true at t=3,4 and false around it"
    );
}

fn find_comparison<'a>(
    spec: &SequenceSpec,
    comparisons: &'a [oce_conformance::SignalComparison],
    output: &PointSpec,
) -> &'a oce_conformance::SignalComparison {
    comparisons
        .iter()
        .find(|comparison| comparison.reference_column == output.reference_name)
        .unwrap_or_else(|| {
            panic!(
                "{} missing comparison for {}",
                spec.name, output.reference_name
            )
        })
}

fn assert_reference_shape(spec: &SequenceSpec, reference: &CombiTimeTable) {
    assert_eq!(reference.name, format!("G36_{}_reference", spec.name));
    assert_eq!(reference.n_rows, spec.t_stop as usize + 1);
    assert_eq!(
        reference.col_names.as_deref(),
        Some(reference_columns(spec).as_slice()),
        "{} reference columns",
        spec.name
    );
}

fn reference_columns(spec: &SequenceSpec) -> Vec<String> {
    let mut names = Vec::new();
    names.push("time".to_string());
    names.extend(
        spec.inputs
            .iter()
            .map(|point| point.reference_name.to_string()),
    );
    names.extend(
        spec.exact_outputs
            .iter()
            .chain(spec.masked_outputs.iter())
            .map(|point| point.reference_name.to_string()),
    );
    names
}

fn assert_signal_provenance(spec: &SequenceSpec, reference: &CombiTimeTable) {
    for output in spec.exact_outputs.iter().chain(spec.masked_outputs.iter()) {
        let prov = read_json(&signal_provenance_path(spec, output.reference_name));
        assert_eq!(
            prov["class_path"], "G36",
            "{} {} class_path",
            spec.name, output.reference_name
        );
        assert_eq!(
            prov["scenario"], spec.name,
            "{} {} scenario",
            spec.name, output.reference_name
        );
        assert_eq!(
            prov["signal"], output.reference_name,
            "{} {} signal",
            spec.name, output.reference_name
        );
        assert_eq!(
            prov["tier"], "A",
            "{} {} tier",
            spec.name, output.reference_name
        );
        assert_eq!(
            prov["depends_on_oce_blocks"], false,
            "{} {} must be independent of oce-blocks",
            spec.name, output.reference_name
        );
        assert!(
            prov["source"]
                .as_str()
                .is_some_and(|source| source.contains("Buildings")),
            "{} {} source should cite Buildings semantics",
            spec.name,
            output.reference_name
        );
        assert_eq!(
            json_string_array(&prov["reference_columns"]),
            reference
                .col_names
                .as_ref()
                .expect("reference columns")
                .clone(),
            "{} {} provenance reference columns",
            spec.name,
            output.reference_name
        );
    }
}

fn json_string_array(value: &Value) -> Vec<String> {
    value
        .as_array()
        .expect("JSON string array")
        .iter()
        .map(|item| item.as_str().expect("JSON string").to_string())
        .collect()
}

fn read_json(path: &Path) -> Value {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("read JSON {} failed: {err}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|err| panic!("parse JSON {} failed: {err}", path.display()))
}

fn signal_provenance_path(spec: &SequenceSpec, signal: &str) -> PathBuf {
    reference_dir(spec).join(format!("{signal}.prov.json"))
}
