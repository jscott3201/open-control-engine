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
    assert_eq!(masked.reference, vec![(0.0, 0.0), (2.0, 0.0), (3.0, 0.0)]);
    assert_eq!(masked.test, vec![(0.0, 0.0), (2.0, 0.0), (3.0, 0.0)]);
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
