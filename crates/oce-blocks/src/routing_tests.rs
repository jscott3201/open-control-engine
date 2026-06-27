use std::cell::RefCell;

use oce_model::Value;

use super::{
    Block, Ctx, Diagnostics, NoopDiagnostics, PortKind, RealExtractSignal, RealExtractor,
    RealScalarReplicator, RealVectorFilter, RealVectorReplicator, Time,
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

fn r(value: f64) -> Value {
    Value::Real(value)
}

fn i(value: i64) -> Value {
    Value::Integer(value)
}

#[test]
fn real_extract_signal_preserves_selector_order_and_duplicates() {
    let block = RealExtractSignal::new(5, 4, vec![5, 2, 2, 1]);
    let sig = block.resolved_signature();
    assert_eq!(sig.inputs.as_ref(), &[PortKind::Real; 5]);
    assert_eq!(sig.outputs.as_ref(), &[PortKind::Real; 4]);
    assert!(block.feeds_through(4, 0));
    assert!(block.feeds_through(1, 1));
    assert!(block.feeds_through(1, 2));
    assert!(block.feeds_through(0, 3));
    assert!(!block.feeds_through(2, 1));

    let out = outs(&block, &[r(10.0), r(20.0), r(30.0), r(40.0), r(50.0)]);
    assert!(out[0].bit_eq(&r(50.0)));
    assert!(out[1].bit_eq(&r(20.0)));
    assert!(out[2].bit_eq(&r(20.0)));
    assert!(out[3].bit_eq(&r(10.0)));
}

#[test]
fn real_extract_signal_zero_width_and_invalid_direct_construction_do_not_panic() {
    let empty = RealExtractSignal::new(0, 0, Vec::new());
    assert!(empty.resolved_signature().inputs.is_empty());
    assert!(empty.resolved_signature().outputs.is_empty());
    assert!(outs(&empty, &[]).is_empty());

    let invalid_no_inputs = RealExtractSignal::new(0, 1, vec![1]);
    assert!(outs(&invalid_no_inputs, &[])[0].bit_eq(&r(0.0)));

    let invalid_selector = RealExtractSignal::new(2, 2, vec![3, 0]);
    let out = outs(&invalid_selector, &[r(7.0), r(8.0)]);
    assert!(out[0].bit_eq(&r(7.0)));
    assert!(out[1].bit_eq(&r(7.0)));
}

#[test]
fn real_extractor_warns_and_clamps_runtime_index() {
    let block = RealExtractor::new(3);
    let sig = block.resolved_signature();
    assert_eq!(
        sig.inputs.as_ref(),
        &[
            PortKind::Integer,
            PortKind::Real,
            PortKind::Real,
            PortKind::Real,
        ]
    );
    assert_eq!(sig.outputs.as_ref(), &[PortKind::Real]);
    for input in 0..4 {
        assert!(block.feeds_through(input, 0));
    }
    assert!(!block.feeds_through(4, 0));

    let diag = CapturingDiagnostics::default();
    let cx = Ctx::new(12.5, &diag);
    let mut out = Vec::new();
    block.step_algebraic(&cx, &[i(2), r(10.0), r(20.0), r(30.0)], &mut |idx, val| {
        assert_eq!(idx, out.len());
        out.push(val);
    });
    assert!(out[0].bit_eq(&r(20.0)));
    assert!(diag.events.borrow().is_empty());

    out.clear();
    block.step_algebraic(&cx, &[i(0), r(10.0), r(20.0), r(30.0)], &mut |idx, val| {
        assert_eq!(idx, out.len());
        out.push(val);
    });
    assert!(out[0].bit_eq(&r(10.0)));

    out.clear();
    block.step_algebraic(&cx, &[i(4), r(10.0), r(20.0), r(30.0)], &mut |idx, val| {
        assert_eq!(idx, out.len());
        out.push(val);
    });
    assert!(out[0].bit_eq(&r(30.0)));

    let events = diag.events.borrow();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].0, "CDL.Routing.RealExtractor");
    assert_eq!(events[0].1, "The extract index is out of the range.");
    assert_eq!(events[0].2.to_bits(), 12.5f64.to_bits());
}

#[test]
fn real_extractor_zero_width_direct_construction_warns_and_emits_zero() {
    let block = RealExtractor::new(0);
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
    assert!(out[0].bit_eq(&r(0.0)));
    assert_eq!(diag.events.borrow().len(), 1);
}

#[test]
fn real_scalar_replicator_fills_every_output() {
    let block = RealScalarReplicator::new(3);
    assert_eq!(
        block.resolved_signature().outputs.as_ref(),
        &[PortKind::Real; 3]
    );
    assert!(block.feeds_through(0, 0));
    assert!(block.feeds_through(0, 2));
    assert!(!block.feeds_through(1, 0));
    let out = outs(&block, &[r(-0.0)]);
    assert_eq!(out.len(), 3);
    assert!(out.iter().all(|value| value.bit_eq(&r(-0.0))));

    let empty = RealScalarReplicator::new(0);
    assert!(empty.resolved_signature().outputs.is_empty());
    assert!(outs(&empty, &[r(1.0)]).is_empty());
}

#[test]
fn real_vector_filter_preserves_true_mask_order() {
    let block = RealVectorFilter::new(4, 2, vec![false, true, true, false]);
    assert_eq!(
        block.resolved_signature().inputs.as_ref(),
        &[PortKind::Real; 4]
    );
    assert_eq!(
        block.resolved_signature().outputs.as_ref(),
        &[PortKind::Real; 2]
    );
    assert!(block.feeds_through(1, 0));
    assert!(block.feeds_through(2, 1));
    assert!(!block.feeds_through(0, 0));
    assert!(!block.feeds_through(3, 1));

    let out = outs(&block, &[r(1.0), r(2.0), r(3.0), r(4.0)]);
    assert!(out[0].bit_eq(&r(2.0)));
    assert!(out[1].bit_eq(&r(3.0)));

    let all_false = RealVectorFilter::new(3, 0, vec![false, false, false]);
    assert!(all_false.resolved_signature().outputs.is_empty());
    assert!(outs(&all_false, &[r(1.0), r(2.0), r(3.0)]).is_empty());

    let invalid_mismatch = RealVectorFilter::new(2, 2, vec![true, false]);
    let out = outs(&invalid_mismatch, &[r(5.0), r(6.0)]);
    assert!(out[0].bit_eq(&r(5.0)));
    assert!(out[1].bit_eq(&r(0.0)));
}

#[test]
fn real_vector_replicator_flattens_matrix_outputs_row_major() {
    let block = RealVectorReplicator::new(2, 3);
    assert_eq!(
        block.resolved_signature().inputs.as_ref(),
        &[PortKind::Real; 2]
    );
    assert_eq!(
        block.resolved_signature().outputs.as_ref(),
        &[PortKind::Real; 6]
    );
    assert!(block.feeds_through(0, 0));
    assert!(block.feeds_through(1, 1));
    assert!(block.feeds_through(0, 2));
    assert!(block.feeds_through(1, 5));
    assert!(!block.feeds_through(1, 2));

    let out = outs(&block, &[r(7.0), r(8.0)]);
    let expected = [7.0, 8.0, 7.0, 8.0, 7.0, 8.0];
    for (actual, expected) in out.iter().zip(expected) {
        assert!(actual.bit_eq(&r(expected)));
    }

    let empty_rows = RealVectorReplicator::new(2, 0);
    assert!(empty_rows.resolved_signature().outputs.is_empty());
    assert!(outs(&empty_rows, &[r(1.0), r(2.0)]).is_empty());

    let empty_cols = RealVectorReplicator::new(0, 3);
    assert!(empty_cols.resolved_signature().inputs.is_empty());
    assert!(empty_cols.resolved_signature().outputs.is_empty());
    assert!(outs(&empty_cols, &[]).is_empty());
}
