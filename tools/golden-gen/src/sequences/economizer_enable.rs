//! G36 MultiZone VAV Economizers.Subsequences.Enable restricted oracle.

use crate::oracle::{Golden, InputSeries, ValueKind};

use super::{ECONOMIZER_ENABLE, input_b, input_i, input_r, r, sequence_golden};

pub(super) fn goldens() -> Vec<Golden> {
    let time = time_grid();
    let inputs = input_trace(&time);
    let trace = economizer_enable_trace(&time, &inputs);
    let input_series = economizer_enable_inputs(&inputs);

    vec![
        sequence_golden(
            ECONOMIZER_ENABLE,
            "outdoor_damper_max_limit",
            ValueKind::Real,
            time.clone(),
            trace.outdoor_damper_max.into_iter().map(r).collect(),
            "Economizer Enable use_enthalpy=false: TOut steps below, above, then below cutoff; supply fan and freeze protection interrupt enabled state",
            "Pinned Enable.mo restricted path: hysOutTem(TOut-TOutCut, uLow=-delTOutHis, uHigh=0) OR false -> TrueFalseHold(600s/600s); disabled branch closes outdoor damper after TrueDelay(disDel=15s)",
            input_series.clone(),
        ),
        sequence_golden(
            ECONOMIZER_ENABLE,
            "return_damper_max_limit",
            ValueKind::Real,
            time.clone(),
            trace.return_damper_max.into_iter().map(r).collect(),
            "Economizer Enable return-damper maximum path over the same source pins",
            "Pinned Enable.mo restricted path: when disabled and retDamFulOpeTim has not elapsed, maxRetDamSwitch forces uRetDamPhy_max; otherwise yRetDam_max follows uRetDam_max",
            input_series.clone(),
        ),
        sequence_golden(
            ECONOMIZER_ENABLE,
            "return_damper_min_limit",
            ValueKind::Real,
            time,
            trace.return_damper_min.into_iter().map(r).collect(),
            "Economizer Enable return-damper minimum path over the same source pins",
            "Pinned Enable.mo restricted path: disabled retDamSwitch selects uRetDam_max until force-to-physical-max is active; enabled selects uRetDam_min",
            input_series,
        ),
    ]
}

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
struct EconomizerEnableTrace {
    outdoor_damper_max: Vec<f64>,
    return_damper_max: Vec<f64>,
    return_damper_min: Vec<f64>,
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

fn economizer_enable_trace(time: &[f64], inputs: &[Row]) -> EconomizerEnableTrace {
    let mut hysteresis = HysteresisState::default();
    let mut hold = TrueFalseHoldState::default();
    let mut outdoor_delay = TrueDelayState::default();
    let mut return_delay = TrueDelayState::default();
    let mut trace = EconomizerEnableTrace::default();

    for (&t, row) in time.iter().zip(inputs) {
        let dry_bulb_delta = row.outdoor_air_temperature - row.outdoor_air_cutoff;
        let outdoor_air_condition = hysteresis.tick(dry_bulb_delta);
        let held_condition = hold.tick(t, outdoor_air_condition);
        let enabled =
            held_condition && row.supply_fan_on && row.freeze_protection_stage == 0;
        let disabled = !enabled;
        let outdoor_delay_done = outdoor_delay.tick(t, disabled, 15.0);
        let return_delay_done = return_delay.tick(t, disabled, 180.0);
        let close_outdoor_damper = disabled && outdoor_delay_done;
        let force_return_damper_physical = disabled && !return_delay_done;

        trace.outdoor_damper_max.push(if close_outdoor_damper {
            row.outdoor_damper_min
        } else {
            row.outdoor_damper_max
        });
        trace.return_damper_max
            .push(if force_return_damper_physical {
                row.return_damper_physical_max
            } else {
                row.return_damper_max
            });
        trace.return_damper_min
            .push(if force_return_damper_physical {
                row.return_damper_physical_max
            } else if disabled {
                row.return_damper_max
            } else {
                row.return_damper_min
            });
    }

    trace
}

fn economizer_enable_inputs(rows: &[Row]) -> Vec<InputSeries> {
    vec![
        input_r(
            "outdoor_air_temperature",
            rows.iter().map(|row| row.outdoor_air_temperature),
        ),
        input_r(
            "outdoor_air_cutoff",
            rows.iter().map(|row| row.outdoor_air_cutoff),
        ),
        input_r(
            "outdoor_damper_min",
            rows.iter().map(|row| row.outdoor_damper_min),
        ),
        input_r(
            "outdoor_damper_max",
            rows.iter().map(|row| row.outdoor_damper_max),
        ),
        input_r(
            "return_damper_max",
            rows.iter().map(|row| row.return_damper_max),
        ),
        input_r(
            "return_damper_min",
            rows.iter().map(|row| row.return_damper_min),
        ),
        input_r(
            "return_damper_physical_max",
            rows.iter().map(|row| row.return_damper_physical_max),
        ),
        input_b("supply_fan_on", rows.iter().map(|row| row.supply_fan_on)),
        input_i(
            "freeze_protection_stage",
            rows.iter().map(|row| row.freeze_protection_stage),
        ),
    ]
}
