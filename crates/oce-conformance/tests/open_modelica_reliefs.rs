//! Scoped OpenModelica differential for one composed G36 Reliefs leaf.

#[path = "open_modelica_reliefs/support.rs"]
mod scoped;

use oce_api::{Engine, OcError};
use oce_conformance::{ComparisonResult, DriveMode, DriverError};

#[test]
fn declared_roots_match_openmodelica_and_all_independent_bits() {
    let canonical = scoped::canonical().unwrap();
    let run = scoped::evaluate(&canonical, scoped::ROOT_OUTPUTS).unwrap();
    assert!(run.load_warnings.is_empty());
    assert_eq!(run.comparisons.len(), 2);
    assert_eq!(
        run.trace
            .columns
            .iter()
            .map(|column| column.name.as_str())
            .collect::<Vec<_>>(),
        scoped::ROOT_OUTPUTS
    );
    assert!(
        run.trace
            .columns
            .iter()
            .all(|column| !column.name.ends_with(".min.y") && !column.name.ends_with(".max.y"))
    );
    for (comparison, expected) in run.comparisons.iter().zip(scoped::EXPECTED) {
        assert!(!comparison.masked);
        assert_eq!(comparison.tolerance, scoped::ZERO_TOLERANCE);
        let ComparisonResult::Exact(exact) = &comparison.result else {
            panic!("Reliefs comparison must be exact");
        };
        assert!(exact.passed, "{exact:?}");
        assert_eq!(exact.compared_points, 7);
        assert_eq!(exact.first_mismatch, None);
        let captured = run.trace.column(&comparison.output).unwrap();
        assert_eq!(
            captured
                .values
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>(),
            expected
        );
        let reference = scoped::column_bits(&canonical, &comparison.reference_column).unwrap();
        assert_eq!(reference, expected);
    }
    let DriveMode::EventAligned { instants } = &run.drive_mode else {
        panic!("Reliefs must use explicit event-aligned facade execution");
    };
    assert_eq!(
        instants
            .iter()
            .map(|value| value.to_bits())
            .collect::<Vec<_>>(),
        scoped::TIME_BITS
    );
}

#[test]
fn topology_binds_declared_roots_to_min_and_max_drivers() {
    let mut engine = Engine::in_memory();
    let report = engine.load_cxf(scoped::CXF.as_bytes()).unwrap();
    assert!(report.warnings.is_empty());
    let topology = engine.topology();
    for (root, driver) in scoped::ROOT_OUTPUTS.into_iter().zip(scoped::CHILD_OUTPUTS) {
        let declared = topology
            .boundary_outputs
            .iter()
            .find(|output| output.path == root)
            .unwrap_or_else(|| panic!("missing declared root {root}"));
        assert_eq!(declared.driver_path, driver);
        assert_ne!(declared.path, declared.driver_path);
    }
}

#[test]
fn repeated_facade_runs_preserve_root_output_and_comparison_bits() {
    let canonical = scoped::canonical().unwrap();
    let first = scoped::evaluate(&canonical, scoped::ROOT_OUTPUTS).unwrap();
    let second = scoped::evaluate(&canonical, scoped::ROOT_OUTPUTS).unwrap();
    assert_eq!(first.trace, second.trace);
    assert_eq!(first.comparisons, second.comparisons);
    assert_eq!(first.drive_mode, second.drive_mode);
}

#[test]
fn parameter_mapping_and_declared_path_controls_turn_red_for_pinned_reasons() {
    let parameter = scoped::parameter_control().unwrap();
    let parameter_run = scoped::evaluate(&parameter, scoped::ROOT_OUTPUTS).unwrap();
    let mismatch = scoped::first_mismatch(&parameter_run, "yOutDam");
    assert_eq!(mismatch.index, 2);
    assert_eq!(mismatch.x.to_bits(), scoped::TIME_BITS[2]);
    assert_eq!(mismatch.expected.to_bits(), 0x3fec_0000_0000_0000);
    assert_eq!(mismatch.actual.to_bits(), 0x3fe2_0000_0000_0000);

    let canonical = scoped::canonical().unwrap();
    let swapped = scoped::evaluate(
        &canonical,
        [scoped::ROOT_OUTPUTS[1], scoped::ROOT_OUTPUTS[0]],
    )
    .unwrap();
    let mismatch = scoped::first_mismatch(&swapped, "yOutDam");
    assert_eq!(mismatch.index, 0);
    assert_eq!(mismatch.expected.to_bits(), scoped::EXPECTED[0][0]);
    assert_eq!(mismatch.actual.to_bits(), scoped::EXPECTED[1][0]);

    let error =
        scoped::evaluate(&canonical, [scoped::ROOT_OUTPUTS[0], scoped::MISSING_ROOT]).unwrap_err();
    assert!(matches!(
        &error,
        DriverError::Engine(OcError::UnknownPoint(point)) if point == scoped::MISSING_ROOT
    ));
    assert_eq!(
        error.to_string(),
        "unknown point/connector 'http://example.org#g36.source.multizone_vav_economizer_modulations_reliefs.yMissing'"
    );
}

#[test]
fn inconsistent_limits_exercise_both_final_overwrite_paths() {
    let clamp = scoped::final_clamp().unwrap();
    let run = scoped::evaluate(&clamp, scoped::ROOT_OUTPUTS).unwrap();
    assert!(run.load_warnings.is_empty());
    for (comparison, expected) in run
        .comparisons
        .iter()
        .zip([[0x3fd0_0000_0000_0000; 7], [0x3fe8_0000_0000_0000; 7]])
    {
        let ComparisonResult::Exact(exact) = &comparison.result else {
            panic!("final-clamp comparison must be exact");
        };
        assert!(exact.passed);
        assert_eq!(exact.compared_points, 7);
        assert_eq!(
            run.trace
                .column(&comparison.output)
                .unwrap()
                .values
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>(),
            expected
        );
    }
}
