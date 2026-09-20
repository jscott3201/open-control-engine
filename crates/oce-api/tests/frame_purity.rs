//! Store-free complete-frame execution across the fixture corpus and hostile Store samples.
//! Corpus inputs are explicit synthetic in-domain observations, not a host fallback policy or
//! a correctness oracle. Independent numerical evidence remains in the sequence suites.

mod support;

use oce_api::oce_store::{
    DomainKey, Durability, OcValue, PointSample, PointStatus, PointStore, PointWrite,
};
use oce_api::{Engine, OcError, Value};
use std::sync::Arc;
use support::recording_store::{RecordingStore, StoreCallSnapshot};

#[test]
fn complete_corpus_frames_never_read_or_write_the_store() {
    let directory =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../oce-cxf/tests/fixtures/g36");
    let mut paths: Vec<_> = std::fs::read_dir(directory)
        .unwrap()
        .map(|p| p.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "jsonld"))
        .collect();
    paths.sort();
    assert_eq!(paths.len(), 47);
    for path in paths {
        let mut engine = Engine::with_store(Arc::new(RecordingStore::default()));
        engine.load_cxf(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            engine.store().calls().save_model,
            1,
            "load counter positive control"
        );
        let inputs: Vec<_> = engine
            .input_definitions()
            .unwrap()
            .into_iter()
            .map(|d| {
                let value = d.min.unwrap_or_else(|| d.value_type.zero_value());
                (d.path, value)
            })
            .collect();
        let entries: Vec<_> = inputs
            .iter()
            .map(|(p, v)| (p.as_str(), v.clone()))
            .collect();
        engine.store().reset_calls();
        engine.store().arm_hot_path_guard();
        for (index, time) in [0.0, 0.0, 1.0].into_iter().enumerate() {
            let prepared = engine.prepare_frame(time, &entries).unwrap();
            let completed = engine.execute_frame(prepared).unwrap();
            assert_eq!(completed.sequence(), index as u64 + 1);
            assert!(completed.outputs().windows(2).all(|w| w[0].0 < w[1].0));
        }
        assert_eq!(
            engine.store().calls(),
            StoreCallSnapshot::default(),
            "{}",
            path.display()
        );
    }
}

#[test]
fn store_samples_neither_supply_missing_determinants_nor_overwrite_complete_values() {
    const A: &str = "urn:legacy-frame:a";
    const B: &str = "urn:legacy-frame:b";
    for status in [PointStatus::Ok, PointStatus::Fault] {
        let mut engine = Engine::with_store(Arc::new(RecordingStore::default()));
        engine
            .load_cxf(include_bytes!("fixtures/legacy_frame_add.jsonld"))
            .unwrap();
        // Include a wrong-typed sample and a stale extreme timestamp: neither is an execution input.
        engine
            .store()
            .write_points(&[
                PointWrite {
                    key: DomainKey::new(A),
                    sample: PointSample {
                        value: OcValue::Real(99.0),
                        status,
                        at_unix_nanos: u64::MAX,
                    },
                    durability: Durability::Telemetry,
                },
                PointWrite {
                    key: DomainKey::new(B),
                    sample: PointSample {
                        value: OcValue::Bool(true),
                        status,
                        at_unix_nanos: 0,
                    },
                    durability: Durability::Telemetry,
                },
            ])
            .unwrap();
        let before = engine.state_snapshot().unwrap();
        engine.store().reset_calls();
        engine.store().arm_hot_path_guard();
        assert!(
            matches!(engine.prepare_frame(0.0, &[(A, Value::Real(1.0))]),
            Err(OcError::FrameMissingInput(path)) if path == B)
        );
        assert_eq!(
            before.as_bytes(),
            engine.state_snapshot().unwrap().as_bytes()
        );
        let plan = engine
            .prepare_frame(0.0, &[(A, Value::Real(1.0)), (B, Value::Real(2.0))])
            .unwrap();
        let frame = engine.execute_frame(plan).unwrap();
        assert!(frame.outputs()[0].1.bit_eq(&Value::Real(3.0)));
        assert_eq!(engine.store().calls(), StoreCallSnapshot::default());
    }
}

#[test]
fn guarded_snapshot_positive_control_cannot_silently_pass() {
    let store = RecordingStore::default();
    store.arm_hot_path_guard();
    let refusal = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| store.snapshot()));
    assert!(
        refusal.is_err(),
        "the guard must detect an injected snapshot call"
    );
    assert_eq!(store.calls().snapshot, 1);
}
