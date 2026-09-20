//! Public lifecycle refusals preserve snapshots, checkpoint-restored images, output/watch bits and
//! every recorded Store call. These are preservation controls, not an independent numeric oracle.

mod support;

use oce_api::{Engine, EngineStateError, OcError, Value};
use std::sync::Arc;
use support::recording_store::{RecordingStore, StoreCallSnapshot};

const MODEL: &[u8] = include_bytes!("fixtures/frame_delay.jsonld");
const Y: &str = "urn:frame:y";

fn entries() -> [(&'static str, Value); 2] {
    [
        ("urn:frame:a", Value::Real(-0.0)),
        ("urn:frame:b", Value::Real(2.0)),
    ]
}

fn values_equal(left: &[(String, Value)], right: &[(String, Value)]) {
    assert_eq!(left.len(), right.len());
    for ((key, value), (other_key, other_value)) in left.iter().zip(right) {
        assert_eq!(key, other_key);
        assert!(value.bit_eq(other_value));
    }
}

#[test]
fn context_readiness_and_time_refusals_preserve_public_images_and_store_calls() {
    for advanced in [false, true] {
        for cause in ["cross-engine", "reload", "dirty", "dirty-resume", "time"] {
            let mut engine = Engine::with_store(Arc::new(RecordingStore::default()));
            engine.load_cxf(MODEL).unwrap();
            let retained = if advanced {
                let plan = engine.prepare_frame(0.0, &entries()).unwrap();
                Some(engine.execute_frame(plan).unwrap())
            } else {
                None
            };
            let mut plan = engine.prepare_frame(0.0, &entries()).unwrap();
            match cause {
                "cross-engine" => {
                    let mut other = Engine::in_memory();
                    other.load_cxf(MODEL).unwrap();
                    plan = other.prepare_frame(0.0, &entries()).unwrap();
                }
                "reload" => {
                    engine.load_cxf(MODEL).unwrap();
                }
                "dirty" | "dirty-resume" => {
                    engine.halt().unwrap();
                    engine
                        .set_param("urn:frame:delay.samplePeriod", Value::Real(1.0))
                        .unwrap();
                    if cause == "dirty-resume" {
                        engine.resume().unwrap();
                    }
                }
                "time" => {
                    let advance = engine.prepare_frame(1.0, &entries()).unwrap();
                    engine.execute_frame(advance).unwrap();
                }
                _ => unreachable!(),
            }
            let snapshot = engine
                .state_snapshot()
                .map(|s| s.into_bytes())
                .map_err(|e| e.to_string());
            let checkpoint = engine.checkpoint();
            let output = engine.get_output(Y).unwrap();
            let watch = engine.watch(&[Y, "urn:frame:delay.y"]).unwrap();
            let retained_image = format!("{retained:?}");
            engine.store().reset_calls();
            engine.store().arm_hot_path_guard();
            let error = engine.execute_frame(plan).unwrap_err();
            assert!(match cause {
                "dirty" => matches!(
                    error,
                    OcError::State(EngineStateError::PendingParameterEdits)
                ),
                "time" => matches!(error, OcError::TimeRegression { .. }),
                _ => matches!(error, OcError::StalePreparedFrame),
            });
            assert_eq!(
                engine
                    .state_snapshot()
                    .map(|s| s.into_bytes())
                    .map_err(|e| e.to_string()),
                snapshot
            );
            assert!(engine.get_output(Y).unwrap().bit_eq(&output));
            values_equal(&engine.watch(&[Y, "urn:frame:delay.y"]).unwrap(), &watch);
            assert_eq!(format!("{retained:?}"), retained_image);
            assert_eq!(engine.store().calls(), StoreCallSnapshot::default());
            if let Ok(checkpoint) = checkpoint {
                let mut observer = Engine::in_memory();
                observer.load_cxf(MODEL).unwrap();
                observer.restore_checkpoint(&checkpoint).unwrap();
                assert_eq!(
                    observer.state_snapshot().unwrap().as_bytes(),
                    snapshot.as_ref().unwrap()
                );
                let after = engine.checkpoint().unwrap();
                observer.restore_checkpoint(&after).unwrap();
                assert_eq!(
                    observer.state_snapshot().unwrap().as_bytes(),
                    snapshot.as_ref().unwrap()
                );
                if cause == "reload" || (!advanced && cause == "cross-engine") {
                    let snapshot = observer.state_snapshot().unwrap();
                    engine.restore_state(&snapshot).unwrap(); // refusal left fresh restore window open
                }
            } else {
                engine.resume().unwrap();
            }
            let next = engine.prepare_frame(1.0, &entries()).unwrap();
            assert_eq!(
                engine.execute_frame(next).unwrap().sequence(),
                1 + u64::from(advanced) + u64::from(cause == "time")
            );
            assert_eq!(engine.store().calls(), StoreCallSnapshot::default());
        }
    }
}

#[test]
fn unloaded_target_refuses_a_moved_plan_and_failed_load_preserves_a_usable_plan() {
    let mut source = Engine::in_memory();
    source.load_cxf(MODEL).unwrap();
    let plan = source.prepare_frame(0.0, &entries()).unwrap();
    let mut target = Engine::with_store(Arc::new(RecordingStore::default()));
    assert!(matches!(
        target.execute_frame(plan),
        Err(OcError::State(EngineStateError::NoLoadedModel))
    ));
    assert!(target.watch(&[]).unwrap().is_empty());
    assert_eq!(target.store().calls(), StoreCallSnapshot::default());
    let plan = source.prepare_frame(0.0, &entries()).unwrap();
    assert!(matches!(source.load_cxf(b"{"), Err(OcError::Cxf(_))));
    assert_eq!(source.execute_frame(plan).unwrap().sequence(), 1);
}

#[test]
fn latest_state_inspections_never_consume_positions_or_change_retained_receipts() {
    let mut engine = Engine::in_memory();
    engine.load_cxf(MODEL).unwrap();
    let plan = engine.prepare_frame(0.0, &entries()).unwrap();
    let retained = engine.execute_frame(plan).unwrap();
    let retained_image = format!("{retained:?}");
    for _ in 0..3 {
        let output = engine.get_output(Y).unwrap();
        let watched = engine.watch(&[Y, Y]).unwrap();
        assert!(watched.iter().all(|(_, value)| value.bit_eq(&output)));
    }
    let plan = engine.prepare_frame(2.0, &entries()).unwrap();
    assert_eq!(engine.execute_frame(plan).unwrap().sequence(), 2);
    assert_eq!(format!("{retained:?}"), retained_image);
}
