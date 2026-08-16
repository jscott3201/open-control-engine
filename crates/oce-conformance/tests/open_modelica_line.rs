//! Scoped finite OpenModelica differential for `CDL.Reals.Line`.

mod block_harness;
#[path = "open_modelica_line/support.rs"]
mod scoped;

use oce_conformance::{ComparisonResult, DriveMode};

#[test]
fn four_closed_views_match_openmodelica_facade_and_independent_bits() {
    let canonical = scoped::canonical().unwrap();
    let views = scoped::views(&canonical).unwrap();
    assert_eq!(views.len(), 4);
    for (mode, view) in views {
        assert_eq!(
            view.col_names
                .as_ref()
                .unwrap()
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["time", "x1", "f1", "x2", "f2", "u", "y"]
        );
        for (source, projected) in canonical
            .data
            .chunks_exact(canonical.n_cols)
            .zip(view.data.chunks_exact(view.n_cols))
        {
            assert_eq!(
                projected[..6]
                    .iter()
                    .map(|value| value.to_bits())
                    .collect::<Vec<_>>(),
                source[..6]
                    .iter()
                    .map(|value| value.to_bits())
                    .collect::<Vec<_>>()
            );
        }
        let outcome = scoped::evaluate(mode, &view);
        assert_eq!(outcome.mode, mode);
        assert_eq!(outcome.warning_count, 0);
        assert_eq!(outcome.reference_bits, scoped::EXPECTED[mode_index(mode)]);
        assert_eq!(outcome.engine_bits, scoped::EXPECTED[mode_index(mode)]);
        let DriveMode::EventAligned { instants } = &outcome.drive_mode else {
            panic!("Line must use explicit event-aligned facade execution");
        };
        assert_eq!(
            instants
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>(),
            scoped::TIME_BITS
        );
        let ComparisonResult::Exact(exact) = outcome.comparison else {
            panic!("Line comparison must be exact");
        };
        assert!(exact.passed, "{mode:?}: {exact:?}");
        assert_eq!(exact.compared_points, scoped::TIME_BITS.len());
        assert_eq!(exact.first_mismatch, None);
    }
}

#[test]
fn repeated_facade_runs_preserve_every_output_bit() {
    let canonical = scoped::canonical().unwrap();
    for (mode, view) in scoped::views(&canonical).unwrap() {
        let first = scoped::evaluate(mode, &view);
        let second = scoped::evaluate(mode, &view);
        assert_eq!(first.reference_bits, second.reference_bits);
        assert_eq!(first.engine_bits, second.engine_bits);
        assert_eq!(first.drive_mode, second.drive_mode);
        assert_eq!(first.comparison, second.comparison);
    }
}

#[test]
fn external_flag_control_fails_at_the_above_range_transition() {
    let control = scoped::flag_control().unwrap();
    let view = scoped::view(&control, scoped::Mode::Below, "yBelow").unwrap();
    let outcome = scoped::evaluate(scoped::Mode::Below, &view);
    let ComparisonResult::Exact(exact) = outcome.comparison else {
        panic!("flag control comparison must be exact");
    };
    assert!(!exact.passed);
    let mismatch = exact.first_mismatch.unwrap();
    assert_eq!(mismatch.index, 8);
    assert_eq!(mismatch.x.to_bits(), scoped::TIME_BITS[8]);
    assert_eq!(mismatch.expected.to_bits(), 3.25_f64.to_bits());
    assert_eq!(mismatch.actual.to_bits(), 4.25_f64.to_bits());
}

#[test]
fn swapped_below_and_above_mapping_fails_at_the_below_range_row() {
    let canonical = scoped::canonical().unwrap();
    let swapped = scoped::view(&canonical, scoped::Mode::Below, "yAbove").unwrap();
    let outcome = scoped::evaluate(scoped::Mode::Below, &swapped);
    let ComparisonResult::Exact(exact) = outcome.comparison else {
        panic!("mapping control comparison must be exact");
    };
    let mismatch = exact.first_mismatch.unwrap();
    assert_eq!(mismatch.index, 0);
    assert_eq!(mismatch.x.to_bits(), scoped::TIME_BITS[0]);
    assert_eq!(mismatch.expected.to_bits(), 0.25_f64.to_bits());
    assert_eq!(mismatch.actual.to_bits(), 1.25_f64.to_bits());
}

#[test]
fn arithmetic_reference_mutants_turn_the_facade_comparator_red() {
    let canonical = scoped::canonical().unwrap();
    for (name, mode, mutant, row, reference_bits, engine_bits) in [
        (
            "always clamp",
            scoped::Mode::Below,
            scoped::always_clamp as fn(scoped::Mode, f64) -> f64,
            8,
            3.25_f64.to_bits(),
            4.25_f64.to_bits(),
        ),
        (
            "never clamp",
            scoped::Mode::Both,
            scoped::never_clamp,
            0,
            0.25_f64.to_bits(),
            1.25_f64.to_bits(),
        ),
        (
            "swapped flags",
            scoped::Mode::Below,
            scoped::swapped_flags,
            0,
            0.25_f64.to_bits(),
            1.25_f64.to_bits(),
        ),
        (
            "omitted intercept",
            scoped::Mode::Both,
            scoped::omitted_intercept,
            0,
            (-1.0_f64).to_bits(),
            1.25_f64.to_bits(),
        ),
    ] {
        let reference = scoped::mutated_reference(&canonical, mode, mutant).unwrap();
        for (canonical_row, mutant_row) in canonical
            .data
            .chunks_exact(canonical.n_cols)
            .zip(reference.data.chunks_exact(reference.n_cols))
        {
            assert_eq!(
                canonical_row[..6]
                    .iter()
                    .map(|value| value.to_bits())
                    .collect::<Vec<_>>(),
                mutant_row[..6]
                    .iter()
                    .map(|value| value.to_bits())
                    .collect::<Vec<_>>(),
                "{name} changed time or input bits"
            );
        }
        let outcome = scoped::evaluate(mode, &reference);
        assert_eq!(outcome.engine_bits, scoped::EXPECTED[mode_index(mode)]);
        let ComparisonResult::Exact(exact) = outcome.comparison else {
            panic!("{name} comparison must be exact");
        };
        assert!(!exact.passed, "{name}");
        let mismatch = exact.first_mismatch.unwrap();
        assert_eq!(mismatch.index, row, "{name}");
        assert_eq!(mismatch.x.to_bits(), scoped::TIME_BITS[row], "{name}");
        assert_eq!(mismatch.expected.to_bits(), reference_bits, "{name}");
        assert_eq!(mismatch.actual.to_bits(), engine_bits, "{name}");
    }
}

fn mode_index(mode: scoped::Mode) -> usize {
    match mode {
        scoped::Mode::Both => 0,
        scoped::Mode::Below => 1,
        scoped::Mode::Above => 2,
        scoped::Mode::Unlimited => 3,
    }
}
