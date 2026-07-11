//! G36 CoolingOnly Dampers source-verified sequence oracle.
//!
//! This independent oracle follows pinned `Dampers.mo` without importing engine code. The airflow
//! path applies the clamped `Line`, supply/zone `Greater` hysteresis, cooling-state gate, and exact
//! override ladder before normalizing the post-override setpoint. The damper path carries a local
//! forward-Euler PI-with-reset recurrence with limiter back-calculation (`Ni=0.9`). A rising
//! `u1Fan` resets `xI := y_reset - yP` during the state update, so the target is observable on the
//! next emit when the error is held. A false fan signal neither holds nor zeroes the controller:
//! `Dampers.mo` wires `u1Fan` only to `conPID.trigger` and has no fan-gating output switch.

use crate::oracle::{Golden, InputSeries, ValueKind};

use super::{
    COOLING_ONLY_DAMPERS, buildings_line, clamp, greater_hysteretic, input_b, input_i, input_r, r,
    sequence_golden,
};

const V_MIN_FLOW: f64 = 0.01;
const V_COO_MAX_FLOW: f64 = 0.09;
const D_T_HYS: f64 = 0.25;
const K_DAM: f64 = 1.0;
const TI_DAM: f64 = 300.0;
const NI: f64 = 0.9;
const INI_DAM: f64 = 0.01;
const Y_MIN: f64 = 0.0;
const Y_MAX: f64 = 1.0;
const SAMPLE_STEP: f64 = 60.0;

#[derive(Clone, Copy)]
struct Row {
    active_minimum_airflow: f64,
    supply_air_temperature: f64,
    zone_temperature: f64,
    cooling_loop: f64,
    active_cooling_maximum_airflow: f64,
    zone_state: i64,
    airflow_override_index: i64,
    supply_fan_status: bool,
    discharge_airflow: f64,
    damper_override_index: i64,
}

const fn row(
    supply_air_temperature: f64,
    cooling_loop: f64,
    zone_state: i64,
    airflow_override_index: i64,
    supply_fan_status: bool,
    discharge_airflow: f64,
    damper_override_index: i64,
) -> Row {
    Row {
        active_minimum_airflow: 0.012,
        supply_air_temperature,
        zone_temperature: 295.0,
        cooling_loop,
        active_cooling_maximum_airflow: 0.075,
        zone_state,
        airflow_override_index,
        supply_fan_status,
        discharge_airflow,
        damper_override_index,
    }
}

const ROWS: [Row; 34] = [
    // Quiescent initialization and the exact uCoo=0 Line knot; fan false does not reset at t=0.
    row(290.0, 0.0, 2, 0, false, 0.012, 0),
    // D3: first false -> true fan transition at t=60 resets the PI integrator after this emit.
    row(290.0, 0.0, 2, 0, true, 0.012, 0),
    // D3: held error exposes y_reset=0.01 on the next emit; a held-high fan does not reset again.
    row(290.0, 0.0, 2, 0, true, 0.012, 0),
    // D5 and exact uCoo=1 knot: deadband state forces active minimum despite full cooling loop.
    row(290.0, 1.0, 2, 0, true, 0.012, 0),
    // D5: heating state likewise forces active minimum at the exact upper Line knot.
    row(290.0, 1.0, 1, 0, true, 0.012, 0),
    // Cooling enables the exact lower knot; active minimum differs from VMin_flow override value.
    row(290.0, 0.0, 3, 0, true, 0.012, 0),
    // Interior Line mapping establishes the cold-supply base value before the D1 crossing.
    row(290.0, 0.5, 3, 0, true, 0.030, 0),
    // D1: supply warmer than zone turns Greater on and forces the active minimum airflow.
    row(296.0, 0.5, 3, 0, true, 0.030, 0),
    // D1: 294.9 K lies inside (TZon-0.25,TZon] and holds the warm-supply minimum branch.
    row(294.9, 0.5, 3, 0, true, 0.030, 0),
    // D1: 294.7 K crosses the release boundary and restores the mapped cooling airflow.
    row(294.7, 0.5, 3, 0, true, 0.030, 0),
    // D6: consecutive override 1 forces exactly zero while the damper PI remains live.
    row(290.0, 0.5, 3, 1, true, 0.020, 0),
    // D6: consecutive override 2 forces design cooling maximum 0.09 m3/s.
    row(290.0, 0.5, 3, 2, true, 0.020, 0),
    // D6: consecutive override 3 forces design minimum 0.01 m3/s.
    row(290.0, 0.5, 3, 3, true, 0.020, 0),
    // D4: out-of-range airflow override 4 passes through the 75% Line base value.
    row(290.0, 0.75, 3, 4, true, 0.020, 0),
    // D4: negative airflow override also passes through, killing sign/range comparator mutants.
    row(290.0, 0.25, 3, -1, true, 0.020, 0),
    // D4: out-of-range damper override 3 passes the PI output rather than forcing a limit.
    row(290.0, 0.6, 3, 0, true, 0.020, 3),
    // D2: a flow override with no damper override visibly retargets the normalized PI setpoint.
    row(290.0, 0.6, 3, 2, true, 0.010, 0),
    // D2: the opposite flow override visibly retracks the still-unbypassed PI.
    row(290.0, 0.6, 3, 1, true, 0.060, 0),
    // D2: force damper open while the hidden PI integrates a large positive error underneath.
    row(290.0, 1.0, 3, 0, true, 0.000, 2),
    // D2: force damper closed; the same hidden PI recurrence continues without being held.
    row(290.0, 1.0, 3, 0, true, 0.000, 1),
    // D2: release the damper override and expose the integrated, anti-windup-bounded PI state.
    row(290.0, 1.0, 3, 0, true, 0.000, 0),
    // D3: fan false does not gate the damper; the PI emits and integrates with a positive error.
    row(290.0, 0.0, 3, 0, false, 0.000, 0),
    // D3: fan remains false while a changed error moves the live controller instead of zeroing it.
    row(290.0, 0.0, 3, 0, false, 0.090, 0),
    // D3: second false -> true edge resets the integrator after this emit.
    row(290.0, 0.0, 3, 0, true, 0.090, 0),
    // D3: held error exposes the second y_reset=0.01 discontinuity on the next emit.
    row(290.0, 0.0, 3, 0, true, 0.090, 0),
    // PID saturation/anti-windup: sustained maximum setpoint and zero measurement begin clipping.
    row(290.0, 0.4, 3, 2, true, 0.000, 0),
    row(290.0, 0.4, 3, 2, true, 0.000, 0),
    row(290.0, 0.4, 3, 2, true, 0.000, 0),
    row(290.0, 0.4, 3, 2, true, 0.000, 0),
    // Anti-windup recovery: reverse the error after the sustained high-limit stretch.
    row(290.0, 0.0, 3, 0, true, 0.090, 0),
    row(290.0, 0.0, 3, 0, true, 0.090, 0),
    // D4: a negative damper index passes the live, recovering PI output.
    row(290.0, 0.5, 3, 0, true, 0.040, -1),
    // D1: cross warm a second time at the exact uCoo=1 knot, again selecting active minimum.
    row(296.0, 1.0, 3, 0, true, 0.020, 0),
    // D1: cross below the release boundary and restore the exact upper Line-knot airflow.
    row(294.7, 1.0, 3, 0, true, 0.020, 0),
];

/// Build independent Tier-A goldens for airflow setpoint and commanded damper position.
///
/// Time is seconds on a 60-second grid, temperatures are kelvin, airflow is cubic metres per
/// second, and damper position is dimensionless in `[0,1]`. The schedule covers the D1-D6 source
/// traps, exact Line knots, two rising-edge resets, live fan-false integration, saturation,
/// anti-windup recovery, and override pass-through. The PI state starts at `xI=0`; state updates
/// follow each emitted sample, matching the established sequence-oracle timing convention.
pub(super) fn goldens() -> Vec<Golden> {
    let time: Vec<f64> = (0..ROWS.len())
        .map(|tick| tick as f64 * SAMPLE_STEP)
        .collect();
    let airflow_setpoint = airflow_setpoints();
    let pid = pi_with_reset_outputs(&time, &airflow_setpoint);
    assert_eq!(pid[2].to_bits(), INI_DAM.to_bits());
    assert!((pid[24] - INI_DAM).abs() <= f64::EPSILON);
    assert!(pid.iter().any(|value| value.to_bits() == Y_MAX.to_bits()));

    let damper_command: Vec<f64> = ROWS
        .iter()
        .zip(pid)
        .map(|(row, pid_output)| match row.damper_override_index {
            1 => 0.0,
            2 => 1.0,
            _ => pid_output,
        })
        .collect();
    let inputs = inputs();

    vec![
        sequence_golden(
            COOLING_ONLY_DAMPERS,
            "airflow_setpoint",
            ValueKind::Real,
            time.clone(),
            airflow_setpoint.into_iter().map(r).collect(),
            "Dampers airflow: clamped Line knots/interior, warm-supply Greater hold/release, cooling-state gate, and exact 1/2/3 plus out-of-range overrides",
            "Dampers.mo lines 228-301 and 334-343: post-override swi1.y is both VSet_flow and the normalized PID setpoint numerator",
            inputs.clone(),
        ),
        sequence_golden(
            COOLING_ONLY_DAMPERS,
            "damper_command",
            ValueKind::Real,
            time,
            damper_command.into_iter().map(r).collect(),
            "Dampers damper: local PI-with-reset recurrence, two fan rising edges, live fan-false state, flow/damper override interactions, saturation, and recovery",
            "Dampers.mo lines 258-267 and 303-332 plus PIDWithReset.mo: normalize by 0.09, emit limited PI, then update xI with reset or Ni=0.9 back-calculation; swi2 applies only damper overrides",
            inputs,
        ),
    ]
}

fn airflow_setpoints() -> Vec<f64> {
    let supply: Vec<f64> = ROWS
        .iter()
        .map(|row| row.supply_air_temperature)
        .collect();
    let zone: Vec<f64> = ROWS.iter().map(|row| row.zone_temperature).collect();
    let supply_warm = greater_hysteretic(&supply, &zone, D_T_HYS, false);

    ROWS
        .iter()
        .zip(supply_warm)
        .map(|(row, warm)| {
            let mapped = buildings_line(
                0.0,
                row.active_minimum_airflow,
                1.0,
                row.active_cooling_maximum_airflow,
                row.cooling_loop,
            );
            let active = if warm {
                row.active_minimum_airflow
            } else {
                mapped
            };
            let base = if row.zone_state == 3 {
                active
            } else {
                row.active_minimum_airflow
            };
            match row.airflow_override_index {
                1 => 0.0,
                2 => V_COO_MAX_FLOW,
                3 => V_MIN_FLOW,
                _ => base,
            }
        })
        .collect()
}

fn pi_with_reset_outputs(time: &[f64], airflow_setpoint: &[f64]) -> Vec<f64> {
    let mut integrator = 0.0;
    let mut previous_time: Option<f64> = None;
    let mut previous_trigger = false;
    let mut outputs = Vec::with_capacity(ROWS.len());

    for ((&t, &setpoint), row) in time.iter().zip(airflow_setpoint).zip(ROWS.iter()) {
        let normalized_setpoint = setpoint / V_COO_MAX_FLOW;
        let normalized_measurement = row.discharge_airflow / V_COO_MAX_FLOW;
        let error = normalized_setpoint - normalized_measurement;
        let proportional = K_DAM * error;
        let unlimited = proportional + integrator;
        let limited = clamp(unlimited, Y_MIN, Y_MAX);
        outputs.push(limited);

        let dt = previous_time.map_or(0.0, |previous| (t - previous).max(0.0));
        let rising_reset = row.supply_fan_status && !previous_trigger;
        integrator = if rising_reset {
            INI_DAM - proportional
        } else {
            let anti_windup_gain = (unlimited - limited) / (K_DAM * NI);
            let corrected_error = error - anti_windup_gain;
            integrator + (K_DAM / TI_DAM) * corrected_error * dt
        };
        previous_time = Some(t);
        previous_trigger = row.supply_fan_status;
    }

    outputs
}

fn inputs() -> Vec<InputSeries> {
    vec![
        input_r(
            "active_minimum_airflow",
            ROWS.iter().map(|row| row.active_minimum_airflow),
        ),
        input_r(
            "supply_air_temperature",
            ROWS.iter().map(|row| row.supply_air_temperature),
        ),
        input_r(
            "zone_temperature",
            ROWS.iter().map(|row| row.zone_temperature),
        ),
        input_r("cooling_loop", ROWS.iter().map(|row| row.cooling_loop)),
        input_r(
            "active_cooling_maximum_airflow",
            ROWS
                .iter()
                .map(|row| row.active_cooling_maximum_airflow),
        ),
        input_i("zone_state", ROWS.iter().map(|row| row.zone_state)),
        input_i(
            "airflow_override_index",
            ROWS.iter().map(|row| row.airflow_override_index),
        ),
        input_b(
            "supply_fan_status",
            ROWS.iter().map(|row| row.supply_fan_status),
        ),
        input_r(
            "discharge_airflow",
            ROWS.iter().map(|row| row.discharge_airflow),
        ),
        input_i(
            "damper_override_index",
            ROWS.iter().map(|row| row.damper_override_index),
        ),
    ]
}
