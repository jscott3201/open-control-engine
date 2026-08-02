//! Source-verified ASHRAE G36 OutdoorAirFlow AHU through the frozen facade.

use oce_api::{CollectSpec, Engine, InputSource, PointDirection, SimMetrics, SimSpec, Value};

const OUTDOOR_AIRFLOW_AHU: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/multizone_vav_outdoor_airflow_ahu.jsonld");

const POPULATION_FLOW: &str = "conn#1";
const AREA_FLOW: &str = "conn#2";
const PRIMARY_FLOW: &str = "conn#16";
const MAX_OUTDOOR_AIR_FRACTION: &str = "conn#13";
const MEASURED_OUTDOOR_AIR: &str = "conn#30";
const UNCORRECTED_OUTDOOR_AIR: &str = "conn#6";
const EFFECTIVE_MIN_OUTDOOR_AIR: &str = "conn#24";
const EFFECTIVE_NORMALIZED: &str = "conn#29";
const MEASURED_NORMALIZED: &str = "conn#32";

const EXPECTED_TIMES: [f64; 5] = [0.0, 1.0, 2.0, 3.0, 4.0];

type ScheduleSignature = (Vec<u32>, Vec<u32>, Vec<u32>);

fn pair(path: &str, value: Value) -> (String, Value) {
    (path.to_string(), value)
}

fn input_row(t: f64) -> (f64, f64, f64, f64, f64) {
    match t as u32 {
        0 => (1.0, 1.0, 5.0, 0.2, 4.0),
        1 => (4.0, 5.0, 2.0, 0.2, 6.0),
        2 => (0.002, 0.001, 0.0, 0.4, 0.1),
        3 => (0.0, 0.0, 1.0, 1.5, 9.0),
        _ => (5.0, 5.0, 100.0, 0.99, 8.8),
    }
}

fn outdoor_airflow_inputs(t: f64) -> Vec<(String, Value)> {
    let (population, area, primary, fraction, measured) = input_row(t);
    vec![
        pair(POPULATION_FLOW, Value::Real(population)),
        pair(AREA_FLOW, Value::Real(area)),
        pair(PRIMARY_FLOW, Value::Real(primary)),
        pair(MAX_OUTDOOR_AIR_FRACTION, Value::Real(fraction)),
        pair(MEASURED_OUTDOOR_AIR, Value::Real(measured)),
    ]
}

fn expected_outputs(t: f64) -> (f64, f64, f64, f64) {
    const V_UNC_DES_OUT_AIR_FLOW: f64 = 6.0;
    const V_DES_TOT_OUT_AIR_FLOW: f64 = 8.0;
    const NEAR_ZERO: f64 = 1E-4;

    let (population, area, primary, fraction, measured) = input_row(t);
    let uncorrected = V_UNC_DES_OUT_AIR_FLOW.min(population + area);
    let guarded_primary = primary.max(V_UNC_DES_OUT_AIR_FLOW * 1E-3);
    let system_efficiency = (1.0 + uncorrected / guarded_primary - fraction).max(NEAR_ZERO);
    let effective = V_DES_TOT_OUT_AIR_FLOW.min(uncorrected / system_efficiency);
    (
        uncorrected,
        effective,
        effective / V_DES_TOT_OUT_AIR_FLOW,
        measured / V_DES_TOT_OUT_AIR_FLOW,
    )
}

fn load_outdoor_airflow_ahu() -> Engine {
    let mut engine = Engine::in_memory();
    let report = engine
        .load_cxf(OUTDOOR_AIRFLOW_AHU.as_bytes())
        .expect("source-verified G36 OutdoorAirFlow AHU fixture loads");
    assert_eq!(report.block_count, 15);
    assert_eq!(report.stateful_blocks, 0);
    assert!(
        report.warnings.is_empty(),
        "fixture should not warn: {:?}",
        report.warnings
    );
    engine
}

fn schedule_signature(engine: &Engine) -> ScheduleSignature {
    let schedule = engine.schedule();
    (
        schedule.order.iter().map(|id| id.0).collect(),
        schedule.connector_order.iter().map(|id| id.0).collect(),
        schedule.driver_of.iter().map(|id| id.0).collect(),
    )
}

fn simulate(mut engine: Engine) -> (ScheduleSignature, SimMetrics) {
    let schedule = schedule_signature(&engine);
    let metrics = engine
        .simulate(&SimSpec {
            t_start: 0.0,
            t_stop: 4.0,
            step: 1.0,
            inputs: InputSource::Closure(Box::new(outdoor_airflow_inputs)),
            collect: CollectSpec::Named {
                points: vec![
                    UNCORRECTED_OUTDOOR_AIR.to_string(),
                    EFFECTIVE_MIN_OUTDOOR_AIR.to_string(),
                    EFFECTIVE_NORMALIZED.to_string(),
                    MEASURED_NORMALIZED.to_string(),
                ],
                stride: 1,
            },
        })
        .expect("G36 OutdoorAirFlow AHU simulates");
    assert_eq!(metrics.ticks, 5);
    assert_eq!(
        metrics
            .trace
            .times()
            .iter()
            .map(|t| t.to_bits())
            .collect::<Vec<_>>(),
        EXPECTED_TIMES
            .iter()
            .map(|t| t.to_bits())
            .collect::<Vec<_>>()
    );
    (schedule, metrics)
}

fn real_column(metrics: &SimMetrics, path: &str) -> Vec<f64> {
    let index = metrics
        .trace
        .columns()
        .iter()
        .position(|column| column == path)
        .unwrap_or_else(|| panic!("missing trace column {path}"));
    let column = metrics.trace.column(index).expect("column index is valid");
    column
        .iter()
        .map(|value| match value {
            Value::Real(x) => *x,
            other => panic!("{path} must be Real, got {other:?}"),
        })
        .collect()
}

fn expected_column(index: usize) -> Vec<f64> {
    EXPECTED_TIMES
        .iter()
        .map(|&t| match index {
            0 => expected_outputs(t).0,
            1 => expected_outputs(t).1,
            2 => expected_outputs(t).2,
            _ => expected_outputs(t).3,
        })
        .collect()
}

fn assert_real_bits(actual: &[f64], expected: &[f64], label: &str) {
    assert_eq!(
        actual.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
        expected.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
        "{label} diverged"
    );
}

fn assert_trace_bit_eq(left: &SimMetrics, right: &SimMetrics) {
    assert_eq!(left.trace.columns(), right.trace.columns());
    assert_eq!(
        left.trace
            .times()
            .iter()
            .map(|t| t.to_bits())
            .collect::<Vec<_>>(),
        right
            .trace
            .times()
            .iter()
            .map(|t| t.to_bits())
            .collect::<Vec<_>>()
    );
    for j in 0..left.trace.columns().len() {
        let l = left.trace.column(j).unwrap();
        let r = right.trace.column(j).unwrap();
        for (row, (lv, rv)) in l.iter().zip(r).enumerate() {
            assert!(
                lv.bit_eq(rv),
                "{} row {row} diverged: {lv:?} vs {rv:?}",
                left.trace.columns()[j]
            );
        }
    }
}

#[test]
fn multizone_vav_outdoor_airflow_ahu_loads_simulates_and_is_deterministic() {
    let engine = load_outdoor_airflow_ahu();
    let paths = engine
        .io()
        .iter()
        .map(|point| point.path.clone())
        .collect::<Vec<_>>();
    for input in [
        POPULATION_FLOW,
        AREA_FLOW,
        PRIMARY_FLOW,
        MAX_OUTDOOR_AIR_FRACTION,
        MEASURED_OUTDOOR_AIR,
    ] {
        assert!(
            paths.contains(&input.to_string()),
            "missing facade input {input}"
        );
    }
    for output in [
        UNCORRECTED_OUTDOOR_AIR,
        EFFECTIVE_MIN_OUTDOOR_AIR,
        EFFECTIVE_NORMALIZED,
        MEASURED_NORMALIZED,
    ] {
        assert!(
            paths.contains(&output.to_string()),
            "missing facade output {output}"
        );
    }
    let output_count = engine
        .io()
        .iter()
        .filter(|point| point.direction == PointDirection::Out)
        .count();
    assert_eq!(
        output_count, 15,
        "each active source block should expose one output"
    );

    let (schedule_a, metrics_a) = simulate(engine);
    let (schedule_b, metrics_b) = simulate(load_outdoor_airflow_ahu());

    assert_eq!(
        schedule_a, schedule_b,
        "OutdoorAirFlow AHU schedule is deterministic"
    );
    assert_trace_bit_eq(&metrics_a, &metrics_b);
    assert_real_bits(
        &real_column(&metrics_a, UNCORRECTED_OUTDOOR_AIR),
        &expected_column(0),
        "uncorrected outdoor airflow",
    );
    assert_real_bits(
        &real_column(&metrics_a, EFFECTIVE_MIN_OUTDOOR_AIR),
        &expected_column(1),
        "effective minimum outdoor airflow",
    );
    assert_real_bits(
        &real_column(&metrics_a, EFFECTIVE_NORMALIZED),
        &expected_column(2),
        "effective normalized outdoor airflow",
    );
    assert_real_bits(
        &real_column(&metrics_a, MEASURED_NORMALIZED),
        &expected_column(3),
        "measured normalized outdoor airflow",
    );
}
