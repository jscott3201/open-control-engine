//! Named execution-manifest compatibility matrix.

use std::sync::Arc;

use super::common::*;
use crate::state::{Portability, WireValueType};
use crate::{EngineCheckpoint, EngineStateError, EngineStateSnapshot};

fn checkpoint_and_target() -> (EngineCheckpoint, Engine) {
    let mut source = Engine::in_memory();
    source
        .build_model_in_memory(build_accumulator_model().0, None)
        .unwrap();
    let checkpoint = source.checkpoint().unwrap();
    let mut target = Engine::in_memory();
    target
        .build_model_in_memory(build_accumulator_model().0, None)
        .unwrap();
    (checkpoint, target)
}

fn assert_incompatible(mutate: impl FnOnce(&mut crate::state::StateImage), expected_subject: &str) {
    let (checkpoint, mut target) = checkpoint_and_target();
    let mut image = (*checkpoint.image).clone();
    mutate(&mut image);
    let error = match target.restore_checkpoint(&EngineCheckpoint {
        image: Arc::new(image),
    }) {
        Err(error) => error,
        Ok(()) => panic!("accepted {expected_subject} mutation"),
    };
    assert!(matches!(
        error,
        OcError::State(EngineStateError::IncompatibleExecution { subject, .. })
            if subject.contains(expected_subject)
    ));
}

#[test]
fn executable_manifest_differences_report_the_first_named_subject() {
    assert_incompatible(
        |image| image.execution_revision += 1,
        "execution-state ABI revision",
    );
    assert_incompatible(
        |image| image.manifest.blocks[0].class_path.push_str(".Changed"),
        "class",
    );
    assert_incompatible(
        |image| {
            image.manifest.blocks[0].params[0].1 = crate::state::WireValue::Real(2.0f64.to_bits())
        },
        "parameter",
    );
    assert_incompatible(
        |image| {
            let block = image
                .manifest
                .blocks
                .iter_mut()
                .find(|block| block.state_revision != 0)
                .unwrap();
            block.state_revision += 1;
        },
        "state revision",
    );
    assert_incompatible(
        |image| {
            let block = image
                .manifest
                .blocks
                .iter_mut()
                .find(|block| block.state_len != 0)
                .unwrap();
            block.state_len += 1;
        },
        "state length",
    );
    assert_incompatible(
        |image| image.manifest.blocks[1].inputs.swap(0, 1),
        "input binding",
    );
    assert_incompatible(
        |image| image.manifest.connectors[0].value_type = WireValueType::Boolean,
        "connector",
    );
    assert_incompatible(
        |image| {
            image.manifest.connections.pop();
        },
        "connections",
    );
    assert_incompatible(|image| image.manifest.schedule.swap(0, 1), "block schedule");
    assert_incompatible(
        |image| {
            let current = image.manifest.driver_of[0].1.clone();
            let replacement = image
                .manifest
                .connectors
                .iter()
                .map(|entry| entry.key.clone())
                .find(|key| key != &current)
                .unwrap();
            image.manifest.driver_of[0].1 = replacement;
        },
        "driver map",
    );
    assert_incompatible(
        |image| {
            image
                .manifest
                .external_inputs
                .push(image.manifest.connectors[0].key.clone());
        },
        "external inputs",
    );
    assert_incompatible(
        |image| {
            image.manifest.boundary_outputs.push((
                "urn:test:changed-boundary".into(),
                image.manifest.connectors[0].key.clone(),
            ));
        },
        "boundary outputs",
    );
}

#[test]
fn enum_member_identity_change_is_incompatible() {
    let mut builder = Mb::new();
    let (_, inputs, _) = builder.block(
        "CDL.Reals.PID",
        &[ValueType::Real, ValueType::Real],
        &[ValueType::Real],
        vec![(
            Arc::from("controllerType"),
            Value::Enum {
                class: oce_model::EnumClassId::SIMPLE_CONTROLLER,
                ordinal: 2,
            },
        )],
    );
    let mut model = builder.finish();
    model.blocks[0].instance_iri = Some(Arc::from("urn:test:enum-block"));
    for (index, connector) in model.connectors.iter_mut().enumerate() {
        connector.iri = Some(Arc::from(format!("urn:test:enum-connector:{index}")));
    }
    model.external_inputs = inputs;
    let mut source = Engine::in_memory();
    source
        .build_model_in_memory(model.clone(), Some("urn:test:enum-model"))
        .unwrap();
    let snapshot = source.state_snapshot().unwrap();
    let mut image = (*snapshot.image).clone();
    image.manifest.enums[0].members.swap(0, 1);
    let manifest = crate::state_manifest_codec::encode_manifest(&image.manifest, false).unwrap();
    image.fingerprint = crate::state_manifest::fingerprint(image.execution_revision, &manifest);
    let bytes = crate::state_codec::encode_snapshot(&image, false).unwrap();
    let snapshot = EngineStateSnapshot::from_bytes(&bytes).unwrap();
    let mut target = Engine::in_memory();
    target
        .build_model_in_memory(model, Some("urn:test:enum-model"))
        .unwrap();
    assert!(matches!(
        target.restore_state(&snapshot),
        Err(OcError::State(EngineStateError::IncompatibleExecution { subject, .. }))
            if subject.contains("enum class")
    ));
}

#[test]
fn capture_rejects_an_enum_parameter_outside_its_canonical_descriptor() {
    let mut builder = Mb::new();
    let (_, inputs, _) = builder.block(
        "CDL.Reals.PID",
        &[ValueType::Real, ValueType::Real],
        &[ValueType::Real],
        vec![(
            Arc::from("controllerType"),
            Value::Enum {
                class: oce_model::EnumClassId::SIMPLE_CONTROLLER,
                ordinal: 2,
            },
        )],
    );
    let mut model = builder.finish();
    model.external_inputs = inputs;
    let mut engine = Engine::in_memory();
    engine.build_model_in_memory(model, None).unwrap();
    let Value::Enum { ordinal, .. } =
        &mut Arc::make_mut(&mut engine.model).blocks[0].params.values[0].1
    else {
        panic!("controllerType is an enum")
    };
    *ordinal = 99;

    assert!(matches!(
        engine.checkpoint(),
        Err(OcError::State(EngineStateError::IneligibleModel { .. }))
    ));
}

#[test]
fn foreign_target_domain_refuses_before_commit() {
    const MINIMAL_LOOP: &[u8] =
        include_bytes!("../../../oce-cxf/tests/fixtures/minimal_loop.jsonld");
    let mut source = Engine::in_memory();
    source.load_cxf(MINIMAL_LOOP).unwrap();
    let snapshot = source.state_snapshot().unwrap();
    let mut image = (*snapshot.image).clone();
    image.execution_revision += 1;
    image.manifest.portability = Portability::TargetBound {
        arch: "foreign-arch".into(),
        os: "foreign-os".into(),
    };
    let foreign = EngineStateSnapshot {
        bytes: snapshot.bytes,
        image: Arc::new(image),
    };
    let mut target = Engine::in_memory();
    target.load_cxf(MINIMAL_LOOP).unwrap();
    assert!(matches!(
        target.restore_state(&foreign),
        Err(OcError::State(
            EngineStateError::TargetDomainMismatch { .. }
        ))
    ));
}
