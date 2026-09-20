//! Pre-serialization controls: a rounded CSV cannot prove an exact source Integer was preserved.

use super::{Golden, InputSeries, Sample, ValueKind, assert_integer_csv_cells_are_exact};

fn conversion(input: i64) -> Golden {
    Golden::new(
        "CDL.Conversions.IntegerToReal",
        "y",
        ValueKind::Real,
        vec![0.0],
        vec![Sample::Real(input as f64)],
        "test input",
        "test conversion",
    )
    .with_inputs(vec![InputSeries::new(
        "u",
        ValueKind::Integer,
        vec![Sample::Integer(input)],
    )])
}

#[test]
fn cdl_integer_inputs_refuse_both_outside_endpoints_and_lossy_source_values() {
    for input in [
        i64::from(i32::MIN) - 1,
        i64::from(i32::MAX) + 1,
        9_007_199_254_740_993,
        -9_007_199_254_740_993,
        i64::MIN,
        i64::MAX,
    ] {
        assert!(
            std::panic::catch_unwind(|| {
                assert_integer_csv_cells_are_exact(&[conversion(input)]);
            })
            .is_err(),
            "source Integer escaped before CSV encoding: {input}"
        );
    }
}

#[test]
fn cdl_integer_outputs_refuse_overflow_even_with_portable_inputs() {
    let golden = Golden::new(
        "CDL.Integers.Abs",
        "y",
        ValueKind::Integer,
        vec![0.0],
        vec![Sample::Integer(2_147_483_648)],
        "u=-2147483648",
        "abs(u)",
    )
    .with_inputs(vec![InputSeries::new(
        "u",
        ValueKind::Integer,
        vec![Sample::Integer(i64::from(i32::MIN))],
    )]);
    assert!(std::panic::catch_unwind(|| assert_integer_csv_cells_are_exact(&[golden])).is_err());
}

#[test]
fn endpoints_are_inclusive_and_real_outputs_are_not_integer_inputs() {
    for input in [i64::from(i32::MIN), -1, 0, 1, i64::from(i32::MAX)] {
        let mut golden = conversion(input);
        // Real output policy is intentionally distinct from the Integer input policy.
        golden.samples = vec![Sample::Real(9_007_199_254_740_992.0)];
        assert_integer_csv_cells_are_exact(&[golden]);
    }
}

#[test]
fn non_cdl_integer_csv_exactness_still_refuses_lossy_values() {
    let mut golden = conversion(9_007_199_254_740_993);
    golden.class_path = "G36";
    assert!(std::panic::catch_unwind(|| assert_integer_csv_cells_are_exact(&[golden])).is_err());
}
