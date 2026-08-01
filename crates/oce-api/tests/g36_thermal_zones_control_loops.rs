//! Source-verified ASHRAE G36 ThermalZones.ControlLoops through the frozen facade.
//!
//! The replay consumes the independent 54-row CL1-CL8 Tier-A schedule and compares both Real
//! outputs bit-exactly. Two fresh engines must expose the same schedule and identical traces.

use std::collections::HashMap;
use std::sync::Arc;

use oce_api::{CollectSpec, Engine, InputSource, SimMetrics, SimSpec, Value};

const CONTROL_LOOPS: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/thermal_zones_control_loops.jsonld");
const REFERENCE_CSV: &str =
    include_str!("../../../tools/golden-gen/goldens/G36/thermal_zones_control_loops/reference.csv");

const COOLING_SETPOINT: &str = "http://example.org#g36.source.thermal_zones_control_loops.TCooSet";
const ZONE_TEMPERATURE: &str = "http://example.org#g36.source.thermal_zones_control_loops.TZon";
const HEATING_SETPOINT: &str = "http://example.org#g36.source.thermal_zones_control_loops.THeaSet";
const COOLING_LOOP_SIGNAL: &str = "conn#22";
const HEATING_LOOP_SIGNAL: &str = "conn#31";

const T_STOP: f64 = 3_180.0;
const SAMPLE_STEP: f64 = 60.0;
const ROWS: usize = 54;

type ScheduleSignature = (Vec<u32>, Vec<u32>, Vec<u32>);

const OUTPUTS: &[OutputPoint] = &[
    OutputPoint {
        reference_name: "cooling_loop_signal",
        runtime_name: COOLING_LOOP_SIGNAL,
    },
    OutputPoint {
        reference_name: "heating_loop_signal",
        runtime_name: HEATING_LOOP_SIGNAL,
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
            "ThermalZones ControlLoops reference columns must be unique"
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
            COOLING_SETPOINT.to_string(),
            Value::Real(reference.value(row, "cooling_setpoint")),
        ),
        (
            ZONE_TEMPERATURE.to_string(),
            Value::Real(reference.value(row, "zone_temperature")),
        ),
        (
            HEATING_SETPOINT.to_string(),
            Value::Real(reference.value(row, "heating_setpoint")),
        ),
    ]
}

fn load_control_loops() -> Engine {
    let mut engine = Engine::in_memory();
    let report = engine
        .load_cxf(CONTROL_LOOPS.as_bytes())
        .expect("source-verified G36 ThermalZones ControlLoops fixture loads");
    assert_eq!(report.block_count, 16);
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
                points: vec![
                    COOLING_LOOP_SIGNAL.to_string(),
                    HEATING_LOOP_SIGNAL.to_string(),
                ],
                stride: 1,
            },
        })
        .expect("G36 ThermalZones ControlLoops simulates");
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
fn both_loop_outputs_match_the_oracle_and_repeat_bit_exactly() {
    let reference = Arc::new(ReferenceTable::parse(REFERENCE_CSV));
    assert_eq!(reference.columns.len(), 6);

    let engine = load_control_loops();
    let paths = engine
        .io()
        .iter()
        .map(|point| point.path.clone())
        .collect::<Vec<_>>();
    for path in [
        COOLING_SETPOINT,
        ZONE_TEMPERATURE,
        HEATING_SETPOINT,
        COOLING_LOOP_SIGNAL,
        HEATING_LOOP_SIGNAL,
    ] {
        assert!(
            paths.contains(&path.to_string()),
            "missing facade path {path}"
        );
    }

    let (schedule_a, metrics_a) = simulate(engine, Arc::clone(&reference));
    let (schedule_b, metrics_b) = simulate(load_control_loops(), Arc::clone(&reference));
    assert_eq!(
        schedule_a, schedule_b,
        "ThermalZones ControlLoops schedule is deterministic"
    );
    assert_trace_bit_eq(&metrics_a, &metrics_b);
    for output in OUTPUTS {
        assert_output_matches_reference(&metrics_a, &reference, *output);
    }
}
