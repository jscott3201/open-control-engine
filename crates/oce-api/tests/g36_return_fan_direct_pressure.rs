//! Source-verified ASHRAE G36 ReturnFanDirectPressure through the frozen facade.

use oce_api::{CollectSpec, Engine, InputSource, PointDirection, SimMetrics, SimSpec, Value};

const RETURN_FAN_DIRECT_PRESSURE: &str = include_str!(
    "../../oce-cxf/tests/fixtures/g36/multizone_vav_return_fan_direct_pressure.jsonld"
);

const BUILDING_PRESSURE: &str = "conn#0";
const MIN_OUTDOOR_AIR_DAMPER: &str = "conn#46";
const SUPPLY_FAN_ON: &str = "conn#22";
const AVERAGED_PRESSURE: &str = "conn#1";
const RELIEF_DAMPER_COMMAND: &str = "conn#20";
const DISCHARGE_PRESSURE_SETPOINT: &str = "conn#24";
const RETURN_FAN_SPEED: &str = "conn#37";
const RETURN_FAN_STATUS: &str = "conn#53";

const EXPECTED_TIMES: [f64; 6] = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0];

type ScheduleSignature = (Vec<u32>, Vec<u32>, Vec<u32>);

#[derive(Default)]
struct ExpectedTrace {
    averaged_pressure: Vec<f64>,
    relief_damper_command: Vec<f64>,
    discharge_pressure_setpoint: Vec<f64>,
    return_fan_speed: Vec<f64>,
    return_fan_status: Vec<bool>,
}

#[derive(Default)]
struct MovingAverageState {
    t_start: f64,
    prev_t: Option<f64>,
    mu: f64,
    points: Vec<(f64, f64)>,
}

impl MovingAverageState {
    const DELTA: f64 = 300.0;
    const MIN_DELTA: f64 = 1e-5;

    fn mu_now(&self, t: f64, input: f64) -> f64 {
        self.mu + input * self.prev_t.map_or(0.0, |prev| t - prev)
    }

    fn output(&self, t: f64, input: f64) -> f64 {
        let t_start = self.prev_t.map_or(t, |_| self.t_start);
        let mu_now = self.mu_now(t, input);
        let mu_del = self.mu_at(t - Self::DELTA, t, mu_now);
        let denom = if t >= t_start + Self::DELTA {
            let retained_lo = self.points.first().map_or(t_start, |point| point.0);
            let t_lo = (t - Self::DELTA).max(retained_lo).max(t_start);
            (t - t_lo).max(Self::MIN_DELTA)
        } else {
            t - t_start + 1e-3
        };
        (mu_now - mu_del) / denom
    }

    fn update(&mut self, t: f64, input: f64) {
        let first = self.prev_t.is_none();
        let mu_now = self.mu_now(t, input);
        if first {
            self.t_start = t;
        }
        self.prune(t - Self::DELTA);
        self.store(t, mu_now);
        self.mu = mu_now;
        self.prev_t = Some(t);
    }

    fn prune(&mut self, cutoff: f64) {
        while self.points.len() > 1 && self.points[1].0 <= cutoff {
            self.points.remove(0);
        }
    }

    fn store(&mut self, t: f64, mu: f64) {
        if let Some(last) = self.points.last_mut()
            && last.0.to_bits() == t.to_bits()
        {
            last.1 = mu;
            return;
        }
        self.points.push((t, mu));
    }

    fn mu_at(&self, target: f64, t_now: f64, mu_now: f64) -> f64 {
        let Some(&(first_t, first_mu)) = self.points.first() else {
            return mu_now;
        };
        if target <= first_t {
            return first_mu;
        }

        let mut previous = (first_t, first_mu);
        for &next in self.points.iter().skip(1) {
            if target <= next.0 {
                let den = next.0 - previous.0;
                return if den == 0.0 {
                    next.1
                } else {
                    previous.1 + (next.1 - previous.1) * ((target - previous.0) / den)
                };
            }
            previous = next;
        }

        if target <= t_now {
            let den = t_now - previous.0;
            if den == 0.0 {
                mu_now
            } else {
                previous.1 + (mu_now - previous.1) * ((target - previous.0) / den)
            }
        } else {
            mu_now
        }
    }
}

fn pair(path: &str, value: Value) -> (String, Value) {
    (path.to_string(), value)
}

fn input_row(t: f64) -> (f64, bool, bool) {
    match t as u32 {
        0 => (12.0, true, false),
        1 => (9.009, false, true),
        2 => (21.006, true, true),
        3 => (-21.012, true, true),
        4 => (135.033, true, true),
        _ => (-264.06, true, true),
    }
}

fn return_fan_direct_pressure_inputs(t: f64) -> Vec<(String, Value)> {
    let (building_pressure, min_outdoor_air_damper, supply_fan_on) = input_row(t);
    vec![
        pair(BUILDING_PRESSURE, Value::Real(building_pressure)),
        pair(
            MIN_OUTDOOR_AIR_DAMPER,
            Value::Boolean(min_outdoor_air_damper),
        ),
        pair(SUPPLY_FAN_ON, Value::Boolean(supply_fan_on)),
    ]
}

fn expected_trace() -> ExpectedTrace {
    const DP_BUI_SET: f64 = 12.0;
    const K: f64 = 1.0;
    const TI: f64 = 0.5;
    const NI: f64 = 0.9;
    const Y_MIN: f64 = 0.0;
    const Y_MAX: f64 = 1.0;

    let mut moving_average = MovingAverageState::default();
    let mut integral = 0.0;
    let mut previous_time: Option<f64> = None;
    let mut trace = ExpectedTrace::default();

    for &t in &EXPECTED_TIMES {
        let (pressure, min_outdoor_air_damper, supply_fan_on) = input_row(t);
        let average = moving_average.output(t, pressure);
        let normalized_pressure = average / DP_BUI_SET;
        let error = 1.0 - normalized_pressure;
        let proportional = K * error;
        let unlimited = proportional + integral;
        let pid = unlimited.clamp(Y_MIN, Y_MAX);

        let relief_damper_line = buildings_line(0.0, 0.0, 0.5, 1.0, pid);
        let return_fan_pressure_setpoint = buildings_line(0.5, 2.4, 1.0, 40.0, pid);
        let return_fan_speed_line =
            buildings_line(2.4, 0.1, 40.0, 1.0, return_fan_pressure_setpoint);

        trace.averaged_pressure.push(average);
        trace
            .relief_damper_command
            .push(if min_outdoor_air_damper && supply_fan_on {
                relief_damper_line
            } else {
                0.0
            });
        trace.discharge_pressure_setpoint.push(if supply_fan_on {
            return_fan_pressure_setpoint
        } else {
            0.0
        });
        trace.return_fan_speed.push(if supply_fan_on {
            return_fan_speed_line
        } else {
            0.0
        });
        trace.return_fan_status.push(supply_fan_on);

        let dt = previous_time.map_or(0.0, |previous| (t - previous).max(0.0));
        let anti_windup_gain = (unlimited - pid) / (K * NI);
        let corrected_error = error - anti_windup_gain;
        integral += (K / TI) * corrected_error * dt;
        previous_time = Some(t);
        moving_average.update(t, pressure);
    }

    trace
}

fn buildings_line(x1: f64, f1: f64, x2: f64, f2: f64, u: f64) -> f64 {
    let x_lim = u.max(x1).min(x2);
    let slope = (f2 - f1) / (x2 - x1);
    let intercept = f2 - slope * x2;
    intercept + slope * x_lim
}

fn load_return_fan_direct_pressure() -> Engine {
    let mut engine = Engine::in_memory();
    let report = engine
        .load_cxf(RETURN_FAN_DIRECT_PRESSURE.as_bytes())
        .expect("source-verified G36 ReturnFanDirectPressure fixture loads");
    assert_eq!(report.block_count, 22);
    assert_eq!(report.stateful_blocks, 2);
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
            inputs: InputSource::Closure(Box::new(return_fan_direct_pressure_inputs)),
            collect: CollectSpec::Named {
                points: vec![
                    AVERAGED_PRESSURE.to_string(),
                    RELIEF_DAMPER_COMMAND.to_string(),
                    DISCHARGE_PRESSURE_SETPOINT.to_string(),
                    RETURN_FAN_SPEED.to_string(),
                    RETURN_FAN_STATUS.to_string(),
                ],
                stride: 1,
            },
        })
        .expect("G36 ReturnFanDirectPressure simulates");
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

fn bool_column(metrics: &SimMetrics, path: &str) -> Vec<bool> {
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
            Value::Boolean(x) => *x,
            other => panic!("{path} must be Boolean, got {other:?}"),
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
fn multizone_vav_return_fan_direct_pressure_loads_simulates_and_is_deterministic() {
    let engine = load_return_fan_direct_pressure();
    let paths = engine
        .io()
        .iter()
        .map(|point| point.path.clone())
        .collect::<Vec<_>>();
    for input in [BUILDING_PRESSURE, MIN_OUTDOOR_AIR_DAMPER, SUPPLY_FAN_ON] {
        assert!(
            paths.contains(&input.to_string()),
            "missing facade input {input}"
        );
    }
    for output in [
        AVERAGED_PRESSURE,
        RELIEF_DAMPER_COMMAND,
        DISCHARGE_PRESSURE_SETPOINT,
        RETURN_FAN_SPEED,
        RETURN_FAN_STATUS,
    ] {
        assert!(
            paths.contains(&output.to_string()),
            "missing facade output {output}"
        );
    }
    assert_eq!(
        paths
            .iter()
            .filter(|path| path.as_str() == SUPPLY_FAN_ON)
            .count(),
        1,
        "u1SupFan should expose one logical facade input while fanning out internally"
    );
    let output_count = engine
        .io()
        .iter()
        .filter(|point| point.direction == PointDirection::Out)
        .count();
    assert_eq!(
        output_count, 22,
        "each active block should expose one output"
    );

    let (schedule_a, metrics_a) = simulate(engine);
    let (schedule_b, metrics_b) = simulate(load_return_fan_direct_pressure());
    let expected = expected_trace();

    assert_eq!(
        schedule_a, schedule_b,
        "ReturnFanDirectPressure schedule is deterministic"
    );
    assert_trace_bit_eq(&metrics_a, &metrics_b);
    assert_real_bits(
        &real_column(&metrics_a, AVERAGED_PRESSURE),
        &expected.averaged_pressure,
        "averaged building pressure",
    );
    assert_real_bits(
        &real_column(&metrics_a, RELIEF_DAMPER_COMMAND),
        &expected.relief_damper_command,
        "relief damper command",
    );
    assert_real_bits(
        &real_column(&metrics_a, DISCHARGE_PRESSURE_SETPOINT),
        &expected.discharge_pressure_setpoint,
        "discharge pressure setpoint",
    );
    assert_real_bits(
        &real_column(&metrics_a, RETURN_FAN_SPEED),
        &expected.return_fan_speed,
        "return fan speed",
    );
    assert_eq!(
        bool_column(&metrics_a, RETURN_FAN_STATUS),
        expected.return_fan_status,
        "return fan status diverged"
    );
}
