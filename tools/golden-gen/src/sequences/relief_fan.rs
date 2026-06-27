//! G36 MultiZone VAV ReliefFan sequence oracle.

use crate::oracle::{Golden, InputSeries, ValueKind};

use super::{RELIEF_FAN, b, input_b, input_r, r, sequence_golden};

pub(super) fn goldens() -> Vec<Golden> {
    let time: Vec<f64> = (0..=29).map(|tick| f64::from(tick) * 60.0).collect();
    let building_pressure: Vec<f64> = time
        .iter()
        .map(|&t| {
            if t < 300.0 {
                12.0
            } else if t <= 1020.0 {
                18.0
            } else {
                12.0
            }
        })
        .collect();
    let supply_fan_status: Vec<bool> = time.iter().map(|&t| t >= 300.0).collect();
    let trace = relief_fan_trace(&time, &building_pressure, &supply_fan_status);
    let inputs = relief_fan_inputs(&building_pressure, &supply_fan_status);

    vec![
        sequence_golden(
            RELIEF_FAN,
            "averaged_building_pressure",
            ValueKind::Real,
            time.clone(),
            trace.averaged_pressure.into_iter().map(r).collect(),
            "ReliefFan: dpBui=12 Pa until 300s, 18 Pa until 1020s, then 12 Pa; u1SupFan turns true at 300s",
            "Pinned ReliefFan.mo: MovingAverage(delta=300) feeds yDpBui and the normalized pressure controller input",
            inputs.clone(),
        ),
        sequence_golden(
            RELIEF_FAN,
            "relief_damper_status",
            ValueKind::Boolean,
            time.clone(),
            trace.damper_status.iter().copied().map(b).collect(),
            "ReliefFan damper cone: high pressure opens through greThr/and2/lat; low controller output clears through lesThr/tim after 300s",
            "Pinned ReliefFan.mo: y1RelDam = lat.y OR relFan.y; lat is clear-dominant and tim.passed clears it after conP.y is near zero for 300s",
            inputs.clone(),
        ),
        sequence_golden(
            RELIEF_FAN,
            "relief_fan_status",
            ValueKind::Boolean,
            time.clone(),
            trace.fan_status.iter().copied().map(b).collect(),
            "ReliefFan fan staging: greThr2 must hold above relFanSpe_min+0.15 for 420s before fan start; lesThr3 below relFanSpe_min for 300s clears",
            "Pinned ReliefFan.mo: y1RelFan = u1SupFan AND lat1.y; lat1 set by upTim.passed and clear-dominant from dowTim.passed",
            inputs.clone(),
        ),
        sequence_golden(
            RELIEF_FAN,
            "relief_fan_speed",
            ValueKind::Real,
            time,
            trace.fan_speed.into_iter().map(r).collect(),
            "ReliefFan fan speed: BooleanToReal(y1RelFan) multiplies the P-controller signal",
            "Pinned ReliefFan.mo: yRelFan = conP.y * BooleanToReal(y1RelFan); conP is P-only reverseActing=false with y=clamp(avg(dpBui)/12 - 1, 0, 1)",
            inputs,
        ),
    ]
}

struct ReliefFanTrace {
    averaged_pressure: Vec<f64>,
    damper_status: Vec<bool>,
    fan_status: Vec<bool>,
    fan_speed: Vec<f64>,
}

fn relief_fan_trace(
    time: &[f64],
    building_pressure: &[f64],
    supply_fan_status: &[bool],
) -> ReliefFanTrace {
    const DP_BUI_SET: f64 = 12.0;
    const REL_FAN_SPEED_MIN: f64 = 0.1;
    const HYS: f64 = 0.005;

    let mut moving_average = MovingAverageState::default();
    let mut gre_thr = HystereticThreshold::greater(0.05, HYS);
    let mut les_thr = HystereticThreshold::less(0.005, HYS);
    let mut tim = TimerState::new(300.0);
    let mut lat = LatchState::default();
    let mut gre_thr2 = HystereticThreshold::greater(REL_FAN_SPEED_MIN + 0.15, HYS);
    let mut up_tim = TimerState::new(420.0);
    let mut les_thr3 = HystereticThreshold::less(REL_FAN_SPEED_MIN, HYS);
    let mut down_tim = TimerState::new(300.0);
    let mut lat1 = LatchState::default();

    let mut averaged_pressure = Vec::with_capacity(time.len());
    let mut damper_status = Vec::with_capacity(time.len());
    let mut fan_status = Vec::with_capacity(time.len());
    let mut fan_speed = Vec::with_capacity(time.len());

    for ((&t, &pressure), &supply_fan_on) in time.iter().zip(building_pressure).zip(supply_fan_status)
    {
        let average = moving_average.output(t, pressure);
        let controller = (average / DP_BUI_SET - 1.0).clamp(0.0, 1.0);

        let damper_open_request = gre_thr.output(controller);
        let damper_close_request = les_thr.output(controller);
        let damper_clear = tim.output(t, damper_close_request).passed;
        let damper_latch = lat.output(damper_open_request && supply_fan_on, damper_clear);

        let fan_start_request = gre_thr2.output(controller);
        let fan_set = up_tim.output(t, fan_start_request).passed;
        let fan_stop_request = les_thr3.output(controller);
        let fan_clear = down_tim.output(t, fan_stop_request).passed;
        let fan_latch = lat1.output(fan_set, fan_clear);
        let fan_on = supply_fan_on && fan_latch;
        let damper_on = damper_latch || fan_on;
        let speed = controller * if fan_on { 1.0 } else { 0.0 };

        averaged_pressure.push(average);
        damper_status.push(damper_on);
        fan_status.push(fan_on);
        fan_speed.push(speed);

        moving_average.update(t, pressure);
        gre_thr.update(controller);
        les_thr.update(controller);
        tim.update(t, damper_close_request);
        lat.update(damper_open_request && supply_fan_on, damper_clear);
        gre_thr2.update(controller);
        up_tim.update(t, fan_start_request);
        les_thr3.update(controller);
        down_tim.update(t, fan_stop_request);
        lat1.update(fan_set, fan_clear);
    }

    ReliefFanTrace {
        averaged_pressure,
        damper_status,
        fan_status,
        fan_speed,
    }
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
            return TimerOutput {
                y: 0.0,
                passed: false,
            };
        }
        let y = t - self.entry_time;
        TimerOutput {
            y,
            passed: y >= self.threshold,
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
    #[allow(dead_code)]
    y: f64,
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

fn relief_fan_inputs(building_pressure: &[f64], supply_fan_status: &[bool]) -> Vec<InputSeries> {
    vec![
        input_r("building_pressure", building_pressure.iter().copied()),
        input_b("supply_fan_status", supply_fan_status.iter().copied()),
    ]
}
