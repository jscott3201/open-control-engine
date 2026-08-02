//! Source-verified ASHRAE G36 OutdoorAirFlow Title 24 SumZone through the frozen facade.

use oce_api::{CollectSpec, Engine, InputSource, PointDirection, SimMetrics, SimSpec, Value};

const OUTDOOR_AIRFLOW_TITLE24_SUMZONE: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/multizone_vav_outdoor_airflow_title24_sumzone.jsonld"
);

const U_OPE_MOD_1: &str = "conn#34";
const U_OPE_MOD_2: &str = "conn#37";
const ABS_MIN_1: &str = "conn#0";
const ABS_MIN_2: &str = "conn#1";
const ABS_MIN_3: &str = "conn#2";
const DES_MIN_1: &str = "conn#5";
const DES_MIN_2: &str = "conn#6";
const DES_MIN_3: &str = "conn#7";
const CO2_1: &str = "conn#40";
const CO2_2: &str = "conn#41";
const CO2_3: &str = "conn#42";

const SUMMED_ABSOLUTE_MIN_FLOW: &str = "conn#28";
const SUMMED_DESIGN_MIN_FLOW: &str = "conn#31";
const MAX_CO2: &str = "conn#43";

const EXPECTED_TIMES: [f64; 6] = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0];

type ScheduleSignature = (Vec<u32>, Vec<u32>, Vec<u32>);

#[derive(Clone, Copy)]
struct Row {
    operation_modes: [i64; 2],
    absolute_minimums: [f64; 3],
    design_minimums: [f64; 3],
    co2: [f64; 3],
}

fn pair(path: &str, value: Value) -> (String, Value) {
    (path.to_string(), value)
}

fn input_row(t: f64) -> Row {
    match t as u32 {
        0 => Row {
            operation_modes: [1, 1],
            absolute_minimums: [1.0, 2.0, 3.0],
            design_minimums: [4.0, 5.0, 6.0],
            co2: [0.1, 0.6, 0.2],
        },
        1 => Row {
            operation_modes: [1, 7],
            absolute_minimums: [2.0, 4.0, 8.0],
            design_minimums: [1.0, 3.0, 5.0],
            co2: [-0.5, 0.0, 0.2],
        },
        2 => Row {
            operation_modes: [4, 1],
            absolute_minimums: [2.5, 0.5, 1.5],
            design_minimums: [6.0, 2.0, 1.0],
            co2: [0.9, 0.3, 0.7],
        },
        3 => Row {
            operation_modes: [7, 6],
            absolute_minimums: [10.0, 20.0, 30.0],
            design_minimums: [3.0, 2.0, 1.0],
            co2: [0.0, 0.0, 0.0],
        },
        4 => Row {
            operation_modes: [1, 1],
            absolute_minimums: [0.0, 0.0, 0.0],
            design_minimums: [0.0, 1.0, 0.0],
            co2: [-1.0, -2.0, -3.0],
        },
        _ => Row {
            operation_modes: [3, 1],
            absolute_minimums: [1.25, 2.5, 5.0],
            design_minimums: [8.0, 13.0, 21.0],
            co2: [1.2, 1.2, 1.1],
        },
    }
}

fn sumzone_inputs(t: f64) -> Vec<(String, Value)> {
    let row = input_row(t);
    vec![
        pair(U_OPE_MOD_1, Value::Integer(row.operation_modes[0])),
        pair(U_OPE_MOD_2, Value::Integer(row.operation_modes[1])),
        pair(ABS_MIN_1, Value::Real(row.absolute_minimums[0])),
        pair(ABS_MIN_2, Value::Real(row.absolute_minimums[1])),
        pair(ABS_MIN_3, Value::Real(row.absolute_minimums[2])),
        pair(DES_MIN_1, Value::Real(row.design_minimums[0])),
        pair(DES_MIN_2, Value::Real(row.design_minimums[1])),
        pair(DES_MIN_3, Value::Real(row.design_minimums[2])),
        pair(CO2_1, Value::Real(row.co2[0])),
        pair(CO2_2, Value::Real(row.co2[1])),
        pair(CO2_3, Value::Real(row.co2[2])),
    ]
}

fn expected_outputs(t: f64) -> (f64, f64, f64) {
    let row = input_row(t);
    let occupied = [row.operation_modes[0] == 1, row.operation_modes[1] == 1];
    let absolute_group = [
        row.absolute_minimums[0] + row.absolute_minimums[1],
        row.absolute_minimums[1] + row.absolute_minimums[2],
    ];
    let design_group = [
        row.design_minimums[0] + row.design_minimums[1],
        row.design_minimums[1] + row.design_minimums[2],
    ];
    let summed_absolute =
        gate(occupied[0], absolute_group[0]) + gate(occupied[1], absolute_group[1]);
    let summed_design = gate(occupied[0], design_group[0]) + gate(occupied[1], design_group[1]);
    let max_co2 = row.co2[0].max(row.co2[1]).max(row.co2[2]);
    (summed_absolute, summed_design, max_co2)
}

fn gate(enabled: bool, value: f64) -> f64 {
    if enabled { value } else { 0.0 }
}

fn load_outdoor_airflow_title24_sumzone() -> Engine {
    let mut engine = Engine::in_memory();
    let report = engine
        .load_cxf(OUTDOOR_AIRFLOW_TITLE24_SUMZONE.as_bytes())
        .expect("source-verified G36 OutdoorAirFlow Title24 SumZone fixture loads");
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
            t_stop: 5.0,
            step: 1.0,
            inputs: InputSource::Closure(Box::new(sumzone_inputs)),
            collect: CollectSpec::Named {
                points: vec![
                    SUMMED_ABSOLUTE_MIN_FLOW.to_string(),
                    SUMMED_DESIGN_MIN_FLOW.to_string(),
                    MAX_CO2.to_string(),
                ],
                stride: 1,
            },
        })
        .expect("G36 OutdoorAirFlow Title24 SumZone simulates");
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
            _ => expected_outputs(t).2,
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
fn multizone_vav_outdoor_airflow_title24_sumzone_loads_simulates_and_is_deterministic() {
    let engine = load_outdoor_airflow_title24_sumzone();
    let paths = engine
        .io()
        .iter()
        .map(|point| point.path.clone())
        .collect::<Vec<_>>();
    for input in [
        U_OPE_MOD_1,
        U_OPE_MOD_2,
        ABS_MIN_1,
        ABS_MIN_2,
        ABS_MIN_3,
        DES_MIN_1,
        DES_MIN_2,
        DES_MIN_3,
        CO2_1,
        CO2_2,
        CO2_3,
    ] {
        assert!(
            paths.contains(&input.to_string()),
            "missing facade input {input}"
        );
    }
    for output in [SUMMED_ABSOLUTE_MIN_FLOW, SUMMED_DESIGN_MIN_FLOW, MAX_CO2] {
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
        output_count, 17,
        "each active source block output should remain inspectable"
    );

    let (schedule_a, metrics_a) = simulate(engine);
    let (schedule_b, metrics_b) = simulate(load_outdoor_airflow_title24_sumzone());

    assert_eq!(
        schedule_a, schedule_b,
        "OutdoorAirFlow Title24 SumZone schedule is deterministic"
    );
    assert_trace_bit_eq(&metrics_a, &metrics_b);
    assert_real_bits(
        &real_column(&metrics_a, SUMMED_ABSOLUTE_MIN_FLOW),
        &expected_column(0),
        "summed absolute minimum outdoor airflow",
    );
    assert_real_bits(
        &real_column(&metrics_a, SUMMED_DESIGN_MIN_FLOW),
        &expected_column(1),
        "summed design minimum outdoor airflow",
    );
    assert_real_bits(
        &real_column(&metrics_a, MAX_CO2),
        &expected_column(2),
        "maximum CO2 loop signal",
    );
}
