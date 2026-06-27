//! Vector-reduction `CDL.Reals` tests.

use std::sync::Arc;

use oce_model::{ParamTable, Value};

use super::{Block, Ctx, MultiMax, MultiMin, MultiSum, NoopDiagnostics, PortKind, lookup};

fn real_out(block: &dyn Block, inputs: &[Value]) -> f64 {
    let diag = NoopDiagnostics;
    let cx = Ctx::new(0.0, &diag);
    let mut out = None;
    block.step_algebraic(&cx, inputs, &mut |idx, val| {
        assert_eq!(idx, 0, "Reals vector reducers have one output");
        out = Some(val);
    });
    match out.expect("block must emit one output") {
        Value::Real(y) => y,
        other => panic!("expected Real output, got {other:?}"),
    }
}

fn real_inputs(xs: &[f64]) -> Vec<Value> {
    xs.iter().copied().map(Value::Real).collect()
}

fn assert_real_bits(block: &dyn Block, inputs: &[f64], expected_bits: u64) {
    let y = real_out(block, &real_inputs(inputs));
    assert_eq!(y.to_bits(), expected_bits, "actual={y:?}");
}

#[test]
fn reals_multi_reducers_use_resolved_width_gains_and_ordered_min_max_policy() {
    assert_real_bits(&MultiSum::new(Vec::new()), &[], 0.0f64.to_bits());
    assert_real_bits(&MultiSum::new(vec![1.0]), &[-7.5], (-7.5f64).to_bits());
    assert_real_bits(
        &MultiSum::new(vec![2.0, -0.5, 3.0]),
        &[1.25, 4.0, -2.0],
        (-5.5f64).to_bits(),
    );
    assert_real_bits(&MultiMin::new(0), &[], f64::INFINITY.to_bits());
    assert_real_bits(&MultiMax::new(0), &[], f64::NEG_INFINITY.to_bits());
    assert_real_bits(&MultiMin::new(1), &[-3.25], (-3.25f64).to_bits());
    assert_real_bits(&MultiMax::new(1), &[-3.25], (-3.25f64).to_bits());
    assert_real_bits(
        &MultiMin::new(4),
        &[3.0, f64::NAN, -0.0, 0.0],
        (-0.0f64).to_bits(),
    );
    assert_real_bits(
        &MultiMax::new(4),
        &[3.0, f64::NAN, -0.0, 0.0],
        3.0f64.to_bits(),
    );
    assert_real_bits(&MultiMin::new(2), &[f64::NAN, f64::NAN], 0x7ff8000000000000);
    assert_real_bits(&MultiMax::new(2), &[f64::NAN, f64::NAN], 0x7ff8000000000000);

    let default_gains = (lookup("CDL.Reals.MultiSum").unwrap().make)(&ParamTable {
        values: vec![(Arc::from("nin"), Value::Integer(3))],
    });
    assert_real_bits(default_gains.as_ref(), &[1.0, 2.0, 3.0], 6.0f64.to_bits());

    let explicit_gains = (lookup("CDL.Reals.MultiSum").unwrap().make)(&ParamTable {
        values: vec![
            (Arc::from("nin"), Value::Integer(3)),
            (Arc::from("k_1"), Value::Real(0.5)),
            (Arc::from("k_3"), Value::Integer(2)),
        ],
    });
    assert_eq!(
        explicit_gains.resolved_signature().inputs.as_ref(),
        &[PortKind::Real, PortKind::Real, PortKind::Real]
    );
    assert_eq!(
        explicit_gains.resolved_signature().outputs.as_ref(),
        &[PortKind::Real]
    );
    assert_real_bits(explicit_gains.as_ref(), &[2.0, 4.0, 3.0], 11.0f64.to_bits());
}

#[test]
fn vector_reducer_inputs_feed_through_only_when_resolved() {
    assert!(MultiSum::new(vec![1.0, 2.0, 3.0]).feeds_through(2, 0));
    assert!(!MultiSum::new(vec![1.0, 2.0, 3.0]).feeds_through(3, 0));
    assert!(MultiMin::new(2).feeds_through(1, 0));
    assert!(!MultiMin::new(2).feeds_through(2, 0));
    assert!(MultiMax::new(2).feeds_through(1, 0));
    assert!(!MultiMax::new(2).feeds_through(2, 0));
}
