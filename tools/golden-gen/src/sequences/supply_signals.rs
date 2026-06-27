//! G36 MultiZone VAV SupplySignals sequence oracle.

use crate::oracle::{Golden, InputSeries, ValueKind};

use super::{SUPPLY_SIGNALS, clamp, input_b, input_r, r, sequence_golden, unit_ticks};

pub(super) fn goldens() -> Vec<Golden> {
    let time = unit_ticks(10);
    let supply_air_temperature_setpoint = [
        295.0, 295.0, 295.0, 300.0, 295.0, 295.0, 320.0, 320.0, 320.0, 320.0,
    ];
    let supply_air_temperature = [
        300.0, 300.0, 300.0, 295.0, 310.0, 320.0, 295.0, 295.0, 295.0, 295.0,
    ];
    let supply_fan_status = [
        false, true, true, true, true, true, true, false, true, true,
    ];
    let (supply_temperature_signal, heating_coil_command, cooling_coil_command) =
        supply_signals_trace(
            &time,
            &supply_air_temperature,
            &supply_air_temperature_setpoint,
            &supply_fan_status,
        );
    let inputs = supply_signals_inputs(
        &supply_air_temperature,
        &supply_air_temperature_setpoint,
        &supply_fan_status,
    );

    vec![
        sequence_golden(
            SUPPLY_SIGNALS,
            "uTSup",
            ValueKind::Real,
            time.clone(),
            supply_temperature_signal.into_iter().map(r).collect(),
            "SupplySignals: fan off at t=0 and t=7; fan rises at t=1 and t=8; temperature error drives heating, cooling, and saturation rows",
            "Pinned SupplySignals.mo default variant: PI PIDWithReset(k=0.05,Ti=600,yMin=-1,yMax=1,reverseActing=false,y_reset=0) feeds Switch(u1=PID.y,u2=u1SupFan,u3=0)",
            inputs.clone(),
        ),
        sequence_golden(
            SUPPLY_SIGNALS,
            "yHeaCoi",
            ValueKind::Real,
            time.clone(),
            heating_coil_command.into_iter().map(r).collect(),
            "SupplySignals: have_heaCoi=true default branch; yHeaCoi follows uTSup heating side",
            "Pinned SupplySignals.mo conSigHea uses Buildings Reals/Line.mo with x1=-1,f1=1,x2=uHea_max=-0.25,f2=0,limitBelow=false,limitAbove=true",
            inputs.clone(),
        ),
        sequence_golden(
            SUPPLY_SIGNALS,
            "yCooCoi",
            ValueKind::Real,
            time,
            cooling_coil_command.into_iter().map(r).collect(),
            "SupplySignals: have_cooCoi=true default branch; yCooCoi follows uTSup cooling side",
            "Pinned SupplySignals.mo conSigCoo uses Buildings Reals/Line.mo with x1=uCoo_min=0.25,f1=0,x2=1,f2=1,limitBelow=true,limitAbove=false",
            inputs,
        ),
    ]
}

fn supply_signals_trace(
    time: &[f64],
    supply_air_temperature: &[f64],
    supply_air_temperature_setpoint: &[f64],
    supply_fan_status: &[bool],
) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    const K: f64 = 0.05;
    const TI: f64 = 600.0;
    const NI: f64 = 0.9;
    const Y_MIN: f64 = -1.0;
    const Y_MAX: f64 = 1.0;
    const RESET: f64 = 0.0;
    const U_HEA_MAX: f64 = -0.25;
    const U_COO_MIN: f64 = 0.25;

    let mut integral = 0.0;
    let mut previous_time: Option<f64> = None;
    let mut previous_trigger = false;

    let mut supply_temperature_signal = Vec::with_capacity(time.len());
    let mut heating_coil_command = Vec::with_capacity(time.len());
    let mut cooling_coil_command = Vec::with_capacity(time.len());

    for (((&t, &measurement), &setpoint), &fan_on) in time
        .iter()
        .zip(supply_air_temperature)
        .zip(supply_air_temperature_setpoint)
        .zip(supply_fan_status)
    {
        let error = measurement - setpoint;
        let proportional = K * error;
        let unlimited = proportional + integral;
        let pid = clamp(unlimited, Y_MIN, Y_MAX);
        let signal = if fan_on { pid } else { 0.0 };

        supply_temperature_signal.push(signal);
        heating_coil_command.push(buildings_line(
            -1.0, 1.0, U_HEA_MAX, 0.0, signal, false, true,
        ));
        cooling_coil_command.push(buildings_line(
            U_COO_MIN, 0.0, 1.0, 1.0, signal, true, false,
        ));

        let dt = previous_time.map_or(0.0, |previous| (t - previous).max(0.0));
        if fan_on && !previous_trigger {
            integral = RESET - proportional;
        } else {
            let anti_windup_gain = (unlimited - pid) / (K * NI);
            let corrected_error = error - anti_windup_gain;
            integral += (K / TI) * corrected_error * dt;
        }
        previous_time = Some(t);
        previous_trigger = fan_on;
    }

    (
        supply_temperature_signal,
        heating_coil_command,
        cooling_coil_command,
    )
}

fn buildings_line(
    x1: f64,
    f1: f64,
    x2: f64,
    f2: f64,
    u: f64,
    limit_below: bool,
    limit_above: bool,
) -> f64 {
    let x_lim = match (limit_below, limit_above) {
        (true, true) => clamp(u, x1, x2),
        (true, false) => u.max(x1),
        (false, true) => u.min(x2),
        (false, false) => u,
    };
    let slope = (f2 - f1) / (x2 - x1);
    let intercept = f2 - slope * x2;
    intercept + slope * x_lim
}

fn supply_signals_inputs(
    supply_air_temperature: &[f64],
    supply_air_temperature_setpoint: &[f64],
    supply_fan_status: &[bool],
) -> Vec<InputSeries> {
    vec![
        input_r(
            "supply_air_temperature",
            supply_air_temperature.iter().copied(),
        ),
        input_r(
            "supply_air_temperature_setpoint",
            supply_air_temperature_setpoint.iter().copied(),
        ),
        input_b("supply_fan_status", supply_fan_status.iter().copied()),
    ]
}
