//! Source-verified ASHRAE G36 OutdoorAirFlow Title 24 AHU through the frozen facade.

use oce_api::{CollectSpec, Engine, InputSource, PointDirection, SimMetrics, SimSpec, Value};

const OUTDOOR_AIRFLOW_TITLE24_AHU: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/multizone_vav_outdoor_airflow_title24_ahu.jsonld"
);

const ABSOLUTE_MIN_FLOW: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_ahu.VSumZonAbsMin_flow";
const DESIGN_MIN_FLOW: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_ahu.VSumZonDesMin_flow";
const CO2_LOOP_MAX: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_ahu.uCO2Loo_max";
const MEASURED_OUTDOOR_AIR: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_ahu.VAirOut_flow";
const EFFECTIVE_ABSOLUTE_OUTDOOR_AIR: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_ahu.min1.y";
const EFFECTIVE_ABSOLUTE_NORMALIZED: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_ahu.norVOutMin1.y";
const EFFECTIVE_DESIGN_OUTDOOR_AIR: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_ahu.min2.y";
const EFFECTIVE_DESIGN_NORMALIZED: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_ahu.norVOutMin.y";
const EFFECTIVE_OUTDOOR_AIR_NORMALIZED: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_ahu.norVOutMin2.y";
const MEASURED_NORMALIZED: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_title24_ahu.norVOut.y";

const EXPECTED_TIMES: [f64; 6] = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0];

type ScheduleSignature = (Vec<u32>, Vec<u32>, Vec<u32>);

fn pair(path: &str, value: Value) -> (String, Value) {
    (path.to_string(), value)
}

fn input_row(t: f64) -> (f64, f64, f64, f64) {
    match t as u32 {
        0 => (1.0, 2.0, 0.0, 4.0),
        1 => (4.0, 10.0, 0.5, 6.0),
        2 => (4.0, 10.0, 0.75, 0.0),
        3 => (2.5, 4.0, 1.4, 9.0),
        4 => (0.0, 0.0, 1.0, 8.0),
        _ => (1.0, 5.0, -0.25, 1.6),
    }
}

fn outdoor_airflow_inputs(t: f64) -> Vec<(String, Value)> {
    let (absolute, design, co2, measured) = input_row(t);
    vec![
        pair(ABSOLUTE_MIN_FLOW, Value::Real(absolute)),
        pair(DESIGN_MIN_FLOW, Value::Real(design)),
        pair(CO2_LOOP_MAX, Value::Real(co2)),
        pair(MEASURED_OUTDOOR_AIR, Value::Real(measured)),
    ]
}

fn expected_outputs(t: f64) -> (f64, f64, f64, f64, f64, f64) {
    const V_ABS_OUT_AIR_FLOW: f64 = 3.0;
    const V_DES_OUT_AIR_FLOW: f64 = 8.0;
    const NEAR_ZERO: f64 = 1E-4;

    let (absolute, design, co2, measured) = input_row(t);
    let effective_absolute = V_ABS_OUT_AIR_FLOW.min(absolute);
    let effective_design = V_DES_OUT_AIR_FLOW.min(design);
    let guarded_absolute = V_ABS_OUT_AIR_FLOW.max(NEAR_ZERO);
    let guarded_design = V_DES_OUT_AIR_FLOW.max(NEAR_ZERO);
    let effective_outdoor_air = buildings_line(
        0.5,
        effective_absolute,
        1.0,
        effective_design,
        co2,
        false,
        true,
    );
    (
        effective_absolute,
        effective_absolute / guarded_absolute,
        effective_design,
        effective_design / guarded_design,
        effective_outdoor_air / guarded_design,
        measured / guarded_design,
    )
}

fn buildings_line(
    x1: f64,
    f1: f64,
    x2: f64,
    f2: f64,
    u: f64,
    limit_below: bool,
    limit_above: bool,
) -> f64 {
    let x_lim = match (limit_below, limit_above) {
        (true, true) => u.max(x1).min(x2),
        (true, false) => u.max(x1),
        (false, true) => u.min(x2),
        (false, false) => u,
    };
    let slope = (f2 - f1) / (x2 - x1);
    let intercept = f2 - slope * x2;
    intercept + slope * x_lim
}

fn load_outdoor_airflow_title24_ahu() -> Engine {
    let mut engine = Engine::in_memory();
    let report = engine
        .load_cxf(OUTDOOR_AIRFLOW_TITLE24_AHU.as_bytes())
        .expect("source-verified G36 OutdoorAirFlow Title24 AHU fixture loads");
    assert_eq!(report.block_count, 14);
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
            t_stop: 5.0,
            step: 1.0,
            inputs: InputSource::Closure(Box::new(outdoor_airflow_inputs)),
            collect: CollectSpec::Named {
                points: vec![
                    EFFECTIVE_ABSOLUTE_OUTDOOR_AIR.to_string(),
                    EFFECTIVE_ABSOLUTE_NORMALIZED.to_string(),
                    EFFECTIVE_DESIGN_OUTDOOR_AIR.to_string(),
                    EFFECTIVE_DESIGN_NORMALIZED.to_string(),
                    EFFECTIVE_OUTDOOR_AIR_NORMALIZED.to_string(),
                    MEASURED_NORMALIZED.to_string(),
                ],
                stride: 1,
            },
        })
        .expect("G36 OutdoorAirFlow Title24 AHU simulates");
    assert_eq!(metrics.ticks, 6);
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
            3 => expected_outputs(t).3,
            4 => expected_outputs(t).4,
            _ => expected_outputs(t).5,
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
fn multizone_vav_outdoor_airflow_title24_ahu_loads_simulates_and_is_deterministic() {
    let engine = load_outdoor_airflow_title24_ahu();
    let paths = engine
        .io()
        .iter()
        .map(|point| point.path.clone())
        .collect::<Vec<_>>();
    for input in [
        ABSOLUTE_MIN_FLOW,
        DESIGN_MIN_FLOW,
        CO2_LOOP_MAX,
        MEASURED_OUTDOOR_AIR,
    ] {
        assert!(
            paths.contains(&input.to_string()),
            "missing facade input {input}"
        );
    }
    for output in [
        EFFECTIVE_ABSOLUTE_OUTDOOR_AIR,
        EFFECTIVE_ABSOLUTE_NORMALIZED,
        EFFECTIVE_DESIGN_OUTDOOR_AIR,
        EFFECTIVE_DESIGN_NORMALIZED,
        EFFECTIVE_OUTDOOR_AIR_NORMALIZED,
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
        output_count, 14,
        "each active source block should expose one output"
    );

    let (schedule_a, metrics_a) = simulate(engine);
    let (schedule_b, metrics_b) = simulate(load_outdoor_airflow_title24_ahu());

    assert_eq!(
        schedule_a, schedule_b,
        "OutdoorAirFlow Title24 AHU schedule is deterministic"
    );
    assert_trace_bit_eq(&metrics_a, &metrics_b);
    assert_real_bits(
        &real_column(&metrics_a, EFFECTIVE_ABSOLUTE_OUTDOOR_AIR),
        &expected_column(0),
        "effective absolute outdoor airflow",
    );
    assert_real_bits(
        &real_column(&metrics_a, EFFECTIVE_ABSOLUTE_NORMALIZED),
        &expected_column(1),
        "effective absolute normalized outdoor airflow",
    );
    assert_real_bits(
        &real_column(&metrics_a, EFFECTIVE_DESIGN_OUTDOOR_AIR),
        &expected_column(2),
        "effective design outdoor airflow",
    );
    assert_real_bits(
        &real_column(&metrics_a, EFFECTIVE_DESIGN_NORMALIZED),
        &expected_column(3),
        "effective design normalized outdoor airflow",
    );
    assert_real_bits(
        &real_column(&metrics_a, EFFECTIVE_OUTDOOR_AIR_NORMALIZED),
        &expected_column(4),
        "effective outdoor airflow normalized by design flow",
    );
    assert_real_bits(
        &real_column(&metrics_a, MEASURED_NORMALIZED),
        &expected_column(5),
        "measured outdoor airflow normalized by design flow",
    );
}
