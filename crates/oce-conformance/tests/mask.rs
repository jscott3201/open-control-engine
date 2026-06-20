//! Indicator masking coverage for don't-care comparison intervals.

use oce_conformance::{Indicator, Mask, Series, Tolerances, compare, compare_masked};

fn exact_tol() -> Tolerances {
    Tolerances {
        atolx: 0.0,
        atoly: 0.0,
        rtolx: 0.0,
        rtoly: 0.0,
        ltolx: 0.0,
        ltoly: 0.0,
    }
}

fn one_y_tol() -> Tolerances {
    Tolerances {
        atoly: 1.0,
        ..exact_tol()
    }
}

fn series<'a>(x: &'a [f64], y: &'a [f64]) -> Series<'a> {
    Series { x, y }
}

#[test]
fn indicator_uses_step_hold_semantics() {
    let indicator = Indicator {
        signal: "fanSta.y".into(),
        samples: vec![(1.0, true), (3.0, false), (5.0, true)],
    };
    assert!(!indicator.active_at(0.5));
    assert!(indicator.active_at(1.0));
    assert!(indicator.active_at(2.999));
    assert!(!indicator.active_at(3.0));
    assert!(!indicator.active_at(4.0));
    assert!(indicator.active_at(5.0));
}

#[test]
fn multiple_indicators_are_anded() {
    let mask = Mask {
        indicators: vec![
            Indicator {
                signal: "fanSta.y".into(),
                samples: vec![(0.0, true), (2.0, false)],
            },
            Indicator {
                signal: "mode.y".into(),
                samples: vec![(0.0, false), (1.0, true)],
            },
        ],
    };
    assert!(!mask.active_at(0.5));
    assert!(mask.active_at(1.5));
    assert!(!mask.active_at(2.0));
}

#[test]
fn dont_care_interval_suppresses_injected_deviation() {
    let ref_x = [0.0, 1.0, 2.0, 3.0];
    let ref_y = [0.0, 0.0, 0.0, 0.0];
    let test_y = [0.0, 50.0, 0.0, 0.0];
    let unmasked = compare(
        series(&ref_x, &ref_y),
        series(&ref_x, &test_y),
        &exact_tol(),
    );
    assert!(!unmasked.passed);
    assert_eq!(unmasked.first_failure_x, Some(1.0));

    let mask = Mask {
        indicators: vec![Indicator {
            signal: "fanSta.y".into(),
            samples: vec![(0.0, true), (0.5, false), (1.5, true)],
        }],
    };
    let masked = compare_masked(
        series(&ref_x, &ref_y),
        series(&ref_x, &test_y),
        &exact_tol(),
        &mask,
    );
    assert!(
        masked.passed,
        "masked interval must not contribute: {masked:?}"
    );
    assert_eq!(masked.max_error.to_bits(), 0.0f64.to_bits());
    assert_eq!(
        masked.reference,
        vec![(0.0, 0.0), (0.5, 0.0), (1.5, 0.0), (2.0, 0.0), (3.0, 0.0)]
    );
    assert_eq!(masked.test, vec![(0.0, 0.0), (2.0, 0.0), (3.0, 0.0)]);
    assert!(masked.compared_points > 0);
}

#[test]
fn segmented_mask_does_not_bridge_dont_care_gap_on_differing_grids() {
    let ref_x = [0.0, 2.0, 4.0];
    let ref_y = [0.0, 99.0, 0.0];
    let test_x = [0.0, 1.8, 2.2, 4.0];
    let test_y = [0.0, 0.0, 0.0, 0.0];
    let mask = Mask {
        indicators: vec![Indicator {
            signal: "fanSta.y".into(),
            samples: vec![(0.0, true), (1.9, false), (2.1, true)],
        }],
    };

    let result = compare_masked(
        series(&ref_x, &ref_y),
        series(&test_x, &test_y),
        &one_y_tol(),
        &mask,
    );
    assert!(
        !result.passed,
        "segmented mask must not fabricate a flat chord across the inactive gap: {result:?}"
    );
    assert!(result.max_error > 90.0);
    assert_eq!(result.compared_points, 6);
    assert_eq!(result.first_failure_x, Some(1.8));
    assert_eq!(
        result.reference,
        vec![(0.0, 0.0), (1.9, 94.05), (2.1, 94.05), (4.0, 0.0)]
    );
}

#[test]
fn segmented_mask_result_is_byte_deterministic() {
    let ref_x = [0.0, 2.0, 4.0];
    let ref_y = [0.0, 99.0, 0.0];
    let test_x = [0.0, 1.8, 2.2, 4.0];
    let test_y = [0.0, 0.0, 0.0, 0.0];
    let mask = Mask {
        indicators: vec![Indicator {
            signal: "fanSta.y".into(),
            samples: vec![(0.0, true), (1.9, false), (2.1, true)],
        }],
    };
    let first = compare_masked(
        series(&ref_x, &ref_y),
        series(&test_x, &test_y),
        &one_y_tol(),
        &mask,
    );
    let second = compare_masked(
        series(&ref_x, &ref_y),
        series(&test_x, &test_y),
        &one_y_tol(),
        &mask,
    );
    assert_eq!(first, second);
    assert_eq!(first.max_error.to_bits(), 93.05f64.to_bits());
    assert_eq!(first.compared_points, 6);
    assert_eq!(
        first.errors,
        vec![
            (0.0, 0.0),
            (1.8, 88.10000000000001),
            (1.9, 93.05),
            (2.1, 93.05),
            (2.2, 88.1),
            (4.0, 0.0)
        ]
    );
}

#[test]
fn all_masked_comparison_is_distinguishable_from_verified_pass() {
    let x = [0.0, 1.0, 2.0];
    let ref_y = [0.0, 0.0, 0.0];
    let test_y = [100.0, -50.0, 25.0];
    let mask = Mask {
        indicators: vec![Indicator {
            signal: "fanSta.y".into(),
            samples: vec![(0.0, false)],
        }],
    };
    let result = compare_masked(series(&x, &ref_y), series(&x, &test_y), &exact_tol(), &mask);
    assert!(result.passed, "a genuine full dont-care may pass");
    assert_eq!(
        result.compared_points, 0,
        "zero comparisons must not look like a verified non-empty pass"
    );
    assert!(result.errors.is_empty());
}

#[test]
fn empty_mask_is_a_noop() {
    let x = [0.0, 1.0];
    let y = [2.0, 3.0];
    let mask = Mask { indicators: vec![] };
    let mut sx = Vec::new();
    let mut sy = Vec::new();
    let masked = mask.apply(series(&x, &y), &mut sx, &mut sy);
    assert_eq!(masked.x, &x);
    assert_eq!(masked.y, &y);
}
