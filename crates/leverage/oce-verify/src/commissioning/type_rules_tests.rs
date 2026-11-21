//! The discrete type rules, cell by cell: every type pair, every accepted encoding, and the
//! values just beside each one.

use super::*;

/// Every NaN shape a trend might carry: Rust's quiet NaN, a payload, a signalling NaN, and both
/// signs.
const NAN_BITS: [u64; 5] = [
    0x7ff8_0000_0000_0000,
    0x7ff8_0000_0000_0001,
    0x7ff0_0000_0000_0001,
    0xfff8_0000_0000_0000,
    0xfff4_0000_0000_0000,
];

#[test]
fn every_type_pair_has_its_rule_and_only_bool_to_float_is_refused() {
    use PointType::{Bool, Float, Int};
    let expected = |device: PointType, cdl: PointType| match (device, cdl) {
        (Float, Float) => Some(CellRule::Real),
        (Int, Float) => Some(CellRule::IntegerAsReal),
        (Float | Int, Int) => Some(CellRule::Integer),
        (Float | Int | Bool, Bool) => Some(CellRule::Boolean),
        (Bool, Int) => Some(CellRule::BooleanAsInteger),
        (Bool, Float) => None,
    };
    let mut refused = Vec::new();
    for device in PointType::ALL {
        for cdl in PointType::ALL {
            match (CellRule::for_types(device, cdl), expected(device, cdl)) {
                (Ok(rule), Some(want)) => assert_eq!(rule, want, "{device} -> {cdl}"),
                (Err(error), None) => {
                    assert_eq!(error, CellError::TypePairRefused { device, cdl });
                    assert_eq!(error.code(), "type-pair-refused");
                    refused.push((device, cdl));
                }
                (got, want) => panic!("{device} -> {cdl}: {got:?}, expected {want:?}"),
            }
        }
    }
    assert_eq!(refused, [(Bool, Float)]);
    assert_eq!(
        CellRule::for_types(Bool, Float).unwrap_err().to_string(),
        "no rule reads a `bool` device point as a `float` CDL point"
    );
}

#[test]
fn a_whole_number_in_an_int_column_widens_exactly_into_a_real() {
    let accepted = [
        (0.0, 0.0),
        (-0.0, 0.0),
        (1.0, 1.0),
        (-1.0, -1.0),
        (2_147_483_646.0, 2_147_483_646.0),
        (2_147_483_647.0, 2_147_483_647.0),
        (-2_147_483_647.0, -2_147_483_647.0),
        (-2_147_483_648.0, -2_147_483_648.0),
    ];
    for (value, real) in accepted {
        let got = real_from_whole_number(value).expect("a whole number inside i32");
        assert_eq!(got.to_bits(), f64::to_bits(real), "{value:?}");
    }
    // -0.0 is the integer zero, so it widens to +0.0.
    assert_eq!(real_from_whole_number(-0.0).map(f64::to_bits), Ok(0));
    for value in [
        2_147_483_648.0,
        -2_147_483_649.0,
        1.0e300,
        f64::MAX,
        f64::MIN,
    ] {
        assert_eq!(
            real_from_whole_number(value),
            Err(CellError::OutsideInt32 { value }),
            "{value:?}"
        );
    }
    for value in [
        0.5,
        2_147_483_646.5,
        -2_147_483_648.5,
        f64::from_bits(1),
        f64::INFINITY,
        f64::NEG_INFINITY,
    ] {
        assert_eq!(
            real_from_whole_number(value),
            Err(CellError::NotIntegral { value }),
            "{value:?}"
        );
    }
    let refused = real_from_whole_number(f64::NAN).unwrap_err();
    assert_eq!(refused.code(), "not-integral");
}

#[test]
fn a_number_is_a_boolean_only_when_exactly_zero_or_one() {
    assert_eq!(boolean_from_number(0.0), Ok(false));
    assert_eq!(boolean_from_number(-0.0), Ok(false));
    assert_eq!(boolean_from_number(1.0), Ok(true));
    let refused = [
        0.5,
        2.0,
        -1.0,
        1.000_000_000_000_000_2,
        0.999_999_999_999_999_9,
        f64::from_bits(1),
        -f64::from_bits(1),
        f64::MIN_POSITIVE,
        f64::EPSILON,
        f64::MAX,
        f64::MIN,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ];
    for value in refused {
        assert_eq!(
            boolean_from_number(value),
            Err(CellError::NotBoolean { value }),
            "{value:?}"
        );
    }
    for bits in NAN_BITS {
        let refused = boolean_from_number(f64::from_bits(bits)).unwrap_err();
        let CellError::NotBoolean { value } = refused else {
            panic!("{bits:#018x}: {refused:?}");
        };
        assert_eq!(value.to_bits(), bits);
    }
    assert_eq!(
        boolean_from_number(0.5).unwrap_err().to_string(),
        "0.5 is not a Boolean: only exactly 0 (false) and 1 (true) are"
    );
}

#[test]
fn a_text_cell_is_a_boolean_only_in_one_of_six_exact_spellings() {
    for (text, value) in [
        ("true", true),
        ("active", true),
        ("1", true),
        ("false", false),
        ("inactive", false),
        ("0", false),
    ] {
        assert_eq!(boolean_from_text(text), Ok(value), "{text}");
    }
    let refused = [
        "",
        "True",
        "TRUE",
        "False",
        "ACTIVE",
        "Active",
        "Inactive",
        " 1",
        "1 ",
        " true",
        "true\n",
        "1.0",
        "0.0",
        "+1",
        "-0",
        "01",
        "yes",
        "no",
        "on",
        "off",
        "t",
        "f",
        "truee",
        "tru",
        "inactiv",
        "\u{feff}1",
        "1\u{200b}",
        "\u{ff11}",
        "null",
    ];
    for text in refused {
        assert_eq!(
            boolean_from_text(text),
            Err(CellError::NotBooleanText {
                text: excerpt(text),
            }),
            "{text:?}"
        );
    }
    assert_eq!(
        boolean_from_text(" 1").unwrap_err().to_string(),
        "` 1` is not a Boolean: only `true`, `false`, `active`, `inactive`, `1` and `0` are"
    );
}

#[test]
fn a_huge_text_cell_is_quoted_as_a_bounded_excerpt() {
    let huge = "t".repeat(1 << 20);
    let CellError::NotBooleanText { text } = boolean_from_text(&huge).unwrap_err() else {
        panic!("not-boolean-text");
    };
    assert_eq!(text.len(), 64);
    assert!(text.ends_with("..."));
}

#[test]
fn a_boolean_as_an_integer_is_zero_or_one() {
    assert_eq!(integer_from_boolean(false), 0);
    assert_eq!(integer_from_boolean(true), 1);
}

#[test]
fn a_number_is_an_integer_when_integral_and_inside_i32_both_ends_included() {
    let accepted = [
        (0.0, 0),
        (-0.0, 0),
        (1.0, 1),
        (-1.0, -1),
        (42.0, 42),
        (2_147_483_646.0, i32::MAX - 1),
        (2_147_483_647.0, i32::MAX),
        (-2_147_483_647.0, i32::MIN + 1),
        (-2_147_483_648.0, i32::MIN),
    ];
    for (value, integer) in accepted {
        assert_eq!(integer_from_number(value), Ok(integer), "{value:?}");
    }
}

#[test]
fn an_integral_number_just_outside_i32_is_refused_as_out_of_range() {
    let outside = [
        2_147_483_648.0,
        -2_147_483_649.0,
        4_294_967_296.0,
        9_007_199_254_740_992.0,
        1.0e300,
        f64::MAX,
        f64::MIN,
    ];
    for value in outside {
        assert_eq!(
            integer_from_number(value),
            Err(CellError::OutsideInt32 { value }),
            "{value:?}"
        );
    }
    assert_eq!(
        integer_from_number(2_147_483_648.0)
            .unwrap_err()
            .to_string(),
        "2147483648.0 is outside the CDL Integer range, -2147483648 to 2147483647"
    );
}

#[test]
fn a_fraction_a_nan_or_an_infinity_is_not_an_integer() {
    let not_integral = [
        0.5,
        -0.5,
        2.000_000_000_000_000_4,
        0.999_999_999_999_999_9,
        2_147_483_646.5,
        2_147_483_647.5,
        -2_147_483_648.5,
        f64::from_bits(1),
        -f64::from_bits(1),
        f64::MIN_POSITIVE,
        1.0e-300,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ];
    for value in not_integral {
        assert_eq!(
            integer_from_number(value),
            Err(CellError::NotIntegral { value }),
            "{value:?}"
        );
    }
    for bits in NAN_BITS {
        let refused = integer_from_number(f64::from_bits(bits)).unwrap_err();
        let CellError::NotIntegral { value } = refused else {
            panic!("{bits:#018x}: {refused:?}");
        };
        assert_eq!(value.to_bits(), bits);
    }
    assert_eq!(
        integer_from_number(0.5).unwrap_err().to_string(),
        "0.5 is not an integral number"
    );
}

#[test]
fn the_cell_error_codes_are_pinned_and_each_variant_has_its_own() {
    assert_eq!(
        CELL_ERROR_CODES,
        [
            "not-boolean",
            "not-boolean-text",
            "not-integral",
            "outside-int32",
            "type-pair-refused",
        ]
    );
    let one_of_each = [
        CellError::NotBoolean { value: 0.5 },
        CellError::NotBooleanText {
            text: String::new(),
        },
        CellError::NotIntegral { value: 0.5 },
        CellError::OutsideInt32 { value: 1.0e10 },
        CellError::TypePairRefused {
            device: PointType::Bool,
            cdl: PointType::Float,
        },
    ];
    let codes: Vec<&str> = one_of_each.iter().map(CellError::code).collect();
    assert_eq!(codes, CELL_ERROR_CODES);
}
