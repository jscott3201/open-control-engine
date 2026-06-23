//! Store-backed input equivalence tests for stateful G36 fixtures.

use std::sync::Arc;

use oce_model::Value;
use oce_store::{Durability, OcValue, PointSample, PointStatus, PointStore, PointWrite};
use oce_store_mem::MemStore;

use crate::{CollectSpec, Engine, InputSource, OutputTrace, SimSpec};

const AHU_SAT_RESET: &str =
    include_str!("../../../oce-cxf/tests/fixtures/g36/ahu_supply_air_temp_reset.jsonld");
const AHU_ECONOMIZER: &str =
    include_str!("../../../oce-cxf/tests/fixtures/g36/ahu_economizer.jsonld");
const VAV_SINGLE_ZONE: &str =
    include_str!("../../../oce-cxf/tests/fixtures/g36/vav_single_zone.jsonld");

const SAT_ZONE_TEMP: &str = "http://example.org#g36.ahu_supply_air_temp_reset.zone_temp";
const SAT_COOLING_SETPOINT: &str =
    "http://example.org#g36.ahu_supply_air_temp_reset.cooling_setpoint";
const ECON_RETURN_AIR_TEMP: &str = "http://example.org#g36.ahu_economizer.return_air_temp";
const ECON_OUTDOOR_AIR_TEMP: &str = "http://example.org#g36.ahu_economizer.outdoor_air_temp";
const ECON_OPERATING_MODE: &str = "http://example.org#g36.ahu_economizer.operating_mode";
const VAV_ZONE_TEMP: &str = "http://example.org#g36.vav_single_zone.zone_temp";
const VAV_COOLING_SETPOINT: &str = "http://example.org#g36.vav_single_zone.cooling_setpoint";
const VAV_HEATING_SETPOINT: &str = "http://example.org#g36.vav_single_zone.heating_setpoint";

struct StoreFixture {
    name: &'static str,
    cxf: &'static str,
    t_stop: u64,
    inputs: fn(f64) -> Vec<(String, Value)>,
}

const STORE_FIXTURES: &[StoreFixture] = &[
    StoreFixture {
        name: "ahu_supply_air_temp_reset",
        cxf: AHU_SAT_RESET,
        t_stop: 4,
        inputs: sat_inputs,
    },
    StoreFixture {
        name: "ahu_economizer",
        cxf: AHU_ECONOMIZER,
        t_stop: 5,
        inputs: economizer_inputs,
    },
    StoreFixture {
        name: "vav_single_zone",
        cxf: VAV_SINGLE_ZONE,
        t_stop: 5,
        inputs: vav_inputs,
    },
];

#[test]
fn store_backed_inputs_match_host_staged_output_trace_for_stateful_g36_fixtures() {
    for fixture in STORE_FIXTURES {
        let host_trace = host_staged_trace(fixture);
        let store_trace = store_backed_trace(fixture);
        assert!(
            host_trace.rows() > 1,
            "{} fixture must exercise multiple ticks",
            fixture.name
        );
        assert_trace_bit_eq(&host_trace, &store_trace, fixture.name);
    }
}

fn host_staged_trace(fixture: &StoreFixture) -> OutputTrace {
    let mut engine = Engine::in_memory();
    engine
        .load_cxf(fixture.cxf.as_bytes())
        .unwrap_or_else(|err| panic!("{} fixture loads for host trace: {err:?}", fixture.name));
    let metrics = engine
        .simulate(&SimSpec {
            t_start: 0.0,
            t_stop: fixture.t_stop as f64,
            step: 1.0,
            inputs: InputSource::Closure(Box::new(fixture.inputs)),
            collect: CollectSpec::All { stride: 1 },
        })
        .unwrap_or_else(|err| panic!("{} fixture host trace simulates: {err:?}", fixture.name));
    metrics.trace
}

fn store_backed_trace(fixture: &StoreFixture) -> OutputTrace {
    let store = Arc::new(MemStore::new());
    let mut engine = Engine::with_store(Arc::clone(&store));
    engine
        .load_cxf(fixture.cxf.as_bytes())
        .unwrap_or_else(|err| panic!("{} fixture loads for store trace: {err:?}", fixture.name));
    let cols = engine
        .resolve_collect(&CollectSpec::All { stride: 1 })
        .unwrap_or_else(|err| panic!("{} fixture resolves outputs: {err:?}", fixture.name));
    let mut trace = OutputTrace::with_columns(cols.iter().map(|(path, _)| path.clone()).collect());
    for step in 0..=fixture.t_stop {
        let t = step as f64;
        write_inputs(store.as_ref(), (fixture.inputs)(t), step);
        engine.tick(t).unwrap_or_else(|err| {
            panic!(
                "{} fixture ticks from store at t={t}: {err:?}",
                fixture.name
            )
        });
        trace.push_row(t, &cols, &engine.state.values);
    }
    trace
}

fn write_inputs(store: &MemStore, inputs: Vec<(String, Value)>, stamp: u64) {
    let writes: Vec<PointWrite> = inputs
        .into_iter()
        .map(|(path, value)| PointWrite {
            key: oce_store::DomainKey::new(path),
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

fn assert_trace_bit_eq(expected: &OutputTrace, actual: &OutputTrace, fixture: &str) {
    assert_eq!(actual.columns(), expected.columns(), "{fixture} columns");
    assert_eq!(actual.rows(), expected.rows(), "{fixture} row count");
    for (idx, (actual_t, expected_t)) in actual.times().iter().zip(expected.times()).enumerate() {
        assert_eq!(
            actual_t.to_bits(),
            expected_t.to_bits(),
            "{fixture} time row {idx}"
        );
    }
    for col in 0..expected.columns().len() {
        let expected_col = expected.column(col).expect("expected column exists");
        let actual_col = actual.column(col).expect("actual column exists");
        for (row, (expected_value, actual_value)) in expected_col.iter().zip(actual_col).enumerate()
        {
            assert!(
                expected_value.bit_eq(actual_value),
                "{fixture} column {} row {row}: expected {expected_value:?}, got {actual_value:?}",
                expected.columns()[col]
            );
        }
    }
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
