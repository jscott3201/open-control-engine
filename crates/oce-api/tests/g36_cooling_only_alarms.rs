//! Source-verified ASHRAE G36 CoolingOnly Alarms through the frozen facade.
//!
//! The four `Utilities.Assert` instances have no output connectors. Their warning side effects
//! remain active while alarm predicates are true, while this replay collects the three Integer
//! alarm outputs exposed by the source composite.

use std::collections::HashMap;
use std::sync::Arc;

use oce_api::{CollectSpec, Engine, InputSource, SimMetrics, SimSpec, Value};

const ALARMS: &str = include_str!("../../oce-cxf/tests/fixtures/g36/cooling_only_alarms.jsonld");
const REFERENCE_CSV: &str =
    include_str!("../../../tools/golden-gen/goldens/G36/cooling_only_alarms/reference.csv");

const DISCHARGE_AIRFLOW: &str = "http://example.org#g36.source.cooling_only_alarms.VDis_flow";
const ACTIVE_AIRFLOW_SETPOINT: &str =
    "http://example.org#g36.source.cooling_only_alarms.VActSet_flow";
const SUPPLY_FAN_STATUS: &str = "http://example.org#g36.source.cooling_only_alarms.u1Fan";
const OPERATION_MODE: &str = "http://example.org#g36.source.cooling_only_alarms.uOpeMod";
const DAMPER_POSITION: &str = "http://example.org#g36.source.cooling_only_alarms.uDam";

const LOW_AIRFLOW_ALARM: &str = "http://example.org#g36.source.cooling_only_alarms.proInt.y";
const AIRFLOW_SENSOR_ALARM: &str = "http://example.org#g36.source.cooling_only_alarms.booToInt2.y";
const LEAKING_DAMPER_ALARM: &str = "http://example.org#g36.source.cooling_only_alarms.booToInt3.y";

const T_STOP: f64 = 3_420.0;
const SAMPLE_STEP: f64 = 60.0;
const ROWS: usize = 58;

type ScheduleSignature = (Vec<u32>, Vec<u32>, Vec<u32>);

const OUTPUTS: &[OutputPoint] = &[
    OutputPoint {
        reference_name: "low_airflow_alarm",
        runtime_name: LOW_AIRFLOW_ALARM,
    },
    OutputPoint {
        reference_name: "airflow_sensor_alarm",
        runtime_name: AIRFLOW_SENSOR_ALARM,
    },
    OutputPoint {
        reference_name: "leaking_damper_alarm",
        runtime_name: LEAKING_DAMPER_ALARM,
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
            "CoolingOnly Alarms reference columns must be unique"
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
            DISCHARGE_AIRFLOW.to_string(),
            Value::Real(reference.value(row, "discharge_airflow")),
        ),
        (
            ACTIVE_AIRFLOW_SETPOINT.to_string(),
            Value::Real(reference.value(row, "active_airflow_setpoint")),
        ),
        (
            SUPPLY_FAN_STATUS.to_string(),
            Value::Boolean(reference.value(row, "supply_fan_status") != 0.0),
        ),
        (
            OPERATION_MODE.to_string(),
            Value::Integer(reference.value(row, "operation_mode") as i64),
        ),
        (
            DAMPER_POSITION.to_string(),
            Value::Real(reference.value(row, "damper_position")),
        ),
    ]
}

fn load_alarms() -> Engine {
    let mut engine = Engine::in_memory();
    let report = engine
        .load_cxf(ALARMS.as_bytes())
        .expect("source-verified G36 CoolingOnly Alarms fixture loads");
    assert_eq!(report.block_count, 47);
    assert!(
        report.warnings.is_empty(),
        "fixture should not warn at load: {:?}",
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
        .expect("G36 CoolingOnly Alarms simulates");
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
fn cooling_only_alarms_loads_simulates_and_is_deterministic() {
    let reference = Arc::new(ReferenceTable::parse(REFERENCE_CSV));
    assert_eq!(reference.columns.len(), 9);

    let engine = load_alarms();
    let paths = engine
        .io()
        .iter()
        .map(|point| point.path.clone())
        .collect::<Vec<_>>();
    for path in [
        DISCHARGE_AIRFLOW,
        ACTIVE_AIRFLOW_SETPOINT,
        SUPPLY_FAN_STATUS,
        OPERATION_MODE,
        DAMPER_POSITION,
        LOW_AIRFLOW_ALARM,
        AIRFLOW_SENSOR_ALARM,
        LEAKING_DAMPER_ALARM,
    ] {
        assert!(
            paths.contains(&path.to_string()),
            "missing facade path {path}"
        );
    }

    let (schedule_a, metrics_a) = simulate(engine, Arc::clone(&reference));
    let (schedule_b, metrics_b) = simulate(load_alarms(), Arc::clone(&reference));
    assert_eq!(
        schedule_a, schedule_b,
        "CoolingOnly Alarms schedule is deterministic"
    );
    assert_trace_bit_eq(&metrics_a, &metrics_b);
    for output in OUTPUTS {
        assert_output_matches_reference(&metrics_a, &reference, *output);
    }
}
