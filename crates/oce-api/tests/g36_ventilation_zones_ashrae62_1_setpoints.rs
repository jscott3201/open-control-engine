//! Source-verified ASHRAE G36 VentilationZones ASHRAE 62.1 Setpoints through the facade.
//!
//! The replay consumes the independent 60-row Tier-A trap schedule and compares all four Real
//! outputs bit-exactly. Two fresh engines must expose the same schedule and identical traces.

use std::collections::HashMap;
use std::sync::Arc;

use oce_api::{CollectSpec, Engine, InputSource, SimMetrics, SimSpec, Value};

const SETPOINTS: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/ventilation_zones_ashrae62_1_setpoints.jsonld");
const REFERENCE_CSV: &str = include_str!(
    "../../../tools/golden-gen/goldens/G36/ventilation_zones_ashrae62_1_setpoints/reference.csv"
);

const WINDOW_STATUS: &str =
    "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.u1Win";
const OCCUPANCY_STATUS: &str =
    "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.u1Occ";
const OPERATING_MODE: &str =
    "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.uOpeMod";
const CO2_SETPOINT: &str =
    "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.ppmCO2Set";
const CO2_CONCENTRATION: &str =
    "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.ppmCO2";
const ZONE_TEMPERATURE: &str =
    "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.TZon";
const DISCHARGE_AIR_TEMPERATURE: &str =
    "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.TDis";

const ADJUSTED_POPULATION_FLOW: &str = "conn#44";
const OCCUPIED_MINIMUM_FLOW: &str = "conn#52";
const ADJUSTED_AREA_FLOW: &str = "conn#48";
const MINIMUM_OUTDOOR_AIRFLOW: &str = "conn#80";

const T_STOP: f64 = 3_540.0;
const SAMPLE_STEP: f64 = 60.0;
const ROWS: usize = 60;

type ScheduleSignature = (Vec<u32>, Vec<u32>, Vec<u32>);

const OUTPUTS: &[OutputPoint] = &[
    OutputPoint {
        reference_name: "adjusted_population_flow",
        runtime_name: ADJUSTED_POPULATION_FLOW,
    },
    OutputPoint {
        reference_name: "occupied_minimum_flow",
        runtime_name: OCCUPIED_MINIMUM_FLOW,
    },
    OutputPoint {
        reference_name: "adjusted_area_flow",
        runtime_name: ADJUSTED_AREA_FLOW,
    },
    OutputPoint {
        reference_name: "minimum_outdoor_airflow",
        runtime_name: MINIMUM_OUTDOOR_AIRFLOW,
    },
];

#[derive(Clone, Copy)]
struct OutputPoint {
    reference_name: &'static str,
    runtime_name: &'static str,
}

#[derive(Clone)]
struct ReferenceTable {
    columns: Vec<String>,
    column_by_name: HashMap<String, usize>,
    rows: Vec<Vec<f64>>,
}

impl ReferenceTable {
    fn parse(text: &str) -> Self {
        let columns = text
            .lines()
            .find_map(|line| line.strip_prefix("# columns: "))
            .expect("reference CSV should declare columns")
            .split_whitespace()
            .map(str::to_string)
            .collect::<Vec<_>>();
        let column_by_name = columns
            .iter()
            .enumerate()
            .map(|(index, name)| (name.clone(), index))
            .collect::<HashMap<_, _>>();
        assert_eq!(
            columns.len(),
            column_by_name.len(),
            "VentilationZones ASHRAE 62.1 Setpoints reference columns must be unique"
        );
        let rows = text
            .lines()
            .filter(|line| {
                !line.is_empty() && !line.starts_with('#') && !line.starts_with("double ")
            })
            .map(|line| {
                line.split_whitespace()
                    .map(|cell| {
                        cell.parse::<f64>()
                            .unwrap_or_else(|err| panic!("bad reference cell {cell:?}: {err}"))
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), ROWS);
        for row in &rows {
            assert_eq!(row.len(), columns.len());
        }
        Self {
            columns,
            column_by_name,
            rows,
        }
    }

    fn col(&self, name: &str) -> usize {
        *self
            .column_by_name
            .get(name)
            .unwrap_or_else(|| panic!("missing reference column {name:?}"))
    }

    fn value(&self, row: usize, name: &str) -> f64 {
        self.rows[row][self.col(name)]
    }

    fn row_at_time(&self, t: f64) -> usize {
        let time_col = self.col("time");
        self.rows
            .iter()
            .position(|row| row[time_col].to_bits() == t.to_bits())
            .unwrap_or_else(|| panic!("missing reference row for t={t:?}"))
    }

    fn times(&self) -> Vec<f64> {
        let time_col = self.col("time");
        self.rows.iter().map(|row| row[time_col]).collect()
    }
}

fn reference_inputs(reference: &ReferenceTable, t: f64) -> Vec<(String, Value)> {
    let row = reference.row_at_time(t);
    vec![
        (
            WINDOW_STATUS.to_string(),
            Value::Boolean(reference.value(row, "window_status") != 0.0),
        ),
        (
            OCCUPANCY_STATUS.to_string(),
            Value::Boolean(reference.value(row, "occupancy_status") != 0.0),
        ),
        (
            OPERATING_MODE.to_string(),
            Value::Integer(reference.value(row, "operating_mode") as i64),
        ),
        (
            CO2_SETPOINT.to_string(),
            Value::Real(reference.value(row, "co2_setpoint")),
        ),
        (
            CO2_CONCENTRATION.to_string(),
            Value::Real(reference.value(row, "co2_concentration")),
        ),
        (
            ZONE_TEMPERATURE.to_string(),
            Value::Real(reference.value(row, "zone_temperature")),
        ),
        (
            DISCHARGE_AIR_TEMPERATURE.to_string(),
            Value::Real(reference.value(row, "discharge_air_temperature")),
        ),
    ]
}

fn load_setpoints() -> Engine {
    let mut engine = Engine::in_memory();
    let report = engine
        .load_cxf(SETPOINTS.as_bytes())
        .expect("source-verified G36 VentilationZones ASHRAE 62.1 Setpoints fixture loads");
    assert_eq!(report.block_count, 34);
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

fn simulate(mut engine: Engine, reference: Arc<ReferenceTable>) -> (ScheduleSignature, SimMetrics) {
    let schedule = schedule_signature(&engine);
    let input_reference = Arc::clone(&reference);
    let metrics = engine
        .simulate(&SimSpec {
            t_start: 0.0,
            t_stop: T_STOP,
            step: SAMPLE_STEP,
            inputs: InputSource::Closure(Box::new(move |t| reference_inputs(&input_reference, t))),
            collect: CollectSpec::Named {
                points: OUTPUTS
                    .iter()
                    .map(|output| output.runtime_name.to_string())
                    .collect(),
                stride: 1,
            },
        })
        .expect("G36 VentilationZones ASHRAE 62.1 Setpoints simulates");
    assert_eq!(metrics.ticks, ROWS as u64);
    assert_eq!(
        metrics
            .trace
            .times()
            .iter()
            .map(|time| time.to_bits())
            .collect::<Vec<_>>(),
        reference
            .times()
            .iter()
            .map(|time| time.to_bits())
            .collect::<Vec<_>>()
    );
    (schedule, metrics)
}

fn assert_output_matches_reference(
    metrics: &SimMetrics,
    reference: &ReferenceTable,
    output: OutputPoint,
) {
    let index = metrics
        .trace
        .columns()
        .iter()
        .position(|column| column == output.runtime_name)
        .unwrap_or_else(|| panic!("missing trace column {}", output.runtime_name));
    let column = metrics.trace.column(index).expect("column index is valid");
    assert_eq!(column.len(), ROWS);
    for (row, actual) in column.iter().enumerate() {
        let expected = Value::Real(reference.value(row, output.reference_name));
        assert!(
            actual.bit_eq(&expected),
            "{} row {row} diverged: {actual:?} vs {expected:?}",
            output.reference_name
        );
    }
}

fn assert_trace_bit_eq(left: &SimMetrics, right: &SimMetrics) {
    assert_eq!(left.trace.columns(), right.trace.columns());
    assert_eq!(
        left.trace
            .times()
            .iter()
            .map(|time| time.to_bits())
            .collect::<Vec<_>>(),
        right
            .trace
            .times()
            .iter()
            .map(|time| time.to_bits())
            .collect::<Vec<_>>()
    );
    for column in 0..left.trace.columns().len() {
        let left_values = left.trace.column(column).expect("left trace column");
        let right_values = right.trace.column(column).expect("right trace column");
        for (row, (left_value, right_value)) in left_values.iter().zip(right_values).enumerate() {
            assert!(
                left_value.bit_eq(right_value),
                "{} row {row} diverged: {left_value:?} vs {right_value:?}",
                left.trace.columns()[column]
            );
        }
    }
}

#[test]
fn ventilation_setpoints_match_all_oracle_outputs_and_repeat_bit_exactly() {
    let reference = Arc::new(ReferenceTable::parse(REFERENCE_CSV));
    assert_eq!(reference.columns.len(), 12);

    let engine = load_setpoints();
    let paths = engine
        .io()
        .iter()
        .map(|point| point.path.clone())
        .collect::<Vec<_>>();
    for path in [
        WINDOW_STATUS,
        OCCUPANCY_STATUS,
        OPERATING_MODE,
        CO2_SETPOINT,
        CO2_CONCENTRATION,
        ZONE_TEMPERATURE,
        DISCHARGE_AIR_TEMPERATURE,
        ADJUSTED_POPULATION_FLOW,
        OCCUPIED_MINIMUM_FLOW,
        ADJUSTED_AREA_FLOW,
        MINIMUM_OUTDOOR_AIRFLOW,
    ] {
        assert!(
            paths.contains(&path.to_string()),
            "missing facade path {path}"
        );
    }

    let (schedule_a, metrics_a) = simulate(engine, Arc::clone(&reference));
    let (schedule_b, metrics_b) = simulate(load_setpoints(), Arc::clone(&reference));
    assert_eq!(
        schedule_a, schedule_b,
        "VentilationZones ASHRAE 62.1 Setpoints schedule is deterministic"
    );
    assert_trace_bit_eq(&metrics_a, &metrics_b);
    for output in OUTPUTS {
        assert_output_matches_reference(&metrics_a, &reference, *output);
    }
}
