//! Resource-bound and bounded-debug tests for engine state images.

use std::sync::Arc;

use oce_model::{
    BlockId, BlockInstance, Connector, ConnectorId, Dir, ModelGraph, ParamTable, Value, ValueType,
};

use super::state_portability_tests::target_bound_model;
use crate::{Engine, EngineStateError, EngineStateSnapshot, OcError};

fn assert_model(message: Arc<str>) -> ModelGraph {
    let mut model = ModelGraph::new();
    model.blocks.push(BlockInstance {
        id: BlockId(0),
        class_iri: Arc::from("CDL.Utilities.Assert"),
        inputs: vec![ConnectorId(0)],
        outputs: Vec::new(),
        params: ParamTable {
            values: vec![(Arc::from("message"), Value::String(message))],
        },
        decl_order: 0,
        instance_iri: Some(Arc::from("urn:test:assert")),
    });
    model.connectors.push(
        Connector::new(ConnectorId(0), BlockId(0), Dir::In, ValueType::Boolean, 0)
            .with_iri("urn:test:assert.u"),
    );
    model.external_inputs.push(ConnectorId(0));
    model
}

fn snapshot_with_message(message: Arc<str>) -> Result<crate::EngineStateSnapshot, OcError> {
    let mut engine = Engine::in_memory();
    engine
        .build_model_in_memory(assert_model(message), Some("urn:test:resource-model"))
        .unwrap();
    engine.state_snapshot()
}

#[test]
fn exact_snapshot_cap_round_trips_and_one_byte_over_refuses_capture() {
    let baseline = snapshot_with_message(Arc::from("")).unwrap();
    let payload_bytes = crate::state::MAX_SNAPSHOT_BYTES as usize - baseline.as_bytes().len();
    drop(baseline);

    let exact = snapshot_with_message(Arc::from("x".repeat(payload_bytes))).unwrap();
    assert_eq!(
        exact.as_bytes().len(),
        crate::state::MAX_SNAPSHOT_BYTES as usize
    );
    EngineStateSnapshot::from_bytes(exact.as_bytes()).unwrap();
    drop(exact);

    assert!(matches!(
        snapshot_with_message(Arc::from("x".repeat(payload_bytes + 1))),
        Err(OcError::State(EngineStateError::SnapshotTooLarge {
            actual_bytes,
            max_bytes
        })) if actual_bytes == max_bytes + 1
    ));
}

#[test]
fn opaque_image_debug_output_is_bounded() {
    let model_id = format!("urn:test:{}", "x".repeat(32 * 1024));
    let mut engine = Engine::in_memory();
    engine
        .build_model_in_memory(ModelGraph::new(), Some(&model_id))
        .unwrap();
    let checkpoint = format!("{:?}", engine.checkpoint().unwrap());
    let snapshot = format!("{:?}", engine.state_snapshot().unwrap());
    assert!(checkpoint.len() < 256, "{checkpoint}");
    assert!(snapshot.len() < 256, "{snapshot}");
    assert!(!checkpoint.contains(&model_id));
    assert!(!snapshot.contains(&model_id));
}

#[test]
fn checkpoint_size_counter_matches_the_wire_encoder() {
    let mut engine = Engine::in_memory();
    engine
        .build_model_in_memory(assert_model(Arc::from("sized")), None)
        .unwrap();
    let checkpoint = engine.checkpoint().unwrap();
    let counted = crate::state_codec::encoded_snapshot_len(&checkpoint.image, true).unwrap();
    let encoded = crate::state_codec::encode_snapshot(&checkpoint.image, true).unwrap();
    assert_eq!(counted, encoded.len());
}

#[test]
fn compact_many_block_snapshot_round_trips_within_the_decode_budget() {
    let mut model = ModelGraph::new();
    for index in 0..1_000u32 {
        let block = BlockId(index);
        let output = ConnectorId(index);
        model.blocks.push(BlockInstance {
            id: block,
            class_iri: Arc::from("CDL.Reals.Sources.CivilTime"),
            inputs: Vec::new(),
            outputs: vec![output],
            params: ParamTable::default(),
            decl_order: index,
            instance_iri: Some(Arc::from(format!("u:{index:03}"))),
        });
        model.connectors.push(
            Connector::new(output, block, Dir::Out, ValueType::Real, 0)
                .with_iri(format!("v:{index:03}")),
        );
    }
    let mut engine = Engine::in_memory();
    engine
        .build_model_in_memory(model, Some("urn:test:compact-manifest"))
        .unwrap();
    let snapshot = engine.state_snapshot().unwrap();
    EngineStateSnapshot::from_bytes(snapshot.as_bytes()).unwrap();
}

#[test]
fn compatibility_diagnostics_bound_large_parameter_values() {
    let mut source = Engine::in_memory();
    source
        .build_model_in_memory(assert_model(Arc::from("x".repeat(1024 * 1024))), None)
        .unwrap();
    let checkpoint = source.checkpoint().unwrap();

    let mut target = Engine::in_memory();
    target
        .build_model_in_memory(assert_model(Arc::from("y".repeat(1024 * 1024))), None)
        .unwrap();
    let error = target.restore_checkpoint(&checkpoint).unwrap_err();
    let OcError::State(EngineStateError::IncompatibleExecution {
        subject,
        snapshot,
        target,
    }) = error
    else {
        panic!("parameter mismatch returned the wrong error: {error:?}")
    };
    assert!(subject.len() <= 259, "{subject}");
    assert!(snapshot.len() <= 259);
    assert!(target.len() <= 259);
}

#[test]
fn target_domain_diagnostics_bound_hostile_descriptor_strings() {
    let mut source = Engine::in_memory();
    source
        .build_model_in_memory(target_bound_model(), Some("urn:test:sun-model"))
        .unwrap();
    let mut image = (*source.state_snapshot().unwrap().image).clone();
    let crate::state::Portability::TargetBound { arch, os } = &mut image.manifest.portability
    else {
        panic!("SunRiseSet snapshot is target-bound")
    };
    *arch = "x".repeat(1024 * 1024);
    *os = "foreign-os".into();
    let manifest = crate::state_manifest_codec::encode_manifest(&image.manifest, false).unwrap();
    image.fingerprint = crate::state_manifest::fingerprint(image.execution_revision, &manifest);
    let bytes = crate::state_codec::encode_snapshot(&image, false).unwrap();
    let snapshot = EngineStateSnapshot::from_bytes(&bytes).unwrap();

    let mut target = Engine::in_memory();
    target
        .build_model_in_memory(target_bound_model(), Some("urn:test:sun-model"))
        .unwrap();
    let OcError::State(EngineStateError::TargetDomainMismatch {
        snapshot_arch,
        snapshot_os,
        target_arch,
        target_os,
    }) = target.restore_state(&snapshot).unwrap_err()
    else {
        panic!("foreign target returned the wrong error")
    };
    for field in [snapshot_arch, snapshot_os, target_arch, target_os] {
        assert!(field.len() <= 259);
    }
}

#[test]
fn malformed_diagnostics_bound_hostile_enum_class_paths() {
    let mut source = Engine::in_memory();
    source
        .build_model_in_memory(target_bound_model(), Some("urn:test:sun-model"))
        .unwrap();
    let mut image = (*source.state_snapshot().unwrap().image).clone();
    let class_path = "x".repeat(1024 * 1024);
    image.manifest.enums.push(crate::state::EnumManifestEntry {
        class_path: class_path.clone(),
        members: vec![Arc::from("member")],
    });
    image.manifest.connectors[0].value_type = crate::state::WireValueType::Enum(class_path);
    let manifest = crate::state_manifest_codec::encode_manifest(&image.manifest, false).unwrap();
    image.fingerprint = crate::state_manifest::fingerprint(image.execution_revision, &manifest);
    let bytes = crate::state_codec::encode_snapshot(&image, false).unwrap();

    let EngineStateError::MalformedSnapshot { detail, .. } =
        EngineStateSnapshot::from_bytes(&bytes).unwrap_err()
    else {
        panic!("unknown enum descriptor returned the wrong error")
    };
    assert!(detail.len() <= 320, "{}", detail.len());
}
