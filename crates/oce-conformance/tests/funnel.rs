//! Edge, golden, oracle-substitute, and determinism coverage for the funnel L1 band.

use oce_conformance::{Series, Tolerances, build_bounds, compare};

type Curve = Vec<(f64, f64)>;

fn tight_tol() -> Tolerances {
    Tolerances {
        atolx: 0.1,
        atoly: 0.1,
        rtolx: 0.0,
        rtoly: 0.0,
        ltolx: 0.0,
        ltoly: 0.0,
    }
}

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

fn large_atolx_tol() -> Tolerances {
    Tolerances {
        atolx: 10.0,
        atoly: 0.1,
        rtolx: 0.0,
        rtoly: 0.0,
        ltolx: 0.0,
        ltoly: 0.0,
    }
}

fn rel_only_tol() -> Tolerances {
    Tolerances {
        atolx: 0.0,
        atoly: 0.0,
        rtolx: 0.1,
        rtoly: 0.1,
        ltolx: 0.0,
        ltoly: 0.0,
    }
}

fn local_only_tol() -> Tolerances {
    Tolerances {
        atolx: 0.0,
        atoly: 0.0,
        rtolx: 0.0,
        rtoly: 0.0,
        ltolx: 0.2,
        ltoly: 0.2,
    }
}

fn asymmetric_tol() -> Tolerances {
    Tolerances {
        atolx: 0.5,
        atoly: 0.25,
        rtolx: 0.25,
        rtoly: 0.5,
        ltolx: 0.125,
        ltoly: 0.25,
    }
}

fn series<'a>(x: &'a [f64], y: &'a [f64]) -> Series<'a> {
    Series { x, y }
}

fn parse_bounds(text: &str) -> (Curve, Curve) {
    enum Section {
        Lower,
        Upper,
    }
    let mut section = None;
    let mut lower = Vec::new();
    let mut upper = Vec::new();
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        match line {
            "lower" => section = Some(Section::Lower),
            "upper" => section = Some(Section::Upper),
            _ => {
                let (x, y) = line.split_once(',').expect("golden point uses comma");
                let point = (x.parse::<f64>().unwrap(), y.parse::<f64>().unwrap());
                match section {
                    Some(Section::Lower) => lower.push(point),
                    Some(Section::Upper) => upper.push(point),
                    None => panic!("point before section header"),
                }
            }
        }
    }
    (lower, upper)
}

fn assert_pairs_bit_eq(actual: &[(f64, f64)], expected: &[(f64, f64)]) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "length mismatch: {actual:?} != {expected:?}"
    );
    for (idx, (a, e)) in actual.iter().zip(expected).enumerate() {
        assert_eq!(
            a.0.to_bits(),
            e.0.to_bits(),
            "x bits differ at {idx}: {a:?} != {e:?}"
        );
        assert_eq!(
            a.1.to_bits(),
            e.1.to_bits(),
            "y bits differ at {idx}: {a:?} != {e:?}"
        );
    }
}

#[test]
fn default_tolerances_match_cdl_listing_12_1() {
    let tol = Tolerances::default();
    assert_eq!(tol.atolx.to_bits(), 10.0f64.to_bits());
    assert_eq!(tol.atoly.to_bits(), 0.0f64.to_bits());
    assert_eq!(tol.rtolx.to_bits(), 0.002f64.to_bits());
    assert_eq!(tol.rtoly.to_bits(), 0.002f64.to_bits());
    assert_eq!(tol.ltolx.to_bits(), 0.0f64.to_bits());
    assert_eq!(tol.ltoly.to_bits(), 0.0f64.to_bits());
}

#[test]
fn build_bounds_peak_extremum_matches_hand_computed_golden() {
    let x = [0.0, 1.0, 2.0];
    let y = [0.0, 1.0, 0.0];
    let (lower, upper) = build_bounds(series(&x, &y), &tight_tol());
    let (want_lower, want_upper) = parse_bounds(include_str!("fixtures/golden/peak_bounds.txt"));
    assert_pairs_bit_eq(&lower, &want_lower);
    assert_pairs_bit_eq(&upper, &want_upper);
}

#[test]
fn build_bounds_valley_extremum_matches_hand_computed_golden() {
    let x = [0.0, 1.0, 2.0];
    let y = [1.0, 0.0, 1.0];
    let (lower, upper) = build_bounds(series(&x, &y), &tight_tol());
    let (want_lower, want_upper) = parse_bounds(include_str!("fixtures/golden/valley_bounds.txt"));
    assert_pairs_bit_eq(&lower, &want_lower);
    assert_pairs_bit_eq(&upper, &want_upper);
}

#[test]
fn build_bounds_multi_extremum_matches_hand_computed_golden() {
    let x = [0.0, 1.0, 2.0, 3.0];
    let y = [0.0, 2.0, -1.0, 1.0];
    let (lower, upper) = build_bounds(series(&x, &y), &tight_tol());
    let (want_lower, want_upper) =
        parse_bounds(include_str!("fixtures/golden/multi_extremum_bounds.txt"));
    assert_pairs_bit_eq(&lower, &want_lower);
    assert_pairs_bit_eq(&upper, &want_upper);
}

#[test]
fn build_bounds_asymmetric_tolerance_matches_hand_computed_golden() {
    let x = [0.0, 2.0];
    let y = [1.0, 3.0];
    let (lower, upper) = build_bounds(series(&x, &y), &asymmetric_tol());
    let (want_lower, want_upper) =
        parse_bounds(include_str!("fixtures/golden/asymmetric_bounds.txt"));
    assert_pairs_bit_eq(&lower, &want_lower);
    assert_pairs_bit_eq(&upper, &want_upper);
}

#[test]
fn zero_length_and_single_point_are_total() {
    let empty: [f64; 0] = [];
    let (lower, upper) = build_bounds(series(&empty, &empty), &tight_tol());
    assert!(lower.is_empty());
    assert!(upper.is_empty());
    assert!(compare(series(&empty, &empty), series(&empty, &empty), &tight_tol()).passed);

    let x = [2.0];
    let y = [3.0];
    let (lower, upper) = build_bounds(series(&x, &y), &tight_tol());
    assert_pairs_bit_eq(&lower, &[(1.9, 2.9), (2.0, 2.9), (2.1, 2.9)]);
    assert_pairs_bit_eq(&upper, &[(1.9, 3.1), (2.0, 3.1), (2.1, 3.1)]);
}

#[test]
fn repeated_x_vertical_segments_remain_monotone_and_deterministic() {
    let x = [0.0, 1.0, 1.0, 2.0];
    let y = [0.0, 0.0, 1.0, 1.0];
    let first = build_bounds(series(&x, &y), &tight_tol());
    let second = build_bounds(series(&x, &y), &tight_tol());
    assert_eq!(first, second);
    assert!(first.0.windows(2).all(|w| w[0].0 <= w[1].0));
    assert!(first.1.windows(2).all(|w| w[0].0 <= w[1].0));
}

#[test]
fn range_zero_collapses_relative_tolerance_to_absolute_only() {
    let x = [0.0, 1.0, 2.0];
    let y = [5.0, 5.0, 5.0];
    let tol = Tolerances {
        rtoly: 0.5,
        ..exact_tol()
    };
    let pass = compare(series(&x, &y), series(&x, &y), &tol);
    assert!(pass.passed);
    let test_y = [5.0, 5.25, 5.0];
    let fail = compare(series(&x, &y), series(&x, &test_y), &tol);
    assert!(
        !fail.passed,
        "range_y == 0 means rtoly contributes no y slack"
    );
    assert_eq!(fail.first_failure_x, Some(1.0));
}

#[test]
fn test_x_outside_reference_range_clamps_to_nearest_bound_endpoint() {
    let ref_x = [0.0, 1.0];
    let ref_y = [0.0, 0.0];
    let test_x = [-1.0, 0.5, 2.0];
    let ok_y = [0.1, 0.0, -0.1];
    let pass = compare(series(&ref_x, &ref_y), series(&test_x, &ok_y), &tight_tol());
    assert!(pass.passed);

    let bad_y = [0.1, 0.0, 0.10000000000000002];
    let fail = compare(
        series(&ref_x, &ref_y),
        series(&test_x, &bad_y),
        &tight_tol(),
    );
    assert!(!fail.passed);
    assert_eq!(fail.first_failure_x, Some(2.0));
}

#[test]
fn nan_inf_and_decreasing_x_are_failures_not_panics() {
    let bad_x = [0.0, f64::NAN];
    let y = [0.0, 1.0];
    assert!(build_bounds(series(&bad_x, &y), &tight_tol()).0.is_empty());
    let result = compare(series(&bad_x, &y), series(&bad_x, &y), &tight_tol());
    assert!(!result.passed);
    assert!(result.max_error.is_infinite());

    let dec_x = [1.0, 0.0];
    let result = compare(series(&dec_x, &y), series(&dec_x, &y), &tight_tol());
    assert!(!result.passed);
    assert!(result.max_error.is_infinite());
}

#[test]
fn compare_checks_bound_breakpoints_not_only_test_samples() {
    let ref_x = [0.0, 1.0, 2.0];
    let ref_y = [0.0, 1.0, 0.0];
    let test_x = [0.0, 2.0];
    let test_y = [0.0, 0.0];
    let result = compare(
        series(&ref_x, &ref_y),
        series(&test_x, &test_y),
        &tight_tol(),
    );
    assert!(!result.passed);
    assert_eq!(result.first_failure_x, Some(0.9));
}

#[test]
fn default_tolerance_self_encloses_clustered_asymmetric_peak() {
    let x = [0.0, 0.5, 3.0];
    let y = [0.0, 5.0, 4.5];
    let result = compare(series(&x, &y), series(&x, &y), &Tolerances::default());
    assert!(
        result.passed,
        "bit-identical trace must self-enclose: {result:?}"
    );
    assert_eq!(result.max_error.to_bits(), 0.0f64.to_bits());
}

#[test]
fn empty_test_against_non_empty_reference_is_a_hard_failure() {
    let ref_x = [0.0, 1.0, 2.0];
    let ref_y = [0.0, 1.0, 0.0];
    let empty: [f64; 0] = [];
    let result = compare(
        series(&ref_x, &ref_y),
        series(&empty, &empty),
        &Tolerances::default(),
    );
    assert!(!result.passed);
    assert!(result.max_error.is_infinite());
}

#[test]
fn reference_series_self_enclose_across_tolerance_regimes() {
    let fixed: Vec<(Vec<f64>, Vec<f64>)> = vec![
        (vec![0.0, 1.0, 2.0, 3.0], vec![0.0, 1.0, 2.0, 3.0]),
        (vec![0.0, 1.0, 2.0], vec![0.0, 3.0, 0.0]),
        (vec![0.0, 0.5, 3.0], vec![0.0, 5.0, 4.5]),
        (vec![0.0, 1.0, 2.0], vec![2.0, -1.0, 2.0]),
        (
            vec![0.0, 1.0, 1.0, 2.0, 2.0, 3.0],
            vec![0.0, 2.0, 2.0, 0.0, 0.0, -1.0],
        ),
        (
            vec![0.0, 1.0, 2.0, 3.0, 4.0],
            vec![0.0, 2.0, -2.0, 3.0, -1.0],
        ),
        (vec![0.0, 1.0, 2.0, 3.0], vec![4.0, 4.0, 4.0, 4.0]),
        (
            vec![0.0, 1.0e-9, 2.0e-9, 1.0],
            vec![0.0, 1.0e-12, -1.0e-12, 0.0],
        ),
        (vec![0.0, 1.0, 2.0, 3.0], vec![-1.0, -3.0, -2.0, -4.0]),
    ];
    let mut cases = fixed;
    let mut seed = 0x5eed_f00d_cafe_babe_u64;
    for _ in 0..32 {
        let len = 3 + (lcg(&mut seed) % 8) as usize;
        let mut x = Vec::with_capacity(len);
        let mut y = Vec::with_capacity(len);
        let mut cursor = 0.0;
        for idx in 0..len {
            if idx > 0 && lcg(&mut seed).is_multiple_of(5) {
                // Deliberately keep a duplicate x sample.
            } else if idx > 0 {
                cursor += (lcg(&mut seed) % 100) as f64 / 37.0;
            }
            x.push(cursor);
            y.push((lcg(&mut seed) % 2_000) as f64 / 17.0 - 1_000.0 / 17.0);
        }
        cases.push((x, y));
    }

    let tolerances = [
        Tolerances::default(),
        tight_tol(),
        large_atolx_tol(),
        rel_only_tol(),
        local_only_tol(),
        asymmetric_tol(),
        exact_tol(),
    ];
    for (case_idx, (x, y)) in cases.iter().enumerate() {
        for (tol_idx, tol) in tolerances.iter().enumerate() {
            let result = compare(series(x, y), series(x, y), tol);
            assert!(
                result.passed,
                "case {case_idx}, tolerance {tol_idx} did not self-enclose: {result:?}"
            );
            assert_eq!(
                result.max_error.to_bits(),
                0.0f64.to_bits(),
                "case {case_idx}, tolerance {tol_idx} produced nonzero self-error"
            );
        }
    }
}

#[test]
fn compare_and_bounds_are_byte_deterministic_across_reruns() {
    let x = [0.0, 1.0, 2.0, 3.0];
    let y = [0.0, 1.0, -1.0, 0.5];
    let test_y = [0.0, 1.05, -0.95, 0.5];
    assert_eq!(
        build_bounds(series(&x, &y), &tight_tol()),
        build_bounds(series(&x, &y), &tight_tol())
    );
    assert_eq!(
        compare(series(&x, &y), series(&x, &test_y), &tight_tol()),
        compare(series(&x, &y), series(&x, &test_y), &tight_tol())
    );
}

fn lcg(seed: &mut u64) -> u64 {
    *seed = seed
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    *seed
}
