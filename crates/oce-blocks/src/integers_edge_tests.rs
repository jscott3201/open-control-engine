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

#[test]
fn integer_edge_and_counter_goldens() {
    let counter = OnCounter { y_start: 5 };
    let (trace, _) = run(
        &counter,
        &[
            (0.0, vec![Value::Boolean(false), Value::Boolean(false)]),
            (1.0, vec![Value::Boolean(true), Value::Boolean(false)]),
            (2.0, vec![Value::Boolean(true), Value::Boolean(false)]),
            (3.0, vec![Value::Boolean(false), Value::Boolean(false)]),
            (4.0, vec![Value::Boolean(true), Value::Boolean(true)]),
            (5.0, vec![Value::Boolean(false), Value::Boolean(true)]),
            (6.0, vec![Value::Boolean(false), Value::Boolean(false)]),
        ],
    );
    for (got, want) in trace.iter().zip([5, 5, 6, 6, 6, 5, 5]) {
        assert_int(&got[0], want);
    }

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
