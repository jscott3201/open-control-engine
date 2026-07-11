//! G36 CoolingOnly SystemRequests source-verified sequence oracle.
//!
//! The oracle is a per-tick recurrence derived from pinned `SystemRequests.mo`; it imports no
//! engine code. `greThr4.h=0.5*floHys` at source line 125 is pre-grounded to `0.005` for the
//! canonical Validation binding `floHys=0.01`. The cooling-loop one-request path is not gated by
//! `uAftSup` (T1), while the pressure one-request path taps raw `greThr3.y` before `tim3` (T2).
//! Loop and damper release thresholds follow their `h=0.01` bindings at `0.94`, not the HTML
//! narrative's `0.85` wording (T3). Temperature inputs are unsampled; cooling-loop, airflow, and
//! damper inputs use 120-second zero-order holds, with crossings deliberately placed between
//! sample instants so that split remains observable.

use crate::oracle::{Golden, InputSeries, ValueKind};

use super::{
    COOLING_ONLY_SYSTEM_REQUESTS, greater_hysteretic, hysteresis, i, initial_sample_time, input_b,
    input_r, sample_due, sampler_output, sequence_golden, true_delay,
};

#[derive(Clone, Copy)]
struct Row {
    after_suppression: bool,
    cooling_setpoint: f64,
    zone_temperature: f64,
    cooling_loop: f64,
    airflow_setpoint: f64,
    discharge_airflow: f64,
    damper_position: f64,
}

const COOLING_SETPOINT: f64 = 295.0;

const fn row(
    after_suppression: bool,
    temperature_difference: f64,
    cooling_loop: f64,
    airflow_setpoint: f64,
    discharge_airflow: f64,
    damper_position: f64,
) -> Row {
    Row {
        after_suppression,
        cooling_setpoint: COOLING_SETPOINT,
        zone_temperature: COOLING_SETPOINT + temperature_difference,
        cooling_loop,
        airflow_setpoint,
        discharge_airflow,
        damper_position,
    }
}

const ROWS: [Row; 31] = [
    // Quiescent initialization: every comparator and delay starts false.
    row(false, 0.0, 0.0, 0.0, 0.1, 0.0),
    // S1/S6/S10: cross temperature, loop, damper, and flow thresholds at non-sample t=60.
    row(true, 3.5, 1.0, 1.0, 0.2, 1.0),
    // S1/S6: sampled one-requests are visible while both TrueDelay paths remain unexpired.
    row(true, 3.5, 1.0, 1.0, 0.2, 1.0),
    // S1/S6/S10: unsampled temperature reaches 120 s and pressure reaches 60 s, both yielding 3.
    row(true, 3.5, 1.0, 1.0, 0.2, 1.0),
    // S3/S7: hot3 holds inside (2.75,3), while sampled 60%-of-setpoint flow yields pressure 2.
    row(true, 2.9, 0.0, 1.0, 0.6, 1.0),
    // S3/S10: hot3 releases below 2.75; non-sample pressure changes remain held at request 2.
    row(true, 2.7, 0.0, 0.0, 0.8, 1.0),
    // S8: healthy sampled flow leaves only the raw high-damper one-request path.
    row(true, 0.0, 0.0, 0.0, 0.8, 1.0),
    // S11: hot2 rises at non-sample t=420, beginning a 120-second delay window.
    row(true, 2.5, 0.0, 0.006, 0.001, 1.0),
    // S9/S11: hot2 drops before expiry; positive sub-threshold VSet makes both starvation terms
    // true on their own, but the still-false setOn gate keeps the pressure request at 1.
    row(true, 0.0, 0.0, 0.006, 0.001, 1.0),
    // S2: begin a clean temperature-difference crossing in the (2,3] request band.
    row(true, 2.5, 0.0, 0.006, 0.001, 0.945),
    // S2/T3: stay inside the temperature delay and the sampled damper 0.94..0.95 hold band.
    row(true, 2.5, 0.0, 0.0, 0.1, 0.945),
    // S2/T3: the temperature two-request expires; a non-sample damper drop remains held high.
    row(true, 2.5, 0.0, 0.0, 0.1, 0.93),
    // S4/T3: sample cooling loop high for request 1; damper below 0.94 releases pressure to 0.
    row(true, 0.0, 1.0, 0.0, 0.1, 0.93),
    // S4/S10: cooling-loop drop to 0.945 at non-sample t=780 remains held high.
    row(true, 0.0, 0.945, 0.0, 0.1, 0.0),
    // S4/T3: sampled 0.945 is inside the loop hysteresis band and holds request 1.
    row(true, 0.0, 0.945, 0.0, 0.1, 0.0),
    // S4/S10: raw loop drops below 0.94 between samples, but the held value still requests 1.
    row(true, 0.0, 0.93, 0.0, 0.1, 0.0),
    // S4/T3: sampled loop below 0.94 releases the one-request path.
    row(true, 0.0, 0.93, 0.0, 0.1, 0.0),
    // S5: with suppression false, start hot3 and raise uCoo at a non-sample instant.
    row(false, 3.5, 1.0, 0.0, 0.1, 0.0),
    // S5: sampled uCoo yields 1 while the temperature delay is still inside its window.
    row(false, 3.5, 1.0, 0.0, 0.1, 0.0),
    // S5 load-bearing T1: hot3 has expired, but false uAftSup blocks 3 and must not block loop 1.
    row(false, 3.5, 1.0, 0.0, 0.1, 0.0),
    row(true, 0.0, 0.0, 0.0, 0.1, 0.0),
    // Reset pressure state, then raise all sampled pressure inputs between sample instants.
    row(true, 0.0, 0.0, 1.0, 0.49, 1.0),
    // T2: raw damper request is 1 at the sample instant before the 60-second delay expires.
    row(true, 0.0, 0.0, 1.0, 0.49, 1.0),
    // T6: 50%-starvation request is active after the damper delay; raw 0.505 is not sampled yet.
    row(true, 0.0, 0.0, 1.0, 0.505, 1.0),
    // T6: sampled 0.505 lies inside the Greater h=0.01 hold band, so request 3 holds.
    row(true, 0.0, 0.0, 1.0, 0.505, 1.0),
    row(true, 0.0, 0.0, 1.0, 0.515, 1.0),
    // T5/T6: sampled 0.515 releases 50% starvation but leaves 70% starvation, yielding 2.
    row(true, 0.0, 0.0, 1.0, 0.515, 1.0),
    row(true, 0.0, 0.0, 1.0, 0.705, 1.0),
    // T6: sampled 0.705 lies inside the 70%-comparison hold band, so request 2 holds.
    row(true, 0.0, 0.0, 1.0, 0.705, 1.0),
    row(true, 0.0, 0.0, 1.0, 0.715, 1.0),
    // T5/T6: sampled 0.715 releases both starvation paths, leaving damper request 1.
    row(true, 0.0, 0.0, 1.0, 0.715, 1.0),
];

/// Build independent Tier-A integer goldens for both SystemRequests output ladders.
///
/// Temperatures are in kelvin, time in seconds, airflow in cubic metres per second, and the loop
/// and damper positions are dimensionless fractions. The schedule covers every request level,
/// hysteresis hold/release, delay expiry/reset, sampler zero-order hold, suppression gating, and
/// the positive-below-threshold pressure gate trap.
pub(super) fn goldens() -> Vec<Golden> {
    let time: Vec<f64> = (0..ROWS.len()).map(|tick| tick as f64 * 60.0).collect();
    let (temperature_requests, pressure_requests) = system_requests_trace(&time);
    let inputs = system_requests_inputs();

    vec![
        sequence_golden(
            COOLING_ONLY_SYSTEM_REQUESTS,
            "zone_temperature_reset_request",
            ValueKind::Integer,
            time.clone(),
            temperature_requests.into_iter().map(i).collect(),
            "SystemRequests: unsampled temperature delays, sampled loop hysteresis, suppression trap, and drop-before-expiry over a 60-second grid",
            "SystemRequests.mo lines 224-272: parallel 3/2 temperature TrueDelay branches feed an Integer switch ladder; sampled greThr loop output supplies ungated request 1",
            inputs.clone(),
        ),
        sequence_golden(
            COOLING_ONLY_SYSTEM_REQUESTS,
            "zone_pressure_reset_request",
            ValueKind::Integer,
            time,
            pressure_requests.into_iter().map(i).collect(),
            "SystemRequests: sampled damper/flow/setpoint recurrence covers raw request 1, delayed 3/2 paths, setpoint gate, and comparator hold bands",
            "SystemRequests.mo lines 273-324: raw greThr3 supplies request 1; delayed damper and greThr4 gate the 50%/70% Greater hysteresis switch ladder",
            inputs,
        ),
    ]
}

fn system_requests_trace(time: &[f64]) -> (Vec<i64>, Vec<i64>) {
    let temperature_difference: Vec<f64> = ROWS
        .iter()
        .map(|row| row.zone_temperature - row.cooling_setpoint)
        .collect();
    let hot3 = hysteresis(&temperature_difference, 2.75, 3.0, false);
    let hot2 = hysteresis(&temperature_difference, 1.75, 2.0, false);
    let delayed3 = true_delay(time, &hot3, 120.0);
    let delayed2 = true_delay(time, &hot2, 120.0);
    let request3: Vec<bool> = ROWS
        .iter()
        .zip(delayed3)
        .map(|(row, delayed)| row.after_suppression && delayed)
        .collect();
    let request2: Vec<bool> = ROWS
        .iter()
        .zip(delayed2)
        .map(|(row, delayed)| row.after_suppression && delayed)
        .collect();

    let sampled_cooling_loop = sampled(
        time,
        &ROWS.iter().map(|row| row.cooling_loop).collect::<Vec<_>>(),
        120.0,
    );
    let cooling_high = hysteresis(&sampled_cooling_loop, 0.94, 0.95, false);
    let temperature_requests = switch_ladder(&request3, &request2, &cooling_high);

    let sampled_setpoint = sampled(
        time,
        &ROWS
            .iter()
            .map(|row| row.airflow_setpoint)
            .collect::<Vec<_>>(),
        120.0,
    );
    let sampled_discharge = sampled(
        time,
        &ROWS
            .iter()
            .map(|row| row.discharge_airflow)
            .collect::<Vec<_>>(),
        120.0,
    );
    let sampled_damper = sampled(
        time,
        &ROWS
            .iter()
            .map(|row| row.damper_position)
            .collect::<Vec<_>>(),
        120.0,
    );
    let setpoint_on = hysteresis(&sampled_setpoint, 0.005, 0.01, false);
    let damper_high = hysteresis(&sampled_damper, 0.94, 0.95, false);
    let damper_held = true_delay(time, &damper_high, 60.0);
    let gate: Vec<bool> = setpoint_on
        .iter()
        .zip(damper_held)
        .map(|(&setpoint, damper)| setpoint && damper)
        .collect();
    let half_setpoint: Vec<f64> = sampled_setpoint.iter().map(|value| 0.5 * value).collect();
    let seventy_setpoint: Vec<f64> = sampled_setpoint.iter().map(|value| 0.7 * value).collect();
    let starved50 = greater_hysteretic(&half_setpoint, &sampled_discharge, 0.01, false);
    let starved70 = greater_hysteretic(&seventy_setpoint, &sampled_discharge, 0.01, false);
    let pressure3: Vec<bool> = gate
        .iter()
        .zip(starved50)
        .map(|(&enabled, starved)| enabled && starved)
        .collect();
    let pressure2: Vec<bool> = gate
        .iter()
        .zip(starved70)
        .map(|(&enabled, starved)| enabled && starved)
        .collect();
    let pressure_requests = switch_ladder(&pressure3, &pressure2, &damper_high);

    (temperature_requests, pressure_requests)
}

fn sampled(time: &[f64], inputs: &[f64], period: f64) -> Vec<f64> {
    let mut initialized = false;
    let mut t0 = 0.0;
    let mut last_index = -1;
    let mut held = 0.0;
    let mut out = Vec::with_capacity(time.len());

    for (&t, &input) in time.iter().zip(inputs) {
        out.push(sampler_output(
            t,
            input,
            period,
            initialized,
            t0,
            last_index,
            held,
        ));
        if !initialized {
            t0 = initial_sample_time(t, period);
            last_index = sample_due(t, t0, period, -1).1;
            held = input;
            initialized = true;
        } else {
            let (due, index) = sample_due(t, t0, period, last_index);
            if due {
                last_index = index;
                held = input;
            }
        }
    }
    out
}

fn switch_ladder(three: &[bool], two: &[bool], one: &[bool]) -> Vec<i64> {
    three
        .iter()
        .zip(two)
        .zip(one)
        .map(|((&three, &two), &one)| {
            if three {
                3
            } else if two {
                2
            } else {
                i64::from(one)
            }
        })
        .collect()
}

fn system_requests_inputs() -> Vec<InputSeries> {
    vec![
        input_b(
            "after_suppression",
            ROWS.iter().map(|row| row.after_suppression),
        ),
        input_r(
            "cooling_setpoint",
            ROWS.iter().map(|row| row.cooling_setpoint),
        ),
        input_r(
            "zone_temperature",
            ROWS.iter().map(|row| row.zone_temperature),
        ),
        input_r("cooling_loop", ROWS.iter().map(|row| row.cooling_loop)),
        input_r(
            "airflow_setpoint",
            ROWS.iter().map(|row| row.airflow_setpoint),
        ),
        input_r(
            "discharge_airflow",
            ROWS.iter().map(|row| row.discharge_airflow),
        ),
        input_r(
            "damper_position",
            ROWS.iter().map(|row| row.damper_position),
        ),
    ]
}
