//! The closed unit table `units/v1` against oracles that share nothing with it (issue #659).
//!
//! - **The bit goldens** (`fixtures/units/units_v1.txt`) are written by
//!   `fixtures/units/derive_units.py`, which evaluates each pair in the design note's order with
//!   Python's binary64 and checks every constant against its base definitions in exact rational
//!   arithmetic; no test runs it and nothing blesses its output.
//! - **The constants** are derived again here from the definitions (the inch, the foot, standard
//!   gravity, the litre, the kelvin offset), as integer ratios reduced and divided once, so each
//!   is the correctly rounded value of its definition.
//! - **The behaviour around the arithmetic**: the table is closed and one-way, spellings are
//!   compared byte for byte, every pair is monotone, and the same call gives the same bits.

use std::collections::BTreeSet;

use oce_verify::{UNIT_ERROR_CODES, UNITS_TABLE, UnitError, convert_unit};

const GOLDEN: &str = include_str!("fixtures/units/units_v1.txt");

/// The eleven pairs of `units/v1`, device unit first, in the design note's order.
const PAIRS: [(&str, &str); 11] = [
    ("degF", "K"),
    ("degC", "K"),
    ("degF", "degC"),
    ("K", "degC"),
    ("kPa", "Pa"),
    ("inH2O", "Pa"),
    ("inH2O_60F", "Pa"),
    ("cfm", "m3/s"),
    ("L/s", "m3/s"),
    ("percent", "1"),
    ("%", "1"),
];

/// One golden row.
struct Row {
    from: String,
    to: String,
    input: f64,
    /// `None` when the reading is refused as `overflow`.
    output: Option<u64>,
    /// The exact conversion rounded once, and its distance in ulps, for a finite result.
    exact: Option<(u64, u64)>,
}

fn hex(field: &str) -> u64 {
    u64::from_str_radix(field.strip_prefix("0x").expect("0x"), 16).expect("hex bits")
}

fn golden() -> Vec<Row> {
    GOLDEN
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            assert_eq!(fields.len(), 6, "{line}");
            Row {
                from: fields[0].to_owned(),
                to: fields[1].to_owned(),
                input: f64::from_bits(hex(fields[2])),
                output: (fields[3] != "overflow").then(|| hex(fields[3])),
                exact: (fields[4] != "-")
                    .then(|| (hex(fields[4]), fields[5].parse().expect("ulps"))),
            }
        })
        .collect()
}

/// The distance between two finite values in units in the last place.
fn ulps_apart(a: f64, b: f64) -> u64 {
    let ordered = |v: f64| {
        let bits = v.to_bits();
        if bits >> 63 == 1 {
            -i128::from(bits & 0x7fff_ffff_ffff_ffff)
        } else {
            i128::from(bits)
        }
    };
    u64::try_from((ordered(a) - ordered(b)).abs()).expect("fits")
}

#[test]
fn every_golden_row_is_returned_bit_for_bit_twice() {
    let rows = golden();
    for row in &rows {
        for _ in 0..2 {
            let got = convert_unit(&row.from, &row.to, row.input);
            match (row.output, got) {
                (Some(bits), Ok(value)) => assert_eq!(
                    value.to_bits(),
                    bits,
                    "{} -> {} at {:#018x}: {value:?}",
                    row.from,
                    row.to,
                    row.input.to_bits()
                ),
                (None, Err(error)) => assert_eq!(error.code(), "overflow"),
                (want, got) => panic!(
                    "{} -> {} at {:#018x}: {got:?}, expected {want:?}",
                    row.from,
                    row.to,
                    row.input.to_bits()
                ),
            }
        }
    }
}

#[test]
fn the_golden_covers_every_listed_pair_at_every_awkward_value() {
    let rows = golden();
    assert_eq!(
        rows.len(),
        404,
        "a row was added or lost: re-derive and review"
    );
    let pairs: BTreeSet<(&str, &str)> = rows
        .iter()
        .map(|row| (row.from.as_str(), row.to.as_str()))
        .collect();
    assert_eq!(pairs, PAIRS.iter().copied().collect());
    let awkward: [u64; 14] = [
        0.0_f64.to_bits(),
        (-0.0_f64).to_bits(),
        f64::MAX.to_bits(),
        f64::MIN.to_bits(),
        1,
        0x8000_0000_0000_0001,
        f64::MIN_POSITIVE.to_bits(),
        f64::INFINITY.to_bits(),
        f64::NEG_INFINITY.to_bits(),
        0x7ff8_0000_0000_0000,
        0x7ff8_0000_0000_0001,
        0x7ff0_0000_0000_0001,
        0xfff8_0000_0000_0000,
        0xfff4_0000_0000_0000,
    ];
    for (from, to) in PAIRS {
        for bits in awkward {
            assert!(
                rows.iter()
                    .any(|r| r.from == from && r.to == to && r.input.to_bits() == bits),
                "{from} -> {to} has no row at {bits:#018x}"
            );
        }
    }
}

#[test]
fn the_exact_column_is_the_true_distance_and_a_scaled_pair_is_within_one_ulp() {
    for row in golden() {
        let (Some(output), Some((exact, ulps))) = (row.output, row.exact) else {
            continue;
        };
        let distance = ulps_apart(f64::from_bits(output), f64::from_bits(exact));
        assert_eq!(distance, ulps, "{} -> {}", row.from, row.to);
        // One multiplication or division by a correctly rounded constant: at most one ulp from
        // the definition. The temperature pairs add or subtract an offset, and near the offset
        // cancellation makes any count of ulps possible, so only their bits are held.
        let temperature = row.to == "K" || row.to == "degC";
        if !temperature {
            assert!(
                distance <= 1,
                "{} -> {} at {:?}: {distance} ulps",
                row.from,
                row.to,
                row.input
            );
        }
    }
}

/// `numerator / denominator` reduced, then divided once in `f64`: the correctly rounded value,
/// since both terms are exact below 2^53 and one IEEE division rounds once.
fn rounded_once(numerator: u128, denominator: u128) -> f64 {
    fn gcd(a: u128, b: u128) -> u128 {
        if b == 0 { a } else { gcd(b, a % b) }
    }
    let divisor = gcd(numerator, denominator);
    let (n, d) = (numerator / divisor, denominator / divisor);
    let limit = 1_u128 << 53;
    assert!(n < limit && d < limit, "{n}/{d} is not exact in f64");
    // Both terms are below 2^53, so each is exact in `f64`.
    let (n, d) = (n as f64, d as f64);
    n / d
}

#[test]
fn each_factor_is_its_definition_rounded_once() {
    // NIST SP 811 B.8: the inch is 0.0254 m and the foot 0.3048 m exactly; NIST SP 330: standard
    // gravity is 9.80665 m/s^2 exactly; the conventional inch of water takes 1000 kg/m^3.
    let (inch, inch_scale) = (254_u128, 10_000_u128);
    let (foot, foot_scale) = (3048_u128, 10_000_u128);
    let (gravity, gravity_scale) = (980_665_u128, 100_000_u128);
    let water_density = 1000_u128;
    let inch_of_water = rounded_once(inch * water_density * gravity, inch_scale * gravity_scale);
    let cfm = rounded_once(
        foot * foot * foot,
        foot_scale * foot_scale * foot_scale * 60,
    );
    let expected = [
        ("kPa", "Pa", rounded_once(1000, 1)),
        ("inH2O", "Pa", inch_of_water),
        ("cfm", "m3/s", cfm),
        ("L/s", "m3/s", rounded_once(1, 1000)),
        ("percent", "1", rounded_once(1, 100)),
        ("%", "1", rounded_once(1, 100)),
    ];
    for (from, to, factor) in expected {
        let got = convert_unit(from, to, 1.0).expect("listed");
        assert_eq!(got.to_bits(), factor.to_bits(), "{from} -> {to}: {got:?}");
    }
    // The decimals as written: the conventional inch of water is exactly 249.08891 Pa, and a
    // cubic foot a minute exactly 0.0004719474432 m^3/s.
    assert_eq!(
        inch_of_water.to_bits(),
        rounded_once(24_908_891, 100_000).to_bits()
    );
    assert_eq!(
        cfm.to_bits(),
        rounded_once(4_719_474_432, 10_000_000_000_000).to_bits()
    );
}

#[test]
fn the_two_inches_of_water_are_two_units() {
    let conventional = convert_unit("inH2O", "Pa", 1.0).expect("listed");
    let at_60f = convert_unit("inH2O_60F", "Pa", 1.0).expect("listed");
    // NIST SP 811 B.8 prints 2.490 889 E+02 and 2.4884 E+02.
    assert!((conventional - 249.0889).abs() < 0.000_05, "{conventional}");
    assert_eq!(at_60f.to_bits(), rounded_once(24_884, 100).to_bits());
    // Water at 60 degF is about 999.0 kg/m^3 (ASHRAE Handbook - Fundamentals), about one part
    // in a thousand lighter than the conventional column: the tabulated value is that inch.
    let derived = 0.0254 * 999.0 * 9.806_65;
    assert!(
        (at_60f - derived).abs() < 0.01,
        "{at_60f} against {derived}"
    );
    assert!(conventional > at_60f);
    assert!(conventional / at_60f > 1.000_9 && conventional / at_60f < 1.001_1);
    // Neither is read as the other.
    for (from, to) in [("inH2O", "inH2O_60F"), ("inH2O_60F", "inH2O")] {
        assert_eq!(
            convert_unit(from, to, 1.0).unwrap_err().code(),
            "unknown-pair"
        );
    }
}

#[test]
fn the_known_temperatures_land_on_their_decimal_values() {
    let k = |from: &str, x: f64| convert_unit(from, "K", x).expect("listed");
    let c = |from: &str, x: f64| convert_unit(from, "degC", x).expect("listed");
    // 32 degF and 0 degC are 273.15 K, and 212 degF and 100 degC are 373.15 K, each the
    // correctly rounded decimal.
    let freezing = rounded_once(27_315, 100);
    let boiling = rounded_once(37_315, 100);
    assert_eq!(k("degF", 32.0).to_bits(), freezing.to_bits());
    assert_eq!(k("degC", 0.0).to_bits(), freezing.to_bits());
    assert_eq!(k("degC", -0.0).to_bits(), freezing.to_bits());
    assert_eq!(k("degF", 212.0).to_bits(), boiling.to_bits());
    assert_eq!(k("degC", 100.0).to_bits(), boiling.to_bits());
    // -40 is where the two scales meet: exactly, and the same kelvin either way.
    assert_eq!(c("degF", -40.0).to_bits(), (-40.0_f64).to_bits());
    assert_eq!(k("degF", -40.0).to_bits(), k("degC", -40.0).to_bits());
    assert_eq!(c("degF", 32.0).to_bits(), 0.0_f64.to_bits());
    assert_eq!(c("degF", 212.0).to_bits(), 100.0_f64.to_bits());
    assert_eq!(c("K", freezing).to_bits(), 0.0_f64.to_bits());
    assert_eq!(c("K", 0.0).to_bits(), (-freezing).to_bits());
    // Absolute zero written in degF is not exactly zero kelvin: -459.67 has no exact binary
    // form, and the note's order leaves -2^-44 K. Stated, not corrected.
    assert_eq!(
        k("degF", -459.67).to_bits(),
        (-(2.0_f64.powi(-44))).to_bits()
    );
}

#[test]
fn the_known_flows_pressures_and_fractions_land_on_their_values() {
    assert_eq!(convert_unit("%", "1", 100.0), Ok(1.0));
    assert_eq!(convert_unit("percent", "1", 100.0), Ok(1.0));
    assert_eq!(convert_unit("percent", "1", 50.0), Ok(0.5));
    assert_eq!(
        convert_unit("%", "1", -0.0).map(f64::to_bits),
        Ok((-0.0_f64).to_bits())
    );
    assert_eq!(convert_unit("L/s", "m3/s", 1000.0), Ok(1.0));
    assert_eq!(convert_unit("kPa", "Pa", 101.325), Ok(101_325.0));
    assert_eq!(
        convert_unit("kPa", "Pa", -0.0).map(f64::to_bits),
        Ok((-0.0_f64).to_bits())
    );
    assert_eq!(convert_unit("inH2O", "Pa", 2.0), Ok(2.0 * 249.088_91));
    assert_eq!(convert_unit("cfm", "m3/s", 0.0), Ok(0.0));
    // The smallest subnormal scaled below one underflows to zero of its sign; it is not refused.
    let tiny = f64::from_bits(1);
    assert_eq!(convert_unit("L/s", "m3/s", tiny).map(f64::to_bits), Ok(0));
    assert_eq!(
        convert_unit("%", "1", -tiny).map(f64::to_bits),
        Ok((-0.0_f64).to_bits())
    );
}

#[test]
fn the_table_is_closed_and_runs_one_way() {
    let spellings: BTreeSet<&str> = PAIRS.iter().flat_map(|(from, to)| [*from, *to]).collect();
    assert_eq!(spellings.len(), 13);
    let mut listed = 0;
    for from in &spellings {
        for to in &spellings {
            let got = convert_unit(from, to, 72.5);
            if from == to {
                assert_eq!(got, Ok(72.5), "{from} -> {to}");
            } else if PAIRS.contains(&(*from, *to)) {
                assert!(got.is_ok(), "{from} -> {to}");
                listed += 1;
            } else {
                assert_eq!(
                    got,
                    Err(UnitError::UnknownPair {
                        from: (*from).to_owned(),
                        to: (*to).to_owned(),
                    }),
                    "{from} -> {to}"
                );
            }
        }
    }
    assert_eq!(listed, PAIRS.len());
}

#[test]
fn a_near_miss_spelling_is_an_unknown_pair_never_a_conversion() {
    let near_misses: [(&str, &str); 24] = [
        ("degf", "K"),
        ("DEGF", "K"),
        ("°F", "K"),
        ("\u{2109}", "K"),
        ("F", "K"),
        ("deg F", "K"),
        (" degF", "K"),
        ("degF ", "K"),
        ("degF\u{200b}", "K"),
        ("\u{feff}degF", "K"),
        ("degF", "k"),
        ("degF", "Kelvin"),
        ("degF", "\u{212a}"),
        ("degC", "°C"),
        ("kpa", "Pa"),
        ("kPa", "pa"),
        ("in H2O", "Pa"),
        ("inH2O_60f", "Pa"),
        ("inH2O60F", "Pa"),
        ("CFM", "m3/s"),
        ("cfm", "m³/s"),
        ("l/s", "m3/s"),
        ("\u{ff05}", "1"),
        ("Percent", "1"),
    ];
    for (from, to) in near_misses {
        let refused = convert_unit(from, to, 32.0).unwrap_err();
        assert_eq!(refused.code(), "unknown-pair", "{from:?} -> {to:?}");
        let message = refused.to_string();
        assert!(message.is_ascii(), "{message}");
        assert!(!message.contains('\u{200b}') && !message.contains('\u{feff}'));
    }
}

#[test]
fn a_spelling_in_another_case_is_not_the_same_unit() {
    // `k` is not `K` and `pa` is not `Pa`: the identity is byte for byte, never case-folded, so
    // a device unit that differs from the CDL one only in case is refused, not passed through.
    let cases: [(&str, &str); 8] = [
        ("k", "K"),
        ("pa", "Pa"),
        ("PA", "Pa"),
        ("DEGC", "degC"),
        ("degc", "degC"),
        ("M3/S", "m3/s"),
        ("DegF", "degF"),
        ("PERCENT", "percent"),
    ];
    for (from, to) in cases {
        assert_eq!(
            convert_unit(from, to, 72.5),
            Err(UnitError::UnknownPair {
                from: from.to_owned(),
                to: to.to_owned(),
            }),
            "{from} -> {to}"
        );
    }
}

#[test]
fn a_megabyte_unit_is_refused_with_a_bounded_message() {
    let huge = "degF".repeat(1 << 18);
    for (from, to) in [(huge.as_str(), "K"), ("degF", huge.as_str())] {
        let refused = convert_unit(from, to, 32.0).unwrap_err();
        assert_eq!(refused.code(), "unknown-pair");
        assert!(
            refused.to_string().len() <= 200,
            "{}",
            refused.to_string().len()
        );
    }
}

/// A deterministic stream of 64-bit words (splitmix64), so a sweep needs no dependency and is the
/// same on every run.
fn words(seed: u64) -> impl Iterator<Item = u64> {
    let mut state = seed;
    std::iter::repeat_with(move || {
        state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    })
}

/// Finite readings over every magnitude, with the trend-sized ones dense, sorted ascending.
fn sweep() -> Vec<f64> {
    let mut values: Vec<f64> = words(659)
        .take(20_000)
        .map(f64::from_bits)
        .filter(|v| v.is_finite())
        .collect();
    values.extend(words(660).take(20_000).map(|w| {
        let unit = (w >> 11) as f64 / (1_u64 << 53) as f64;
        unit * 2000.0 - 1000.0
    }));
    values.extend([
        0.0,
        -0.0,
        32.0,
        -40.0,
        273.15,
        f64::MAX,
        f64::MIN,
        f64::from_bits(1),
    ]);
    values.sort_by(f64::total_cmp);
    values
}

#[test]
fn every_pair_is_monotone_over_every_magnitude() {
    let values = sweep();
    for (from, to) in PAIRS {
        let mut previous: Option<(f64, f64)> = None;
        for &x in &values {
            let Ok(y) = convert_unit(from, to, x) else {
                continue;
            };
            if let Some((px, py)) = previous {
                assert!(
                    py <= y,
                    "{from} -> {to}: {px:?} gives {py:?}, {x:?} gives {y:?}"
                );
            }
            previous = Some((x, y));
        }
    }
}

#[test]
fn every_pair_is_strictly_increasing_across_neighbouring_trend_readings() {
    // Around the values a building trends, the next representable reading never converts to a
    // smaller value, and a step of a thousandth of a unit always moves the result.
    for (from, to) in PAIRS {
        for x in [-40.0, 0.0, 32.0, 55.0, 72.5, 101.325, 400.0] {
            let here = convert_unit(from, to, x).expect("listed");
            let next = convert_unit(from, to, x.next_up()).expect("listed");
            let step = convert_unit(from, to, x + 0.001).expect("listed");
            assert!(here <= next && here < step, "{from} -> {to} at {x}");
        }
    }
}

#[test]
fn kelvin_and_celsius_share_one_offset_and_fahrenheit_one_ratio() {
    for &x in sweep().iter().filter(|x| x.abs() < 1.0e12) {
        let kelvin = convert_unit("degC", "K", x).expect("listed");
        let back = convert_unit("K", "degC", kelvin).expect("listed");
        let ulp = (x.abs() + 273.15).next_up() - (x.abs() + 273.15);
        assert!((back - x).abs() <= ulp, "degC {x:?} came back {back:?}");
        // degF to K is degF to degC and then degC to K: the same operations in the same order,
        // so the same bits, which holds the three pairs to one offset and one ratio.
        let direct = convert_unit("degF", "K", x).expect("listed");
        let celsius = convert_unit("degF", "degC", x).expect("listed");
        let through = convert_unit("degC", "K", celsius).expect("listed");
        assert_eq!(direct.to_bits(), through.to_bits(), "degF {x:?}");
    }
}

#[test]
fn the_golden_would_catch_a_fused_multiply_add() {
    // degF -> K multiplies, then adds. Fused, the rounding between the two is gone and the bits
    // move; the golden holds inputs where they do, so a contraction cannot pass unseen.
    let differing = golden()
        .iter()
        .filter(|row| row.from == "degF" && row.to == "K" && row.input.is_finite())
        .filter(|row| {
            let fused = (row.input - 32.0).mul_add(5.0 / 9.0, 273.15);
            row.output != Some(fused.to_bits())
        })
        .count();
    assert!(differing >= 3, "{differing}");
}

#[test]
fn the_table_is_named_and_its_codes_are_kebab_case() {
    assert_eq!(UNITS_TABLE, "units/v1");
    assert_eq!(
        UNIT_ERROR_CODES,
        ["unknown-pair", "one-sided", "empty-unit", "overflow"]
    );
    for code in UNIT_ERROR_CODES {
        assert!(
            code.bytes().all(|b| b.is_ascii_lowercase() || b == b'-'),
            "{code}"
        );
    }
}
