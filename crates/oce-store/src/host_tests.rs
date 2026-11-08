//! Tests for the host-side durability sequence: call order against a recording double, failure
//! propagation at every step, the count-mismatch refusal, and the happy path against `MemStore`.

use std::sync::Mutex;

use super::*;
use crate::{
    EquipmentDto, PointListRow, PointSnapshot, PointWrite, RelationDto, RetrievalHit,
    SemanticPayloadDto, SemanticQuery, SemanticStore, Store, TemplatePointReq,
};

fn model(id: &str) -> ResolvedModel {
    ResolvedModel {
        model_id: DomainKey::new(id),
        schema_rev: 1,
        classes: Vec::new(),
        blocks: Vec::new(),
        points: Vec::new(),
        connections: Vec::new(),
        containment: Vec::new(),
    }
}

const INPUTS: [(&str, PointValueType); 3] = [
    ("ahu.TZon", PointValueType::Real),
    ("ahu.uOcc", PointValueType::Bool),
    ("ahu.uOpeMod", PointValueType::Int),
];

/// What the recording double does at each port call.
#[derive(Default)]
struct Script {
    fail_recover: bool,
    fail_save: bool,
    fail_resolve: bool,
    /// Added to the number of keys when building the resolve result (negative drops handles).
    handle_skew: isize,
}

/// A `Store` that records every port call in order and fails on command.
#[derive(Default)]
struct Recorder {
    script: Script,
    calls: Mutex<Vec<String>>,
}

impl Recorder {
    fn with(script: Script) -> Self {
        Self {
            script,
            calls: Mutex::new(Vec::new()),
        }
    }
    fn log(&self, call: impl Into<String>) {
        self.calls.lock().expect("calls lock").push(call.into());
    }
    fn calls(&self) -> Vec<String> {
        self.calls.lock().expect("calls lock").clone()
    }
}

impl Durable for Recorder {
    fn commit(&self) -> StoreResult<()> {
        self.log("commit");
        Ok(())
    }
    fn flush(&self) -> StoreResult<()> {
        self.log("flush");
        Ok(())
    }
    fn recover(&self) -> StoreResult<()> {
        self.log("recover");
        if self.script.fail_recover {
            return Err(StoreError::Backend("recover refused".to_owned()));
        }
        Ok(())
    }
}

impl ModelStore for Recorder {
    fn save_model(&self, model: &ResolvedModel) -> StoreResult<()> {
        self.log(format!("save_model {}", model.model_id.as_str()));
        if self.script.fail_save {
            return Err(StoreError::Durability("save refused".to_owned()));
        }
        Ok(())
    }
    fn load_model(&self, model_id: &DomainKey) -> StoreResult<ResolvedModel> {
        self.log("load_model");
        Err(StoreError::ModelNotLoaded(model_id.clone()))
    }
    fn list_models(&self) -> StoreResult<Vec<DomainKey>> {
        self.log("list_models");
        Ok(Vec::new())
    }
    fn delete_model(&self, _model_id: &DomainKey) -> StoreResult<()> {
        self.log("delete_model");
        Ok(())
    }
}

impl PointStore for Recorder {
    fn resolve_points(&self, keys: &[DomainKey]) -> StoreResult<Vec<PointHandle>> {
        let names: Vec<&str> = keys.iter().map(DomainKey::as_str).collect();
        self.log(format!("resolve_points {}", names.join(",")));
        if self.script.fail_resolve {
            return Err(StoreError::Backend("resolve refused".to_owned()));
        }
        let n = keys.len().saturating_add_signed(self.script.handle_skew);
        Ok((0..n as u64).map(PointHandle).collect())
    }
    fn snapshot(&self) -> StoreResult<Box<dyn PointSnapshot>> {
        self.log("snapshot");
        Err(StoreError::Unsupported("recorder has no snapshot"))
    }
    fn write_points(&self, batch: &[PointWrite]) -> StoreResult<usize> {
        self.log("write_points");
        Ok(batch.len())
    }
}

impl SemanticStore for Recorder {
    fn upsert_equipment(&self, _eq: &EquipmentDto) -> StoreResult<()> {
        unreachable!("the post-load sequence never touches the semantic port")
    }
    fn add_relation(&self, _rel: &RelationDto) -> StoreResult<()> {
        unreachable!("the post-load sequence never touches the semantic port")
    }
    fn put_semantic_payload(&self, _p: &SemanticPayloadDto) -> StoreResult<()> {
        unreachable!("the post-load sequence never touches the semantic port")
    }
    fn get_semantic_payloads(&self, _s: &DomainKey) -> StoreResult<Vec<SemanticPayloadDto>> {
        unreachable!("the post-load sequence never touches the semantic port")
    }
    fn point_list(&self, _device: Option<&str>) -> StoreResult<Vec<PointListRow>> {
        unreachable!("the post-load sequence never touches the semantic port")
    }
    fn retrieve(&self, _q: &SemanticQuery) -> StoreResult<Vec<RetrievalHit>> {
        unreachable!("the post-load sequence never touches the semantic port")
    }
    fn match_template(&self, _req: &[TemplatePointReq]) -> StoreResult<Vec<DomainKey>> {
        unreachable!("the post-load sequence never touches the semantic port")
    }
}

const FULL_ORDER: [&str; 3] = [
    "recover",
    "save_model seq:ahu",
    "resolve_points ahu.TZon,ahu.uOcc,ahu.uOpeMod",
];

#[test]
fn sequence_runs_recover_then_save_then_resolve_and_nothing_else() {
    let store = Recorder::default();
    let handles = persist_loaded_model(&store, &model("seq:ahu"), INPUTS).unwrap();
    assert_eq!(
        handles,
        vec![PointHandle(0), PointHandle(1), PointHandle(2)]
    );
    // Exactly three port calls, in this order: no commit, no flush, no read-back.
    assert_eq!(store.calls(), FULL_ORDER);
}

#[test]
fn sequence_accepts_a_trait_object_store() {
    let store = Recorder::default();
    let dyn_store: &dyn Store = &store;
    persist_loaded_model(dyn_store, &model("seq:ahu"), INPUTS).unwrap();
    assert_eq!(store.calls(), FULL_ORDER);
}

#[test]
fn recover_failure_stops_the_sequence_before_any_write() {
    let store = Recorder::with(Script {
        fail_recover: true,
        ..Script::default()
    });
    let err = persist_loaded_model(&store, &model("seq:ahu"), INPUTS).unwrap_err();
    assert!(matches!(&err, StoreError::Backend(m) if m == "recover refused"));
    assert_eq!(store.calls(), ["recover"]);
}

#[test]
fn save_failure_propagates_unchanged_and_skips_input_resolution() {
    let store = Recorder::with(Script {
        fail_save: true,
        ..Script::default()
    });
    let err = persist_loaded_model(&store, &model("seq:ahu"), INPUTS).unwrap_err();
    assert!(matches!(&err, StoreError::Durability(m) if m == "save refused"));
    assert_eq!(store.calls(), FULL_ORDER[..2]);
}

#[test]
fn resolve_failure_propagates_unchanged_after_recover_and_save() {
    let store = Recorder::with(Script {
        fail_resolve: true,
        ..Script::default()
    });
    let err = persist_loaded_model(&store, &model("seq:ahu"), INPUTS).unwrap_err();
    assert!(matches!(&err, StoreError::Backend(m) if m == "resolve refused"));
    assert_eq!(store.calls(), FULL_ORDER);
}

#[test]
fn too_few_handles_is_a_validation_refusal_with_a_pinned_message() {
    let store = Recorder::with(Script {
        handle_skew: -1,
        ..Script::default()
    });
    let err = persist_loaded_model(&store, &model("seq:ahu"), INPUTS).unwrap_err();
    assert_eq!(
        err.to_string(),
        "schema/validation rejected at commit: \
         PointStore::resolve_points returned 2 handles for 3 input points"
    );
    // The model was already saved; the refusal does not roll it back.
    assert_eq!(store.calls(), FULL_ORDER);
}

#[test]
fn too_many_handles_is_a_validation_refusal() {
    let store = Recorder::with(Script {
        handle_skew: 1,
        ..Script::default()
    });
    let err = validate_inputs(&store, INPUTS).unwrap_err();
    assert!(matches!(&err, StoreError::Validation(m)
        if m == "PointStore::resolve_points returned 4 handles for 3 input points"));
}

#[test]
fn a_backend_returning_no_handles_for_one_input_is_refused() {
    let store = Recorder::with(Script {
        handle_skew: -1,
        ..Script::default()
    });
    let err = validate_inputs(&store, [("ahu.TZon", PointValueType::Real)]).unwrap_err();
    assert!(matches!(err, StoreError::Validation(_)));
}

#[test]
fn no_inputs_skips_resolution_entirely() {
    // A skewed backend would refuse any resolve; with no inputs it is never asked.
    let store = Recorder::with(Script {
        handle_skew: 5,
        fail_resolve: true,
        ..Script::default()
    });
    let handles = persist_loaded_model(&store, &model("seq:empty"), []).unwrap();
    assert!(handles.is_empty());
    assert_eq!(store.calls(), ["recover", "save_model seq:empty"]);
}

#[test]
fn each_step_is_callable_alone_and_makes_exactly_its_own_call() {
    let store = Recorder::default();
    recover(&store).unwrap();
    assert_eq!(store.calls(), ["recover"]);
    save_model(&store, &model("seq:one")).unwrap();
    assert_eq!(store.calls(), ["recover", "save_model seq:one"]);
    validate_inputs(&store, [("p", PointValueType::Bool)]).unwrap();
    assert_eq!(
        store.calls(),
        ["recover", "save_model seq:one", "resolve_points p"]
    );
}

#[test]
fn inputs_resolve_in_iteration_order_whatever_their_value_types() {
    let store = Recorder::default();
    let reversed = INPUTS.iter().rev().copied();
    validate_inputs(&store, reversed).unwrap();
    assert_eq!(
        store.calls(),
        ["resolve_points ahu.uOpeMod,ahu.uOcc,ahu.TZon"]
    );
}

/// A backend with only the three ports the sequence touches: no `SemanticStore`, so not a
/// `Store`. Its methods outside the sequence are unreachable by construction.
#[derive(Default)]
struct PortsOnly {
    calls: Mutex<Vec<&'static str>>,
}

impl Durable for PortsOnly {
    fn commit(&self) -> StoreResult<()> {
        unreachable!("the post-load sequence never commits")
    }
    fn flush(&self) -> StoreResult<()> {
        unreachable!("the post-load sequence never flushes")
    }
    fn recover(&self) -> StoreResult<()> {
        self.calls.lock().expect("calls lock").push("recover");
        Ok(())
    }
}

impl ModelStore for PortsOnly {
    fn save_model(&self, _model: &ResolvedModel) -> StoreResult<()> {
        self.calls.lock().expect("calls lock").push("save_model");
        Ok(())
    }
    fn load_model(&self, _model_id: &DomainKey) -> StoreResult<ResolvedModel> {
        unreachable!("the post-load sequence never reads a model back")
    }
    fn list_models(&self) -> StoreResult<Vec<DomainKey>> {
        unreachable!("the post-load sequence never lists models")
    }
    fn delete_model(&self, _model_id: &DomainKey) -> StoreResult<()> {
        unreachable!("the post-load sequence never deletes")
    }
}

impl PointStore for PortsOnly {
    fn resolve_points(&self, keys: &[DomainKey]) -> StoreResult<Vec<PointHandle>> {
        self.calls
            .lock()
            .expect("calls lock")
            .push("resolve_points");
        Ok((0..keys.len() as u64).map(PointHandle).collect())
    }
    fn snapshot(&self) -> StoreResult<Box<dyn PointSnapshot>> {
        unreachable!("the post-load sequence never snapshots")
    }
    fn write_points(&self, _batch: &[PointWrite]) -> StoreResult<usize> {
        unreachable!("the post-load sequence never writes points")
    }
}

#[test]
fn a_backend_without_a_semantic_port_can_run_the_whole_sequence() {
    let store = PortsOnly::default();
    let handles = persist_loaded_model(&store, &model("seq:ahu"), INPUTS).unwrap();
    assert_eq!(
        handles,
        vec![PointHandle(0), PointHandle(1), PointHandle(2)]
    );
    let calls = store.calls.lock().expect("calls lock").clone();
    assert_eq!(calls, ["recover", "save_model", "resolve_points"]);
}

#[cfg(feature = "mem")]
mod with_mem_store {
    use super::*;
    use crate::MemStore;

    #[test]
    fn mem_store_happy_path_saves_the_model_and_resolves_inputs_in_order() {
        let store = MemStore::new();
        let m = model("seq:ahu");
        let handles = persist_loaded_model(&store, &m, INPUTS).unwrap();
        assert_eq!(
            handles,
            vec![PointHandle(0), PointHandle(1), PointHandle(2)]
        );
        assert_eq!(store.load_model(&m.model_id).unwrap().model_id, m.model_id);
        assert_eq!(store.list_models().unwrap(), vec![m.model_id.clone()]);
        // Resolution allocates slots but writes no samples.
        let snap = store.snapshot().unwrap();
        assert!(snap.read_by_key(&DomainKey::new("ahu.TZon")).is_none());
    }

    #[test]
    fn mem_store_rerun_is_idempotent_and_keeps_handles_stable() {
        let store = MemStore::new();
        let first = persist_loaded_model(&store, &model("seq:ahu"), INPUTS).unwrap();
        let second = persist_loaded_model(&store, &model("seq:ahu"), INPUTS).unwrap();
        assert_eq!(first, second);
        assert_eq!(store.list_models().unwrap().len(), 1);
    }

    #[test]
    fn mem_store_duplicate_input_paths_share_one_handle_and_pass_the_count_check() {
        let store = MemStore::new();
        let inputs = [("x", PointValueType::Real), ("x", PointValueType::Real)];
        let handles = validate_inputs(&store, inputs).unwrap();
        assert_eq!(handles, vec![PointHandle(0), PointHandle(0)]);
    }

    #[test]
    fn mem_store_save_replaces_a_model_with_the_same_id() {
        let store = MemStore::new();
        persist_loaded_model(&store, &model("seq:ahu"), []).unwrap();
        let mut newer = model("seq:ahu");
        newer.schema_rev = 2;
        persist_loaded_model(&store, &newer, []).unwrap();
        let stored = store.load_model(&DomainKey::new("seq:ahu")).unwrap();
        assert_eq!(stored.schema_rev, 2);
    }
}
