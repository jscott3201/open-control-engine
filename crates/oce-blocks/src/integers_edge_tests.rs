use oce_model::{ParamTable, Value};

use super::{Block, Ctx, IntegerChange, NoopDiagnostics, OnCounter};

fn init_region(block: &dyn Block) -> Vec<u64> {
    let mut region = vec![0u64; block.state_len()];
    block.init_state(&mut region, &ParamTable::default());
    region
}

fn tick_at(block: &dyn Block, region: &mut [u64], t: f64, inputs: Vec<Value>) -> Vec<Value> {
    let diag = NoopDiagnostics;
    let cx = Ctx::new(t, &diag);
    let mut out = Vec::new();
    block.emit_from_state(&cx, &inputs, region, &mut |idx, val| {
        assert_eq!(idx, out.len(), "outputs must be emitted in port order");
        out.push(val);
    });
    block.update_state(&cx, &inputs, region);
    out
}

fn run(block: &dyn Block, steps: &[(f64, Vec<Value>)]) -> (Vec<Vec<Value>>, Vec<u64>) {
    let mut region = init_region(block);
    let trace = steps
        .iter()
        .map(|(t, inputs)| tick_at(block, &mut region, *t, inputs.clone()))
        .collect();
    (trace, region)
}

fn assert_bool(value: &Value, want: bool) {
    assert!(
        value.bit_eq(&Value::Boolean(want)),
        "got {value:?}, want {want}"
    );
}

fn assert_int(value: &Value, want: i64) {
    assert!(
        value.bit_eq(&Value::Integer(want)),
        "got {value:?}, want {want}"
    );
}

fn bool_inputs(trigger: bool, reset: bool) -> Vec<Value> {
    vec![Value::Boolean(trigger), Value::Boolean(reset)]
}

fn assert_int_trace(trace: &[Vec<Value>], expected: &[i64]) {
    assert_eq!(trace.len(), expected.len());
    for (got, want) in trace.iter().zip(expected) {
        assert_int(&got[0], *want);
    }
}

#[test]
fn integer_edge_and_counter_goldens() {
    let counter = OnCounter { y_start: 5 };
    let (trace, _) = run(
        &counter,
        &[
            (0.0, bool_inputs(false, false)),
            (1.0, bool_inputs(true, false)),
            (2.0, bool_inputs(true, false)),
            (3.0, bool_inputs(false, false)),
            (4.0, bool_inputs(true, true)),
            (5.0, bool_inputs(false, true)),
            (6.0, bool_inputs(false, false)),
        ],
    );
    assert_int_trace(&trace, &[5, 5, 6, 6, 6, 5, 5]);

    let change = IntegerChange { pre_u_start: 10 };
    let (trace, _) = run(
        &change,
        &[
            (0.0, vec![Value::Integer(10)]),
            (0.0, vec![Value::Integer(12)]),
            (0.0, vec![Value::Integer(9)]),
            (0.0, vec![Value::Integer(9)]),
        ],
    );
    let expected = [
        (false, false, false),
        (true, true, false),
        (true, false, true),
        (false, false, false),
    ];
    for (got, (changed, up, down)) in trace.iter().zip(expected) {
        assert_bool(&got[0], changed);
        assert_bool(&got[1], up);
        assert_bool(&got[2], down);
    }
}

#[test]
fn held_reset_level_suppresses_trigger_rising_edges() {
    let counter = OnCounter { y_start: 3 };
    let (trace, _) = run(
        &counter,
        &[
            (0.0, bool_inputs(false, true)),
            (1.0, bool_inputs(false, true)),
            (2.0, bool_inputs(true, true)),
            (3.0, bool_inputs(true, true)),
            (4.0, bool_inputs(false, true)),
            (5.0, bool_inputs(true, true)),
            (6.0, bool_inputs(true, true)),
        ],
    );
    assert_int_trace(&trace, &[3, 3, 3, 3, 3, 3, 3]);
}

#[test]
fn trigger_true_on_initial_tick_does_not_increment() {
    let counter = OnCounter { y_start: 2 };
    let (trace, _) = run(
        &counter,
        &[
            (0.0, bool_inputs(true, false)),
            (1.0, bool_inputs(true, false)),
            (2.0, bool_inputs(false, false)),
            (3.0, bool_inputs(true, false)),
            (4.0, bool_inputs(true, false)),
        ],
    );
    assert_int_trace(&trace, &[2, 2, 2, 2, 3]);
}

#[test]
fn simultaneous_trigger_and_reset_rise_stores_start_value() {
    let counter = OnCounter { y_start: 4 };
    let (trace, _) = run(
        &counter,
        &[
            (0.0, bool_inputs(false, false)),
            (1.0, bool_inputs(true, false)),
            (2.0, bool_inputs(false, false)),
            (3.0, bool_inputs(true, true)),
            (4.0, bool_inputs(true, true)),
        ],
    );
    assert_int_trace(&trace, &[4, 4, 5, 5, 4]);
}
