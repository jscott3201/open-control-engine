//! G36 CoolingOnly Alarms source-verified sequence oracle.
//!
//! This per-tick recurrence is independently derived from pinned `Alarms.mo`. The three alarm
//! ladders intentionally retain their different gates: occupancy gates low-airflow and sensor
//! alarms but not leaking-damper alarms; the 1,800-second fan-start delay gates only low airflow;
//! and the sensor/leak ladders require the fan off/on respectively. The schedule makes the leak
//! alarm fire while unoccupied and before fan arming, exercises exact occupied-mode equality on
//! both neighboring ordinals, and preserves the source's severity inversion (50% starvation is
//! level 2 while 70% starvation is level 3).
//!
//! `greThr.h=0.5*floHys` (source lines 91-94) and
//! `cloDam.h=0.5*damPosHys` (lines 195-198) are pre-grounded to `0.005` for the canonical
//! Validation bindings. `staPreMul=1` makes `greThr1` structurally true, so its suppression and
//! final multiply branches are output-unobservable under this fixture; the recurrence still
//! carries that multiplier. Assert blocks have no output connector and therefore add no column.

use crate::oracle::{Golden, InputSeries, ValueKind};

use super::{
    COOLING_ONLY_ALARMS, greater_hysteretic, hysteresis, i, input_b, input_i, input_r,
    less_hysteretic, sequence_golden, true_delay,
};

const OCCUPIED_MODE: i64 = 1;
const LOW_FLOW_TIME: f64 = 300.0;
const FAN_OFF_TIME: f64 = 600.0;
const LEAK_FLOW_TIME: f64 = 600.0;
const FAN_START_TIME: f64 = 1_800.0;
const FLOW_HYSTERESIS: f64 = 0.01;
const COOLING_MAXIMUM_AIRFLOW: f64 = 0.5;
const IMPORTANCE_MULTIPLIER: f64 = 1.0;

#[derive(Clone, Copy)]
struct Row {
    discharge_airflow: f64,
    active_airflow_setpoint: f64,
    supply_fan_status: bool,
    operation_mode: i64,
    damper_position: f64,
}

const fn row(
    discharge_airflow: f64,
    active_airflow_setpoint: f64,
    supply_fan_status: bool,
    operation_mode: i64,
    damper_position: f64,
) -> Row {
    Row {
        discharge_airflow,
        active_airflow_setpoint,
        supply_fan_status,
        operation_mode,
        damper_position,
    }
}

const ROWS: [Row; 58] = [
    // Quiescent initialization: fan off, unoccupied, zero flows/setpoint, damper open.
    row(0.0, 0.0, false, 0, 1.0), // 0
    // A1 + 300-second drop probe: leak/arming start while unoccupied; setpoint pulse starts.
    row(0.1, 0.02, true, 0, 0.0), // 60
    row(0.1, 0.02, true, 0, 0.0), // 120
    // The setpoint pulse drops before truDel4 can expire.
    row(0.1, 0.0, true, 0, 0.0), // 180
    row(0.1, 0.0, true, 0, 0.0), // 240
    row(0.1, 0.0, true, 0, 0.0), // 300
    row(0.1, 0.0, true, 0, 0.0), // 360
    row(0.1, 0.0, true, 0, 0.0), // 420
    row(0.1, 0.0, true, 0, 0.0), // 480
    row(0.1, 0.0, true, 0, 0.0), // 540
    row(0.1, 0.0, true, 0, 0.0), // 600
    // A1: leak alarm fires unoccupied and 1,200 seconds before fan arming.
    row(0.1, 0.0, true, 0, 0.0), // 660
    // LessThreshold damper-closed band hold, then release above 0.015.
    row(0.1, 0.0, true, 0, 0.012), // 720
    row(0.1, 0.0, true, 0, 0.016), // 780
    // Restart the leak timer, hold flowHigh in (0.04, 0.05), then drop it before 600 seconds.
    row(0.1, 0.0, true, 0, 0.0),   // 840
    row(0.045, 0.0, true, 0, 0.0), // 900
    row(0.039, 0.0, true, 0, 0.0), // 960
    row(0.1, 0.0, true, 0, 1.0),   // 1020
    row(0.1, 0.0, true, 0, 1.0),   // 1080
    row(0.1, 0.0, true, 0, 1.0),   // 1140
    row(0.1, 0.0, true, 0, 1.0),   // 1200
    row(0.1, 0.0, true, 0, 1.0),   // 1260
    row(0.1, 0.0, true, 0, 1.0),   // 1320
    row(0.1, 0.0, true, 0, 1.0),   // 1380
    row(0.1, 0.0, true, 0, 1.0),   // 1440
    row(0.1, 0.0, true, 0, 1.0),   // 1500
    row(0.1, 0.0, true, 0, 1.0),   // 1560
    row(0.1, 0.0, true, 0, 1.0),   // 1620
    row(0.1, 0.0, true, 0, 1.0),   // 1680
    row(0.1, 0.0, true, 0, 1.0),   // 1740
    // Flow is already at the future 60%-of-setpoint level before the fresh setpoint activation.
    row(0.6, 0.0, true, 0, 1.0), // 1800
    // A5: fan is armed; fresh setpoint activation starts both setOn and low70 delays.
    row(0.6, 1.0, true, OCCUPIED_MODE, 1.0), // 1860
    row(0.6, 1.0, true, OCCUPIED_MODE, 1.0), // 1920
    row(0.6, 1.0, true, OCCUPIED_MODE, 1.0), // 1980
    row(0.6, 1.0, true, OCCUPIED_MODE, 1.0), // 2040
    row(0.6, 1.0, true, OCCUPIED_MODE, 1.0), // 2100
    // A5/A6: both 300-second gates expire and level 3 fires at 60% flow.
    row(0.6, 1.0, true, OCCUPIED_MODE, 1.0), // 2160
    // A6/A7: cross 50%; exact-equality probes below and above occupied suppress the live level 3.
    row(0.4, 1.0, true, 0, 1.0),               // 2220
    row(0.505, 1.0, true, 2, 1.0),             // 2280
    row(0.505, 1.0, true, OCCUPIED_MODE, 1.0), // 2340
    row(0.505, 1.0, true, OCCUPIED_MODE, 1.0), // 2400
    row(0.505, 1.0, true, OCCUPIED_MODE, 1.0), // 2460
    // A3/A6: low50 delay expires inside its hold band; level 2 wins over live level 3.
    row(0.505, 1.0, true, OCCUPIED_MODE, 1.0), // 2520
    // A4: release low50 above 0.51, then hold/release opposite-form low70 at 0.705/0.715.
    row(0.515, 1.0, true, OCCUPIED_MODE, 1.0), // 2580
    row(0.705, 1.0, true, OCCUPIED_MODE, 1.0), // 2640
    // Fan off starts the sensor timer while the low70 comparator crosses its release boundary.
    row(0.715, 1.0, false, OCCUPIED_MODE, 1.0), // 2700
    // GreaterThreshold setpoint band hold at 0.007, then release below 0.005.
    row(0.1, 0.007, false, OCCUPIED_MODE, 1.0), // 2760
    row(0.1, 0.004, false, OCCUPIED_MODE, 1.0), // 2820
    row(0.1, 0.0, false, OCCUPIED_MODE, 1.0),   // 2880
    row(0.1, 0.0, false, OCCUPIED_MODE, 1.0),   // 2940
    row(0.1, 0.0, false, OCCUPIED_MODE, 1.0),   // 3000
    row(0.1, 0.0, false, OCCUPIED_MODE, 1.0),   // 3060
    row(0.1, 0.0, false, OCCUPIED_MODE, 1.0),   // 3120
    row(0.1, 0.0, false, OCCUPIED_MODE, 1.0),   // 3180
    row(0.1, 0.0, false, OCCUPIED_MODE, 1.0),   // 3240
    // Sensor alarm fires after 600 seconds fan-off, then resets immediately when the fan starts.
    row(0.1, 0.0, false, OCCUPIED_MODE, 1.0), // 3300
    row(0.1, 0.0, true, OCCUPIED_MODE, 1.0),  // 3360
    // Fresh fan pulse drops before the distinct 1,800-second fanIni delay can expire.
    row(0.1, 0.0, false, OCCUPIED_MODE, 1.0), // 3420
];

/// Build independent Tier-A integer goldens for all three CoolingOnly alarm ladders.
///
/// Time is in seconds, airflow in cubic metres per second, and damper position is dimensionless.
/// The 60-second schedule covers A1-A7, every comparator's hold/release band, alarm fire/reset,
/// and pre-expiry drops for the 300-, 600-, and 1,800-second TrueDelay durations.
pub(super) fn goldens() -> Vec<Golden> {
    let time: Vec<f64> = (0..ROWS.len()).map(|tick| tick as f64 * 60.0).collect();
    let (low_airflow, airflow_sensor, leaking_damper) = alarm_trace(&time);
    let inputs = alarm_inputs();

    vec![
        sequence_golden(
            COOLING_ONLY_ALARMS,
            "low_airflow_alarm",
            ValueKind::Integer,
            time.clone(),
            low_airflow.into_iter().map(i).collect(),
            "Alarms: armed and occupied low-flow recurrence covers setpoint delay, 70%-to-50% escalation, exact mode equality, and opposite comparator bands",
            "Alarms.mo lines 247-367: fanIni, truDel4, truDel/truDel1, occupancy, switch severity 2/3, and importance multiplier compose the low-airflow ladder",
            inputs.clone(),
        ),
        sequence_golden(
            COOLING_ONLY_ALARMS,
            "airflow_sensor_alarm",
            ValueKind::Integer,
            time.clone(),
            airflow_sensor.into_iter().map(i).collect(),
            "Alarms: fan-off airflow sensor delay fires only while exactly occupied and resets on fan start",
            "Alarms.mo lines 288-299 and 314-349: flowHigh and not(u1Fan) feed truDel2, then occupancy gates level 3",
            inputs.clone(),
        ),
        sequence_golden(
            COOLING_ONLY_ALARMS,
            "leaking_damper_alarm",
            ValueKind::Integer,
            time,
            leaking_damper.into_iter().map(i).collect(),
            "Alarms: fan-on closed-damper high-flow delay fires unoccupied before fan arming and covers both comparator release bands",
            "Alarms.mo lines 302-307 and 368-377: raw fan, cloDam, and flowHigh feed truDel3 without occupancy or fanIni gates, yielding level 4",
            inputs,
        ),
    ]
}

fn alarm_trace(time: &[f64]) -> (Vec<i64>, Vec<i64>, Vec<i64>) {
    let discharge: Vec<f64> = ROWS.iter().map(|row| row.discharge_airflow).collect();
    let setpoint: Vec<f64> = ROWS.iter().map(|row| row.active_airflow_setpoint).collect();
    let fan: Vec<bool> = ROWS.iter().map(|row| row.supply_fan_status).collect();
    let damper: Vec<f64> = ROWS.iter().map(|row| row.damper_position).collect();
    let occupied: Vec<bool> = ROWS
        .iter()
        .map(|row| row.operation_mode == OCCUPIED_MODE)
        .collect();

    let armed = true_delay(time, &fan, FAN_START_TIME);
    let setpoint_nonzero = hysteresis(&setpoint, 0.005, 0.01, false);
    let setpoint_on = true_delay(time, &setpoint_nonzero, LOW_FLOW_TIME);
    let half_setpoint: Vec<f64> = setpoint.iter().map(|value| 0.5 * value).collect();
    let seventy_setpoint: Vec<f64> = setpoint.iter().map(|value| 0.7 * value).collect();
    let low50 = less_hysteretic(&discharge, &half_setpoint, FLOW_HYSTERESIS, false);
    let low70 = greater_hysteretic(&seventy_setpoint, &discharge, FLOW_HYSTERESIS, false);
    let low50_armed: Vec<bool> = low50
        .iter()
        .zip(&armed)
        .map(|(&low, &is_armed)| low && is_armed)
        .collect();
    let low70_armed: Vec<bool> = low70
        .iter()
        .zip(&armed)
        .map(|(&low, &is_armed)| low && is_armed)
        .collect();
    let held50 = true_delay(time, &low50_armed, LOW_FLOW_TIME);
    let held70 = true_delay(time, &low70_armed, LOW_FLOW_TIME);
    let multiplier_on = IMPORTANCE_MULTIPLIER > 0.0;
    let low_airflow: Vec<i64> = held50
        .iter()
        .zip(&held70)
        .zip(&setpoint_on)
        .zip(&occupied)
        .map(|(((&level2, &level3), &set_on), &is_occupied)| {
            let level2 = level2 && set_on && multiplier_on && is_occupied;
            let level3 = level3 && set_on && multiplier_on && is_occupied;
            let selected = if level2 {
                2
            } else if level3 {
                3
            } else {
                0
            };
            selected * i64::from(multiplier_on)
        })
        .collect();

    let high_threshold = vec![0.1 * COOLING_MAXIMUM_AIRFLOW; ROWS.len()];
    let flow_high = greater_hysteretic(&discharge, &high_threshold, FLOW_HYSTERESIS, false);
    let sensor_candidate: Vec<bool> = flow_high
        .iter()
        .zip(&fan)
        .map(|(&high, &fan_on)| high && !fan_on)
        .collect();
    let sensor_held = true_delay(time, &sensor_candidate, FAN_OFF_TIME);
    let airflow_sensor: Vec<i64> = sensor_held
        .iter()
        .zip(&occupied)
        .map(|(&held, &is_occupied)| if held && is_occupied { 3 } else { 0 })
        .collect();

    let damper_threshold = vec![0.01; ROWS.len()];
    let damper_closed = less_hysteretic(&damper, &damper_threshold, 0.005, false);
    let leak_candidate: Vec<bool> = fan
        .iter()
        .zip(&damper_closed)
        .zip(&flow_high)
        .map(|((&fan_on, &closed), &high)| fan_on && closed && high)
        .collect();
    let leak_held = true_delay(time, &leak_candidate, LEAK_FLOW_TIME);
    let leaking_damper = leak_held
        .into_iter()
        .map(|held| if held { 4 } else { 0 })
        .collect();

    (low_airflow, airflow_sensor, leaking_damper)
}

fn alarm_inputs() -> Vec<InputSeries> {
    vec![
        input_r(
            "discharge_airflow",
            ROWS.iter().map(|row| row.discharge_airflow),
        ),
        input_r(
            "active_airflow_setpoint",
            ROWS.iter().map(|row| row.active_airflow_setpoint),
        ),
        input_b(
            "supply_fan_status",
            ROWS.iter().map(|row| row.supply_fan_status),
        ),
        input_i("operation_mode", ROWS.iter().map(|row| row.operation_mode)),
        input_r(
            "damper_position",
            ROWS.iter().map(|row| row.damper_position),
        ),
    ]
}
