//! The range rendering, rule kind by rule kind, on rules written here: the exact text, which
//! parameters each line is listed for, and how Real bounds are spelled. The catalog's own rules
//! are held to the rendering in `tests/vocabulary/ranges.rs`.

use oce_api::{CatalogPortKind, CatalogRule, CatalogValueKind};

use super::parameter_range;

fn range(rule: CatalogRule, parameter: &str) -> Vec<String> {
    parameter_range(&[rule], parameter)
}

fn one(text: &str) -> Vec<String> {
    vec![text.to_owned()]
}

#[test]
fn a_bound_on_one_parameter_is_listed_for_that_parameter_only() {
    let cases = [
        (CatalogRule::RealFinite { name: "p" }, "p is finite"),
        (
            CatalogRule::RealGreaterThan {
                name: "p",
                min: 0.0,
            },
            "p > 0.0",
        ),
        (
            CatalogRule::RealFiniteGreaterThan {
                name: "p",
                min: 0.0,
            },
            "p > 0.0 and finite",
        ),
        (
            CatalogRule::RealGreaterOrEqual {
                name: "p",
                min: 1e-13,
            },
            "p >= 1e-13",
        ),
        (
            CatalogRule::RealLessOrEqualConstant {
                name: "p",
                max: -1e-13,
            },
            "p <= -1e-13",
        ),
        (
            CatalogRule::IntegerGreaterOrEqual { name: "p", min: -1 },
            "p >= -1",
        ),
        (
            CatalogRule::IntegerLessOrEqualConstant {
                name: "p",
                max: 1_048_576,
            },
            "p <= 1048576",
        ),
    ];
    for (rule, text) in cases {
        assert_eq!(range(rule.clone(), "p"), one(text));
        for other in ["", "q", "P", "p_", "p_1"] {
            assert!(range(rule.clone(), other).is_empty(), "{text}: {other:?}");
        }
    }
}

#[test]
fn a_relation_between_two_parameters_is_listed_for_both() {
    let cases = [
        (
            CatalogRule::RealLessOrEqual {
                lower: "a",
                upper: "b",
            },
            "a <= b",
        ),
        (
            CatalogRule::RealTimesIntegerInclusiveRange {
                real: "a",
                integer: "b",
                min: 0.001,
                max: 0.5,
            },
            "0.001 <= a * b <= 0.5",
        ),
        (
            CatalogRule::IntegerProductLessOrEqualConstant {
                left: "a",
                right: "b",
                max: 1_048_576,
            },
            "a * b <= 1048576",
        ),
        (
            CatalogRule::BooleanArrayTrueCountEquals {
                base: "a",
                len: "n",
                count: "b",
                default: true,
            },
            "the number of true a_<i> == b",
        ),
    ];
    for (rule, text) in cases {
        assert_eq!(range(rule.clone(), "a"), one(text));
        assert_eq!(range(rule.clone(), "b"), one(text));
        assert!(range(rule.clone(), "n").is_empty(), "{text}");
        assert!(range(rule, "c").is_empty(), "{text}");
    }
}

#[test]
fn a_relation_that_only_warns_names_the_case_that_warns() {
    let cases = [
        (
            CatalogRule::RealLessOrEqualWarning {
                lower: "a",
                upper: "b",
            },
            "a > b warns",
        ),
        (
            CatalogRule::RealGreaterOrEqualScaledWarning {
                left: "a",
                right: "b",
                factor: 2.0,
            },
            "a < 2.0 * b warns",
        ),
        (
            CatalogRule::RealEqualWarning {
                left: "a",
                right: "b",
            },
            "a == b warns",
        ),
    ];
    for (rule, text) in cases {
        assert_eq!(range(rule.clone(), "a"), one(text));
        assert_eq!(range(rule, "b"), one(text));
    }
}

#[test]
fn an_element_bound_is_listed_for_the_array_under_its_template_name() {
    let rule = CatalogRule::IntegerArrayElementsInRange {
        base: "extract",
        len: "nout",
        len_default: 1,
        min: 1,
        max: "nin",
        max_default: 1,
        default_to_index: true,
    };
    assert_eq!(
        range(rule.clone(), "extract"),
        one("1 <= extract_<i> <= nin")
    );
    // The bound is the elements': it is not a range of the length or of the upper bound.
    assert!(range(rule.clone(), "nout").is_empty());
    assert!(range(rule.clone(), "nin").is_empty());
    assert!(range(rule, "extract_<i>").is_empty());
}

#[test]
fn rules_about_type_shape_and_structure_render_no_range() {
    let rules = [
        CatalogRule::Required {
            name: "p",
            kind: CatalogValueKind::Real,
        },
        CatalogRule::Boolean { name: "p" },
        CatalogRule::Real { name: "p" },
        CatalogRule::EnumMembers {
            name: "p",
            members: &["A", "B"],
        },
        CatalogRule::Structural { name: "p" },
        CatalogRule::StructuralArrayElements { base: "p" },
        CatalogRule::IntegerArrayElements {
            base: "p",
            len: "n",
        },
        CatalogRule::RealArrayElements {
            base: "p",
            len: "n",
        },
        CatalogRule::BooleanArrayElements {
            base: "p",
            len: "n",
        },
        CatalogRule::RealMatrixElements {
            base: "p",
            rows: "n",
            default_rows: 2,
            cols: "m",
            default_cols: 2,
        },
        CatalogRule::TimeTableMatrix {
            base: "p",
            values: CatalogPortKind::Real,
            time_scale: "n",
            period: Some("m"),
            extrapolation: Some("e"),
        },
        CatalogRule::TimeTableOffset {
            base: "p",
            table: "n",
        },
    ];
    for rule in rules {
        for parameter in ["p", "n", "m", "e"] {
            assert!(range(rule.clone(), parameter).is_empty(), "{rule:?}");
        }
    }
}

#[test]
fn lines_keep_rule_order_and_real_bounds_read_back_to_the_same_bits() {
    let rules = [
        CatalogRule::Required {
            name: "p",
            kind: CatalogValueKind::Real,
        },
        CatalogRule::RealGreaterOrEqual {
            name: "p",
            min: 1e-13,
        },
        CatalogRule::RealLessOrEqualConstant {
            name: "q",
            max: 1.0,
        },
        CatalogRule::RealLessOrEqual {
            lower: "p",
            upper: "q",
        },
        CatalogRule::RealEqualWarning {
            left: "p",
            right: "q",
        },
    ];
    assert_eq!(
        parameter_range(&rules, "p"),
        ["p >= 1e-13", "p <= q", "p == q warns"]
    );
    assert_eq!(
        parameter_range(&rules, "q"),
        ["q <= 1.0", "p <= q", "p == q warns"]
    );
    assert!(parameter_range(&rules, "r").is_empty());
    assert!(parameter_range(&[], "p").is_empty());

    // The spelling of a Real bound is exact: it parses back to the bits the rule holds.
    for bound in [
        1e-13,
        -1e-13,
        0.0,
        -0.0,
        0.001,
        0.1,
        f64::MIN_POSITIVE,
        5e-324,
        f64::MAX,
        9_007_199_254_740_993.0,
        1.0 / 3.0,
    ] {
        let line = range(
            CatalogRule::RealGreaterOrEqual {
                name: "p",
                min: bound,
            },
            "p",
        )
        .remove(0);
        let written = line.strip_prefix("p >= ").expect("the bound");
        let read: f64 = written.parse().expect("a Real");
        assert_eq!(read.to_bits(), bound.to_bits(), "{line}");
    }
    // A non-finite bound is spelled, not dropped and not a panic.
    assert_eq!(
        range(
            CatalogRule::RealGreaterThan {
                name: "p",
                min: f64::NEG_INFINITY
            },
            "p"
        ),
        one("p > -inf")
    );
    assert_eq!(
        range(
            CatalogRule::RealGreaterOrEqual {
                name: "p",
                min: f64::NAN
            },
            "p"
        ),
        one("p >= NaN")
    );
}
