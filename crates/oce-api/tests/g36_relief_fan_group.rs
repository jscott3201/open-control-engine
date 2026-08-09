//! Source-verified ASHRAE G36 ReliefFanGroup through the frozen facade.

use oce_api::{CollectSpec, Engine, InputSource, SimMetrics, SimSpec, Value};

const RELIEF_FAN_GROUP: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/multizone_vav_relief_fan_group.jsonld");

const SUPPLY_FAN_1: &str =
    "http://example.org#g36.source.multizone_vav_relief_fan_group.u1SupFan_1";
const SUPPLY_FAN_2: &str =
    "http://example.org#g36.source.multizone_vav_relief_fan_group.u1SupFan_2";
const BUILDING_PRESSURE: &str =
    "http://example.org#g36.source.multizone_vav_relief_fan_group.dpBui";
const ALARM_1: &str = "http://example.org#g36.source.multizone_vav_relief_fan_group.uRelFanAla_1";
const ALARM_2: &str = "http://example.org#g36.source.multizone_vav_relief_fan_group.uRelFanAla_2";
const ALARM_3: &str = "http://example.org#g36.source.multizone_vav_relief_fan_group.uRelFanAla_3";
const ALARM_4: &str = "http://example.org#g36.source.multizone_vav_relief_fan_group.uRelFanAla_4";
const PROOF_1: &str = "http://example.org#g36.source.multizone_vav_relief_fan_group.u1RelFan_1";
const PROOF_2: &str = "http://example.org#g36.source.multizone_vav_relief_fan_group.u1RelFan_2";
const PROOF_3: &str = "http://example.org#g36.source.multizone_vav_relief_fan_group.u1RelFan_3";
const PROOF_4: &str = "http://example.org#g36.source.multizone_vav_relief_fan_group.u1RelFan_4";

const AVERAGED_PRESSURE: &str =
    "http://example.org#g36.source.multizone_vav_relief_fan_group.movMea.y";
const RELIEF_FAN_SPEEDS: [&str; 4] = [
    "http://example.org#g36.source.multizone_vav_relief_fan_group.pro3_1.y",
    "http://example.org#g36.source.multizone_vav_relief_fan_group.pro3_2.y",
    "http://example.org#g36.source.multizone_vav_relief_fan_group.pro3_3.y",
    "http://example.org#g36.source.multizone_vav_relief_fan_group.pro3_4.y",
];
const RELIEF_DAMPERS: [&str; 4] = [
    "http://example.org#g36.source.multizone_vav_relief_fan_group.mul_1.y",
    "http://example.org#g36.source.multizone_vav_relief_fan_group.mul_2.y",
    "http://example.org#g36.source.multizone_vav_relief_fan_group.mul_3.y",
    "http://example.org#g36.source.multizone_vav_relief_fan_group.mul_4.y",
];

const SAMPLE_STEP_SECONDS: u32 = 15;
const SAMPLE_STEP: f64 = SAMPLE_STEP_SECONDS as f64;
const T_STOP: u32 = 2400;
const ROWS: usize = T_STOP as usize / SAMPLE_STEP_SECONDS as usize + 1;

type ScheduleSignature = (Vec<u32>, Vec<u32>, Vec<u32>);

#[derive(Clone, Copy)]
struct Inputs {
    supply: [bool; 2],
    pressure: f64,
    alarm: [i64; 4],
    proof: [bool; 4],
}

#[derive(Clone)]
struct Trace {
    averaged_pressure: Vec<f64>,
    fan_speed: [Vec<f64>; 4],
    damper: [Vec<f64>; 4],
}

fn pair(path: &str, value: Value) -> (String, Value) {
    (path.to_string(), value)
}

fn input_row(t: f64) -> Inputs {
    let supply_enabled = (300.0..2220.0).contains(&t);
    Inputs {
        supply: [supply_enabled, supply_enabled],
        pressure: if (300.0..1620.0).contains(&t) {
            18.0
        } else {
            12.0
        },
        alarm: [if (360.0..600.0).contains(&t) { 2 } else { 0 }, 0, 0, 0],
        proof: [
            (1500.0..2100.0).contains(&t),
            false,
            (900.0..2100.0).contains(&t),
            false,
        ],
    }
}

fn relief_fan_group_inputs(t: f64) -> Vec<(String, Value)> {
    let row = input_row(t);
    vec![
        pair(SUPPLY_FAN_1, Value::Boolean(row.supply[0])),
        pair(SUPPLY_FAN_2, Value::Boolean(row.supply[1])),
        pair(BUILDING_PRESSURE, Value::Real(row.pressure)),
        pair(ALARM_1, Value::Integer(row.alarm[0])),
        pair(ALARM_2, Value::Integer(row.alarm[1])),
        pair(ALARM_3, Value::Integer(row.alarm[2])),
        pair(ALARM_4, Value::Integer(row.alarm[3])),
        pair(PROOF_1, Value::Boolean(row.proof[0])),
        pair(PROOF_2, Value::Boolean(row.proof[1])),
        pair(PROOF_3, Value::Boolean(row.proof[2])),
        pair(PROOF_4, Value::Boolean(row.proof[3])),
    ]
}

fn load_relief_fan_group() -> Engine {
    let mut engine = Engine::in_memory();
    let report = engine
        .load_cxf(RELIEF_FAN_GROUP.as_bytes())
        .expect("source-verified G36 ReliefFanGroup fixture loads");
    assert_eq!(report.block_count, 226);
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
    let mut points = Vec::with_capacity(9);
    points.push(AVERAGED_PRESSURE.to_string());
    points.extend(RELIEF_FAN_SPEEDS.iter().map(|point| (*point).to_string()));
    points.extend(RELIEF_DAMPERS.iter().map(|point| (*point).to_string()));
    let metrics = engine
        .simulate(&SimSpec {
            t_start: 0.0,
            t_stop: f64::from(T_STOP),
            step: SAMPLE_STEP,
            inputs: InputSource::Closure(Box::new(relief_fan_group_inputs)),
            collect: CollectSpec::Named { points, stride: 1 },
        })
        .expect("G36 ReliefFanGroup simulates");
    assert_eq!(metrics.ticks, ROWS as u64);
    assert_eq!(
        metrics
            .trace
            .times()
            .iter()
            .map(|t| t.to_bits())
            .collect::<Vec<_>>(),
        expected_times()
            .iter()
            .map(|t| t.to_bits())
            .collect::<Vec<_>>()
    );
    (schedule, metrics)
}

fn expected_times() -> Vec<f64> {
    (0..=T_STOP / SAMPLE_STEP_SECONDS)
        .map(|tick| f64::from(tick) * SAMPLE_STEP)
        .collect()
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
    assert_eq!(actual.len(), expected.len(), "{label} length diverged");
    for (row, (&left, &right)) in actual.iter().zip(expected).enumerate() {
        assert_eq!(
            left.to_bits(),
            right.to_bits(),
            "{label} row {row} diverged: actual={left:?} expected={right:?}"
        );
    }
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
fn multizone_vav_relief_fan_group_loads_simulates_and_is_deterministic() {
    let engine = load_relief_fan_group();
    let paths = engine
        .io()
        .iter()
        .map(|point| point.path.clone())
        .collect::<Vec<_>>();
    for input in [
        SUPPLY_FAN_1,
        SUPPLY_FAN_2,
        BUILDING_PRESSURE,
        ALARM_1,
        ALARM_2,
        ALARM_3,
        ALARM_4,
        PROOF_1,
        PROOF_2,
        PROOF_3,
        PROOF_4,
    ] {
        assert!(
            paths.contains(&input.to_string()),
            "missing facade input {input}"
        );
    }
    for output in RELIEF_FAN_SPEEDS
        .iter()
        .chain(RELIEF_DAMPERS.iter())
        .chain([AVERAGED_PRESSURE].iter())
    {
        assert!(
            paths.contains(&output.to_string()),
            "missing facade output {output}"
        );
    }

    let oracle = relief_fan_group_trace(&expected_times());
    let (schedule_a, metrics_a) = simulate(engine);
    let (schedule_b, metrics_b) = simulate(load_relief_fan_group());
    assert_eq!(
        schedule_a, schedule_b,
        "ReliefFanGroup schedule is deterministic"
    );
    assert_trace_bit_eq(&metrics_a, &metrics_b);
    assert_real_bits(
        &real_column(&metrics_a, AVERAGED_PRESSURE),
        &oracle.averaged_pressure,
        "averaged building pressure",
    );
    for index in 0..4 {
        assert_real_bits(
            &real_column(&metrics_a, RELIEF_FAN_SPEEDS[index]),
            &oracle.fan_speed[index],
            &format!("relief fan {} speed", index + 1),
        );
        assert_real_bits(
            &real_column(&metrics_a, RELIEF_DAMPERS[index]),
            &oracle.damper[index],
            &format!("relief damper {} command", index + 1),
        );
    }
}

fn relief_fan_group_trace(time: &[f64]) -> Trace {
    const DP_BUI_SET: f64 = 12.0;
    const REL_FAN_SPEED_MIN: f64 = 0.1;
    const HYS: f64 = 0.005;
    const STA_VEC: [f64; 4] = [2.0, 3.0, 1.0, 4.0];
    const REL_FAN_MAT: [[f64; 2]; 4] = [[1.0, 0.0], [1.0, 0.0], [0.0, 1.0], [0.0, 1.0]];

    let mut moving_average = MovingAverageState::default();
    let mut gre_thr = HystereticThreshold::greater(0.05, HYS);
    let mut les_thr = HystereticThreshold::less(0.005, HYS);
    let mut tim = TimerState::new(300.0);
    let mut lat = LatchState::default();
    let mut gre_thr1 = [(); 4].map(|_| HystereticThreshold::greater(0.5, 0.0));
    let mut gre_thr2 = HystereticThreshold::greater(REL_FAN_SPEED_MIN + 0.15, HYS);
    let mut up_tim = TimerState::new(420.0);
    let mut pre = PreState::new(true);
    let mut les_thr1 = [(); 4].map(|_| HystereticThreshold::less(0.5, 0.0));
    let mut les_thr2 = [(); 4].map(|_| HystereticThreshold::less(0.5, 0.0));
    let mut tru_del = [(); 4].map(|_| TrueDelayState::new(2.0));
    let mut lat2 = LatchState::default();
    let mut les_thr3 = HystereticThreshold::less(REL_FAN_SPEED_MIN, HYS);
    let mut dow_tim = TimerState::new(300.0);
    let mut pre1 = PreState::new(true);
    let mut les_thr4 = [(); 4].map(|_| HystereticThreshold::less(0.5, 0.0));
    let mut les_thr5 = [(); 4].map(|_| HystereticThreshold::less(0.5, 0.0));
    let mut tru_del1 = [(); 4].map(|_| TrueDelayState::new(2.0));
    let mut lat1 = LatchState::default();
    let mut gre_thr3 = [(); 4].map(|_| HystereticThreshold::greater(0.5, 0.0));

    let mut trace = Trace {
        averaged_pressure: Vec::with_capacity(time.len()),
        fan_speed: [(); 4].map(|_| Vec::with_capacity(time.len())),
        damper: [(); 4].map(|_| Vec::with_capacity(time.len())),
    };

    for &t in time {
        let row = input_row(t);
        let supply_real = row.supply.map(bool_to_real);
        let proof_real = row.proof.map(bool_to_real);
        let ena_rel = REL_FAN_MAT.map(|fan| fan[0] * supply_real[0] + fan[1] * supply_real[1]);
        let gai = each4(|i| ena_rel[i] * STA_VEC[i]);
        let average = moving_average.output(t, row.pressure);
        let controller = (average / DP_BUI_SET - 1.0).clamp(0.0, 1.0);
        let ena_rel_group = row.supply[0] || row.supply[1];
        let gated_controller =
            bool_to_real(ena_rel_group) * controller.clamp(REL_FAN_SPEED_MIN, 1.0);

        let damper_open_request = gre_thr.output(controller);
        let damper_close_request = les_thr.output(controller);
        let damper_clear = tim.output(t, damper_close_request).passed;
        let lat_y = lat.output(damper_open_request && ena_rel_group, damper_clear);
        let boo_rep = [lat_y; 4];
        let gre_thr1_y = each4(|i| gre_thr1[i].output(gai[i]));
        let ena_dam = each4(|i| gre_thr1_y[i] && boo_rep[i]);

        let sub2 = each4(|i| ena_rel[i] - proof_real[i]);
        let pro1 = each4(|i| gai[i] * sub2[i]);
        let all_fans_off = row.proof.iter().all(|proof| !proof);
        let les_thr1_y = each4(|i| les_thr1[i].output(pro1[i]));
        let swi = each4(|i| {
            if les_thr1_y[i] {
                pro1[i] + 5.0
            } else {
                pro1[i]
            }
        });
        let swi3 = each4(|i| if all_fans_off { pro1[i] } else { swi[i] });
        let next_stage_order = min4(swi3);
        let next_stage = each4(|i| les_thr2[i].output((gai[i] - next_stage_order).abs()));
        let or2 = each4(|i| next_stage[i] || row.proof[i]);
        let delayed_start = each4(|i| tru_del[i].output(t, or2[i]));
        let newly_started_proven = each4(|i| delayed_start[i] == row.proof[i]);
        let all_newly_started_proven = newly_started_proven.iter().all(|&value| value);

        let stage_up_request = gre_thr2.output(controller);
        let up_gate = stage_up_request && pre.output();
        let up_passed = up_tim.output(t, up_gate).passed;
        let lat2_y = lat2.output(up_passed, all_newly_started_proven);

        let pro2 = each4(|i| gai[i] * proof_real[i]);
        let les_thr4_y = each4(|i| les_thr4[i].output(pro2[i]));
        let swi1 = each4(|i| {
            if les_thr4_y[i] {
                pro2[i] + 5.0
            } else {
                pro2[i]
            }
        });
        let next_down_order = min4(swi1);
        let next_down = each4(|i| les_thr5[i].output((gai[i] - next_down_order).abs()));
        let xor = each4(|i| next_down[i] ^ row.proof[i]);
        let not5 = xor.map(|value| !value);
        let delayed_stop_not = each4(|i| tru_del1[i].output(t, not5[i]));
        let stopped_matches = each4(|i| row.proof[i] != delayed_stop_not[i]);
        let all_stopped_matches = stopped_matches.iter().all(|&value| value);

        let stage_down_request = les_thr3.output(controller);
        let down_gate = stage_down_request && pre1.output();
        let down_passed = dow_tim.output(t, down_gate).passed;
        let lat1_y = lat1.output(down_passed, all_stopped_matches);

        let log_swi1 = each4(|i| if lat1_y { xor[i] } else { row.proof[i] });
        let log_swi3 = each4(|i| {
            if all_fans_off {
                row.proof[i]
            } else {
                log_swi1[i]
            }
        });
        let log_swi = each4(|i| if lat2_y { or2[i] } else { log_swi3[i] });
        let any_enabled = each4(|i| gre_thr3[i].output(ena_rel[i]))
            .iter()
            .any(|&value| value);
        let speed_base = if any_enabled { gated_controller } else { 0.0 };
        let any_fan_commanded = log_swi.iter().any(|&value| value);
        let log_swi2 = each4(|i| {
            if any_fan_commanded {
                log_swi[i]
            } else {
                ena_dam[i]
            }
        });

        trace.averaged_pressure.push(average);
        for i in 0..4 {
            let alarm_off_guard = row.alarm[i] != 2 || row.proof[i];
            trace.fan_speed[i].push(speed_base * bool_to_real(log_swi[i]));
            trace.damper[i].push(bool_to_real(log_swi2[i]) * bool_to_real(alarm_off_guard));
        }

        moving_average.update(t, row.pressure);
        gre_thr.update(controller);
        les_thr.update(controller);
        tim.update(t, damper_close_request);
        lat.update(damper_open_request && ena_rel_group, damper_clear);
        for i in 0..4 {
            gre_thr1[i].update(gai[i]);
            les_thr1[i].update(pro1[i]);
            les_thr2[i].update((gai[i] - next_stage_order).abs());
            tru_del[i].update(t, or2[i]);
            les_thr4[i].update(pro2[i]);
            les_thr5[i].update((gai[i] - next_down_order).abs());
            tru_del1[i].update(t, not5[i]);
            gre_thr3[i].update(ena_rel[i]);
        }
        gre_thr2.update(controller);
        up_tim.update(t, up_gate);
        pre.update(!up_passed);
        lat2.update(up_passed, all_newly_started_proven);
        les_thr3.update(controller);
        dow_tim.update(t, down_gate);
        pre1.update(!down_passed);
        lat1.update(down_passed, all_stopped_matches);
    }

    trace
}

fn each4<T>(mut f: impl FnMut(usize) -> T) -> [T; 4] {
    [f(0), f(1), f(2), f(3)]
}

fn bool_to_real(value: bool) -> f64 {
    if value { 1.0 } else { 0.0 }
}

fn min4(values: [f64; 4]) -> f64 {
    values.into_iter().fold(f64::INFINITY, f64::min)
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

struct HystereticThreshold {
    threshold: f64,
    hysteresis: f64,
    previous: bool,
    mode: ThresholdMode,
}

impl HystereticThreshold {
    fn greater(threshold: f64, hysteresis: f64) -> Self {
        Self {
            threshold,
            hysteresis,
            previous: false,
            mode: ThresholdMode::Greater,
        }
    }

    fn less(threshold: f64, hysteresis: f64) -> Self {
        Self {
            threshold,
            hysteresis,
            previous: false,
            mode: ThresholdMode::Less,
        }
    }

    fn output(&self, input: f64) -> bool {
        match self.mode {
            ThresholdMode::Greater => {
                (!self.previous && input > self.threshold)
                    || (self.previous && input > self.threshold - self.hysteresis)
            }
            ThresholdMode::Less => {
                (!self.previous && input < self.threshold)
                    || (self.previous && input < self.threshold + self.hysteresis)
            }
        }
    }

    fn update(&mut self, input: f64) {
        self.previous = self.output(input);
    }
}

enum ThresholdMode {
    Greater,
    Less,
}

#[derive(Default)]
struct TimerState {
    threshold: f64,
    entry_time: f64,
    prev_t: Option<f64>,
    prev_u: bool,
}

impl TimerState {
    fn new(threshold: f64) -> Self {
        Self {
            threshold,
            ..Self::default()
        }
    }

    fn output(&self, t: f64, u: bool) -> TimerOutput {
        if !u || self.prev_t.is_none() || !self.prev_u {
            return TimerOutput { passed: false };
        }
        TimerOutput {
            passed: t - self.entry_time >= self.threshold,
        }
    }

    fn update(&mut self, t: f64, u: bool) {
        if u && (self.prev_t.is_none() || !self.prev_u) {
            self.entry_time = t;
        }
        self.prev_t = Some(t);
        self.prev_u = u;
    }
}

struct TimerOutput {
    passed: bool,
}

#[derive(Default)]
struct LatchState {
    held: bool,
    prev_u: bool,
}

impl LatchState {
    fn output(&self, u: bool, clear: bool) -> bool {
        !clear && ((u && !self.prev_u) || self.held)
    }

    fn update(&mut self, u: bool, clear: bool) {
        self.held = self.output(u, clear);
        self.prev_u = u;
    }
}

struct PreState {
    previous: bool,
}

impl PreState {
    fn new(y_start: bool) -> Self {
        Self { previous: y_start }
    }

    fn output(&self) -> bool {
        self.previous
    }

    fn update(&mut self, input: bool) {
        self.previous = input;
    }
}

#[derive(Default)]
struct TrueDelayState {
    delay_time: f64,
    entry_time: Option<f64>,
    previous_u: bool,
}

impl TrueDelayState {
    fn new(delay_time: f64) -> Self {
        Self {
            delay_time,
            ..Self::default()
        }
    }

    fn output(&self, t: f64, u: bool) -> bool {
        u && self
            .entry_time
            .is_some_and(|entry_time| t - entry_time >= self.delay_time)
    }

    fn update(&mut self, t: f64, u: bool) {
        if u && !self.previous_u {
            self.entry_time = Some(t);
        } else if !u {
            self.entry_time = None;
        }
        self.previous_u = u;
    }
}
