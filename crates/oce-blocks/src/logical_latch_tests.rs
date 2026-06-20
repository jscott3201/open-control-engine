use oce_model::{ParamTable, Value};

use super::{Block, Ctx, FallingEdge, Latch, LogicalChange, NoopDiagnostics, Toggle};

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

#[test]
fn logical_edge_latch_goldens() {
    let falling = FallingEdge::default();
    let (trace, _) = run(
        &falling,
        &[
            (0.0, vec![Value::Boolean(false)]),
            (0.0, vec![Value::Boolean(true)]),
            (0.0, vec![Value::Boolean(false)]),
            (0.0, vec![Value::Boolean(false)]),
        ],
    );
    for (got, want) in trace.iter().zip([false, false, true, false]) {
        assert_bool(&got[0], want);
    }

    let seeded = FallingEdge { pre_u_start: true };
    let (trace, _) = run(&seeded, &[(0.0, vec![Value::Boolean(false)])]);
    assert_bool(&trace[0][0], true);

    let change = LogicalChange { pre_u_start: true };
    let (trace, _) = run(
        &change,
        &[
            (0.0, vec![Value::Boolean(true)]),
            (0.0, vec![Value::Boolean(false)]),
            (0.0, vec![Value::Boolean(false)]),
            (0.0, vec![Value::Boolean(true)]),
        ],
    );
    for (got, want) in trace.iter().zip([false, true, false, true]) {
        assert_bool(&got[0], want);
    }

    let latch = Latch;
    let (trace, _) = run(
        &latch,
        &[
            (0.0, vec![Value::Boolean(true), Value::Boolean(true)]),
            (0.0, vec![Value::Boolean(false), Value::Boolean(false)]),
            (0.0, vec![Value::Boolean(true), Value::Boolean(false)]),
            (0.0, vec![Value::Boolean(false), Value::Boolean(false)]),
            (0.0, vec![Value::Boolean(false), Value::Boolean(true)]),
        ],
    );
    for (got, want) in trace.iter().zip([false, false, true, true, false]) {
        assert_bool(&got[0], want);
    }

    let toggle = Toggle;
    let (trace, _) = run(
        &toggle,
        &[
            (0.0, vec![Value::Boolean(true), Value::Boolean(false)]),
            (0.0, vec![Value::Boolean(true), Value::Boolean(false)]),
            (0.0, vec![Value::Boolean(false), Value::Boolean(false)]),
            (0.0, vec![Value::Boolean(true), Value::Boolean(false)]),
            (0.0, vec![Value::Boolean(true), Value::Boolean(true)]),
        ],
    );
    for (got, want) in trace.iter().zip([true, true, true, false, false]) {
        assert_bool(&got[0], want);
    }
}
