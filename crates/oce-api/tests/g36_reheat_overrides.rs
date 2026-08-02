//! Source-verified ASHRAE G36 Reheat Overrides through the frozen facade.

use std::collections::HashMap;
use std::sync::Arc;

use oce_api::{CollectSpec, Engine, InputSource, SimMetrics, SimSpec, Value};

const REHEAT_OVERRIDES: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/reheat_overrides.jsonld");
const REFERENCE_CSV: &str =
    include_str!("../../../tools/golden-gen/goldens/G36/reheat_overrides/reference.csv");

const DAMPER_OVERRIDE_INDEX: &str = "http://example.org#g36.source.reheat_overrides.oveDamPos";
const DAMPER_COMMAND_INPUT: &str = "http://example.org#g36.source.reheat_overrides.uDam";
const HEATING_VALVE_OFF: &str = "http://example.org#g36.source.reheat_overrides.uHeaOff";
const HEATING_VALVE_COMMAND_INPUT: &str = "http://example.org#g36.source.reheat_overrides.uVal";

const DAMPER_COMMAND: &str = "http://example.org#g36.source.reheat_overrides.swi1.y";
const HEATING_VALVE_COMMAND: &str = "http://example.org#g36.source.reheat_overrides.pro.y";

const T_STOP: f64 = 11.0;
const SAMPLE_STEP: f64 = 1.0;
const ROWS: usize = 12;

type ScheduleSignature = (Vec<u32>, Vec<u32>, Vec<u32>);

const OUTPUTS: &[OutputPoint] = &[
    OutputPoint {
        reference_name: "damper_command",
        runtime_name: DAMPER_COMMAND,
    },
    OutputPoint {
        reference_name: "heating_valve_command",
        runtime_name: HEATING_VALVE_COMMAND,
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
            "Reheat Overrides reference columns must be unique"
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
            DAMPER_OVERRIDE_INDEX.to_string(),
            Value::Integer(reference.value(row, "damper_override_index") as i64),
        ),
        (
            DAMPER_COMMAND_INPUT.to_string(),
            Value::Real(reference.value(row, "damper_command_input")),
        ),
        (
            HEATING_VALVE_OFF.to_string(),
            Value::Boolean(reference.value(row, "heating_valve_off") != 0.0),
        ),
        (
            HEATING_VALVE_COMMAND_INPUT.to_string(),
            Value::Real(reference.value(row, "heating_valve_command_input")),
        ),
    ]
}

fn load_reheat_overrides() -> Engine {
    let mut engine = Engine::in_memory();
    let report = engine
        .load_cxf(REHEAT_OVERRIDES.as_bytes())
        .expect("source-verified G36 Reheat Overrides fixture loads");
    assert_eq!(report.block_count, 11);
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
                    DAMPER_COMMAND.to_string(),
                    HEATING_VALVE_COMMAND.to_string(),
                ],
                stride: 1,
            },
        })
        .expect("G36 Reheat Overrides simulates");
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
        let expected = reference.value(row, output.reference_name);
        match actual {
            Value::Real(value) => assert_eq!(
                value.to_bits(),
                expected.to_bits(),
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
fn reheat_overrides_loads_simulates_and_is_deterministic() {
    let reference = Arc::new(ReferenceTable::parse(REFERENCE_CSV));
    assert_eq!(reference.columns.len(), 7);

    let engine = load_reheat_overrides();
    let paths = engine
        .io()
        .iter()
        .map(|point| point.path.clone())
        .collect::<Vec<_>>();
    for path in [
        DAMPER_OVERRIDE_INDEX,
        DAMPER_COMMAND_INPUT,
        HEATING_VALVE_OFF,
        HEATING_VALVE_COMMAND_INPUT,
        DAMPER_COMMAND,
        HEATING_VALVE_COMMAND,
    ] {
        assert!(
            paths.contains(&path.to_string()),
            "missing facade path {path}"
        );
    }

    let (schedule_a, metrics_a) = simulate(engine, Arc::clone(&reference));
    let (schedule_b, metrics_b) = simulate(load_reheat_overrides(), Arc::clone(&reference));
    assert_eq!(
        schedule_a, schedule_b,
        "Reheat Overrides schedule is deterministic"
    );
    assert_trace_bit_eq(&metrics_a, &metrics_b);
    for output in OUTPUTS {
        assert_output_matches_reference(&metrics_a, &reference, *output);
    }
}
