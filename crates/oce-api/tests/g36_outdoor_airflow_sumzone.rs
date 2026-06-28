//! Source-verified ASHRAE G36 OutdoorAirFlow ASHRAE 62.1 SumZone through the frozen facade.

use oce_api::{CollectSpec, Engine, InputSource, PointDirection, SimMetrics, SimSpec, Value};

const OUTDOOR_AIRFLOW_SUMZONE: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/multizone_vav_outdoor_airflow_sumzone.jsonld");

const U_OPE_MOD_1: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_sumzone.uOpeMod_1";
const U_OPE_MOD_2: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_sumzone.uOpeMod_2";
const POP_1: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_sumzone.VAdjPopBreZon_flow_1";
const POP_2: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_sumzone.VAdjPopBreZon_flow_2";
const POP_3: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_sumzone.VAdjPopBreZon_flow_3";
const AREA_1: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_sumzone.VAdjAreBreZon_flow_1";
const AREA_2: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_sumzone.VAdjAreBreZon_flow_2";
const AREA_3: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_sumzone.VAdjAreBreZon_flow_3";
const PRI_1: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_sumzone.VZonPri_flow_1";
const PRI_2: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_sumzone.VZonPri_flow_2";
const PRI_3: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_sumzone.VZonPri_flow_3";
const MIN_OA_1: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_sumzone.VMinOA_flow_1";
const MIN_OA_2: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_sumzone.VMinOA_flow_2";
const MIN_OA_3: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_sumzone.VMinOA_flow_3";

const SUMMED_POP_FLOW: &str = "conn#52";
const SUMMED_AREA_FLOW: &str = "conn#55";
const SUMMED_PRIMARY_FLOW: &str = "conn#58";
const MAX_OUTDOOR_AIR_FRACTION: &str = "conn#62";

const EXPECTED_TIMES: [f64; 6] = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0];

type ScheduleSignature = (Vec<u32>, Vec<u32>, Vec<u32>);

#[derive(Clone, Copy)]
struct Row {
    operation_modes: [i64; 2],
    population: [f64; 3],
    area: [f64; 3],
    primary: [f64; 3],
    minimum_outdoor_air: [f64; 3],
}

fn pair(path: &str, value: Value) -> (String, Value) {
    (path.to_string(), value)
}

fn input_row(t: f64) -> Row {
    match t as u32 {
        0 => Row {
            operation_modes: [1, 1],
            population: [1.0, 2.0, 3.0],
            area: [4.0, 5.0, 6.0],
            primary: [10.0, 20.0, 30.0],
            minimum_outdoor_air: [1.0, 2.0, 3.0],
        },
        1 => Row {
            operation_modes: [1, 7],
            population: [2.0, 4.0, 8.0],
            area: [1.0, 3.0, 5.0],
            primary: [2.0, 4.0, 8.0],
            minimum_outdoor_air: [1.0, 2.0, 4.0],
        },
        2 => Row {
            operation_modes: [4, 1],
            population: [2.5, 0.5, 1.5],
            area: [6.0, 2.0, 1.0],
            primary: [5.0, 0.5, 1.5],
            minimum_outdoor_air: [10.0, 0.1, 2.0],
        },
        3 => Row {
            operation_modes: [7, 6],
            population: [10.0, 20.0, 30.0],
            area: [3.0, 2.0, 1.0],
            primary: [0.0, 0.0, 0.0],
            minimum_outdoor_air: [1.0, 1.0, 1.0],
        },
        4 => Row {
            operation_modes: [1, 1],
            population: [0.0, 0.0, 0.0],
            area: [0.0, 1.0, 0.0],
            primary: [0.0, 0.00005, 0.5],
            minimum_outdoor_air: [1.0, 1.0, 0.1],
        },
        _ => Row {
            operation_modes: [1, 1],
            population: [1.25, 2.5, 5.0],
            area: [8.0, 13.0, 21.0],
            primary: [1.0, 1.0, 1.0],
            minimum_outdoor_air: [0.2, 0.8, 0.3],
        },
    }
}

fn sumzone_inputs(t: f64) -> Vec<(String, Value)> {
    let row = input_row(t);
    vec![
        pair(U_OPE_MOD_1, Value::Integer(row.operation_modes[0])),
        pair(U_OPE_MOD_2, Value::Integer(row.operation_modes[1])),
        pair(POP_1, Value::Real(row.population[0])),
        pair(POP_2, Value::Real(row.population[1])),
        pair(POP_3, Value::Real(row.population[2])),
        pair(AREA_1, Value::Real(row.area[0])),
        pair(AREA_2, Value::Real(row.area[1])),
        pair(AREA_3, Value::Real(row.area[2])),
        pair(PRI_1, Value::Real(row.primary[0])),
        pair(PRI_2, Value::Real(row.primary[1])),
        pair(PRI_3, Value::Real(row.primary[2])),
        pair(MIN_OA_1, Value::Real(row.minimum_outdoor_air[0])),
        pair(MIN_OA_2, Value::Real(row.minimum_outdoor_air[1])),
        pair(MIN_OA_3, Value::Real(row.minimum_outdoor_air[2])),
    ]
}

fn expected_outputs(t: f64) -> (f64, f64, f64, f64) {
    let row = input_row(t);
    let occupied = [row.operation_modes[0] == 1, row.operation_modes[1] == 1];
    let population_group = [
        row.population[0] + row.population[1],
        row.population[1] + row.population[2],
    ];
    let area_group = [row.area[0] + row.area[1], row.area[1] + row.area[2]];
    let primary_group = [
        row.primary[0] + row.primary[1],
        row.primary[1] + row.primary[2],
    ];
    let zone_fraction = [
        outdoor_air_fraction(row.primary[0], row.minimum_outdoor_air[0]),
        outdoor_air_fraction(row.primary[1], row.minimum_outdoor_air[1]),
        outdoor_air_fraction(row.primary[2], row.minimum_outdoor_air[2]),
    ];
    let occupied_zone_membership = [
        if occupied[0] { 1.0 } else { 0.0 },
        if occupied[0] { 1.0 } else { 0.0 } + if occupied[1] { 1.0 } else { 0.0 },
        if occupied[1] { 1.0 } else { 0.0 },
    ];
    let max_fraction = (occupied_zone_membership[0] * zone_fraction[0])
        .max(occupied_zone_membership[1] * zone_fraction[1])
        .max(occupied_zone_membership[2] * zone_fraction[2]);
    (
        gate(occupied[0], population_group[0]) + gate(occupied[1], population_group[1]),
        gate(occupied[0], area_group[0]) + gate(occupied[1], area_group[1]),
        gate(occupied[0], primary_group[0]) + gate(occupied[1], primary_group[1]),
        max_fraction,
    )
}

fn outdoor_air_fraction(primary: f64, minimum_outdoor_air: f64) -> f64 {
    primary.min(minimum_outdoor_air) / primary.max(1e-4)
}

fn gate(enabled: bool, value: f64) -> f64 {
    if enabled { value } else { 0.0 }
}

fn load_outdoor_airflow_sumzone() -> Engine {
    let mut engine = Engine::in_memory();
    let report = engine
        .load_cxf(OUTDOOR_AIRFLOW_SUMZONE.as_bytes())
        .expect("source-verified G36 OutdoorAirFlow ASHRAE62_1 SumZone fixture loads");
    assert_eq!(report.block_count, 35);
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
                    SUMMED_POP_FLOW.to_string(),
                    SUMMED_AREA_FLOW.to_string(),
                    SUMMED_PRIMARY_FLOW.to_string(),
                    MAX_OUTDOOR_AIR_FRACTION.to_string(),
                ],
                stride: 1,
            },
        })
        .expect("G36 OutdoorAirFlow ASHRAE62_1 SumZone simulates");
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
fn multizone_vav_outdoor_airflow_sumzone_loads_simulates_and_is_deterministic() {
    let engine = load_outdoor_airflow_sumzone();
    let paths = engine
        .io()
        .iter()
        .map(|point| point.path.clone())
        .collect::<Vec<_>>();
    for input in [
        U_OPE_MOD_1,
        U_OPE_MOD_2,
        POP_1,
        POP_2,
        POP_3,
        AREA_1,
        AREA_2,
        AREA_3,
        PRI_1,
        PRI_2,
        PRI_3,
        MIN_OA_1,
        MIN_OA_2,
        MIN_OA_3,
    ] {
        assert!(
            paths.contains(&input.to_string()),
            "missing facade input {input}"
        );
    }
    for output in [
        SUMMED_POP_FLOW,
        SUMMED_AREA_FLOW,
        SUMMED_PRIMARY_FLOW,
        MAX_OUTDOOR_AIR_FRACTION,
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
        output_count, 40,
        "each active source block output should remain inspectable"
    );

    let (schedule_a, metrics_a) = simulate(engine);
    let (schedule_b, metrics_b) = simulate(load_outdoor_airflow_sumzone());

    assert_eq!(
        schedule_a, schedule_b,
        "OutdoorAirFlow ASHRAE62_1 SumZone schedule is deterministic"
    );
    assert_trace_bit_eq(&metrics_a, &metrics_b);
    assert_real_bits(
        &real_column(&metrics_a, SUMMED_POP_FLOW),
        &expected_column(0),
        "summed adjusted population breathing zone flow",
    );
    assert_real_bits(
        &real_column(&metrics_a, SUMMED_AREA_FLOW),
        &expected_column(1),
        "summed adjusted area breathing zone flow",
    );
    assert_real_bits(
        &real_column(&metrics_a, SUMMED_PRIMARY_FLOW),
        &expected_column(2),
        "summed zone primary airflow",
    );
    assert_real_bits(
        &real_column(&metrics_a, MAX_OUTDOOR_AIR_FRACTION),
        &expected_column(3),
        "maximum zone outdoor air fraction",
    );
}
