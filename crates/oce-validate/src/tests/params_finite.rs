//! The engine-wide Real-parameter finiteness gate (issue #362): predicate truth table, flattened
//! array elements past the first, unregistered classes, class-rule suppression scope, the pinned
//! diagnostic rendering, and repeat determinism. The registry-wide census over every class and
//! every Real parameter lives in `oce-api` (`params_finite_tests`), where it also drives the
//! tune-at-rest edit path.

use super::common::*;
use crate::parameter_value_is_finite;

fn render(diags: &[oce_diag::Diagnostic]) -> Vec<String> {
    diags
        .iter()
        .map(|diag| {
            format!(
                "{}|{}|{}|{}",
                diag.severity.as_str(),
                diag.code.as_str(),
                diag.subject.as_deref().unwrap_or("<none>"),
                diag.message
            )
        })
        .collect()
}

#[test]
fn finiteness_predicate_refuses_exactly_the_non_finite_reals() {
    for bad in [
        f64::NAN,
        -f64::NAN,
        f64::from_bits(0x7ff0_0000_0000_0001), // signalling-NaN payload
        f64::from_bits(0xfff8_dead_beef_0001), // negative quiet-NaN payload
        f64::INFINITY,
        f64::NEG_INFINITY,
    ] {
        assert!(!parameter_value_is_finite(&Value::Real(bad)), "{bad:?}");
    }
    for good in [
        0.0,
        -0.0,
        f64::MAX,
        f64::MIN,
        f64::MIN_POSITIVE,
        -f64::MIN_POSITIVE,
        f64::from_bits(1),
        -f64::from_bits(1),
        f64::EPSILON,
    ] {
        assert!(parameter_value_is_finite(&Value::Real(good)), "{good:e}");
    }
    for other in [
        Value::Integer(i64::MIN),
        Value::Integer(i64::MAX),
        Value::Boolean(false),
        Value::String(Arc::from("NaN")),
        Value::Enum {
            class: oce_model::EnumClassId::ZERO_TIME,
            ordinal: 1,
        },
    ] {
        assert!(parameter_value_is_finite(&other), "{other:?}");
    }
}

#[test]
fn every_non_finite_array_element_is_refused_in_declaration_order_with_a_pinned_rendering() {
    let multi_sum = one_block_model(
        "CDL.Reals.MultiSum",
        &[ValueType::Real, ValueType::Real, ValueType::Real],
        &[ValueType::Real],
        vec![
            (Arc::from("nin"), Value::Integer(3)),
            rp("k_1", 1.0),
            rp("k_2", f64::NAN),
            rp("k_3", f64::INFINITY),
        ],
    );
    let err = validate(&multi_sum).expect_err("non-finite gain elements must refuse");
    assert_eq!(
        render(&err.diagnostics),
        [
            "error|non-finite-parameter|block#0|parameter `k_2` on block `CDL.Reals.MultiSum` must be finite; got NaN",
            "error|non-finite-parameter|block#0|parameter `k_3` on block `CDL.Reals.MultiSum` must be finite; got inf",
        ]
    );

    let matrix_gain = one_block_model(
        "CDL.Reals.MatrixGain",
        &[ValueType::Real, ValueType::Real],
        &[ValueType::Real, ValueType::Real],
        vec![
            (Arc::from("nout"), Value::Integer(2)),
            (Arc::from("nin"), Value::Integer(2)),
            rp("K_1_1", 1.0),
            rp("K_1_2", 0.0),
            rp("K_2_1", f64::NEG_INFINITY),
            rp("K_2_2", 1.0),
        ],
    );
    let err = validate(&matrix_gain).expect_err("non-finite matrix element must refuse");
    assert_eq!(
        render(&err.diagnostics),
        [
            "error|non-finite-parameter|block#0|parameter `K_2_1` on block `CDL.Reals.MatrixGain` must be finite; got -inf",
        ]
    );
}

#[test]
fn unregistered_classes_are_still_inside_the_gate() {
    // Port and class-rule checks skip an unknown class (`oce-api` owns that refusal), but the
    // finiteness gate does not depend on registry coverage.
    let model = one_block_model(
        "CDL.Reals.NotARegisteredClass",
        &[],
        &[ValueType::Real],
        vec![rp("k", f64::NAN)],
    );
    let err = validate(&model).expect_err("non-finite parameter on any class must refuse");
    assert_eq!(
        render(&err.diagnostics),
        [
            "error|non-finite-parameter|block#0|parameter `k` on block `CDL.Reals.NotARegisteredClass` must be finite; got NaN",
        ]
    );
}

#[test]
fn class_rules_are_suppressed_only_on_the_block_carrying_the_non_finite_value() {
    // Both Ramps violate `Td >= 1e-15`; only block 0 also carries a NaN. Block 0 reports just the
    // non-finite cause (its class rules presume finite operands); block 1's range error survives.
    let ramp = |id: u32, raising: f64| {
        block_with_params(
            id,
            "CDL.Reals.Ramp",
            &[id * 3, id * 3 + 1],
            &[id * 3 + 2],
            vec![
                rp("raisingSlewRate", raising),
                rp("fallingSlewRate", -1.0),
                rp("Td", 0.0),
            ],
        )
    };
    let mut connectors = Vec::new();
    for id in 0..2u32 {
        connectors.push(conn(id * 3, id, Dir::In, ValueType::Real));
        connectors.push(conn(id * 3 + 1, id, Dir::In, ValueType::Boolean));
        connectors.push(conn(id * 3 + 2, id, Dir::Out, ValueType::Real));
    }
    let model = ModelGraph {
        blocks: vec![ramp(0, f64::NAN), ramp(1, 1.0)],
        connectors,
        connections: vec![],
        external_inputs: [0, 1, 3, 4].into_iter().map(ConnectorId).collect(),
        boundary_inputs: vec![],
        boundary_outputs: vec![],
    };
    let first = validate(&model).expect_err("both blocks are invalid");
    assert_eq!(
        render(&first.diagnostics),
        [
            "error|non-finite-parameter|block#0|parameter `raisingSlewRate` on block `CDL.Reals.Ramp` must be finite; got NaN",
            "error|parameter-out-of-range|block#1|parameter `Td` on block `CDL.Reals.Ramp` must be >= 0.000000000000001; got 0",
        ]
    );
    // Repeat determinism: the refusal is bit-identical run to run.
    let second = validate(&model).expect_err("both blocks are invalid");
    assert_eq!(render(&first.diagnostics), render(&second.diagnostics));
}

#[test]
fn integer_promoted_real_parameter_is_never_non_finite() {
    let model = real_to_real_model(
        "CDL.Reals.MultiplyByParameter",
        vec![(Arc::from("k"), Value::Integer(i64::MAX))],
    );
    assert!(
        validate(&model)
            .expect("an Integer literal on a Real gain is finite")
            .is_empty()
    );
}
