//! The `%.16g` writer against three references: literals worked out by hand at every place the
//! format branches, glibc's own `snprintf` over 4,792 doubles (captured with the funnel
//! fixture), and an emulation built on the standard library's exact `{:.15e}` over sixty
//! thousand more.

use super::*;

/// glibc's `snprintf("%.16g")` of a fixed list of doubles: one `<16 hex digits of the bits>`,
/// a tab, and the text per line. Written by `tests/fixtures/funnel/capture/capture.py` on
/// x86_64 Linux, never by this crate.
const GLIBC: &str = include_str!("../tests/fixtures/funnel/printf/glibc.tsv");

fn glibc_rows() -> impl Iterator<Item = (f64, &'static str)> {
    GLIBC.lines().map(|line| {
        let (bits, text) = line.split_once('\t').expect("a `bits<TAB>text` row");
        let bits = u64::from_str_radix(bits, 16).expect("sixteen hex digits");
        (f64::from_bits(bits), text)
    })
}

/// `%.16g` assembled from the standard library's `{:.15e}`, which rounds the exact value to
/// sixteen digits: a second implementation that shares no code with the writer.
fn by_the_standard_library(value: f64) -> String {
    if value.is_nan() {
        return if value.is_sign_negative() {
            "-nan"
        } else {
            "nan"
        }
        .to_owned();
    }
    if value.is_infinite() {
        return if value < 0.0 { "-inf" } else { "inf" }.to_owned();
    }
    let sign = if value.is_sign_negative() { "-" } else { "" };
    if value == 0.0 {
        return format!("{sign}0");
    }
    let scientific = format!("{:.15e}", value.abs());
    let (mantissa, exponent) = scientific.split_once('e').expect("an exponent");
    let exponent: i32 = exponent.parse().expect("a decimal exponent");
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let strip = |text: &str| -> String {
        if text.contains('.') {
            text.trim_end_matches('0').trim_end_matches('.').to_owned()
        } else {
            text.to_owned()
        }
    };
    if (-4..16).contains(&exponent) {
        let fixed = if exponent < 0 {
            format!(
                "0.{}{digits}",
                "0".repeat(exponent.unsigned_abs() as usize - 1)
            )
        } else {
            let (integer, fraction) = digits.split_at(exponent.unsigned_abs() as usize + 1);
            format!("{integer}.{fraction}")
        };
        format!("{sign}{}", strip(&fixed))
    } else {
        let mantissa = strip(&format!("{}.{}", &digits[..1], &digits[1..]));
        let mark = if exponent < 0 { '-' } else { '+' };
        format!("{sign}{mantissa}e{mark}{:02}", exponent.unsigned_abs())
    }
}

#[test]
fn zero_keeps_its_sign_and_nothing_else() {
    assert_eq!(format_g16(0.0), "0");
    assert_eq!(format_g16(-0.0), "-0");
}

#[test]
fn the_switch_to_an_exponent_is_below_1e_minus_4() {
    assert_eq!(format_g16(0.0001), "0.0001");
    assert_eq!(
        format_g16(0.000_123_456_789_012_345_67),
        "0.0001234567890123457"
    );
    // The double just below 1e-4 has the decimal exponent -5.
    assert_eq!(
        format_g16(f64::from_bits(0.0001_f64.to_bits() - 1)),
        "9.999999999999999e-05"
    );
    assert_eq!(format_g16(0.00001), "1e-05");
    assert_eq!(format_g16(0.000_095), "9.500000000000001e-05");
    assert_eq!(format_g16(0.000_015), "1.5e-05");
    assert_eq!(format_g16(1e-7), "1e-07");
    assert_eq!(format_g16(-2.5e-7), "-2.5e-07");
}

#[test]
fn the_switch_to_an_exponent_is_at_1e16() {
    assert_eq!(format_g16(1e15), "1000000000000000");
    assert_eq!(format_g16(999_999_999_999_999.9), "999999999999999.9");
    assert_eq!(format_g16(9_999_999_999_999_998.0), "9999999999999998");
    assert_eq!(format_g16(1e16), "1e+16");
    // The next double, 10000000000000002, has seventeen digits: its last one is rounded away.
    assert_eq!(format_g16(10_000_000_000_000_002.0), "1e+16");
    assert_eq!(
        format_g16(1.234_567_890_123_456_8e17),
        "1.234567890123457e+17"
    );
    assert_eq!(format_g16(9_007_199_254_740_992.0), "9007199254740992");
    assert_eq!(
        format_g16(1_152_921_504_606_846_976.0),
        "1.152921504606847e+18"
    );
}

#[test]
fn a_seventeenth_digit_of_exactly_five_rounds_to_the_even_sixteenth() {
    // Halves of odd integers below 2^53 are doubles with seventeen digits ending in 5.
    assert_eq!(format_g16(1_234_567_890_123_456.5), "1234567890123456");
    assert_eq!(format_g16(1_234_567_890_123_457.5), "1234567890123458");
    assert_eq!(format_g16(2_251_799_813_685_247.5), "2251799813685248");
    assert_eq!(format_g16(4_503_599_627_370_495.5), "4503599627370496");
    // 2^-24 = 5.9604644775390625e-08, a tie below one.
    assert_eq!(format_g16(1.0 / 16_777_216.0), "5.960464477539062e-08");
    // Not ties: a quarter rounds down, three quarters up.
    assert_eq!(
        format_g16(1_234_567_890_123_456.0 + 0.25),
        "1234567890123456"
    );
    assert_eq!(
        format_g16(1_234_567_890_123_456.0 + 0.75),
        "1234567890123457"
    );
}

#[test]
fn seventeen_digit_values_lose_their_last_digit() {
    assert_eq!(format_g16(0.1 + 0.2), "0.3");
    assert_eq!(format_g16(0.1), "0.1");
    assert_eq!(format_g16(1.0 / 3.0), "0.3333333333333333");
    assert_eq!(format_g16(2.0 / 3.0), "0.6666666666666666");
    assert_eq!(format_g16(0.799_999_999_999_999_9), "0.7999999999999999");
    // The double nearest 1e23 is below it, and sixteen digits show that.
    assert_eq!(format_g16(1e23), "9.999999999999999e+22");
    assert_eq!(format_g16(1e22), "1e+22");
    assert_eq!(format_g16(8.41e21), "8.409999999999999e+21");
}

#[test]
fn rounding_can_carry_through_every_digit_and_move_the_exponent() {
    // The double nearest 1e-7 is 9.99999999999999954748…e-08: sixteen nines, then more than a
    // half. The carry runs through all sixteen digits and the exponent goes from -8 to -7.
    let below = 1e-7_f64;
    assert_eq!(format!("{below:.16e}"), "9.9999999999999995e-8");
    assert_eq!(format_g16(below), "1e-07");
    // 1 - 2^-53 is 0.99999999999999988897…: the sixteenth digit rounds up to a nine, no carry.
    assert_eq!(
        format_g16(f64::from_bits(1.0_f64.to_bits() - 1)),
        "0.9999999999999999"
    );
    // 1 + 2^-52 is 1.00000000000000022204…: the digits after the sixteenth round away.
    assert_eq!(format_g16(f64::from_bits(1.0_f64.to_bits() + 1)), "1");
    assert_eq!(
        format_g16(f64::from_bits(10.0_f64.to_bits() - 1)),
        "9.999999999999998"
    );
}

#[test]
fn every_power_of_ten_prints_as_the_double_it_parses_to() {
    // Most powers of ten are not doubles; whether the nearest double prints as `1e±XX` or as
    // nines is the rounding's decision, and in the subnormals too few bits are left for either.
    let (mut exact_looking, mut other) = (0, 0);
    for exponent in -320..=308 {
        let power: f64 = format!("1e{exponent}").parse().expect("a power of ten");
        let text = format_g16(power);
        assert_eq!(text, by_the_standard_library(power), "1e{exponent}");
        if text.trim_end_matches('0') == "1" || text.starts_with("1e") || text.starts_with("0.") {
            exact_looking += 1;
        } else {
            other += 1;
        }
    }
    assert_eq!(exact_looking + other, 629);
    assert_eq!(format_g16(1e23), "9.999999999999999e+22");
    assert_eq!(format_g16(1e-320), "9.99988867182683e-321");
    assert_eq!(format_g16(1e-317), "1.000000230692537e-317");
    assert!(other >= 2);
}

#[test]
fn subnormals_and_the_extremes_are_exact() {
    assert_eq!(format_g16(5e-324), "4.940656458412465e-324");
    assert_eq!(format_g16(1e-310), "9.999999999999969e-311");
    assert_eq!(format_g16(f64::MIN_POSITIVE), "2.225073858507201e-308");
    assert_eq!(
        format_g16(f64::from_bits(f64::MIN_POSITIVE.to_bits() - 1)),
        "2.225073858507201e-308"
    );
    assert_eq!(format_g16(f64::MAX), "1.797693134862316e+308");
    assert_eq!(format_g16(f64::MIN), "-1.797693134862316e+308");
}

#[test]
fn an_exponent_has_at_least_two_digits_and_three_when_it_needs_them() {
    assert_eq!(format_g16(1e-5), "1e-05");
    assert_eq!(format_g16(1e99), "1e+99");
    assert_eq!(format_g16(1e100), "1e+100");
    assert_eq!(format_g16(1.5e300), "1.5e+300");
    assert_eq!(format_g16(1e-100), "1e-100");
}

#[test]
fn the_non_finite_values_are_spelled_as_glibc_spells_them() {
    assert_eq!(format_g16(f64::INFINITY), "inf");
    assert_eq!(format_g16(f64::NEG_INFINITY), "-inf");
    assert_eq!(format_g16(f64::from_bits(0x7ff8_0000_0000_0000)), "nan");
    assert_eq!(format_g16(f64::from_bits(0xfff8_0000_0000_0000)), "-nan");
    // The payload and the signalling bit do not show.
    assert_eq!(format_g16(f64::from_bits(0x7ff0_0000_0000_0001)), "nan");
    assert_eq!(format_g16(f64::from_bits(0xffff_ffff_ffff_ffff)), "-nan");
}

#[test]
fn ordinary_values_read_as_written() {
    for (value, text) in [
        (1.0, "1"),
        (-1.0, "-1"),
        (100.0, "100"),
        (0.5, "0.5"),
        (123_456.789, "123456.789"),
        (54_000.0, "54000"),
        (3_700_000_030.72, "3700000030.72"),
        (1.7e12, "1700000000000"),
        (2.675, "2.675"),
        (0.008_935_670_100_205_906, "0.008935670100205906"),
    ] {
        assert_eq!(format_g16(value), text);
    }
}

#[test]
fn every_row_of_the_glibc_capture_is_reproduced() {
    let mut rows = 0;
    let (mut exponential, mut fixed, mut special) = (0, 0, 0);
    for (value, text) in glibc_rows() {
        assert_eq!(format_g16(value), text, "bits {:016x}", value.to_bits());
        rows += 1;
        if !value.is_finite() {
            special += 1;
        } else if text.contains('e') {
            exponential += 1;
        } else {
            fixed += 1;
        }
    }
    // Pinned, so a capture that lost rows, or one kind of row, fails here.
    assert_eq!(rows, 4_792);
    assert_eq!((exponential, fixed, special), (4_587, 200, 5));
}

#[test]
fn the_glibc_capture_holds_the_landmarks_the_writer_is_pinned_at() {
    let rows: Vec<(u64, &str)> = glibc_rows()
        .map(|(value, text)| (value.to_bits(), text))
        .collect();
    for (value, text) in [
        (-0.0, "-0"),
        (1e-5, "1e-05"),
        (0.0001, "0.0001"),
        (1e16, "1e+16"),
        (9_999_999_999_999_998.0, "9999999999999998"),
        (5e-324, "4.940656458412465e-324"),
        (1_234_567_890_123_456.5, "1234567890123456"),
        (1_234_567_890_123_457.5, "1234567890123458"),
        (0.1 + 0.2, "0.3"),
        (f64::NEG_INFINITY, "-inf"),
    ] {
        assert!(
            rows.contains(&(f64::to_bits(value), text)),
            "glibc printed {value:e} as {text}"
        );
    }
}

#[test]
fn the_writer_agrees_with_the_standard_librarys_exact_rounding_on_sixty_thousand_doubles() {
    // SplitMix64 over all bit patterns: every binade, subnormals and non-finite values.
    let mut state = 655_u64;
    let mut next = || {
        state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    };
    for _ in 0..30_000 {
        let bits = next();
        let value = f64::from_bits(bits);
        assert_eq!(
            format_g16(value),
            by_the_standard_library(value),
            "bits {bits:016x}"
        );
        // And near one, where fixed notation and short expansions live.
        let near_one = f64::from_bits(0x3ff0_0000_0000_0000 | (bits >> 12));
        let scaled = near_one * [1e-6, 1e-4, 1.0, 1e3, 1e15, 1e17][(bits % 6) as usize];
        assert_eq!(format_g16(scaled), by_the_standard_library(scaled));
    }
}

#[test]
fn the_text_never_holds_more_than_sixteen_significant_digits_or_a_trailing_zero() {
    for (value, _) in glibc_rows().filter(|(value, _)| value.is_finite() && *value != 0.0) {
        let text = format_g16(value);
        let mantissa = text.split('e').next().expect("a mantissa");
        let digits = mantissa
            .trim_start_matches('-')
            .trim_start_matches("0.")
            .trim_start_matches('0');
        assert!(
            digits.chars().filter(char::is_ascii_digit).count() <= 16,
            "{text}"
        );
        if mantissa.contains('.') {
            assert!(
                !mantissa.ends_with('0') && !mantissa.ends_with('.'),
                "{text}"
            );
        }
    }
}

#[test]
fn the_exact_expansion_is_every_decimal_digit_of_the_double() {
    // The significant digits without trailing zeros, and the decimal exponent of the first.
    let expansion = |value: f64| -> (String, i32) {
        let (digits, scale) = exact_expansion(value);
        let exponent = digits.len() as i32 - 1 - scale;
        let text: String = digits
            .iter()
            .map(|digit| char::from(b'0' + digit))
            .collect();
        (text.trim_end_matches('0').to_owned(), exponent)
    };
    assert_eq!(expansion(1.0), ("1".to_owned(), 0));
    assert_eq!(expansion(0.5), ("5".to_owned(), -1));
    assert_eq!(expansion(1024.0), ("1024".to_owned(), 3));
    assert_eq!(
        expansion(0.1),
        (
            "1000000000000000055511151231257827021181583404541015625".to_owned(),
            -1
        )
    );
    // 2^70, an integer of 22 digits that crosses limbs.
    assert_eq!(
        expansion(1_180_591_620_717_411_303_424.0),
        ("1180591620717411303424".to_owned(), 21)
    );
    // The smallest subnormal, 2^-1074 = 5^1074 / 10^1074: 751 digits.
    let (smallest, exponent) = expansion(5e-324);
    assert_eq!(exponent, -324);
    assert_eq!(smallest.len(), 751);
    assert!(smallest.starts_with("4940656458412465441765687928682213723650598026143247"));
    assert!(smallest.ends_with("625"));
    // The largest double, (2^53 - 1) × 2^971: 309 digits, an integer.
    let (largest, exponent) = expansion(f64::MAX);
    assert_eq!(exponent, 308);
    assert!(
        largest
            .starts_with("17976931348623157081452742373170435679807056752584499659891747680315726")
    );
    assert_eq!(exact_expansion(f64::MAX).0.len(), 309);
}
