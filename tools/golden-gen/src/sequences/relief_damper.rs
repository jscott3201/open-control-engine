//! G36 MultiZone VAV ReliefDamper sequence oracle.

use crate::oracle::{Golden, InputSeries, ValueKind};

use super::{RELIEF_DAMPER, input_b, input_r, r, sequence_golden};

pub(super) fn goldens() -> Vec<Golden> {
    let time: Vec<f64> = (0..6).map(f64::from).collect();
    let building_pressure = [10.0, 12.0, 13.0, 14.0, 15.0, 20.0];
    let supply_fan_status = [false, true, true, true, true, false];
    let command = relief_damper_command(&building_pressure, &supply_fan_status);
    let inputs = relief_damper_inputs(&building_pressure, &supply_fan_status);

    vec![sequence_golden(
        RELIEF_DAMPER,
        "relief_damper_command",
        ValueKind::Real,
        time,
        command.into_iter().map(r).collect(),
        "ReliefDamper: disabled high/low rows plus enabled setpoint, proportional, and upper clamp rows",
        "Pinned ReliefDamper.mo: conErr=dpBui-dpBuiSet, PID controllerType=P with reverseActing=false gives clamp(k*(dpBui-12),0,1), and Reals.Switch emits zero when u1SupFan=false",
        inputs,
    )]
}

fn relief_damper_command(building_pressure: &[f64], supply_fan_status: &[bool]) -> Vec<f64> {
    const DP_BUI_SET: f64 = 12.0;
    const GAIN: f64 = 0.5;

    building_pressure
        .iter()
        .zip(supply_fan_status)
        .map(|(&pressure, &enabled)| {
            if enabled {
                (GAIN * (pressure - DP_BUI_SET)).clamp(0.0, 1.0)
            } else {
                0.0
            }
        })
        .collect()
}

fn relief_damper_inputs(
    building_pressure: &[f64],
    supply_fan_status: &[bool],
) -> Vec<InputSeries> {
    vec![
        input_r("building_pressure", building_pressure.iter().copied()),
        input_b("supply_fan_status", supply_fan_status.iter().copied()),
    ]
}
