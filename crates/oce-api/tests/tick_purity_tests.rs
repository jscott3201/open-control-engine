//! Tick-path store-purity and allocation guards for the `Engine` facade.
//!
//! This gate drives all three in-tree G36 fixtures (`ahu_supply_air_temp_reset`,
//! `ahu_economizer`, and `vav_single_zone`) so the eval path covers the current
//! stateful/control surface: integrators, filters, latches, timers, hysteresis,
//! discrete blocks, integers, and conversions.

mod support;

use std::alloc::System;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use oce_api::oce_store::{
    DomainKey, Durability, Durable, EquipmentDto, ModelStore, OcValue, PointHandle, PointListRow,
    PointSample, PointSnapshot, PointStatus, PointStore, PointWrite, RelationDto, ResolvedModel,
    RetrievalHit, SemanticPayloadDto, SemanticQuery, SemanticStore, StoreResult, TemplatePointReq,
};
use oce_api::{CollectSpec, Engine, InputSource, SimSpec, Value};
use oce_store_mem::MemStore;
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, Stats, StatsAlloc};
use support::recording_store::{RecordingStore, StoreCallSnapshot};

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

const AHU_SAT_RESET: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/ahu_supply_air_temp_reset.jsonld");
const AHU_ECONOMIZER: &str = include_str!("../../oce-cxf/tests/fixtures/g36/ahu_economizer.jsonld");
const VAV_SINGLE_ZONE: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/vav_single_zone.jsonld");
const NO_STORE_INPUTS: &str = r##"{
  "@context": {
    "S231": "http://data.ashrae.org/S231P#",
    "base": "http://example.org#"
  },
  "@graph": [
    {
      "@id": "http://example.org#NoStoreInputs",
      "@type": "S231:Block",
      "S231:label": "NoStoreInputs",
      "S231:containsBlock": { "@id": "http://example.org#NoStoreInputs.con" },
      "S231:hasOutput": { "@id": "http://example.org#NoStoreInputs.y" }
    },
    {
      "@id": "http://example.org#NoStoreInputs.con",
      "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
      "S231:label": "con",
      "S231:hasParameter": { "@id": "http://example.org#NoStoreInputs.con.k" },
      "S231:hasOutput": { "@id": "http://example.org#NoStoreInputs.con.y" }
    },
    {
      "@id": "http://example.org#NoStoreInputs.con.k",
      "S231:value": { "@value": "2.0", "@type": "http://www.w3.org/2001/XMLSchema#double" }
    },
    {
      "@id": "http://example.org#NoStoreInputs.con.y",
      "@type": "S231:RealOutput",
      "S231:isOfDataType": { "@id": "S231:Real" },
      "S231:isConnectedTo": { "@id": "http://example.org#NoStoreInputs.y" }
    },
    {
      "@id": "http://example.org#NoStoreInputs.y",
      "@type": "S231:RealOutput",
      "S231:isOfDataType": { "@id": "S231:Real" }
    }
  ]
}"##;

const SAT_ZONE_TEMP: &str = "http://example.org#g36.ahu_supply_air_temp_reset.zone_temp";
const SAT_COOLING_SETPOINT: &str =
    "http://example.org#g36.ahu_supply_air_temp_reset.cooling_setpoint";
const ECON_RETURN_AIR_TEMP: &str = "http://example.org#g36.ahu_economizer.return_air_temp";
const ECON_OUTDOOR_AIR_TEMP: &str = "http://example.org#g36.ahu_economizer.outdoor_air_temp";
const ECON_OPERATING_MODE: &str = "http://example.org#g36.ahu_economizer.operating_mode";
const VAV_ZONE_TEMP: &str = "http://example.org#g36.vav_single_zone.zone_temp";
const VAV_COOLING_SETPOINT: &str = "http://example.org#g36.vav_single_zone.cooling_setpoint";
const VAV_HEATING_SETPOINT: &str = "http://example.org#g36.vav_single_zone.heating_setpoint";

struct G36Fixture {
    name: &'static str,
    cxf: &'static str,
    t_stop: u64,
    inputs: fn(f64) -> Vec<(String, Value)>,
}

const G36_FIXTURES: &[G36Fixture] = &[
    G36Fixture {
        name: "ahu_supply_air_temp_reset",
        cxf: AHU_SAT_RESET,
        t_stop: 4,
        inputs: sat_inputs,
    },
    G36Fixture {
        name: "ahu_economizer",
        cxf: AHU_ECONOMIZER,
        t_stop: 5,
        inputs: economizer_inputs,
    },
    G36Fixture {
        name: "vav_single_zone",
        cxf: VAV_SINGLE_ZONE,
        t_stop: 5,
        inputs: vav_inputs,
    },
];

#[test]
fn g36_tick_path_stays_store_pure_and_alloc_free() {
    assert_recording_store_guard_and_counters_are_live();
    assert_read_resolved_returns_none_for_missing_handles();
    assert_load_saves_model_once_and_tick_does_not_save_again();
    assert_simulate_uses_no_forbidden_store_methods();
    assert_manual_g36_tick_allocates_nothing();
    assert_real_mem_store_tick_allocates_snapshot_floor();
}

#[test]
fn no_store_input_tick_path_skips_snapshot_and_reads() {
    let store = Arc::new(RecordingStore::default());
    let mut engine = Engine::with_store(Arc::clone(&store));
    let report = engine
        .load_cxf(NO_STORE_INPUTS.as_bytes())
        .unwrap_or_else(|e| panic!("no-store-input fixture loads: {e:?}"));
    let input_count = report.io.analog_inputs + report.io.digital_inputs;
    assert_eq!(input_count, 0, "fixture must have no store-backed inputs");

    store.reset_calls();
    store.arm_hot_path_guard();
    let metrics = engine
        .simulate(&SimSpec {
            t_start: 0.0,
            t_stop: 3.0,
            step: 1.0,
            inputs: InputSource::Closure(Box::new(no_inputs)),
            collect: CollectSpec::All { stride: 1 },
        })
        .unwrap_or_else(|e| panic!("no-store-input fixture simulates: {e:?}"));

    assert_eq!(metrics.ticks, 4);
    let calls = store.calls();
    assert_tick_store_pure(calls);
    assert_store_backed_read_counts(calls, metrics.ticks, input_count, "no_store_inputs");
}

fn assert_recording_store_guard_and_counters_are_live() {
    let store = RecordingStore::default();
    let handle = store
        .resolve_points(&[DomainKey::new("test:recording-store-counter")])
        .expect("recording store delegates point resolution")[0];
    store.reset_calls();

    let snapshot = store
        .snapshot()
        .expect("recording store delegates snapshot acquisition");
    assert!(
        snapshot.read_resolved(handle).is_none(),
        "recording snapshot delegates read_resolved"
    );
    let calls = store.calls();
    assert_eq!(calls.snapshot, 1, "snapshot counter increments");
    assert_eq!(calls.read_resolved, 1, "read_resolved counter increments");

    let guarded_store = RecordingStore::default();
    guarded_store.arm_hot_path_guard();
    let panic = catch_unwind(AssertUnwindSafe(|| {
        let _ = guarded_store.commit();
    }));
    assert!(panic.is_err(), "armed guard panics on forbidden methods");
    assert_eq!(
        guarded_store.calls().commit,
        1,
        "forbidden method counter increments before the guard panics"
    );
}

fn assert_read_resolved_returns_none_for_missing_handles() {
    let store = MemStore::new();
    let handle = store
        .resolve_points(&[DomainKey::new("test:unwritten-point")])
        .expect("point resolution is infallible for MemStore")[0];
    let snapshot = store.snapshot().expect("snapshot acquisition succeeds");

    assert!(
        snapshot.read_resolved(handle).is_none(),
        "a resolved but unwritten point returns None"
    );
    assert!(
        snapshot.read_resolved(PointHandle(1)).is_none(),
        "the first out-of-range point handle returns None"
    );
    assert!(
        snapshot.read_resolved(PointHandle(u64::MAX)).is_none(),
        "an out-of-range point handle returns None"
    );
    assert!(
        snapshot
            .read_by_key(&DomainKey::new("test:unknown-point"))
            .is_none(),
        "an unknown point key returns None"
    );
}

fn assert_load_saves_model_once_and_tick_does_not_save_again() {
    for fixture in G36_FIXTURES {
        let store = Arc::new(RecordingStore::default());
        let mut engine = Engine::with_store(Arc::clone(&store));
        let report = engine
            .load_cxf(fixture.cxf.as_bytes())
            .unwrap_or_else(|e| panic!("{} fixture loads: {e:?}", fixture.name));
        assert!(
            !report.model_id.as_str().is_empty(),
            "{} fixture reports the saved model id",
            fixture.name
        );

        let post_load = store.calls();
        assert_eq!(
            post_load.save_model, 1,
            "{} fixture saves the model exactly once at load",
            fixture.name
        );
        assert_eq!(
            post_load.recover, 1,
            "{} fixture opens the store lifecycle exactly once at load",
            fixture.name
        );

        store.arm_hot_path_guard();
        for (path, value) in (fixture.inputs)(0.0) {
            engine
                .set_input(&path, value)
                .unwrap_or_else(|e| panic!("{} fixture stages input {path}: {e:?}", fixture.name));
        }
        engine
            .tick(0.0)
            .unwrap_or_else(|e| panic!("{} fixture ticks after load: {e:?}", fixture.name));

        let post_tick = store.calls();
        assert_eq!(
            post_tick.save_model, post_load.save_model,
            "{} fixture must not save the model on tick",
            fixture.name
        );
        assert_eq!(
            post_tick.recover, post_load.recover,
            "{} fixture must not recover on tick",
            fixture.name
        );
    }
}

fn assert_simulate_uses_no_forbidden_store_methods() {
    for fixture in G36_FIXTURES {
        let store = Arc::new(RecordingStore::default());
        let mut engine = Engine::with_store(Arc::clone(&store));
        let report = engine
            .load_cxf(fixture.cxf.as_bytes())
            .unwrap_or_else(|e| panic!("{} fixture loads: {e:?}", fixture.name));
        let input_count = report.io.analog_inputs + report.io.digital_inputs;

        store.reset_calls();
        store.arm_hot_path_guard();
        let inputs = fixture.inputs;
        let metrics = engine
            .simulate(&SimSpec {
                t_start: 0.0,
                t_stop: fixture.t_stop as f64,
                step: 1.0,
                inputs: InputSource::Closure(Box::new(inputs)),
                collect: CollectSpec::All { stride: 1 },
            })
            .unwrap_or_else(|e| panic!("{} fixture simulates: {e:?}", fixture.name));

        assert_eq!(metrics.ticks, fixture.t_stop + 1, "{}", fixture.name);
        let calls = store.calls();
        assert_tick_store_pure(calls);
        assert_store_backed_read_counts(calls, metrics.ticks, input_count, fixture.name);
    }
}

fn assert_manual_g36_tick_allocates_nothing() {
    for fixture in G36_FIXTURES {
        let store = Arc::new(AllocationStore::default());
        let mut engine = Engine::with_store(Arc::clone(&store));
        engine
            .load_cxf(fixture.cxf.as_bytes())
            .unwrap_or_else(|e| panic!("{} fixture loads: {e:?}", fixture.name));

        for step in 0..=fixture.t_stop {
            let t = step as f64;
            for (path, value) in (fixture.inputs)(t) {
                engine.set_input(&path, value).unwrap_or_else(|e| {
                    panic!("{} fixture stages input {path}: {e:?}", fixture.name)
                });
            }

            let region = Region::new(GLOBAL);
            engine
                .tick(t)
                .unwrap_or_else(|e| panic!("{} fixture ticks at t={t}: {e:?}", fixture.name));
            assert_no_heap_traffic(region.change(), fixture.name, t);
        }
    }
}

fn assert_real_mem_store_tick_allocates_snapshot_floor() {
    for fixture in G36_FIXTURES {
        let store = Arc::new(MemStore::new());
        let mut engine = Engine::with_store(Arc::clone(&store));
        engine
            .load_cxf(fixture.cxf.as_bytes())
            .unwrap_or_else(|e| panic!("{} fixture loads: {e:?}", fixture.name));

        for step in 0..=fixture.t_stop {
            let t = step as f64;
            write_inputs_to_store(store.as_ref(), (fixture.inputs)(t), step);

            let snapshot_floor = snapshot_alloc_stats(store.as_ref(), fixture.name, t);
            assert_box_snapshot_floor(snapshot_floor, fixture.name, t);

            let region = Region::new(GLOBAL);
            engine.tick(t).unwrap_or_else(|e| {
                panic!(
                    "{} real MemStore fixture ticks at t={t}: {e:?}",
                    fixture.name
                )
            });
            let tick_stats = region.change();
            assert_same_alloc_stats(tick_stats, snapshot_floor, fixture.name, t);
        }
    }
}

fn assert_tick_store_pure(calls: StoreCallSnapshot) {
    assert_eq!(calls.save_model, 0, "save_model is off-tick only");
    assert_eq!(calls.load_model, 0, "load_model is off-tick only");
    assert_eq!(calls.list_models, 0, "list_models is off-tick only");
    assert_eq!(calls.delete_model, 0, "delete_model is off-tick only");
    assert_eq!(calls.resolve_points, 0, "resolve_points is load-time only");
    assert_eq!(calls.write_points, 0, "write_points is off-tick only");
    assert!(
        calls.read_resolved >= calls.snapshot,
        "read_resolved calls should come from tick snapshots: calls={calls:?}"
    );
    assert_eq!(calls.read_by_key, 0, "read_by_key is not a hot-path read");
    assert_eq!(
        calls.upsert_equipment, 0,
        "semantic graph writes are off-tick only"
    );
    assert_eq!(
        calls.add_relation, 0,
        "semantic graph writes are off-tick only"
    );
    assert_eq!(
        calls.put_semantic_payload, 0,
        "semantic payload writes are off-tick only"
    );
    assert_eq!(
        calls.get_semantic_payloads, 0,
        "semantic payload queries are off-tick only"
    );
    assert_eq!(calls.point_list, 0, "point_list is a graph traversal");
    assert_eq!(calls.retrieve, 0, "retrieve is a graph/retrieval query");
    assert_eq!(
        calls.match_template, 0,
        "template matching is a graph query"
    );
    assert_eq!(calls.commit, 0, "commit is off-tick only");
    assert_eq!(calls.flush, 0, "flush is off-tick only");
    assert_eq!(calls.recover, 0, "recover is load/open only");
}

fn assert_store_backed_read_counts(
    calls: StoreCallSnapshot,
    ticks: u64,
    input_count: usize,
    fixture: &str,
) {
    let ticks = usize::try_from(ticks).expect("test tick count fits usize");
    let expected_snapshots = if input_count == 0 { 0 } else { ticks };
    assert_eq!(
        calls.snapshot, expected_snapshots,
        "{fixture} must acquire one store snapshot per tick only when it has input handles"
    );
    assert_eq!(
        calls.read_resolved,
        ticks * input_count,
        "{fixture} must read every pre-resolved input handle once per tick"
    );
}

fn assert_no_heap_traffic(stats: Stats, fixture: &str, t: f64) {
    assert_eq!(
        stats.allocations, 0,
        "{fixture} tick at t={t} allocated: {stats:?}"
    );
    assert_eq!(
        stats.reallocations, 0,
        "{fixture} tick at t={t} reallocated: {stats:?}"
    );
    assert_eq!(
        stats.deallocations, 0,
        "{fixture} tick at t={t} deallocated: {stats:?}"
    );
    assert_eq!(
        stats.bytes_allocated, 0,
        "{fixture} tick at t={t} allocated bytes: {stats:?}"
    );
    assert_eq!(
        stats.bytes_reallocated, 0,
        "{fixture} tick at t={t} reallocated bytes: {stats:?}"
    );
    assert_eq!(
        stats.bytes_deallocated, 0,
        "{fixture} tick at t={t} deallocated bytes: {stats:?}"
    );
}

fn snapshot_alloc_stats(store: &MemStore, fixture: &str, t: f64) -> Stats {
    let region = Region::new(GLOBAL);
    {
        let _snapshot = store
            .snapshot()
            .unwrap_or_else(|e| panic!("{fixture} snapshot floor at t={t}: {e:?}"));
    }
    region.change()
}

fn assert_box_snapshot_floor(stats: Stats, fixture: &str, t: f64) {
    // The frozen `Box<dyn PointSnapshot>` seam leaves one small Box allocation per tick. The point
    // state itself is Arc-backed, so this bound is O(1) and independent of point count.
    assert_eq!(
        stats.allocations, 1,
        "{fixture} snapshot floor at t={t} must allocate exactly one Box: {stats:?}"
    );
    assert_eq!(
        stats.reallocations, 0,
        "{fixture} snapshot floor at t={t} must not reallocate: {stats:?}"
    );
    assert_eq!(
        stats.deallocations, 1,
        "{fixture} snapshot floor at t={t} must deallocate exactly one Box: {stats:?}"
    );
    assert!(
        stats.bytes_allocated <= 128,
        "{fixture} snapshot floor at t={t} allocated too many bytes: {stats:?}"
    );
    assert_eq!(
        stats.bytes_reallocated, 0,
        "{fixture} snapshot floor at t={t} must not reallocate bytes: {stats:?}"
    );
    assert!(
        stats.bytes_deallocated <= 128,
        "{fixture} snapshot floor at t={t} deallocated too many bytes: {stats:?}"
    );
}

fn assert_same_alloc_stats(actual: Stats, expected: Stats, fixture: &str, t: f64) {
    assert_eq!(
        actual.allocations, expected.allocations,
        "{fixture} real MemStore tick at t={t} allocations must match snapshot floor: actual={actual:?}, expected={expected:?}"
    );
    assert_eq!(
        actual.reallocations, expected.reallocations,
        "{fixture} real MemStore tick at t={t} reallocations must match snapshot floor: actual={actual:?}, expected={expected:?}"
    );
    assert_eq!(
        actual.deallocations, expected.deallocations,
        "{fixture} real MemStore tick at t={t} deallocations must match snapshot floor: actual={actual:?}, expected={expected:?}"
    );
    assert_eq!(
        actual.bytes_allocated, expected.bytes_allocated,
        "{fixture} real MemStore tick at t={t} allocated bytes must match snapshot floor: actual={actual:?}, expected={expected:?}"
    );
    assert_eq!(
        actual.bytes_reallocated, expected.bytes_reallocated,
        "{fixture} real MemStore tick at t={t} reallocated bytes must match snapshot floor: actual={actual:?}, expected={expected:?}"
    );
    assert_eq!(
        actual.bytes_deallocated, expected.bytes_deallocated,
        "{fixture} real MemStore tick at t={t} deallocated bytes must match snapshot floor: actual={actual:?}, expected={expected:?}"
    );
}

fn write_inputs_to_store(store: &MemStore, inputs: Vec<(String, Value)>, stamp: u64) {
    let writes: Vec<PointWrite> = inputs
        .into_iter()
        .map(|(path, value)| PointWrite {
            key: DomainKey::new(path),
            sample: PointSample {
                value: oc_value(value),
                status: PointStatus::Ok,
                at_unix_nanos: stamp,
            },
            durability: Durability::Telemetry,
        })
        .collect();
    store
        .write_points(&writes)
        .expect("MemStore accepts off-tick input writes");
}

fn oc_value(value: Value) -> OcValue {
    match value {
        Value::Real(v) => OcValue::Real(v),
        Value::Integer(v) => OcValue::Int(v),
        Value::Boolean(v) => OcValue::Bool(v),
        Value::String(v) => OcValue::String(v.to_string()),
        Value::Enum { ordinal, .. } => OcValue::Int(i64::from(ordinal)),
    }
}

fn pair(path: &str, value: Value) -> (String, Value) {
    (path.to_owned(), value)
}

fn no_inputs(_t: f64) -> Vec<(String, Value)> {
    Vec::new()
}

#[derive(Default)]
struct AllocationStore {
    inner: MemStore,
}

struct EmptySnapshot;

impl PointSnapshot for EmptySnapshot {
    fn read_resolved(&self, _handle: PointHandle) -> Option<PointSample> {
        None
    }

    fn read_by_key(&self, _key: &DomainKey) -> Option<PointSample> {
        None
    }
}

impl ModelStore for AllocationStore {
    fn save_model(&self, model: &ResolvedModel) -> StoreResult<()> {
        self.inner.save_model(model)
    }

    fn load_model(&self, model_id: &DomainKey) -> StoreResult<ResolvedModel> {
        self.inner.load_model(model_id)
    }

    fn list_models(&self) -> StoreResult<Vec<DomainKey>> {
        self.inner.list_models()
    }

    fn delete_model(&self, model_id: &DomainKey) -> StoreResult<()> {
        self.inner.delete_model(model_id)
    }
}

impl PointStore for AllocationStore {
    fn resolve_points(&self, keys: &[DomainKey]) -> StoreResult<Vec<PointHandle>> {
        self.inner.resolve_points(keys)
    }

    fn snapshot(&self) -> StoreResult<Box<dyn PointSnapshot>> {
        Ok(Box::new(EmptySnapshot))
    }

    fn write_points(&self, batch: &[PointWrite]) -> StoreResult<usize> {
        self.inner.write_points(batch)
    }
}

impl SemanticStore for AllocationStore {
    fn upsert_equipment(&self, eq: &EquipmentDto) -> StoreResult<()> {
        self.inner.upsert_equipment(eq)
    }

    fn add_relation(&self, rel: &RelationDto) -> StoreResult<()> {
        self.inner.add_relation(rel)
    }

    fn put_semantic_payload(&self, p: &SemanticPayloadDto) -> StoreResult<()> {
        self.inner.put_semantic_payload(p)
    }

    fn get_semantic_payloads(&self, subject: &DomainKey) -> StoreResult<Vec<SemanticPayloadDto>> {
        self.inner.get_semantic_payloads(subject)
    }

    fn point_list(&self, controlled_device: Option<&str>) -> StoreResult<Vec<PointListRow>> {
        self.inner.point_list(controlled_device)
    }

    fn retrieve(&self, q: &SemanticQuery) -> StoreResult<Vec<RetrievalHit>> {
        self.inner.retrieve(q)
    }

    fn match_template(&self, required_points: &[TemplatePointReq]) -> StoreResult<Vec<DomainKey>> {
        self.inner.match_template(required_points)
    }
}

impl Durable for AllocationStore {
    fn commit(&self) -> StoreResult<()> {
        self.inner.commit()
    }

    fn flush(&self) -> StoreResult<()> {
        self.inner.flush()
    }

    fn recover(&self) -> StoreResult<()> {
        self.inner.recover()
    }
}

fn sat_inputs(t: f64) -> Vec<(String, Value)> {
    let zone_temp = match t as u32 {
        0 => 22.0,
        1 => 24.0,
        2 => 24.5,
        _ => 25.5,
    };
    vec![
        pair(SAT_ZONE_TEMP, Value::Real(zone_temp)),
        pair(SAT_COOLING_SETPOINT, Value::Real(24.0)),
    ]
}

fn economizer_inputs(t: f64) -> Vec<(String, Value)> {
    let (return_temp, outdoor_temp, operating_mode) = match t as u32 {
        0 => (24.0, 23.0, 1),
        1..=3 => (24.0, 19.0, 1),
        4 => (24.0, 24.0, 1),
        _ => (24.0, 19.0, 0),
    };
    vec![
        pair(ECON_RETURN_AIR_TEMP, Value::Real(return_temp)),
        pair(ECON_OUTDOOR_AIR_TEMP, Value::Real(outdoor_temp)),
        pair(ECON_OPERATING_MODE, Value::Integer(operating_mode)),
    ]
}

fn vav_inputs(t: f64) -> Vec<(String, Value)> {
    let zone_temp = match t as u32 {
        0 => 22.0,
        1 => 27.0,
        2 => 27.5,
        3 => 19.0,
        4 => 19.3,
        _ => 21.0,
    };
    vec![
        pair(VAV_ZONE_TEMP, Value::Real(zone_temp)),
        pair(VAV_COOLING_SETPOINT, Value::Real(24.0)),
        pair(VAV_HEATING_SETPOINT, Value::Real(20.0)),
    ]
}
