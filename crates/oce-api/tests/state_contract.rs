//! Host-visible state compatibility. Expectations are derived from the authored IO contract,
//! not an external numerical oracle; runtime continuation has separate recurrence tests.

use oce_api::{Engine, EngineStateError, EngineStateSnapshot, OcError, StatePortability, Value};

const MODEL: &[u8] = include_bytes!("fixtures/legacy_frame_add.jsonld");

#[test]
fn historical_revision_one_vector_refuses_without_a_migration_fallback() {
    // Unchanged empty-authored-model golden from the preceding wire contract, not re-blessed.
    let hex = include_str!("fixtures/state_legacy.hex").trim();
    let bytes: Vec<_> = (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap())
        .collect();
    assert!(matches!(
        EngineStateSnapshot::from_bytes(&bytes),
        Err(EngineStateError::UnsupportedFormat { revision: 1 })
    ));
}

#[test]
fn portable_policy_is_public_owned_inspection_not_restore_authority() {
    let mut engine = Engine::in_memory();
    engine.load_cxf(MODEL).unwrap();
    let snapshot = engine.state_snapshot().unwrap();
    let policy = snapshot.portability().clone();
    assert_eq!(policy, StatePortability::Portable);
    let parsed = EngineStateSnapshot::from_bytes(snapshot.as_bytes()).unwrap();
    assert_eq!(parsed.portability(), &policy);
    drop(engine);
    drop(snapshot);
    assert_eq!(parsed.portability(), &StatePortability::Portable);
    let mut unloaded = Engine::in_memory();
    assert!(matches!(
        unloaded.restore_state(&parsed),
        Err(OcError::State(EngineStateError::NoLoadedModel))
    ));
}

fn changed_node(path: &str, field: &str, value: serde_json::Value) -> Vec<u8> {
    let mut document: serde_json::Value = serde_json::from_slice(MODEL).unwrap();
    let node = document["@graph"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|node| node["@id"] == path)
        .unwrap();
    node[field] = value;
    serde_json::to_vec(&document).unwrap()
}

#[test]
fn changed_input_acceptance_domain_refuses_before_mutation() {
    let mut source = Engine::in_memory();
    source.load_cxf(MODEL).unwrap();
    let snapshot = source.state_snapshot().unwrap();
    for field in ["S231:min", "S231:max"] {
        let document = changed_node("urn:legacy-frame:a", field, 1.into());
        let mut target = Engine::in_memory();
        target.load_cxf(&document).unwrap();
        let before = target.state_snapshot().unwrap();
        assert!(matches!(
            target.restore_state(&snapshot),
            Err(OcError::State(EngineStateError::IncompatibleExecution { subject, .. }))
                if subject == "input definition urn:legacy-frame:a"
        ));
        assert_eq!(
            target.state_snapshot().unwrap().as_bytes(),
            before.as_bytes()
        );
        target.restore_state(&before).unwrap();
    }
}

#[test]
fn changed_computation_units_and_quantities_refuse_before_mutation() {
    let mut source = Engine::in_memory();
    source.load_cxf(MODEL).unwrap();
    let snapshot = source.state_snapshot().unwrap();
    for (field, value) in [
        ("S231:unit", "K"),
        ("S231:quantity", "ThermodynamicTemperature"),
    ] {
        let document = changed_node("urn:legacy-frame:y", field, value.into());
        let mut target = Engine::in_memory();
        target.load_cxf(&document).unwrap();
        let before = target.state_snapshot().unwrap();
        assert!(matches!(
            target.restore_state(&snapshot),
            Err(OcError::State(EngineStateError::IncompatibleExecution { subject, .. }))
                if subject.starts_with("connector ")
        ));
        assert_eq!(
            target.state_snapshot().unwrap().as_bytes(),
            before.as_bytes()
        );
        target.restore_state(&before).unwrap();
    }
}

#[test]
fn parsed_snapshot_continuation_preserves_signed_zero_and_retained_results() {
    let mut source = Engine::in_memory();
    source
        .load_cxf(include_bytes!("fixtures/frame_pre.jsonld"))
        .unwrap();
    assert!(source.input_definitions().unwrap().is_empty());
    let prepared = source.prepare_frame(-0.0, &[]).unwrap();
    let retained = source.execute_frame(prepared).unwrap();
    let snapshot = source.state_snapshot().unwrap();
    let parsed = EngineStateSnapshot::from_bytes(snapshot.as_bytes()).unwrap();
    let mut target = Engine::in_memory();
    target
        .load_cxf(include_bytes!("fixtures/frame_pre.jsonld"))
        .unwrap();
    target.restore_state(&parsed).unwrap();
    assert_eq!(
        target.state_snapshot().unwrap().as_bytes(),
        snapshot.as_bytes()
    );
    for (time, expected) in [(-0.0, true), (0.0, false), (1.0, true), (1.0, false)] {
        let left = source
            .execute_frame(source.prepare_frame(time, &[]).unwrap())
            .unwrap();
        let right = target
            .execute_frame(target.prepare_frame(time, &[]).unwrap())
            .unwrap();
        // Hand recurrence: Pre starts false and latches Not(Pre) exactly once per call.
        assert!(
            left.outputs()
                .iter()
                .all(|(_, value)| value.bit_eq(&Value::Boolean(expected)))
        );
        assert_eq!(left.time().to_bits(), right.time().to_bits());
        assert_eq!(left.outputs().len(), right.outputs().len());
        assert!(
            left.outputs()
                .iter()
                .zip(right.outputs())
                .all(|((a, x), (b, y))| a == b && x.bit_eq(y))
        );
        assert_eq!(
            source.state_snapshot().unwrap().as_bytes(),
            target.state_snapshot().unwrap().as_bytes()
        );
    }
    assert_eq!(retained.time().to_bits(), (-0.0f64).to_bits());
    assert_eq!(retained.sequence(), 1);
    assert!(
        retained
            .outputs()
            .iter()
            .all(|(_, value)| value.bit_eq(&Value::Boolean(false)))
    );
}
