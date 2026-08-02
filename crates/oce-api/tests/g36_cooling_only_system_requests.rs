//! Source-verified ASHRAE G36 CoolingOnly SystemRequests through the frozen facade.

use std::collections::HashMap;
use std::sync::Arc;

use oce_api::{CollectSpec, Engine, InputSource, SimMetrics, SimSpec, Value};

const SYSTEM_REQUESTS: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/cooling_only_system_requests.jsonld");
const REFERENCE_CSV: &str = include_str!(
    "../../../tools/golden-gen/goldens/G36/cooling_only_system_requests/reference.csv"
);

const AFTER_SUPPRESSION: &str = "conn#24";
const COOLING_SETPOINT: &str = "conn#19";
const ZONE_TEMPERATURE: &str = "conn#18";
const COOLING_LOOP: &str = "conn#71";
const AIRFLOW_SETPOINT: &str = "conn#77";
const DISCHARGE_AIRFLOW: &str = "conn#75";
const DAMPER_POSITION: &str = "conn#73";

const ZONE_TEMPERATURE_REQUEST: &str = "conn#43";
const ZONE_PRESSURE_REQUEST: &str = "conn#51";

const T_STOP: f64 = 1_800.0;
const SAMPLE_STEP: f64 = 60.0;
const ROWS: usize = 31;

type ScheduleSignature = (Vec<u32>, Vec<u32>, Vec<u32>);

const OUTPUTS: &[OutputPoint] = &[
    OutputPoint {
        reference_name: "zone_temperature_reset_request",
        runtime_name: ZONE_TEMPERATURE_REQUEST,
    },
    OutputPoint {
        reference_name: "zone_pressure_reset_request",
        runtime_name: ZONE_PRESSURE_REQUEST,
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
            "CoolingOnly SystemRequests reference columns must be unique"
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
            AFTER_SUPPRESSION.to_string(),
            Value::Boolean(reference.value(row, "after_suppression") != 0.0),
        ),
        (
            COOLING_SETPOINT.to_string(),
            Value::Real(reference.value(row, "cooling_setpoint")),
        ),
        (
            ZONE_TEMPERATURE.to_string(),
            Value::Real(reference.value(row, "zone_temperature")),
        ),
        (
            COOLING_LOOP.to_string(),
            Value::Real(reference.value(row, "cooling_loop")),
        ),
        (
            AIRFLOW_SETPOINT.to_string(),
            Value::Real(reference.value(row, "airflow_setpoint")),
        ),
        (
            DISCHARGE_AIRFLOW.to_string(),
            Value::Real(reference.value(row, "discharge_airflow")),
        ),
        (
            DAMPER_POSITION.to_string(),
            Value::Real(reference.value(row, "damper_position")),
        ),
    ]
}

fn load_system_requests() -> Engine {
    let mut engine = Engine::in_memory();
    let report = engine
        .load_cxf(SYSTEM_REQUESTS.as_bytes())
        .expect("source-verified G36 CoolingOnly SystemRequests fixture loads");
    assert_eq!(report.block_count, 33);
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
                points: vec![
                    ZONE_TEMPERATURE_REQUEST.to_string(),
                    ZONE_PRESSURE_REQUEST.to_string(),
                ],
                stride: 1,
            },
        })
        .expect("G36 CoolingOnly SystemRequests simulates");
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
        let expected = reference.value(row, output.reference_name) as i64;
        match actual {
            Value::Integer(value) => assert_eq!(
                *value, expected,
                "{} row {row} diverged",
                output.reference_name
            ),
            other => panic!(
                "{} row {row} had wrong runtime kind: {other:?}",
                output.reference_name
            ),
        }
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
        let left_values = left.trace.column(column).unwrap();
        let right_values = right.trace.column(column).unwrap();
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
fn cooling_only_system_requests_loads_simulates_and_is_deterministic() {
    let reference = Arc::new(ReferenceTable::parse(REFERENCE_CSV));
    assert_eq!(reference.columns.len(), 10);

    let engine = load_system_requests();
    let paths = engine
        .io()
        .iter()
        .map(|point| point.path.clone())
        .collect::<Vec<_>>();
    for path in [
        AFTER_SUPPRESSION,
        COOLING_SETPOINT,
        ZONE_TEMPERATURE,
        COOLING_LOOP,
        AIRFLOW_SETPOINT,
        DISCHARGE_AIRFLOW,
        DAMPER_POSITION,
        ZONE_TEMPERATURE_REQUEST,
        ZONE_PRESSURE_REQUEST,
    ] {
        assert!(
            paths.contains(&path.to_string()),
            "missing facade path {path}"
        );
    }

    let (schedule_a, metrics_a) = simulate(engine, Arc::clone(&reference));
    let (schedule_b, metrics_b) = simulate(load_system_requests(), Arc::clone(&reference));
    assert_eq!(
        schedule_a, schedule_b,
        "CoolingOnly SystemRequests schedule is deterministic"
    );
    assert_trace_bit_eq(&metrics_a, &metrics_b);
    for output in OUTPUTS {
        assert_output_matches_reference(&metrics_a, &reference, *output);
    }
}
