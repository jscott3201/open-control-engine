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

use oce_api::oce_store::{DomainKey, Durable, PointHandle, PointStore};
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
        assert_tick_store_pure(calls, metrics.ticks);
        assert_store_backed_read_counts(calls, metrics.ticks, input_count, fixture.name);
    }
}

fn assert_manual_g36_tick_allocates_nothing() {
    for fixture in G36_FIXTURES {
        let store = Arc::new(RecordingStore::default());
        let mut engine = Engine::with_store(Arc::clone(&store));
        engine
            .load_cxf(fixture.cxf.as_bytes())
            .unwrap_or_else(|e| panic!("{} fixture loads: {e:?}", fixture.name));
        let _snapshot = store
            .snapshot()
            .unwrap_or_else(|e| panic!("{} fixture warms recording snapshot: {e:?}", fixture.name));

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

fn assert_tick_store_pure(calls: StoreCallSnapshot, ticks: u64) {
    let snapshot_limit = usize::try_from(ticks).expect("test tick count fits usize");
    // Snapshot calls are 0 today because the tick is still store-decoupled. The <= ticks bound is
    // the standing future contract; tighten it to equality when the store-backed read path lands.
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
    assert_eq!(
        calls.snapshot, ticks,
        "{fixture} must acquire exactly one store snapshot per tick"
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

fn pair(path: &str, value: Value) -> (String, Value) {
    (path.to_owned(), value)
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
