use super::*;

fn assert_bool_trace(got: &[Value], want: &[bool]) {
    assert_eq!(got.len(), want.len());
    for (idx, (got, want)) in got.iter().zip(want).enumerate() {
        assert!(
            got.bit_eq(&Value::Boolean(*want)),
            "trace[{idx}] got {got:?}, want {want}"
        );
    }
}

fn greater_h_to_not_trace(inputs: &[f64]) -> (Vec<Value>, Vec<Value>) {
    let mut b = ModelBuilder::default();
    let (_zero, _, zero_out) = b.block_real(make(
        "CDL.Reals.Sources.Constant",
        &[("k", Value::Real(0.0))],
    ));
    let (_add, add_in, add_out) = b.block_real(make("CDL.Reals.Add", &[]));
    b.connect(zero_out[0], add_in[1]);
    let (_threshold, _, threshold_out) = b.block_real(make(
        "CDL.Reals.Sources.Constant",
        &[("k", Value::Real(10.0))],
    ));
    let (_greater, greater_in, greater_out) =
        b.block_real(make("CDL.Reals.Greater", &[("h", Value::Real(2.0))]));
    b.connect(add_out[0], greater_in[0]);
    b.connect(threshold_out[0], greater_in[1]);
    let (_not, not_in, not_out) = b.block_real(make("CDL.Logical.Not", &[]));
    b.connect(greater_out[0], not_in[0]);

    let sched = compile(&b.model, &b.blocks).unwrap();
    let mut state = allocate_state(&b.model, &b.blocks);
    let mut greater_trace = Vec::with_capacity(inputs.len());
    let mut not_trace = Vec::with_capacity(inputs.len());
    for (k, &u) in inputs.iter().enumerate() {
        state.values[add_in[0].0 as usize] = Value::Real(u);
        tick_once(&b.model, &sched, &b.blocks, &mut state, k as f64);
        greater_trace.push(state.values[greater_out[0].0 as usize].clone());
        not_trace.push(state.values[not_out[0].0 as usize].clone());
    }
    (greater_trace, not_trace)
}

fn hysteresis_to_not_trace(inputs: &[f64]) -> (Vec<Value>, Vec<Value>) {
    let mut b = ModelBuilder::default();
    let (_zero, _, zero_out) = b.block_real(make(
        "CDL.Reals.Sources.Constant",
        &[("k", Value::Real(0.0))],
    ));
    let (_add, add_in, add_out) = b.block_real(make("CDL.Reals.Add", &[]));
    b.connect(zero_out[0], add_in[1]);
    let (_hyst, hyst_in, hyst_out) = b.block_real(make(
        "CDL.Reals.Hysteresis",
        &[("uLow", Value::Real(8.0)), ("uHigh", Value::Real(10.0))],
    ));
    b.connect(add_out[0], hyst_in[0]);
    let (_not, not_in, not_out) = b.block_real(make("CDL.Logical.Not", &[]));
    b.connect(hyst_out[0], not_in[0]);

    let sched = compile(&b.model, &b.blocks).unwrap();
    let mut state = allocate_state(&b.model, &b.blocks);
    let mut hyst_trace = Vec::with_capacity(inputs.len());
    let mut not_trace = Vec::with_capacity(inputs.len());
    for (k, &u) in inputs.iter().enumerate() {
        state.values[add_in[0].0 as usize] = Value::Real(u);
        tick_once(&b.model, &sched, &b.blocks, &mut state, k as f64);
        hyst_trace.push(state.values[hyst_out[0].0 as usize].clone());
        not_trace.push(state.values[not_out[0].0 as usize].clone());
    }
    (hyst_trace, not_trace)
}

#[test]
fn feedthrough_stateful_comparators_emit_current_inputs_and_hold_across_ticks() {
    let inputs = [9.0, 10.5, 9.0, 8.0, 10.5];
    let (greater, not) = greater_h_to_not_trace(&inputs);
    assert_bool_trace(&greater, &[false, true, true, false, true]);
    assert_bool_trace(&not, &[true, false, false, true, false]);
    let (greater2, not2) = greater_h_to_not_trace(&inputs);
    for (idx, (a, b)) in greater.iter().zip(&greater2).enumerate() {
        assert!(a.bit_eq(b), "Greater trace changed at {idx}");
    }
    for (idx, (a, b)) in not.iter().zip(&not2).enumerate() {
        assert!(a.bit_eq(b), "Not trace changed at {idx}");
    }

    let hyst_inputs = [9.0, 10.5, 9.0, 8.0, 7.9, 10.5];
    let (hyst, not) = hysteresis_to_not_trace(&hyst_inputs);
    assert_bool_trace(&hyst, &[false, true, true, true, false, true]);
    assert_bool_trace(&not, &[true, false, false, false, true, false]);
}

#[test]
fn h0_comparator_allocates_no_state_through_real_graph_build() {
    let mut b = ModelBuilder::default();
    let (greater, _, _) = b.block_real(make("CDL.Reals.Greater", &[]));
    let sched = compile(&b.model, &b.blocks).unwrap();
    let state = allocate_state(&b.model, &b.blocks);
    let block = b.blocks[greater.0 as usize].as_ref();
    assert!(
        block.signature().stateful,
        "class hint remains stateful-capable"
    );
    assert_eq!(block.kind(), BlockKind::Algebraic);
    assert_eq!(block.state_len(), 0);
    assert!(state.words.is_empty());
    assert!(state.slots.is_empty());
    assert_eq!(state.slot_of[greater.0 as usize], usize::MAX);
    assert_eq!(sched.order, vec![greater]);
}
