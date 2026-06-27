//! Shared helpers for `oce-blocks` in-crate scenario tests.

pub(super) use std::cell::RefCell;
pub(super) use std::sync::Arc;

pub(super) use oce_model::{ParamTable, Value};

pub(super) use crate::{
    Abs, Add, AddParameter, And, Assert, Block, BlockKind, Constant, Ctx, Derivative, Diagnostics,
    Divide, Edge, Greater, GreaterThreshold, Hysteresis, IntegerStage, IntegratorWithReset, Less,
    LessThreshold, LimitSlewRate, Limiter, Line, Max, Min, MovingAverage, Multiply,
    MultiplyByParameter, NoopDiagnostics, Not, ParamRule, Pid, PidWithReset, Pre, Proof, Ramp,
    SampleTrigger, Subtract, Switch, Time, TriggeredSampler, UnitDelay, lookup, read_int,
};

#[derive(Default)]
pub(super) struct CapturingDiagnostics {
    pub(super) events: RefCell<Vec<(String, String, Time)>>,
}

impl Diagnostics for CapturingDiagnostics {
    fn warn(&self, source: &str, message: &str, t: Time) {
        self.events
            .borrow_mut()
            .push((source.to_string(), message.to_string(), t));
    }
}

/// Run an `[A]` block's `step_algebraic` and collect outputs in port-index order.
pub(super) fn outs(b: &dyn Block, inputs: &[Value]) -> Vec<Value> {
    let mut v = Vec::new();
    let diag = NoopDiagnostics;
    let cx = Ctx::new(0.0, &diag);
    b.step_algebraic(&cx, inputs, &mut |idx, val| {
        assert_eq!(idx, v.len(), "outputs must be emitted in port-index order");
        v.push(val);
    });
    v
}

/// Run an `[S]` block's `emit_from_state` and collect outputs.
pub(super) fn emit(b: &dyn Block, inputs: &[Value], region: &[u64]) -> Vec<Value> {
    let mut v = Vec::new();
    let diag = NoopDiagnostics;
    let cx = Ctx::new(0.0, &diag);
    b.emit_from_state(&cx, inputs, region, &mut |idx, val| {
        assert_eq!(idx, v.len());
        v.push(val);
    });
    v
}

/// Drive an `[S]` block across a sequence of `(inputs, t)` ticks, returning the single-output
/// Boolean trace. Mirrors the engine's per-tick order exactly (`01` §9): `emit_from_state` reads the
/// **prior** state, then `update_state` advances it. The block seeds its own state via `init_state`
/// (Edge from `pre_u_start`, SampleTrigger to `last_k = -1`), so the `ParamTable` is unused here.
pub(super) fn drive_bool(b: &dyn Block, steps: &[(Vec<Value>, Time)]) -> Vec<bool> {
    let mut region = vec![0u64; b.state_len()];
    b.init_state(&mut region, &ParamTable::default());
    let mut trace = Vec::with_capacity(steps.len());
    let diag = NoopDiagnostics;
    for (inputs, t) in steps {
        let cx = Ctx::new(*t, &diag);
        let mut out = None;
        b.emit_from_state(&cx, inputs, &region, &mut |idx, val| {
            assert_eq!(idx, 0, "single-output block emits only port 0");
            match val {
                Value::Boolean(x) => out = Some(x),
                other => panic!("expected Boolean output, got {other:?}"),
            }
        });
        b.update_state(&cx, inputs, &mut region);
        trace.push(out.expect("block must emit its output each tick"));
    }
    trace
}

/// A SampleTrigger source has no inputs; each tick is just a model time.
pub(super) fn ticks(times: &[Time]) -> Vec<(Vec<Value>, Time)> {
    times.iter().map(|t| (Vec::new(), *t)).collect()
}

/// A single-input Boolean block driven at `t = 0` for every tick (Edge is time-independent).
pub(super) fn bool_ticks(us: &[bool]) -> Vec<(Vec<Value>, Time)> {
    us.iter().map(|u| (vec![Value::Boolean(*u)], 0.0)).collect()
}
