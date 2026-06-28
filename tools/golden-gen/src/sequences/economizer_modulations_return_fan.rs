//! G36 MultiZone VAV Economizers.Subsequences.Modulations.ReturnFan sequence oracle.

use crate::oracle::{Golden, InputSeries, ValueKind};

use super::{
    ECONOMIZER_MODULATIONS_RETURN_FAN, ECONOMIZER_MODULATIONS_RETURN_FAN_RELIEF_DAMPER,
    buildings_line, input_r, r, sequence_golden, unit_ticks,
};

pub(super) fn goldens() -> Vec<Golden> {
    let mut out = source_default_goldens();
    out.extend(relief_damper_goldens());
    out
}

fn source_default_goldens() -> Vec<Golden> {
    let time = unit_ticks(7);
    let supply_temperature_signal = [-0.5, -0.25, -0.125, 0.0, 0.125, 0.25, 0.5];
    let return_damper_min = [0.125; 7];
    let return_damper_max = [0.75; 7];

    let (return_damper_command, outdoor_damper_command) = economizer_return_fan_trace(
        &supply_temperature_signal,
        &return_damper_min,
        &return_damper_max,
    );
    let inputs = economizer_return_fan_inputs(
        &supply_temperature_signal,
        &return_damper_min,
        &return_damper_max,
    );

    vec![
        sequence_golden(
            ECONOMIZER_MODULATIONS_RETURN_FAN,
            "return_damper_command",
            ValueKind::Real,
            time.clone(),
            return_damper_command.into_iter().map(r).collect(),
            "Economizer Modulations.ReturnFan: uTSup sweeps below, through, and above the return damper control window with fixed dyadic damper limits",
            "Pinned ReturnFan.mo source-default have_dirCon=true: retDamPos=Line(x1=uMin=-0.25,f1=uRetDam_max,x2=uMax=0.25,f2=uRetDam_min,limitBelow=true,limitAbove=true); yRetDam=retDamPos.y",
            inputs.clone(),
        ),
        sequence_golden(
            ECONOMIZER_MODULATIONS_RETURN_FAN,
            "outdoor_damper_command",
            ValueKind::Real,
            time,
            outdoor_damper_command.into_iter().map(r).collect(),
            "Economizer Modulations.ReturnFan: source-default have_dirCon=true keeps outdoor damper command fully open",
            "Pinned ReturnFan.mo source-default have_dirCon=true connects one.y (k=1) directly to yOutDam; relief damper branch is inactive",
            inputs,
        ),
    ]
}

fn relief_damper_goldens() -> Vec<Golden> {
    let time = unit_ticks(7);
    let supply_temperature_signal = [-0.5, -0.25, -0.125, 0.0, 0.125, 0.25, 0.5];
    let return_damper_min = [0.125; 7];
    let return_damper_max = [0.75; 7];

    let (return_damper_command, relief_damper_command, outdoor_damper_command) =
        economizer_return_fan_relief_damper_trace(
            &supply_temperature_signal,
            &return_damper_min,
            &return_damper_max,
        );
    let inputs = economizer_return_fan_inputs(
        &supply_temperature_signal,
        &return_damper_min,
        &return_damper_max,
    );

    vec![
        sequence_golden(
            ECONOMIZER_MODULATIONS_RETURN_FAN_RELIEF_DAMPER,
            "return_damper_command",
            ValueKind::Real,
            time.clone(),
            return_damper_command.into_iter().map(r).collect(),
            "Economizer Modulations.ReturnFan: have_dirCon=false with uTSup sweep and fixed dyadic return damper limits",
            "Pinned ReturnFan.mo have_dirCon=false keeps retDamPos=Line(x1=uMin=-0.25,f1=uRetDam_max,x2=uMax=0.25,f2=uRetDam_min,limitBelow=true,limitAbove=true); yRetDam=retDamPos.y",
            inputs.clone(),
        ),
        sequence_golden(
            ECONOMIZER_MODULATIONS_RETURN_FAN_RELIEF_DAMPER,
            "relief_damper_command",
            ValueKind::Real,
            time.clone(),
            relief_damper_command.into_iter().map(r).collect(),
            "Economizer Modulations.ReturnFan: have_dirCon=false activates the relief damper line from zero to one across the same control window",
            "Pinned ReturnFan.mo have_dirCon=false: relDamPos=Line(x1=uMin=-0.25,f1=0,x2=uMax=0.25,f2=1,limitBelow=true,limitAbove=true); yRelDam=relDamPos.y",
            inputs.clone(),
        ),
        sequence_golden(
            ECONOMIZER_MODULATIONS_RETURN_FAN_RELIEF_DAMPER,
            "outdoor_damper_command",
            ValueKind::Real,
            time,
            outdoor_damper_command.into_iter().map(r).collect(),
            "Economizer Modulations.ReturnFan: have_dirCon=false still keeps outdoor damper command fully open",
            "Pinned ReturnFan.mo connects one.y (k=1) directly to yOutDam in both direct-control and relief-damper variants",
            inputs,
        ),
    ]
}

fn economizer_return_fan_trace(
    supply_temperature_signal: &[f64],
    return_damper_min: &[f64],
    return_damper_max: &[f64],
) -> (Vec<f64>, Vec<f64>) {
    const U_MIN: f64 = -0.25;
    const U_MAX: f64 = 0.25;

    let mut return_damper_command = Vec::with_capacity(supply_temperature_signal.len());
    let mut outdoor_damper_command = Vec::with_capacity(supply_temperature_signal.len());

    for ((&u_t_sup, &u_ret_dam_min), &u_ret_dam_max) in supply_temperature_signal
        .iter()
        .zip(return_damper_min)
        .zip(return_damper_max)
    {
        return_damper_command.push(buildings_line(
            U_MIN,
            u_ret_dam_max,
            U_MAX,
            u_ret_dam_min,
            u_t_sup,
        ));
        outdoor_damper_command.push(1.0);
    }

    (return_damper_command, outdoor_damper_command)
}

fn economizer_return_fan_relief_damper_trace(
    supply_temperature_signal: &[f64],
    return_damper_min: &[f64],
    return_damper_max: &[f64],
) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    const U_MIN: f64 = -0.25;
    const U_MAX: f64 = 0.25;

    let mut return_damper_command = Vec::with_capacity(supply_temperature_signal.len());
    let mut relief_damper_command = Vec::with_capacity(supply_temperature_signal.len());
    let mut outdoor_damper_command = Vec::with_capacity(supply_temperature_signal.len());

    for ((&u_t_sup, &u_ret_dam_min), &u_ret_dam_max) in supply_temperature_signal
        .iter()
        .zip(return_damper_min)
        .zip(return_damper_max)
    {
        return_damper_command.push(buildings_line(
            U_MIN,
            u_ret_dam_max,
            U_MAX,
            u_ret_dam_min,
            u_t_sup,
        ));
        relief_damper_command.push(buildings_line(U_MIN, 0.0, U_MAX, 1.0, u_t_sup));
        outdoor_damper_command.push(1.0);
    }

    (
        return_damper_command,
        relief_damper_command,
        outdoor_damper_command,
    )
}

fn economizer_return_fan_inputs(
    supply_temperature_signal: &[f64],
    return_damper_min: &[f64],
    return_damper_max: &[f64],
) -> Vec<InputSeries> {
    vec![
        input_r(
            "supply_temperature_signal",
            supply_temperature_signal.iter().copied(),
        ),
        input_r("return_damper_min", return_damper_min.iter().copied()),
        input_r("return_damper_max", return_damper_max.iter().copied()),
    ]
}
