use std::cell::RefCell;

use oce_model::Value;

use super::{
    Block, BooleanExtractSignal, BooleanExtractor, BooleanScalarReplicator, BooleanVectorFilter,
    BooleanVectorReplicator, Ctx, Diagnostics, NoopDiagnostics, PortKind, Time,
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

fn b(value: bool) -> Value {
    Value::Boolean(value)
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
fn boolean_extract_signal_preserves_selector_order_and_duplicates() {
    let block = BooleanExtractSignal::new(5, 4, vec![5, 2, 2, 1]);
    let sig = block.resolved_signature();
    assert_eq!(sig.inputs.as_ref(), &[PortKind::Boolean; 5]);
    assert_eq!(sig.outputs.as_ref(), &[PortKind::Boolean; 4]);
    assert!(block.feeds_through(4, 0));
    assert!(block.feeds_through(1, 1));
    assert!(block.feeds_through(1, 2));
    assert!(block.feeds_through(0, 3));
    assert!(!block.feeds_through(2, 1));

    let out = outs(&block, &[b(true), b(false), b(true), b(false), b(true)]);
    assert_values(&out, &[b(true), b(false), b(false), b(true)]);
}

#[test]
fn boolean_extract_signal_zero_width_and_invalid_direct_construction_do_not_panic() {
    let empty = BooleanExtractSignal::new(0, 0, Vec::new());
    assert!(empty.resolved_signature().inputs.is_empty());
    assert!(empty.resolved_signature().outputs.is_empty());
    assert!(outs(&empty, &[]).is_empty());

    let invalid_no_inputs = BooleanExtractSignal::new(0, 1, vec![1]);
    assert_values(&outs(&invalid_no_inputs, &[]), &[b(false)]);

    let invalid_selector = BooleanExtractSignal::new(2, 2, vec![3, 0]);
    assert_values(
        &outs(&invalid_selector, &[b(true), b(false)]),
        &[b(true), b(true)],
    );
}

#[test]
fn boolean_extractor_warns_and_clamps_runtime_index() {
    let block = BooleanExtractor::new(3);
    let sig = block.resolved_signature();
    assert_eq!(
        sig.inputs.as_ref(),
        &[
            PortKind::Integer,
            PortKind::Boolean,
            PortKind::Boolean,
            PortKind::Boolean,
        ]
    );
    assert_eq!(sig.outputs.as_ref(), &[PortKind::Boolean]);
    for input in 0..4 {
        assert!(block.feeds_through(input, 0));
    }
    assert!(!block.feeds_through(4, 0));

    let diag = CapturingDiagnostics::default();
    let cx = Ctx::new(12.5, &diag);
    let mut out = Vec::new();
    block.step_algebraic(
        &cx,
        &[i(2), b(false), b(true), b(false)],
        &mut |idx, val| {
            assert_eq!(idx, out.len());
            out.push(val);
        },
    );
    assert_values(&out, &[b(true)]);
    assert!(diag.events.borrow().is_empty());

    out.clear();
    block.step_algebraic(
        &cx,
        &[i(0), b(false), b(true), b(false)],
        &mut |idx, val| {
            assert_eq!(idx, out.len());
            out.push(val);
        },
    );
    assert_values(&out, &[b(false)]);

    out.clear();
    block.step_algebraic(
        &cx,
        &[i(4), b(false), b(true), b(false)],
        &mut |idx, val| {
            assert_eq!(idx, out.len());
            out.push(val);
        },
    );
    assert_values(&out, &[b(false)]);

    let events = diag.events.borrow();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].0, "CDL.Routing.BooleanExtractor");
    assert_eq!(events[0].1, "The extract index is out of the range.");
    assert_eq!(events[0].2.to_bits(), 12.5f64.to_bits());
}

#[test]
fn boolean_extractor_zero_width_direct_construction_warns_and_emits_false() {
    let block = BooleanExtractor::new(0);
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
    assert_values(&out, &[b(false)]);
    assert_eq!(diag.events.borrow().len(), 1);
}

#[test]
fn boolean_scalar_replicator_fills_every_output() {
    let block = BooleanScalarReplicator::new(3);
    assert_eq!(
        block.resolved_signature().outputs.as_ref(),
        &[PortKind::Boolean; 3]
    );
    assert!(block.feeds_through(0, 0));
    assert!(block.feeds_through(0, 2));
    assert!(!block.feeds_through(1, 0));
    assert_values(&outs(&block, &[b(true)]), &[b(true), b(true), b(true)]);

    let empty = BooleanScalarReplicator::new(0);
    assert!(empty.resolved_signature().outputs.is_empty());
    assert!(outs(&empty, &[b(true)]).is_empty());
}

#[test]
fn boolean_vector_filter_preserves_true_mask_order() {
    let block = BooleanVectorFilter::new(4, 2, vec![false, true, true, false]);
    assert_eq!(
        block.resolved_signature().inputs.as_ref(),
        &[PortKind::Boolean; 4]
    );
    assert_eq!(
        block.resolved_signature().outputs.as_ref(),
        &[PortKind::Boolean; 2]
    );
    assert!(block.feeds_through(1, 0));
    assert!(block.feeds_through(2, 1));
    assert!(!block.feeds_through(0, 0));
    assert!(!block.feeds_through(3, 1));

    assert_values(
        &outs(&block, &[b(true), b(false), b(true), b(false)]),
        &[b(false), b(true)],
    );

    let all_false = BooleanVectorFilter::new(3, 0, vec![false, false, false]);
    assert!(all_false.resolved_signature().outputs.is_empty());
    assert!(outs(&all_false, &[b(true), b(false), b(true)]).is_empty());

    let invalid_mismatch = BooleanVectorFilter::new(2, 2, vec![true, false]);
    assert_values(
        &outs(&invalid_mismatch, &[b(true), b(false)]),
        &[b(true), b(false)],
    );
}

#[test]
fn boolean_vector_replicator_flattens_matrix_outputs_row_major() {
    let block = BooleanVectorReplicator::new(2, 3);
    assert_eq!(
        block.resolved_signature().inputs.as_ref(),
        &[PortKind::Boolean; 2]
    );
    assert_eq!(
        block.resolved_signature().outputs.as_ref(),
        &[PortKind::Boolean; 6]
    );
    assert!(block.feeds_through(0, 0));
    assert!(block.feeds_through(1, 1));
    assert!(block.feeds_through(0, 2));
    assert!(block.feeds_through(1, 5));
    assert!(!block.feeds_through(1, 2));

    assert_values(
        &outs(&block, &[b(true), b(false)]),
        &[b(true), b(false), b(true), b(false), b(true), b(false)],
    );

    let empty_rows = BooleanVectorReplicator::new(2, 0);
    assert!(empty_rows.resolved_signature().outputs.is_empty());
    assert!(outs(&empty_rows, &[b(true), b(false)]).is_empty());

    let empty_cols = BooleanVectorReplicator::new(0, 3);
    assert!(empty_cols.resolved_signature().inputs.is_empty());
    assert!(empty_cols.resolved_signature().outputs.is_empty());
    assert!(outs(&empty_cols, &[]).is_empty());
}
