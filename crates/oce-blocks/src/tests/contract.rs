use super::common::*;

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
    let line = Line::default();
    assert!(
        line.feeds_through(0, 0)
            && line.feeds_through(1, 0)
            && line.feeds_through(2, 0)
            && line.feeds_through(3, 0)
            && line.feeds_through(4, 0)
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
    assert!(MultiAnd::new(3).feeds_through(0, 0));
    assert!(MultiAnd::new(3).feeds_through(2, 0));
    assert!(!MultiAnd::new(3).feeds_through(3, 0));
    assert!(MultiOr::new(2).feeds_through(1, 0));
    assert!(MultiSum::new(vec![1.0, 2.0, 3.0]).feeds_through(2, 0));
    assert!(!MultiSum::new(vec![1.0, 2.0, 3.0]).feeds_through(3, 0));
    assert!(MultiMin::new(2).feeds_through(1, 0));
    assert!(MultiMax::new(2).feeds_through(1, 0));
    assert!(RealExtractSignal::new(3, 2, vec![3, 1]).feeds_through(2, 0));
    assert!(RealExtractSignal::new(3, 2, vec![3, 1]).feeds_through(0, 1));
    assert!(RealExtractor::new(3).feeds_through(0, 0));
    assert!(RealExtractor::new(3).feeds_through(3, 0));
    assert!(RealScalarReplicator::new(2).feeds_through(0, 1));
    assert!(RealVectorFilter::new(3, 2, vec![true, false, true]).feeds_through(2, 1));
    assert!(RealVectorReplicator::new(2, 3).feeds_through(1, 5));
    assert!(Not.feeds_through(0, 0));
    assert!(Switch.feeds_through(0, 0) && Switch.feeds_through(1, 0) && Switch.feeds_through(2, 0));

    assert!(!Constant { k: 0.0 }.feeds_through(0, 0)); // no inputs
    assert!(!Pre::default().feeds_through(0, 0)); // THE cut
    assert!(!UnitDelay::default().feeds_through(0, 0)); // discrete cut
    assert!(Sampler::default().feeds_through(0, 0)); // initial/sample instants emit current u
    assert!(ZeroOrderHold::default().feeds_through(0, 0)); // conservative: initial tick feeds u
    assert!(FirstOrderHold::default().feeds_through(0, 0)); // conservative: initial tick feeds u
    assert!(TriggeredMax.feeds_through(0, 0)); // current u seeds initial y and trigger samples
    assert!(TriggeredMax.feeds_through(1, 0)); // current trigger selects same-tick event
    assert!(TriggeredMovingMean::default().feeds_through(0, 0)); // initial/event sample includes current u
    assert!(TriggeredMovingMean::default().feeds_through(1, 0)); // current trigger selects event
    assert!(TriggeredSampler::default().feeds_through(0, 0)); // current u sampled on a trigger
    assert!(TriggeredSampler::default().feeds_through(1, 0)); // current trigger chooses sampling
    assert!(!IntegratorWithReset::default().feeds_through(0, 0)); // integrating loop cut
    assert!(!IntegratorWithReset::default().feeds_through(1, 0)); // reset value is delayed
    assert!(!IntegratorWithReset::default().feeds_through(2, 0)); // trigger is delayed

    // Edge is stateful (owns `prev`) but FEEDS THROUGH on the current `u` — the edge is a function
    // of the current input vs the prior bit, so it is NOT a loop cut (`01` §11.2 req 3). Getting
    // this backwards would let the DAG scheduler treat it as a cut and corrupt the schedule.
    assert!(Edge::default().feeds_through(0, 0));
    assert_eq!(Edge::default().kind(), BlockKind::Stateful);
    assert!(Proof::default().feeds_through(0, 0));
    assert!(Proof::default().feeds_through(0, 1));
    assert!(Proof::default().feeds_through(1, 0));
    assert!(Proof::default().feeds_through(1, 1));
    assert_eq!(Proof::default().kind(), BlockKind::Stateful);
    assert!(LogicalVariablePulse::default().feeds_through(0, 0));
    assert_eq!(LogicalVariablePulse::default().kind(), BlockKind::Stateful);
    assert!(IntegerStage::default().feeds_through(0, 0));
    assert_eq!(IntegerStage::default().kind(), BlockKind::Stateful);
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
    assert_eq!(Sampler::default().kind(), BlockKind::Stateful);
    assert_eq!(ZeroOrderHold::default().kind(), BlockKind::Stateful);
    assert_eq!(FirstOrderHold::default().kind(), BlockKind::Stateful);
    assert_eq!(TriggeredMax.kind(), BlockKind::Stateful);
    assert_eq!(TriggeredMovingMean::default().kind(), BlockKind::Stateful);
    assert_eq!(TriggeredSampler::default().kind(), BlockKind::Stateful);
    assert_eq!(IntegratorWithReset::default().kind(), BlockKind::Stateful);
    assert_eq!(Derivative::default().kind(), BlockKind::Stateful);
    assert!(Derivative::default().feeds_through(0, 0));
    assert_eq!(LimitSlewRate::default().kind(), BlockKind::Stateful);
    assert!(LimitSlewRate::default().feeds_through(0, 0));
    assert_eq!(Ramp::default().kind(), BlockKind::Stateful);
    assert!(Ramp::default().feeds_through(0, 0));
    assert!(Ramp::default().feeds_through(1, 0));
    assert_eq!(MovingAverage::default().kind(), BlockKind::Stateful);
    assert!(MovingAverage::default().feeds_through(0, 0));
    assert!(Pid::default().feeds_through(0, 0) && Pid::default().feeds_through(1, 0));
    assert!(
        PidWithReset::default().feeds_through(0, 0)
            && PidWithReset::default().feeds_through(1, 0)
            && !PidWithReset::default().feeds_through(2, 0)
    );
    assert_eq!(Assert::default().kind(), BlockKind::Algebraic);
    assert!(!Assert::default().feeds_through(0, 0));
    assert!(Assert::default().signature().outputs.is_empty());
    assert_eq!(SunRiseSet::default().kind(), BlockKind::Stateful);
    assert!(!SunRiseSet::default().feeds_through(0, 0));
    assert!(SunRiseSet::default().signature().inputs.is_empty());
    assert_eq!(SunRiseSet::default().signature().outputs.len(), 3);
    assert_eq!(CalendarTime::default().kind(), BlockKind::Algebraic);
    assert!(!CalendarTime::default().feeds_through(0, 0));
    assert!(CalendarTime::default().signature().inputs.is_empty());
    assert_eq!(CalendarTime::default().signature().outputs.len(), 6);
    assert_eq!(Add.kind(), BlockKind::Algebraic);
}

#[test]
fn registry_resolves_canonical_paths() {
    const PATHS: &[&str] = &[
        "CDL.Reals.Sources.Constant",
        "CDL.Reals.Sources.CivilTime",
        "CDL.Reals.Sources.Pulse",
        "CDL.Reals.Sources.Ramp",
        "CDL.Reals.Sources.Sin",
        "CDL.Reals.Sources.CalendarTime",
        "CDL.Reals.Add",
        "CDL.Reals.Subtract",
        "CDL.Reals.Multiply",
        "CDL.Reals.Divide",
        "CDL.Reals.Sqrt",
        "CDL.Reals.Average",
        "CDL.Reals.Modulo",
        "CDL.Reals.Round",
        "CDL.Reals.AddParameter",
        "CDL.Reals.MultiplyByParameter",
        "CDL.Reals.Abs",
        "CDL.Reals.Min",
        "CDL.Reals.Max",
        "CDL.Reals.MultiMax",
        "CDL.Reals.MultiMin",
        "CDL.Reals.MultiSum",
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
        "CDL.Reals.Ramp",
        "CDL.Reals.MovingAverage",
        "CDL.Reals.PID",
        "CDL.Reals.PIDWithReset",
        "CDL.Routing.BooleanExtractSignal",
        "CDL.Routing.BooleanExtractor",
        "CDL.Routing.BooleanScalarReplicator",
        "CDL.Routing.BooleanVectorFilter",
        "CDL.Routing.BooleanVectorReplicator",
        "CDL.Routing.IntegerExtractSignal",
        "CDL.Routing.IntegerExtractor",
        "CDL.Routing.IntegerScalarReplicator",
        "CDL.Routing.IntegerVectorFilter",
        "CDL.Routing.IntegerVectorReplicator",
        "CDL.Routing.RealExtractSignal",
        "CDL.Routing.RealExtractor",
        "CDL.Routing.RealScalarReplicator",
        "CDL.Routing.RealVectorFilter",
        "CDL.Routing.RealVectorReplicator",
        "CDL.Psychrometrics.DewPoint_TDryBulPhi",
        "CDL.Psychrometrics.SpecificEnthalpy_TDryBulPhi",
        "CDL.Psychrometrics.WetBulb_TDryBulPhi",
        "CDL.Logical.Sources.Constant",
        "CDL.Logical.Sources.Pulse",
        "CDL.Logical.And",
        "CDL.Logical.Or",
        "CDL.Logical.MultiAnd",
        "CDL.Logical.MultiOr",
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
        "CDL.Logical.Proof",
        "CDL.Logical.Toggle",
        "CDL.Logical.Timer",
        "CDL.Logical.TimerAccumulating",
        "CDL.Logical.TrueDelay",
        "CDL.Logical.TrueFalseHold",
        "CDL.Logical.TrueHoldWithReset",
        "CDL.Logical.VariablePulse",
        "CDL.Logical.Sources.SampleTrigger",
        "CDL.Conversions.BooleanToInteger",
        "CDL.Conversions.BooleanToReal",
        "CDL.Conversions.IntegerToReal",
        "CDL.Conversions.RealToInteger",
        "CDL.Integers.Sources.Constant",
        "CDL.Integers.Sources.Pulse",
        "CDL.Integers.Abs",
        "CDL.Integers.Add",
        "CDL.Integers.Subtract",
        "CDL.Integers.Multiply",
        "CDL.Integers.AddParameter",
        "CDL.Integers.Max",
        "CDL.Integers.Min",
        "CDL.Integers.MultiSum",
        "CDL.Integers.Switch",
        "CDL.Integers.Equal",
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
        "CDL.Integers.Stage",
        "CDL.Discrete.FirstOrderHold",
        "CDL.Discrete.Sampler",
        "CDL.Discrete.TriggeredMax",
        "CDL.Discrete.TriggeredMovingMean",
        "CDL.Discrete.TriggeredSampler",
        "CDL.Discrete.UnitDelay",
        "CDL.Discrete.ZeroOrderHold",
        "CDL.Utilities.Assert",
        "CDL.Utilities.SunRiseSet",
    ];
    assert_eq!(PATHS.len(), 116, "registry count");
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

    let round_params = ParamTable {
        values: vec![(Arc::from("n"), Value::Integer(2))],
    };
    let round = (lookup("CDL.Reals.Round").unwrap().make)(&round_params);
    assert!(outs(round.as_ref(), &[Value::Real(1.125)])[0].bit_eq(&Value::Real(1.13)));

    let delay_params = ParamTable {
        values: vec![(Arc::from("y_start"), Value::Real(1.25))],
    };
    let delay = (lookup("CDL.Discrete.UnitDelay").unwrap().make)(&delay_params);
    let mut region = vec![0u64; delay.state_len()];
    delay.init_state(&mut region, &delay_params);
    assert!(emit(delay.as_ref(), &[Value::Real(0.0)], &region)[0].bit_eq(&Value::Real(1.25)));

    let triggered_max = (lookup("CDL.Discrete.TriggeredMax").unwrap().make)(&ParamTable::default());
    assert_eq!(triggered_max.kind(), BlockKind::Stateful);
    assert_eq!(triggered_max.state_len(), 3);
    let mut region = vec![0u64; triggered_max.state_len()];
    triggered_max.init_state(&mut region, &ParamTable::default());
    assert!(
        emit(
            triggered_max.as_ref(),
            &[Value::Real(-2.0), Value::Boolean(false)],
            &region,
        )[0]
        .bit_eq(&Value::Real(-2.0))
    );

    let moving_mean_params = ParamTable {
        values: vec![(Arc::from("n"), Value::Integer(3))],
    };
    let moving_mean =
        (lookup("CDL.Discrete.TriggeredMovingMean").unwrap().make)(&moving_mean_params);
    assert_eq!(moving_mean.kind(), BlockKind::Stateful);
    assert_eq!(moving_mean.state_len(), 7);
    let mut region = vec![0u64; moving_mean.state_len()];
    moving_mean.init_state(&mut region, &moving_mean_params);
    assert!(
        emit(
            moving_mean.as_ref(),
            &[Value::Real(6.0), Value::Boolean(false)],
            &region,
        )[0]
        .bit_eq(&Value::Real(6.0))
    );

    let moving_mean_default =
        (lookup("CDL.Discrete.TriggeredMovingMean").unwrap().make)(&ParamTable::default());
    assert_eq!(
        moving_mean_default.state_len(),
        5,
        "direct construction without validated n falls back to n=1"
    );

    let sampler = (lookup("CDL.Discrete.TriggeredSampler").unwrap().make)(&delay_params);
    assert_eq!(sampler.kind(), BlockKind::Stateful);
    assert_eq!(sampler.state_len(), 2);
    let mut region = vec![0u64; sampler.state_len()];
    sampler.init_state(&mut region, &delay_params);
    assert!(
        emit(
            sampler.as_ref(),
            &[Value::Real(0.0), Value::Boolean(false)],
            &region,
        )[0]
        .bit_eq(&Value::Real(1.25))
    );

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

    let pi_reset = (lookup("CDL.Reals.PIDWithReset").unwrap().make)(&ParamTable {
        values: vec![
            (
                Arc::from("controllerType"),
                Value::String(Arc::from(
                    "Buildings.Controls.OBC.CDL.Types.SimpleController.PI",
                )),
            ),
            (Arc::from("k"), Value::Real(1.0)),
            (Arc::from("Ti"), Value::Real(1.0)),
            (Arc::from("xi_start"), Value::Real(4.25)),
            (Arc::from("yMin"), Value::Real(-100.0)),
            (Arc::from("yMax"), Value::Real(100.0)),
        ],
    });
    let mut region = vec![0u64; pi_reset.state_len()];
    pi_reset.init_state(&mut region, &ParamTable::default());
    let diag = NoopDiagnostics;
    let cx = Ctx::new(0.0, &diag);
    let inputs = [Value::Real(1.0), Value::Real(0.0), Value::Boolean(true)];
    pi_reset.update_state(&cx, &inputs, &mut region);
    assert!(
        emit(pi_reset.as_ref(), &inputs, &region)[0].bit_eq(&Value::Real(4.25)),
        "PIDWithReset.y_reset must default to the resolved xi_start parameter"
    );

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

    let sampled_params = ParamTable {
        values: vec![(Arc::from("samplePeriod"), Value::Real(0.5))],
    };
    let sampler = (lookup("CDL.Discrete.Sampler").unwrap().make)(&sampled_params);
    let mut region = vec![0u64; sampler.state_len()];
    sampler.init_state(&mut region, &sampled_params);
    assert!(tick_real(sampler.as_ref(), &mut region, 0.0, 1.0).bit_eq(&Value::Real(1.0)));
    assert!(tick_real(sampler.as_ref(), &mut region, 0.25, 2.0).bit_eq(&Value::Real(1.0)));
    assert!(tick_real(sampler.as_ref(), &mut region, 0.5, 3.0).bit_eq(&Value::Real(3.0)));
    assert!(tick_real(sampler.as_ref(), &mut region, 0.75, 4.0).bit_eq(&Value::Real(3.0)));

    let zero_hold = (lookup("CDL.Discrete.ZeroOrderHold").unwrap().make)(&sampled_params);
    let mut region = vec![0u64; zero_hold.state_len()];
    zero_hold.init_state(&mut region, &sampled_params);
    assert!(tick_real(zero_hold.as_ref(), &mut region, 0.0, 1.0).bit_eq(&Value::Real(1.0)));
    assert!(tick_real(zero_hold.as_ref(), &mut region, 0.25, 2.0).bit_eq(&Value::Real(1.0)));
    assert!(tick_real(zero_hold.as_ref(), &mut region, 0.5, 3.0).bit_eq(&Value::Real(1.0)));
    assert!(tick_real(zero_hold.as_ref(), &mut region, 0.75, 4.0).bit_eq(&Value::Real(3.0)));

    let first_hold = (lookup("CDL.Discrete.FirstOrderHold").unwrap().make)(&sampled_params);
    let mut region = vec![0u64; first_hold.state_len()];
    first_hold.init_state(&mut region, &sampled_params);
    assert!(tick_real(first_hold.as_ref(), &mut region, 0.0, 1.0).bit_eq(&Value::Real(1.0)));
    assert!(tick_real(first_hold.as_ref(), &mut region, 0.25, 2.0).bit_eq(&Value::Real(1.0)));
    assert!(tick_real(first_hold.as_ref(), &mut region, 0.5, 3.0).bit_eq(&Value::Real(1.0)));
    assert!(tick_real(first_hold.as_ref(), &mut region, 0.75, 4.0).bit_eq(&Value::Real(2.0)));

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

    let ramp = (lookup("CDL.Reals.Ramp").unwrap().make)(&ParamTable {
        values: vec![
            (Arc::from("raisingSlewRate"), Value::Real(2.0)),
            (Arc::from("fallingSlewRate"), Value::Real(-3.0)),
            (Arc::from("Td"), Value::Real(0.1)),
        ],
    });
    assert_eq!(ramp.kind(), BlockKind::Stateful);
    assert_eq!(ramp.state_len(), 3);
    let mut region = vec![0u64; ramp.state_len()];
    ramp.init_state(&mut region, &ParamTable::default());
    let diag = NoopDiagnostics;
    let cx = Ctx::new(0.0, &diag);
    let mut out = None;
    let inputs = [Value::Real(10.0), Value::Boolean(true)];
    ramp.emit_from_state(&cx, &inputs, &region, &mut |idx, val| {
        assert_eq!(idx, 0);
        out = Some(val);
    });
    assert!(
        out.expect("Ramp emits one output")
            .bit_eq(&Value::Real(10.0)),
        "Ramp initial equation y_internal=u must be visible on the first tick"
    );

    let proof = (lookup("CDL.Logical.Proof").unwrap().make)(&ParamTable {
        values: vec![
            (Arc::from("debounce"), Value::Real(2.0)),
            (Arc::from("feedbackDelay"), Value::Real(5.0)),
        ],
    });
    assert_eq!(proof.kind(), BlockKind::Stateful);
    assert_eq!(proof.state_len(), 24);

    let stage = (lookup("CDL.Integers.Stage").unwrap().make)(&ParamTable {
        values: vec![
            (Arc::from("n"), Value::Integer(4)),
            (Arc::from("holdDuration"), Value::Real(2.0)),
            (Arc::from("pre_y_start"), Value::Integer(3)),
        ],
    });
    assert_eq!(stage.kind(), BlockKind::Stateful);
    assert_eq!(stage.state_len(), 8);
    let mut region = vec![0u64; stage.state_len()];
    stage.init_state(&mut region, &ParamTable::default());
    assert!(
        emit(stage.as_ref(), &[Value::Real(1.0)], &region)[0].bit_eq(&Value::Integer(3)),
        "Stage.pre_y_start must seed the initial output until a post-initial tick updates"
    );

    let moving_average = (lookup("CDL.Reals.MovingAverage").unwrap().make)(&ParamTable {
        values: vec![(Arc::from("delta"), Value::Real(0.25))],
    });
    let mut region = vec![0u64; moving_average.state_len()];
    moving_average.init_state(&mut region, &ParamTable::default());
    assert!(tick_real(moving_average.as_ref(), &mut region, 0.0, 0.0).bit_eq(&Value::Real(0.0)));
    assert!(tick_real(moving_average.as_ref(), &mut region, 0.25, 4.0).bit_eq(&Value::Real(4.0)));
    assert!(tick_real(moving_average.as_ref(), &mut region, 0.5, 0.0).bit_eq(&Value::Real(0.0)));

    let extract_signal = (lookup("CDL.Routing.RealExtractSignal").unwrap().make)(&ParamTable {
        values: vec![
            (Arc::from("nin"), Value::Integer(3)),
            (Arc::from("nout"), Value::Integer(2)),
            (Arc::from("extract_1"), Value::Integer(3)),
            (Arc::from("extract_2"), Value::Integer(1)),
        ],
    });
    assert_eq!(
        extract_signal.resolved_signature().inputs.as_ref(),
        &[PortKind::Real; 3]
    );
    assert!(
        outs(
            extract_signal.as_ref(),
            &[Value::Real(1.0), Value::Real(2.0), Value::Real(3.0)]
        )[0]
        .bit_eq(&Value::Real(3.0))
    );

    let vector_filter = (lookup("CDL.Routing.RealVectorFilter").unwrap().make)(&ParamTable {
        values: vec![
            (Arc::from("nin"), Value::Integer(3)),
            (Arc::from("nout"), Value::Integer(2)),
            (Arc::from("msk_1"), Value::Boolean(true)),
            (Arc::from("msk_2"), Value::Boolean(false)),
            (Arc::from("msk_3"), Value::Boolean(true)),
        ],
    });
    assert_eq!(
        vector_filter.resolved_signature().outputs.as_ref(),
        &[PortKind::Real; 2]
    );
    assert!(
        outs(
            vector_filter.as_ref(),
            &[Value::Real(4.0), Value::Real(5.0), Value::Real(6.0)]
        )[1]
        .bit_eq(&Value::Real(6.0))
    );

    let vector_replicator =
        (lookup("CDL.Routing.RealVectorReplicator").unwrap().make)(&ParamTable {
            values: vec![
                (Arc::from("nin"), Value::Integer(2)),
                (Arc::from("nout"), Value::Integer(3)),
            ],
        });
    assert_eq!(
        vector_replicator.resolved_signature().outputs.as_ref(),
        &[PortKind::Real; 6]
    );
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

    let sampler = (lookup("CDL.Discrete.TriggeredSampler").unwrap().make)(&y_int);
    let mut region = vec![0u64; sampler.state_len()];
    sampler.init_state(&mut region, &y_int);
    assert!(
        emit(
            sampler.as_ref(),
            &[Value::Real(0.0), Value::Boolean(false)],
            &region,
        )[0]
        .bit_eq(&Value::Real(5.0)),
        "Integer(5) TriggeredSampler.y_start must seed the initial held output"
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
