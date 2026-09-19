//! Passing gap evidence for the complete-frame contract, not a future frame implementation.
//!
//! Independent arithmetic oracle: Add's default gains are one, so the explicit input tuples
//! (0,0), (0,2), (8,2), (3,2) yield exactly 0, 2, 10, 5 in binary64. The checked-in output golden
//! is hand-authored. Snapshot comparisons are current-state/determinism evidence, not an external
//! solver oracle or a claim about future generation/replay fields. Existing Pre tests cover words.

use std::fmt::Write as _;

use oce_api::oce_store::{
    DomainKey, Durability, OcValue, PointSample, PointStatus, PointStore, PointWrite,
};
use oce_api::{Engine, EngineStateError, OcError, Value};

const MODEL: &[u8] = include_bytes!("fixtures/legacy_frame_add.jsonld");
const A: &str = "urn:legacy-frame:a";
const B: &str = "urn:legacy-frame:b";
const Y: &str = "urn:legacy-frame:y";

fn loaded() -> Engine {
    let mut engine = Engine::in_memory();
    let report = engine.load_cxf(MODEL).unwrap();
    assert_eq!(report.block_count, 1);
    assert!(report.warnings.is_empty());
    engine
}

fn image(engine: &Engine) -> Vec<u8> {
    engine.state_snapshot().unwrap().as_bytes().to_vec()
}

fn assert_output(engine: &Engine, expected: f64) {
    assert!(engine.get_output(Y).unwrap().bit_eq(&Value::Real(expected)));
    let outputs: Vec<_> = engine.outputs().iter().collect();
    assert_eq!(outputs.len(), 1);
    assert!(outputs[0].1.bit_eq(&Value::Real(expected)));
}

fn write(engine: &Engine, point: &str, value: OcValue) {
    engine
        .store()
        .write_points(&[PointWrite {
            key: DomainKey::new(point),
            sample: PointSample {
                value,
                status: PointStatus::Ok,
                at_unix_nanos: 0,
            },
            durability: Durability::Telemetry,
        }])
        .unwrap();
}

#[test]
fn setter_refusal_keeps_the_already_staged_prefix() {
    for (name, value, wrong_type) in [
        ("urn:legacy-frame:unknown", Value::Real(4.0), false),
        (Y, Value::Real(4.0), false),
        (B, Value::Boolean(true), true),
    ] {
        let mut engine = loaded();
        engine.set_input(B, Value::Real(2.0)).unwrap();
        engine.tick(4.0).unwrap();
        let before = image(&engine);
        engine.set_input(A, Value::Real(8.0)).unwrap();
        let staged = image(&engine);
        assert_ne!(
            before, staged,
            "the successful prefix really mutated the image"
        );
        let error = engine.set_input(name, value).unwrap_err();
        if wrong_type {
            assert!(matches!(error, OcError::InputType(p) if p == name));
        } else {
            assert!(matches!(error, OcError::UnknownPoint(p) if p == name));
        }
        assert_eq!(
            image(&engine),
            staged,
            "only the individual bad setter is atomic"
        );
        assert_output(&engine, 2.0);
        engine.tick(4.0).unwrap();
        assert_output(&engine, 10.0);
    }
}

#[test]
fn store_type_refusal_keeps_a_prefix_without_advancing_execution() {
    let mut repetitions = Vec::new();
    for _ in 0..2 {
        let mut engine = loaded();
        let fresh = engine.state_snapshot().unwrap();
        let mut prefix_only = loaded();
        prefix_only.set_input(A, Value::Real(8.0)).unwrap();
        write(&engine, A, OcValue::Real(8.0));
        write(&engine, B, OcValue::Bool(true));

        assert!(matches!(engine.tick(10.0), Err(OcError::InputType(p)) if p == B));
        let refused = image(&engine);
        assert_ne!(refused, fresh.as_bytes());
        assert_eq!(
            refused,
            image(&prefix_only),
            "prefix retained, clock/output not advanced"
        );
        assert_output(&engine, 0.0);
        assert!(matches!(
            engine.restore_state(&fresh),
            Err(OcError::State(EngineStateError::DurableTargetAdvanced))
        ));
        assert_eq!(image(&engine), refused);

        write(&engine, B, OcValue::Real(2.0));
        engine.tick(4.0).unwrap(); // Failed time 10 never armed the monotonic guard.
        assert_output(&engine, 10.0);
        repetitions.push((refused, image(&engine)));
    }
    assert_eq!(
        repetitions[0], repetitions[1],
        "both refusal and continuation repeat exactly"
    );
}

#[test]
fn time_refusal_does_not_undo_prior_setters() {
    for time in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 3.0] {
        let mut engine = loaded();
        engine.set_input(A, Value::Real(1.0)).unwrap();
        engine.set_input(B, Value::Real(2.0)).unwrap();
        engine.tick(4.0).unwrap();
        let completed = image(&engine);
        engine.set_input(A, Value::Real(8.0)).unwrap();
        let staged = image(&engine);
        assert_ne!(staged, completed);
        let error = engine.tick(time).unwrap_err();
        if time.is_finite() {
            assert!(matches!(error, OcError::TimeRegression { now, prev }
                if now.to_bits() == time.to_bits() && prev.to_bits() == 4.0_f64.to_bits()));
        } else {
            assert!(
                matches!(error, OcError::NonFiniteTime { now } if now.to_bits() == time.to_bits())
            );
        }
        assert_eq!(image(&engine), staged);
        assert_output(&engine, 3.0);
        engine.tick(4.0).unwrap();
        assert_output(&engine, 10.0);
    }
}

#[test]
fn sparse_and_repeated_writes_follow_hold_last_and_last_wins() {
    let mut images = Vec::new();
    for _ in 0..2 {
        let mut engine = loaded();
        let mut trace = String::new();
        for (label, pairs, expected) in [
            ("omitted", vec![], 0.0),
            ("sparse", vec![(B, 2.0)], 2.0),
            ("repeated", vec![(A, 3.0), (A, 8.0)], 10.0),
            ("held", vec![(A, 3.0)], 5.0),
        ] {
            for (name, value) in pairs {
                engine.set_input(name, Value::Real(value)).unwrap();
            }
            engine.tick(4.0).unwrap();
            assert_output(&engine, expected);
            let Value::Real(value) = engine.get_output(Y).unwrap() else {
                panic!("Add output is Real");
            };
            writeln!(trace, "{label}:{:016x}", value.to_bits()).unwrap();
        }
        assert_eq!(
            trace.as_bytes(),
            include_bytes!("fixtures/legacy_frame_outputs.txt")
        );
        images.push(image(&engine));
    }
    assert_eq!(images[0], images[1]);
}
