//! Fresh and advanced stateful refusal image controls. Snapshot bytes include connector values,
//! words and both clocks; output/watch comparisons are separate. Expected refusal classes and
//! bounds are authored here, not derived from the implementation. No frame-execution claim.

mod support;

use std::sync::Arc;

use oce_api::{Engine, EngineCheckpoint, EngineStateError, OcError, Value};
use support::recording_store::{RecordingStore, StoreCallSnapshot};

const MODEL: &[u8] = include_bytes!("fixtures/frame_delay.jsonld");
const A: &str = "urn:frame:a";
const B: &str = "urn:frame:b";
const Y: &str = "urn:frame:y";

fn complete() -> Vec<(&'static str, Value)> {
    vec![(A, Value::Real(-0.0)), (B, Value::Real(2.0))]
}

#[test]
fn every_refusal_preserves_fresh_and_advanced_stateful_images_and_store() {
    for advanced in [false, true] {
        let mut engine = Engine::with_store(Arc::new(RecordingStore::default()));
        let report = engine.load_cxf(MODEL).unwrap();
        assert_eq!(report.stateful_blocks, 1);
        assert!(report.warnings.is_empty());
        if advanced {
            engine.set_input(A, Value::Real(0.5)).unwrap();
            engine.set_input(B, Value::Real(2.0)).unwrap();
            engine.tick(0.0).unwrap();
            engine.tick(4.0).unwrap();
        }
        let snapshot = engine.state_snapshot().unwrap();
        let checkpoint = engine.checkpoint().unwrap();
        let checkpoint_before = checkpoint_image(&checkpoint);
        let outputs: Vec<_> = engine.outputs().iter().collect();
        let watched = engine.watch(&[Y, "urn:frame:delay.y"]).unwrap();
        engine.store().reset_calls();
        engine.store().arm_hot_path_guard();
        let mut cases = vec![
            (
                4.0,
                vec![(A, Value::Real(0.0)), (A, Value::Real(0.0))],
                "duplicate",
            ),
            (4.0, vec![(B, Value::Real(2.0))], "missing"),
            (
                4.0,
                vec![
                    (A, Value::Real(0.0)),
                    (B, Value::Real(0.0)),
                    ("unknown", Value::Real(0.0)),
                ],
                "unknown",
            ),
            (4.0, vec![(Y, Value::Real(0.0))], "direction"),
            (
                4.0,
                vec![("urn:frame:delay.u", Value::Real(0.0))],
                "direction",
            ),
            (
                4.0,
                vec![(A, Value::Boolean(false)), (B, Value::Real(0.0))],
                "type",
            ),
            (
                4.0,
                vec![(A, Value::Real(2.0)), (B, Value::Real(0.0))],
                "domain",
            ),
            (
                4.0,
                vec![(A, Value::Real(f64::NAN)), (B, Value::Real(0.0))],
                "domain",
            ),
            (
                4.0,
                vec![(A, Value::Real(f64::INFINITY)), (B, Value::Real(0.0))],
                "domain",
            ),
            (
                4.0,
                vec![(A, Value::Real(f64::NEG_INFINITY)), (B, Value::Real(0.0))],
                "domain",
            ),
            (f64::NAN, complete(), "time"),
            (f64::INFINITY, complete(), "time"),
            (f64::NEG_INFINITY, complete(), "time"),
            (f64::MAX, complete(), "representability"),
        ];
        if advanced {
            cases.push((3.0, complete(), "regression"));
        }
        for (time, entries, kind) in cases {
            let error = engine.prepare_frame(time, &entries).unwrap_err();
            match kind {
                "duplicate" => assert!(matches!(error, OcError::FrameDuplicateInput(p) if p == A)),
                "missing" => assert!(matches!(error, OcError::FrameMissingInput(p) if p == A)),
                "unknown" => assert!(
                    matches!(error, OcError::FrameUnknownInput { prefix, .. } if prefix == "unknown")
                ),
                "direction" => assert!(matches!(error, OcError::FrameNotInput { .. })),
                "type" => assert!(matches!(error, OcError::InputType(p) if p == A)),
                "domain" => assert!(matches!(error, OcError::InputDomain(p) if p == A)),
                "time" => assert!(
                    matches!(error, OcError::NonFiniteTime { now } if now.to_bits() == time.to_bits())
                ),
                "representability" => {
                    assert!(matches!(error, OcError::ModelTimeUnrepresentable { .. }))
                }
                "regression" => assert!(matches!(error, OcError::TimeRegression { .. })),
                _ => unreachable!(),
            }
            assert_eq!(
                engine.state_snapshot().unwrap().as_bytes(),
                snapshot.as_bytes(),
                "{kind}"
            );
            for ((id, before), (after_id, after)) in outputs.iter().zip(engine.outputs().iter()) {
                assert_eq!(*id, after_id);
                assert!(before.bit_eq(after));
            }
            for ((key, before), (after_key, after)) in watched
                .iter()
                .zip(engine.watch(&[Y, "urn:frame:delay.y"]).unwrap())
            {
                assert_eq!(*key, after_key);
                assert!(before.bit_eq(&after));
            }
            assert_eq!(engine.store().calls(), StoreCallSnapshot::default());
            assert_eq!(
                checkpoint_image(&engine.checkpoint().unwrap()),
                checkpoint_before
            );
        }
        for value in [-1.0, -0.0, 0.0, f64::from_bits(1), f64::MIN_POSITIVE, 1.0] {
            engine
                .prepare_frame(4.0, &[(A, Value::Real(value)), (B, Value::Real(f64::NAN))])
                .unwrap();
        }
        assert_eq!(
            engine.state_snapshot().unwrap().as_bytes(),
            snapshot.as_bytes()
        );
        assert_eq!(engine.store().calls(), StoreCallSnapshot::default());
        if !advanced {
            engine.restore_state(&snapshot).unwrap();
        }
        engine.restore_checkpoint(&checkpoint).unwrap();
        assert_eq!(
            engine.state_snapshot().unwrap().as_bytes(),
            snapshot.as_bytes()
        );
    }
}

#[test]
fn unloaded_and_pending_edits_are_distinct_from_an_empty_complete_schema() {
    let mut engine = Engine::in_memory();
    assert!(matches!(
        engine.prepare_frame(f64::NAN, &[]),
        Err(OcError::State(EngineStateError::NoLoadedModel))
    ));
    assert!(matches!(
        engine.input_definitions(),
        Err(OcError::State(EngineStateError::NoLoadedModel))
    ));
    engine.load_cxf(MODEL).unwrap();
    engine.halt().unwrap();
    engine
        .set_param("urn:frame:delay.samplePeriod", Value::Real(2.0))
        .unwrap();
    assert!(matches!(
        engine.prepare_frame(f64::NAN, &[]),
        Err(OcError::State(EngineStateError::PendingParameterEdits))
    ));
    engine.resume().unwrap();
    engine.prepare_frame(0.0, &complete()).unwrap();
}

fn checkpoint_image(checkpoint: &EngineCheckpoint) -> Vec<u8> {
    // The checkpoint is intentionally opaque. Observe its entire restored image in a separate
    // compatible engine, without closing the tested engine's fresh durable-restore window.
    let mut observer = Engine::in_memory();
    observer.load_cxf(MODEL).unwrap();
    observer.restore_checkpoint(checkpoint).unwrap();
    observer.state_snapshot().unwrap().into_bytes()
}
