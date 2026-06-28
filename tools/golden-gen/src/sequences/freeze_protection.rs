//! G36 MultiZone VAV FreezeProtection source-default sequence oracle.

use crate::oracle::{Golden, InputSeries, ValueKind};

use super::{FREEZE_PROTECTION, b, i, input_b, input_r, r, sequence_golden};

const SAMPLE_STEP_SECONDS: u32 = 60;
const SAMPLE_STEP: f64 = SAMPLE_STEP_SECONDS as f64;
const T_STOP: u32 = 6600;

const OUTPUTS: [(&str, ValueKind); 11] = [
    ("freeze_protection_stage", ValueKind::Integer),
    ("chilled_water_pump_enable", ValueKind::Boolean),
    ("return_damper_command", ValueKind::Real),
    ("outdoor_damper_command", ValueKind::Real),
    ("minimum_outdoor_damper_command", ValueKind::Real),
    ("supply_fan_status", ValueKind::Boolean),
    ("supply_fan_speed", ValueKind::Real),
    ("cooling_coil_command", ValueKind::Real),
    ("heating_coil_command", ValueKind::Real),
    ("hot_water_plant_request", ValueKind::Integer),
    ("alarm_level", ValueKind::Integer),
];

pub(super) fn goldens() -> Vec<Golden> {
    let time = expected_times();
    let rows = input_rows(&time);
    let inputs = freeze_protection_inputs(&rows);
    let trace = freeze_protection_trace(&time);

    OUTPUTS
        .iter()
        .map(|&(signal, kind)| {
            sequence_golden(
                FREEZE_PROTECTION,
                signal,
                kind,
                time.clone(),
                samples(signal, &trace),
                "FreezeProtection source-default: staged low supply-air temperatures exercise stage 1, stage 2, fast stage 3, operator reset back to stage 2, stage-2 expiry back to stage 1, stage-1 clear, and the slow 900 s stage-3 path",
                "Pinned FreezeProtection.mo source-default graph: hysteretic temperature thresholds feed Timers, TrueFalseHold, Latches, switch priority ladders, and two PI heating-coil controllers with default reverseActing=true",
                inputs.clone(),
            )
        })
        .collect()
}

#[derive(Clone, Copy)]
struct Inputs {
    outdoor_damper_min_position: f64,
    outdoor_damper: f64,
    heating_coil: f64,
    minimum_outdoor_damper: f64,
    return_damper: f64,
    supply_air_temperature: f64,
    software_reset: bool,
    supply_fan_status: bool,
    supply_fan_speed: f64,
    cooling_coil: f64,
    mixed_air_temperature: f64,
}

#[derive(Default)]
struct Trace {
    freeze_protection_stage: Vec<i64>,
    chilled_water_pump_enable: Vec<bool>,
    return_damper_command: Vec<f64>,
    outdoor_damper_command: Vec<f64>,
    minimum_outdoor_damper_command: Vec<f64>,
    supply_fan_status: Vec<bool>,
    supply_fan_speed: Vec<f64>,
    cooling_coil_command: Vec<f64>,
    heating_coil_command: Vec<f64>,
    hot_water_plant_request: Vec<i64>,
    alarm_level: Vec<i64>,
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
    Inputs {
        outdoor_damper_min_position: 0.20,
        outdoor_damper: 0.62,
        heating_coil: 0.31,
        minimum_outdoor_damper: 0.17,
        return_damper: 0.73,
        supply_air_temperature: supply_air_temperature(t),
        software_reset: (t == 1980.0) || (t == 6360.0),
        supply_fan_status: true,
        supply_fan_speed: 0.58,
        cooling_coil: 0.44,
        mixed_air_temperature: mixed_air_temperature(t),
    }
}

fn supply_air_temperature(t: f64) -> f64 {
    let celsius = if t < 60.0 {
        8.0
    } else if t < 600.0 {
        4.0
    } else if t < 960.0 {
        7.2
    } else if t < 1500.0 {
        3.0
    } else if t < 1860.0 {
        0.5
    } else if t < 2040.0 {
        7.2
    } else if t < 4920.0 {
        5.5
    } else if t < 5400.0 {
        7.2
    } else if t < 6360.0 {
        2.0
    } else {
        7.2
    };
    273.15 + celsius
}

fn mixed_air_temperature(t: f64) -> f64 {
    if (1800.0..1920.0).contains(&t) {
        299.75
    } else if (1920.0..2040.0).contains(&t) || (6300.0..6420.0).contains(&t) {
        300.50
    } else {
        300.15
    }
}

fn freeze_protection_inputs(rows: &[Inputs]) -> Vec<InputSeries> {
    vec![
        input_r(
            "outdoor_damper_min_position",
            rows.iter().map(|row| row.outdoor_damper_min_position),
        ),
        input_r("outdoor_damper", rows.iter().map(|row| row.outdoor_damper)),
        input_r("heating_coil", rows.iter().map(|row| row.heating_coil)),
        input_r(
            "minimum_outdoor_damper",
            rows.iter().map(|row| row.minimum_outdoor_damper),
        ),
        input_r("return_damper", rows.iter().map(|row| row.return_damper)),
        input_r(
            "supply_air_temperature",
            rows.iter().map(|row| row.supply_air_temperature),
        ),
        input_b(
            "software_reset",
            rows.iter().map(|row| row.software_reset),
        ),
        input_b(
            "supply_fan_status_input",
            rows.iter().map(|row| row.supply_fan_status),
        ),
        input_r(
            "supply_fan_speed_input",
            rows.iter().map(|row| row.supply_fan_speed),
        ),
        input_r("cooling_coil", rows.iter().map(|row| row.cooling_coil)),
        input_r(
            "mixed_air_temperature",
            rows.iter().map(|row| row.mixed_air_temperature),
        ),
    ]
}

fn freeze_protection_trace(time: &[f64]) -> Trace {
    let mut les_thr = ThresholdState::less(277.55, 0.25);
    let mut tim = TimerState::new(300.0);
    let mut hea_coil_stage_1 = PidState::default();
    let mut gre_thr = ThresholdState::greater(280.15, 0.25);
    let mut lat = LatchState::default();
    let mut tim1 = TimerState::new(300.0);
    let mut end_sta_one = EdgeState::default();
    let mut les_thr1 = ThresholdState::less(276.45, 0.25);
    let mut tim2 = TimerState::new(300.0);
    let mut hol_sta2 = TrueFalseHoldState::new(3600.0, 0.0);
    let mut tim3 = TimerState::new(900.0);
    let mut les_thr2 = ThresholdState::less(274.15, 0.25);
    let mut tim4 = TimerState::new(300.0);
    let mut lat1 = LatchState::default();
    let mut hea_coil_stage_3 = PidState::default();
    let mut tim5 = TimerState::new(3600.0);
    let mut lat2 = LatchState::default();
    let mut end_sta_two = EdgeState::default();
    let mut trace = Trace::default();

    for &t in time {
        let row = input_row(t);

        let les_thr_y = les_thr.output(row.supply_air_temperature);
        let tim_passed = tim.output(t, les_thr_y).passed;
        let gre_thr_y = gre_thr.output(row.supply_air_temperature);
        let tim1_passed = tim1.output(t, gre_thr_y).passed;
        let end_sta_one_y = end_sta_one.output(tim1_passed);

        let les_thr1_y = les_thr1.output(row.supply_air_temperature);
        let tim2_passed = tim2.output(t, les_thr1_y).passed;
        let hol_sta2_y = hol_sta2.output(t, tim2_passed);
        let tim3_passed = tim3.output(t, les_thr1_y).passed;

        let les_thr2_y = les_thr2.output(row.supply_air_temperature);
        let tim4_passed = tim4.output(t, les_thr2_y).passed;
        let or7_y = tim3_passed || tim4_passed;
        let or8_y = or7_y || false;
        let lat1_y = lat1.output(or8_y, row.software_reset);

        let tim5_passed = tim5.output(t, hol_sta2_y).passed;
        let end_sta_two_y = end_sta_two.output(tim5_passed);
        let lat2_y = lat2.output(hol_sta2_y, end_sta_two_y);

        let or2_y = tim_passed || end_sta_two_y;
        let lat_y = lat.output(or2_y, end_sta_one_y);

        let stage_1_heat = hea_coil_stage_1.output(279.15, row.supply_air_temperature);
        let stage_3_measurement = row.supply_air_temperature.max(row.mixed_air_temperature);
        let stage_3_heat = hea_coil_stage_3.output(300.15, stage_3_measurement);

        let hot_water_stage_1 = if lat_y { 2 } else { 0 };
        let minimum_ventilation = if lat_y {
            row.outdoor_damper_min_position
        } else {
            row.outdoor_damper
        };
        let heating_stage_1 = if lat_y {
            stage_1_heat
        } else {
            row.heating_coil
        };
        let outdoor_stage_2 = if lat2_y { 0.0 } else { minimum_ventilation };
        let minimum_outdoor_stage_2 = if lat2_y {
            0.0
        } else {
            row.minimum_outdoor_damper
        };
        let return_stage_2 = if lat2_y { 1.0 } else { row.return_damper };
        let alarm_stage_2 = if lat2_y { 3 } else { 0 };
        let supply_fan_speed = if lat1_y { 0.0 } else { row.supply_fan_speed };
        let outdoor_damper = if lat1_y { 0.0 } else { outdoor_stage_2 };
        let cooling_coil = if lat1_y { 1.0 } else { row.cooling_coil };
        let hot_water_request = if lat1_y { 2 } else { hot_water_stage_1 };
        let heating_coil = if lat1_y {
            stage_3_heat
        } else {
            heating_stage_1
        };
        let alarm_level = if lat1_y { 2 } else { alarm_stage_2 };
        let minimum_outdoor_damper = if lat1_y {
            0.0
        } else {
            minimum_outdoor_stage_2
        };
        let return_damper = if lat1_y { 0.0 } else { return_stage_2 };
        let supply_fan_status = row.supply_fan_status && !lat1_y;
        let freeze_stage = if lat1_y {
            3
        } else if lat2_y {
            2
        } else if lat_y {
            1
        } else {
            0
        };

        trace.freeze_protection_stage.push(freeze_stage);
        trace.chilled_water_pump_enable.push(lat1_y);
        trace.return_damper_command.push(return_damper);
        trace.outdoor_damper_command.push(outdoor_damper);
        trace
            .minimum_outdoor_damper_command
            .push(minimum_outdoor_damper);
        trace.supply_fan_status.push(supply_fan_status);
        trace.supply_fan_speed.push(supply_fan_speed);
        trace.cooling_coil_command.push(cooling_coil);
        trace.heating_coil_command.push(heating_coil);
        trace.hot_water_plant_request.push(hot_water_request);
        trace.alarm_level.push(alarm_level);

        les_thr.update(row.supply_air_temperature);
        tim.update(t, les_thr_y);
        hea_coil_stage_1.update(t, 279.15, row.supply_air_temperature);
        gre_thr.update(row.supply_air_temperature);
        lat.update(or2_y, end_sta_one_y);
        tim1.update(t, gre_thr_y);
        end_sta_one.update(tim1_passed);
        les_thr1.update(row.supply_air_temperature);
        tim2.update(t, les_thr1_y);
        hol_sta2.update(t, tim2_passed);
        tim3.update(t, les_thr1_y);
        les_thr2.update(row.supply_air_temperature);
        tim4.update(t, les_thr2_y);
        lat1.update(or8_y, row.software_reset);
        hea_coil_stage_3.update(t, 300.15, stage_3_measurement);
        tim5.update(t, hol_sta2_y);
        lat2.update(hol_sta2_y, end_sta_two_y);
        end_sta_two.update(tim5_passed);
    }

    trace
}

fn samples(signal: &str, trace: &Trace) -> Vec<crate::oracle::Sample> {
    match signal {
        "freeze_protection_stage" => trace
            .freeze_protection_stage
            .iter()
            .copied()
            .map(i)
            .collect(),
        "chilled_water_pump_enable" => trace
            .chilled_water_pump_enable
            .iter()
            .copied()
            .map(b)
            .collect(),
        "return_damper_command" => trace
            .return_damper_command
            .iter()
            .copied()
            .map(r)
            .collect(),
        "outdoor_damper_command" => trace
            .outdoor_damper_command
            .iter()
            .copied()
            .map(r)
            .collect(),
        "minimum_outdoor_damper_command" => trace
            .minimum_outdoor_damper_command
            .iter()
            .copied()
            .map(r)
            .collect(),
        "supply_fan_status" => trace.supply_fan_status.iter().copied().map(b).collect(),
        "supply_fan_speed" => trace.supply_fan_speed.iter().copied().map(r).collect(),
        "cooling_coil_command" => trace.cooling_coil_command.iter().copied().map(r).collect(),
        "heating_coil_command" => trace.heating_coil_command.iter().copied().map(r).collect(),
        "hot_water_plant_request" => trace
            .hot_water_plant_request
            .iter()
            .copied()
            .map(i)
            .collect(),
        "alarm_level" => trace.alarm_level.iter().copied().map(i).collect(),
        _ => unreachable!("unknown FreezeProtection signal {signal}"),
    }
}

struct ThresholdState {
    threshold: f64,
    hysteresis: f64,
    previous: bool,
    mode: ThresholdMode,
}

impl ThresholdState {
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
        let y = if !u || self.prev_t.is_none() || !self.prev_u {
            0.0
        } else {
            t - self.entry_time
        };
        TimerOutput {
            passed: u && y >= self.threshold,
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
struct EdgeState {
    previous: bool,
}

impl EdgeState {
    fn output(&self, u: bool) -> bool {
        u && !self.previous
    }

    fn update(&mut self, u: bool) {
        self.previous = u;
    }
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

#[derive(Default)]
struct TrueFalseHoldState {
    held: bool,
    timer: f64,
    prev_t: Option<f64>,
    true_hold_duration: f64,
    false_hold_duration: f64,
}

impl TrueFalseHoldState {
    fn new(true_hold_duration: f64, false_hold_duration: f64) -> Self {
        Self {
            true_hold_duration,
            false_hold_duration,
            ..Self::default()
        }
    }

    fn output(&self, t: f64, u: bool) -> bool {
        self.output_and_timer(t, u).0
    }

    fn update(&mut self, t: f64, u: bool) {
        let (y, timer) = self.output_and_timer(t, u);
        self.held = y;
        self.timer = timer;
        self.prev_t = Some(t);
    }

    fn output_and_timer(&self, t: f64, u: bool) -> (bool, f64) {
        let Some(previous_t) = self.prev_t else {
            return (u, 0.0);
        };
        let timer = self.timer + (t - previous_t).max(0.0);
        if u == self.held {
            (self.held, timer)
        } else {
            let required = if self.held {
                self.true_hold_duration.max(0.0)
            } else {
                self.false_hold_duration.max(0.0)
            };
            if timer >= required {
                (u, 0.0)
            } else {
                (self.held, timer)
            }
        }
    }
}

#[derive(Default)]
struct PidState {
    integral: f64,
    prev_t: Option<f64>,
}

impl PidState {
    const K: f64 = 1.0;
    const TI: f64 = 0.5;
    const NI: f64 = 0.9;
    const Y_MIN: f64 = 0.0;
    const Y_MAX: f64 = 1.0;

    fn output(&self, setpoint: f64, measurement: f64) -> f64 {
        let error = setpoint - measurement;
        let unlimited = Self::K * error + self.integral;
        unlimited.clamp(Self::Y_MIN, Self::Y_MAX)
    }

    fn update(&mut self, t: f64, setpoint: f64, measurement: f64) {
        let error = setpoint - measurement;
        let unlimited = Self::K * error + self.integral;
        let limited = unlimited.clamp(Self::Y_MIN, Self::Y_MAX);
        let dt = self.prev_t.map_or(0.0, |previous| (t - previous).max(0.0));
        let anti_windup_gain = (unlimited - limited) / (Self::K * Self::NI);
        let corrected_error = error - anti_windup_gain;
        self.integral += (Self::K / Self::TI) * corrected_error * dt;
        self.prev_t = Some(t);
    }
}
