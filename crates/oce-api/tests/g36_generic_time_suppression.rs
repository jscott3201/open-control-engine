//! Source-verified ASHRAE G36 Generic.TimeSuppression through the frozen facade.
//!
//! The replay uses the independent 60-second Tier-A schedule and collects the single Boolean
//! `yAftSup` output exactly. Two fresh engines must produce identical schedules and bit-equal
//! traces.

use std::collections::HashMap;
use std::sync::Arc;

use oce_api::{CollectSpec, Engine, InputSource, SimMetrics, SimSpec, Value};

const TIME_SUPPRESSION: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/generic_time_suppression.jsonld");
const REFERENCE_CSV: &str =
    include_str!("../../../tools/golden-gen/goldens/G36/generic_time_suppression/reference.csv");

const SETPOINT_TEMPERATURE: &str = "http://example.org#g36.source.generic_time_suppression.TSet";
const ZONE_TEMPERATURE: &str = "http://example.org#g36.source.generic_time_suppression.TZon";
const AFTER_SUPPRESSION: &str =
    "http://example.org#g36.source.generic_time_suppression.pasSupTim.y";

const T_STOP: f64 = 5_400.0;
const SAMPLE_STEP: f64 = 60.0;
const ROWS: usize = 91;

type ScheduleSignature = (Vec<u32>, Vec<u32>, Vec<u32>);

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
            "Generic TimeSuppression reference columns must be unique"
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
            SETPOINT_TEMPERATURE.to_string(),
            Value::Real(reference.value(row, "setpoint_temperature")),
        ),
        (
            ZONE_TEMPERATURE.to_string(),
            Value::Real(reference.value(row, "zone_temperature")),
        ),
    ]
}

fn load_time_suppression() -> Engine {
    let mut engine = Engine::in_memory();
    let report = engine
        .load_cxf(TIME_SUPPRESSION.as_bytes())
        .expect("source-verified G36 Generic TimeSuppression fixture loads");
    assert_eq!(report.block_count, 24);
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
                points: vec![AFTER_SUPPRESSION.to_string()],
                stride: 1,
            },
        })
        .expect("G36 Generic TimeSuppression simulates");
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

fn assert_output_matches_reference(metrics: &SimMetrics, reference: &ReferenceTable) {
    let index = metrics
        .trace
        .columns()
        .iter()
        .position(|column| column == AFTER_SUPPRESSION)
        .expect("after-suppression trace column");
    let column = metrics.trace.column(index).expect("column index is valid");
    assert_eq!(column.len(), ROWS);
    for (row, actual) in column.iter().enumerate() {
        let expected = reference.value(row, "after_suppression") != 0.0;
        match actual {
            Value::Boolean(value) => {
                assert_eq!(*value, expected, "after_suppression row {row} diverged")
            }
            other => panic!("after_suppression row {row} had wrong runtime kind: {other:?}"),
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
fn generic_time_suppression_loads_simulates_and_is_deterministic() {
    let reference = Arc::new(ReferenceTable::parse(REFERENCE_CSV));
    assert_eq!(reference.columns.len(), 4);

    let engine = load_time_suppression();
    let paths = engine
        .io()
        .iter()
        .map(|point| point.path.clone())
        .collect::<Vec<_>>();
    for path in [SETPOINT_TEMPERATURE, ZONE_TEMPERATURE, AFTER_SUPPRESSION] {
        assert!(
            paths.contains(&path.to_string()),
            "missing facade path {path}"
        );
    }

    let (schedule_a, metrics_a) = simulate(engine, Arc::clone(&reference));
    let (schedule_b, metrics_b) = simulate(load_time_suppression(), Arc::clone(&reference));
    assert_eq!(
        schedule_a, schedule_b,
        "Generic TimeSuppression schedule is deterministic"
    );
    assert_trace_bit_eq(&metrics_a, &metrics_b);
    assert_output_matches_reference(&metrics_a, &reference);
}
