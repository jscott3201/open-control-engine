use super::common::*;

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
            &Line::default(),
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
    let ud = UnitDelay {
        y_start: 2.5,
        sample_period: 1.0,
    };
    assert_eq!(ud.state_len(), 4);
    let mut region = vec![0u64; ud.state_len()];
    ud.init_state(&mut region, &ParamTable::default());

    assert!(emit(&ud, &[Value::Real(99.0)], &region)[0].bit_eq(&Value::Real(2.5))); // seed
    let diag = NoopDiagnostics;
    let cx = Ctx::new(0.0, &diag);
    ud.update_state(&cx, &[Value::Real(99.0)], &mut region);
    assert!(emit(&ud, &[Value::Real(7.0)], &region)[0].bit_eq(&Value::Real(99.0))); // prior sample
}

#[test]
fn unit_delay_holds_between_nondefault_sample_instants() {
    let ud = UnitDelay {
        y_start: 0.0,
        sample_period: 2.0,
    };
    let mut region = vec![0u64; ud.state_len()];
    ud.init_state(&mut region, &ParamTable::default());
    let diag = NoopDiagnostics;

    let cx = Ctx::new(0.0, &diag);
    assert!(emit(&ud, &[Value::Real(10.0)], &region)[0].bit_eq(&Value::Real(0.0)));
    ud.update_state(&cx, &[Value::Real(10.0)], &mut region);

    let cx = Ctx::new(1.0, &diag);
    assert!(emit(&ud, &[Value::Real(20.0)], &region)[0].bit_eq(&Value::Real(10.0)));
    ud.update_state(&cx, &[Value::Real(20.0)], &mut region);

    let cx = Ctx::new(2.0, &diag);
    assert!(emit(&ud, &[Value::Real(30.0)], &region)[0].bit_eq(&Value::Real(10.0)));
    ud.update_state(&cx, &[Value::Real(30.0)], &mut region);

    let cx = Ctx::new(3.0, &diag);
    assert!(emit(&ud, &[Value::Real(40.0)], &region)[0].bit_eq(&Value::Real(30.0)));
    ud.update_state(&cx, &[Value::Real(40.0)], &mut region);
}

#[test]
fn unit_delay_real_output_canonicalizes_held_nan_bits() {
    let ud = UnitDelay {
        y_start: f64::from_bits(0xfff8_0000_0000_0000),
        sample_period: 1.0,
    };
    let mut region = vec![0u64; ud.state_len()];
    ud.init_state(&mut region, &ParamTable::default());

    assert!(
        emit(&ud, &[Value::Real(0.0)], &region)[0]
            .bit_eq(&Value::Real(f64::from_bits(0x7ff8_0000_0000_0000)))
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
fn sample_trigger_shift_after_period_folds_to_zero_phase() {
    // Buildings SampleTrigger.mo anchors t0 at mod(shift, period), so period=1, shift=2 folds to
    // phase=0 and fires at every integer tick from t=0. A raw-shift implementation would skip t=0
    // and t=1.
    let st = SampleTrigger {
        period: 1.0,
        shift: 2.0,
    };
    let trace = drive_bool(&st, &ticks(&[0.0, 1.0, 2.0, 3.0, 4.0, 5.0]));
    assert_eq!(trace, vec![true, true, true, true, true, true]);
}

#[test]
fn sample_trigger_negative_shift_folds_to_positive_phase() {
    // Modelica mod is floored: mod(-0.5, 1.0)=0.5, so samples fire at t=0.5, 1.5, ...
    // A raw-shift implementation would incorrectly fire at t=0.
    let st = SampleTrigger {
        period: 1.0,
        shift: -0.5,
    };
    let trace = drive_bool(&st, &ticks(&[0.0, 0.5, 1.0, 1.5, 2.0]));
    assert_eq!(trace, vec![false, true, false, true, false]);
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
