//! The host-selected `NonFiniteInputPolicy`: the default `Admit` keeps the existing contract
//! (non-finite Real values on unbounded inputs flow to blocks unchanged), `Refuse` is opt-in, and
//! switching between frames is safe — including a plan prepared under one policy and executed
//! under the other. The exhaustive `Refuse` sweep lives in `finite_frame_inputs.rs`.
//!
//! Expectations are hand-derived from the authored fixture (an `IntegratorWithReset`, a `Timer`,
//! an `IntegerToReal` and an `Add`); the only engine-against-engine comparisons are controls
//! between a default engine and one whose policy was touched, which must be bit-identical.

use oce_api::{CompletedFrame, Engine, NonFiniteInputPolicy, OcError, ReplayRecord, Value};

const MODEL: &[u8] = include_bytes!("fixtures/finite_frame.jsonld");
const N: &str = "urn:fin:n";
const RESET: &str = "urn:fin:reset";
const TRIGGER: &str = "urn:fin:trigger";
const U: &str = "urn:fin:u";
const SUM: usize = 1;

/// Two unbounded Real inputs feeding `Add`; `u1` is canonically first.
const TWO_INPUTS: &[u8] =
    include_bytes!("../../oce-conformance/tests/fixtures/driver/free_add.jsonld");
const U1: &str = "http://example.org#DriverAdd.u1";
const U2: &str = "http://example.org#DriverAdd.u2";

/// Quiet NaN of both signs, a payload NaN, a signalling-pattern NaN, and both infinities.
const NON_FINITE: [u64; 6] = [
    0x7ff8_0000_0000_0000,
    0xfff8_0000_0000_0000,
    0x7ff8_0000_0000_0042,
    0x7ff0_0000_0000_0001,
    0x7ff0_0000_0000_0000,
    0xfff0_0000_0000_0000,
];

fn loaded() -> Engine {
    let mut engine = Engine::in_memory();
    engine.load_cxf(MODEL).expect("fixture loads");
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

/// Every output as exact bits, keyed by path, for bit-exact comparisons that treat NaN as data.
fn output_bits(frame: &CompletedFrame) -> Vec<(String, u64)> {
    frame
        .outputs()
        .iter()
        .map(|(path, value)| (path.clone(), value.as_real().unwrap().to_bits()))
        .collect()
}

fn snapshot(engine: &Engine) -> Vec<u8> {
    engine.state_snapshot().unwrap().as_bytes().to_vec()
}

#[test]
fn the_default_policy_is_admit_before_and_after_load() {
    assert_eq!(NonFiniteInputPolicy::default(), NonFiniteInputPolicy::Admit);
    assert_eq!(
        Engine::in_memory().non_finite_input_policy(),
        NonFiniteInputPolicy::Admit
    );
    assert_eq!(
        loaded().non_finite_input_policy(),
        NonFiniteInputPolicy::Admit
    );
    assert_ne!(NonFiniteInputPolicy::Admit, NonFiniteInputPolicy::Refuse);
}

#[test]
fn admit_passes_every_non_finite_unbounded_input_through_bit_exactly() {
    for bits in NON_FINITE {
        let bad = f64::from_bits(bits);
        let mut engine = loaded();
        let frame = run(&mut engine, 0.0, &entries(bad, 0.0, false, 2));
        assert_eq!(frame.sequence(), 1);
        // The accepted input keeps its exact bits, payload and sign included.
        let accepted = frame
            .inputs()
            .iter()
            .find(|(path, _)| path == U)
            .map(|(_, value)| value.as_real().unwrap().to_bits());
        assert_eq!(accepted, Some(bits));
        // Add: bad + IntegerToReal(2). An infinity plus a finite value is that infinity exactly;
        // NaN of any pattern stays NaN (its payload is the platform's, so only the class is
        // pinned).
        let sum = frame.outputs()[SUM].1.as_real().unwrap();
        if bad.is_nan() {
            assert!(sum.is_nan(), "{bits:#018x}: {sum}");
        } else {
            assert_eq!(sum.to_bits(), bits, "{bits:#018x}");
        }
    }
}

#[test]
fn admit_keeps_declared_bounds_refusing_with_input_domain() {
    let engine = loaded();
    // `reset` is bounded to [-10, 10]: NaN fails both comparisons, each infinity one of them.
    for bits in NON_FINITE {
        assert!(
            matches!(
                engine.prepare_frame(0.0, &entries(1.0, f64::from_bits(bits), false, 2)),
                Err(OcError::InputDomain(path)) if path == RESET
            ),
            "{bits:#018x}"
        );
    }
}

#[test]
fn an_explicit_admit_is_bit_identical_to_the_untouched_default() {
    // Includes non-finite samples, so the comparison covers the NaN-latching path too.
    let script = [
        (0.0, entries(2.0, 0.0, false, 1)),
        (1.0, entries(f64::NAN, 0.0, false, 1)),
        (2.0, entries(f64::INFINITY, 3.0, true, 2)),
        (2.0, entries(-1.0, -4.0, false, 0)),
        (5.5, entries(f64::NEG_INFINITY, 0.0, true, -3)),
    ];
    let mut default = loaded();
    let mut explicit = loaded();
    explicit.set_non_finite_input_policy(NonFiniteInputPolicy::Refuse);
    explicit.set_non_finite_input_policy(NonFiniteInputPolicy::Admit);
    for (time, frame) in &script {
        let expected = run(&mut default, *time, frame);
        let actual = run(&mut explicit, *time, frame);
        assert_eq!(output_bits(&actual), output_bits(&expected), "t={time}");
        assert_eq!(actual.sequence(), expected.sequence());
        assert_eq!(snapshot(&explicit), snapshot(&default), "t={time}");
    }
}

#[test]
fn switching_policy_between_frames_governs_only_later_frames() {
    let mut engine = loaded();
    let nan = entries(f64::NAN, 0.0, false, 1);
    assert_eq!(run(&mut engine, 0.0, &nan).sequence(), 1);

    engine.set_non_finite_input_policy(NonFiniteInputPolicy::Refuse);
    let before = snapshot(&engine);
    assert!(matches!(
        engine.prepare_frame(1.0, &nan),
        Err(OcError::InputNonFinite(path)) if path == U
    ));
    assert_eq!(snapshot(&engine), before);
    // A finite frame is still accepted, and the refusal consumed no sequence number.
    assert_eq!(
        run(&mut engine, 1.0, &entries(1.0, 0.0, false, 1)).sequence(),
        2
    );

    engine.set_non_finite_input_policy(NonFiniteInputPolicy::Admit);
    let frame = run(&mut engine, 2.0, &entries(f64::INFINITY, 0.0, false, 1));
    assert_eq!(frame.sequence(), 3);
    assert_eq!(
        frame.outputs()[SUM].1.as_real().unwrap().to_bits(),
        f64::INFINITY.to_bits()
    );
}

#[test]
fn a_plan_prepared_under_admit_cannot_execute_a_non_finite_value_under_refuse() {
    for bits in NON_FINITE {
        let bad = f64::from_bits(bits);
        let mut engine = loaded();
        run(&mut engine, 0.0, &entries(1.0, 0.0, false, 1));
        let before = snapshot(&engine);
        let plan = engine
            .prepare_frame(1.0, &entries(bad, 0.0, false, 1))
            .expect("Admit prepares a non-finite unbounded input");
        engine.set_non_finite_input_policy(NonFiniteInputPolicy::Refuse);
        let error = engine.execute_frame(plan).unwrap_err();
        assert!(
            matches!(&error, OcError::InputNonFinite(path) if path == U),
            "{bits:#018x}: {error:?}"
        );
        assert_eq!(snapshot(&engine), before, "{bits:#018x}");
        // Time did not advance and no sequence was consumed: t=1 is still accepted as next.
        assert_eq!(
            run(&mut engine, 1.0, &entries(1.0, 0.0, false, 1)).sequence(),
            2
        );
    }
}

#[test]
fn finite_plans_pass_the_execution_recheck_in_either_direction() {
    // Only unbounded `u` can carry a non-finite value under Admit; a bounded one is refused at
    // preparation already. A plan with finite values passes the recheck in either direction.
    let mut engine = loaded();
    let finite = engine
        .prepare_frame(0.0, &entries(-0.0, 10.0, true, 4))
        .unwrap();
    engine.set_non_finite_input_policy(NonFiniteInputPolicy::Refuse);
    let frame = engine.execute_frame(finite).unwrap();
    assert_eq!(frame.sequence(), 1);
    // Hand-derived: Add(-0.0, 4.0) = 4.0.
    assert_eq!(
        frame.outputs()[SUM].1.as_real().unwrap().to_bits(),
        4.0_f64.to_bits()
    );

    let strict = engine
        .prepare_frame(1.0, &entries(f64::MAX, -10.0, false, 0))
        .unwrap();
    engine.set_non_finite_input_policy(NonFiniteInputPolicy::Admit);
    let frame = engine.execute_frame(strict).unwrap();
    assert_eq!(frame.sequence(), 2);
    assert_eq!(
        frame.outputs()[SUM].1.as_real().unwrap().to_bits(),
        f64::MAX.to_bits()
    );
}

#[test]
fn changing_policy_mutates_no_engine_state_and_does_not_stale_a_plan() {
    let mut engine = loaded();
    run(&mut engine, 0.0, &entries(1.0, 0.0, false, 1));
    let before = snapshot(&engine);
    let plan = engine
        .prepare_frame(3.0, &entries(2.0, 0.0, false, 1))
        .unwrap();
    for policy in [
        NonFiniteInputPolicy::Refuse,
        NonFiniteInputPolicy::Refuse,
        NonFiniteInputPolicy::Admit,
        NonFiniteInputPolicy::Refuse,
    ] {
        engine.set_non_finite_input_policy(policy);
        assert_eq!(engine.non_finite_input_policy(), policy);
        assert_eq!(snapshot(&engine), before);
    }
    // The outstanding plan is neither stale nor refused: it carries only finite values.
    assert_eq!(engine.execute_frame(plan).unwrap().sequence(), 2);
}

#[test]
fn snapshots_do_not_carry_the_policy_and_restore_keeps_the_host_choice() {
    let script = [
        (0.0, entries(2.0, 0.0, false, 1)),
        (1.5, entries(-3.0, 0.0, true, 2)),
    ];
    let mut admit = loaded();
    let mut refuse = loaded();
    refuse.set_non_finite_input_policy(NonFiniteInputPolicy::Refuse);
    for (time, frame) in &script {
        assert_eq!(
            output_bits(&run(&mut admit, *time, frame)),
            output_bits(&run(&mut refuse, *time, frame))
        );
    }
    let bytes = snapshot(&refuse);
    assert_eq!(
        bytes,
        snapshot(&admit),
        "the policy is not in snapshot bytes"
    );

    // A fresh Refuse target restores an Admit capture and keeps refusing; the reverse holds too.
    let captured = admit.state_snapshot().unwrap();
    let mut target = Engine::in_memory();
    target.set_non_finite_input_policy(NonFiniteInputPolicy::Refuse);
    target.load_cxf(MODEL).unwrap();
    target.restore_state(&captured).unwrap();
    assert_eq!(
        target.non_finite_input_policy(),
        NonFiniteInputPolicy::Refuse
    );
    assert!(matches!(
        target.prepare_frame(2.0, &entries(f64::NAN, 0.0, false, 1)),
        Err(OcError::InputNonFinite(path)) if path == U
    ));
    let checkpoint = refuse.checkpoint().unwrap();
    admit.restore_checkpoint(&checkpoint).unwrap();
    assert_eq!(admit.non_finite_input_policy(), NonFiniteInputPolicy::Admit);
    assert!(
        admit
            .prepare_frame(2.0, &entries(f64::NAN, 0.0, false, 1))
            .is_ok()
    );
}

#[test]
fn a_replay_record_with_a_non_finite_input_replays_only_under_admit() {
    let mut source = loaded();
    run(&mut source, 0.0, &entries(1.0, 0.0, false, 1));
    let completed = run(&mut source, 1.0, &entries(f64::NEG_INFINITY, 0.0, false, 1));
    let record = ReplayRecord::from_bytes(completed.replay_record().unwrap().as_bytes()).unwrap();
    let inputs: Vec<_> = record
        .inputs()
        .iter()
        .map(|(path, value)| (path.as_str(), value.clone()))
        .collect();

    let replay_target = |policy| {
        let mut engine = Engine::in_memory();
        engine.set_non_finite_input_policy(policy);
        engine.load_cxf(MODEL).unwrap();
        run(&mut engine, 0.0, &entries(1.0, 0.0, false, 1));
        engine
    };
    let strict = replay_target(NonFiniteInputPolicy::Refuse);
    assert!(matches!(
        strict.prepare_frame(record.time(), &inputs),
        Err(OcError::InputNonFinite(path)) if path == U
    ));
    let mut lenient = replay_target(NonFiniteInputPolicy::Admit);
    let plan = lenient.prepare_frame(record.time(), &inputs).unwrap();
    let replayed = lenient.execute_frame(plan).unwrap();
    record.verify(&replayed).unwrap();
    assert_eq!(output_bits(&replayed), output_bits(&completed));
}

#[test]
fn integer_boolean_and_time_refusals_are_independent_of_the_policy() {
    for policy in [NonFiniteInputPolicy::Admit, NonFiniteInputPolicy::Refuse] {
        let mut engine = loaded();
        engine.set_non_finite_input_policy(policy);
        run(
            &mut engine,
            1.0,
            &entries(0.0, 0.0, true, i64::from(i32::MAX)),
        );
        for time in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(matches!(
                engine.prepare_frame(time, &entries(0.0, 0.0, false, 0)),
                Err(OcError::NonFiniteTime { .. })
            ));
        }
        assert!(matches!(
            engine.prepare_frame(0.5, &entries(f64::NAN, 0.0, false, 0)),
            Err(OcError::TimeRegression { .. })
        ));
        let mut typed = entries(f64::NAN, 0.0, false, 0);
        typed[3] = (N, Value::Real(f64::NAN));
        assert!(matches!(
            engine.prepare_frame(2.0, &typed),
            Err(OcError::InputType(path)) if path == N
        ));
    }
}

#[test]
fn execution_recheck_names_the_first_canonical_non_finite_input() {
    let mut probe = Engine::in_memory();
    probe.load_cxf(TWO_INPUTS).unwrap();
    let schema: Vec<_> = probe
        .input_definitions()
        .unwrap()
        .into_iter()
        .map(|definition| {
            (
                definition.path,
                definition.min.is_none(),
                definition.max.is_none(),
            )
        })
        .collect();
    assert_eq!(
        schema,
        [(U1.to_owned(), true, true), (U2.to_owned(), true, true)]
    );
    let nan = f64::NAN;
    let inf = f64::INFINITY;
    for (u1, u2, named) in [
        (nan, -inf, U1),
        (inf, nan, U1),
        (1.0, nan, U2),
        (-inf, 0.0, U1),
    ] {
        for reversed in [false, true] {
            let mut entries = vec![(U1, Value::Real(u1)), (U2, Value::Real(u2))];
            if reversed {
                entries.reverse();
            }
            let mut engine = Engine::in_memory();
            engine.load_cxf(TWO_INPUTS).unwrap();
            let plan = engine.prepare_frame(0.0, &entries).expect("Admit prepares");
            engine.set_non_finite_input_policy(NonFiniteInputPolicy::Refuse);
            let error = engine.execute_frame(plan).unwrap_err();
            assert!(
                matches!(&error, OcError::InputNonFinite(path) if path == named),
                "u1={u1} u2={u2}: {error:?}"
            );
            // Preparation under Refuse names the same input.
            assert!(matches!(
                engine.prepare_frame(0.0, &entries),
                Err(OcError::InputNonFinite(path)) if path == named
            ));
        }
    }
}

#[test]
fn incarnation_and_time_refusals_precede_the_execution_recheck() {
    let nan_frame = entries(f64::NAN, 0.0, false, 1);
    // Stale: a successful reload supersedes the plan's incarnation.
    let mut engine = loaded();
    let plan = engine.prepare_frame(0.0, &nan_frame).unwrap();
    engine.load_cxf(MODEL).unwrap();
    engine.set_non_finite_input_policy(NonFiniteInputPolicy::Refuse);
    assert!(matches!(
        engine.execute_frame(plan),
        Err(OcError::StalePreparedFrame)
    ));

    // Readiness causes (unloaded, pending edits) and the remaining time causes are swept in
    // the crate-private preflight matrix, which can inject them into a non-finite plan.

    // Time regression: a later finite plan executes first.
    let mut engine = loaded();
    let early = engine.prepare_frame(1.0, &nan_frame).unwrap();
    let late = engine
        .prepare_frame(2.0, &entries(1.0, 0.0, false, 1))
        .unwrap();
    engine.execute_frame(late).unwrap();
    engine.set_non_finite_input_policy(NonFiniteInputPolicy::Refuse);
    let before = snapshot(&engine);
    assert!(matches!(
        engine.execute_frame(early),
        Err(OcError::TimeRegression { now, prev }) if now == 1.0 && prev == 2.0
    ));
    assert_eq!(snapshot(&engine), before);
}
