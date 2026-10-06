//! Host-policy fixture, not an OCE build token, authenticator, rollback API or field qualification.
//! Identities stand for externally authenticated full build/deployment qualifiers, NOT Cargo versions.

mod support;

use oce_api::{
    CompatibilityDescriptor, Engine, EngineStateError, EngineStateSnapshot, OcError, ReplayRecord,
    Value,
};
use std::sync::Arc;
use support::recording_store::{RecordingStore, StoreCallSnapshot};

const CURRENT: &str = "e81480b02271456719d55cbe1e5090b0dea6d63c";
const HISTORICAL: &str = "909a8ba699e6a2fccf3de6ac0616a9e83a04060f";
const MODEL: &[u8] = include_bytes!("fixtures/frame_pre.jsonld");

#[derive(Debug, PartialEq, Eq)]
enum HostRefusal {
    MissingEnvelope,
    Unauthenticated,
    CrossCandidate,
    UnqualifiedCandidate,
}

#[derive(Clone, Copy)]
struct Envelope<'a> {
    producer: &'a str,
    authenticated: bool,
    bytes: &'a [u8],
}

// The closure is the decoder/engine boundary. Real hosts authenticate exact bytes, complete build
// qualifiers, generation and freshness BEFORE producing this policy decision. No crypto is modeled.
fn admit<T>(
    envelope: Option<Envelope<'_>>,
    consumer: &str,
    consume: impl FnOnce(&[u8]) -> T,
) -> Result<T, HostRefusal> {
    let envelope = envelope.ok_or(HostRefusal::MissingEnvelope)?;
    if !envelope.authenticated {
        return Err(HostRefusal::Unauthenticated);
    }
    if ![CURRENT, HISTORICAL].contains(&consumer)
        || ![CURRENT, HISTORICAL].contains(&envelope.producer)
    {
        return Err(HostRefusal::UnqualifiedCandidate);
    }
    if envelope.producer != consumer {
        return Err(HostRefusal::CrossCandidate);
    }
    if consumer != CURRENT {
        return Err(HostRefusal::UnqualifiedCandidate);
    }
    Ok(consume(envelope.bytes))
}

fn engine() -> Engine<RecordingStore> {
    let mut engine = Engine::with_store(Arc::new(RecordingStore::default()));
    engine.load_cxf(MODEL).unwrap();
    engine.store().reset_calls();
    engine.store().arm_hot_path_guard();
    engine
}

#[test]
fn cross_candidate_envelopes_refuse_before_decode_or_engine_mutation() {
    let mut source = engine();
    let completed = source
        .execute_frame(source.prepare_frame(-0.0, &[]).unwrap())
        .unwrap();
    let snapshot = source.state_snapshot().unwrap();
    let record = completed.replay_record().unwrap();
    // Valid current bytes mislabeled as historical are deliberate hostile envelopes, NOT artifacts
    // produced by v0.1.0 (which has neither API). OCE could otherwise accept these same-version bytes.
    for advanced in [false, true] {
        let mut target = engine();
        if advanced {
            target
                .execute_frame(target.prepare_frame(-0.0, &[]).unwrap())
                .unwrap();
        }
        let before = target.state_snapshot().unwrap();
        for bytes in [snapshot.as_bytes(), record.as_bytes(), b"malformed"] {
            for (producer, consumer, cause) in [
                (HISTORICAL, CURRENT, HostRefusal::CrossCandidate),
                (CURRENT, HISTORICAL, HostRefusal::CrossCandidate),
                (HISTORICAL, HISTORICAL, HostRefusal::UnqualifiedCandidate),
                ("unknown", CURRENT, HostRefusal::UnqualifiedCandidate),
            ] {
                let mut decodes = 0;
                let result = admit(
                    Some(Envelope {
                        producer,
                        authenticated: true,
                        bytes,
                    }),
                    consumer,
                    |_| {
                        decodes += 1;
                    },
                );
                assert_eq!(result, Err(cause));
                assert_eq!(decodes, 0, "refusal precedes even malformed-byte decoding");
                assert_eq!(
                    target.state_snapshot().unwrap().as_bytes(),
                    before.as_bytes()
                );
                assert_eq!(target.store().calls(), StoreCallSnapshot::default());
            }
        }
        if advanced {
            assert!(matches!(
                target.restore_state(&before),
                Err(OcError::State(EngineStateError::DurableTargetAdvanced))
            ));
        } else {
            target.restore_state(&before).unwrap();
        }
    }
}

#[test]
fn approved_same_candidate_continuation_differs_from_fresh_cold_requalification() {
    let mut source = engine();
    let first = source
        .execute_frame(source.prepare_frame(-0.0, &[]).unwrap())
        .unwrap();
    let snapshot = source.state_snapshot().unwrap();
    let expected = source
        .execute_frame(source.prepare_frame(-0.0, &[]).unwrap())
        .unwrap();
    let record = expected.replay_record().unwrap();
    let mut target = engine();
    admit(
        Some(Envelope {
            producer: CURRENT,
            authenticated: true,
            bytes: snapshot.as_bytes(),
        }),
        CURRENT,
        |bytes| {
            target
                .restore_state(&EngineStateSnapshot::from_bytes(bytes).unwrap())
                .unwrap();
        },
    )
    .unwrap();
    admit(
        Some(Envelope {
            producer: CURRENT,
            authenticated: true,
            bytes: record.as_bytes(),
        }),
        CURRENT,
        |bytes| {
            let replay = ReplayRecord::from_bytes(bytes).unwrap();
            replay
                .check_compatible(&CompatibilityDescriptor::current(None).unwrap())
                .unwrap();
            let plan = target.prepare_frame(replay.time(), &[]).unwrap();
            replay.verify(&target.execute_frame(plan).unwrap()).unwrap();
        },
    )
    .unwrap();
    assert_eq!(
        source.state_snapshot().unwrap().as_bytes(),
        target.state_snapshot().unwrap().as_bytes()
    );
    assert_eq!(target.store().calls(), StoreCallSnapshot::default());

    // Host explicitly chooses cold load and isolated requalification, never old state or a rewritten
    // header. This is a fresh Engine, not automatic recovery on the advanced target.
    let mut cold = engine();
    let cold_first = cold
        .execute_frame(cold.prepare_frame(-0.0, &[]).unwrap())
        .unwrap();
    assert_eq!(cold_first.sequence(), 1);
    assert_eq!(cold_first.outputs().len(), 2);
    for index in 0..2 {
        // Independent Pre/Not recurrence: fresh output false, continued second output true.
        assert!(cold_first.outputs()[index].1.bit_eq(&Value::Boolean(false)));
        assert!(first.outputs()[index].1.bit_eq(&Value::Boolean(false)));
        assert!(expected.outputs()[index].1.bit_eq(&Value::Boolean(true)));
    }
    assert_eq!(
        cold.state_snapshot().unwrap().as_bytes(),
        snapshot.as_bytes()
    );
    assert_ne!(
        cold.state_snapshot().unwrap().as_bytes(),
        target.state_snapshot().unwrap().as_bytes()
    );
    assert_eq!(cold.store().calls(), StoreCallSnapshot::default());
}

#[test]
fn absent_or_unauthenticated_envelopes_never_reach_the_decoder() {
    assert_eq!(
        admit(None, CURRENT, |_| panic!("must not decode")),
        Err::<(), _>(HostRefusal::MissingEnvelope)
    );
    let envelope = Envelope {
        producer: CURRENT,
        authenticated: false,
        bytes: b"untrusted",
    };
    assert_eq!(
        admit(Some(envelope), CURRENT, |_| panic!("must not decode")),
        Err::<(), _>(HostRefusal::Unauthenticated)
    );
}

#[test]
fn rollback_handoff_retains_prior_qualified_identity_and_opaque_state_outside_oce() {
    // A prior qualified binary is an external host fixture, NOT a qualification of v0.1.0.
    // Its bytes deliberately are not OCE snapshot/replay bytes. The host dispatches only to that
    // prior binary; no current Engine decoder, state transplant or rollback method is called.
    let prior = Envelope {
        producer: "prior-qualified-host-build",
        authenticated: true,
        bytes: b"prior-build-own-authenticated-state",
    };
    assert_eq!(
        admit(Some(prior), CURRENT, |_| panic!(
            "current must not consume prior state"
        )),
        Err::<(), _>(HostRefusal::UnqualifiedCandidate)
    );
    let mut external_handoffs = Vec::new();
    let handoff =
        |qualified_binary: &str, envelope: Envelope<'_>, sink: &mut Vec<(String, Vec<u8>)>| {
            if !envelope.authenticated {
                return Err(HostRefusal::Unauthenticated);
            }
            if qualified_binary != envelope.producer {
                return Err(HostRefusal::CrossCandidate);
            }
            sink.push((qualified_binary.to_owned(), envelope.bytes.to_vec()));
            Ok(())
        };
    assert_eq!(
        handoff(CURRENT, prior, &mut external_handoffs),
        Err(HostRefusal::CrossCandidate)
    );
    assert!(external_handoffs.is_empty());
    handoff(prior.producer, prior, &mut external_handoffs).unwrap();
    assert_eq!(
        external_handoffs,
        vec![(prior.producer.to_owned(), prior.bytes.to_vec())]
    );
}
