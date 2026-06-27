//! G36 MultiZone VAV ReturnFanDirectPressure sequence oracle.

use crate::oracle::{Golden, InputSeries, ValueKind};

use super::{RETURN_FAN_DIRECT_PRESSURE, b, buildings_line, input_b, input_r, r, sequence_golden, unit_ticks};

pub(super) fn goldens() -> Vec<Golden> {
    let time = unit_ticks(6);
    let building_pressure = [12.0, 9.009, 21.006, -21.012, 135.033, -264.06];
    let min_outdoor_air_damper = [true, false, true, true, true, true];
    let supply_fan_status = [false, true, true, true, true, true];
    let trace = return_fan_direct_pressure_trace(
        &time,
        &building_pressure,
        &min_outdoor_air_damper,
        &supply_fan_status,
    );
    let inputs = return_fan_direct_pressure_inputs(
        &building_pressure,
        &min_outdoor_air_damper,
        &supply_fan_status,
    );

    vec![
        sequence_golden(
            RETURN_FAN_DIRECT_PRESSURE,
            "averaged_building_pressure",
            ValueKind::Real,
            time.clone(),
            trace.averaged_pressure.into_iter().map(r).collect(),
            "ReturnFanDirectPressure: building_pressure chosen so MovingAverage(delta=300) emits 0, 9, 15, 3, 36, and -24 Pa over 1s ticks",
            "Pinned ReturnFanDirectPressure.mo: movMea.y feeds yDpBui and the normalized pressure input div.y=movMea.y/dpBuiSet",
            inputs.clone(),
        ),
        sequence_golden(
            RETURN_FAN_DIRECT_PRESSURE,
            "relief_damper_command",
            ValueKind::Real,
            time.clone(),
            trace.relief_damper_command.into_iter().map(r).collect(),
            "ReturnFanDirectPressure damper path: fan-off row, min-OA disabled row, interior modulation, and default Line clamps",
            "Pinned source: yRelDam = Switch(Line(conP.y, x1=0, f1=0, x2=0.5, f2=1), u1MinOutAirDam AND u1SupFan, 0)",
            inputs.clone(),
        ),
        sequence_golden(
            RETURN_FAN_DIRECT_PRESSURE,
            "discharge_pressure_setpoint",
            ValueKind::Real,
            time.clone(),
            trace.discharge_pressure_setpoint.into_iter().map(r).collect(),
            "ReturnFanDirectPressure pressure setpoint path: supply-fan switch plus lower, interior, and upper Line clamp rows",
            "Pinned source: dpDisSet = Switch(Line(conP.y, x1=0.5, f1=2.4, x2=1, f2=40), u1SupFan, 0)",
            inputs.clone(),
        ),
        sequence_golden(
            RETURN_FAN_DIRECT_PRESSURE,
            "return_fan_speed",
            ValueKind::Real,
            time.clone(),
            trace.return_fan_speed.into_iter().map(r).collect(),
            "ReturnFanDirectPressure fan-speed path: supply-fan switch plus parent-default disSpe_min=0.1 and disSpe_max=1",
            "Pinned source plus parent Controller defaults: yRetFan = Switch(Line(dpDisSetPre, x1=2.4, f1=0.1, x2=40, f2=1), u1SupFan, 0)",
            inputs.clone(),
        ),
        sequence_golden(
            RETURN_FAN_DIRECT_PRESSURE,
            "return_fan_status",
            ValueKind::Boolean,
            time,
            trace.return_fan_status.into_iter().map(b).collect(),
            "ReturnFanDirectPressure status path: y1RetFan is the source boundary alias of u1SupFan",
            "Pinned ReturnFanDirectPressure.mo connects u1SupFan directly to y1RetFan; the checked-in explicit-CXF fixture uses a pure Boolean identity bridge because OCE exposes boundary outputs through block output connectors",
            inputs,
        ),
    ]
}

#[derive(Default)]
struct ReturnFanDirectPressureTrace {
    averaged_pressure: Vec<f64>,
    relief_damper_command: Vec<f64>,
    discharge_pressure_setpoint: Vec<f64>,
    return_fan_speed: Vec<f64>,
    return_fan_status: Vec<bool>,
}

fn return_fan_direct_pressure_trace(
    time: &[f64],
    building_pressure: &[f64],
    min_outdoor_air_damper: &[bool],
    supply_fan_status: &[bool],
) -> ReturnFanDirectPressureTrace {
    const DP_BUI_SET: f64 = 12.0;
    const K: f64 = 1.0;
    const TI: f64 = 0.5;
    const NI: f64 = 0.9;
    const Y_MIN: f64 = 0.0;
    const Y_MAX: f64 = 1.0;

    let mut moving_average = MovingAverageState::default();
    let mut integral = 0.0;
    let mut previous_time: Option<f64> = None;
    let mut trace = ReturnFanDirectPressureTrace::default();

    for (((&t, &pressure), &min_oa), &fan_on) in time
        .iter()
        .zip(building_pressure)
        .zip(min_outdoor_air_damper)
        .zip(supply_fan_status)
    {
        let average = moving_average.output(t, pressure);
        let normalized_pressure = average / DP_BUI_SET;
        let error = 1.0 - normalized_pressure;
        let proportional = K * error;
        let unlimited = proportional + integral;
        let pid = unlimited.clamp(Y_MIN, Y_MAX);

        let relief_damper_line = buildings_line(0.0, 0.0, 0.5, 1.0, pid);
        let return_fan_pressure_setpoint = buildings_line(0.5, 2.4, 1.0, 40.0, pid);
        let return_fan_speed_line = buildings_line(
            2.4,
            0.1,
            40.0,
            1.0,
            return_fan_pressure_setpoint,
        );

        trace.averaged_pressure.push(average);
        trace
            .relief_damper_command
            .push(if min_oa && fan_on { relief_damper_line } else { 0.0 });
        trace.discharge_pressure_setpoint.push(if fan_on {
            return_fan_pressure_setpoint
        } else {
            0.0
        });
        trace.return_fan_speed.push(if fan_on {
            return_fan_speed_line
        } else {
            0.0
        });
        trace.return_fan_status.push(fan_on);

        let dt = previous_time.map_or(0.0, |previous| (t - previous).max(0.0));
        let anti_windup_gain = (unlimited - pid) / (K * NI);
        let corrected_error = error - anti_windup_gain;
        integral += (K / TI) * corrected_error * dt;
        previous_time = Some(t);
        moving_average.update(t, pressure);
    }

    trace
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

fn return_fan_direct_pressure_inputs(
    building_pressure: &[f64],
    min_outdoor_air_damper: &[bool],
    supply_fan_status: &[bool],
) -> Vec<InputSeries> {
    vec![
        input_r("building_pressure", building_pressure.iter().copied()),
        input_b(
            "min_outdoor_air_damper",
            min_outdoor_air_damper.iter().copied(),
        ),
        input_b("supply_fan_status", supply_fan_status.iter().copied()),
    ]
}
