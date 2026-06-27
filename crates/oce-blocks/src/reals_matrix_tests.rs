//! Matrix and sorting `CDL.Reals` tests.

use oce_model::{ParamTable, Value};

use super::{
    Block, Ctx, MAX_RESOLVED_PORT_WIDTH, MatrixGain, MatrixMax, MatrixMin, MatrixReductionAxis,
    NoopDiagnostics, PortKind, Sort, lookup,
};

fn outs(block: &dyn Block, inputs: &[Value]) -> Vec<Value> {
    let diag = NoopDiagnostics;
    let cx = Ctx::new(0.0, &diag);
    let mut out = Vec::new();
    block.step_algebraic(&cx, inputs, &mut |idx, value| {
        assert_eq!(idx, out.len(), "outputs must be emitted in order");
        out.push(value);
    });
    out
}

fn real_inputs(values: &[f64]) -> Vec<Value> {
    values.iter().copied().map(Value::Real).collect()
}

fn real_bits(values: &[Value]) -> Vec<u64> {
    values
        .iter()
        .map(|value| match value {
            Value::Real(x) => x.to_bits(),
            other => panic!("expected Real output, got {other:?}"),
        })
        .collect()
}

fn integers(values: &[Value]) -> Vec<i64> {
    values
        .iter()
        .map(|value| match value {
            Value::Integer(x) => *x,
            other => panic!("expected Integer output, got {other:?}"),
        })
        .collect()
}

#[test]
fn matrix_gain_uses_default_identity_and_row_major_custom_matrix() {
    let default = MatrixGain::default();
    assert_eq!(
        default.resolved_signature().inputs.as_ref(),
        &[PortKind::Real, PortKind::Real]
    );
    assert_eq!(
        default.resolved_signature().outputs.as_ref(),
        &[PortKind::Real, PortKind::Real]
    );
    assert_eq!(
        real_bits(&outs(&default, &real_inputs(&[3.5, -2.0]))),
        vec![3.5f64.to_bits(), (-2.0f64).to_bits()]
    );

    let custom = MatrixGain::new(3, 2, vec![0.5, 2.0, -1.0, 3.0, 0.0, 0.25]);
    assert_eq!(
        custom.resolved_signature().inputs.as_ref(),
        &[PortKind::Real, PortKind::Real, PortKind::Real]
    );
    assert_eq!(
        custom.resolved_signature().outputs.as_ref(),
        &[PortKind::Real, PortKind::Real]
    );
    assert!(custom.feeds_through(2, 1));
    assert!(!custom.feeds_through(3, 0));
    assert_eq!(
        real_bits(&outs(&custom, &real_inputs(&[4.0, -1.0, 8.0]))),
        vec![(-8.0f64).to_bits(), 14.0f64.to_bits()]
    );

    let via_registry = (lookup("CDL.Reals.MatrixGain").unwrap().make)(&ParamTable {
        values: vec![
            ("nout".into(), Value::Integer(2)),
            ("nin".into(), Value::Integer(3)),
            ("K_1_1".into(), Value::Real(0.5)),
            ("K_1_2".into(), Value::Real(2.0)),
            ("K_1_3".into(), Value::Real(-1.0)),
            ("K_2_1".into(), Value::Integer(3)),
            ("K_2_2".into(), Value::Real(0.0)),
            ("K_2_3".into(), Value::Real(0.25)),
        ],
    });
    assert_eq!(
        real_bits(&outs(
            via_registry.as_ref(),
            &real_inputs(&[4.0, -1.0, 8.0])
        )),
        vec![(-8.0f64).to_bits(), 14.0f64.to_bits()]
    );

    let oversize_product = (lookup("CDL.Reals.MatrixGain").unwrap().make)(&ParamTable {
        values: vec![
            (
                "nout".into(),
                Value::Integer(i64::try_from(MAX_RESOLVED_PORT_WIDTH).unwrap()),
            ),
            (
                "nin".into(),
                Value::Integer(i64::try_from(MAX_RESOLVED_PORT_WIDTH).unwrap()),
            ),
        ],
    });
    assert!(oversize_product.resolved_signature().inputs.is_empty());
    assert!(oversize_product.resolved_signature().outputs.is_empty());
}

#[test]
fn matrix_reducers_follow_row_or_column_axis_and_deterministic_nan_policy() {
    let inputs = real_inputs(&[3.0, f64::NAN, -0.0, 5.0, 0.0, 4.0]);

    let row_max = MatrixMax::new(2, 3, MatrixReductionAxis::Rows);
    assert_eq!(
        real_bits(&outs(&row_max, &inputs)),
        vec![3.0f64.to_bits(), 5.0f64.to_bits()]
    );
    assert!(row_max.feeds_through(1, 0));
    assert!(!row_max.feeds_through(3, 0));

    let col_max = MatrixMax::new(2, 3, MatrixReductionAxis::Columns);
    assert_eq!(
        real_bits(&outs(&col_max, &inputs)),
        vec![5.0f64.to_bits(), 0.0f64.to_bits(), 4.0f64.to_bits()]
    );
    assert!(col_max.feeds_through(3, 0));
    assert!(!col_max.feeds_through(3, 1));

    let row_min = MatrixMin::new(2, 3, MatrixReductionAxis::Rows);
    assert_eq!(
        real_bits(&outs(&row_min, &inputs)),
        vec![(-0.0f64).to_bits(), 0.0f64.to_bits()]
    );

    let col_min = MatrixMin::new(2, 3, MatrixReductionAxis::Columns);
    assert_eq!(
        real_bits(&outs(&col_min, &inputs)),
        vec![3.0f64.to_bits(), 0.0f64.to_bits(), (-0.0f64).to_bits()]
    );
}

#[test]
fn sort_matches_modelica_shellsort_values_and_one_based_indices() {
    let ascending = Sort::new(4, true);
    assert_eq!(
        ascending.resolved_signature().inputs.as_ref(),
        &[
            PortKind::Real,
            PortKind::Real,
            PortKind::Real,
            PortKind::Real
        ]
    );
    assert_eq!(
        ascending.resolved_signature().outputs.as_ref(),
        &[
            PortKind::Real,
            PortKind::Real,
            PortKind::Real,
            PortKind::Real,
            PortKind::Integer,
            PortKind::Integer,
            PortKind::Integer,
            PortKind::Integer,
        ]
    );
    assert!(ascending.feeds_through(3, 7));
    assert!(!ascending.feeds_through(4, 0));

    let got = outs(&ascending, &real_inputs(&[3.0, 1.0, 2.0, 1.0]));
    assert_eq!(
        real_bits(&got[..4]),
        vec![
            1.0f64.to_bits(),
            1.0f64.to_bits(),
            2.0f64.to_bits(),
            3.0f64.to_bits(),
        ]
    );
    assert_eq!(integers(&got[4..]), vec![2, 4, 3, 1]);

    let descending = Sort::new(4, false);
    let got = outs(&descending, &real_inputs(&[3.0, 1.0, 2.0, 1.0]));
    assert_eq!(
        real_bits(&got[..4]),
        vec![
            3.0f64.to_bits(),
            2.0f64.to_bits(),
            1.0f64.to_bits(),
            1.0f64.to_bits(),
        ]
    );
    assert_eq!(integers(&got[4..]), vec![1, 3, 2, 4]);

    let nan_probe = outs(&Sort::new(3, true), &real_inputs(&[2.0, f64::NAN, 1.0]));
    assert_eq!(
        real_bits(&nan_probe[..3]),
        vec![2.0f64.to_bits(), 0x7ff8000000000000, 1.0f64.to_bits()]
    );
    assert_eq!(integers(&nan_probe[3..]), vec![1, 2, 3]);

    let empty = Sort::new(0, true);
    assert!(empty.resolved_signature().inputs.is_empty());
    assert!(empty.resolved_signature().outputs.is_empty());

    let singleton = outs(&Sort::new(1, false), &real_inputs(&[-0.0]));
    assert_eq!(real_bits(&singleton[..1]), vec![(-0.0f64).to_bits()]);
    assert_eq!(integers(&singleton[1..]), vec![1]);
}
