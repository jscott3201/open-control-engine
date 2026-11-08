//! Oracle cross-check: the `libm` functions the `no_std + alloc` core four call in place of the
//! `std`-only `f64` methods return the same bits as those methods.
//!
//! `oce-blocks` and `oce-expr` call `math::{floor, ceil, trunc, round, sqrt}` — their `math`
//! modules re-export the pinned `libm` functions — where they called
//! `f64::{floor, ceil, trunc, round, sqrt}`, because those methods are not in `core` on Rust 1.97.
//! All five are exactly specified by IEEE 754 (the first four return an integral value with no
//! rounding; `sqrt` is correctly rounded), so any conforming implementation agrees on every
//! non-NaN input. This test checks that claim against the `std` methods on the build target: a
//! fixed table of boundary values and a deterministic pseudo-random sweep of bit patterns. A NaN
//! input must give a NaN output; its payload is not compared, because every engine `Real` output
//! is canonicalized (`oce_model::determinism::canonicalize_real`) and the evaluator refuses NaN
//! where it would reach an integer.
//!
//! The `std` methods are this oracle's whole point. `clippy.toml` disallows them and the core
//! crate roots deny that lint (issue #399); an integration test is its own crate, which the
//! workspace lint table already allows, and the attribute below keeps that true if the table moves.
#![allow(clippy::disallowed_methods)]

type Pair = (&'static str, fn(f64) -> f64, fn(f64) -> f64);

const PAIRS: [Pair; 5] = [
    ("floor", libm::floor, f64::floor),
    ("ceil", libm::ceil, f64::ceil),
    ("trunc", libm::trunc, f64::trunc),
    ("round", libm::round, f64::round),
    ("sqrt", libm::sqrt, f64::sqrt),
];

/// Values where rounding functions change behaviour: signed zeros, halves, the edges of the
/// integral range of `f64` (`2^52`, `2^53`), the `i64` range edges the engine casts at, the
/// subnormal range and the infinities.
fn boundary_values() -> Vec<f64> {
    let mut values = vec![
        0.0,
        -0.0,
        0.5,
        1.5,
        2.5,
        0.499_999_999_999_999_94,
        1.0 - f64::EPSILON / 2.0,
        4_503_599_627_370_495.5,
        4_503_599_627_370_496.0,
        9_007_199_254_740_992.0,
        9_007_199_254_740_993.0,
        9_223_372_036_854_775_808.0,
        9_223_372_036_854_774_784.0,
        1e-9,
        1e300,
        f64::MIN_POSITIVE,
        f64::MIN_POSITIVE / 2.0,
        f64::from_bits(1),
        f64::MAX,
        f64::INFINITY,
        f64::EPSILON,
        86_400.0,
        31_536_000.0,
        1_700_000_000.123_456,
    ];
    for k in 0..=64_i32 {
        let base = f64::from(k);
        values.extend([base, base + 0.5, base - 0.5, base * 1.0e6 + 0.5]);
        values.push(2.0_f64.powi(k));
        values.push(f64::from_bits(2.0_f64.powi(k).to_bits() - 1));
        values.push(f64::from_bits(2.0_f64.powi(k).to_bits() + 1));
    }
    let negated: Vec<f64> = values.iter().map(|value| -value).collect();
    values.extend(negated);
    values
}

/// A deterministic xorshift64* sweep of raw bit patterns: every exponent and sign is reached.
fn swept_values(count: usize) -> Vec<f64> {
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    (0..count)
        .map(|_| f64::from_bits(xorshift(&mut state)))
        .collect()
}

/// The deterministic xorshift64* generator behind both sweeps.
fn xorshift(state: &mut u64) -> u64 {
    *state ^= *state >> 12;
    *state ^= *state << 25;
    *state ^= *state >> 27;
    state.wrapping_mul(0x2545_f491_4f6c_dd1d)
}

/// Values around the decision boundaries of the rounding functions, which a raw bit-pattern sweep
/// rarely reaches (about 2.6% of its draws have a magnitude in `[0.5, 2^53]`). For `count` random
/// integers `k` below `2^53`, both signs: `k + 0.5`, one ulp either side of it, `k + 0.25` and
/// `k + 0.75`. Each candidate is formed by `f64` addition, so the low-precision cases near `2^53`
/// round to the nearest representable value; the exact cases are dense through `2^52`.
fn half_integer_values(count: usize) -> Vec<f64> {
    let mut state: u64 = 0xd1b5_4a32_d192_ed03;
    let mut values = Vec::with_capacity(count * 5);
    for _ in 0..count {
        let draw = xorshift(&mut state);
        // Spread k over every magnitude below 2^53: a random bit width, then random bits.
        let width = (draw >> 58) % 54;
        let k = (xorshift(&mut state) >> 11) & ((1_u64 << width) - 1);
        let sign = if draw & 1 == 0 { 1.0 } else { -1.0 };
        let base = k as f64;
        let half = base + 0.5;
        for x in [
            half,
            f64::from_bits(half.to_bits() + 1),
            f64::from_bits(half.to_bits() - 1),
            base + 0.25,
            base + 0.75,
        ] {
            values.push(sign * x);
        }
    }
    values
}

fn assert_parity(values: &[f64]) {
    for &(name, from_libm, from_std) in &PAIRS {
        for &x in values {
            let (got, want) = (from_libm(x), from_std(x));
            if want.is_nan() {
                assert!(
                    got.is_nan(),
                    "{name}({x:e} = {:#018x}): libm {got:e}",
                    x.to_bits()
                );
            } else {
                assert_eq!(
                    got.to_bits(),
                    want.to_bits(),
                    "{name}({x:e} = {:#018x}): libm {got:e}, std {want:e}",
                    x.to_bits()
                );
            }
        }
    }
}

#[test]
fn libm_matches_std_bit_for_bit_on_boundary_values() {
    let values = boundary_values();
    assert!(values.len() > 500);
    assert_parity(&values);
}

#[test]
fn libm_matches_std_bit_for_bit_across_a_bit_pattern_sweep() {
    assert_parity(&swept_values(1 << 20));
}

#[test]
fn libm_matches_std_bit_for_bit_around_half_integers() {
    let values = half_integer_values(1 << 18);
    // The sweep does reach the rounding boundary: most cases are exact halves below 2^52.
    let exact_halves = values
        .iter()
        .filter(|x| x.abs() < 4_503_599_627_370_496.0 && x.fract().abs() == 0.5)
        .count();
    assert!(
        exact_halves > values.len() / 10,
        "{exact_halves} exact halves"
    );
    assert_parity(&values);
}

#[test]
fn nan_inputs_give_nan_outputs() {
    let nans = [
        f64::NAN,
        -f64::NAN,
        f64::from_bits(0x7ff8_0000_0000_0001),
        f64::from_bits(0xfff8_dead_beef_0000),
    ];
    assert_parity(&nans);
}

#[test]
fn negative_sqrt_is_nan_in_both() {
    for x in [
        -1.0,
        -f64::MIN_POSITIVE,
        -f64::from_bits(1),
        f64::NEG_INFINITY,
    ] {
        assert!(libm::sqrt(x).is_nan() && x.sqrt().is_nan(), "sqrt({x:e})");
    }
}
