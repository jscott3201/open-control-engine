//! Under the host-selected `NonFiniteInputPolicy::Refuse`, the facade admits no non-finite Real
//! input: NaN of either sign (with any payload) and both infinities are refused by
//! `prepare_frame` with `InputNonFinite`, naming the canonical input. The default `Admit` policy,
//! and switching between the two, are pinned in `non_finite_input_policy.rs`.
//!
//! Expectations are hand-derived from the authored fixture (an `IntegratorWithReset`, a `Timer`,
//! an `IntegerToReal` and an `Add`); no engine output is used as its own oracle. "Untouched" is
//! checked twice: the refused engine's portable snapshot bytes do not move, and a run that saw
//! refusals is bit-identical, frame by frame, to a run that never saw them.

use oce_api::{CompletedFrame, Engine, NonFiniteInputPolicy, OcError, Value};

const MODEL: &[u8] = include_bytes!("fixtures/finite_frame.jsonld");
const N: &str = "urn:fin:n";
const RESET: &str = "urn:fin:reset";
const TRIGGER: &str = "urn:fin:trigger";
const U: &str = "urn:fin:u";
const OUTPUTS: [&str; 3] = ["urn:fin:elapsed", "urn:fin:sum", "urn:fin:y"];

/// Every non-finite binary64 class the host could hand over: quiet NaN of both signs, a payload
/// NaN, a signalling-pattern NaN, and both infinities.
const NON_FINITE: [u64; 6] = [
    0x7ff8_0000_0000_0000,
    0xfff8_0000_0000_0000,
    0x7ff8_0000_0000_0042,
    0x7ff0_0000_0000_0001,
    0x7ff0_0000_0000_0000,
    0xfff0_0000_0000_0000,
];

/// A fresh engine with the strict policy selected before load, as a production host would.
fn strict() -> Engine {
    let mut engine = Engine::in_memory();
    engine.set_non_finite_input_policy(NonFiniteInputPolicy::Refuse);
    engine
}

fn loaded() -> Engine {
    let mut engine = strict();
    engine.load_cxf(MODEL).expect("fixture loads");
    assert_eq!(
        engine.non_finite_input_policy(),
        NonFiniteInputPolicy::Refuse
    );
    engine
}

fn entries(u: f64, reset: f64, trigger: bool, n: i64) -> Vec<(&'static str, Value)> {
    vec![
        (U, Value::Real(u)),
        (RESET, Value::Real(reset)),
        (TRIGGER, Value::Boolean(trigger)),
        (N, Value::Integer(n)),
    ]
}

fn run(engine: &mut Engine, time: f64, frame: &[(&str, Value)]) -> CompletedFrame {
    let plan = engine.prepare_frame(time, frame).expect("valid frame");
    engine.execute_frame(plan).expect("valid frame executes")
}

fn output_bits(frame: &CompletedFrame) -> Vec<(String, u64)> {
    frame
        .outputs()
        .iter()
        .map(|(path, value)| {
            (
                path.clone(),
                value.as_real().expect("Real output").to_bits(),
            )
        })
        .collect()
}

#[test]
fn the_schema_is_the_authored_one() {
    let engine = loaded();
    let paths: Vec<_> = engine
        .input_definitions()
        .unwrap()
        .into_iter()
        .map(|definition| definition.path)
        .collect();
    assert_eq!(paths, [N, RESET, TRIGGER, U]);
    let outputs: Vec<_> = run(&mut loaded(), 0.0, &entries(1.0, 0.0, false, 2))
        .outputs()
        .iter()
        .map(|(path, _)| path.clone())
        .collect();
    assert_eq!(outputs, OUTPUTS);
}

#[test]
fn every_non_finite_real_is_refused_on_unbounded_bounded_and_fanned_out_inputs() {
    let mut engine = loaded();
    run(&mut engine, 0.0, &entries(1.0, 0.0, false, 2));
    let before = engine.state_snapshot().unwrap();
    for bits in NON_FINITE {
        let bad = f64::from_bits(bits);
        // `u` is unbounded and fans out to two blocks; `reset` is bounded to [-10, 10].
        for (frame, expected) in [
            (entries(bad, 0.0, false, 2), U),
            (entries(1.0, bad, false, 2), RESET),
        ] {
            for time in [0.0, 1.0, 1.0e9] {
                let error = engine.prepare_frame(time, &frame).unwrap_err();
                assert!(
                    matches!(&error, OcError::InputNonFinite(path) if path == expected),
                    "{bits:#018x} on {expected}: {error:?}"
                );
                assert_eq!(
                    error.to_string(),
                    format!(
                        "non-finite Real value for input '{expected}'; the host must substitute \
                         a finite value"
                    )
                );
                assert_eq!(
                    engine.state_snapshot().unwrap().as_bytes(),
                    before.as_bytes()
                );
            }
        }
    }
}

#[test]
fn refusal_names_the_first_canonical_non_finite_input_and_keeps_type_and_domain_order() {
    let engine = {
        let mut engine = loaded();
        run(&mut engine, 0.0, &entries(1.0, 0.0, false, 2));
        engine
    };
    // Both Real inputs non-finite: the canonically first (`reset` < `u`) is named.
    let mut both = entries(f64::NAN, f64::INFINITY, false, 2);
    for _ in 0..both.len() {
        both.rotate_left(1);
        assert!(matches!(
            engine.prepare_frame(1.0, &both),
            Err(OcError::InputNonFinite(path)) if path == RESET
        ));
    }
    // A type error on an earlier canonical input wins over a later non-finite value...
    let mut typed = entries(f64::NAN, 0.0, false, 2);
    typed[3] = (N, Value::Real(2.0));
    assert!(matches!(engine.prepare_frame(1.0, &typed), Err(OcError::InputType(p)) if p == N));
    // ...and on the same input the type check runs first (a non-Real carries no float).
    let mut same = entries(1.0, 0.0, false, 2);
    same[0] = (U, Value::Boolean(true));
    assert!(matches!(engine.prepare_frame(1.0, &same), Err(OcError::InputType(p)) if p == U));
    // Non-finite precedes the domain check on a bounded input: -inf is also below its min.
    assert!(matches!(
        engine.prepare_frame(1.0, &entries(1.0, f64::NEG_INFINITY, false, 2)),
        Err(OcError::InputNonFinite(p)) if p == RESET
    ));
    // A finite out-of-domain value on an earlier input wins over a later non-finite one.
    assert!(matches!(
        engine.prepare_frame(1.0, &entries(f64::NAN, 11.0, false, 2)),
        Err(OcError::InputDomain(p)) if p == RESET
    ));
    // Time refusals still precede every value refusal.
    assert!(matches!(
        engine.prepare_frame(f64::NAN, &entries(f64::NAN, 0.0, false, 2)),
        Err(OcError::NonFiniteTime { .. })
    ));
    assert!(matches!(
        engine.prepare_frame(-1.0, &entries(f64::NAN, 0.0, false, 2)),
        Err(OcError::TimeRegression { .. })
    ));
}

#[test]
fn finite_extremes_are_accepted_bit_exactly() {
    for (u, reset) in [
        (f64::MAX, 10.0),
        (f64::MIN, -10.0),
        (f64::from_bits(1), -0.0),
        (-f64::from_bits(1), 0.0),
        (f64::MIN_POSITIVE, f64::MIN_POSITIVE),
        (-0.0, -10.0),
    ] {
        let mut engine = loaded();
        let frame = run(&mut engine, 0.0, &entries(u, reset, false, 0));
        let accepted: Vec<_> = frame
            .inputs()
            .iter()
            .filter_map(|(path, value)| value.as_real().ok().map(|v| (path.as_str(), v.to_bits())))
            .collect();
        assert_eq!(accepted, [(RESET, reset.to_bits()), (U, u.to_bits())]);
        // Add: u + IntegerToReal(0) == u, exactly (x + 0.0 keeps x except -0.0 + 0.0 == +0.0).
        let sum = frame.outputs()[1].1.as_real().unwrap();
        assert_eq!(sum.to_bits(), (u + 0.0).to_bits());
    }
}

#[test]
fn integer_and_boolean_inputs_are_unaffected() {
    for n in [i64::from(i32::MIN), -1, 0, 1, i64::from(i32::MAX)] {
        for trigger in [false, true] {
            let mut engine = loaded();
            let frame = run(&mut engine, 0.0, &entries(0.0, 0.0, trigger, n));
            assert_eq!(
                frame.outputs()[1].1.as_real().unwrap().to_bits(),
                (n as f64).to_bits(),
                "sum = 0 + n"
            );
        }
    }
}

#[test]
fn a_run_that_saw_refusals_is_bit_identical_to_one_that_never_did() {
    // The integrator emits its prior accumulator, then advances it by u * dt (dt = 0 on the
    // first tick). Hand-derived: x = 0 after t=0 (2 * 0), x = 0 + (-1) * 1 = -1 after t=1, so
    // the output reads -1 at t=3. A non-finite sample reaching it would latch NaN forever.
    let script = [
        (0.0, entries(2.0, 0.0, false, 1)),
        (1.0, entries(-1.0, 0.0, false, 1)),
        (3.0, entries(0.5, 0.0, true, 1)),
        (3.0, entries(4.0, 5.0, false, 3)),
        (7.5, entries(0.25, 0.0, false, -2)),
    ];
    let mut clean = loaded();
    let reference: Vec<_> = script
        .iter()
        .map(|(time, frame)| output_bits(&run(&mut clean, *time, frame)))
        .collect();
    let reference_state = clean.state_snapshot().unwrap();
    assert_eq!(reference[1][2], ("urn:fin:y".to_owned(), 0.0_f64.to_bits()));
    assert_eq!(
        reference[2][2],
        ("urn:fin:y".to_owned(), (-1.0_f64).to_bits())
    );
    for bits in NON_FINITE {
        let bad = f64::from_bits(bits);
        let mut noisy = loaded();
        let mut sequences = Vec::new();
        for ((time, frame), expected) in script.iter().zip(&reference) {
            let before = noisy.state_snapshot().unwrap();
            for corrupt in [
                entries(bad, 0.0, false, 1),
                entries(0.0, bad, true, 1),
                entries(bad, bad, false, 1),
            ] {
                assert!(matches!(
                    noisy.prepare_frame(*time, &corrupt),
                    Err(OcError::InputNonFinite(_))
                ));
                assert!(noisy.prepare_frame(f64::from_bits(bits), frame).is_err());
            }
            assert_eq!(
                noisy.state_snapshot().unwrap().as_bytes(),
                before.as_bytes()
            );
            let completed = run(&mut noisy, *time, frame);
            sequences.push(completed.sequence());
            assert_eq!(
                &output_bits(&completed),
                expected,
                "{bits:#018x} at t={time}"
            );
        }
        assert_eq!(
            sequences,
            [1, 2, 3, 4, 5],
            "refusals never consume a sequence"
        );
        assert_eq!(
            noisy.state_snapshot().unwrap().as_bytes(),
            reference_state.as_bytes()
        );
    }
}

#[test]
fn no_non_finite_value_reaches_a_block_through_inputs_or_time() {
    let mut engine = loaded();
    let mut time = 0.0;
    let mut accepted = 0;
    for step in 0..12_u32 {
        for bits in NON_FINITE {
            let bad = f64::from_bits(bits);
            // A host that executes whatever preparation accepts: nothing here may be accepted.
            for (at, frame) in [
                (time, entries(bad, 0.0, step % 2 == 0, 1)),
                (time, entries(1.0, bad, step % 3 == 0, 1)),
                (bad, entries(1.0, 0.0, false, 1)),
            ] {
                if let Ok(plan) = engine.prepare_frame(at, &frame) {
                    accepted += 1;
                    engine.execute_frame(plan).unwrap();
                }
            }
        }
        run(
            &mut engine,
            time,
            &entries(f64::from(step) - 5.0, 1.0, step % 4 == 0, 7),
        );
        let watched = engine
            .watch(&[
                "urn:fin:integ.y",
                "urn:fin:timer.y",
                "urn:fin:toReal.y",
                "urn:fin:add.y",
            ])
            .unwrap();
        for (path, value) in watched {
            assert!(
                value.as_real().unwrap().is_finite(),
                "{path} at step {step}"
            );
        }
        time += 0.5;
    }
    // Overflowing time steps are swept in `finite_time_step.rs`.
    assert_eq!(accepted, 0, "a non-finite input or time was accepted");
}

/// A finite, in-domain value for one definition: a declared bound when there is one, else zero.
fn admissible(definition: &oce_api::InputDefinition) -> Value {
    match definition.value_type {
        oce_api::ValueType::Real => definition
            .min
            .clone()
            .or_else(|| definition.max.clone())
            .unwrap_or(Value::Real(0.0)),
        oce_api::ValueType::Integer => {
            let lower = definition.min.as_ref().and_then(|v| v.as_integer().ok());
            let upper = definition.max.as_ref().and_then(|v| v.as_integer().ok());
            Value::Integer(lower.unwrap_or(0).max(0).min(upper.unwrap_or(i64::MAX)))
        }
        oce_api::ValueType::Boolean => Value::Boolean(false),
        _ => definition
            .min
            .clone()
            .expect("enum inputs carry ordinal bounds"),
    }
}

#[test]
fn every_real_input_of_every_g36_fixture_refuses_every_non_finite_value() {
    let dir = format!(
        "{}/../oce-cxf/tests/fixtures/g36",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "jsonld"))
        .collect();
    paths.sort();
    let (mut models, mut real_inputs) = (0, 0);
    for path in paths {
        let mut engine = strict();
        engine
            .load_cxf(&std::fs::read(&path).unwrap())
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        models += 1;
        let definitions = engine.input_definitions().unwrap();
        let baseline: Vec<_> = definitions
            .iter()
            .map(|definition| (definition.path.as_str(), admissible(definition)))
            .collect();
        let before = engine.state_snapshot().unwrap();
        engine
            .prepare_frame(0.0, &baseline)
            .unwrap_or_else(|error| panic!("{}: baseline refused: {error}", path.display()));
        for (index, definition) in definitions.iter().enumerate() {
            if definition.value_type != oce_api::ValueType::Real {
                continue;
            }
            real_inputs += 1;
            for bits in NON_FINITE {
                let mut entries = baseline.clone();
                entries[index].1 = Value::Real(f64::from_bits(bits));
                assert!(
                    matches!(
                        engine.prepare_frame(0.0, &entries),
                        Err(OcError::InputNonFinite(p)) if p == definition.path
                    ),
                    "{}: {bits:#018x} on {}",
                    path.display(),
                    definition.path
                );
            }
        }
        assert_eq!(
            engine.state_snapshot().unwrap().as_bytes(),
            before.as_bytes()
        );
    }
    // Guard the sweep itself: every corpus fixture loads (47 at the time of writing, with 126
    // Real boundary inputs). CXF array connectors are refused at import, so every boundary
    // input is a scalar connector; there is no vector-valued input to sweep separately.
    assert!(models >= 47, "loaded {models} G36 fixtures");
    assert!(real_inputs >= 126, "swept {real_inputs} Real inputs");
}
