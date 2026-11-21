//! The range rule and the error read at each tick, on constructed results: the integration tests
//! (`tests/tolerance_windows.rs`) drive the rest through real runs.

use super::{RANGE, at_ticks, in_range};
use crate::{CompareLimits, Envelope, Series, Tolerances};

#[test]
fn the_range_is_two_to_the_thousand_and_its_edge_is_judged() {
    assert_eq!(RANGE, 2.0_f64.powi(1000));
    assert!(in_range(RANGE));
    assert!(in_range(-RANGE));
    assert!(in_range(0.0) && in_range(-0.0));
    let above = f64::from_bits(RANGE.to_bits() + 1);
    assert!(!in_range(above));
    assert!(!in_range(-above));
    assert!(!in_range(f64::NAN));
    assert!(!in_range(f64::INFINITY));
    assert!(!in_range(f64::NEG_INFINITY));
    assert!(!in_range(f64::MAX));
}

#[test]
fn the_aligned_grid_errors_between_ticks_are_not_read() {
    let x = [0.0, 60.0, 120.0];
    let reference = [0.0, 1.0, 0.0];
    let test = [0.0, 1.25, 0.0];
    let tolerances = Tolerances {
        atolx: 30.0,
        atoly: 0.125,
        ..Tolerances::ZERO
    };
    let result = Envelope::AlignedGrid
        .compare(
            Series::new(&x, &reference),
            Series::new(&x, &test),
            &tolerances,
            &CompareLimits::default(),
        )
        .expect("compared");
    // The grid holds the rectangles' edges too, so it reports more errors than ticks.
    assert!(result.errors.len() > x.len());
    let errors = at_ticks(&result, &x).expect("every tick has an error");
    assert_eq!(errors, [0.0, 0.125, 0.0]);
    let result = Envelope::Funnel
        .compare(
            Series::new(&x, &reference),
            Series::new(&x, &test),
            &tolerances,
            &CompareLimits::default(),
        )
        .expect("compared");
    assert_eq!(result.errors.len(), x.len());
    let errors = at_ticks(&result, &x).expect("every tick has an error");
    assert_eq!(errors, result.errors.y());
}

#[test]
fn a_tick_the_errors_do_not_hold_reads_as_none() {
    let x = [0.0, 60.0];
    let y = [1.0, 1.0];
    let result = Envelope::Funnel
        .compare(
            Series::new(&x, &y),
            Series::new(&x, &y),
            &Tolerances {
                atoly: 0.5,
                ..Tolerances::ZERO
            },
            &CompareLimits::default(),
        )
        .expect("compared");
    assert_eq!(at_ticks(&result, &[0.0, 30.0]), None);
    assert_eq!(at_ticks(&result, &[60.0, 0.0]), None);
    assert_eq!(at_ticks(&result, &[0.0, 60.0]), Some(vec![0.0, 0.0]));
}
