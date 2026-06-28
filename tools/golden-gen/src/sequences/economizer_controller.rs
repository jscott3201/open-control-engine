//! G36 MultiZone VAV Economizers.Controller restricted sequence oracle.

use crate::oracle::{Golden, InputSeries, ValueKind};

use super::{
    ECONOMIZER_CONTROLLER_SINGLE_DAMPER_RELIEF_DAMPER_FIXED_21, b, buildings_line, clamp, input_b,
    input_i, input_r, r, sequence_golden,
};

pub(super) fn goldens() -> Vec<Golden> {
    let time = time_grid();
    let rows = input_trace(&time);
    let trace = economizer_controller_trace(&time, &rows);
    let inputs = economizer_controller_inputs(&rows);

    vec![
        sequence_golden(
            ECONOMIZER_CONTROLLER_SINGLE_DAMPER_RELIEF_DAMPER_FIXED_21,
            "outdoor_damper_min_limit",
            ValueKind::Real,
            time.clone(),
            trace.outdoor_damper_min_limit.iter().copied().map(r).collect(),
            "Economizers.Controller restricted variant: common-damper minimum-OA loop drives outdoor minimum limit while fan and mode rows toggle the loop enable",
            "Pinned Controller.mo restricted path minOADes=SingleDamper: damLim.yOutDam_min is exported directly as yOutDam_min and feeds Enable/Reliefs",
            inputs.clone(),
        ),
        sequence_golden(
            ECONOMIZER_CONTROLLER_SINGLE_DAMPER_RELIEF_DAMPER_FIXED_21,
            "minimum_outdoor_air_loop_enabled",
            ValueKind::Boolean,
            time.clone(),
            trace
                .minimum_outdoor_air_loop_enabled
                .iter()
                .copied()
                .map(b)
                .collect(),
            "Economizers.Controller restricted variant: yEnaMinOut follows Limits.Common fan proof and occupied-mode gate",
            "Pinned Controller.mo connects damLim.yEnaMinOut directly to yEnaMinOut; Enable high-limit state does not gate this status output",
            inputs.clone(),
        ),
        sequence_golden(
            ECONOMIZER_CONTROLLER_SINGLE_DAMPER_RELIEF_DAMPER_FIXED_21,
            "outdoor_damper_command",
            ValueKind::Real,
            time.clone(),
            trace.outdoor_damper_command.iter().copied().map(r).collect(),
            "Economizers.Controller restricted variant: yOutDam composes common-damper limits, fixed dry-bulb enable delays, and relief-damper modulation",
            "Pinned Controller.mo path: damLim limits -> enaDis max limits with TCut=294.15K -> modRel.yOutDam",
            inputs.clone(),
        ),
        sequence_golden(
            ECONOMIZER_CONTROLLER_SINGLE_DAMPER_RELIEF_DAMPER_FIXED_21,
            "return_damper_command",
            ValueKind::Real,
            time,
            trace.return_damper_command.iter().copied().map(r).collect(),
            "Economizers.Controller restricted variant: yRetDam composes common-damper return limits, enable return-damper delays, and relief-damper modulation",
            "Pinned Controller.mo path: damLim return limits -> enaDis return limits -> modRel.yRetDam",
            inputs,
        ),
    ]
}

#[derive(Clone, Copy)]
struct Row {
    outdoor_airflow_normalized: f64,
    minimum_outdoor_airflow_setpoint_normalized: f64,
    supply_temperature_signal: f64,
    outdoor_air_temperature: f64,
    supply_fan_status: bool,
    operation_mode: i64,
    freeze_protection_stage: i64,
}

#[derive(Default)]
struct DamperLimitsTrace {
    outdoor_damper_min_limit: Vec<f64>,
    outdoor_damper_max_limit: Vec<f64>,
    return_damper_min_limit: Vec<f64>,
    return_damper_max_limit: Vec<f64>,
    return_damper_physical_max_limit: Vec<f64>,
    minimum_outdoor_air_loop_enabled: Vec<bool>,
}

#[derive(Default)]
struct EnableTrace {
    outdoor_damper_max_limit: Vec<f64>,
    return_damper_min_limit: Vec<f64>,
    return_damper_max_limit: Vec<f64>,
}

#[derive(Default)]
struct ControllerTrace {
    outdoor_damper_min_limit: Vec<f64>,
    minimum_outdoor_air_loop_enabled: Vec<bool>,
    outdoor_damper_command: Vec<f64>,
    return_damper_command: Vec<f64>,
}

#[derive(Default)]
struct HysteresisState {
    previous: bool,
}

impl HysteresisState {
    fn tick(&mut self, u: f64) -> bool {
        let y = (!self.previous && u > 0.0) || (self.previous && u >= -1.0);
        self.previous = y;
        y
    }
}

#[derive(Default)]
struct TrueFalseHoldState {
    initialized: bool,
    held: bool,
    timer: f64,
    previous_time: f64,
}

impl TrueFalseHoldState {
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
struct TrueDelayState {
    initialized: bool,
    previous_input: bool,
    held_output: bool,
    timer: f64,
    previous_time: f64,
}

impl TrueDelayState {
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

fn time_grid() -> Vec<f64> {
    (0..24).map(|tick| f64::from(tick) * 60.0).collect()
}

fn input_trace(time: &[f64]) -> Vec<Row> {
    time.iter().map(|&t| row_at(t)).collect()
}

fn row_at(t: f64) -> Row {
    let tick = t as u32 / 60;
    const SUPPLY_TEMPERATURE_SIGNALS: [f64; 7] = [-0.5, -0.25, -0.125, 0.0, 0.125, 0.25, 0.5];

    Row {
        outdoor_airflow_normalized: 0.0,
        minimum_outdoor_airflow_setpoint_normalized: match tick {
            0..=5 => 0.20,
            6..=14 => 0.80,
            _ => 0.20,
        },
        supply_temperature_signal: SUPPLY_TEMPERATURE_SIGNALS[tick as usize % 7],
        outdoor_air_temperature: match tick {
            0 => 293.0,
            1..=15 => 295.0,
            16..=23 => 293.0,
            _ => unreachable!("unexpected test instant {t}"),
        },
        supply_fan_status: !matches!(tick, 11..=14),
        operation_mode: if matches!(tick, 20..=21) { 0 } else { 1 },
        freeze_protection_stage: if tick == 15 { 1 } else { 0 },
    }
}

fn economizer_controller_trace(time: &[f64], rows: &[Row]) -> ControllerTrace {
    let damper_limits = damper_limits_trace(time, rows);
    let enable = enable_trace(time, rows, &damper_limits);
    let (outdoor_damper_command, return_damper_command) =
        relief_modulation_trace(rows, &damper_limits, &enable);

    ControllerTrace {
        outdoor_damper_min_limit: damper_limits.outdoor_damper_min_limit,
        minimum_outdoor_air_loop_enabled: damper_limits.minimum_outdoor_air_loop_enabled,
        outdoor_damper_command,
        return_damper_command,
    }
}

fn damper_limits_trace(time: &[f64], rows: &[Row]) -> DamperLimitsTrace {
    const K: f64 = 1.0;
    const TI: f64 = 0.5;
    const NI: f64 = 0.9;
    const Y_MIN: f64 = 0.0;
    const Y_MAX: f64 = 1.0;
    const RESET: f64 = 0.0;
    const U_RET_DAM_MIN: f64 = 0.5;
    const RET_DAM_PHY_MAX: f64 = 1.0;
    const RET_DAM_PHY_MIN: f64 = 0.0;
    const OUT_DAM_PHY_MAX: f64 = 1.0;
    const OUT_DAM_PHY_MIN: f64 = 0.0;
    const OCCUPIED: i64 = 1;

    let mut integral = 0.0;
    let mut previous_time: Option<f64> = None;
    let mut previous_trigger = false;
    let mut trace = DamperLimitsTrace::default();

    for (&t, row) in time.iter().zip(rows) {
        let error =
            row.minimum_outdoor_airflow_setpoint_normalized - row.outdoor_airflow_normalized;
        let proportional = K * error;
        let unlimited = proportional + integral;
        let loop_signal = clamp(unlimited, Y_MIN, Y_MAX);

        let enabled = row.supply_fan_status && row.operation_mode == OCCUPIED;
        let disabled = !enabled;
        let outdoor_damper_max = if disabled {
            OUT_DAM_PHY_MIN
        } else {
            OUT_DAM_PHY_MAX
        };
        let return_damper_min = if disabled {
            RET_DAM_PHY_MAX
        } else {
            RET_DAM_PHY_MIN
        };

        trace.outdoor_damper_min_limit.push(buildings_line(
            Y_MIN,
            OUT_DAM_PHY_MIN,
            U_RET_DAM_MIN,
            outdoor_damper_max,
            loop_signal,
        ));
        trace.outdoor_damper_max_limit.push(outdoor_damper_max);
        trace.return_damper_min_limit.push(return_damper_min);
        trace.return_damper_max_limit.push(buildings_line(
            U_RET_DAM_MIN,
            RET_DAM_PHY_MAX,
            Y_MAX,
            return_damper_min,
            loop_signal,
        ));
        trace.return_damper_physical_max_limit.push(RET_DAM_PHY_MAX);
        trace.minimum_outdoor_air_loop_enabled.push(enabled);

        let dt = previous_time.map_or(0.0, |previous| (t - previous).max(0.0));
        if row.supply_fan_status && !previous_trigger {
            integral = RESET - proportional;
        } else {
            let anti_windup_gain = (unlimited - loop_signal) / (K * NI);
            let corrected_error = error - anti_windup_gain;
            integral += (K / TI) * corrected_error * dt;
        }
        previous_time = Some(t);
        previous_trigger = row.supply_fan_status;
    }

    trace
}

fn enable_trace(time: &[f64], rows: &[Row], damper_limits: &DamperLimitsTrace) -> EnableTrace {
    const OUTDOOR_AIR_CUTOFF: f64 = 294.15;

    let mut hysteresis = HysteresisState::default();
    let mut hold = TrueFalseHoldState::default();
    let mut outdoor_delay = TrueDelayState::default();
    let mut return_delay = TrueDelayState::default();
    let mut trace = EnableTrace::default();

    for (idx, (&t, row)) in time.iter().zip(rows).enumerate() {
        let dry_bulb_delta = row.outdoor_air_temperature - OUTDOOR_AIR_CUTOFF;
        let outdoor_air_condition = hysteresis.tick(dry_bulb_delta);
        let held_condition = hold.tick(t, outdoor_air_condition);
        let enabled =
            held_condition && row.supply_fan_status && row.freeze_protection_stage == 0;
        let disabled = !enabled;
        let outdoor_delay_done = outdoor_delay.tick(t, disabled, 15.0);
        let return_delay_done = return_delay.tick(t, disabled, 180.0);
        let close_outdoor_damper = disabled && outdoor_delay_done;
        let force_return_damper_physical = disabled && !return_delay_done;

        trace.outdoor_damper_max_limit.push(if close_outdoor_damper {
            damper_limits.outdoor_damper_min_limit[idx]
        } else {
            damper_limits.outdoor_damper_max_limit[idx]
        });
        trace.return_damper_max_limit.push(if force_return_damper_physical {
            damper_limits.return_damper_physical_max_limit[idx]
        } else {
            damper_limits.return_damper_max_limit[idx]
        });
        let return_min_base = if disabled {
            damper_limits.return_damper_max_limit[idx]
        } else {
            damper_limits.return_damper_min_limit[idx]
        };
        trace.return_damper_min_limit.push(if force_return_damper_physical {
            damper_limits.return_damper_physical_max_limit[idx]
        } else {
            return_min_base
        });
    }

    trace
}

fn relief_modulation_trace(
    rows: &[Row],
    damper_limits: &DamperLimitsTrace,
    enable: &EnableTrace,
) -> (Vec<f64>, Vec<f64>) {
    const U_MIN: f64 = -0.25;
    const U_MAX: f64 = 0.25;
    const U_OUT_DAM_MAX: f64 = 0.0;
    const U_RET_DAM_MIN: f64 = 0.0;

    let mut outdoor_damper_command = Vec::with_capacity(rows.len());
    let mut return_damper_command = Vec::with_capacity(rows.len());

    for (idx, row) in rows.iter().enumerate() {
        let out_dam_pos = buildings_line(
            U_MIN,
            damper_limits.outdoor_damper_min_limit[idx],
            U_OUT_DAM_MAX,
            enable.outdoor_damper_max_limit[idx],
            row.supply_temperature_signal,
        );
        let ret_dam_pos = buildings_line(
            U_RET_DAM_MIN,
            enable.return_damper_max_limit[idx],
            U_MAX,
            enable.return_damper_min_limit[idx],
            row.supply_temperature_signal,
        );
        outdoor_damper_command.push(out_dam_pos.min(enable.outdoor_damper_max_limit[idx]));
        return_damper_command.push(ret_dam_pos.max(enable.return_damper_min_limit[idx]));
    }

    (outdoor_damper_command, return_damper_command)
}

fn economizer_controller_inputs(rows: &[Row]) -> Vec<InputSeries> {
    vec![
        input_r(
            "outdoor_airflow_normalized",
            rows.iter().map(|row| row.outdoor_airflow_normalized),
        ),
        input_r(
            "minimum_outdoor_airflow_setpoint_normalized",
            rows.iter()
                .map(|row| row.minimum_outdoor_airflow_setpoint_normalized),
        ),
        input_r(
            "supply_temperature_signal",
            rows.iter().map(|row| row.supply_temperature_signal),
        ),
        input_r(
            "outdoor_air_temperature",
            rows.iter().map(|row| row.outdoor_air_temperature),
        ),
        input_b(
            "supply_fan_status",
            rows.iter().map(|row| row.supply_fan_status),
        ),
        input_i("operation_mode", rows.iter().map(|row| row.operation_mode)),
        input_i(
            "freeze_protection_stage",
            rows.iter().map(|row| row.freeze_protection_stage),
        ),
    ]
}
