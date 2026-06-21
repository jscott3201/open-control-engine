//! Tick-path store-purity and allocation guards for the `Engine` facade.

mod support;

use std::alloc::System;
use std::sync::Arc;

use oce_api::oce_store::{DomainKey, PointHandle, PointStore};
use oce_api::{CollectSpec, Engine, InputSource, SimSpec, Value};
use oce_store_mem::MemStore;
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, Stats, StatsAlloc};
use support::recording_store::{RecordingStore, StoreCallSnapshot};

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

const VAV_SINGLE_ZONE: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/vav_single_zone.jsonld");

const VAV_ZONE_TEMP: &str = "http://example.org#g36.vav_single_zone.zone_temp";
const VAV_COOLING_SETPOINT: &str = "http://example.org#g36.vav_single_zone.cooling_setpoint";
const VAV_HEATING_SETPOINT: &str = "http://example.org#g36.vav_single_zone.heating_setpoint";

#[test]
fn g36_tick_path_stays_store_pure_and_alloc_free() {
    assert_read_resolved_returns_none_for_missing_handles();
    assert_simulate_uses_no_forbidden_store_methods();
    assert_manual_g36_tick_allocates_nothing();
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
        snapshot.read_resolved(PointHandle(u64::MAX)).is_none(),
        "an out-of-range point handle returns None"
    );
}

fn assert_simulate_uses_no_forbidden_store_methods() {
    let store = Arc::new(RecordingStore::default());
    let mut engine = Engine::with_store(Arc::clone(&store));
    engine
        .load_cxf(VAV_SINGLE_ZONE.as_bytes())
        .expect("G36 VAV fixture loads through the recording store");

    store.reset_calls();
    store.arm_hot_path_guard();
    let metrics = engine
        .simulate(&SimSpec {
            t_start: 0.0,
            t_stop: 5.0,
            step: 1.0,
            inputs: InputSource::Closure(Box::new(vav_input_vec)),
            collect: CollectSpec::All { stride: 1 },
        })
        .expect("G36 VAV fixture simulates through the recording store");

    assert_eq!(metrics.ticks, 6);
    assert_tick_store_pure(store.calls(), metrics.ticks);
}

fn assert_manual_g36_tick_allocates_nothing() {
    let store = Arc::new(RecordingStore::default());
    let mut engine = Engine::with_store(store);
    engine
        .load_cxf(VAV_SINGLE_ZONE.as_bytes())
        .expect("G36 VAV fixture loads");

    for step in 0..=5 {
        let t = step as f64;
        for (path, value) in vav_inputs(t) {
            engine
                .set_input(path, value)
                .expect("fixture input point accepts its typed value");
        }

        let region = Region::new(GLOBAL);
        engine
            .tick(t)
            .expect("tick evaluates without store or alloc");
        assert_no_heap_traffic(region.change(), t);
    }
}

fn assert_tick_store_pure(calls: StoreCallSnapshot, ticks: u64) {
    let snapshot_limit = usize::try_from(ticks).expect("test tick count fits usize");
    assert!(
        calls.snapshot <= snapshot_limit,
        "snapshot calls must be <= one per tick: calls={calls:?}, ticks={ticks}"
    );

    assert_eq!(calls.save_model, 0, "save_model is off-tick only");
    assert_eq!(calls.load_model, 0, "load_model is off-tick only");
    assert_eq!(calls.list_models, 0, "list_models is off-tick only");
    assert_eq!(calls.delete_model, 0, "delete_model is off-tick only");
    assert_eq!(calls.resolve_points, 0, "resolve_points is load-time only");
    assert_eq!(calls.write_points, 0, "write_points is off-tick only");
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

fn assert_no_heap_traffic(stats: Stats, t: f64) {
    assert_eq!(stats.allocations, 0, "tick at t={t} allocated: {stats:?}");
    assert_eq!(
        stats.reallocations, 0,
        "tick at t={t} reallocated: {stats:?}"
    );
    assert_eq!(
        stats.deallocations, 0,
        "tick at t={t} deallocated: {stats:?}"
    );
    assert_eq!(
        stats.bytes_allocated, 0,
        "tick at t={t} allocated bytes: {stats:?}"
    );
    assert_eq!(
        stats.bytes_reallocated, 0,
        "tick at t={t} reallocated bytes: {stats:?}"
    );
    assert_eq!(
        stats.bytes_deallocated, 0,
        "tick at t={t} deallocated bytes: {stats:?}"
    );
}

fn vav_input_vec(t: f64) -> Vec<(String, Value)> {
    vav_inputs(t)
        .into_iter()
        .map(|(path, value)| (path.to_owned(), value))
        .collect()
}

fn vav_inputs(t: f64) -> [(&'static str, Value); 3] {
    let zone_temp = match t as u32 {
        0 => 22.0,
        1 => 27.0,
        2 => 27.5,
        3 => 19.0,
        4 => 19.3,
        _ => 21.0,
    };
    [
        (VAV_ZONE_TEMP, Value::Real(zone_temp)),
        (VAV_COOLING_SETPOINT, Value::Real(24.0)),
        (VAV_HEATING_SETPOINT, Value::Real(20.0)),
    ]
}
