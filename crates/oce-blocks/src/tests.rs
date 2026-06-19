//! Per-block tests for the M0 starter catalog: feedthrough classification against the `03` §3/§5
//! table, algebraic step semantics, the loop-breakers' emit-then-latch cycle, and registry
//! resolution.

use std::cell::RefCell;
use std::sync::Arc;

use oce_model::{ParamTable, Value};

use super::{
    Abs, Add, AddParameter, And, Block, BlockKind, Constant, Ctx, Diagnostics, Divide, Edge,
    Greater, GreaterThreshold, Hysteresis, Less, LessThreshold, Limiter, Line, Max, Min, Multiply,
    MultiplyByParameter, NoopDiagnostics, Not, Pre, SampleTrigger, Subtract, Switch, Time,
    UnitDelay, lookup, read_int,
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

/// Run an `[A]` block's `step_algebraic` and collect outputs in port-index order.
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

/// Run an `[S]` block's `emit_from_state` and collect outputs.
fn emit(b: &dyn Block, inputs: &[Value], region: &[u64]) -> Vec<Value> {
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
fn drive_bool(b: &dyn Block, steps: &[(Vec<Value>, Time)]) -> Vec<bool> {
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
fn ticks(times: &[Time]) -> Vec<(Vec<Value>, Time)> {
    times.iter().map(|t| (Vec::new(), *t)).collect()
}

/// A single-input Boolean block driven at `t = 0` for every tick (Edge is time-independent).
fn bool_ticks(us: &[bool]) -> Vec<(Vec<Value>, Time)> {
    us.iter().map(|u| (vec![Value::Boolean(*u)], 0.0)).collect()
}

#[test]
fn ctx_warn_uses_scheduler_time_not_block_fabricated_time() {
    let diag = CapturingDiagnostics::default();
    let cx = Ctx::new(3.0, &diag);
    cx.warn("test.assert", "tripped");
    let events = diag.events.borrow();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, "test.assert");
    assert_eq!(events[0].1, "tripped");
    assert_eq!(events[0].2.to_bits(), 3.0f64.to_bits());
}

#[test]
fn read_int_reads_integer_and_release_degrades_to_zero() {
    assert_eq!(read_int(&[Value::Integer(42)], 0), 42);
    assert_eq!(read_int(&[Value::Integer(-7)], 0), -7);
    if cfg!(debug_assertions) {
        assert!(
            std::panic::catch_unwind(|| read_int(&[Value::Real(1.0)], 0)).is_err(),
            "debug builds must trip the validation-bug assertion"
        );
    } else {
        assert_eq!(read_int(&[Value::Real(1.0)], 0), 0);
    }
}

#[test]
fn feedthrough_classification_matches_spec() {
    // [A] math/logic blocks feed through every (in, out) pair; the two loop-breakers cut.
    assert!(Add.feeds_through(0, 0) && Add.feeds_through(1, 0));
    assert!(Subtract.feeds_through(0, 0) && Subtract.feeds_through(1, 0));
    assert!(Multiply.feeds_through(0, 0) && Multiply.feeds_through(1, 0));
    assert!(Divide.feeds_through(0, 0) && Divide.feeds_through(1, 0));
    assert!(AddParameter { p: 0.0 }.feeds_through(0, 0));
    assert!(MultiplyByParameter { k: 1.0 }.feeds_through(0, 0));
    assert!(Abs.feeds_through(0, 0));
    assert!(Min.feeds_through(0, 0) && Min.feeds_through(1, 0));
    assert!(Max.feeds_through(0, 0) && Max.feeds_through(1, 0));
    assert!(
        Limiter {
            u_min: 0.0,
            u_max: 1.0
        }
        .feeds_through(0, 0)
    );
    assert!(
        Line.feeds_through(0, 0)
            && Line.feeds_through(1, 0)
            && Line.feeds_through(2, 0)
            && Line.feeds_through(3, 0)
            && Line.feeds_through(4, 0)
    );
    assert!(Greater::default().feeds_through(0, 0) && Greater::default().feeds_through(1, 0));
    assert!(
        Greater {
            h: 1.0,
            pre_y_start: false
        }
        .feeds_through(0, 0)
            && Greater {
                h: 1.0,
                pre_y_start: false
            }
            .feeds_through(1, 0)
    );
    assert!(Less::default().feeds_through(0, 0) && Less::default().feeds_through(1, 0));
    assert!(GreaterThreshold::default().feeds_through(0, 0));
    assert!(LessThreshold::default().feeds_through(0, 0));
    assert!(Hysteresis::default().feeds_through(0, 0));
    assert!(And.feeds_through(0, 0) && And.feeds_through(1, 0));
    assert!(Not.feeds_through(0, 0));
    assert!(Switch.feeds_through(0, 0) && Switch.feeds_through(1, 0) && Switch.feeds_through(2, 0));

    assert!(!Constant { k: 0.0 }.feeds_through(0, 0)); // no inputs
    assert!(!Pre::default().feeds_through(0, 0)); // THE cut
    assert!(!UnitDelay::default().feeds_through(0, 0)); // discrete cut

    // Edge is stateful (owns `prev`) but FEEDS THROUGH on the current `u` — the edge is a function
    // of the current input vs the prior bit, so it is NOT a loop cut (`01` §11.2 req 3). Getting
    // this backwards would let the DAG scheduler treat it as a cut and corrupt the schedule.
    assert!(Edge::default().feeds_through(0, 0));
    assert_eq!(Edge::default().kind(), BlockKind::Stateful);
    // SampleTrigger is a stateful source: no inputs, so it does not feed through (Constant convention).
    assert!(!SampleTrigger::default().feeds_through(0, 0));
    assert_eq!(SampleTrigger::default().kind(), BlockKind::Stateful);
    assert!(SampleTrigger::default().signature().inputs.is_empty());
    assert_eq!(
        Greater {
            h: 1.0,
            pre_y_start: false
        }
        .kind(),
        BlockKind::Stateful
    );
    assert_eq!(Greater::default().kind(), BlockKind::Algebraic);
    assert_eq!(Hysteresis::default().kind(), BlockKind::Stateful);

    assert_eq!(Pre::default().kind(), BlockKind::Stateful);
    assert_eq!(UnitDelay::default().kind(), BlockKind::Stateful);
    assert_eq!(Add.kind(), BlockKind::Algebraic);
}

#[test]
fn arithmetic_blocks() {
    assert!(outs(&Constant { k: 3.5 }, &[])[0].bit_eq(&Value::Real(3.5)));
    assert!(outs(&Add, &[Value::Real(2.0), Value::Real(5.0)])[0].bit_eq(&Value::Real(7.0)));
    assert!(outs(&Subtract, &[Value::Real(2.0), Value::Real(5.0)])[0].bit_eq(&Value::Real(-3.0)));
    assert!(outs(&Multiply, &[Value::Real(2.0), Value::Real(5.0)])[0].bit_eq(&Value::Real(10.0)));
    assert!(outs(&Divide, &[Value::Real(10.0), Value::Real(4.0)])[0].bit_eq(&Value::Real(2.5)));
    assert!(outs(&AddParameter { p: 3.0 }, &[Value::Real(4.0)])[0].bit_eq(&Value::Real(7.0)));
    assert!(
        outs(&MultiplyByParameter { k: 2.0 }, &[Value::Real(4.0)])[0].bit_eq(&Value::Real(8.0))
    );
    assert!(outs(&Abs, &[Value::Real(-3.0)])[0].bit_eq(&Value::Real(3.0)));
    assert!(outs(&Min, &[Value::Real(2.0), Value::Real(5.0)])[0].bit_eq(&Value::Real(2.0)));
    assert!(outs(&Max, &[Value::Real(2.0), Value::Real(5.0)])[0].bit_eq(&Value::Real(5.0)));
    assert!(
        outs(
            &Line,
            &[
                Value::Real(0.0),
                Value::Real(2.0),
                Value::Real(4.0),
                Value::Real(10.0),
                Value::Real(1.5),
            ],
        )[0]
        .bit_eq(&Value::Real(5.0))
    );
}

#[test]
fn limiter_clips_and_tolerates_inverted_bounds() {
    let lim = Limiter {
        u_min: 0.0,
        u_max: 10.0,
    };
    assert!(outs(&lim, &[Value::Real(-5.0)])[0].bit_eq(&Value::Real(0.0)));
    assert!(outs(&lim, &[Value::Real(15.0)])[0].bit_eq(&Value::Real(10.0)));
    assert!(outs(&lim, &[Value::Real(5.0)])[0].bit_eq(&Value::Real(5.0)));
    // Inverted bounds must not panic; they degrade to u_max deterministically.
    let inv = Limiter {
        u_min: 10.0,
        u_max: 0.0,
    };
    assert!(outs(&inv, &[Value::Real(5.0)])[0].bit_eq(&Value::Real(0.0)));
}

#[test]
fn comparison_and_logical_blocks() {
    assert!(
        outs(&Greater::default(), &[Value::Real(3.0), Value::Real(2.0)])[0]
            .bit_eq(&Value::Boolean(true))
    );
    assert!(
        outs(&Greater::default(), &[Value::Real(2.0), Value::Real(2.0)])[0]
            .bit_eq(&Value::Boolean(false))
    );
    assert!(
        outs(&Less::default(), &[Value::Real(2.0), Value::Real(3.0)])[0]
            .bit_eq(&Value::Boolean(true))
    );
    assert!(
        outs(
            &GreaterThreshold {
                t: 2.5,
                h: 0.0,
                pre_y_start: false
            },
            &[Value::Real(2.5)]
        )[0]
        .bit_eq(&Value::Boolean(false))
    );
    assert!(
        outs(
            &LessThreshold {
                t: 2.5,
                h: 0.0,
                pre_y_start: false
            },
            &[Value::Real(2.4)]
        )[0]
        .bit_eq(&Value::Boolean(true))
    );
    assert!(
        outs(&And, &[Value::Boolean(true), Value::Boolean(false)])[0]
            .bit_eq(&Value::Boolean(false))
    );
    assert!(
        outs(&And, &[Value::Boolean(true), Value::Boolean(true)])[0].bit_eq(&Value::Boolean(true))
    );
    assert!(outs(&Not, &[Value::Boolean(false)])[0].bit_eq(&Value::Boolean(true)));
}

#[test]
fn switch_selects_on_the_middle_boolean() {
    let inputs_true = [Value::Real(1.0), Value::Boolean(true), Value::Real(9.0)];
    assert!(outs(&Switch, &inputs_true)[0].bit_eq(&Value::Real(1.0)));
    let inputs_false = [Value::Real(1.0), Value::Boolean(false), Value::Real(9.0)];
    assert!(outs(&Switch, &inputs_false)[0].bit_eq(&Value::Real(9.0)));
}

#[test]
fn pre_emits_prior_then_latches_current() {
    let pre = Pre { y_start: true };
    assert_eq!(pre.state_len(), 1);
    let mut region = vec![0u64; pre.state_len()];
    pre.init_state(&mut region, &ParamTable::default());

    // Emit returns the prior (seed) value before any update this tick.
    assert!(emit(&pre, &[Value::Boolean(false)], &region)[0].bit_eq(&Value::Boolean(true)));
    let diag = NoopDiagnostics;
    let cx = Ctx::new(0.0, &diag);
    pre.update_state(&cx, &[Value::Boolean(false)], &mut region);
    // Next tick: the latched `false` is emitted.
    assert!(emit(&pre, &[Value::Boolean(true)], &region)[0].bit_eq(&Value::Boolean(false)));
}

#[test]
fn unit_delay_holds_prior_sample() {
    let ud = UnitDelay { y_start: 2.5 };
    assert_eq!(ud.state_len(), 1);
    let mut region = vec![0u64; ud.state_len()];
    ud.init_state(&mut region, &ParamTable::default());

    assert!(emit(&ud, &[Value::Real(99.0)], &region)[0].bit_eq(&Value::Real(2.5))); // seed
    let diag = NoopDiagnostics;
    let cx = Ctx::new(0.0, &diag);
    ud.update_state(&cx, &[Value::Real(99.0)], &mut region);
    assert!(emit(&ud, &[Value::Real(7.0)], &region)[0].bit_eq(&Value::Real(99.0))); // prior sample
}

#[test]
fn registry_resolves_canonical_paths() {
    const PATHS: &[&str] = &[
        "CDL.Reals.Sources.Constant",
        "CDL.Reals.Add",
        "CDL.Reals.Subtract",
        "CDL.Reals.Multiply",
        "CDL.Reals.Divide",
        "CDL.Reals.AddParameter",
        "CDL.Reals.MultiplyByParameter",
        "CDL.Reals.Abs",
        "CDL.Reals.Min",
        "CDL.Reals.Max",
        "CDL.Reals.Limiter",
        "CDL.Reals.Line",
        "CDL.Reals.Greater",
        "CDL.Reals.GreaterThreshold",
        "CDL.Reals.Hysteresis",
        "CDL.Reals.Less",
        "CDL.Reals.LessThreshold",
        "CDL.Reals.Switch",
        "CDL.Logical.And",
        "CDL.Logical.Not",
        "CDL.Logical.Pre",
        "CDL.Logical.Edge",
        "CDL.Logical.Sources.SampleTrigger",
        "CDL.Discrete.UnitDelay",
    ];
    for path in PATHS {
        let entry = lookup(path).unwrap_or_else(|| panic!("missing catalog entry: {path}"));
        assert_eq!(entry.class_path, *path);
        // The constructor builds the matching class.
        let blk = (entry.make)(&ParamTable::default());
        assert_eq!(blk.signature().class_path, *path);
    }
    assert!(lookup("CDL.Reals.Nonexistent").is_none());
}

#[test]
fn registry_make_resolves_parameters() {
    let params = ParamTable {
        values: vec![(Arc::from("k"), Value::Real(4.0))],
    };
    let constant = (lookup("CDL.Reals.Sources.Constant").unwrap().make)(&params);
    assert!(outs(constant.as_ref(), &[])[0].bit_eq(&Value::Real(4.0)));

    let add_params = ParamTable {
        values: vec![(Arc::from("p"), Value::Real(2.5))],
    };
    let add_param = (lookup("CDL.Reals.AddParameter").unwrap().make)(&add_params);
    assert!(outs(add_param.as_ref(), &[Value::Real(1.5)])[0].bit_eq(&Value::Real(4.0)));

    let delay_params = ParamTable {
        values: vec![(Arc::from("y_start"), Value::Real(1.25))],
    };
    let delay = (lookup("CDL.Discrete.UnitDelay").unwrap().make)(&delay_params);
    let mut region = vec![0u64; delay.state_len()];
    delay.init_state(&mut region, &delay_params);
    assert!(emit(delay.as_ref(), &[Value::Real(0.0)], &region)[0].bit_eq(&Value::Real(1.25)));

    let greater_h = (lookup("CDL.Reals.Greater").unwrap().make)(&ParamTable {
        values: vec![(Arc::from("h"), Value::Real(1.0))],
    });
    assert_eq!(greater_h.kind(), BlockKind::Stateful);
    assert_eq!(greater_h.state_len(), 1);

    let hysteresis = (lookup("CDL.Reals.Hysteresis").unwrap().make)(&ParamTable {
        values: vec![
            (Arc::from("uLow"), Value::Real(2.0)),
            (Arc::from("uHigh"), Value::Real(5.0)),
            (Arc::from("pre_y_start"), Value::Boolean(true)),
        ],
    });
    assert_eq!(hysteresis.kind(), BlockKind::Stateful);
    assert_eq!(
        drive_bool(hysteresis.as_ref(), &[(vec![Value::Real(3.0)], 0.0)]),
        vec![true],
        "pre_y_start=true must seed the initial hold state"
    );
}

#[test]
fn real_param_promotes_integer_to_real() {
    // Modelica/CDL Int→Real promotion: an integer literal bound to a `Real` parameter is its real
    // value, NOT silently dropped to the constructor default. CXF can carry a bare integer for a
    // Real parameter (no `isOfDataType` re-types it), so a non-zero integer `y_start`/`k` must reach
    // the block. Tripwire for the silent-wrong-initial-state hole (M1-PR-5 review C-3).
    let k_int = ParamTable {
        values: vec![(Arc::from("k"), Value::Integer(5))],
    };
    let constant = (lookup("CDL.Reals.Sources.Constant").unwrap().make)(&k_int);
    assert!(
        outs(constant.as_ref(), &[])[0].bit_eq(&Value::Real(5.0)),
        "Integer(5) bound to Real param k must promote to 5.0, not default to 0.0"
    );

    // A non-zero integer UnitDelay.y_start must seed the loop-breaker's initial output to 5.0.
    let y_int = ParamTable {
        values: vec![(Arc::from("y_start"), Value::Integer(5))],
    };
    let delay = (lookup("CDL.Discrete.UnitDelay").unwrap().make)(&y_int);
    let mut region = vec![0u64; delay.state_len()];
    delay.init_state(&mut region, &y_int);
    assert!(
        emit(delay.as_ref(), &[Value::Real(0.0)], &region)[0].bit_eq(&Value::Real(5.0)),
        "Integer(5) y_start must seed the initial output to 5.0, not silently default to 0.0"
    );
}

// ---- Edge (rising-edge detector, `01` §11.2) -----------------------------------------------

#[test]
fn edge_detects_rising_edges_golden() {
    // pre_u_start defaults to false, so a `u` already true on tick 0 IS a rising edge (`01` §11.2
    // req 2). Golden output trace for a hand-traced input sequence.
    let edge = Edge::default();
    let trace = drive_bool(
        &edge,
        &bool_ticks(&[false, true, true, false, true, false, false, true]),
    );
    //                          F->T            F->T              F->T
    assert_eq!(
        trace,
        vec![false, true, false, false, true, false, false, true]
    );

    // u already true on tick 0, default seed false => edge on tick 0.
    assert_eq!(
        drive_bool(&Edge::default(), &bool_ticks(&[true])),
        vec![true]
    );
}

#[test]
fn edge_seed_suppresses_initial_edge() {
    // pre_u_start = true means the prior bit is already true, so a true on tick 0 is NOT a new edge.
    let edge = Edge { pre_u_start: true };
    let trace = drive_bool(&edge, &bool_ticks(&[true, true, false, true]));
    //                                          held    fall   F->T
    assert_eq!(trace, vec![false, false, false, true]);
}

#[test]
fn edge_emit_is_pure_with_respect_to_state() {
    // emit_from_state must NOT mutate the region (`01` §9 req 2): two emits with no update between
    // are identical. This is what keeps the two-pass tick correct for a feedthrough `[S]` block.
    let edge = Edge::default();
    let mut region = vec![0u64; edge.state_len()];
    edge.init_state(&mut region, &ParamTable::default());
    let a = emit(&edge, &[Value::Boolean(true)], &region);
    let b = emit(&edge, &[Value::Boolean(true)], &region);
    assert!(a[0].bit_eq(&b[0]) && a[0].bit_eq(&Value::Boolean(true)));
}

// ---- SampleTrigger (periodic sample clock, `01` §11.1) -------------------------------------

#[test]
fn sample_trigger_fires_on_each_period_boundary_golden() {
    // period = 2, shift = 0, host cadence == period: fire at every even instant.
    let st = SampleTrigger {
        period: 2.0,
        shift: 0.0,
    };
    let trace = drive_bool(&st, &ticks(&[0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0]));
    assert_eq!(trace, vec![true, false, true, false, true, false, true]);
}

#[test]
fn sample_trigger_finer_cadence_fires_only_on_crossing() {
    // Host cadence finer than period: fire only on the tick that crosses a new boundary, not every
    // tick (`01` §11.1 req 1).
    let st = SampleTrigger {
        period: 2.0,
        shift: 0.0,
    };
    let trace = drive_bool(&st, &ticks(&[0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0]));
    assert_eq!(trace, vec![true, false, false, false, true, false, false]);
}

#[test]
fn sample_trigger_coarse_tick_fires_once_and_snaps_to_latest() {
    // A single coarse tick spanning several boundaries fires ONCE and snaps `last_k` to the current
    // k — no sub-tick replay (`01` §11.1 req 2).
    let st = SampleTrigger {
        period: 1.0,
        shift: 0.0,
    };
    let trace = drive_bool(&st, &ticks(&[0.0, 5.0, 5.5, 6.0]));
    //                              k0   k5(snap) hold  k6
    assert_eq!(trace, vec![true, true, false, true]);
}

#[test]
fn sample_trigger_respects_shift_and_never_fires_before_it() {
    // period = 2, shift = 1: no sample before t = 1; first fire at t = 1 (k = 0), next at t = 3.
    let st = SampleTrigger {
        period: 2.0,
        shift: 1.0,
    };
    let trace = drive_bool(&st, &ticks(&[0.0, 0.5, 1.0, 2.0, 3.0]));
    assert_eq!(trace, vec![false, false, true, false, true]);
}

#[test]
fn sample_trigger_epsilon_makes_boundaries_deterministic() {
    // period = 0.1: 0.3 is not exactly representable — 0.3/0.1 == 2.9999999999999996, so a bare floor
    // yields k=2 and the t=0.3 sample would be silently skipped (a safety-critical wrong-value class).
    // The fixed boundary epsilon (`01` §11.1 req 3) lifts it to k=3. (0.2/0.1 == exactly 2.0, so the
    // t=0.2 sample needs no rescue.) Must be all-true.
    let st = SampleTrigger {
        period: 0.1,
        shift: 0.0,
    };
    let trace = drive_bool(&st, &ticks(&[0.0, 0.1, 0.2, 0.3]));
    assert_eq!(
        trace,
        vec![true, true, true, true],
        "boundary epsilon must catch every instant despite f64 rounding"
    );
}

#[test]
fn sample_trigger_saturates_at_extreme_horizon_without_panic() {
    // At an out-of-range horizon the period-normalized floor exceeds i64 range; the `f64 -> i64` cast
    // SATURATES to i64::MAX (no UB, no panic). The trigger fires once at the first saturated tick,
    // then never again — every further instant collapses to the same un-representable index.
    let st = SampleTrigger {
        period: 1.0,
        shift: 0.0,
    };
    let trace = drive_bool(&st, &ticks(&[0.0, 1e30, 1e31]));
    assert_eq!(trace, vec![true, true, false]);
}

#[test]
fn sample_trigger_degrades_safely_on_nonpositive_or_nan_period() {
    // `period > 0` is required by CDL but not yet enforced by oce-validate; a non-positive or NaN
    // period must NOT panic (period is input-derived; exit #6) and must degrade deterministically to
    // "one sample at/after shift, then never". This exercises the assert-free degraded branch.
    for bad_period in [0.0, -1.0, f64::NAN] {
        let st = SampleTrigger {
            period: bad_period,
            shift: 0.0,
        };
        assert_eq!(
            drive_bool(&st, &ticks(&[0.0, 1.0, 2.0])),
            vec![true, false, false],
            "degraded period={bad_period} must fire once at/after shift then never, panic-free"
        );
    }
}

#[test]
fn sample_trigger_is_deterministic_across_runs() {
    // Determinism golden: two independent drives over the same time sequence are bit-identical
    // (TESTING.md determinism standard) and match the expected trace.
    let st = SampleTrigger {
        period: 2.0,
        shift: 0.0,
    };
    let seq = ticks(&[0.0, 1.0, 2.0, 3.0, 4.0]);
    let run1 = drive_bool(&st, &seq);
    let run2 = drive_bool(&st, &seq);
    assert_eq!(
        run1, run2,
        "SampleTrigger must be bit-identical across runs"
    );
    assert_eq!(run1, vec![true, false, true, false, true]);
}

#[test]
fn registry_constructs_edge_and_sample_trigger_with_params() {
    // Edge: pre_u_start latches the seed, suppressing the tick-0 edge for a u already true.
    let edge_hi = (lookup("CDL.Logical.Edge").unwrap().make)(&ParamTable {
        values: vec![(Arc::from("pre_u_start"), Value::Boolean(true))],
    });
    assert_eq!(
        drive_bool(edge_hi.as_ref(), &bool_ticks(&[true, false, true])),
        vec![false, false, true]
    );

    // SampleTrigger: period/shift resolve from the ParamTable; period=2, shift=1 first fires at t=1.
    let st = (lookup("CDL.Logical.Sources.SampleTrigger").unwrap().make)(&ParamTable {
        values: vec![
            (Arc::from("period"), Value::Real(2.0)),
            (Arc::from("shift"), Value::Real(1.0)),
        ],
    });
    assert_eq!(
        drive_bool(st.as_ref(), &ticks(&[0.0, 1.0, 2.0, 3.0])),
        vec![false, true, false, true]
    );
}

/// Compile-time guard (R-API-PY-2): the `Block` trait object is `Send + Sync`, localized to
/// oce-blocks' own boundary. The `Block: Send + Sync` supertrait already forces every `impl Block`
/// to be `Send + Sync` at its impl site, so a future non-`Send` block class fails to compile; this
/// also pins the **trait object** (`dyn Block` / `Box<dyn Block>`) so the engine's
/// `Vec<Box<dyn Block>>` stays shareable. Never called — its compilation IS the assertion.
#[allow(dead_code)]
fn _assert_block_object_send_sync() {
    fn needs<T: Send + Sync + ?Sized>() {}
    needs::<dyn Block>();
    needs::<Box<dyn Block>>();
}
