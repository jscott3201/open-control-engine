//! Source-verified ASHRAE G36 FreezeProtection through the frozen facade.

use std::collections::HashMap;
use std::sync::Arc;

use oce_api::{CollectSpec, Engine, InputSource, SimMetrics, SimSpec, Value};

const FREEZE_PROTECTION: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/multizone_vav_freeze_protection.jsonld");
const REFERENCE_CSV: &str = include_str!(
    "../../../tools/golden-gen/goldens/G36/multizone_vav_freeze_protection/reference.csv"
);

const OUTDOOR_DAMPER_MIN_POSITION: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.uOutDamPosMin";
const OUTDOOR_DAMPER: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.uOutDam";
const HEATING_COIL: &str = "http://example.org#g36.source.multizone_vav_freeze_protection.uHeaCoi";
const MINIMUM_OUTDOOR_DAMPER: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.uMinOutDam";
const RETURN_DAMPER: &str = "http://example.org#g36.source.multizone_vav_freeze_protection.uRetDam";
const SUPPLY_AIR_TEMPERATURE: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.TAirSup";
const SOFTWARE_RESET: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.u1SofSwiRes";
const SUPPLY_FAN_STATUS_INPUT: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.u1SupFan";
const SUPPLY_FAN_SPEED_INPUT: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.uSupFan";
const COOLING_COIL: &str = "http://example.org#g36.source.multizone_vav_freeze_protection.uCooCoi";
const MIXED_AIR_TEMPERATURE: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.TAirMix";

const FREEZE_PROTECTION_STAGE: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.intSwi2.y";
const CHILLED_WATER_PUMP_ENABLE: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.lat1.y";
const RETURN_DAMPER_COMMAND: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.retDam.y";
const OUTDOOR_DAMPER_COMMAND: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.outDam.y";
const MINIMUM_OUTDOOR_DAMPER_COMMAND: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.minOutDam.y";
const SUPPLY_FAN_STATUS: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.and2.y";
const SUPPLY_FAN_SPEED: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.supFan.y";
const COOLING_COIL_COMMAND: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.cooCoiVal.y";
const HEATING_COIL_COMMAND: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.heaCoiPos.y";
const HOT_WATER_PLANT_REQUEST: &str =
    "http://example.org#g36.source.multizone_vav_freeze_protection.hotWatPlaReq3.y";
const ALARM_LEVEL: &str = "http://example.org#g36.source.multizone_vav_freeze_protection.intSwi3.y";

const T_STOP: f64 = 6600.0;
const SAMPLE_STEP: f64 = 60.0;
const ROWS: usize = 111;

type ScheduleSignature = (Vec<u32>, Vec<u32>, Vec<u32>);

const INPUTS: &[InputPoint] = &[
    InputPoint::real("outdoor_damper_min_position", OUTDOOR_DAMPER_MIN_POSITION),
    InputPoint::real("outdoor_damper", OUTDOOR_DAMPER),
    InputPoint::real("heating_coil", HEATING_COIL),
    InputPoint::real("minimum_outdoor_damper", MINIMUM_OUTDOOR_DAMPER),
    InputPoint::real("return_damper", RETURN_DAMPER),
    InputPoint::real("supply_air_temperature", SUPPLY_AIR_TEMPERATURE),
    InputPoint::boolean("software_reset", SOFTWARE_RESET),
    InputPoint::boolean("supply_fan_status_input", SUPPLY_FAN_STATUS_INPUT),
    InputPoint::real("supply_fan_speed_input", SUPPLY_FAN_SPEED_INPUT),
    InputPoint::real("cooling_coil", COOLING_COIL),
    InputPoint::real("mixed_air_temperature", MIXED_AIR_TEMPERATURE),
];

const OUTPUTS: &[OutputPoint] = &[
    OutputPoint::integer("freeze_protection_stage", FREEZE_PROTECTION_STAGE),
    OutputPoint::boolean("chilled_water_pump_enable", CHILLED_WATER_PUMP_ENABLE),
    OutputPoint::real("return_damper_command", RETURN_DAMPER_COMMAND),
    OutputPoint::real("outdoor_damper_command", OUTDOOR_DAMPER_COMMAND),
    OutputPoint::real(
        "minimum_outdoor_damper_command",
        MINIMUM_OUTDOOR_DAMPER_COMMAND,
    ),
    OutputPoint::boolean("supply_fan_status", SUPPLY_FAN_STATUS),
    OutputPoint::real("supply_fan_speed", SUPPLY_FAN_SPEED),
    OutputPoint::real("cooling_coil_command", COOLING_COIL_COMMAND),
    OutputPoint::real("heating_coil_command", HEATING_COIL_COMMAND),
    OutputPoint::integer("hot_water_plant_request", HOT_WATER_PLANT_REQUEST),
    OutputPoint::integer("alarm_level", ALARM_LEVEL),
];

#[derive(Clone, Copy)]
struct InputPoint {
    reference_name: &'static str,
    runtime_name: &'static str,
    kind: Kind,
}

impl InputPoint {
    const fn real(reference_name: &'static str, runtime_name: &'static str) -> Self {
        Self {
            reference_name,
            runtime_name,
            kind: Kind::Real,
        }
    }

    const fn boolean(reference_name: &'static str, runtime_name: &'static str) -> Self {
        Self {
            reference_name,
            runtime_name,
            kind: Kind::Boolean,
        }
    }
}

#[derive(Clone, Copy)]
struct OutputPoint {
    reference_name: &'static str,
    runtime_name: &'static str,
    kind: Kind,
}

impl OutputPoint {
    const fn real(reference_name: &'static str, runtime_name: &'static str) -> Self {
        Self {
            reference_name,
            runtime_name,
            kind: Kind::Real,
        }
    }

    const fn integer(reference_name: &'static str, runtime_name: &'static str) -> Self {
        Self {
            reference_name,
            runtime_name,
            kind: Kind::Integer,
        }
    }

    const fn boolean(reference_name: &'static str, runtime_name: &'static str) -> Self {
        Self {
            reference_name,
            runtime_name,
            kind: Kind::Boolean,
        }
    }
}

#[derive(Clone, Copy)]
enum Kind {
    Real,
    Integer,
    Boolean,
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
            "FreezeProtection reference columns must be unique"
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

fn pair(path: &str, value: Value) -> (String, Value) {
    (path.to_string(), value)
}

fn reference_inputs(reference: &ReferenceTable, t: f64) -> Vec<(String, Value)> {
    let row = reference.row_at_time(t);
    INPUTS
        .iter()
        .map(|point| {
            let raw = reference.value(row, point.reference_name);
            let value = match point.kind {
                Kind::Real => Value::Real(raw),
                Kind::Integer => Value::Integer(raw as i64),
                Kind::Boolean => Value::Boolean(raw != 0.0),
            };
            pair(point.runtime_name, value)
        })
        .collect()
}

fn load_freeze_protection() -> Engine {
    let mut engine = Engine::in_memory();
    let report = engine
        .load_cxf(FREEZE_PROTECTION.as_bytes())
        .expect("source-verified G36 FreezeProtection fixture loads");
    assert_eq!(report.block_count, 61);
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
    let points = OUTPUTS
        .iter()
        .map(|output| output.runtime_name.to_string())
        .collect();
    let input_reference = Arc::clone(&reference);
    let metrics = engine
        .simulate(&SimSpec {
            t_start: 0.0,
            t_stop: T_STOP,
            step: SAMPLE_STEP,
            inputs: InputSource::Closure(Box::new(move |t| reference_inputs(&input_reference, t))),
            collect: CollectSpec::Named { points, stride: 1 },
        })
        .expect("G36 FreezeProtection simulates");
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
            (Kind::Real, Value::Real(value)) => assert_eq!(
                value.to_bits(),
                expected.to_bits(),
                "{} row {row} diverged",
                output.reference_name
            ),
            (Kind::Integer, Value::Integer(value)) => assert_eq!(
                *value, expected as i64,
                "{} row {row} diverged",
                output.reference_name
            ),
            (Kind::Boolean, Value::Boolean(value)) => assert_eq!(
                *value,
                expected != 0.0,
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

fn assert_stage(reference: &ReferenceTable, t: f64, expected: i64) {
    let row = reference.row_at_time(t);
    assert_eq!(
        reference.value(row, "freeze_protection_stage") as i64,
        expected,
        "stage at t={t}"
    );
}

#[test]
fn multizone_vav_freeze_protection_loads_simulates_and_is_deterministic() {
    let reference = Arc::new(ReferenceTable::parse(REFERENCE_CSV));
    assert_eq!(reference.columns.len(), 23);
    assert_stage(&reference, 300.0, 0);
    assert_stage(&reference, 360.0, 1);
    assert_stage(&reference, 1260.0, 2);
    assert_stage(&reference, 1800.0, 3);
    assert_stage(&reference, 1980.0, 2);
    assert_stage(&reference, 6300.0, 3);
    assert_stage(&reference, 6360.0, 2);

    let engine = load_freeze_protection();
    let paths = engine
        .io()
        .iter()
        .map(|point| point.path.clone())
        .collect::<Vec<_>>();
    for input in INPUTS {
        assert!(
            paths.contains(&input.runtime_name.to_string()),
            "missing facade input {}",
            input.runtime_name
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
    let (schedule_b, metrics_b) = simulate(load_freeze_protection(), Arc::clone(&reference));
    assert_eq!(
        schedule_a, schedule_b,
        "FreezeProtection schedule is deterministic"
    );
    assert_trace_bit_eq(&metrics_a, &metrics_b);
    for output in OUTPUTS {
        assert_output_matches_reference(&metrics_a, &reference, *output);
    }
}
