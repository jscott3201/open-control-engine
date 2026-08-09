//! Source-verified ASHRAE G36 Economizers.Subsequences.Enable through the frozen facade.

use oce_api::{CollectSpec, Engine, InputSource, SimMetrics, SimSpec, Value};

const ECONOMIZER_ENABLE: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/multizone_vav_economizer_enable.jsonld");

const OUTDOOR_AIR_TEMPERATURE: &str =
    "http://example.org#g36.source.multizone_vav_economizer_enable.TOut";
const OUTDOOR_AIR_CUTOFF: &str =
    "http://example.org#g36.source.multizone_vav_economizer_enable.TOutCut";
const OUTDOOR_DAMPER_MIN: &str =
    "http://example.org#g36.source.multizone_vav_economizer_enable.uOutDam_min";
const OUTDOOR_DAMPER_MAX: &str =
    "http://example.org#g36.source.multizone_vav_economizer_enable.uOutDam_max";
const RETURN_DAMPER_MAX: &str =
    "http://example.org#g36.source.multizone_vav_economizer_enable.uRetDam_max";
const RETURN_DAMPER_MIN: &str =
    "http://example.org#g36.source.multizone_vav_economizer_enable.uRetDam_min";
const RETURN_DAMPER_PHYSICAL_MAX: &str =
    "http://example.org#g36.source.multizone_vav_economizer_enable.uRetDamPhy_max";
const SUPPLY_FAN_ON: &str =
    "http://example.org#g36.source.multizone_vav_economizer_enable.u1SupFan";
const FREEZE_PROTECTION_STAGE: &str =
    "http://example.org#g36.source.multizone_vav_economizer_enable.uFreProSta";

const OUTDOOR_DAMPER_MAX_OUTPUT: &str =
    "http://example.org#g36.source.multizone_vav_economizer_enable.outDamSwitch.y";
const RETURN_DAMPER_MAX_OUTPUT: &str =
    "http://example.org#g36.source.multizone_vav_economizer_enable.maxRetDamSwitch.y";
const RETURN_DAMPER_MIN_OUTPUT: &str =
    "http://example.org#g36.source.multizone_vav_economizer_enable.minRetDamSwitch.y";

const EXPECTED_TIMES: [f64; 24] = [
    0.0, 60.0, 120.0, 180.0, 240.0, 300.0, 360.0, 420.0, 480.0, 540.0, 600.0, 660.0, 720.0, 780.0,
    840.0, 900.0, 960.0, 1020.0, 1080.0, 1140.0, 1200.0, 1260.0, 1320.0, 1380.0,
];

type ScheduleSignature = (Vec<u32>, Vec<u32>, Vec<u32>);

#[derive(Clone, Copy)]
struct Row {
    outdoor_air_temperature: f64,
    outdoor_air_cutoff: f64,
    outdoor_damper_min: f64,
    outdoor_damper_max: f64,
    return_damper_max: f64,
    return_damper_min: f64,
    return_damper_physical_max: f64,
    supply_fan_on: bool,
    freeze_protection_stage: i64,
}

#[derive(Default)]
struct ExpectedTrace {
    outdoor_damper_max: Vec<f64>,
    return_damper_max: Vec<f64>,
    return_damper_min: Vec<f64>,
}

#[derive(Default)]
struct HysteresisOracle {
    previous: bool,
}

impl HysteresisOracle {
    fn tick(&mut self, u: f64) -> bool {
        let y = (!self.previous && u > 0.0) || (self.previous && u >= -1.0);
        self.previous = y;
        y
    }
}

#[derive(Default)]
struct TrueFalseHoldOracle {
    initialized: bool,
    held: bool,
    timer: f64,
    previous_time: f64,
}

impl TrueFalseHoldOracle {
    fn tick(&mut self, t: f64, u: bool) -> bool {
        if !self.initialized {
            self.initialized = true;
            self.held = u;
            self.timer = 0.0;
            self.previous_time = t;
            return u;
        }
        self.timer += (t - self.previous_time).max(0.0);
        self.previous_time = t;
        if u == self.held {
            self.held
        } else if self.timer >= 600.0 {
            self.held = u;
            self.timer = 0.0;
            self.held
        } else {
            self.held
        }
    }
}

#[derive(Default)]
struct TrueDelayOracle {
    initialized: bool,
    previous_input: bool,
    held_output: bool,
    timer: f64,
    previous_time: f64,
}

impl TrueDelayOracle {
    fn tick(&mut self, t: f64, u: bool, delay: f64) -> bool {
        if !u {
            self.initialized = true;
            self.previous_input = false;
            self.held_output = false;
            self.timer = 0.0;
            self.previous_time = t;
            return false;
        }
        let output = if !self.initialized || self.held_output {
            true
        } else if !self.previous_input {
            delay <= 0.0
        } else {
            self.timer += (t - self.previous_time).max(0.0);
            self.timer >= delay
        };
        self.initialized = true;
        self.previous_input = true;
        self.held_output = output;
        self.timer = if output { delay } else { self.timer };
        self.previous_time = t;
        output
    }
}

fn pair(path: &str, value: Value) -> (String, Value) {
    (path.to_string(), value)
}

fn row_at(t: f64) -> Row {
    Row {
        outdoor_air_temperature: match t as u32 {
            0 => 294.0,
            60..=900 => 296.0,
            960..=1380 => 293.0,
            _ => unreachable!("unexpected test instant {t}"),
        },
        outdoor_air_cutoff: 295.0,
        outdoor_damper_min: 0.2,
        outdoor_damper_max: 0.9,
        return_damper_max: 0.8,
        return_damper_min: 0.1,
        return_damper_physical_max: 1.0,
        supply_fan_on: !matches!(t as u32, 660..=840),
        freeze_protection_stage: if (900.0..960.0).contains(&t) { 1 } else { 0 },
    }
}

fn economizer_enable_inputs(t: f64) -> Vec<(String, Value)> {
    let row = row_at(t);
    vec![
        pair(
            OUTDOOR_AIR_TEMPERATURE,
            Value::Real(row.outdoor_air_temperature),
        ),
        pair(OUTDOOR_AIR_CUTOFF, Value::Real(row.outdoor_air_cutoff)),
        pair(OUTDOOR_DAMPER_MIN, Value::Real(row.outdoor_damper_min)),
        pair(OUTDOOR_DAMPER_MAX, Value::Real(row.outdoor_damper_max)),
        pair(RETURN_DAMPER_MAX, Value::Real(row.return_damper_max)),
        pair(RETURN_DAMPER_MIN, Value::Real(row.return_damper_min)),
        pair(
            RETURN_DAMPER_PHYSICAL_MAX,
            Value::Real(row.return_damper_physical_max),
        ),
        pair(SUPPLY_FAN_ON, Value::Boolean(row.supply_fan_on)),
        pair(
            FREEZE_PROTECTION_STAGE,
            Value::Integer(row.freeze_protection_stage),
        ),
    ]
}

fn expected_trace() -> ExpectedTrace {
    let mut hysteresis = HysteresisOracle::default();
    let mut hold = TrueFalseHoldOracle::default();
    let mut outdoor_delay = TrueDelayOracle::default();
    let mut return_delay = TrueDelayOracle::default();
    let mut trace = ExpectedTrace::default();

    for t in EXPECTED_TIMES {
        let row = row_at(t);
        let dry_bulb_delta = row.outdoor_air_temperature - row.outdoor_air_cutoff;
        let outdoor_air_condition = hysteresis.tick(dry_bulb_delta);
        let held_condition = hold.tick(t, outdoor_air_condition);
        let enabled = held_condition && row.supply_fan_on && row.freeze_protection_stage == 0;
        let disabled = !enabled;
        let outdoor_delay_done = outdoor_delay.tick(t, disabled, 15.0);
        let return_delay_done = return_delay.tick(t, disabled, 180.0);
        let close_outdoor_damper = disabled && outdoor_delay_done;
        let force_return_damper_physical = disabled && !return_delay_done;

        let return_min_base = if disabled {
            row.return_damper_max
        } else {
            row.return_damper_min
        };
        trace.outdoor_damper_max.push(if close_outdoor_damper {
            row.outdoor_damper_min
        } else {
            row.outdoor_damper_max
        });
        trace
            .return_damper_max
            .push(if force_return_damper_physical {
                row.return_damper_physical_max
            } else {
                row.return_damper_max
            });
        trace
            .return_damper_min
            .push(if force_return_damper_physical {
                row.return_damper_physical_max
            } else {
                return_min_base
            });
    }
    trace
}

fn load_economizer_enable() -> Engine {
    let mut engine = Engine::in_memory();
    let report = engine
        .load_cxf(ECONOMIZER_ENABLE.as_bytes())
        .expect("source-verified G36 Economizers.Subsequences.Enable fixture loads");
    assert_eq!(report.block_count, 19);
    assert_eq!(report.stateful_blocks, 4);
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
            t_stop: 1380.0,
            step: 60.0,
            inputs: InputSource::Closure(Box::new(economizer_enable_inputs)),
            collect: CollectSpec::Named {
                points: vec![
                    OUTDOOR_DAMPER_MAX_OUTPUT.to_string(),
                    RETURN_DAMPER_MAX_OUTPUT.to_string(),
                    RETURN_DAMPER_MIN_OUTPUT.to_string(),
                ],
                stride: 1,
            },
        })
        .expect("G36 Economizers.Subsequences.Enable simulates");
    assert_eq!(metrics.ticks, 24);
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
fn multizone_vav_economizer_enable_loads_simulates_and_is_deterministic() {
    let engine = load_economizer_enable();
    let points = engine
        .io()
        .iter()
        .map(|point| point.path.clone())
        .collect::<Vec<_>>();
    for input in [
        OUTDOOR_AIR_TEMPERATURE,
        OUTDOOR_AIR_CUTOFF,
        OUTDOOR_DAMPER_MIN,
        OUTDOOR_DAMPER_MAX,
        RETURN_DAMPER_MAX,
        RETURN_DAMPER_MIN,
        RETURN_DAMPER_PHYSICAL_MAX,
        SUPPLY_FAN_ON,
        FREEZE_PROTECTION_STAGE,
    ] {
        assert!(
            points.contains(&input.to_string()),
            "missing facade input {input}"
        );
        assert_eq!(
            points.iter().filter(|path| path.as_str() == input).count(),
            1,
            "source input {input} should expose one logical facade point"
        );
    }
    for output in [
        OUTDOOR_DAMPER_MAX_OUTPUT,
        RETURN_DAMPER_MAX_OUTPUT,
        RETURN_DAMPER_MIN_OUTPUT,
    ] {
        assert!(
            points.contains(&output.to_string()),
            "missing runtime output {output}"
        );
    }
    let (schedule, metrics) = simulate(engine);
    let expected = expected_trace();
    assert_real_bits(
        &real_column(&metrics, OUTDOOR_DAMPER_MAX_OUTPUT),
        &expected.outdoor_damper_max,
        "outdoor damper maximum",
    );
    assert_real_bits(
        &real_column(&metrics, RETURN_DAMPER_MAX_OUTPUT),
        &expected.return_damper_max,
        "return damper maximum",
    );
    assert_real_bits(
        &real_column(&metrics, RETURN_DAMPER_MIN_OUTPUT),
        &expected.return_damper_min,
        "return damper minimum",
    );

    let (second_schedule, second_metrics) = simulate(load_economizer_enable());
    assert_eq!(schedule, second_schedule);
    assert_trace_bit_eq(&metrics, &second_metrics);
}
