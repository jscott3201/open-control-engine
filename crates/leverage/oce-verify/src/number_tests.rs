//! The canonical number spelling: goldens at both thresholds, at the zeros and at the ends of
//! binary64, a comparison with Rust's own shortest spelling, and the read-back property.

use super::*;

/// Goldens: a value by its bits, and the bytes the canonical form writes. Each row was derived
/// outside this crate (the bits with Python's `struct`, the digits from Python's shortest
/// `repr`, its `e-05` and `e+16` exponents respelled by the rule in the module documentation),
/// so no row comes from the function under test. The comments give the values.
const REAL_GOLDENS: &[(u64, &str)] = &[
    // +0.0 and -0.0
    (0x0000_0000_0000_0000, "0.0"),
    (0x8000_0000_0000_0000, "-0.0"),
    // 1.0, -1.0, 60.0, 0.5, 0.1, 1/3
    (0x3ff0_0000_0000_0000, "1.0"),
    (0xbff0_0000_0000_0000, "-1.0"),
    (0x404e_0000_0000_0000, "60.0"),
    (0x3fe0_0000_0000_0000, "0.5"),
    (0x3fb9_9999_9999_999a, "0.1"),
    (0x3fd5_5555_5555_5555, "0.3333333333333333"),
    // The lower threshold: 1e-4 is plain decimal, the binary64 just below it is not.
    (0x3f1a_36e2_eb1c_432d, "0.0001"),
    (0x3f1a_36e2_eb1c_432c, "9.999999999999999e-5"),
    (0xbf1a_36e2_eb1c_432d, "-0.0001"),
    (0xbf1a_36e2_eb1c_432c, "-9.999999999999999e-5"),
    // 1e-5 and 1.5e-7
    (0x3ee4_f8b5_88e3_68f1, "1e-5"),
    (0x3e84_21f5_f40d_8376, "1.5e-7"),
    // The upper threshold: the binary64 just below 1e16 is plain decimal, 1e16 is not.
    (0x4341_c379_37e0_7fff, "9999999999999998.0"),
    (0x4341_c379_37e0_8000, "1e16"),
    (0xc341_c379_37e0_7fff, "-9999999999999998.0"),
    (0xc341_c379_37e0_8000, "-1e16"),
    // 1e15, 2^53, 2^53 + 2, 1.2345678901234568e17, 1e21, 1e22
    (0x430c_6bf5_2634_0000, "1000000000000000.0"),
    (0x4340_0000_0000_0000, "9007199254740992.0"),
    (0x4340_0000_0000_0001, "9007199254740994.0"),
    (0x437b_69b4_ba63_0f35, "1.2345678901234568e17"),
    (0x444b_1ae4_d6e2_ef50, "1e21"),
    (0x4480_f0cf_064d_d592, "1e22"),
    // The smallest subnormal, the largest subnormal, the smallest normal, the largest finite.
    (0x0000_0000_0000_0001, "5e-324"),
    (0x000f_ffff_ffff_ffff, "2.225073858507201e-308"),
    (0x0010_0000_0000_0000, "2.2250738585072014e-308"),
    (0x7fef_ffff_ffff_ffff, "1.7976931348623157e308"),
    (0xffef_ffff_ffff_ffff, "-1.7976931348623157e308"),
    (0x8000_0000_0000_0001, "-5e-324"),
    // The next two subnormals: several strings of the shortest length read back to each (see
    // `of_the_shortest_strings_that_read_back_the_closest_is_written`).
    (0x0000_0000_0000_0002, "1e-323"),
    (0x0000_0000_0000_0003, "1.5e-323"),
];

#[test]
fn a_real_is_written_as_its_golden_at_the_thresholds_the_zeros_and_the_ends() {
    for (bits, expected) in REAL_GOLDENS {
        let value = f64::from_bits(*bits);
        assert_eq!(canonical_real(value), *expected, "{bits:#018x}");
    }
}

#[test]
fn every_golden_reads_back_to_the_bits_it_was_written_from() {
    for (bits, spelling) in REAL_GOLDENS {
        let read: f64 = spelling.parse().expect("a golden is a number");
        assert_eq!(read.to_bits(), *bits, "{spelling}");
    }
}

#[test]
fn a_real_always_carries_a_fraction_or_an_exponent() {
    for (bits, _) in REAL_GOLDENS {
        let spelling = canonical_real(f64::from_bits(*bits));
        assert!(
            spelling.contains('.') || spelling.contains('e'),
            "{spelling} would read back as an integer token"
        );
        assert!(
            !spelling.contains('+') && !spelling.contains('E'),
            "{spelling}"
        );
        assert!(
            !spelling.contains("e0") && !spelling.contains("e-0"),
            "{spelling}"
        );
    }
}

#[test]
fn of_the_shortest_strings_that_read_back_the_closest_is_written() {
    // "Shortest digits that read back" is not one string. Each row: the bits of a subnormal,
    // every string of the shortest length that reads back to it, and the one the writer takes,
    // which is the one closest to the exact value (n * 2^-1074, worked out with exact rationals
    // outside this crate: 4.94e-324, 9.88e-324 and 1.482e-323).
    let rows: [(u64, &[&str], &str); 3] = [
        (
            1,
            &["3e-324", "4e-324", "5e-324", "6e-324", "7e-324"],
            "5e-324",
        ),
        (2, &["8e-324", "9e-324", "1e-323"], "1e-323"),
        (
            3,
            &["1.3e-323", "1.4e-323", "1.5e-323", "1.6e-323", "1.7e-323"],
            "1.5e-323",
        ),
    ];
    for (bits, candidates, written) in rows {
        for candidate in candidates {
            let read: f64 = candidate.parse().expect("a number");
            assert_eq!(read.to_bits(), bits, "{candidate} reads back");
        }
        assert!(candidates.contains(&written));
        assert_eq!(canonical_real(f64::from_bits(bits)), written);
    }
    // One step outside each list reads back to a neighbour, so the lists are whole.
    for (outside, bits) in [
        ("2e-324", 0),
        ("8e-324", 2),
        ("7e-324", 1),
        ("1.1e-323", 2),
        ("1.2e-323", 2),
        ("1.8e-323", 4),
    ] {
        let read: f64 = outside.parse().expect("a number");
        assert_eq!(read.to_bits(), bits, "{outside}");
    }
}

/// `text` read as a binary64, by its bits.
fn bits_of(text: &str) -> u64 {
    text.parse::<f64>().expect("a number").to_bits()
}

#[test]
fn the_digits_are_the_shortest_that_read_back_and_the_closest_of_those() {
    // Checked with exact integers, where it can be: a binary64 of magnitude 2^53 or more is an
    // integer `m * 2^shift`, so its distance to a decimal `digits * 10^power` is exact in `u128`.
    let mut ties_broken = 0_u32;
    let mut patterns = bit_patterns(6_000);
    for round in 0..6_000_u32 {
        let pattern = patterns.next().expect("a pattern");
        let mantissa = (1_u64 << 52) | (pattern >> 12);
        let shift = 1 + (round % 60);
        let exact = u128::from(mantissa) << shift;
        let value =
            f64::from_bits((u64::from(1023 + 52 + shift) << 52) | (mantissa & ((1 << 52) - 1)));
        let (digits, exponent) = shortest_digits(value);
        let power = u32::try_from(exponent).expect("positive") + 1
            - u32::try_from(digits.len()).expect("fits");
        let scale = 10_u128.pow(power);
        let written: u128 = digits.parse().expect("digits");
        let spell = |candidate: u128, power: u32| format!("{candidate}e{power}");
        assert_eq!(
            bits_of(&spell(written, power)),
            value.to_bits(),
            "{digits}e{power}"
        );
        // Closest: no other string of this length that reads back is nearer the exact value.
        let distance = (written * scale).abs_diff(exact);
        let (low, high) = (
            10_u128.pow(u32::try_from(digits.len()).expect("fits") - 1),
            10_u128.pow(u32::try_from(digits.len()).expect("fits")),
        );
        for other in (written.saturating_sub(30)..=written + 30).filter(|d| (low..high).contains(d))
        {
            if other != written && bits_of(&spell(other, power)) == value.to_bits() {
                assert!(
                    (other * scale).abs_diff(exact) >= distance,
                    "{other}e{power} reads back and is nearer than {written}e{power}"
                );
                ties_broken += 1;
            }
        }
        // Shortest: the two strings one digit shorter that bracket the value do not read back
        // (the strings that read back form one interval around it, so no shorter one does).
        if digits.len() > 1 {
            let below = exact / (scale * 10);
            for shorter in [below, below + 1] {
                assert_ne!(
                    bits_of(&spell(shorter, power + 1)),
                    value.to_bits(),
                    "{shorter}e{} is shorter than {digits}e{power} and reads back",
                    power + 1
                );
            }
        }
    }
    // Other strings of the same length did read back, often: the closest-of rule was exercised.
    assert!(ties_broken > 1_000, "{ties_broken}");
}

/// A deterministic walk over bit patterns: a 64-bit linear congruential step (Knuth's MMIX
/// constants), so the sweep is the same on every run and host.
fn bit_patterns(count: usize) -> impl Iterator<Item = u64> {
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    std::iter::repeat_with(move || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        state
    })
    .take(count)
}

/// Values worth asking about: every power of ten binary64 can hold, each with its neighbours,
/// and a sweep of arbitrary bit patterns.
fn probes() -> Vec<f64> {
    let mut values = Vec::new();
    for exponent in -323..=308 {
        let power: f64 = format!("1e{exponent}").parse().expect("a power of ten");
        for value in [power.next_down(), power, power.next_up()] {
            values.push(value);
            values.push(-value);
        }
    }
    values.extend(bit_patterns(20_000).map(f64::from_bits));
    values.retain(|value| value.is_finite());
    values
}

#[test]
fn the_spelling_agrees_with_rusts_shortest_debug_spelling() {
    // An oracle, not the implementation: `canonical_real` lays the digits out itself, and Rust's
    // `{:?}` for an `f64` follows the same rule (plain decimal for zero and for a magnitude in
    // [1e-4, 1e16), otherwise `d[.ddd]e[-]N`). If a toolchain ever moved that rule this test
    // would say so, and the goldens above would still pin this crate's bytes.
    let probes = probes();
    assert!(probes.len() > 20_000, "{}", probes.len());
    for value in probes {
        assert_eq!(
            canonical_real(value),
            format!("{value:?}"),
            "{:#018x}",
            value.to_bits()
        );
    }
}

#[test]
fn every_spelling_reads_back_to_the_same_bits() {
    for value in probes() {
        let spelling = canonical_real(value);
        let read: f64 = spelling.parse().expect("a canonical real is a number");
        assert_eq!(read.to_bits(), value.to_bits(), "{spelling}");
    }
}

#[test]
fn the_layout_changes_exactly_at_the_two_thresholds() {
    let low: f64 = 1e-4;
    let high: f64 = 1e16;
    assert!(!canonical_real(low).contains('e'));
    assert!(canonical_real(low.next_down()).contains('e'));
    assert!(canonical_real(high).contains('e'));
    assert!(!canonical_real(high.next_down()).contains('e'));
    // Inside the plain range nothing is written with an exponent.
    for value in [0.000_123, 1.0, 123.456, 1e15, 9.007_199_254_740_993e15] {
        assert!(!canonical_real(value).contains('e'), "{value}");
    }
}

#[test]
fn a_non_finite_real_has_no_spelling_and_is_written_null() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(canonical_real(value), "null");
    }
}

#[test]
fn an_integer_token_is_written_as_a_decimal_integer() {
    for (value, expected) in [
        (0_i64, "0"),
        (1, "1"),
        (-1, "-1"),
        (60, "60"),
        (9_007_199_254_740_993, "9007199254740993"),
        (i64::MAX, "9223372036854775807"),
        (i64::MIN, "-9223372036854775808"),
    ] {
        assert_eq!(Number::Integer(value).canonical(), expected);
        assert_eq!(Literal::Integer(value).canonical(), expected);
    }
    assert_eq!(Literal::Boolean(true).canonical(), "true");
    assert_eq!(Literal::Boolean(false).canonical(), "false");
    assert_eq!(Literal::Real(-0.0).canonical(), "-0.0");
}

#[test]
fn a_time_reads_negative_zero_as_zero_and_an_integer_exactly() {
    assert_eq!(Number::Real(-0.0).seconds().to_bits(), 0.0_f64.to_bits());
    assert_eq!(Number::Real(0.0).seconds().to_bits(), 0.0_f64.to_bits());
    assert_eq!(Number::Integer(0).seconds().to_bits(), 0.0_f64.to_bits());
    assert_eq!(Number::Real(-1.5).seconds().to_bits(), (-1.5_f64).to_bits());
    assert_eq!(
        Number::Integer(crate::MAX_TIME_INTEGER).seconds().to_bits(),
        0x4340_0000_0000_0000,
        "2^53 is exact"
    );
    assert_eq!(
        Number::Integer(-crate::MAX_TIME_INTEGER)
            .seconds()
            .to_bits(),
        0xc340_0000_0000_0000
    );
    // The written spelling keeps the sign a time drops.
    assert_eq!(Number::Real(-0.0).canonical(), "-0.0");
}

#[test]
fn values_compare_by_kind_and_a_real_by_its_bits() {
    assert_eq!(Literal::Real(0.0), Literal::Real(0.0));
    assert_ne!(Literal::Real(0.0), Literal::Real(-0.0));
    assert_ne!(Literal::Integer(1), Literal::Real(1.0));
    assert_ne!(Literal::Boolean(true), Literal::Integer(1));
    assert_eq!(Number::Integer(7), Number::Integer(7));
    assert_ne!(Number::Integer(0), Number::Real(0.0));
    assert_ne!(Number::Real(0.0), Number::Real(-0.0));
    assert_eq!(
        Literal::from(Number::Real(2.5)),
        Literal::Real(2.5),
        "a number keeps its kind as a value"
    );
    assert_eq!(Literal::from(Number::Integer(-3)), Literal::Integer(-3));
}
