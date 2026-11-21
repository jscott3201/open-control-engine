//! The resolution order of a point's two units, the table's shape against the engine's own
//! spellings, the bits of an unconverted value, and the bounded excerpt a refusal quotes. The
//! table's arithmetic is held to an outside oracle in `tests/unit_table.rs`.

use std::collections::BTreeSet;

use super::*;

/// Rust's quiet NaN, a quiet payload, a signalling NaN, and both with the sign bit set.
const NAN_BITS: [u64; 5] = [
    0x7ff8_0000_0000_0000,
    0x7ff8_0000_0000_0001,
    0x7ff0_0000_0000_0001,
    0xfff8_0000_0000_0000,
    0xfff4_0000_0000_0000,
];

/// Every bit pattern the pass-through cases must keep: the zeros, the extremes, a subnormal,
/// the infinities and the NaNs.
fn awkward_bits() -> Vec<u64> {
    let mut bits = vec![
        0.0_f64.to_bits(),
        (-0.0_f64).to_bits(),
        f64::MAX.to_bits(),
        f64::MIN.to_bits(),
        1,
        f64::INFINITY.to_bits(),
        f64::NEG_INFINITY.to_bits(),
        72.5_f64.to_bits(),
    ];
    bits.extend(NAN_BITS);
    bits
}

#[test]
fn no_unit_on_either_side_is_no_conversion_and_keeps_every_bit() {
    let conversion = UnitConversion::resolve(None, None).expect("no units, no conversion");
    assert_eq!(conversion, UnitConversion::Unitless);
    for bits in awkward_bits() {
        let out = conversion
            .apply(f64::from_bits(bits))
            .expect("never refused");
        assert_eq!(out.to_bits(), bits, "{bits:#018x}");
    }
}

#[test]
fn a_unit_on_one_side_only_is_refused_naming_the_side_that_has_it() {
    let device = UnitConversion::resolve(Some("degF"), None).unwrap_err();
    assert_eq!(
        device,
        UnitError::OneSided {
            side: UnitSide::Device,
            unit: "degF".to_owned(),
        }
    );
    assert_eq!(device.code(), "one-sided");
    assert_eq!(
        device.to_string(),
        "the device side has the unit `degF` and the cdl side has none"
    );
    let cdl = UnitConversion::resolve(None, Some("K")).unwrap_err();
    assert_eq!(
        cdl,
        UnitError::OneSided {
            side: UnitSide::Cdl,
            unit: "K".to_owned(),
        }
    );
    assert_eq!(
        cdl.to_string(),
        "the cdl side has the unit `K` and the device side has none"
    );
    // A unit the table lists, an identity spelling and an unknown one are all one-sided alike.
    for unit in ["1", "psi", "degC"] {
        for (device, cdl) in [(Some(unit), None), (None, Some(unit))] {
            assert_eq!(
                UnitConversion::resolve(device, cdl).unwrap_err().code(),
                "one-sided"
            );
        }
    }
}

#[test]
fn an_empty_unit_is_refused_before_a_missing_one_and_the_device_side_first() {
    let cases = [
        (Some(""), None, UnitSide::Device),
        (None, Some(""), UnitSide::Cdl),
        (Some(""), Some(""), UnitSide::Device),
        (Some(""), Some("K"), UnitSide::Device),
        (Some("degF"), Some(""), UnitSide::Cdl),
    ];
    for (device, cdl, side) in cases {
        let refused = UnitConversion::resolve(device, cdl).unwrap_err();
        assert_eq!(refused, UnitError::EmptyUnit { side }, "{device:?} {cdl:?}");
        assert_eq!(refused.code(), "empty-unit");
    }
    assert_eq!(
        convert_unit("", "", 1.0).unwrap_err().to_string(),
        "the device unit is empty"
    );
    assert_eq!(
        convert_unit("K", "", 1.0).unwrap_err().to_string(),
        "the cdl unit is empty"
    );
}

#[test]
fn identical_spellings_are_the_identity_for_every_bit_pattern() {
    // Listed spellings, a unit the table does not list, and a spelling with a character no
    // reader sees: the same bytes on both sides are the same unit, whatever they spell.
    for unit in ["K", "degF", "Pa", "1", "%", "psi", "K\u{200b}", " "] {
        let conversion = UnitConversion::resolve(Some(unit), Some(unit)).expect("identity");
        assert_eq!(conversion, UnitConversion::Identity, "{unit:?}");
        for bits in awkward_bits() {
            let out = convert_unit(unit, unit, f64::from_bits(bits)).expect("identity");
            assert_eq!(out.to_bits(), bits, "{unit:?} {bits:#018x}");
        }
    }
}

/// The one pair whose reverse is also listed: `degC` -> `K` and `K` -> `degC` are both in the
/// note's table, as two one-way rows.
const LISTED_BOTH_WAYS: [(&str, &str); 2] = [("degC", "K"), ("K", "degC")];

#[test]
fn every_listed_pair_resolves_to_its_own_entry_and_its_reverse_is_unknown() {
    for listed in &UNITS_V1 {
        let conversion = UnitConversion::resolve(Some(listed.from), Some(listed.to))
            .expect("a listed pair resolves");
        assert_eq!(conversion, UnitConversion::Listed(listed));
        if LISTED_BOTH_WAYS.contains(&(listed.from, listed.to)) {
            let reverse = UnitConversion::resolve(Some(listed.to), Some(listed.from))
                .expect("listed in its own row");
            let UnitConversion::Listed(row) = reverse else {
                panic!("{reverse:?}");
            };
            assert_eq!((row.from, row.to), (listed.to, listed.from));
            continue;
        }
        let reverse = UnitConversion::resolve(Some(listed.to), Some(listed.from)).unwrap_err();
        assert_eq!(
            reverse,
            UnitError::UnknownPair {
                from: listed.to.to_owned(),
                to: listed.from.to_owned(),
            }
        );
    }
}

#[test]
fn the_table_is_the_notes_eleven_pairs_in_its_order_each_once() {
    let pairs: Vec<(&str, &str)> = UNITS_V1.iter().map(|e| (e.from, e.to)).collect();
    assert_eq!(
        pairs,
        [
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
        ]
    );
    let distinct: BTreeSet<(&str, &str)> = pairs.iter().copied().collect();
    assert_eq!(distinct.len(), UNITS_V1.len());
    let mut both_ways = Vec::new();
    for (from, to) in &pairs {
        assert_ne!(from, to, "an identity pair is decided before the table");
        if pairs.contains(&(*to, *from)) {
            both_ways.push((*from, *to));
        }
    }
    assert_eq!(both_ways, LISTED_BOTH_WAYS);
}

#[test]
fn every_spelling_is_printable_ascii_with_no_space_and_reads_back_as_itself() {
    for listed in &UNITS_V1 {
        for spelling in [listed.from, listed.to] {
            assert!(
                !spelling.is_empty() && spelling.bytes().all(|b| b.is_ascii_graphic()),
                "{spelling:?}"
            );
            assert_eq!(excerpt(spelling), spelling);
        }
    }
}

#[test]
fn a_unit_the_engine_also_reads_has_the_engines_spelling() {
    // The engine's unit reading (`oce_api::QUDT_UNITS`) gives each unit it knows one string.
    // Where the table names one of those units it must use that string: the CDL side's five
    // and degF. The engine has no reading of the other six device spellings: it drops
    // modelica-json's `cfm` and `inH2O` entries, whose QUDT identifiers name other units.
    let engine: BTreeSet<&str> = oce_api::QUDT_UNITS.iter().map(|(_, unit)| *unit).collect();
    let shared = ["K", "degC", "degF", "Pa", "m3/s", "1"];
    let unshared = ["kPa", "inH2O", "inH2O_60F", "cfm", "L/s", "percent", "%"];
    let mut spellings = BTreeSet::new();
    for listed in &UNITS_V1 {
        spellings.insert(listed.from);
        spellings.insert(listed.to);
    }
    let expected: BTreeSet<&str> = shared.iter().chain(&unshared).copied().collect();
    assert_eq!(spellings, expected);
    for unit in shared {
        assert!(
            engine.contains(unit),
            "the engine spells `{unit}` otherwise"
        );
    }
    for unit in unshared {
        assert!(!engine.contains(unit), "the engine also reads `{unit}`");
    }
}

#[test]
fn a_non_finite_reading_keeps_its_bits_through_every_listed_pair() {
    for listed in &UNITS_V1 {
        let conversion = UnitConversion::Listed(listed);
        for bits in [f64::INFINITY.to_bits(), f64::NEG_INFINITY.to_bits()]
            .into_iter()
            .chain(NAN_BITS)
        {
            let out = conversion
                .apply(f64::from_bits(bits))
                .expect("passed through");
            assert_eq!(out.to_bits(), bits, "{} {bits:#018x}", listed.from);
        }
    }
}

#[test]
fn a_finite_reading_that_overflows_is_refused_naming_the_pair_and_the_reading() {
    let refused = convert_unit("kPa", "Pa", f64::MAX).unwrap_err();
    assert_eq!(
        refused,
        UnitError::Overflow {
            from: "kPa",
            to: "Pa",
            value: f64::MAX,
        }
    );
    assert_eq!(refused.code(), "overflow");
    assert_eq!(
        refused.to_string(),
        "the reading 1.7976931348623157e308 in `kPa` is beyond the largest `f64` in `Pa`"
    );
    assert_eq!(
        convert_unit("inH2O", "Pa", f64::MIN).unwrap_err(),
        UnitError::Overflow {
            from: "inH2O",
            to: "Pa",
            value: f64::MIN,
        }
    );
    // A factor below one, a divisor and an offset never overflow.
    for listed in &UNITS_V1 {
        if matches!(listed.operation, Operation::Multiply(factor) if factor > 1.0) {
            continue;
        }
        for x in [f64::MAX, f64::MIN] {
            assert!(
                convert_unit(listed.from, listed.to, x).is_ok(),
                "{} {x}",
                listed.from
            );
        }
    }
}

#[test]
fn an_unknown_pair_names_both_spellings_as_excerpts() {
    let refused = convert_unit("°F", "K", 32.0).unwrap_err();
    assert_eq!(
        refused,
        UnitError::UnknownPair {
            from: "\\u{b0}F".to_owned(),
            to: "K".to_owned(),
        }
    );
    assert_eq!(
        refused.to_string(),
        "units/v1 has no conversion from `\\u{b0}F` to `K`"
    );
    let long = "x".repeat(1 << 20);
    let refused = convert_unit(&long, "Pa", 1.0).unwrap_err();
    let UnitError::UnknownPair { from, .. } = &refused else {
        panic!("an unknown pair: {refused:?}");
    };
    assert_eq!(from.len(), MAX_UNIT_EXCERPT_BYTES);
    assert!(refused.to_string().len() < 2 * MAX_UNIT_EXCERPT_BYTES);
    let refused = UnitConversion::resolve(None, Some(&long)).unwrap_err();
    let UnitError::OneSided { unit, .. } = &refused else {
        panic!("one-sided: {refused:?}");
    };
    assert_eq!(unit.len(), MAX_UNIT_EXCERPT_BYTES);
}

#[test]
fn the_error_codes_are_pinned_and_each_variant_has_its_own() {
    assert_eq!(
        UNIT_ERROR_CODES,
        ["unknown-pair", "one-sided", "empty-unit", "overflow"]
    );
    let one_of_each = [
        UnitError::UnknownPair {
            from: String::new(),
            to: String::new(),
        },
        UnitError::OneSided {
            side: UnitSide::Device,
            unit: String::new(),
        },
        UnitError::EmptyUnit {
            side: UnitSide::Cdl,
        },
        UnitError::Overflow {
            from: "",
            to: "",
            value: 0.0,
        },
    ];
    assert_eq!(one_of_each.len(), UNIT_ERROR_CODES.len());
    let codes: Vec<&str> = one_of_each.iter().map(UnitError::code).collect();
    assert_eq!(codes, UNIT_ERROR_CODES);
    assert_eq!(UnitSide::Device.as_str(), "device");
    assert_eq!(UnitSide::Cdl.as_str(), "cdl");
}

#[test]
fn an_excerpt_is_whole_when_it_fits_and_cut_with_a_mark_when_it_does_not() {
    assert_eq!(excerpt("degF"), "degF");
    assert_eq!(excerpt(""), "");
    let fits = "a".repeat(MAX_UNIT_EXCERPT_BYTES);
    assert_eq!(excerpt(&fits), fits);
    let over = "a".repeat(MAX_UNIT_EXCERPT_BYTES + 1);
    assert_eq!(
        excerpt(&over),
        format!("{}...", "a".repeat(MAX_UNIT_EXCERPT_BYTES - 3))
    );
    // An escape is never cut: seven eight-byte escapes and the mark, 59 bytes.
    let invisible = "\u{200b}".repeat(20);
    assert_eq!(excerpt(&invisible), format!("{}...", "\\u{200b}".repeat(7)));
    let huge = "\u{feff}".repeat(1 << 18);
    assert!(excerpt(&huge).len() <= MAX_UNIT_EXCERPT_BYTES);
}

#[test]
fn an_excerpt_escapes_everything_but_printable_ascii_and_the_quoting_characters() {
    assert_eq!(excerpt("m3/s"), "m3/s");
    assert_eq!(excerpt("deg F"), "deg\\u{20}F");
    assert_eq!(excerpt("K\u{200b}"), "K\\u{200b}");
    assert_eq!(excerpt("\u{feff}K"), "\\u{feff}K");
    assert_eq!(excerpt("a`b\\c"), "a\\u{60}b\\u{5c}c");
    assert_eq!(excerpt("\n\t\u{7f}"), "\\u{a}\\u{9}\\u{7f}");
    assert_eq!(excerpt("m³/s"), "m\\u{b3}/s");
}
