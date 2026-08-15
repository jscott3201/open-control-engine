//! G36 MultiZone VAV ReliefFanGroup HostTick v1 profile reference.
//!
//! The stage-up and stage-down loops contain `CDL.Logical.Pre`; this recurrence models those blocks
//! as one-transition HostTick memories. It is independent of `oce-blocks`, but it is not a Modelica
//! event-iteration oracle.

use crate::oracle::{Golden, InputSeries, ValueKind};

use super::{RELIEF_FAN_GROUP, input_b, input_i, input_r, r, sequence_golden};

const SAMPLE_STEP_SECONDS: u32 = 15;
const SAMPLE_STEP: f64 = SAMPLE_STEP_SECONDS as f64;
const T_STOP: u32 = 2400;

const SPEED_SIGNALS: [&str; 4] = [
    "relief_fan_1_speed",
    "relief_fan_2_speed",
    "relief_fan_3_speed",
    "relief_fan_4_speed",
];
const DAMPER_SIGNALS: [&str; 4] = [
    "relief_damper_1_command",
    "relief_damper_2_command",
    "relief_damper_3_command",
    "relief_damper_4_command",
];

pub(super) fn goldens() -> Vec<Golden> {
    let time = expected_times();
    let rows = input_rows(&time);
    let inputs = relief_fan_group_inputs(&rows);
    let trace = relief_fan_group_trace(&time);

    let mut out = Vec::with_capacity(9);
    out.push(sequence_golden(
        RELIEF_FAN_GROUP,
        "averaged_building_pressure",
        ValueKind::Real,
        time.clone(),
        trace.averaged_pressure.iter().copied().map(r).collect(),
        "ReliefFanGroup source-default: supply fans on from 300s to 2220s; dpBui high from 300s to 1620s; fan proofs and one level-2 alarm exercise staging and damper guards",
        "Pinned ReliefFanGroup.mo: MovingAverage(delta=300) feeds yDpBui and the P-only normalized pressure controller input",
        inputs.clone(),
    ));

    for index in 0..4 {
        out.push(sequence_golden(
            RELIEF_FAN_GROUP,
            SPEED_SIGNALS[index],
            ValueKind::Real,
            time.clone(),
            trace.fan_speed[index].iter().copied().map(r).collect(),
            "ReliefFanGroup staged fan speed: source-default relFanMat, staVec, 420s stage-up timer, 300s stage-down timer, proof feedback, and 2s TrueDelay acknowledgement",
            "Pinned ReliefFanGroup.mo: yRelFan[i] = logSwi[i] * Limiter(conP.y, relFanSpe_min, 1) gated by enabled relief fans",
            inputs.clone(),
        ));
    }

    for index in 0..4 {
        out.push(sequence_golden(
            RELIEF_FAN_GROUP,
            DAMPER_SIGNALS[index],
            ValueKind::Real,
            time.clone(),
            trace.damper[index].iter().copied().map(r).collect(),
            "ReliefFanGroup damper command: stage-zero damper enable, staged fan command fallback, and fan-1 level-2 alarm while unproven",
            "Pinned ReliefFanGroup.mo: yDam[i] selects the staged command or stage-zero damper enable, then forces zero when uRelFanAla[i] == 2 and u1RelFan[i] is false",
            inputs.clone(),
        ));
    }

    out
}

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

fn expected_times() -> Vec<f64> {
    (0..=T_STOP / SAMPLE_STEP_SECONDS)
        .map(|tick| f64::from(tick) * SAMPLE_STEP)
        .collect()
}

fn input_rows(time: &[f64]) -> Vec<Inputs> {
    time.iter().copied().map(input_row).collect()
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
        alarm: [
            if (360.0..600.0).contains(&t) { 2 } else { 0 },
            0,
            0,
            0,
        ],
        proof: [
            (1500.0..2100.0).contains(&t),
            false,
            (900.0..2100.0).contains(&t),
            false,
        ],
    }
}

fn relief_fan_group_inputs(rows: &[Inputs]) -> Vec<InputSeries> {
    vec![
        input_b("supply_fan_1_status", rows.iter().map(|row| row.supply[0])),
        input_b("supply_fan_2_status", rows.iter().map(|row| row.supply[1])),
        input_r("building_pressure", rows.iter().map(|row| row.pressure)),
        input_i("relief_fan_1_alarm", rows.iter().map(|row| row.alarm[0])),
        input_i("relief_fan_2_alarm", rows.iter().map(|row| row.alarm[1])),
        input_i("relief_fan_3_alarm", rows.iter().map(|row| row.alarm[2])),
        input_i("relief_fan_4_alarm", rows.iter().map(|row| row.alarm[3])),
        input_b("relief_fan_1_proof", rows.iter().map(|row| row.proof[0])),
        input_b("relief_fan_2_proof", rows.iter().map(|row| row.proof[1])),
        input_b("relief_fan_3_proof", rows.iter().map(|row| row.proof[2])),
        input_b("relief_fan_4_proof", rows.iter().map(|row| row.proof[3])),
    ]
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
        let swi = each4(|i| if les_thr1_y[i] { pro1[i] + 5.0 } else { pro1[i] });
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
        let swi1 = each4(|i| if les_thr4_y[i] { pro2[i] + 5.0 } else { pro2[i] });
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
        let log_swi3 = each4(|i| if all_fans_off { row.proof[i] } else { log_swi1[i] });
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
