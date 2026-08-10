//! Arithmetic-portability classification and target descriptor tests.

use std::sync::Arc;

use oce_model::{
    BlockId, BlockInstance, Connector, ConnectorId, Dir, ModelGraph, ParamTable, Value, ValueType,
};

use crate::state::Portability;
use crate::{Engine, EngineStateError, EngineStateSnapshot, OcError};

fn matrix_artifact_path(variable: &str) -> Option<std::path::PathBuf> {
    let path = std::path::PathBuf::from(std::env::var_os(variable)?);
    Some(if path.is_absolute() {
        path
    } else {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(path)
    })
}

fn write_matrix_artifact(variable: &str, bytes: &[u8]) {
    if let Some(path) = matrix_artifact_path(variable) {
        std::fs::write(path, bytes).unwrap();
    }
}

fn portable_model() -> ModelGraph {
    let mut model = ModelGraph::new();
    model.blocks.push(BlockInstance {
        id: BlockId(0),
        class_iri: Arc::from("CDL.Reals.IntegratorWithReset"),
        inputs: vec![ConnectorId(0), ConnectorId(1), ConnectorId(2)],
        outputs: vec![ConnectorId(3)],
        params: ParamTable::default(),
        decl_order: 0,
        instance_iri: Some(Arc::from("urn:test:portable-integrator")),
    });
    for (index, value_type) in [
        ValueType::Real,
        ValueType::Real,
        ValueType::Boolean,
        ValueType::Real,
    ]
    .into_iter()
    .enumerate()
    {
        let direction = if index < 3 { Dir::In } else { Dir::Out };
        model.connectors.push(
            Connector::new(
                ConnectorId(index as u32),
                BlockId(0),
                direction,
                value_type,
                index.min(3) as u32,
            )
            .with_iri(format!("urn:test:portable-integrator.c{index}")),
        );
    }
    model.external_inputs = vec![ConnectorId(0), ConnectorId(1), ConnectorId(2)];
    model
}

pub(super) fn target_bound_model() -> ModelGraph {
    let mut model = ModelGraph::new();
    model.blocks.push(BlockInstance {
        id: BlockId(0),
        class_iri: Arc::from("CDL.Utilities.SunRiseSet"),
        inputs: Vec::new(),
        outputs: vec![ConnectorId(0), ConnectorId(1), ConnectorId(2)],
        params: ParamTable {
            values: vec![
                (Arc::from("lat"), Value::Real(0.0)),
                (Arc::from("lon"), Value::Real(0.0)),
                (Arc::from("timZon"), Value::Real(0.0)),
            ],
        },
        decl_order: 0,
        instance_iri: Some(Arc::from("urn:test:sun")),
    });
    for (index, value_type) in [ValueType::Real, ValueType::Real, ValueType::Boolean]
        .into_iter()
        .enumerate()
    {
        model.connectors.push(
            Connector::new(
                ConnectorId(index as u32),
                BlockId(0),
                Dir::Out,
                value_type,
                index as u32,
            )
            .with_iri(format!("urn:test:sun.y{index}")),
        );
    }
    model
}

#[test]
fn target_bound_class_set_is_exactly_pinned() {
    assert_eq!(
        crate::state_manifest::TARGET_BOUND_CLASSES,
        [
            "CDL.Psychrometrics.DewPoint_TDryBulPhi",
            "CDL.Psychrometrics.SpecificEnthalpy_TDryBulPhi",
            "CDL.Psychrometrics.WetBulb_TDryBulPhi",
            "CDL.Reals.Acos",
            "CDL.Reals.Asin",
            "CDL.Reals.Atan",
            "CDL.Reals.Atan2",
            "CDL.Reals.Cos",
            "CDL.Reals.Exp",
            "CDL.Reals.Log",
            "CDL.Reals.Log10",
            "CDL.Reals.Sin",
            "CDL.Reals.Sources.Sin",
            "CDL.Reals.Tan",
            "CDL.Utilities.SunRiseSet",
        ]
    );
    for class_path in crate::state_manifest::TARGET_BOUND_CLASSES {
        assert!(crate::state_manifest::requires_target(class_path));
    }
    assert!(!crate::state_manifest::requires_target("CDL.Reals.Add"));
}

#[test]
fn target_bound_capture_carries_the_compile_target() {
    let mut engine = Engine::in_memory();
    engine
        .build_model_in_memory(target_bound_model(), Some("urn:test:sun-model"))
        .unwrap();
    engine.tick(0.0).unwrap();
    let snapshot = engine.state_snapshot().unwrap();
    assert!(matches!(
        &snapshot.image.manifest.portability,
        Portability::TargetBound { arch, os }
            if arch == std::env::consts::ARCH && os == std::env::consts::OS
    ));
    write_matrix_artifact("OCE_TARGET_STATE_OUT", snapshot.as_bytes());
}

#[test]
fn portable_engine_snapshot_continues_and_emits_matrix_artifact() {
    let model = portable_model();
    let mut source = Engine::in_memory();
    source
        .build_model_in_memory(model.clone(), Some("urn:test:portable-model"))
        .unwrap();
    source
        .set_input("urn:test:portable-integrator.c0", Value::Real(2.0))
        .unwrap();
    source
        .set_input("urn:test:portable-integrator.c1", Value::Real(7.0))
        .unwrap();
    source
        .set_input("urn:test:portable-integrator.c2", Value::Boolean(false))
        .unwrap();
    source.tick(0.0).unwrap();
    source.tick(0.25).unwrap();

    let snapshot = source.state_snapshot().unwrap();
    assert!(matches!(
        snapshot.image.manifest.portability,
        Portability::CrossPlatform
    ));
    let decoded = EngineStateSnapshot::from_bytes(snapshot.as_bytes()).unwrap();
    write_matrix_artifact("OCE_PORTABLE_STATE_OUT", snapshot.as_bytes());

    let mut restored = Engine::in_memory();
    restored
        .build_model_in_memory(model, Some("urn:test:portable-model"))
        .unwrap();
    restored.restore_state(&decoded).unwrap();
    source.tick(0.5).unwrap();
    restored.tick(0.5).unwrap();
    assert_eq!(restored.state.words, source.state.words);
    assert!(
        restored
            .state
            .values
            .iter()
            .zip(&source.state.values)
            .all(|(left, right)| left.bit_eq(right))
    );
}

#[test]
fn foreign_matrix_target_snapshot_refuses_restore_when_supplied() {
    let Some(path) = matrix_artifact_path("OCE_FOREIGN_TARGET_STATE_IN") else {
        return;
    };
    let bytes = std::fs::read(path).unwrap();
    let snapshot = EngineStateSnapshot::from_bytes(&bytes).unwrap();
    let mut target = Engine::in_memory();
    target
        .build_model_in_memory(target_bound_model(), Some("urn:test:sun-model"))
        .unwrap();
    assert!(matches!(
        target.restore_state(&snapshot),
        Err(OcError::State(
            EngineStateError::TargetDomainMismatch { .. }
        ))
    ));
}
