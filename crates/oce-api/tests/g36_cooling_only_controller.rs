//! Whole-controller ASHRAE G36 CoolingOnly.Controller replay through the frozen facade.

use std::collections::HashMap;
use std::sync::Arc;

use oce_api::{CollectSpec, Engine, InputSource, SimMetrics, SimSpec, Value};

const FIXTURE: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/cooling_only_controller.jsonld");
const REFERENCE_CSV: &str =
    include_str!("../../../tools/golden-gen/goldens/G36/cooling_only_controller/reference.csv");
const MODEL: &str = "http://example.org#g36.source.cooling_only_controller";

const AIRFLOW_SETPOINT: &str = "conn#460";
const DAMPER_COMMAND: &str = "conn#485";
const ADJUSTED_POPULATION_FLOW: &str = "conn#352";
const ADJUSTED_AREA_FLOW: &str = "conn#356";
const MINIMUM_OUTDOOR_AIRFLOW: &str = "conn#388";
const ZONE_TEMPERATURE_RESET_REQUEST: &str = "conn#68";
const ZONE_PRESSURE_RESET_REQUEST: &str = "conn#76";
const LOW_AIRFLOW_ALARM: &str = "conn#182";
const AIRFLOW_SENSOR_ALARM: &str = "conn#212";
const LEAKING_DAMPER_ALARM: &str = "conn#227";

const T_STOP: f64 = 86_400.0;
const SAMPLE_STEP: f64 = 60.0;
const ROWS: usize = 1_441;

type ScheduleSignature = (Vec<u32>, Vec<u32>, Vec<u32>);

#[derive(Clone, Copy)]
enum OutputKind {
    Real,
    Integer,
}

#[derive(Clone, Copy)]
struct OutputPoint {
    reference_name: &'static str,
    runtime_name: &'static str,
    kind: OutputKind,
}

const OUTPUTS: &[OutputPoint] = &[
    OutputPoint {
        reference_name: "airflow_setpoint",
        runtime_name: AIRFLOW_SETPOINT,
        kind: OutputKind::Real,
    },
    OutputPoint {
        reference_name: "damper_command",
        runtime_name: DAMPER_COMMAND,
        kind: OutputKind::Real,
    },
    OutputPoint {
        reference_name: "adjusted_population_flow",
        runtime_name: ADJUSTED_POPULATION_FLOW,
        kind: OutputKind::Real,
    },
    OutputPoint {
        reference_name: "adjusted_area_flow",
        runtime_name: ADJUSTED_AREA_FLOW,
        kind: OutputKind::Real,
    },
    OutputPoint {
        reference_name: "minimum_outdoor_airflow",
        runtime_name: MINIMUM_OUTDOOR_AIRFLOW,
        kind: OutputKind::Real,
    },
    OutputPoint {
        reference_name: "zone_temperature_reset_request",
        runtime_name: ZONE_TEMPERATURE_RESET_REQUEST,
        kind: OutputKind::Integer,
    },
    OutputPoint {
        reference_name: "zone_pressure_reset_request",
        runtime_name: ZONE_PRESSURE_RESET_REQUEST,
        kind: OutputKind::Integer,
    },
    OutputPoint {
        reference_name: "low_airflow_alarm",
        runtime_name: LOW_AIRFLOW_ALARM,
        kind: OutputKind::Integer,
    },
    OutputPoint {
        reference_name: "airflow_sensor_alarm",
        runtime_name: AIRFLOW_SENSOR_ALARM,
        kind: OutputKind::Integer,
    },
    OutputPoint {
        reference_name: "leaking_damper_alarm",
        runtime_name: LEAKING_DAMPER_ALARM,
        kind: OutputKind::Integer,
    },
];

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
            "Controller reference columns must be unique"
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
                            .unwrap_or_else(|error| panic!("bad reference cell {cell:?}: {error}"))
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), ROWS);
        assert!(rows.iter().all(|row| row.len() == columns.len()));
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
        let row = (t / SAMPLE_STEP) as usize;
        assert_eq!(
            self.value(row, "time").to_bits(),
            t.to_bits(),
            "reference time grid"
        );
        row
    }

    fn times(&self) -> Vec<f64> {
        let time = self.col("time");
        self.rows.iter().map(|row| row[time]).collect()
    }
}

fn point(name: &str) -> String {
    format!("{MODEL}.{name}")
}

fn reference_inputs(reference: &ReferenceTable, t: f64) -> Vec<(String, Value)> {
    let row = reference.row_at_time(t);
    vec![
        (
            point("TZon"),
            Value::Real(reference.value(row, "zone_temperature")),
        ),
        (
            point("TCooSet"),
            Value::Real(reference.value(row, "cooling_setpoint")),
        ),
        (
            point("THeaSet"),
            Value::Real(reference.value(row, "heating_setpoint")),
        ),
        (
            point("u1Win"),
            Value::Boolean(reference.value(row, "window_status") != 0.0),
        ),
        (
            point("u1Occ"),
            Value::Boolean(reference.value(row, "occupancy_status") != 0.0),
        ),
        (
            point("uOpeMod"),
            Value::Integer(reference.value(row, "operating_mode") as i64),
        ),
        (
            point("ppmCO2Set"),
            Value::Real(reference.value(row, "co2_setpoint")),
        ),
        (
            point("ppmCO2"),
            Value::Real(reference.value(row, "co2_concentration")),
        ),
        (
            point("TDis"),
            Value::Real(reference.value(row, "discharge_air_temperature")),
        ),
        (
            point("TSup"),
            Value::Real(reference.value(row, "supply_air_temperature")),
        ),
        (
            point("VDis_flow"),
            Value::Real(reference.value(row, "discharge_airflow")),
        ),
        (
            point("oveFloSet"),
            Value::Integer(reference.value(row, "airflow_override_index") as i64),
        ),
        (
            point("oveDamPos"),
            Value::Integer(reference.value(row, "damper_override_index") as i64),
        ),
        (
            point("u1Fan"),
            Value::Boolean(reference.value(row, "supply_fan_status") != 0.0),
        ),
    ]
}

fn load_controller() -> Engine {
    let mut engine = Engine::in_memory();
    let report = engine
        .load_cxf(FIXTURE.as_bytes())
        .expect("source-verified G36 CoolingOnly.Controller fixture loads");
    assert_eq!(report.block_count, 213);
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
        .expect("G36 CoolingOnly.Controller simulates");
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
        match (output.kind, actual) {
            (OutputKind::Real, Value::Real(value)) => assert_eq!(
                value.to_bits(),
                expected.to_bits(),
                "{} row {row} diverged: {value:?} vs {expected:?}",
                output.reference_name
            ),
            (OutputKind::Integer, Value::Integer(value)) => assert_eq!(
                *value, expected as i64,
                "{} row {row} diverged",
                output.reference_name
            ),
            (_, other) => panic!(
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
        let left_values = left.trace.column(column).expect("left column");
        let right_values = right.trace.column(column).expect("right column");
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
fn whole_controller_replays_all_outputs_bit_exactly_and_repeats_deterministically() {
    let reference = Arc::new(ReferenceTable::parse(REFERENCE_CSV));
    assert_eq!(reference.columns.len(), 25);
    let engine = load_controller();
    let paths = engine
        .io()
        .iter()
        .map(|point| point.path.clone())
        .collect::<Vec<_>>();
    for input in [
        "TZon",
        "TCooSet",
        "THeaSet",
        "u1Win",
        "u1Occ",
        "uOpeMod",
        "ppmCO2Set",
        "ppmCO2",
        "TDis",
        "TSup",
        "VDis_flow",
        "oveFloSet",
        "oveDamPos",
        "u1Fan",
    ] {
        assert!(
            paths.contains(&point(input)),
            "missing facade input {input}"
        );
    }
    for output in OUTPUTS {
        assert!(
            paths.contains(&output.runtime_name.to_string()),
            "missing facade output {}",
            output.runtime_name
        );
    }

    let (schedule_a, metrics_a) = simulate(engine, Arc::clone(&reference));
    for output in OUTPUTS {
        assert_output_matches_reference(&metrics_a, &reference, *output);
    }
    let (schedule_b, metrics_b) = simulate(load_controller(), Arc::clone(&reference));
    assert_eq!(schedule_a, schedule_b);
    assert_trace_bit_eq(&metrics_a, &metrics_b);
}
