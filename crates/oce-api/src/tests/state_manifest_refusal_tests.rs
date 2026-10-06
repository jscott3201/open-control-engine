//! Rechecksummed manifest mutation corpus through parse + durable restore, not private-image bypass.
//! The oracle is the explicit compatibility grammar. No external state-codec oracle exists.

use std::sync::Arc;

use super::state_tests::CountingStore;
use crate::state::{BlockKey, Portability, StateImage, WireValue, WireValueType};
use crate::{Engine, EngineStateError, EngineStateSnapshot, OcError};

const MODEL: &[u8] = include_bytes!("../../../oce-cxf/tests/fixtures/minimal_loop.jsonld");

enum Refusal {
    Malformed(&'static str),
    Incompatible(&'static str),
}

type Mutation = (&'static str, fn(&mut StateImage), Refusal);

fn mutations() -> Vec<Mutation> {
    use Refusal::{Incompatible as I, Malformed as M};
    vec![
        (
            "profile/ABI",
            |i| i.execution_revision += 1,
            I("execution-state ABI revision"),
        ),
        (
            "portability",
            |i| {
                i.manifest.portability = Portability::TargetBound {
                    arch: "x".into(),
                    os: "y".into(),
                }
            },
            M("arithmetic-portability tag disagrees with manifest class set"),
        ),
        (
            "enum set",
            |i| {
                i.manifest.enums.push(crate::state::EnumManifestEntry {
                    class_path: "future".into(),
                    members: vec!["member".into()],
                })
            },
            M("enum descriptor set does not exactly match referenced classes"),
        ),
        (
            "block key",
            |i| i.manifest.blocks[0].key = BlockKey::Authored("different".into()),
            M("block input key disagrees with ordered port position"),
        ),
        (
            "class",
            |i| i.manifest.blocks[0].class_path.push_str(".Changed"),
            I("block http://example.org#MinLoop.gt class"),
        ),
        (
            "kind",
            |i| i.manifest.blocks[0].kind = oce_blocks::BlockKind::Stateful,
            M("has no state contract"),
        ),
        (
            "state revision",
            |i| {
                i.manifest
                    .blocks
                    .iter_mut()
                    .find(|b| b.state_revision != 0)
                    .unwrap()
                    .state_revision += 1
            },
            I("block http://example.org#MinLoop.del state revision"),
        ),
        (
            "state length",
            |i| {
                i.manifest
                    .blocks
                    .iter_mut()
                    .find(|b| b.state_len != 0)
                    .unwrap()
                    .state_len += 1
            },
            M("state-slot length differs from its block descriptor"),
        ),
        (
            "parameter name",
            |i| {
                i.manifest
                    .blocks
                    .iter_mut()
                    .find(|b| !b.params.is_empty())
                    .unwrap()
                    .params[0]
                    .0 = "changed".into()
            },
            I("parameter http://example.org#MinLoop.con name"),
        ),
        (
            "parameter value",
            |i| {
                i.manifest
                    .blocks
                    .iter_mut()
                    .find(|b| !b.params.is_empty())
                    .unwrap()
                    .params[0]
                    .1 = WireValue::Real(3.0f64.to_bits())
            },
            I("parameter http://example.org#MinLoop.con.k"),
        ),
        (
            "parameter count",
            |i| {
                i.manifest
                    .blocks
                    .iter_mut()
                    .find(|b| !b.params.is_empty())
                    .unwrap()
                    .params
                    .clear();
            },
            I("block http://example.org#MinLoop.con parameter count"),
        ),
        (
            "input binding",
            |i| i.manifest.blocks[0].inputs.swap(0, 1),
            M("block input key disagrees with ordered port position"),
        ),
        (
            "output binding",
            |i| i.manifest.blocks[0].outputs[0].port_index += 1,
            M("block output key disagrees with ordered port position"),
        ),
        (
            "connector key",
            |i| i.manifest.connectors[0].key.port_index = 99,
            M("connector keys are duplicate or out of order"),
        ),
        (
            "connector path",
            |i| i.manifest.connectors[0].path.push_str(".changed"),
            I("connector http://example.org#MinLoop.gt:In:0"),
        ),
        (
            "declaration order",
            |i| i.manifest.connectors[0].declaration_order += 1,
            I("connector http://example.org#MinLoop.gt:In:0"),
        ),
        (
            "connector type",
            |i| i.manifest.connectors[0].value_type = WireValueType::Boolean,
            M("connector value tag differs from its manifest type"),
        ),
        (
            "computation unit",
            |i| i.manifest.connectors[0].unit = Some("K".into()),
            I("connector http://example.org#MinLoop.gt:In:0"),
        ),
        (
            "quantity",
            |i| i.manifest.connectors[0].quantity = Some("Temperature".into()),
            I("connector http://example.org#MinLoop.gt:In:0"),
        ),
        (
            "connections",
            |i| {
                i.manifest.connections.pop();
            },
            I("connections"),
        ),
        (
            "block schedule",
            |i| i.manifest.schedule.swap(0, 1),
            I("block schedule"),
        ),
        (
            "connector schedule",
            |i| i.manifest.connector_order.swap(0, 1),
            I("connector schedule"),
        ),
        (
            "driver map",
            |i| i.manifest.driver_of[0].1 = i.manifest.driver_of[0].0.clone(),
            I("driver map"),
        ),
        (
            "slot offset",
            |i| i.manifest.state_slots[0].1 = 1,
            M("state-slot ranges are not contiguous"),
        ),
        (
            "external inputs",
            |i| {
                i.manifest.external_inputs.clear();
                i.manifest.input_definitions.clear();
            },
            I("external inputs"),
        ),
        (
            "boundary outputs",
            |i| i.manifest.boundary_outputs[0].0.push_str(".changed"),
            I("boundary outputs"),
        ),
        (
            "input definition path",
            |i| i.manifest.input_definitions[0].path.push_str(".changed"),
            M("input definition path or type differs from connectors"),
        ),
        (
            "input definition type",
            |i| i.manifest.input_definitions[0].value_type = WireValueType::Integer,
            M("input definition path or type differs from connectors"),
        ),
        (
            "input min",
            |i| i.manifest.input_definitions[0].min = Some(WireValue::Real(0)),
            I("input definition http://example.org#MinLoop.uSet"),
        ),
        (
            "input max",
            |i| i.manifest.input_definitions[0].max = Some(WireValue::Real(0)),
            I("input definition http://example.org#MinLoop.uSet"),
        ),
        (
            "input bound type",
            |i| i.manifest.input_definitions[0].min = Some(WireValue::Boolean(false)),
            M("input bound type differs from its domain"),
        ),
        (
            "input definition count",
            |i| i.manifest.input_definitions.clear(),
            M("input definitions differ from external input paths"),
        ),
        (
            "duplicate input definition",
            |i| {
                i.manifest
                    .input_definitions
                    .push(i.manifest.input_definitions[0].clone())
            },
            M("input definitions are duplicate or out of order"),
        ),
    ]
}

#[test]
fn every_manifest_field_refuses_deterministically_without_mutating_engine_or_store() {
    let store = Arc::new(CountingStore::default());
    let mut engine = Engine::with_store(Arc::clone(&store));
    engine.load_cxf(MODEL).unwrap();
    engine.halt().unwrap();
    let before = engine.state_snapshot().unwrap();
    let checkpoint = engine.checkpoint().unwrap();
    let checkpoint_bytes = crate::state_codec::encode_snapshot(&checkpoint.image, true).unwrap();
    let calls = store.calls();
    let frame_generation = Arc::clone(&engine.frame_generation);
    let model = Arc::clone(&engine.model);
    let inspection = format!(
        "{:?}{:?}{:?}{:?}",
        engine.io, engine.params, engine.schedule, engine.semantic_warnings
    );
    for (name, mutate, expected) in mutations() {
        let mut image = (*before.image).clone();
        mutate(&mut image);
        let manifest =
            crate::state_manifest_codec::encode_manifest(&image.manifest, false).unwrap();
        image.fingerprint = crate::state_manifest::fingerprint(image.execution_revision, &manifest);
        let bytes = crate::state_codec::encode_snapshot(&image, false).unwrap();
        let mut prior_error = None;
        for _ in 0..2 {
            let error = match EngineStateSnapshot::from_bytes(&bytes) {
                Err(error) => {
                    assert!(
                        matches!((&expected, &error), (Refusal::Malformed(want), EngineStateError::MalformedSnapshot { detail, .. }) if detail.contains(want)),
                        "{name}: {error:?}"
                    );
                    error
                }
                Ok(snapshot) => {
                    let error = engine.restore_state(&snapshot).expect_err(name);
                    let OcError::State(error) = error else {
                        panic!("{name}: {error:?}")
                    };
                    assert!(
                        matches!((&expected, &error), (Refusal::Incompatible(want), EngineStateError::IncompatibleExecution { subject, .. }) if subject == want),
                        "{name}: {error:?}"
                    );
                    error
                }
            };
            let error = format!("{error:?}");
            if let Some(prior) = &prior_error {
                assert_eq!(&error, prior, "{name}");
            }
            prior_error = Some(error);
            assert_eq!(
                engine.state_snapshot().unwrap().as_bytes(),
                before.as_bytes(),
                "{name}"
            );
            assert_eq!(
                crate::state_codec::encode_snapshot(&engine.checkpoint().unwrap().image, true)
                    .unwrap(),
                checkpoint_bytes,
                "{name}"
            );
            assert!(engine.durable_restore_ready, "{name}");
            assert_eq!(engine.accepted_frame_sequence, 0);
            assert_eq!(engine.mode(), crate::RunMode::Halted);
            assert!(Arc::ptr_eq(&engine.frame_generation, &frame_generation));
            assert!(Arc::ptr_eq(&engine.model, &model));
            assert_eq!(
                format!(
                    "{:?}{:?}{:?}{:?}",
                    engine.io, engine.params, engine.schedule, engine.semantic_warnings
                ),
                inspection
            );
            assert_eq!(store.calls(), calls, "{name}");
        }
    }
    engine.restore_state(&before).unwrap();
    engine.restore_checkpoint(&checkpoint).unwrap();
    assert_eq!(store.calls(), calls);
}

#[test]
fn capture_cannot_return_a_snapshot_rejected_by_the_canonical_decoder() {
    let mut engine = Engine::in_memory();
    engine.load_cxf(MODEL).unwrap();
    // Inject a future producer defect: the existing encoder permits duplicate edges, but the
    // canonical decoder does not. Public ingest need not be able to manufacture this defect.
    let model = Arc::make_mut(&mut engine.model);
    model.connections.push(model.connections[0]);
    assert!(matches!(engine.state_snapshot(), Err(OcError::State(
        EngineStateError::MalformedSnapshot { detail, .. }
    )) if detail == "connections are duplicate or out of order"));
    assert!(engine.durable_restore_ready);
}
