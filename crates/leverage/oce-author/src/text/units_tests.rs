//! Unit suffixes: which suffixes are base units, which are scaled, and the exact scaling (issue
//! #501; vocabulary note, "Units" and decision 14). Every expected value is an exact oracle: an
//! integer the product equals, or the decimal it equals parsed by Rust's correctly rounded
//! parser, never the product of two `f64`s.

use crate::model::Literal;

use super::{SuffixKind, check_suffix, scale};
use crate::text::codes::{MALFORMED_SUFFIX, UNIT_NOT_BASE};

fn factor(symbol: &str) -> u32 {
    oce_vocab::scale_factor(symbol)
        .expect("a scaled symbol")
        .factor
}

fn bits(value: Option<f64>) -> Option<u64> {
    value.map(f64::to_bits)
}

#[test]
fn each_scaled_symbol_alone_is_scaled_and_base_units_are_not() {
    for row in oce_vocab::SCALE_FACTORS {
        assert_eq!(check_suffix(row.symbol), Ok(SuffixKind::Scaled(row)));
        assert_eq!(SuffixKind::Scaled(row).reduced(row.symbol), "s");
    }
    for base in ["s", "K", "H", "m/s2", "kg/m.s2", "1/s", "Hz", "W/m2"] {
        assert_eq!(check_suffix(base), Ok(SuffixKind::Base), "{base}");
        assert_eq!(SuffixKind::Base.reduced(base), base);
    }
}

#[test]
fn a_scaled_symbol_inside_a_compound_or_misspelled_is_not_a_base_unit() {
    for refused in [
        "m/h", "m3/h", "1/h", "h2", "h-1", "h.m", "min.s", "d2", "hh", "hr", "Min", "MIN", "day",
        "ms", "ks", "degC", "degF", "kPa",
    ] {
        let (code, detail) = check_suffix(refused).expect_err(refused);
        assert_eq!(code, UNIT_NOT_BASE, "{refused}: {detail}");
    }
    let (_, inside) = check_suffix("m3/h").expect_err("compound");
    assert!(inside.contains("only as the whole suffix"), "{inside}");
    let (_, other) = check_suffix("degC").expect_err("affine");
    assert!(other.contains("`min`, `h` and `d` alone"), "{other}");
    for malformed in ["%", "h/", "/h", "h0", "1", "h.", ".h"] {
        let (code, _) = check_suffix(malformed).expect_err(malformed);
        assert_eq!(code, MALFORMED_SUFFIX, "{malformed}");
    }
}

#[test]
fn integers_scale_by_their_exact_product() {
    for symbol in ["min", "h", "d"] {
        let k = i64::from(factor(symbol));
        for v in -1_000_i64..=1_000 {
            let exact = i32::try_from(v * k).expect("small");
            assert_eq!(
                bits(scale(&Literal::Integer(v), factor(symbol))),
                Some(f64::from(exact).to_bits()),
                "{v} {symbol}"
            );
        }
    }
    // `0 h` is +0.0: an Integer has no negative zero.
    assert_eq!(
        bits(scale(&Literal::Integer(0), 3_600)),
        Some(0.0_f64.to_bits())
    );
}

#[test]
fn a_real_scales_its_decimal_exactly_with_one_rounding() {
    // The note's example: the decimal 1.1 times 3600 is exactly 3960. Multiplying the parsed
    // `f64` is not exact, and that is the difference this rule exists for.
    assert_eq!(
        bits(scale(&Literal::Real(1.1), 3_600)),
        Some(3_960.0_f64.to_bits())
    );
    assert_ne!((1.1_f64 * 3_600.0).to_bits(), 3_960.0_f64.to_bits());
    let cases: &[(f64, &str, f64)] = &[
        (1.5, "min", 90.0),
        (2.0, "d", 172_800.0),
        (0.25, "h", 900.0),
        (2.5, "d", 216_000.0),
        (-1.5, "min", -90.0),
        (0.001, "h", 3.6),
        (1e-7, "h", 0.00036),
        (1e21, "h", 3.6e24),
        (0.1, "min", 6.0),
        (1.7, "min", 102.0),
        (123_456.789, "min", 7_407_407.34),
    ];
    for &(written, symbol, expected) in cases {
        assert_eq!(
            bits(scale(&Literal::Real(written), factor(symbol))),
            Some(expected.to_bits()),
            "{written:?} {symbol}"
        );
    }
}

#[test]
fn every_tenth_and_hundredth_scales_to_its_exact_integer() {
    // n/10 and n/100 are spelled with one or two decimals, so the product is an exact integer.
    for n in -20_000_i32..=20_000 {
        let tenth = f64::from(n) / 10.0;
        assert_eq!(
            bits(scale(&Literal::Real(tenth), 60)),
            Some(f64::from(n * 6).to_bits()),
            "{tenth:?} min"
        );
        let hundredth = f64::from(n) / 100.0;
        assert_eq!(
            bits(scale(&Literal::Real(hundredth), 3_600)),
            Some(f64::from(n * 36).to_bits()),
            "{hundredth:?} h"
        );
        assert_eq!(
            bits(scale(&Literal::Real(hundredth), 86_400)),
            Some(f64::from(n * 864).to_bits()),
            "{hundredth:?} d"
        );
    }
}

#[test]
fn a_literal_past_two_to_the_fifty_third_is_rounded_once() {
    // 9007199254740993 min is exactly 540431955284459580 s; its neighbours in binary64 are
    // ...520 and ...584, and ...584 is nearer. Converting the Integer to `f64` first would give
    // 2^53 minutes, ...520.
    assert_eq!(
        bits(scale(&Literal::Integer(9_007_199_254_740_993), 60)),
        Some(540_431_955_284_459_584.0_f64.to_bits())
    );
    assert_ne!(
        (9_007_199_254_740_993_i64 as f64 * 60.0).to_bits(),
        540_431_955_284_459_584.0_f64.to_bits()
    );
    // The extremes of `i64` in days: exact products, rounded once.
    let max: f64 = "796899343984252629724800".parse().expect("decimal");
    let min: f64 = "-796899343984252629811200".parse().expect("decimal");
    assert_eq!(
        bits(scale(&Literal::Integer(i64::MAX), 86_400)),
        Some(max.to_bits())
    );
    assert_eq!(
        bits(scale(&Literal::Integer(i64::MIN), 86_400)),
        Some(min.to_bits())
    );
}

#[test]
fn signs_zeros_subnormals_and_overflow() {
    assert_eq!(
        bits(scale(&Literal::Real(-0.0), 3_600)),
        Some((-0.0_f64).to_bits())
    );
    assert_eq!(
        bits(scale(&Literal::Real(0.0), 3_600)),
        Some(0.0_f64.to_bits())
    );
    // The smallest subnormal is spelled `5e-324`; that decimal is what is scaled.
    let tiny: f64 = "1.8e-320".parse().expect("decimal");
    assert_eq!(
        bits(scale(&Literal::Real(5e-324), 3_600)),
        Some(tiny.to_bits())
    );
    assert_eq!(
        bits(scale(&Literal::Real(f64::MIN_POSITIVE), 60)),
        // `2.2250738585072014e-308` times 60, digit for digit.
        Some(("1.33504431510432084e-306".parse::<f64>().expect("decimal")).to_bits())
    );
    // Past binary64: refused, never infinite.
    assert_eq!(scale(&Literal::Real(f64::MAX), 60), None);
    assert_eq!(scale(&Literal::Real(1e308), 60), None);
    assert_eq!(scale(&Literal::Real(-1e308), 86_400), None);
    assert!(scale(&Literal::Real(2e303), 86_400).is_some());
    for non_finite in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(scale(&Literal::Real(non_finite), 60), None);
    }
    for other in [
        Literal::Boolean(true),
        Literal::Text("1".to_owned()),
        Literal::Member(crate::model::MemberRef::parse("SimpleController.PI")),
    ] {
        assert_eq!(scale(&other, 60), None);
    }
}

#[test]
fn scaling_is_the_same_for_every_spelling_of_one_value() {
    // The decimal scaled is the shortest round-trip spelling, which is what `fmt` prints, so a
    // value scales the same whichever spelling the author wrote.
    for (spelling, expected) in [("1.10", 3_960.0), ("1.1e0", 3_960.0), ("11e-1", 3_960.0)] {
        let value: f64 = spelling.parse().expect("decimal");
        assert_eq!(
            bits(scale(&Literal::Real(value), 3_600)),
            Some(f64::to_bits(expected)),
            "{spelling}"
        );
    }
}
