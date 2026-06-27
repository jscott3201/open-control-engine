use std::cell::RefCell;

use oce_model::Value;

use super::{
    Block, Ctx, Diagnostics, IntegerExtractSignal, IntegerExtractor, IntegerScalarReplicator,
    IntegerVectorFilter, IntegerVectorReplicator, NoopDiagnostics, PortKind, Time,
};

#[derive(Default)]
struct CapturingDiagnostics {
    events: RefCell<Vec<(String, String, Time)>>,
}

impl Diagnostics for CapturingDiagnostics {
    fn warn(&self, source: &str, message: &str, t: Time) {
        self.events
            .borrow_mut()
            .push((source.to_string(), message.to_string(), t));
    }
}

fn outs(b: &dyn Block, inputs: &[Value]) -> Vec<Value> {
    let mut v = Vec::new();
    let diag = NoopDiagnostics;
    let cx = Ctx::new(0.0, &diag);
    b.step_algebraic(&cx, inputs, &mut |idx, val| {
        assert_eq!(idx, v.len(), "outputs must be emitted in port-index order");
        v.push(val);
    });
    v
}

fn i(value: i64) -> Value {
    Value::Integer(value)
}

fn assert_values(actual: &[Value], expected: &[Value]) {
    assert_eq!(actual.len(), expected.len(), "value length mismatch");
    for (idx, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        assert!(
            actual.bit_eq(expected),
            "value {idx} mismatch: actual={actual:?} expected={expected:?}"
        );
    }
}

#[test]
fn integer_extract_signal_preserves_selector_order_and_duplicates() {
    let block = IntegerExtractSignal::new(5, 4, vec![5, 2, 2, 1]);
    let sig = block.resolved_signature();
    assert_eq!(sig.inputs.as_ref(), &[PortKind::Integer; 5]);
    assert_eq!(sig.outputs.as_ref(), &[PortKind::Integer; 4]);
    assert!(block.feeds_through(4, 0));
    assert!(block.feeds_through(1, 1));
    assert!(block.feeds_through(1, 2));
    assert!(block.feeds_through(0, 3));
    assert!(!block.feeds_through(2, 1));

    let out = outs(&block, &[i(10), i(20), i(30), i(40), i(50)]);
    assert_values(&out, &[i(50), i(20), i(20), i(10)]);
}

#[test]
fn integer_extract_signal_zero_width_and_invalid_direct_construction_do_not_panic() {
    let empty = IntegerExtractSignal::new(0, 0, Vec::new());
    assert!(empty.resolved_signature().inputs.is_empty());
    assert!(empty.resolved_signature().outputs.is_empty());
    assert!(outs(&empty, &[]).is_empty());

    let invalid_no_inputs = IntegerExtractSignal::new(0, 1, vec![1]);
    assert_values(&outs(&invalid_no_inputs, &[]), &[i(0)]);

    let invalid_selector = IntegerExtractSignal::new(2, 2, vec![3, 0]);
    assert_values(&outs(&invalid_selector, &[i(7), i(8)]), &[i(7), i(7)]);
}

#[test]
fn integer_extractor_warns_and_clamps_runtime_index_without_real_coercion() {
    let block = IntegerExtractor::new(3);
    let sig = block.resolved_signature();
    assert_eq!(
        sig.inputs.as_ref(),
        &[
            PortKind::Integer,
            PortKind::Integer,
            PortKind::Integer,
            PortKind::Integer,
        ]
    );
    assert_eq!(sig.outputs.as_ref(), &[PortKind::Integer]);
    for input in 0..4 {
        assert!(block.feeds_through(input, 0));
    }
    assert!(!block.feeds_through(4, 0));

    let values = [
        i((1_i64 << 53) + 1),
        i(-((1_i64 << 53) + 3)),
        i(i32::MAX as i64),
    ];
    let diag = CapturingDiagnostics::default();
    let cx = Ctx::new(12.5, &diag);
    let mut out = Vec::new();
    block.step_algebraic(
        &cx,
        &[
            i(2),
            values[0].clone(),
            values[1].clone(),
            values[2].clone(),
        ],
        &mut |idx, val| {
            assert_eq!(idx, out.len());
            out.push(val);
        },
    );
    assert_values(&out, &[values[1].clone()]);
    assert!(diag.events.borrow().is_empty());

    out.clear();
    block.step_algebraic(
        &cx,
        &[
            i(0),
            values[0].clone(),
            values[1].clone(),
            values[2].clone(),
        ],
        &mut |idx, val| {
            assert_eq!(idx, out.len());
            out.push(val);
        },
    );
    assert_values(&out, &[values[0].clone()]);

    out.clear();
    block.step_algebraic(
        &cx,
        &[
            i(4),
            values[0].clone(),
            values[1].clone(),
            values[2].clone(),
        ],
        &mut |idx, val| {
            assert_eq!(idx, out.len());
            out.push(val);
        },
    );
    assert_values(&out, &[values[2].clone()]);

    let events = diag.events.borrow();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].0, "CDL.Routing.IntegerExtractor");
    assert_eq!(events[0].1, "The extract index is out of the range.");
    assert_eq!(events[0].2.to_bits(), 12.5f64.to_bits());
}

#[test]
fn integer_extractor_zero_width_direct_construction_warns_and_emits_zero() {
    let block = IntegerExtractor::new(0);
    assert_eq!(
        block.resolved_signature().inputs.as_ref(),
        &[PortKind::Integer]
    );
    let diag = CapturingDiagnostics::default();
    let cx = Ctx::new(1.0, &diag);
    let mut out = Vec::new();
    block.step_algebraic(&cx, &[i(1)], &mut |idx, val| {
        assert_eq!(idx, out.len());
        out.push(val);
    });
    assert_values(&out, &[i(0)]);
    assert_eq!(diag.events.borrow().len(), 1);
}

#[test]
fn integer_scalar_replicator_fills_every_output_exactly() {
    let block = IntegerScalarReplicator::new(3);
    assert_eq!(
        block.resolved_signature().outputs.as_ref(),
        &[PortKind::Integer; 3]
    );
    assert!(block.feeds_through(0, 0));
    assert!(block.feeds_through(0, 2));
    assert!(!block.feeds_through(1, 0));
    let exact = (1_i64 << 53) + 5;
    assert_values(&outs(&block, &[i(exact)]), &[i(exact), i(exact), i(exact)]);

    let empty = IntegerScalarReplicator::new(0);
    assert!(empty.resolved_signature().outputs.is_empty());
    assert!(outs(&empty, &[i(1)]).is_empty());
}

#[test]
fn integer_vector_filter_preserves_true_mask_order() {
    let block = IntegerVectorFilter::new(4, 2, vec![false, true, true, false]);
    assert_eq!(
        block.resolved_signature().inputs.as_ref(),
        &[PortKind::Integer; 4]
    );
    assert_eq!(
        block.resolved_signature().outputs.as_ref(),
        &[PortKind::Integer; 2]
    );
    assert!(block.feeds_through(1, 0));
    assert!(block.feeds_through(2, 1));
    assert!(!block.feeds_through(0, 0));
    assert!(!block.feeds_through(3, 1));

    assert_values(&outs(&block, &[i(1), i(2), i(3), i(4)]), &[i(2), i(3)]);

    let all_false = IntegerVectorFilter::new(3, 0, vec![false, false, false]);
    assert!(all_false.resolved_signature().outputs.is_empty());
    assert!(outs(&all_false, &[i(1), i(2), i(3)]).is_empty());

    let invalid_mismatch = IntegerVectorFilter::new(2, 2, vec![true, false]);
    assert_values(&outs(&invalid_mismatch, &[i(5), i(6)]), &[i(5), i(0)]);
}

#[test]
fn integer_vector_replicator_flattens_matrix_outputs_row_major() {
    let block = IntegerVectorReplicator::new(2, 3);
    assert_eq!(
        block.resolved_signature().inputs.as_ref(),
        &[PortKind::Integer; 2]
    );
    assert_eq!(
        block.resolved_signature().outputs.as_ref(),
        &[PortKind::Integer; 6]
    );
    assert!(block.feeds_through(0, 0));
    assert!(block.feeds_through(1, 1));
    assert!(block.feeds_through(0, 2));
    assert!(block.feeds_through(1, 5));
    assert!(!block.feeds_through(1, 2));

    assert_values(
        &outs(&block, &[i(7), i(8)]),
        &[i(7), i(8), i(7), i(8), i(7), i(8)],
    );

    let empty_rows = IntegerVectorReplicator::new(2, 0);
    assert!(empty_rows.resolved_signature().outputs.is_empty());
    assert!(outs(&empty_rows, &[i(1), i(2)]).is_empty());

    let empty_cols = IntegerVectorReplicator::new(0, 3);
    assert!(empty_cols.resolved_signature().inputs.is_empty());
    assert!(empty_cols.resolved_signature().outputs.is_empty());
    assert!(outs(&empty_cols, &[]).is_empty());
}
