use super::common::*;

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
    assert!(!IntegratorWithReset::default().feeds_through(0, 0)); // integrating loop cut
    assert!(!IntegratorWithReset::default().feeds_through(1, 0)); // reset value is delayed
    assert!(!IntegratorWithReset::default().feeds_through(2, 0)); // trigger is delayed

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
    assert_eq!(IntegratorWithReset::default().kind(), BlockKind::Stateful);
    assert_eq!(Derivative::default().kind(), BlockKind::Stateful);
    assert!(Derivative::default().feeds_through(0, 0));
    assert_eq!(LimitSlewRate::default().kind(), BlockKind::Stateful);
    assert!(LimitSlewRate::default().feeds_through(0, 0));
    assert_eq!(MovingAverage::default().kind(), BlockKind::Stateful);
    assert!(MovingAverage::default().feeds_through(0, 0));
    assert!(Pid::default().feeds_through(0, 0) && Pid::default().feeds_through(1, 0));
    assert!(
        PidWithReset::default().feeds_through(0, 0)
            && PidWithReset::default().feeds_through(1, 0)
            && !PidWithReset::default().feeds_through(2, 0)
            && !PidWithReset::default().feeds_through(3, 0)
    );
    assert_eq!(Assert::default().kind(), BlockKind::Algebraic);
    assert!(!Assert::default().feeds_through(0, 0));
    assert!(Assert::default().signature().outputs.is_empty());
    assert_eq!(Add.kind(), BlockKind::Algebraic);
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
        "CDL.Reals.IntegratorWithReset",
        "CDL.Reals.Derivative",
        "CDL.Reals.LimitSlewRate",
        "CDL.Reals.MovingAverage",
        "CDL.Reals.PID",
        "CDL.Reals.PIDWithReset",
        "CDL.Logical.Sources.Constant",
        "CDL.Logical.And",
        "CDL.Logical.Or",
        "CDL.Logical.Not",
        "CDL.Logical.Nand",
        "CDL.Logical.Nor",
        "CDL.Logical.Xor",
        "CDL.Logical.Switch",
        "CDL.Logical.Pre",
        "CDL.Logical.Edge",
        "CDL.Logical.FallingEdge",
        "CDL.Logical.Change",
        "CDL.Logical.Latch",
        "CDL.Logical.Toggle",
        "CDL.Logical.Timer",
        "CDL.Logical.TimerAccumulating",
        "CDL.Logical.TrueDelay",
        "CDL.Logical.TrueFalseHold",
        "CDL.Logical.TrueHoldWithReset",
        "CDL.Logical.Sources.SampleTrigger",
        "CDL.Conversions.BooleanToInteger",
        "CDL.Conversions.BooleanToReal",
        "CDL.Conversions.IntegerToReal",
        "CDL.Conversions.RealToInteger",
        "CDL.Integers.Sources.Constant",
        "CDL.Integers.Abs",
        "CDL.Integers.Add",
        "CDL.Integers.Subtract",
        "CDL.Integers.Multiply",
        "CDL.Integers.AddParameter",
        "CDL.Integers.MultiplyByParameter",
        "CDL.Integers.Max",
        "CDL.Integers.Min",
        "CDL.Integers.Switch",
        "CDL.Integers.Greater",
        "CDL.Integers.GreaterThreshold",
        "CDL.Integers.GreaterEqual",
        "CDL.Integers.GreaterEqualThreshold",
        "CDL.Integers.Less",
        "CDL.Integers.LessThreshold",
        "CDL.Integers.LessEqual",
        "CDL.Integers.LessEqualThreshold",
        "CDL.Integers.OnCounter",
        "CDL.Integers.Change",
        "CDL.Discrete.UnitDelay",
        "CDL.Utilities.Assert",
    ];
    assert_eq!(PATHS.len(), 70, "registry count");
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

    let integrator = (lookup("CDL.Reals.IntegratorWithReset").unwrap().make)(&delay_params);
    let mut region = vec![0u64; integrator.state_len()];
    integrator.init_state(&mut region, &delay_params);
    assert!(emit(integrator.as_ref(), &[], &region)[0].bit_eq(&Value::Real(1.25)));

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

    let pid = (lookup("CDL.Reals.PID").unwrap().make)(&ParamTable {
        values: vec![
            (
                Arc::from("controllerType"),
                Value::String(Arc::from(
                    "Buildings.Controls.OBC.CDL.Types.SimpleController.PID",
                )),
            ),
            (Arc::from("xi_start"), Value::Real(2.0)),
        ],
    });
    assert_eq!(pid.kind(), BlockKind::Stateful);
    assert_eq!(pid.state_len(), 3);

    let p_reset = (lookup("CDL.Reals.PIDWithReset").unwrap().make)(&ParamTable {
        values: vec![(
            Arc::from("controllerType"),
            Value::String(Arc::from(
                "Buildings.Controls.OBC.CDL.Types.SimpleController.P",
            )),
        )],
    });
    assert_eq!(p_reset.kind(), BlockKind::Algebraic);
    assert_eq!(p_reset.state_len(), 0);

    fn tick_real(block: &dyn Block, region: &mut [u64], t: Time, u: f64) -> Value {
        let diag = NoopDiagnostics;
        let cx = Ctx::new(t, &diag);
        let inputs = [Value::Real(u)];
        let mut out = None;
        block.emit_from_state(&cx, &inputs, region, &mut |idx, val| {
            assert_eq!(idx, 0);
            out = Some(val);
        });
        block.update_state(&cx, &inputs, region);
        out.expect("single-output stateful block emits one value")
    }

    let slew = (lookup("CDL.Reals.LimitSlewRate").unwrap().make)(&ParamTable {
        values: vec![
            (Arc::from("raisingSlewRate"), Value::Real(2.0)),
            (Arc::from("fallingSlewRate"), Value::Real(-3.0)),
            (Arc::from("Td"), Value::Real(0.1)),
        ],
    });
    let mut region = vec![0u64; slew.state_len()];
    slew.init_state(&mut region, &ParamTable::default());
    assert!(tick_real(slew.as_ref(), &mut region, 0.0, 0.0).bit_eq(&Value::Real(0.0)));
    assert!(tick_real(slew.as_ref(), &mut region, 1.0, 10.0).bit_eq(&Value::Real(2.0)));
    assert!(tick_real(slew.as_ref(), &mut region, 2.0, -10.0).bit_eq(&Value::Real(-1.0)));

    let moving_average = (lookup("CDL.Reals.MovingAverage").unwrap().make)(&ParamTable {
        values: vec![(Arc::from("delta"), Value::Real(0.25))],
    });
    let mut region = vec![0u64; moving_average.state_len()];
    moving_average.init_state(&mut region, &ParamTable::default());
    assert!(tick_real(moving_average.as_ref(), &mut region, 0.0, 0.0).bit_eq(&Value::Real(0.0)));
    assert!(tick_real(moving_average.as_ref(), &mut region, 0.25, 4.0).bit_eq(&Value::Real(4.0)));
    assert!(tick_real(moving_average.as_ref(), &mut region, 0.5, 0.0).bit_eq(&Value::Real(0.0)));
}

#[test]
fn real_param_promotes_integer_to_real() {
    // Modelica/CDL Int→Real promotion: an integer literal bound to a `Real` parameter is its real
    // value, NOT silently dropped to the constructor default. CXF can carry a bare integer for a
    // Real parameter (no `isOfDataType` re-types it), so a non-zero integer `y_start`/`k` must reach
    // the block. Tripwire for the silent-wrong-initial-state hole.
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
