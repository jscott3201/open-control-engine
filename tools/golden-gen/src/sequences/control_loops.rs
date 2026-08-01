//! G36 ThermalZones ControlLoops source-verified sequence oracle.
//!
//! This independent recurrence follows pinned `ControlLoops.mo`: opposite-order hysteretic
//! enable comparisons drive two PI-with-reset controllers, then hysteretic near-zero detectors
//! and engine-faithful `TrueDelay(delayOnInit=false)` blocks gate each raw PID output through an
//! inverted Boolean-to-Real multiplier. The cooling controller is direct acting while the
//! heating controller is reverse acting. Both controllers keep updating while their downstream
//! output gates are closed.

use crate::oracle::{Golden, InputSeries, ValueKind};

use super::{
    THERMAL_ZONES_CONTROL_LOOPS, clamp, input_r, less_hysteretic, r, sequence_golden,
    true_delay_output,
};

const K: f64 = 0.1;
const TI: f64 = 900.0;
const ENABLE_HYSTERESIS: f64 = 0.25;
const LOOP_ZERO_THRESHOLD: f64 = 0.01;
const LOOP_ZERO_HYSTERESIS: f64 = 0.008;
const DISABLE_DELAY: f64 = 30.0;
const NI: f64 = 0.9;
const Y_RESET: f64 = 0.0;
const Y_MIN: f64 = 0.0;
const Y_MAX: f64 = 1.0;
const SAMPLE_STEP: f64 = 60.0;

#[derive(Clone, Copy)]
struct Row {
    cooling_setpoint: f64,
    zone_temperature: f64,
    heating_setpoint: f64,
}

const fn row(cooling_setpoint: f64, zone_temperature: f64, heating_setpoint: f64) -> Row {
    Row {
        cooling_setpoint,
        zone_temperature,
        heating_setpoint,
    }
}

const ROWS: [Row; 54] = [
    // Quiescent initialization: the zone lies between setpoints, so neither loop is enabled.
    row(297.15, 295.15, 293.15), // 0
    // CL1-CL3: cooling only; the direct-acting edge emits high, then reset is visible next tick.
    row(297.15, 317.15, 293.15), // 1
    row(297.15, 317.15, 293.15), // 2
    // CL2: constant positive direct-acting error produces a monotonic cooling ramp.
    row(297.15, 317.15, 293.15), // 3
    row(297.15, 317.15, 293.15), // 4
    row(297.15, 317.15, 293.15), // 5
    row(297.15, 317.15, 293.15), // 6
    row(297.15, 317.15, 293.15), // 7
    row(297.15, 317.15, 293.15), // 8
    row(297.15, 317.15, 293.15), // 9
    // CL8: sustained saturation makes limiter back-calculation observable on recovery.
    row(297.15, 317.15, 293.15), // 10
    row(297.15, 317.15, 293.15), // 11
    row(297.15, 317.15, 293.15), // 12
    row(297.15, 317.15, 293.15), // 13
    row(297.15, 317.15, 293.15), // 14
    row(297.15, 317.15, 293.15), // 15
    row(297.15, 317.15, 293.15), // 16
    row(297.15, 317.15, 293.15), // 17
    row(297.15, 317.15, 293.15), // 18
    row(297.15, 317.15, 293.15), // 19
    row(297.15, 317.15, 293.15), // 20
    row(297.15, 317.15, 293.15), // 21
    row(297.15, 317.15, 293.15), // 22
    row(297.15, 317.15, 293.15), // 23
    row(297.15, 317.15, 293.15), // 24
    row(297.15, 317.15, 293.15), // 25
    row(297.15, 317.15, 293.15), // 26
    row(297.15, 317.15, 293.15), // 27
    // CL4-CL6: cooling is disabled but the downstream gate preserves the still-positive raw PI.
    row(297.15, 295.15, 270.00), // 28
    row(297.15, 295.15, 270.00), // 29
    row(297.15, 295.15, 270.00), // 30
    row(297.15, 295.15, 270.00), // 31
    // Drive the live disabled PID below the near-zero threshold and let its 30-second gate close.
    row(297.15, 287.15, 270.00), // 32
    row(297.15, 287.15, 270.00), // 33
    row(297.15, 287.15, 270.00), // 34
    row(297.15, 287.15, 270.00), // 35
    row(297.15, 287.15, 270.00), // 36
    row(297.15, 287.15, 270.00), // 37
    // CL3/CL7: re-enable, then traverse the LessThreshold hold band and release boundary.
    row(297.15, 297.65, 270.00), // 38
    row(297.15, 297.70, 270.00), // 39
    row(297.15, 297.74, 270.00), // 40
    row(297.15, 297.77, 270.00), // 41
    // CL1/CL2: heating only; reverse-acting error is setpoint minus measured temperature.
    row(320.00, 283.15, 293.15), // 42
    row(320.00, 283.15, 293.15), // 43
    row(320.00, 283.15, 293.15), // 44
    row(320.00, 283.15, 293.15), // 45
    row(320.00, 283.15, 293.15), // 46
    row(320.00, 283.15, 293.15), // 47
    // CL3 plus drop-before-expiry: disable for one tick, then re-enable before TrueDelay emits.
    row(320.00, 310.00, 293.15), // 48
    row(320.00, 283.15, 293.15), // 49
    row(320.00, 283.15, 293.15), // 50
    row(320.00, 283.15, 293.15), // 51
    // CL1: inverted setpoint ordering deliberately makes both loops enabled at once.
    row(294.00, 295.00, 296.00), // 52
    // Return to neither enabled, retaining live hidden PID state behind the output gates.
    row(297.15, 295.15, 293.15), // 53
];

struct Trace {
    cooling_enable: Vec<bool>,
    heating_enable: Vec<bool>,
    cooling_raw: Vec<f64>,
    heating_raw: Vec<f64>,
    cooling_near_zero: Vec<bool>,
    heating_near_zero: Vec<bool>,
    cooling_delay: Vec<bool>,
    heating_delay: Vec<bool>,
    cooling_output: Vec<f64>,
    heating_output: Vec<f64>,
}

/// Build independent Tier-A goldens for both thermal-zone loop signals.
///
/// Time is seconds on a 60-second grid and temperatures are kelvin. Both dimensionless outputs
/// are clamped to `[0,1]`. The two lane-local PI recurrences use the block/engine defaults
/// `Ni=0.9`, `yMin=0`, and `yMax=1`; these are not fixture bindings. The schedule covers CL1-CL8:
/// opposite comparator orders and PID directions, rising-edge reset, live disabled integration,
/// both disable legs, inverted multiplier gates, the `[0.01,0.018)` near-zero hold band, and
/// saturation with limiter back-calculation.
pub(super) fn goldens() -> Vec<Golden> {
    let time: Vec<f64> = (0..ROWS.len())
        .map(|tick| tick as f64 * SAMPLE_STEP)
        .collect();
    let trace = control_loop_trace(&time);

    assert!(!trace.cooling_enable[0] && !trace.heating_enable[0]);
    assert!(trace.cooling_enable[1] && !trace.heating_enable[1]);
    assert!(!trace.cooling_enable[42] && trace.heating_enable[42]);
    assert!(trace.cooling_enable[52] && trace.heating_enable[52]);
    assert_eq!(trace.cooling_raw[2].to_bits(), 0.0f64.to_bits());
    assert!(trace.cooling_raw[3..10].windows(2).all(|pair| pair[0] < pair[1]));
    assert!(trace.cooling_raw[10..28]
        .iter()
        .filter(|&&value| value.to_bits() == Y_MAX.to_bits())
        .count()
        >= 3);
    assert!(trace.cooling_output[28..32].iter().all(|&value| value > 0.0));
    assert!(!trace.cooling_delay[32] && trace.cooling_delay[33]);
    assert!(trace.cooling_near_zero[40]);
    assert!((LOOP_ZERO_THRESHOLD..LOOP_ZERO_THRESHOLD + LOOP_ZERO_HYSTERESIS)
        .contains(&trace.cooling_raw[40]));
    assert!(!trace.cooling_near_zero[41]);
    assert!(trace.cooling_raw[41] >= LOOP_ZERO_THRESHOLD + LOOP_ZERO_HYSTERESIS);
    assert!(trace.heating_raw[43..48].windows(2).all(|pair| pair[0] < pair[1]));
    assert!(trace.heating_near_zero[48] && !trace.heating_delay[48]);
    assert!(!trace.heating_near_zero[49] && !trace.heating_delay[49]);

    let inputs = inputs();
    vec![
        sequence_golden(
            THERMAL_ZONES_CONTROL_LOOPS,
            "cooling_loop_signal",
            ValueKind::Real,
            time.clone(),
            trace.cooling_output.into_iter().map(r).collect(),
            "ControlLoops cooling: direct-acting PI covers reset, ramp, saturation/back-calculation, live disabled decay, delayed zero gate, threshold hold/release, and re-enable",
            "ControlLoops.mo lines 56-61 and 75-177: less(TCooSet,TZon), PIDWithReset reverseActing=false, LessThreshold(t=0.01,h=0.008), TrueDelay(30s,delayOnInit=false), and inverted multiply gate",
            inputs.clone(),
        ),
        sequence_golden(
            THERMAL_ZONES_CONTROL_LOOPS,
            "heating_loop_signal",
            ValueKind::Real,
            time,
            trace.heating_output.into_iter().map(r).collect(),
            "ControlLoops heating: reverse-acting PI covers independent reset/ramp, one-tick disable probe, re-enable, simultaneous loop enable, and live hidden state",
            "ControlLoops.mo lines 63-73 and 97-179: less(TZon,THeaSet), default reverse-acting PIDWithReset made explicit, LessThreshold/TrueDelay ladder, and inverted multiply gate",
            inputs,
        ),
    ]
}

fn control_loop_trace(time: &[f64]) -> Trace {
    let cooling_setpoint: Vec<f64> = ROWS.iter().map(|row| row.cooling_setpoint).collect();
    let zone_temperature: Vec<f64> = ROWS.iter().map(|row| row.zone_temperature).collect();
    let heating_setpoint: Vec<f64> = ROWS.iter().map(|row| row.heating_setpoint).collect();
    let cooling_enable = less_hysteretic(
        &cooling_setpoint,
        &zone_temperature,
        ENABLE_HYSTERESIS,
        false,
    );
    let heating_enable = less_hysteretic(
        &zone_temperature,
        &heating_setpoint,
        ENABLE_HYSTERESIS,
        false,
    );
    let cooling_raw = cooling_pid_direct(time, &cooling_enable);
    let heating_raw = heating_pid_reverse(time, &heating_enable);
    let threshold = vec![LOOP_ZERO_THRESHOLD; ROWS.len()];
    let cooling_near_zero = less_hysteretic(
        &cooling_raw,
        &threshold,
        LOOP_ZERO_HYSTERESIS,
        false,
    );
    let heating_near_zero = less_hysteretic(
        &heating_raw,
        &threshold,
        LOOP_ZERO_HYSTERESIS,
        false,
    );
    let cooling_delay = delayed_true(time, &cooling_near_zero);
    let heating_delay = delayed_true(time, &heating_near_zero);
    let cooling_output = cooling_raw
        .iter()
        .zip(&cooling_delay)
        .zip(&cooling_enable)
        .map(|((&pid, &delayed), &enabled)| pid * if delayed && !enabled { 0.0 } else { 1.0 })
        .collect();
    let heating_output = heating_raw
        .iter()
        .zip(&heating_delay)
        .zip(&heating_enable)
        .map(|((&pid, &delayed), &enabled)| pid * if delayed && !enabled { 0.0 } else { 1.0 })
        .collect();

    Trace {
        cooling_enable,
        heating_enable,
        cooling_raw,
        heating_raw,
        cooling_near_zero,
        heating_near_zero,
        cooling_delay,
        heating_delay,
        cooling_output,
        heating_output,
    }
}

fn cooling_pid_direct(time: &[f64], enable: &[bool]) -> Vec<f64> {
    let mut integrator = 0.0;
    let mut previous_time: Option<f64> = None;
    let mut previous_trigger = false;
    let mut outputs = Vec::with_capacity(ROWS.len());

    for ((&t, row), &trigger) in time.iter().zip(ROWS.iter()).zip(enable) {
        let error = row.zone_temperature - row.cooling_setpoint;
        let proportional = K * error;
        let unlimited = proportional + integrator;
        let limited = clamp(unlimited, Y_MIN, Y_MAX);
        outputs.push(limited);

        let dt = previous_time.map_or(0.0, |previous| (t - previous).max(0.0));
        integrator = if trigger && !previous_trigger {
            Y_RESET - proportional
        } else {
            let anti_windup_gain = (unlimited - limited) / (K * NI);
            integrator + (K / TI) * (error - anti_windup_gain) * dt
        };
        previous_time = Some(t);
        previous_trigger = trigger;
    }

    outputs
}

fn heating_pid_reverse(time: &[f64], enable: &[bool]) -> Vec<f64> {
    let mut integrator = 0.0;
    let mut previous_time: Option<f64> = None;
    let mut previous_trigger = false;
    let mut outputs = Vec::with_capacity(ROWS.len());

    for ((&t, row), &trigger) in time.iter().zip(ROWS.iter()).zip(enable) {
        let error = row.heating_setpoint - row.zone_temperature;
        let proportional = K * error;
        let unlimited = proportional + integrator;
        let limited = clamp(unlimited, Y_MIN, Y_MAX);
        outputs.push(limited);

        let dt = previous_time.map_or(0.0, |previous| (t - previous).max(0.0));
        integrator = if trigger && !previous_trigger {
            Y_RESET - proportional
        } else {
            let anti_windup_gain = (unlimited - limited) / (K * NI);
            integrator + (K / TI) * (error - anti_windup_gain) * dt
        };
        previous_time = Some(t);
        previous_trigger = trigger;
    }

    outputs
}

fn delayed_true(time: &[f64], input: &[bool]) -> Vec<bool> {
    let mut previous_time = None;
    let mut previous_input = false;
    let mut held = false;
    let mut timer = 0.0;
    let mut outputs = Vec::with_capacity(input.len());

    for (&t, &value) in time.iter().zip(input) {
        let (output, next_timer) = true_delay_output(
            t,
            value,
            DISABLE_DELAY,
            false,
            previous_time,
            previous_input,
            held,
            timer,
        );
        outputs.push(output);
        previous_time = Some(t);
        previous_input = value;
        held = output;
        timer = next_timer;
    }

    outputs
}

fn inputs() -> Vec<InputSeries> {
    vec![
        input_r(
            "cooling_setpoint",
            ROWS.iter().map(|row| row.cooling_setpoint),
        ),
        input_r(
            "zone_temperature",
            ROWS.iter().map(|row| row.zone_temperature),
        ),
        input_r(
            "heating_setpoint",
            ROWS.iter().map(|row| row.heating_setpoint),
        ),
    ]
}
