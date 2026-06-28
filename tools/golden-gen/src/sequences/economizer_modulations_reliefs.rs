//! G36 MultiZone VAV Economizers.Subsequences.Modulations.Reliefs sequence oracle.

use crate::oracle::{Golden, InputSeries, ValueKind};

use super::{
    ECONOMIZER_MODULATIONS_RELIEFS, buildings_line, input_r, r, sequence_golden, unit_ticks,
};

pub(super) fn goldens() -> Vec<Golden> {
    let time = unit_ticks(7);
    let supply_temperature_signal = [-0.5, -0.25, -0.125, 0.0, 0.125, 0.25, 0.5];
    let outdoor_damper_min = [0.25; 7];
    let outdoor_damper_max = [0.875; 7];
    let return_damper_min = [0.125; 7];
    let return_damper_max = [0.75; 7];

    let (outdoor_damper_command, return_damper_command) = economizer_reliefs_trace(
        &supply_temperature_signal,
        &outdoor_damper_min,
        &outdoor_damper_max,
        &return_damper_min,
        &return_damper_max,
    );
    let inputs = economizer_reliefs_inputs(
        &supply_temperature_signal,
        &outdoor_damper_min,
        &outdoor_damper_max,
        &return_damper_min,
        &return_damper_max,
    );

    vec![
        sequence_golden(
            ECONOMIZER_MODULATIONS_RELIEFS,
            "outdoor_damper_command",
            ValueKind::Real,
            time.clone(),
            outdoor_damper_command.into_iter().map(r).collect(),
            "Economizer Modulations.Reliefs: uTSup sweeps below, through, and above the relief control window with fixed dyadic damper limits",
            "Pinned Reliefs.mo default parameters: outDamPos=Line(x1=uMin=-0.25,f1=uOutDam_min,x2=uOutDamMax=0,f2=uOutDam_max,limitBelow=true,limitAbove=true); yOutDam=min(outDamPos.y,uOutDam_max)",
            inputs.clone(),
        ),
        sequence_golden(
            ECONOMIZER_MODULATIONS_RELIEFS,
            "return_damper_command",
            ValueKind::Real,
            time,
            return_damper_command.into_iter().map(r).collect(),
            "Economizer Modulations.Reliefs: same sweep exercises return damper high clamp, midpoint interpolation, and low clamp",
            "Pinned Reliefs.mo default parameters: retDamPos=Line(x1=uRetDamMin=0,f1=uRetDam_max,x2=uMax=0.25,f2=uRetDam_min,limitBelow=true,limitAbove=true); yRetDam=max(retDamPos.y,uRetDam_min)",
            inputs,
        ),
    ]
}

fn economizer_reliefs_trace(
    supply_temperature_signal: &[f64],
    outdoor_damper_min: &[f64],
    outdoor_damper_max: &[f64],
    return_damper_min: &[f64],
    return_damper_max: &[f64],
) -> (Vec<f64>, Vec<f64>) {
    const U_MIN: f64 = -0.25;
    const U_MAX: f64 = 0.25;
    const U_OUT_DAM_MAX: f64 = 0.0;
    const U_RET_DAM_MIN: f64 = 0.0;

    let mut outdoor_damper_command = Vec::with_capacity(supply_temperature_signal.len());
    let mut return_damper_command = Vec::with_capacity(supply_temperature_signal.len());

    for ((((&u_t_sup, &u_out_dam_min), &u_out_dam_max), &u_ret_dam_min), &u_ret_dam_max) in
        supply_temperature_signal
            .iter()
            .zip(outdoor_damper_min)
            .zip(outdoor_damper_max)
            .zip(return_damper_min)
            .zip(return_damper_max)
    {
        let out_dam_pos = buildings_line(
            U_MIN,
            u_out_dam_min,
            U_OUT_DAM_MAX,
            u_out_dam_max,
            u_t_sup,
        );
        let ret_dam_pos = buildings_line(
            U_RET_DAM_MIN,
            u_ret_dam_max,
            U_MAX,
            u_ret_dam_min,
            u_t_sup,
        );
        outdoor_damper_command.push(out_dam_pos.min(u_out_dam_max));
        return_damper_command.push(ret_dam_pos.max(u_ret_dam_min));
    }

    (outdoor_damper_command, return_damper_command)
}

fn economizer_reliefs_inputs(
    supply_temperature_signal: &[f64],
    outdoor_damper_min: &[f64],
    outdoor_damper_max: &[f64],
    return_damper_min: &[f64],
    return_damper_max: &[f64],
) -> Vec<InputSeries> {
    vec![
        input_r(
            "supply_temperature_signal",
            supply_temperature_signal.iter().copied(),
        ),
        input_r("outdoor_damper_min", outdoor_damper_min.iter().copied()),
        input_r("outdoor_damper_max", outdoor_damper_max.iter().copied()),
        input_r("return_damper_min", return_damper_min.iter().copied()),
        input_r("return_damper_max", return_damper_max.iter().copied()),
    ]
}
