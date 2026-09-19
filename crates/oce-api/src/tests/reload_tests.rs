//! Ordinary reload refusal preserves the complete owned run image, not external Store effects.
//! No independent transaction oracle exists: comparisons are same-engine before/after controls.
//! Private caches use transient Debug comparisons, never checked-in layout/debug goldens.

use super::*;

const MODEL: &[u8] = include_bytes!("../../../oce-cxf/tests/fixtures/minimal_loop.jsonld");
const LOOP: &[u8] = include_bytes!("fixtures/analog_warning_algebraic_loop.jsonld");
const UNITS: &[u8] = include_bytes!("../../../oce-cxf/tests/fixtures/invalid/unit_mismatch.jsonld");
const DISPLAY: &str =
    include_str!("../../../oce-cxf/tests/fixtures/invalid/display_unit_divergence.jsonld");

fn bits(value: &Value) -> String {
    match value {
        Value::Real(value) => format!("Real:{:016x}", value.to_bits()),
        value => format!("{value:?}"),
    }
}

// Exhaustive destructuring makes new Engine fields require an explicit preservation decision.
// Model identity plus its full content, block allocation identity, all caches and bitwise
// numeric images are compared. This does not inspect trait-object implementation layouts.
fn image<S: Store>(engine: &Engine<S>) -> Vec<String> {
    let Engine {
        store,
        cxf_byte_limit,
        model,
        schedule,
        blocks,
        state,
        outputs,
        prev_t,
        store_inputs,
        model_id,
        semantic_warnings,
        params,
        mode,
        params_dirty,
        io,
        durable_batch,
        realtime_epoch_unix_nanos,
        loaded,
        durable_restore_ready,
    } = engine;
    vec![
        format!("{:p} {cxf_byte_limit}", Arc::as_ptr(store)),
        format!("{:p} {model:?} {model_id:?}", Arc::as_ptr(model)),
        format!("{schedule:?}"),
        format!(
            "{:?}",
            blocks
                .iter()
                .map(|block| &**block as *const dyn Block)
                .collect::<Vec<_>>()
        ),
        format!("{:?}", state.values.iter().map(bits).collect::<Vec<_>>()),
        format!("{:?} {:?} {:?}", state.words, state.slots, state.slot_of),
        format!("{:?}", state.scratch.iter().map(bits).collect::<Vec<_>>()),
        format!("{} {:?}", state.t.to_bits(), prev_t.map(f64::to_bits)),
        format!("{outputs:?}"),
        format!(
            "{:?}",
            outputs
                .iter()
                .map(|(id, v)| (id, bits(v)))
                .collect::<Vec<_>>()
        ),
        format!("{store_inputs:?} {semantic_warnings:?}"),
        format!("{params:?} {mode:?} {params_dirty}"),
        format!(
            "{:?}",
            params
                .iter()
                .map(|(p, v, a)| (p, bits(&v), format!("{a:?}")))
                .collect::<Vec<_>>()
        ),
        format!("{io:?}"),
        format!("{durable_batch:?}"),
        format!(
            "{:?}",
            durable_batch
                .writes()
                .iter()
                .map(|w| match w.sample.value {
                    OcValue::Real(v) => Some(v.to_bits()),
                    _ => None,
                })
                .collect::<Vec<_>>()
        ),
        format!("{realtime_epoch_unix_nanos:?} {loaded} {durable_restore_ready}"),
    ]
}

fn prepared(state: u8) -> Engine<LoadFailureStore> {
    let mut engine = Engine::with_store(Arc::new(LoadFailureStore::new(StoreFailure::None)));
    engine.load_cxf(MODEL).unwrap();
    engine.set_realtime_epoch_unix_nanos(1_700_000_000_000_000_000);
    if state > 0 {
        engine
            .set_input("http://example.org#MinLoop.uSet", Value::Real(-0.0))
            .unwrap();
        engine.step_realtime(0.0).unwrap();
        engine.step_realtime(2.0).unwrap();
        assert!(!engine.state.words.is_empty());
        assert!(!engine.durable_restore_ready);
    }
    if state > 1 {
        engine.halt().unwrap();
        engine
            .set_param("http://example.org#MinLoop.con.k", Value::Real(3.0))
            .unwrap();
        assert!(engine.params_dirty);
    }
    engine.store.calls.lock().unwrap().clear();
    engine
}

fn terminal(error: &OcError) -> &OcError {
    if matches!(error, OcError::LoadContext(_)) {
        std::error::Error::source(error)
            .unwrap()
            .downcast_ref()
            .unwrap()
    } else {
        error
    }
}

fn structural_refusal() -> Vec<u8> {
    let mut doc: serde_json::Value = serde_json::from_str(DISPLAY).unwrap();
    let node = doc["@graph"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|node| node["@id"] == "http://example.org#U.con")
        .unwrap();
    node.as_object_mut().unwrap().remove("S231:hasParameter");
    serde_json::to_vec(&doc).unwrap()
}

#[test]
fn pre_store_refusals_preserve_fresh_advanced_and_dirty_run_images() {
    let structural = structural_refusal();
    let oversize = vec![b'?'; crate::MAX_CXF_BYTES + 1];
    for state in 0..3 {
        for (bytes, stage) in [
            (oversize.as_slice(), DiagnosticStage::Import),
            (b"{".as_slice(), DiagnosticStage::Import),
            (
                b"{\"@context\":{},\"@graph\":[]}".as_slice(),
                DiagnosticStage::Import,
            ),
            (UNITS, DiagnosticStage::AttributeUnification),
            (structural.as_slice(), DiagnosticStage::Validation),
            (LOOP, DiagnosticStage::Schedule),
        ] {
            for receipt in [false, true] {
                let mut engine = prepared(state);
                let before = image(&engine);
                let error = if receipt {
                    let failure = engine.load_cxf_with_receipt(bytes).unwrap_err();
                    assert_eq!(failure.stage(), stage);
                    failure.into_error()
                } else {
                    engine.load_cxf(bytes).unwrap_err()
                };
                match stage {
                    DiagnosticStage::Import if bytes.len() > crate::MAX_CXF_BYTES => {
                        assert!(matches!(error, OcError::CxfTooLarge { .. }))
                    }
                    DiagnosticStage::Import => assert!(matches!(error, OcError::Cxf(_))),
                    DiagnosticStage::AttributeUnification | DiagnosticStage::Validation => {
                        assert!(matches!(terminal(&error), OcError::Validate(_)))
                    }
                    DiagnosticStage::Schedule => {
                        assert!(matches!(terminal(&error), OcError::Build(_)))
                    }
                    _ => unreachable!(),
                }
                assert_eq!(image(&engine), before, "{stage:?} state={state}");
                assert!(engine.store.calls.lock().unwrap().is_empty());
            }
        }
    }
}

#[test]
fn store_refusals_preserve_fresh_advanced_and_dirty_run_images() {
    for state in 0..3 {
        for (fault, stage, calls) in [
            (StoreFailure::Recover, DiagnosticStage::StoreRecovery, 1),
            (StoreFailure::SaveModel, DiagnosticStage::StoreSave, 2),
            (StoreFailure::ResolvePoints, DiagnosticStage::StoreInputs, 3),
            (StoreFailure::HandleCount, DiagnosticStage::StoreInputs, 3),
        ] {
            for receipt in [false, true] {
                let mut engine = prepared(state);
                *engine.store.failure.lock().unwrap() = fault;
                let before = image(&engine);
                let error = if receipt {
                    let failure = engine
                        .load_cxf_with_receipt(ANALOG_WARNING.as_bytes())
                        .unwrap_err();
                    assert_eq!(failure.stage(), stage);
                    failure.into_error()
                } else {
                    engine.load_cxf(ANALOG_WARNING.as_bytes()).unwrap_err()
                };
                match fault {
                    StoreFailure::HandleCount => assert!(matches!(
                        terminal(&error),
                        OcError::Store(StoreError::Validation(_))
                    )),
                    _ => assert!(matches!(
                        terminal(&error),
                        OcError::Store(StoreError::Backend(_))
                    )),
                }
                assert_eq!(error.all_diagnostics().count(), 1);
                assert_eq!(image(&engine), before, "{stage:?} state={state}");
                assert_eq!(
                    *engine.store.calls.lock().unwrap(),
                    ["recover", "save_model", "resolve_points"][..calls]
                );
            }
        }
    }
}

#[test]
fn failed_handle_resolution_leaves_saved_model_and_allocated_store_handles() {
    let mut engine = prepared(1);
    let before = image(&engine);
    let old_models = engine.store.list_models().unwrap();
    let (candidate, report) =
        oce_cxf::import_cxf(ANALOG_WARNING.as_bytes(), &Default::default()).unwrap();
    let candidate_id = DomainKey::new(report.model_iri.unwrap());
    *engine.store.failure.lock().unwrap() = StoreFailure::HandleCount;
    let error = engine.load_cxf(ANALOG_WARNING.as_bytes()).unwrap_err();
    assert!(matches!(
        terminal(&error),
        OcError::Store(StoreError::Validation(_))
    ));
    assert_eq!(image(&engine), before);
    assert!(!old_models.contains(&candidate_id));
    assert_eq!(
        engine.store.load_model(&candidate_id).unwrap().model_id,
        candidate_id
    );
    let keys: Vec<_> = IoInventory::build_at_load(&candidate)
        .input_bindings()
        .iter()
        .map(|input| DomainKey::new(input.path.clone()))
        .collect();
    let residual_handles = engine.store.resolved.lock().unwrap().clone();
    assert!(!residual_handles.is_empty());
    // MemStore reuses the already allocated candidate handles; there is no engine rollback.
    assert_eq!(
        engine.store.inner.resolve_points(&keys).unwrap(),
        residual_handles
    );
    assert_ne!(engine.model_id, candidate_id);
}

#[test]
fn private_build_tail_refusals_preserve_the_run_image() {
    // Import refuses unknown classes earlier; the internal build seam has a real typed
    // instantiation refusal. Projection rejects an empty durable block identity.
    for projection in [false, true] {
        let mut engine = prepared(2);
        let before = image(&engine);
        let mut model = (*engine.model).clone();
        if projection {
            model.blocks[0].instance_iri = Some(Arc::from(""));
        } else {
            model.blocks[0].class_iri = Arc::from("unregistered:class");
        }
        let mut capture = DiagnosticCapture::new(true, DiagnosticStage::Instantiation);
        let error = engine
            .build_validated_model_in_memory(model, None, vec![], &mut capture)
            .unwrap_err();
        let failure = OperationFailure::new(error, capture);
        if projection {
            assert_eq!(failure.stage(), DiagnosticStage::Projection);
            assert!(matches!(
                failure.error(),
                OcError::Store(StoreError::Validation(_))
            ));
        } else {
            assert_eq!(failure.stage(), DiagnosticStage::Instantiation);
            assert!(matches!(failure.error(), OcError::Load { .. }));
        }
        assert_eq!(image(&engine), before);
        assert!(engine.store.calls.lock().unwrap().is_empty());
    }
}

#[test]
fn successful_reload_replaces_model_bound_caches_but_not_host_policy() {
    let mut engine = prepared(2);
    let old_model = Arc::clone(&engine.model);
    engine.set_cxf_byte_limit(64 * 1024).unwrap();
    engine.load_cxf(ANALOG_WARNING.as_bytes()).unwrap();
    let mut fresh = Engine::in_memory();
    fresh.load_cxf(ANALOG_WARNING.as_bytes()).unwrap();
    assert!(!Arc::ptr_eq(&old_model, &engine.model));
    assert_eq!(engine.model_id, fresh.model_id);
    assert_eq!(
        format!("{:?}", engine.schedule),
        format!("{:?}", fresh.schedule)
    );
    assert_eq!(engine.blocks.len(), fresh.blocks.len());
    assert_eq!(engine.state.words, fresh.state.words);
    assert_eq!(
        engine.state.values.iter().map(bits).collect::<Vec<_>>(),
        fresh.state.values.iter().map(bits).collect::<Vec<_>>()
    );
    assert_eq!(
        engine.state_snapshot().unwrap().as_bytes(),
        fresh.state_snapshot().unwrap().as_bytes()
    );
    assert_eq!(
        engine
            .outputs
            .to_map()
            .iter()
            .map(|(p, v)| (p, bits(v)))
            .collect::<Vec<_>>(),
        fresh
            .outputs
            .to_map()
            .iter()
            .map(|(p, v)| (p, bits(v)))
            .collect::<Vec<_>>()
    );
    assert_eq!(engine.io.to_vec(), fresh.io.to_vec());
    assert_eq!(engine.params.len(), fresh.params.len());
    assert_eq!(
        engine
            .params
            .iter()
            .map(|(p, v, _)| (p, bits(&v)))
            .collect::<Vec<_>>(),
        fresh
            .params
            .iter()
            .map(|(p, v, _)| (p, bits(&v)))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        format!("{:?}", engine.durable_batch),
        format!("{:?}", fresh.durable_batch)
    );
    assert_eq!(engine.semantic_warnings, fresh.semantic_warnings);
    assert_eq!(engine.mode, RunMode::Running);
    assert!(!engine.params_dirty);
    assert_eq!(engine.prev_t, None);
    assert!(engine.loaded && engine.durable_restore_ready);
    assert_eq!(engine.cxf_byte_limit(), 64 * 1024);
    assert_eq!(
        engine.realtime_epoch_unix_nanos(),
        Some(1_700_000_000_000_000_000)
    );
    assert!(matches!(
        engine.get_output("http://example.org#MinLoop.yAlarm"),
        Err(OcError::UnknownPoint(_))
    ));
    for input in &engine.store_inputs {
        assert!(!input.path.contains("MinLoop"));
    }
    // Exercise rebuilt batch/index mappings; old model outputs must not leak into writes.
    engine.step_realtime(0.0).unwrap();
    for write in engine.durable_batch.writes() {
        assert!(!write.key.as_str().contains("MinLoop"));
    }
}

#[test]
fn configuration_refusal_preserves_an_advanced_run_and_its_limit() {
    let mut engine = prepared(1);
    engine.set_cxf_byte_limit(64 * 1024).unwrap();
    let before = image(&engine);
    assert!(matches!(
        engine.set_cxf_byte_limit(usize::MAX),
        Err(OcError::CxfByteLimitTooLarge { .. })
    ));
    assert_eq!(image(&engine), before);
    assert!(engine.store.calls.lock().unwrap().is_empty());
}
