//! Public-only host composition of accepted-frame replay records. No implementation crate access.

mod replay_oracle;
mod support;

use allocation_counter::measure;
use oce_api::{
    CompatibilityDescriptor, Engine, EngineStateSnapshot, ReplayError, ReplayExactness,
    ReplayRecord, StatePortability, Value,
};
use std::hint::black_box;
use std::sync::Arc;
use support::recording_store::{RecordingStore, StoreCallSnapshot};

const ADD: &[u8] = include_bytes!("fixtures/legacy_frame_add.jsonld");

#[test]
fn accepted_inputs_survive_execution_reload_and_caller_buffer_reuse() {
    let mut engine = Engine::in_memory();
    engine.load_cxf(ADD).unwrap();
    let mut observations = vec![
        (
            "urn:legacy-frame:b".to_owned(),
            Value::Real(f64::from_bits(0x7ff8_0000_0000_0042)),
        ),
        ("urn:legacy-frame:a".to_owned(), Value::Real(-0.0)),
    ];
    let borrowed: Vec<_> = observations
        .iter()
        .map(|(p, v)| (p.as_str(), v.clone()))
        .collect();
    let plan = engine.prepare_frame(-0.0, &borrowed).unwrap();
    drop(borrowed);
    observations.clear();
    let accepted = engine.execute_frame(plan).unwrap();
    engine.load_cxf(ADD).unwrap();
    assert_eq!(accepted.inputs().len(), 2);
    assert_eq!(accepted.inputs()[0].0, "urn:legacy-frame:a");
    assert!(accepted.inputs()[0].1.bit_eq(&Value::Real(-0.0)));
    assert_eq!(accepted.inputs()[1].0, "urn:legacy-frame:b");
    assert!(
        accepted.inputs()[1]
            .1
            .bit_eq(&Value::Real(f64::from_bits(0x7ff8_0000_0000_0042)))
    );
}

#[test]
fn canonical_receipt_matches_independent_bytes_and_repeats_without_reexecution() {
    let expected = replay_oracle::add();
    assert_eq!(expected.len(), 389, "hand-counted wire size");
    for _ in 0..3 {
        let mut engine = Engine::with_store(Arc::new(RecordingStore::default()));
        engine.load_cxf(ADD).unwrap();
        engine.store().reset_calls();
        engine.store().arm_hot_path_guard();
        let plan = engine
            .prepare_frame(
                -0.0,
                &[
                    ("urn:legacy-frame:b", Value::Real(2.25)),
                    ("urn:legacy-frame:a", Value::Real(1.5)),
                ],
            )
            .unwrap();
        let completed = engine.execute_frame(plan).unwrap();
        let end = engine.state_snapshot().unwrap();
        for _ in 0..3 {
            let record = completed.replay_record().unwrap();
            assert_eq!(record.as_bytes(), expected);
            assert_eq!(record.descriptor(), replay_oracle::DESCRIPTOR);
            assert_eq!(record.exactness(), ReplayExactness::ExactBits);
            assert_eq!(record.format_revision(), 1);
            assert_eq!(record.portability(), &StatePortability::Portable);
            assert_eq!(
                record.content_id().to_string(),
                format!(
                    "replay:1:fnv1a128:{:032x}",
                    replay_oracle::checksum(&expected)
                )
            );
            record.verify(&completed).unwrap();
        }
        assert_eq!(engine.state_snapshot().unwrap().as_bytes(), end.as_bytes());
        assert_eq!(engine.store().calls(), StoreCallSnapshot::default());
        drop(engine);
        assert_eq!(completed.replay_record().unwrap().as_bytes(), expected);
    }
}

#[test]
fn host_stream_continues_from_separate_snapshot_with_equal_times_and_exact_end_state() {
    // This entire host prototype imports ONLY oce_api (and test instrumentation). Envelope
    // authentication/build approval are assumed by this deterministic fixture, never simulated
    // by the content tag. The host, not OCE, owns this loop and the separately supplied sidecar.
    let model = include_bytes!("fixtures/frame_pre.jsonld");
    let mut producer = Engine::in_memory();
    producer.load_cxf(model).unwrap();
    let plan = producer.prepare_frame(-0.0, &[]).unwrap();
    assert_eq!(producer.execute_frame(plan).unwrap().sequence(), 1);
    let start_bytes = producer.state_snapshot().unwrap().into_bytes();
    let mut playback = Engine::in_memory();
    playback.load_cxf(model).unwrap();
    playback
        .restore_state(&EngineStateSnapshot::from_bytes(&start_bytes).unwrap())
        .unwrap();
    let descriptor = CompatibilityDescriptor::current(None).unwrap();
    let mut peak = None;
    let mut first = None;
    let count = 16_384;
    for index in 0..count {
        let inner = measure(|| {
            let time = if index == 0 { -0.0 } else { (index / 4) as f64 };
            let plan = producer.prepare_frame(time, &[]).unwrap();
            let completed = producer.execute_frame(plan).unwrap();
            assert_eq!(completed.sequence(), index + 2);
            // Hand-derived Pre/Not feedback alternates, even at equal timestamp.
            for (_, value) in completed.outputs() {
                assert!(value.bit_eq(&Value::Boolean(index % 2 == 0)));
            }
            let record = completed.replay_record().unwrap();
            let decoded = ReplayRecord::from_bytes(record.as_bytes()).unwrap();
            decoded.check_compatible(&descriptor).unwrap();
            let entries: Vec<_> = decoded
                .inputs()
                .iter()
                .map(|(p, v)| (p.as_str(), v.clone()))
                .collect();
            let plan = playback.prepare_frame(decoded.time(), &entries).unwrap();
            let replayed = playback.execute_frame(plan).unwrap();
            assert_eq!(
                replayed.sequence(),
                index + 1,
                "not durable sequence position"
            );
            decoded.verify(&replayed).unwrap();
            black_box(decoded);
        });
        assert_eq!(inner.bytes_current, 0);
        assert_eq!(inner.count_current, 0);
        if let Some(expected) = peak {
            assert_eq!(inner.bytes_max, expected);
        } else {
            peak = Some(inner.bytes_max);
        }
        if let Some(expected) = first {
            assert_eq!(inner.bytes_total, expected);
        } else {
            first = Some(inner.bytes_total);
        }
    }
    assert!(
        peak.unwrap() < 16_384,
        "bounded one-record workspace for this fixture"
    );
    assert_eq!(
        producer.state_snapshot().unwrap().as_bytes(),
        playback.state_snapshot().unwrap().as_bytes()
    );
    let positive = measure(|| {
        black_box(vec![0_u8; 32_768]);
    });
    assert_eq!(positive.bytes_max, 32_768);
    assert!(
        positive.bytes_max > peak.unwrap(),
        "counter observes whole-sequence-sized retention"
    );
}

#[test]
fn output_input_and_time_mismatches_are_exact_and_do_not_change_the_completed_engine() {
    let bytes = replay_oracle::add();
    let expected = ReplayRecord::from_bytes(&bytes).unwrap();
    let mut engine = Engine::in_memory();
    engine.load_cxf(ADD).unwrap();
    let entries: Vec<_> = expected
        .inputs()
        .iter()
        .map(|(p, v)| (p.as_str(), v.clone()))
        .collect();
    let plan = engine.prepare_frame(-0.0, &entries).unwrap();
    let actual = engine.execute_frame(plan).unwrap();
    let end = engine.state_snapshot().unwrap();
    for (offset, error) in [
        (replay_oracle::body_start() + 7, ReplayError::TimeMismatch),
        (
            replay_oracle::offset(&bytes, b"urn:legacy-frame:a") + 18 + 1,
            ReplayError::InputMismatch { index: 0 },
        ),
        (
            replay_oracle::offset(&bytes, b"urn:legacy-frame:y") + 18 + 1,
            ReplayError::OutputMismatch { index: 0 },
        ),
    ] {
        let mut changed = bytes.clone();
        changed[offset] ^= 1;
        replay_oracle::seal(&mut changed);
        for _ in 0..3 {
            let record = ReplayRecord::from_bytes(&changed).unwrap();
            assert_eq!(record.verify(&actual), Err(error.clone()));
            assert_eq!(engine.state_snapshot().unwrap().as_bytes(), end.as_bytes());
        }
    }
}

#[test]
fn warning_records_match_hand_bytes_and_replay_without_severity_or_order_policy() {
    let expected = replay_oracle::assemble(
        replay_oracle::DESCRIPTOR,
        None,
        &replay_oracle::hex(include_str!("fixtures/replay_warning.hex")),
    );
    let mut engine = Engine::in_memory();
    for _ in 0..3 {
        engine
            .load_cxf(include_bytes!("fixtures/assertion_model.jsonld"))
            .unwrap();
        let record = ReplayRecord::from_bytes(&expected).unwrap();
        record
            .check_compatible(&CompatibilityDescriptor::current(None).unwrap())
            .unwrap();
        let entries: Vec<_> = record
            .inputs()
            .iter()
            .map(|(p, v)| (p.as_str(), v.clone()))
            .collect();
        let plan = engine.prepare_frame(record.time(), &entries).unwrap();
        let completed = engine.execute_frame(plan).unwrap();
        record.verify(&completed).unwrap();
        assert_eq!(completed.replay_record().unwrap().as_bytes(), expected);
    }
}

#[test]
fn stateful_inputs_replay_from_a_separate_start_sidecar_to_exact_end_bytes() {
    let mut source = Engine::in_memory();
    source
        .load_cxf(include_bytes!("fixtures/frame_delay.jsonld"))
        .unwrap();
    let entries = |v| {
        [
            ("urn:frame:a", Value::Real(0.0)),
            ("urn:frame:b", Value::Real(v)),
        ]
    };
    let plan = source.prepare_frame(0.0, &entries(2.0)).unwrap();
    source.execute_frame(plan).unwrap();
    let sidecar = source.state_snapshot().unwrap();
    let mut playback = Engine::in_memory();
    playback
        .load_cxf(include_bytes!("fixtures/frame_delay.jsonld"))
        .unwrap();
    playback
        .restore_state(&EngineStateSnapshot::from_bytes(sidecar.as_bytes()).unwrap())
        .unwrap();
    let descriptor = CompatibilityDescriptor::current(None).unwrap();
    for (time, input, output) in [(1.0, 3.0, 2.0), (1.0, 4.0, 2.0), (2.0, 5.0, 3.0)] {
        let plan = source.prepare_frame(time, &entries(input)).unwrap();
        let completed = source.execute_frame(plan).unwrap();
        assert!(completed.outputs()[0].1.bit_eq(&Value::Real(output)));
        let bytes = completed.replay_record().unwrap();
        let record = ReplayRecord::from_bytes(bytes.as_bytes()).unwrap();
        record.check_compatible(&descriptor).unwrap();
        let inputs: Vec<_> = record
            .inputs()
            .iter()
            .map(|(p, v)| (p.as_str(), v.clone()))
            .collect();
        let plan = playback.prepare_frame(record.time(), &inputs).unwrap();
        let actual = playback.execute_frame(plan).unwrap();
        record.verify(&actual).unwrap();
        assert_eq!(
            source.state_snapshot().unwrap().as_bytes(),
            playback.state_snapshot().unwrap().as_bytes()
        );
    }
}

#[test]
fn host_eligibility_refusals_preserve_fresh_and_advanced_state_store_and_restore_window() {
    let payload = replay_oracle::hex(include_str!("fixtures/replay_add.hex"));
    for advanced in [false, true] {
        let mut engine = Engine::with_store(Arc::new(RecordingStore::default()));
        engine.load_cxf(ADD).unwrap();
        if advanced {
            let frame = engine
                .prepare_frame(
                    -0.0,
                    &[
                        ("urn:legacy-frame:a", Value::Real(1.5)),
                        ("urn:legacy-frame:b", Value::Real(2.25)),
                    ],
                )
                .unwrap();
            engine.execute_frame(frame).unwrap();
        }
        engine.store().reset_calls();
        engine.store().arm_hot_path_guard();
        let before = engine.state_snapshot().unwrap();
        let descriptor = CompatibilityDescriptor::current(None).unwrap();
        for (text, target, cause) in [
            (
                replay_oracle::DESCRIPTOR.replace("io-schema:1", "io-schema:2"),
                None,
                ReplayError::DescriptorMismatch(oce_api::CompatibilityMismatch::IoSchema),
            ),
            (
                replay_oracle::DESCRIPTOR.into(),
                Some(("foreign", std::env::consts::OS)),
                ReplayError::TargetMismatch,
            ),
        ] {
            let bytes = replay_oracle::assemble(&text, target, &payload);
            for _ in 0..3 {
                let record = ReplayRecord::from_bytes(&bytes).unwrap();
                assert_eq!(record.check_compatible(&descriptor), Err(cause.clone()));
                assert_eq!(
                    engine.state_snapshot().unwrap().as_bytes(),
                    before.as_bytes()
                );
                assert_eq!(engine.store().calls(), StoreCallSnapshot::default());
            }
        }
        if advanced {
            assert!(matches!(
                engine.restore_state(&before),
                Err(oce_api::OcError::State(
                    oce_api::EngineStateError::DurableTargetAdvanced
                ))
            ));
        } else {
            engine.restore_state(&before).unwrap();
        }
    }
}
